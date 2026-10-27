//! Vendor messages out of USB transfers: detection, reassembly, formatting.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use quietlinux_core::protocol::{self as p, Message, PACKET_SIZE, Packet};

use crate::usb::{Dir, Transfer};

/// What a capture contains, in time order.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Transfer(Transfer),
    /// A note written while capturing (`qlNote()` in the WebHID logger).
    Note {
        time: f64,
        text: String,
    },
}

impl Event {
    fn time(&self) -> f64 {
        match self {
            Event::Transfer(t) => t.time,
            Event::Note { time, .. } => *time,
        }
    }
}

/// The vendor packet in a transfer: 64 bytes (or 65 with a leading report ID 0)
/// with a plausible length byte and a valid CRC. Other devices' traffic fails the CRC.
pub fn vendor_packet(data: &[u8]) -> Option<Packet> {
    let data = match data {
        [0, rest @ ..] if rest.len() == PACKET_SIZE => rest,
        _ => data,
    };
    let packet: Packet = data.try_into().ok()?;
    ((2..=61).contains(&packet[0]) && p::crc_ok(&packet)).then_some(packet)
}

/// Devices that sent or received vendor packets, with their count, most first.
pub fn devices(events: &[Event]) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for event in events {
        if let Event::Transfer(t) = event {
            if vendor_packet(&t.data).is_some() {
                *counts.entry(&t.device).or_default() += 1;
            }
        }
    }
    let mut devices: Vec<(String, usize)> =
        counts.into_iter().map(|(d, n)| (d.to_owned(), n)).collect();
    devices.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    devices
}

/// One decoded line of a capture.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// A complete message, at the time of its first fragment.
    Message {
        time: f64,
        dir: Dir,
        message: Message,
        packets: Vec<Packet>,
    },
    /// A fragment that does not belong to a complete message.
    Broken {
        time: f64,
        dir: Dir,
        packet: Packet,
    },
    Note {
        time: f64,
        text: String,
    },
}

/// Vendor messages of `device`, fragments reassembled per direction, with the notes.
pub fn lines(events: &[Event], device: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pending: [Vec<(f64, Packet)>; 2] = [Vec::new(), Vec::new()];
    for event in events {
        let t = match event {
            Event::Note { time, text } => {
                out.push(Line::Note {
                    time: *time,
                    text: text.clone(),
                });
                continue;
            }
            Event::Transfer(t) if t.device == device => t,
            Event::Transfer(_) => continue,
        };
        let Some(packet) = vendor_packet(&t.data) else {
            continue;
        };
        let buf = &mut pending[usize::from(t.dir == Dir::In)];
        if p::fragment_index(&packet) == 0 {
            flush_broken(&mut out, buf, t.dir);
        }
        buf.push((t.time, packet));
        if !p::has_more(&packet) {
            let packets: Vec<Packet> = buf.iter().map(|(_, p)| *p).collect();
            match p::reassemble(&packets) {
                Some(message) => {
                    out.push(Line::Message {
                        time: buf[0].0,
                        dir: t.dir,
                        message,
                        packets,
                    });
                    buf.clear();
                }
                None => flush_broken(&mut out, buf, t.dir),
            }
        }
    }
    for (i, dir) in [Dir::Out, Dir::In].into_iter().enumerate() {
        flush_broken(&mut out, &mut pending[i], dir);
    }
    out.sort_by(|a, b| line_time(a).total_cmp(&line_time(b)));
    out
}

fn flush_broken(out: &mut Vec<Line>, buf: &mut Vec<(f64, Packet)>, dir: Dir) {
    out.extend(
        buf.drain(..)
            .map(|(time, packet)| Line::Broken { time, dir, packet }),
    );
}

fn line_time(line: &Line) -> f64 {
    match line {
        Line::Message { time, .. } | Line::Broken { time, .. } | Line::Note { time, .. } => *time,
    }
}

/// Time of the first event, the zero of the printed times.
pub fn start_time(events: &[Event]) -> f64 {
    events.iter().map(Event::time).fold(f64::INFINITY, f64::min)
}

/// Bytes as hex separated by spaces.
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn arrow(dir: Dir) -> &'static str {
    match dir {
        Dir::Out => "→",
        Dir::In => "←",
    }
}

/// Serial number replies are never printed: they identify the keyboard.
fn is_serial_reply(dir: Dir, message: &Message) -> bool {
    dir == Dir::In && message.cmd == p::GET_SERIAL
}

/// Human-readable line, or `None` if filtered out.
pub fn format(line: &Line, start: f64, key_events: bool) -> Option<String> {
    Some(match line {
        Line::Message {
            time, dir, message, ..
        } => {
            if !key_events && message.cmd == p::KEY_EVENT {
                return None;
            }
            let data = if is_serial_reply(*dir, message) {
                "<serial number redacted>".to_owned()
            } else {
                hex(&message.data)
            };
            format!(
                "{:>10.3}  {}  {} {:<12} seq {:02x} s{:02x}  {data}",
                time - start,
                arrow(*dir),
                message.cmd,
                p::command_name(message.cmd).unwrap_or("?"),
                message.seq,
                message.session,
            )
        }
        Line::Broken { time, dir, packet } => {
            format!(
                "{:>10.3}  {}  broken fragment  {}",
                time - start,
                arrow(*dir),
                hex(packet)
            )
        }
        Line::Note { time, text } => format!("{:>10.3}  # {text}", time - start),
    })
}

/// Test fixture line (packets as hex, see `crates/quietlinux-core/tests/fixtures`).
/// Anonymized: serial number replies and key events are left out.
pub fn fixture(line: &Line) -> Option<String> {
    let Line::Message {
        dir,
        message,
        packets,
        ..
    } = line
    else {
        return None;
    };
    if is_serial_reply(*dir, message) || message.cmd == p::KEY_EVENT {
        return None;
    }
    let packets: Vec<String> = packets.iter().map(|p| hex(p).replace(' ', "")).collect();
    let dir = match dir {
        Dir::Out => "out",
        Dir::In => "in",
    };
    Some(format!(
        "{}  # {dir} {} {}",
        packets.join(" "),
        message.cmd,
        p::command_name(message.cmd).unwrap_or("?")
    ))
}

#[cfg(test)]
mod tests {
    use quietlinux_core::Rgb;
    use quietlinux_core::protocol::Led;

    use super::*;

    fn transfer(time: f64, device: &str, dir: Dir, data: &[u8]) -> Event {
        Event::Transfer(Transfer {
            time,
            device: device.to_owned(),
            dir,
            data: data.to_vec(),
        })
    }

    #[test]
    fn detects_vendor_packets() {
        let packet = p::get_mode(1, p::SESSION);
        assert_eq!(vendor_packet(&packet), Some(packet));
        let mut with_id = vec![0];
        with_id.extend_from_slice(&packet);
        assert_eq!(vendor_packet(&with_id), Some(packet));
        let mut corrupt = packet;
        corrupt[9] ^= 1;
        assert_eq!(vendor_packet(&corrupt), None);
        assert_eq!(vendor_packet(&packet[..63]), None);
        assert_eq!(vendor_packet(&[0u8; 8]), None); // boot keyboard report
    }

    #[test]
    fn picks_the_device_with_vendor_packets() {
        let events = [
            transfer(0.0, "1.2", Dir::In, &[0; 8]),
            transfer(1.0, "1.3", Dir::Out, &p::get_version(1)),
            transfer(
                2.0,
                "1.3",
                Dir::In,
                &p::frame(p::GET_VERSION, &[2, 0, 1, 1, 0, 0, 3, 2], 1, 0).unwrap(),
            ),
        ];
        assert_eq!(devices(&events), [("1.3".to_owned(), 2)]);
    }

    #[test]
    fn reassembles_fragments() {
        let leds: Vec<Led> = (0..40).map(|i| Led::new(i, Rgb::new(1, 2, 3))).collect();
        let message = &p::update(&leds, 5, p::SESSION)[0];
        assert!(message.len() > 1);
        let mut events = vec![Event::Note {
            time: 0.5,
            text: "all LEDs".into(),
        }];
        events.extend(message.iter().enumerate().map(|(i, pk)| {
            transfer(
                1.0 + f64::from(u8::try_from(i).unwrap()),
                "1.3",
                Dir::Out,
                pk,
            )
        }));
        events.push(transfer(9.0, "1.3", Dir::In, &p::get_mode(9, p::SESSION)));
        let lines = lines(&events, "1.3");
        assert_eq!(lines.len(), 3);
        assert!(matches!(&lines[0], Line::Note { .. }));
        let Line::Message {
            time, message: m, ..
        } = &lines[1]
        else {
            panic!("{lines:?}")
        };
        assert_eq!((*time, m.cmd, m.seq), (1.0, p::SET_LEDS, 5));
        assert_eq!(p::parse_leds(message).unwrap().entries.len(), 40);
    }

    #[test]
    fn incomplete_message_is_broken() {
        let leds: Vec<Led> = (0..40).map(|i| Led::new(i, Rgb::BLACK)).collect();
        let message = &p::update(&leds, 5, p::SESSION)[0];
        let events: Vec<Event> = message[..2]
            .iter()
            .map(|pk| transfer(1.0, "1.3", Dir::Out, pk))
            .collect();
        let lines = lines(&events, "1.3");
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| matches!(l, Line::Broken { .. })));
    }

    #[test]
    fn serial_is_redacted_and_key_events_filtered() {
        let serial = p::frame(p::GET_SERIAL, b"\x0cSECRET123456", 3, 0).unwrap();
        let key = p::frame(p::KEY_EVENT, &[0x5c, 0, 1, 1, 0, 0x4f], 0, p::SESSION).unwrap();
        let events = [
            transfer(0.0, "1.3", Dir::In, &serial),
            transfer(1.0, "1.3", Dir::In, &key),
        ];
        let lines = lines(&events, "1.3");
        let shown: Vec<String> = lines.iter().filter_map(|l| format(l, 0.0, true)).collect();
        assert!(
            shown[0].contains("redacted") && !shown[0].contains("53 45 43"),
            "{}",
            shown[0]
        );
        assert!(shown[1].contains("key event"));
        assert_eq!(
            lines.iter().filter_map(|l| format(l, 0.0, false)).count(),
            1
        );
        assert_eq!(lines.iter().filter_map(fixture).count(), 0);
    }

    #[test]
    fn fixture_lines_load_like_the_core_fixtures() {
        let packet = p::static_color(Rgb::new(0xe6, 0x30, 0), 10, 0x2f, p::SESSION);
        let lines = lines(&[transfer(0.0, "1.3", Dir::Out, &packet)], "1.3");
        let line = fixture(&lines[0]).unwrap();
        let hex = line.split('#').next().unwrap().trim();
        assert_eq!(hex.len(), 128);
        assert!(line.ends_with("# out 10:06 effect"));
    }
}
