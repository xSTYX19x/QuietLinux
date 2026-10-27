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
        p::request_id_of(raw),
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
    assert_eq!(current.len(), 0, "update without its last message");
    out
}

// --- fixtures --------------------------------------------------------------------

#[test]
fn fixtures_present() {
    assert_ne!(frames().len(), 0);
    assert_eq!(leds().len(), 39);
    assert_ne!(mode_responses().len(), 0);
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
    assert!(p::frame(p::SET_LAYER_CONFIG, &[0; p::FIRST_CAPACITY], 0, p::SESSION).is_ok());
    assert!(
        p::frame(
            p::SET_LAYER_CONFIG,
            &[0; p::FIRST_CAPACITY + 1],
            0,
            p::SESSION
        )
        .is_err()
    );
}

// --- lighting --------------------------------------------------------------------

#[test]
fn static_color() {
    let statics: Vec<Packet> = frames()
        .into_iter()
        .filter(|r| {
            p::command_of(r) == p::SET_LAYER_CONFIG
                && r[8] == p::EFFECT_STATIC
                && r[12] == p::COLOR_MODE_SINGLE
        })
        .collect();
    assert_ne!(statics.len(), 0);
    for raw in statics {
        let (_, _, seq, session) = fields(&raw);
        assert_eq!(
            p::static_color(Rgb::new(raw[13], raw[14], raw[15]), raw[10], seq, session),
            raw
        );
    }
}

#[test]
fn set_lighting_mode() {
    let modes: Vec<Packet> = frames()
        .into_iter()
        .filter(|r| p::command_of(r) == p::SET_LIGHTING_MODE)
        .collect();
    let mut seen: Vec<u8> = modes.iter().map(|r| r[7]).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, [p::MODE_GENERAL, p::MODE_REALTIME]);
    for raw in modes {
        let (_, data, seq, session) = fields(&raw);
        assert_eq!(p::set_lighting_mode(data[0], seq, session), raw);
    }
}

#[test]
fn get_mode_request() {
    let requests: Vec<Packet> = frames()
        .into_iter()
        .filter(|r| p::command_of(r) == p::GET_LIGHTING_MODE)
        .collect();
    assert_ne!(requests.len(), 0);
    for raw in requests {
        let (_, _, seq, session) = fields(&raw);
        assert_eq!(p::get_lighting_mode(seq, session), raw);
    }
}

#[test]
fn get_mode_responses() {
    // the reply echoes seq and command, the mode is the first data byte
    let mut seen = Vec::new();
    for raw in mode_responses() {
        assert_eq!(p::command_of(&raw), p::GET_LIGHTING_MODE);
        let (_, _, seq, session) = fields(&raw);
        assert!(p::is_reply(&raw, &p::get_lighting_mode(seq, session)));
        assert!(!p::is_reply(
            &raw,
            &p::get_lighting_mode(seq.wrapping_add(1), session)
        ));
        seen.push(p::lighting_mode_of(&raw).unwrap());
    }
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, [p::MODE_GENERAL, p::MODE_REALTIME]);
}

// --- identification ----------------------------------------------------------------

#[test]
fn identification_requests() {
    for raw in info_requests() {
        let (cmd, data, seq, session) = fields(&raw);
        assert!(cmd == p::GET_DEVICE_INFO || cmd == p::GET_SERIAL_NUMBER);
        assert_eq!(data.len(), 0);
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
    assert!(web.iter().any(|r| p::command_of(r) == p::GET_DEVICE_INFO));
    assert!(web.iter().any(|r| p::command_of(r) == p::GET_SERIAL_NUMBER));
    for raw in web {
        let build = if p::command_of(&raw) == p::GET_DEVICE_INFO {
            p::get_device_info
        } else {
            p::get_serial_number
        };
        assert_eq!(build(p::request_id_of(&raw)), raw);
    }
}

#[test]
fn version_replies() {
    assert_ne!(version_responses().len(), 0);
    for resp in version_responses() {
        assert_eq!(
            p::device_info_of(&resp),
            [0x02, 0x00, 0x01, 0x01, 0x00, 0x00, 0x03, 0x02]
        );
    }
}

#[test]
fn serial_of() {
    let resp = p::frame(p::GET_SERIAL_NUMBER, b"\x04AB12", 7, p::SESSION_NONE).unwrap();
    assert_eq!(p::serial_of(&resp), "AB12");
    // a length byte larger than the data is clamped
    let resp = p::frame(p::GET_SERIAL_NUMBER, b"\xffAB", 7, p::SESSION_NONE).unwrap();
    assert_eq!(p::serial_of(&resp), "AB");
}

// --- key events (captured on 2026-10-15 by bbossa) ---------------------------------

fn event(data: &[u8]) -> Packet {
    p::frame(p::ACTION_FIRED, data, 0x00, p::SESSION).unwrap()
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
    assert_eq!(p::fn_function(&p::get_lighting_mode(1, p::SESSION)), None);
    assert_eq!(p::fn_function(&event(&[0x5c, 0x80])), None); // too short
    let packets = p::message(p::ACTION_FIRED, &[0; 70], 0, p::SESSION).unwrap();
    assert_eq!(p::fn_function(&packets[1]), None); // continuation fragment
}

// --- per-LED colors ------------------------------------------------------------------

#[test]
fn led_messages() {
    for msg in leds() {
        let decoded = p::parse_leds(&msg).unwrap();
        let payload = p::leds_payload(&decoded.entries, decoded.last).unwrap();
        assert_eq!(
            p::message(
                p::PERFORM_REALTIME_UPDATES,
                &payload,
                decoded.request_id,
                decoded.session
            )
            .unwrap(),
            msg,
            "seq {}",
            decoded.request_id
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
            p::update(&entries, first.request_id, first.session),
            s,
            "seq {}",
            first.request_id
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
fn request_id_skips_zero() {
    let entries: Vec<Led> = (0..80).map(|i| Led::new(i, Rgb::BLACK)).collect();
    let msgs = p::update(&entries, 0xff, p::SESSION);
    assert_eq!(
        msgs.iter()
            .map(|m| p::request_id_of(&m[0]))
            .collect::<Vec<_>>(),
        [0xff, 0x01] // 0 marks notifications
    );
}

#[test]
fn empty_update() {
    assert_eq!(p::update(&[], 0, p::SESSION).len(), 0);
}

// --- fragmentation ---------------------------------------------------------------------

#[test]
fn capacities() {
    // 55 data bytes in the first fragment, 59 in the following ones
    for (size, count) in [(0, 1), (55, 1), (56, 2), (55 + 59, 2), (55 + 59 + 1, 3)] {
        assert_eq!(
            p::message(p::PERFORM_REALTIME_UPDATES, &vec![0; size], 0, p::SESSION)
                .unwrap()
                .len(),
            count,
            "{size} bytes"
        );
    }
    assert!(
        p::message(
            p::PERFORM_REALTIME_UPDATES,
            &vec![0; p::MAX_PAYLOAD],
            0,
            p::SESSION
        )
        .is_ok()
    );
    assert!(
        p::message(
            p::PERFORM_REALTIME_UPDATES,
            &vec![0; p::MAX_PAYLOAD + 1],
            0,
            p::SESSION
        )
        .is_err()
    );
}

#[test]
fn index_and_continuation() {
    let packets = p::message(p::PERFORM_REALTIME_UPDATES, &[0; 200], 0x42, 0x03).unwrap();
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
    let packets = p::message(p::PERFORM_REALTIME_UPDATES, &payload, 7, p::SESSION).unwrap();
    let msg = p::reassemble(&packets).unwrap();
    assert_eq!(
        (msg.request_id, msg.session, msg.cmd, msg.data),
        (7, p::SESSION, p::PERFORM_REALTIME_UPDATES, payload)
    );
}

#[test]
fn reassembly_rejects_incomplete_or_shuffled() {
    let packets = p::message(p::PERFORM_REALTIME_UPDATES, &[0; 200], 0, p::SESSION).unwrap();
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
            p::device_info_of(&raw),
            p::serial_of(&raw),
            p::lighting_mode_of(&raw),
            p::fn_function(&raw),
        );
        let _ = p::parse_leds(&[raw]);
    }
}

// --- IO Center Web connecting (capture made for QuietLinux, connect-web.hex) --------

/// Messages with the direction from the fixture comment (`out` or `in`).
fn connect_web() -> Vec<(String, Packet)> {
    let path = format!(
        "{}/tests/fixtures/connect-web.hex",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (hex, comment) = l.split_once('#').unwrap();
            let dir = comment.split_whitespace().next().unwrap().to_owned();
            (dir, parse_packet(hex.trim()))
        })
        .collect()
}

fn reply_data(cmd: Command) -> Vec<u8> {
    let (_, raw) = connect_web()
        .into_iter()
        .find(|(dir, raw)| dir == "in" && p::command_of(raw) == cmd)
        .unwrap_or_else(|| panic!("no reply to {cmd}"));
    p::data(&raw).to_vec()
}

#[test]
fn connect_web_requests_are_reproduced() {
    let messages = connect_web();
    assert!(messages.len() > 20);
    for (dir, raw) in &messages {
        assert!(p::crc_ok(raw), "{raw:02x?}");
        assert_eq!(p::status_of(raw), p::STATUS_SUCCESS);
        if dir == "out" {
            let (cmd, data, id, session) = fields(raw);
            assert_eq!(p::frame(cmd, &data, id, session).unwrap(), *raw, "{cmd}");
        }
    }
}

#[test]
fn connect_web_open_session() {
    let (_, request) = connect_web()
        .into_iter()
        .find(|(dir, raw)| dir == "out" && p::command_of(raw) == p::OPEN_SESSION)
        .unwrap();
    assert_eq!(p::open_session(0xfd68, 7), request);
    let info = p::parse_open_session(&reply_data(p::OPEN_SESSION)).unwrap();
    assert_eq!(
        info,
        p::SessionInfo {
            connection_id: 0xfd68,
            session: 2,
            state: 0,
            timeout: 3
        }
    );
    // every request after the session was opened uses its session byte
    let after: Vec<u8> = connect_web()
        .iter()
        .filter(|(dir, raw)| dir == "out" && p::request_id_of(raw) > 7)
        .map(|(_, raw)| raw[2])
        .collect();
    assert!(after.iter().all(|&s| s == 2), "{after:?}");
}

#[test]
fn connect_web_replies_decode() {
    use quietlinux_core::info;

    assert_eq!(
        info::parse_qlink_version(&reply_data(p::GET_QLINK_VERSION)).unwrap(),
        "0.0.23"
    );
    let features = info::parse_features(&reply_data(p::GET_SUPPORTED_FEATURES)).unwrap();
    assert_eq!(
        features.iter().map(|f| f.0).collect::<Vec<_>>(),
        [0x01, 0x02, 0x03, 0x04, 0x06, 0x07, 0x10, 0x11, 0x12]
    );
    assert!(features.iter().all(|f| f.1 == 1));
    let layout = info::Layout::parse(&reply_data(p::GET_LAYOUT)).unwrap();
    assert_eq!(
        (layout.physical_name(), layout.visual_name()),
        (Some("ISO"), Some("German"))
    );
    assert_eq!(reply_data(p::GET_POLLING_RATE), [0xe8, 0x03]); // 1000 Hz
    assert_eq!(reply_data(p::GET_LIGHTING_MODE), [p::MODE_GENERAL]);
    assert_eq!(reply_data(p::GET_GAME_MODE_CONFIG), [0x1f]);
    assert_eq!(reply_data(p::GET_GAME_MODE_STATE), [0x00]);
    assert_eq!(reply_data(p::GET_BINDINGS_CONFIG), [0x01]);
    assert_eq!(reply_data(p::GET_LAYER_CONFIG), [0x00]); // IO Center Web gets no layer config either
    let (total, bindings) = info::parse_bindings(&reply_data(p::GET_BINDINGS)).unwrap();
    let shown: Vec<String> = bindings
        .iter()
        .map(|b| format!("{}: {}", b.key_name(), b.action))
        .collect();
    assert_eq!(total, 5);
    assert_eq!(
        shown,
        [
            "M1: media: mic mute",
            "M2: key F13",
            "M3: media: next track",
            "M4: media: play/pause",
            "M5: macro 0 (action 2)"
        ]
    );
}
