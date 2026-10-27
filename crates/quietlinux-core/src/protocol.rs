//! Vendor protocol of the be quiet! Light Mount (interface 2, 64-byte packets). No I/O.
//!
//! Ported from `lightmount/protocol.py` of
//! [bbossa/keyboard_bequiet_lightmount_rgb](https://github.com/bbossa/keyboard_bequiet_lightmount_rgb)
//! (GPL-2.0-or-later). Field meanings come from captures of the vendor software;
//! see `docs/protocol.md`.
//!
//! ```text
//! Packet:  [len][idx][session][body ...][CRC-16/MODBUS of bytes 0..62, little endian, at 62..64]
//!   len     index of the last meaningful byte (= 2 + body length)
//!   idx     fragment number (bits 0..6), bit 7 set when more fragments follow
//! The body of the first fragment starts with [00][seq][cmd][sub]: seq fits in one byte.
//! ```

use std::fmt;

use crate::Rgb;

pub const PACKET_SIZE: usize = 64;

/// One HID report of the vendor interface, without report ID.
pub type Packet = [u8; PACKET_SIZE];

/// Session byte. IO Center gets it from `01:01`; `01` is accepted without opening a session.
pub const SESSION: u8 = 0x01;
/// Session byte used before a session is opened (identification requests).
pub const SESSION_NONE: u8 = 0x00;

/// Data bytes in the first fragment (after the 4-byte header).
pub const FIRST_CAPACITY: usize = 55;
/// Data bytes in each following fragment.
pub const NEXT_CAPACITY: usize = 59;
const BODY_START: usize = 3;
const BODY_END: usize = 62;
const MORE_FRAGMENTS: u8 = 0x80;
const MAX_FRAGMENTS: usize = 0x80;
/// Largest payload a message can carry (128 fragments).
pub const MAX_PAYLOAD: usize = FIRST_CAPACITY + (MAX_FRAGMENTS - 1) * NEXT_CAPACITY;

/// A command, written `group:id` in hex like the captures (`10:0d`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Command {
    pub group: u8,
    pub id: u8,
}

impl Command {
    pub const fn new(group: u8, id: u8) -> Self {
        Self { group, id }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02x}:{:02x}", self.group, self.id)
    }
}

pub const GET_VERSION: Command = Command::new(0x03, 0x01);
pub const GET_SERIAL: Command = Command::new(0x03, 0x02);
/// Read the lighting mode.
pub const GET_MODE: Command = Command::new(0x10, 0x01);
/// Set the lighting mode. The keyboard sends no reply with the same seq.
pub const SET_MODE: Command = Command::new(0x10, 0x02);
/// Lighting effect, stored in the keyboard.
pub const SET_LIGHTING: Command = Command::new(0x10, 0x06);
/// Per-LED colors, volatile.
pub const SET_LEDS: Command = Command::new(0x10, 0x0d);
/// Sent by the keyboard on its own, one per key press.
pub const KEY_EVENT: Command = Command::new(0x11, 0x02);
/// Open a session (IO Center); format not documented yet.
pub const OPEN_SESSION: Command = Command::new(0x01, 0x01);

/// Short name of a known command, for logs and capture analysis.
pub fn command_name(cmd: Command) -> Option<&'static str> {
    let name = match cmd {
        OPEN_SESSION => "open session",
        GET_VERSION => "version",
        GET_SERIAL => "serial",
        GET_MODE => "get mode",
        SET_MODE => "set mode",
        SET_LIGHTING => "effect",
        SET_LEDS => "LEDs",
        KEY_EVENT => "key event",
        _ => return None,
    };
    Some(name)
}

/// Lighting mode where `10:06` effects apply.
pub const MODE_EFFECTS: u8 = 0x01;
/// Lighting mode where `10:0d` per-LED colors apply.
pub const MODE_CUSTOM: u8 = 0x03;

/// Key event `data[1]` when Fn is held.
pub const FN_LAYER: u8 = 0x80;
/// Fn-layer function: lighting effect change (Fn+←/→).
pub const FN_EFFECT: u8 = 0x09;

pub const EFFECT_STATIC: u8 = 0x00;
pub const SPEED_DEFAULT: u8 = 0x32;
pub const COLOR_MODE_SINGLE: u8 = 0x00;

const ENTRY_TAG: u8 = 0x03;
const ENTRY_SIZE: usize = 6;
/// IO Center's split of a full per-LED update.
pub const ENTRIES_PER_MESSAGE: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    PayloadTooLarge { len: usize, max: usize },
    TooManyEntries { len: usize, max: usize },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge { len, max } => {
                write!(f, "payload of {len} bytes, at most {max} fit")
            }
            Self::TooManyEntries { len, max } => {
                write!(f, "{len} LED entries in one message, at most {max} fit")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

/// CRC-16/MODBUS (poly 0x8005 reflected, init 0xffff).
pub fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xffff;
    for &byte in data {
        crc ^= u16::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xa001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

fn packet(idx: u8, session: u8, body: &[u8]) -> Packet {
    debug_assert!(body.len() <= BODY_END - BODY_START);
    let mut raw = [0u8; PACKET_SIZE];
    // body.len() <= 59, so the length always fits in a byte.
    #[allow(clippy::cast_possible_truncation)]
    {
        raw[0] = 2 + body.len() as u8;
    }
    raw[1] = idx;
    raw[2] = session;
    raw[BODY_START..BODY_START + body.len()].copy_from_slice(body);
    let crc = crc16_modbus(&raw[..BODY_END]);
    raw[BODY_END..].copy_from_slice(&crc.to_le_bytes());
    raw
}

/// Split a message into 64-byte fragments.
pub fn message(
    cmd: Command,
    payload: &[u8],
    seq: u8,
    session: u8,
) -> Result<Vec<Packet>, ProtocolError> {
    if payload.len() > MAX_PAYLOAD {
        return Err(ProtocolError::PayloadTooLarge {
            len: payload.len(),
            max: MAX_PAYLOAD,
        });
    }
    let (first, mut rest) = payload.split_at(payload.len().min(FIRST_CAPACITY));
    let mut chunks = vec![first];
    while !rest.is_empty() {
        let (chunk, tail) = rest.split_at(rest.len().min(NEXT_CAPACITY));
        chunks.push(chunk);
        rest = tail;
    }
    let count = chunks.len();
    let packets = chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            // i < MAX_FRAGMENTS (128), checked above through MAX_PAYLOAD.
            #[allow(clippy::cast_possible_truncation)]
            let idx = i as u8 | if i + 1 < count { MORE_FRAGMENTS } else { 0 };
            if i == 0 {
                let mut body = vec![0x00, seq, cmd.group, cmd.id];
                body.extend_from_slice(chunk);
                packet(idx, session, &body)
            } else {
                packet(idx, session, chunk)
            }
        })
        .collect();
    Ok(packets)
}

/// Single-packet message.
pub fn frame(cmd: Command, payload: &[u8], seq: u8, session: u8) -> Result<Packet, ProtocolError> {
    if payload.len() > FIRST_CAPACITY {
        return Err(ProtocolError::PayloadTooLarge {
            len: payload.len(),
            max: FIRST_CAPACITY,
        });
    }
    Ok(message(cmd, payload, seq, session)?[0])
}

/// Frame whose payload is a short constant: cannot fail.
fn short_frame(cmd: Command, payload: &[u8], seq: u8, session: u8) -> Packet {
    frame(cmd, payload, seq, session).expect("short payload fits in one packet")
}

pub fn crc_ok(raw: &Packet) -> bool {
    u16::from_le_bytes([raw[BODY_END], raw[BODY_END + 1]]) == crc16_modbus(&raw[..BODY_END])
}

/// Reply with the same seq and command as `request`.
pub fn is_reply(resp: &Packet, request: &Packet) -> bool {
    resp[3..7] == request[3..7]
}

/// Fragment index (bits 0..6 of byte 1).
pub fn fragment_index(raw: &Packet) -> u8 {
    raw[1] & !MORE_FRAGMENTS
}

/// True when more fragments follow this one.
pub fn has_more(raw: &Packet) -> bool {
    raw[1] & MORE_FRAGMENTS != 0
}

/// Meaningful body bytes of a packet. The length byte comes from the keyboard,
/// so it is clamped instead of trusted.
fn body(raw: &Packet) -> &[u8] {
    let end = (usize::from(raw[0]) + 1).clamp(BODY_START, BODY_END);
    &raw[BODY_START..end]
}

/// Command of a first fragment.
pub fn command_of(raw: &Packet) -> Command {
    Command::new(raw[5], raw[6])
}

/// Seq of a first fragment.
pub fn seq_of(raw: &Packet) -> u8 {
    raw[4]
}

/// Data of a single-packet message (after the 4-byte header).
pub fn data(raw: &Packet) -> &[u8] {
    body(raw).get(4..).unwrap_or(&[])
}

/// A message rebuilt from its fragments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub seq: u8,
    pub session: u8,
    pub cmd: Command,
    pub data: Vec<u8>,
}

/// Rebuild a message from its fragments, or `None` if they are not one complete message.
pub fn reassemble(packets: &[Packet]) -> Option<Message> {
    let (first, _) = packets.split_first()?;
    let last = packets.len() - 1;
    for (i, raw) in packets.iter().enumerate() {
        if usize::from(fragment_index(raw)) != i || has_more(raw) != (i < last) {
            return None;
        }
    }
    let mut data = data(first).to_vec();
    for raw in &packets[1..] {
        data.extend_from_slice(body(raw));
    }
    Some(Message {
        seq: seq_of(first),
        session: first[2],
        cmd: command_of(first),
        data,
    })
}

// --- 03:01 / 03:02: identification ------------------------------------------
// IO Center sends both before opening a session (01:01), with session byte 00.

pub fn get_version(seq: u8) -> Packet {
    short_frame(GET_VERSION, &[], seq, SESSION_NONE)
}

/// `03:01` reply data: 8 bytes, identical in every capture (firmware version, presumably).
pub fn version_of(resp: &Packet) -> &[u8] {
    data(resp)
}

pub fn get_serial(seq: u8) -> Packet {
    short_frame(GET_SERIAL, &[], seq, SESSION_NONE)
}

/// `03:02` reply data: `[length]` then ASCII, specific to each keyboard (serial number, presumably).
pub fn serial_of(resp: &Packet) -> String {
    let data = data(resp);
    let Some((&count, text)) = data.split_first() else {
        return String::new();
    };
    let text = &text[..text.len().min(usize::from(count))];
    String::from_utf8_lossy(text).into_owned()
}

// --- 10:01 / 10:02: lighting mode -------------------------------------------

pub fn get_mode(seq: u8, session: u8) -> Packet {
    short_frame(GET_MODE, &[], seq, session)
}

pub fn mode_of(resp: &Packet) -> Option<u8> {
    data(resp).first().copied()
}

pub fn set_mode(mode: u8, seq: u8, session: u8) -> Packet {
    short_frame(SET_MODE, &[mode], seq, session)
}

// --- 11:02: key events -------------------------------------------------------
//   plain key: [vendor key index][00][01][01][00][HID usage]
//   Fn layer:  [vendor key index][80][01][function]...   (Fn alone: function 1b)

/// Function code of an Fn-layer key event, or `None` for anything else (plain keys included).
pub fn fn_function(raw: &Packet) -> Option<u8> {
    if fragment_index(raw) != 0 || command_of(raw) != KEY_EVENT || raw[0] < 10 {
        return None;
    }
    (raw[8] == FN_LAYER).then_some(raw[10])
}

// --- 10:06: lighting effect --------------------------------------------------
//   [00][effect][00][brightness][speed][color mode][R][G][B]

/// Static color effect. `brightness` is 0..=100 in IO Center (captured: 10, 50, 100).
pub fn static_color(rgb: Rgb, brightness: u8, seq: u8, session: u8) -> Packet {
    let payload = [
        0x00,
        EFFECT_STATIC,
        0x00,
        brightness,
        SPEED_DEFAULT,
        COLOR_MODE_SINGLE,
        rgb.r,
        rgb.g,
        rgb.b,
    ];
    short_frame(SET_LIGHTING, &payload, seq, session)
}

// --- 10:0d: per-LED colors ---------------------------------------------------
//   [N] N × [03][index][00][R][G][B] [00 if last message]
//   N = number of entries, +1 for the last message of an update

/// One LED and its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Led {
    pub index: u8,
    pub color: Rgb,
}

impl Led {
    pub const fn new(index: u8, color: Rgb) -> Self {
        Self { index, color }
    }
}

pub fn leds_payload(entries: &[Led], last: bool) -> Result<Vec<u8>, ProtocolError> {
    let max = usize::from(u8::MAX) - 1;
    let count = u8::try_from(entries.len())
        .ok()
        .filter(|&n| usize::from(n) <= max)
        .ok_or(ProtocolError::TooManyEntries {
            len: entries.len(),
            max,
        })?;
    let mut payload = Vec::with_capacity(2 + entries.len() * ENTRY_SIZE);
    payload.push(if last { count + 1 } else { count });
    for led in entries {
        payload.extend_from_slice(&[
            ENTRY_TAG,
            led.index,
            0x00,
            led.color.r,
            led.color.g,
            led.color.b,
        ]);
    }
    if last {
        payload.push(0x00);
    }
    Ok(payload)
}

/// Full update: messages of 40 entries, the last one marked, seq incremented per message.
pub fn update(entries: &[Led], seq: u8, session: u8) -> Vec<Vec<Packet>> {
    let batches: Vec<&[Led]> = entries.chunks(ENTRIES_PER_MESSAGE).collect();
    let count = batches.len();
    batches
        .into_iter()
        .zip(0u8..)
        .map(|(batch, i)| {
            let last = usize::from(i) + 1 == count;
            let payload = leds_payload(batch, last).expect("40 entries fit in one message");
            message(SET_LEDS, &payload, seq.wrapping_add(i), session)
                .expect("40 entries fit in one message")
        })
        .collect()
}

/// A decoded `10:0d` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedMessage {
    pub seq: u8,
    pub session: u8,
    pub entries: Vec<Led>,
    pub last: bool,
}

/// Decode a `10:0d` message from its fragments.
pub fn parse_leds(packets: &[Packet]) -> Option<LedMessage> {
    let msg = reassemble(packets)?;
    if msg.cmd != SET_LEDS {
        return None;
    }
    let (&n, rest) = msg.data.split_first()?;
    let entries: Vec<Led> = rest
        .chunks_exact(ENTRY_SIZE)
        .take_while(|e| e[0] == ENTRY_TAG)
        .map(|e| Led::new(e[1], Rgb::new(e[3], e[4], e[5])))
        .collect();
    let last = usize::from(n) == entries.len() + 1;
    Some(LedMessage {
        seq: msg.seq,
        session: msg.session,
        entries,
        last,
    })
}
