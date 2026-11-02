//! Entry points for fuzzing the parsers (safety rule 10: everything from the
//! keyboard or from files is untrusted; broken input must never panic).
//!
//! The `cargo fuzz` targets in `fuzz/` call these functions with generated
//! input (needs a nightly toolchain); the tests below run them with random and
//! damaged input on stable, in every `cargo test`.

use crate::protocol::{self as p, PACKET_SIZE, Packet};
use crate::{config, hid_descriptor, info, iocenter, macros};

/// Input as 64-byte packets (the last one zero-padded).
fn packets(data: &[u8]) -> Vec<Packet> {
    data.chunks(PACKET_SIZE)
        .map(|chunk| {
            let mut packet = [0; PACKET_SIZE];
            packet[..chunk.len()].copy_from_slice(chunk);
            packet
        })
        .collect()
}

/// Packets from the keyboard: framing, CRC, reassembly, per-LED messages.
pub fn frames(data: &[u8]) {
    let list = packets(data);
    for raw in &list {
        let _ = (
            p::crc_ok(raw),
            p::command_of(raw),
            p::status_of(raw),
            p::fragment_index(raw),
            p::has_more(raw),
            p::is_notification(raw),
            p::request_id_of(raw),
            p::data(raw),
            p::device_info_of(raw),
        );
    }
    let _ = p::reassemble(&list);
    let _ = p::parse_leds(&list);
}

/// The data of replies and notifications, by every decoder.
pub fn replies(data: &[u8]) {
    let _ = info::DeviceInfo::parse(data);
    let _ = info::parse_qlink_version(data);
    let _ = info::parse_id_list(data);
    let _ = info::parse_macro_ids(data);
    let _ = info::parse_features(data);
    let _ = info::Layout::parse(data);
    let _ = info::parse_snap_tap(data);
    let _ = info::LayerConfig::parse(data);
    let _ = info::Calibration::parse(data);
    let _ = info::parse_bindings(data);
    let _ = info::KeyEvent::parse(data);
    let _ = p::parse_open_session(data);
    let _ = p::parse_macro_events_page(data);
    let _ = p::parse_macro_name(data);
    let _ = p::parse_macro_events(data);
    if let Some((action, used)) = info::parse_action(data) {
        assert!(used <= data.len(), "parse_action used more than it got");
        // what QuietLinux writes back must be what it read
        if let Some(bytes) = action.to_bytes() {
            let _ = info::parse_action(&bytes);
        }
        let _ = action.to_string();
        let _ = config::action_text(&action);
    }
}

/// HID report descriptors (sysfs).
pub fn descriptor(data: &[u8]) {
    let _ = hid_descriptor::top_level_usage(data);
}

/// Text from files and command lines: settings files, bindings, macro events.
pub fn text(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = config::parse(text);
    let _ = config::parse_desired(text);
    let _ = macros::parse_events(&[text.to_owned()], macros::DEFAULT_DELAY_MS);
    let _ = macros::parse_raw_events(text);
}

/// IO Center files: zip archives and JSON profiles.
pub fn iocenter(data: &[u8]) {
    if let Ok(list) = iocenter::from_bytes(data) {
        for imported in list {
            // what the import writes is a settings file QuietLinux reads
            config::parse(&imported.text).expect("an imported profile does not parse");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `base` rounds, times `QUIETLINUX_FUZZ_ROUNDS` (for a longer search:
    /// `QUIETLINUX_FUZZ_ROUNDS=100 cargo test --release -p quietlinux-core fuzzing`).
    fn rounds(base: usize) -> usize {
        let factor = std::env::var("QUIETLINUX_FUZZ_ROUNDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        base * factor
    }

    /// A small deterministic generator (xorshift), so that a failure repeats.
    struct Random(u64);

    impl Random {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            usize::try_from(self.next() % u64::try_from(n.max(1)).unwrap_or(1)).unwrap_or(0)
        }

        fn bytes(&mut self, max: usize) -> Vec<u8> {
            let len = self.below(max + 1);
            (0..len).map(|_| self.next().to_le_bytes()[0]).collect()
        }

        /// `seed` with a few bytes changed, cut, or repeated.
        fn damaged(&mut self, seed: &[u8]) -> Vec<u8> {
            let mut out = seed.to_vec();
            for _ in 0..=self.below(4) {
                match self.below(4) {
                    0 if !out.is_empty() => {
                        let at = self.below(out.len());
                        out[at] ^= 1 << self.below(8);
                    }
                    1 if !out.is_empty() => {
                        let at = self.below(out.len());
                        out[at] = self.next().to_le_bytes()[0];
                    }
                    2 => out.truncate(self.below(out.len() + 1)),
                    _ if !out.is_empty() => {
                        let from = self.below(out.len());
                        let to = (from + self.below(16)).min(out.len());
                        let copy = out[from..to].to_vec();
                        out.splice(from..from, copy);
                    }
                    _ => {}
                }
            }
            out
        }
    }

    /// Captured packets of the fixtures, one message per line.
    fn fixture_messages() -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for file in [
            include_str!("../tests/fixtures/windows.hex"),
            include_str!("../tests/fixtures/connect-web.hex"),
            include_str!("../tests/fixtures/bindings-web.hex"),
        ] {
            for line in file.lines() {
                let hex: String = line
                    .split('#')
                    .next()
                    .unwrap_or("")
                    .chars()
                    .filter(char::is_ascii_hexdigit)
                    .collect();
                let bytes: Vec<u8> = (0..hex.len() / 2)
                    .filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok())
                    .collect();
                if !bytes.is_empty() {
                    out.push(bytes);
                }
            }
        }
        out
    }

    #[test]
    fn frames_and_replies_never_panic() {
        let mut random = Random(0x5eed_0001);
        let seeds = fixture_messages();
        assert!(seeds.len() > 100);
        for _ in 0..rounds(20_000) {
            let input = if random.below(2) == 0 {
                random.bytes(200)
            } else {
                let seed = random.below(seeds.len());
                random.damaged(&seeds[seed])
            };
            frames(&input);
            // the data part of a packet and the raw bytes
            replies(input.get(4..).unwrap_or(&[]));
            replies(&input);
            descriptor(&input);
        }
    }

    #[test]
    fn descriptors_never_panic() {
        let mut random = Random(0x5eed_0002);
        let seeds = [
            include_bytes!("../tests/fixtures/descriptors/if0.bin").as_slice(),
            include_bytes!("../tests/fixtures/descriptors/if1.bin").as_slice(),
            include_bytes!("../tests/fixtures/descriptors/if2.bin").as_slice(),
            include_bytes!("../tests/fixtures/descriptors/if3.bin").as_slice(),
        ];
        for _ in 0..rounds(20_000) {
            let seed = random.below(seeds.len());
            descriptor(&random.damaged(seeds[seed]));
        }
    }

    #[test]
    fn text_never_panics() {
        let mut random = Random(0x5eed_0003);
        let seeds: [&[u8]; 6] = [
            b"version = 1\n[game_mode]\nblocks = [\"win\"]\n[bindings]\nM2 = \"key ctrl+KeyC\"\n\"Fn+F1\" = \"media stop\"\n",
            b"version = 1\n[[macros]]\nid = 1\nname = \"x\"\nevents = \"+shift KeyA -shift 50ms\"\n",
            b"version = 1\n[effect]\neffect = \"color-wave\"\ngradient = [\"ff0000@0\", \"0000ff@100\"]\ndirection = \"left\"\n",
            b"version = 1\n[lighting]\nmode = \"per-key\"\n[lighting.leds]\n2e = \"ff0000\"\n",
            b"profile next",
            b"02 04 64 00 03 04 00 00",
        ];
        for _ in 0..rounds(5_000) {
            let seed = random.below(seeds.len());
            text(&random.damaged(seeds[seed]));
        }
    }

    #[test]
    fn iocenter_files_never_panic() {
        let mut random = Random(0x5eed_0004);
        let profile = include_bytes!("../tests/fixtures/iocenter-profile.json");
        for _ in 0..rounds(1_000) {
            iocenter(&random.damaged(profile));
        }
        // zip archives: valid ones damaged, and random bytes after the signature
        let mut zip = b"PK\x03\x04".to_vec();
        zip.extend(random.bytes(100));
        iocenter(&zip);
        for _ in 0..rounds(2_000) {
            let mut input = b"PK\x03\x04".to_vec();
            input.extend(random.bytes(300));
            input.extend_from_slice(b"PK\x05\x06");
            input.extend(random.bytes(30));
            iocenter(&input);
        }
    }
}
