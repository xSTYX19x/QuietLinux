//! Vendor messages out of USB transfers: detection, reassembly, formatting.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use quietlinux_core::protocol::{self as p, Command, Message, PACKET_SIZE, Packet};

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
    /// Capture time in milliseconds.
    pub fn time(&self) -> f64 {
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
    /// A message with the contents of a macro, printed without its data (see [`hide_macros`]).
    Hidden {
        time: f64,
        dir: Dir,
        message: Message,
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
        Line::Message { time, .. }
        | Line::Broken { time, .. }
        | Line::Hidden { time, .. }
        | Line::Note { time, .. } => *time,
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

/// Key press notifications: `11:02` ActionFired and, while a macro is recorded,
/// `12:05` InputEvent (request ID 0; as requests the same codes are `SetBinding`
/// and `GetMacroEvents`).
fn is_key_event(message: &Message) -> bool {
    message.is_notification() && [p::ACTION_FIRED, p::INPUT_EVENT].contains(&message.cmd)
}

/// Macro events and names are private: a macro can hold typed text. Messages
/// with the contents of a macro other than `keep` become [`Line::Hidden`]. The
/// macro ID is the start of a request; a reply gets it from its request. Replies
/// to `SetMacroEvents` and `SetMacroName` are hidden too: their unused bytes
/// repeat the start of the request. [cap]
pub fn hide_macros(lines: Vec<Line>, keep: Option<u16>) -> Vec<Line> {
    const CONTENTS: [Command; 4] = [
        p::GET_MACRO_EVENTS,
        p::SET_MACRO_EVENTS,
        p::GET_MACRO_NAME,
        p::SET_MACRO_NAME,
    ];
    let mut requested: HashMap<(Command, u8), Option<u16>> = HashMap::new();
    lines
        .into_iter()
        .map(|line| {
            let Line::Message {
                time,
                dir,
                ref message,
                ..
            } = line
            else {
                return line;
            };
            let key = (message.cmd, message.request_id);
            if message.is_notification() || !CONTENTS.contains(&message.cmd) {
                return line;
            }
            let macro_id = match dir {
                Dir::Out => {
                    let id = match message.data[..] {
                        [lo, hi, ..] => Some(u16::from_le_bytes([lo, hi])),
                        _ => None,
                    };
                    requested.insert(key, id);
                    // a Get request holds only the ID
                    if [p::GET_MACRO_EVENTS, p::GET_MACRO_NAME].contains(&message.cmd) {
                        return line;
                    }
                    id
                }
                Dir::In => requested.remove(&key).flatten(),
            };
            if keep.is_some() && macro_id == keep {
                return line;
            }
            let Line::Message { message, .. } = line else {
                unreachable!()
            };
            Line::Hidden { time, dir, message }
        })
        .collect()
}

/// Serial number replies are never printed: they identify the keyboard.
fn is_serial_reply(dir: Dir, message: &Message) -> bool {
    dir == Dir::In && message.cmd == p::GET_SERIAL_NUMBER
}

/// Human-readable line, or `None` if filtered out.
pub fn format(line: &Line, start: f64, key_events: bool) -> Option<String> {
    Some(match line {
        Line::Message {
            time, dir, message, ..
        } => {
            if !key_events && is_key_event(message) {
                return None;
            }
            let data = if is_serial_reply(*dir, message) {
                "<serial number redacted>".to_owned()
            } else {
                hex(&message.data)
            };
            message_line(*time - start, *dir, message, &data)
        }
        Line::Hidden { time, dir, message } => {
            let data = format!("<macro contents hidden, {} bytes>", message.data.len());
            message_line(*time - start, *dir, message, &data)
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

fn message_line(time: f64, dir: Dir, message: &Message, data: &str) -> String {
    let status = match message.status {
        p::STATUS_SUCCESS => String::new(),
        s => format!(
            " status {}",
            p::status_name(s).map_or_else(|| format!("{s:#04x}"), str::to_owned)
        ),
    };
    let request = if message.is_notification() {
        "notify".to_owned()
    } else {
        format!("req {:02x}", message.request_id)
    };
    format!(
        "{time:>10.3}  {}  {} {:<22} {request} s{:02x}{status}  {data}",
        arrow(dir),
        message.cmd,
        message.name().unwrap_or("?"),
        message.session,
    )
}

/// Test fixture line (packets as hex, see `crates/quietlinux-core/tests/fixtures`).
/// Anonymized: serial number replies, key events and hidden macro contents are left out.
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
    if is_serial_reply(*dir, message) || is_key_event(message) {
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
        message.name().unwrap_or("?")
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
        let packet = p::get_lighting_mode(1, p::SESSION);
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
            transfer(1.0, "1.3", Dir::Out, &p::get_device_info(1)),
            transfer(
                2.0,
                "1.3",
                Dir::In,
                &p::frame(p::GET_DEVICE_INFO, &[2, 0, 1, 1, 0, 0, 3, 2], 1, 0).unwrap(),
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
        events.push(transfer(
            9.0,
            "1.3",
            Dir::In,
            &p::get_lighting_mode(9, p::SESSION),
        ));
        let lines = lines(&events, "1.3");
        assert_eq!(lines.len(), 3);
        assert!(matches!(&lines[0], Line::Note { .. }));
        let Line::Message {
            time, message: m, ..
        } = &lines[1]
        else {
            panic!("{lines:?}")
        };
        assert_eq!(
            (*time, m.cmd, m.request_id),
            (1.0, p::PERFORM_REALTIME_UPDATES, 5)
        );
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
        let serial = p::frame(p::GET_SERIAL_NUMBER, b"\x0cSECRET123456", 3, 0).unwrap();
        let key = p::frame(p::ACTION_FIRED, &[0x5c, 0, 1, 1, 0, 0x4f], 0, p::SESSION).unwrap();
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
        assert!(shown[1].contains("ActionFired") && shown[1].contains("notify"));
        assert_eq!(
            lines.iter().filter_map(|l| format(l, 0.0, false)).count(),
            1
        );
        assert_eq!(lines.iter().filter_map(fixture).count(), 0);
    }

    #[test]
    fn macro_contents_are_hidden() {
        let events_of = |id: u8, req: u8| {
            p::frame(
                p::SET_MACRO_EVENTS,
                &[id, 0, 1, 0, 0, 0, 2, 4, 0, 0],
                req,
                p::SESSION,
            )
            .unwrap()
        };
        let get_name = p::frame(p::GET_MACRO_NAME, &[2, 0], 3, p::SESSION).unwrap();
        let name = p::frame(p::GET_MACRO_NAME, b"\x02\x00\x06secret", 3, p::SESSION).unwrap();
        let recorded = p::frame(p::INPUT_EVENT, &[2, 4], 0, p::SESSION).unwrap();
        // the keyboard's reply repeats the request in its unused bytes
        let mut echo = p::frame(p::SET_MACRO_EVENTS, &[], 1, p::SESSION).unwrap();
        echo[7..17].copy_from_slice(&events_of(1, 1)[7..17]);
        let crc = p::crc16_modbus(&echo[..62]).to_le_bytes();
        echo[62..].copy_from_slice(&crc);
        let events = [
            transfer(0.0, "1.3", Dir::Out, &events_of(1, 1)),
            transfer(1.0, "1.3", Dir::Out, &events_of(3, 2)),
            transfer(2.0, "1.3", Dir::Out, &get_name),
            transfer(3.0, "1.3", Dir::In, &name),
            transfer(4.0, "1.3", Dir::In, &recorded),
            transfer(5.0, "1.3", Dir::In, &echo),
        ];
        let lines = hide_macros(lines(&events, "1.3"), Some(3));
        let shown: Vec<String> = lines.iter().filter_map(|l| format(l, 0.0, false)).collect();
        assert_eq!(shown.len(), 5, "{shown:?}");
        assert!(shown[4].contains("hidden, 0 bytes"), "{}", shown[4]);
        assert!(
            shown[0].ends_with("<macro contents hidden, 10 bytes>"),
            "{}",
            shown[0]
        );
        assert!(
            shown[1].ends_with("03 00 01 00 00 00 02 04 00 00"),
            "{}",
            shown[1]
        );
        assert!(shown[2].ends_with("02 00"), "{}", shown[2]);
        assert!(
            shown[3].contains("hidden") && !shown[3].contains("73 65"),
            "{}",
            shown[3]
        );
        let fixtures: Vec<String> = lines.iter().filter_map(fixture).collect();
        assert_eq!(fixtures.len(), 2);
        assert!(fixtures[0].ends_with("12:06 SetMacroEvents"));
        assert!(fixtures[1].ends_with("12:07 GetMacroName"));
        assert_eq!(
            hide_macros(lines, None).iter().filter_map(fixture).count(),
            1
        );
    }

    #[test]
    fn fixture_lines_load_like_the_core_fixtures() {
        let packet = p::static_color(Rgb::new(0xe6, 0x30, 0), 10, 0x2f, p::SESSION);
        let lines = lines(&[transfer(0.0, "1.3", Dir::Out, &packet)], "1.3");
        let line = fixture(&lines[0]).unwrap();
        let hex = line.split('#').next().unwrap().trim();
        assert_eq!(hex.len(), 128);
        assert!(line.ends_with("# out 10:06 SetLayerConfig"));
    }
}
