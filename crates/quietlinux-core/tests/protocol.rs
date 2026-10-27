//! Vendor protocol (interface 2) against captured frames. Needs no keyboard.
//!
//! Port of `tests/test_protocol.py` of `bbossa/keyboard_bequiet_lightmount_rgb`; the
//! fixtures are copied unchanged from that project (see tests/fixtures/README.md).

use quietlinux_core::Rgb;
use quietlinux_core::protocol::{self as p, Command, Led, Packet};

/// Messages of a fixture file; a message is a list of 64-byte packets.
fn load(name: &str) -> Vec<Vec<Packet>> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    text.lines()
        .map(|line| line.split('#').next().unwrap().trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.split_whitespace().map(parse_packet).collect())
        .collect()
}

fn parse_packet(hex: &str) -> Packet {
    assert_eq!(hex.len(), 128, "packet of {} hex digits", hex.len());
    let mut packet = [0u8; 64];
    for (i, byte) in packet.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    packet
}

fn singles(name: &str) -> Vec<Packet> {
    load(name).into_iter().map(|m| m[0]).collect()
}

fn frames() -> Vec<Packet> {
    singles("frames-out.hex")
}

fn leds() -> Vec<Vec<Packet>> {
    load("leds-10-0d.hex")
}

fn mode_responses() -> Vec<Packet> {
    singles("responses-10-01.hex")
}

fn info_requests() -> Vec<Packet> {
    singles("requests-03.hex")
}

fn version_responses() -> Vec<Packet> {
    singles("responses-03-01.hex")
}

fn all_packets() -> Vec<Packet> {
    let mut all = frames();
    all.extend(leds().into_iter().flatten());
    all.extend(mode_responses());
    all.extend(info_requests());
    all.extend(version_responses());
    all
}

/// `(cmd, data, seq, session)` of a single-packet frame.
fn fields(raw: &Packet) -> (Command, Vec<u8>, u8, u8) {
    (
        p::command_of(raw),
        p::data(raw).to_vec(),
        p::seq_of(raw),
        raw[2],
    )
}

/// Group 10:0d messages into complete updates (up to the message marked last).
fn series(messages: Vec<Vec<Packet>>) -> Vec<Vec<Vec<Packet>>> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    for msg in messages {
        let last = p::parse_leds(&msg).unwrap().last;
        current.push(msg);
        if last {
            out.push(std::mem::take(&mut current));
        }
    }
    assert!(current.is_empty(), "update without its last message");
    out
}

// --- fixtures --------------------------------------------------------------------

#[test]
fn fixtures_present() {
    assert!(!frames().is_empty());
    assert_eq!(leds().len(), 39);
    assert!(!mode_responses().is_empty());
}

// --- CRC -------------------------------------------------------------------------

#[test]
fn crc_check_value() {
    // check value from the CRC catalogue for CRC-16/MODBUS
    assert_eq!(p::crc16_modbus(b"123456789"), 0x4b37);
}

#[test]
fn crc_of_captured_packets() {
    for raw in all_packets() {
        assert!(p::crc_ok(&raw), "{raw:02x?}");
    }
}

#[test]
fn crc_rejects_corruption() {
    let mut raw = frames()[0];
    raw[10] ^= 0x01;
    assert!(!p::crc_ok(&raw));
}

// --- frames ----------------------------------------------------------------------

#[test]
fn single_packet_frames() {
    for raw in frames() {
        let (cmd, data, seq, session) = fields(&raw);
        assert_eq!(
            p::frame(cmd, &data, seq, session).unwrap(),
            raw,
            "{cmd} seq {seq}"
        );
        // frame() and message() describe the same format for a single packet
        assert_eq!(p::message(cmd, &data, seq, session).unwrap(), [raw]);
    }
}

#[test]
fn seq_is_one_byte() {
    // byte 3 is always 00 in a first fragment: seq only uses byte 4
    for raw in all_packets() {
        if p::fragment_index(&raw) == 0 {
            assert_eq!(raw[3], 0x00);
        }
    }
}

#[test]
fn frame_single_packet_only() {
    assert!(p::frame(p::SET_LIGHTING, &[0; p::FIRST_CAPACITY], 0, p::SESSION).is_ok());
    assert!(p::frame(p::SET_LIGHTING, &[0; p::FIRST_CAPACITY + 1], 0, p::SESSION).is_err());
}

// --- lighting --------------------------------------------------------------------

#[test]
fn static_color() {
    let statics: Vec<Packet> = frames()
        .into_iter()
        .filter(|r| {
            p::command_of(r) == p::SET_LIGHTING
                && r[8] == p::EFFECT_STATIC
                && r[12] == p::COLOR_MODE_SINGLE
        })
        .collect();
    assert!(!statics.is_empty());
    for raw in statics {
        let (_, _, seq, session) = fields(&raw);
        assert_eq!(
            p::static_color(Rgb::new(raw[13], raw[14], raw[15]), raw[10], seq, session),
            raw
        );
    }
}

#[test]
fn set_mode() {
    let modes: Vec<Packet> = frames()
        .into_iter()
        .filter(|r| p::command_of(r) == p::SET_MODE)
        .collect();
    let mut seen: Vec<u8> = modes.iter().map(|r| r[7]).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, [p::MODE_EFFECTS, p::MODE_CUSTOM]);
    for raw in modes {
        let (_, data, seq, session) = fields(&raw);
        assert_eq!(p::set_mode(data[0], seq, session), raw);
    }
}

#[test]
fn get_mode_request() {
    let requests: Vec<Packet> = frames()
        .into_iter()
        .filter(|r| p::command_of(r) == p::GET_MODE)
        .collect();
    assert!(!requests.is_empty());
    for raw in requests {
        let (_, _, seq, session) = fields(&raw);
        assert_eq!(p::get_mode(seq, session), raw);
    }
}

#[test]
fn get_mode_responses() {
    // the reply echoes seq and command, the mode is the first data byte
    let mut seen = Vec::new();
    for raw in mode_responses() {
        assert_eq!(p::command_of(&raw), p::GET_MODE);
        let (_, _, seq, session) = fields(&raw);
        assert!(p::is_reply(&raw, &p::get_mode(seq, session)));
        assert!(!p::is_reply(
            &raw,
            &p::get_mode(seq.wrapping_add(1), session)
        ));
        seen.push(p::mode_of(&raw).unwrap());
    }
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, [p::MODE_EFFECTS, p::MODE_CUSTOM]);
}

// --- identification ----------------------------------------------------------------

#[test]
fn identification_requests() {
    for raw in info_requests() {
        let (cmd, data, seq, session) = fields(&raw);
        assert!(cmd == p::GET_VERSION || cmd == p::GET_SERIAL);
        assert!(data.is_empty());
        assert_eq!(p::frame(cmd, &[], seq, session).unwrap(), raw);
    }
}

#[test]
fn web_identification_requests() {
    // IO Center Web sends them before 01:01, with session 00: the frames we send
    let web: Vec<Packet> = info_requests()
        .into_iter()
        .filter(|r| r[2] == p::SESSION_NONE)
        .collect();
    assert!(web.iter().any(|r| p::command_of(r) == p::GET_VERSION));
    assert!(web.iter().any(|r| p::command_of(r) == p::GET_SERIAL));
    for raw in web {
        let build = if p::command_of(&raw) == p::GET_VERSION {
            p::get_version
        } else {
            p::get_serial
        };
        assert_eq!(build(p::seq_of(&raw)), raw);
    }
}

#[test]
fn version_replies() {
    assert!(!version_responses().is_empty());
    for resp in version_responses() {
        assert_eq!(
            p::version_of(&resp),
            [0x02, 0x00, 0x01, 0x01, 0x00, 0x00, 0x03, 0x02]
        );
    }
}

#[test]
fn serial_of() {
    let resp = p::frame(p::GET_SERIAL, b"\x04AB12", 7, p::SESSION_NONE).unwrap();
    assert_eq!(p::serial_of(&resp), "AB12");
    // a length byte larger than the data is clamped
    let resp = p::frame(p::GET_SERIAL, b"\xffAB", 7, p::SESSION_NONE).unwrap();
    assert_eq!(p::serial_of(&resp), "AB");
}

// --- key events (captured on 2026-10-15 by bbossa) ---------------------------------

fn event(data: &[u8]) -> Packet {
    p::frame(p::KEY_EVENT, data, 0x00, p::SESSION).unwrap()
}

#[test]
fn fn_effect() {
    assert_eq!(
        p::fn_function(&event(&[0x5c, 0x80, 0x01, 0x09, 0x03])),
        Some(p::FN_EFFECT)
    ); // Fn+→
}

#[test]
fn fn_alone() {
    assert_eq!(
        p::fn_function(&event(&[0x4b, 0x80, 0x01, 0x1b])),
        Some(0x1b)
    );
}

#[test]
fn plain_keys() {
    for data in [
        [0x53, 0x00, 0x01, 0x01, 0x00, 0x50],
        [0x5c, 0x00, 0x01, 0x01, 0x00, 0x4f],
    ] {
        assert_eq!(p::fn_function(&event(&data)), None); // ←, →
    }
}

#[test]
fn other_messages_are_not_fn_events() {
    assert_eq!(p::fn_function(&p::get_mode(1, p::SESSION)), None);
    assert_eq!(p::fn_function(&event(&[0x5c, 0x80])), None); // too short
    let packets = p::message(p::KEY_EVENT, &[0; 70], 0, p::SESSION).unwrap();
    assert_eq!(p::fn_function(&packets[1]), None); // continuation fragment
}

// --- per-LED colors ------------------------------------------------------------------

#[test]
fn led_messages() {
    for msg in leds() {
        let decoded = p::parse_leds(&msg).unwrap();
        let payload = p::leds_payload(&decoded.entries, decoded.last).unwrap();
        assert_eq!(
            p::message(p::SET_LEDS, &payload, decoded.seq, decoded.session).unwrap(),
            msg,
            "seq {}",
            decoded.seq
        );
    }
}

#[test]
fn complete_updates() {
    // update() reproduces IO Center's series: 40 entries per message, last one marked
    let all = series(leds());
    assert_eq!(all.iter().map(Vec::len).sum::<usize>(), leds().len());
    for s in all {
        let first = p::parse_leds(&s[0]).unwrap();
        let entries: Vec<Led> = s
            .iter()
            .flat_map(|m| p::parse_leds(m).unwrap().entries)
            .collect();
        assert_eq!(
            p::update(&entries, first.seq, first.session),
            s,
            "seq {}",
            first.seq
        );
    }
}

#[test]
fn n_rule() {
    // N = entries for an intermediate message, entries + 1 followed by 00 for the last one
    let entries: Vec<Led> = (0..3).map(|i| Led::new(i, Rgb::new(1, 2, 3))).collect();
    assert_eq!(p::leds_payload(&entries, false).unwrap()[0], 3);
    let last = p::leds_payload(&entries, true).unwrap();
    assert_eq!(
        (last[0], *last.last().unwrap(), last.len()),
        (4, 0x00, 1 + 3 * 6 + 1)
    );
}

#[test]
fn entry_format() {
    let payload = p::leds_payload(&[Led::new(0x2d, Rgb::new(0xff, 0x2a, 0x00))], false).unwrap();
    assert_eq!(payload, [1, 0x03, 0x2d, 0x00, 0xff, 0x2a, 0x00]);
}

#[test]
fn too_many_entries() {
    let entries = vec![Led::new(0, Rgb::BLACK); 255];
    assert!(p::leds_payload(&entries, false).is_err());
}

#[test]
fn seq_wraps() {
    let entries: Vec<Led> = (0..80).map(|i| Led::new(i, Rgb::BLACK)).collect();
    let msgs = p::update(&entries, 0xff, p::SESSION);
    assert_eq!(
        msgs.iter().map(|m| p::seq_of(&m[0])).collect::<Vec<_>>(),
        [0xff, 0x00]
    );
}

#[test]
fn empty_update() {
    assert!(p::update(&[], 0, p::SESSION).is_empty());
}

// --- fragmentation ---------------------------------------------------------------------

#[test]
fn capacities() {
    // 55 data bytes in the first fragment, 59 in the following ones
    for (size, count) in [(0, 1), (55, 1), (56, 2), (55 + 59, 2), (55 + 59 + 1, 3)] {
        assert_eq!(
            p::message(p::SET_LEDS, &vec![0; size], 0, p::SESSION)
                .unwrap()
                .len(),
            count,
            "{size} bytes"
        );
    }
    assert!(p::message(p::SET_LEDS, &vec![0; p::MAX_PAYLOAD], 0, p::SESSION).is_ok());
    assert!(p::message(p::SET_LEDS, &vec![0; p::MAX_PAYLOAD + 1], 0, p::SESSION).is_err());
}

#[test]
fn index_and_continuation() {
    let packets = p::message(p::SET_LEDS, &[0; 200], 0x42, 0x03).unwrap();
    assert_eq!(
        packets.iter().map(|pk| pk[1]).collect::<Vec<_>>(),
        [0x80, 0x81, 0x82, 0x03]
    );
    assert!(packets.iter().all(|pk| pk[2] == 0x03));
    assert_eq!(packets[0][3..7], [0x00, 0x42, 0x10, 0x0d]);
}

#[test]
fn reassembly() {
    let payload: Vec<u8> = (0..200).collect();
    let packets = p::message(p::SET_LEDS, &payload, 7, p::SESSION).unwrap();
    let msg = p::reassemble(&packets).unwrap();
    assert_eq!(
        (msg.seq, msg.session, msg.cmd, msg.data),
        (7, p::SESSION, p::SET_LEDS, payload)
    );
}

#[test]
fn reassembly_rejects_incomplete_or_shuffled() {
    let packets = p::message(p::SET_LEDS, &[0; 200], 0, p::SESSION).unwrap();
    assert!(p::reassemble(&[]).is_none());
    assert!(p::reassemble(&packets[..3]).is_none()); // last fragment missing
    assert!(p::reassemble(&packets[1..]).is_none()); // first fragment missing
    let shuffled = [packets[0], packets[2], packets[1], packets[3]];
    assert!(p::reassemble(&shuffled).is_none());
}

#[test]
fn garbage_does_not_panic() {
    // whatever the keyboard sends, decoding must not panic
    for len in [0u8, 1, 2, 3, 6, 7, 61, 62, 63, 0xff] {
        let mut raw = [0xaa; 64];
        raw[0] = len;
        let _ = (
            p::data(&raw),
            p::version_of(&raw),
            p::serial_of(&raw),
            p::mode_of(&raw),
            p::fn_function(&raw),
        );
        let _ = p::parse_leds(&[raw]);
    }
}
