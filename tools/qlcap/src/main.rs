//! `qlcap`: decode captures of the be quiet! Light Mount vendor protocol.
//!
//! Reads Wireshark captures (pcap/pcapng) from Linux usbmon or Windows USBPcap,
//! and logs of `tools/webhid-logger.js`. See `docs/capturing.md`.

mod capture;
mod decode;
mod usb;
mod webhid;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::capture::Format;
use crate::decode::Event;

#[derive(Debug, Parser)]
#[command(
    name = "qlcap",
    version,
    about = "Decode captures of the Light Mount vendor protocol"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the vendor messages of a capture, one per line
    Frames {
        /// Capture files: .pcapng/.pcap (usbmon, USBPcap) or WebHID logs; several files
        /// (e.g. one per USB root hub) are merged by time
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Device to decode as BUS.ADDRESS (default: the one with the most vendor packets)
        #[arg(long)]
        device: Option<String>,
        /// Leave out key events (11:02)
        #[arg(long)]
        no_key_events: bool,
    },
    /// Print the vendor messages as test fixture lines (hex); serial number and key events left out
    Fixtures {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[arg(long)]
        device: Option<String>,
    },
}

fn load(path: &Path) -> Result<Vec<Event>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let events = match capture::detect(&bytes) {
        Format::Text => {
            let text = String::from_utf8(bytes)
                .map_err(|_| format!("{}: neither a capture nor a WebHID log", path.display()))?;
            webhid::parse(&text)?
        }
        format => {
            let records = if format == Format::Pcap {
                capture::read_pcap(&bytes)?
            } else {
                capture::read_pcapng(&bytes)?
            };
            records
                .iter()
                .filter_map(usb::transfer)
                .map(Event::Transfer)
                .collect()
        }
    };
    Ok(events)
}

/// Events of several files, merged by time. USBPcap writes one file per root hub;
/// their device names (`hub.address`) do not collide.
fn load_all(paths: &[PathBuf]) -> Result<Vec<Event>, String> {
    let mut events = Vec::new();
    for path in paths {
        events.extend(load(path)?);
    }
    events.sort_by(|a, b| a.time().total_cmp(&b.time()));
    Ok(events)
}

/// The device to decode; reports the choice on stderr.
fn choose_device(events: &[Event], wanted: Option<&str>) -> Result<String, String> {
    let devices = decode::devices(events);
    let list = || {
        devices
            .iter()
            .map(|(d, n)| format!("{d} ({n} packets)"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let device = match wanted {
        Some(wanted) if devices.iter().any(|(d, _)| d == wanted) => wanted.to_owned(),
        Some(wanted) => {
            return Err(format!(
                "no vendor packets for device {wanted} (found: {})",
                list()
            ));
        }
        None => devices
            .first()
            .map(|(d, _)| d.clone())
            .ok_or("no Light Mount vendor packets in this capture")?,
    };
    if devices.len() > 1 {
        eprintln!(
            "qlcap: decoding device {device}; devices with vendor packets: {}",
            list()
        );
    }
    Ok(device)
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Frames {
            files,
            device,
            no_key_events,
        } => {
            let events = load_all(&files)?;
            let device = choose_device(&events, device.as_deref())?;
            let start = decode::start_time(&events);
            for line in decode::lines(&events, &device) {
                if let Some(text) = decode::format(&line, start, !no_key_events) {
                    println!("{text}");
                }
            }
        }
        Command::Fixtures { files, device } => {
            let events = load_all(&files)?;
            let device = choose_device(&events, device.as_deref())?;
            let name = files
                .iter()
                .filter_map(|f| f.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(", ");
            println!("# Extracted from {name} with qlcap (anonymized).");
            for line in decode::lines(&events, &device) {
                if let Some(text) = decode::fixture(&line) {
                    println!("{text}");
                }
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("qlcap: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use quietlinux_core::protocol as p;

    use super::*;
    use crate::capture::tests::pcapng;
    use crate::usb::tests::{usbmon_packet, usbpcap_packet};

    #[test]
    fn cli_definition() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    fn decode_file(bytes: &[u8]) -> Vec<String> {
        let dir =
            std::env::temp_dir().join(format!("qlcap-test-{}-{}", std::process::id(), bytes.len()));
        std::fs::write(&dir, bytes).unwrap();
        let events = load(&dir).unwrap();
        std::fs::remove_file(&dir).unwrap();
        let device = choose_device(&events, None).unwrap();
        let start = decode::start_time(&events);
        decode::lines(&events, &device)
            .iter()
            .filter_map(|l| decode::format(l, start, true))
            .collect()
    }

    #[test]
    fn usbmon_capture_end_to_end() {
        let request = p::get_lighting_mode(7, p::SESSION);
        let reply = p::frame(p::GET_LIGHTING_MODE, &[p::MODE_GENERAL], 7, p::SESSION).unwrap();
        let file = pcapng(
            220,
            &[
                (0, &usbmon_packet(b'C', 0x81, 2, &[0; 8])), // another device
                (1_000_000, &usbmon_packet(b'S', 0x04, 3, &request)),
                (1_000_000, &usbmon_packet(b'C', 0x04, 3, &[])),
                (2_500_000, &usbmon_packet(b'C', 0x83, 3, &reply)),
            ],
        );
        let lines = decode_file(&file);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines[0].contains("→  10:01 GetLightingMode") && lines[0].contains("req 07"),
            "{}",
            lines[0]
        );
        assert!(
            lines[1].trim_start().starts_with("2.500") && lines[1].ends_with("01"),
            "{}",
            lines[1]
        );
    }

    #[test]
    fn usbpcap_files_of_several_hubs_are_merged() {
        let on_hub = |hub: u16, packet: Vec<u8>| {
            let mut packet = packet;
            packet[17..19].copy_from_slice(&hub.to_le_bytes());
            packet
        };
        let dir = std::env::temp_dir().join(format!("qlcap-merge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mouse = pcapng(
            249,
            &[(0, &on_hub(1, usbpcap_packet(true, 0x81, 5, &[0; 4])))],
        );
        let request = p::get_device_info(4);
        let reply = p::frame(p::GET_DEVICE_INFO, &[2, 0, 1, 1, 0, 0, 3, 2], 4, 0).unwrap();
        let keyboard = pcapng(
            249,
            &[
                (2_000_000, &on_hub(2, usbpcap_packet(true, 0x83, 5, &reply))),
                (
                    1_000_000,
                    &on_hub(2, usbpcap_packet(false, 0x04, 5, &request)),
                ),
            ],
        );
        let paths = [dir.join("hub1.pcap"), dir.join("hub2.pcap")];
        std::fs::write(&paths[0], mouse).unwrap();
        std::fs::write(&paths[1], keyboard).unwrap();
        let events = load_all(&paths).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(choose_device(&events, None).unwrap(), "2.5");
        let start = decode::start_time(&events);
        let lines: Vec<String> = decode::lines(&events, "2.5")
            .iter()
            .filter_map(|l| decode::format(l, start, true))
            .collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("→  03:01"), "{lines:?}");
    }

    #[test]
    fn usbpcap_capture_end_to_end() {
        let request = p::get_device_info(4);
        let file = pcapng(249, &[(0, &usbpcap_packet(false, 0x04, 5, &request))]);
        let lines = decode_file(&file);
        assert!(lines[0].contains("03:01 GetDeviceInfo"), "{lines:?}");
    }

    #[test]
    fn webhid_log_end_to_end() {
        let request = decode::hex(&p::get_device_info(4)).replace(' ', "");
        let log = format!(
            "# quietlinux webhid log v1\n10.0 note opened IO Center\n12.0 out 373f:0002 0 {request}\n"
        );
        let lines = decode_file(log.as_bytes());
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with("# opened IO Center"));
        assert!(lines[1].trim_start().starts_with("2.000"));
    }
}
