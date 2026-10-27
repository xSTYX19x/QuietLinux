//! Vendor protocol of the be quiet! Light Mount (interface 2, 64-byte packets). No I/O.
//!
//! Ported from `lightmount/protocol.py` of
//! [bbossa/keyboard_bequiet_lightmount_rgb](https://github.com/bbossa/keyboard_bequiet_lightmount_rgb)
//! (GPL-2.0-or-later). Field and command names follow IO Center Web, which calls
//! the protocol "QLink"; see `docs/protocol.md` for sources.
//!
//! ```text
//! Packet:  [len][seq][session][body ...][CRC-16/MODBUS of bytes 0..62, little endian, at 62..64]
//!   len     index of the last meaningful byte (= 2 + body length)
//!   seq     fragment number (bits 0..6), bit 7 set when more fragments follow
//! The body of the first fragment starts with [status][request id][feature][command].
//!   status      0 in requests; an error code (Status) in replies
//!   request id  1..=255, echoed by the reply; 0 marks a notification from the keyboard
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

/// A command: feature and command ID, written `feature:id` in hex like the captures (`10:0d`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Command {
    pub feature: u8,
    pub id: u8,
}

impl Command {
    pub const fn new(feature: u8, id: u8) -> Self {
        Self { feature, id }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02x}:{:02x}", self.feature, self.id)
    }
}

// Features (IO Center Web). Firmware updates (DFU) are out of scope: no command
// of that feature is ever built here.
pub const FEATURE_ROOT: u8 = 0x01;
pub const FEATURE_DFU: u8 = 0x02;
pub const FEATURE_DEVICE_INFO: u8 = 0x03;
pub const FEATURE_STORAGE: u8 = 0x04;
pub const FEATURE_USB_DEVICE: u8 = 0x06;
pub const FEATURE_KEYBOARD: u8 = 0x07;
pub const FEATURE_LIGHTINGS: u8 = 0x10;
pub const FEATURE_BINDINGS: u8 = 0x11;
pub const FEATURE_MACROS: u8 = 0x12;
pub const FEATURE_CONTROLS: u8 = 0x31;

/// Open a session; format documented, not yet captured.
pub const OPEN_SESSION: Command = Command::new(FEATURE_ROOT, 0x01);
/// Model ID, revision and firmware version (bbossa: "version").
pub const GET_DEVICE_INFO: Command = Command::new(FEATURE_DEVICE_INFO, 0x01);
pub const GET_SERIAL_NUMBER: Command = Command::new(FEATURE_DEVICE_INFO, 0x02);
pub const GET_LIGHTING_MODE: Command = Command::new(FEATURE_LIGHTINGS, 0x01);
/// Set the lighting mode. The keyboard sends no reply with the same request ID.
pub const SET_LIGHTING_MODE: Command = Command::new(FEATURE_LIGHTINGS, 0x02);
/// Effect of a lighting layer, stored in the keyboard (bbossa: "effect").
pub const SET_LAYER_CONFIG: Command = Command::new(FEATURE_LIGHTINGS, 0x06);
/// Per-LED colors in realtime mode, volatile (bbossa: "LEDs").
pub const PERFORM_REALTIME_UPDATES: Command = Command::new(FEATURE_LIGHTINGS, 0x0d);
/// Notification: a key was pressed (bbossa: "key event"). Same code as `SET_BINDING`.
pub const ACTION_FIRED: Command = Command::new(FEATURE_BINDINGS, 0x02);
/// Request: bind an action to a key. Same code as the `ACTION_FIRED` notification.
pub const SET_BINDING: Command = Command::new(FEATURE_BINDINGS, 0x02);
/// Game mode flags (keys game mode blocks).
pub const SET_GAME_MODE_CONFIG: Command = Command::new(FEATURE_KEYBOARD, 0x03);
/// Keeps a session open; IO Center Web sends it every 1.5 s (timeout 3 s).
pub const KEEP_ALIVE: Command = Command::new(FEATURE_ROOT, 0x03);

/// Read-only requests (IO Center Web layouts, see `docs/protocol.md`). Only these
/// can be sent with [`crate::device::Session::query`]: none of them changes the keyboard.
pub const READ_ONLY: [Command; 17] = [
    Command::new(FEATURE_ROOT, 0x04),       // GetSupportedFeatures
    Command::new(FEATURE_ROOT, 0x05),       // GetQLinkVersion
    Command::new(FEATURE_ROOT, 0x06),       // GetActiveSessionInfo
    GET_DEVICE_INFO,                        // GetDeviceInfo
    Command::new(FEATURE_USB_DEVICE, 0x01), // GetPollingRate
    Command::new(FEATURE_KEYBOARD, 0x01),   // GetLayout
    Command::new(FEATURE_KEYBOARD, 0x02),   // GetConfig (game mode flags)
    Command::new(FEATURE_KEYBOARD, 0x04),   // GetState (game mode on/off)
    Command::new(FEATURE_KEYBOARD, 0x06),   // GetSnapTapConfig
    GET_LIGHTING_MODE,                      // GetLightingMode
    Command::new(FEATURE_LIGHTINGS, 0x03),  // GetLayersLayout
    Command::new(FEATURE_LIGHTINGS, 0x05),  // GetLayerConfig
    Command::new(FEATURE_LIGHTINGS, 0x0e),  // GetGlobalLayers
    Command::new(FEATURE_LIGHTINGS, 0x0f),  // GetCalibration
    Command::new(FEATURE_BINDINGS, 0x01),   // GetBindings
    Command::new(FEATURE_BINDINGS, 0x04),   // GetConfig (bindings enabled)
    Command::new(FEATURE_MACROS, 0x01),     // GetMacros
];

pub const GET_SUPPORTED_FEATURES: Command = READ_ONLY[0];
pub const GET_QLINK_VERSION: Command = READ_ONLY[1];
pub const GET_POLLING_RATE: Command = READ_ONLY[4];
pub const GET_LAYOUT: Command = READ_ONLY[5];
pub const GET_GAME_MODE_CONFIG: Command = READ_ONLY[6];
pub const GET_GAME_MODE_STATE: Command = READ_ONLY[7];
pub const GET_SNAP_TAP_CONFIG: Command = READ_ONLY[8];
pub const GET_LAYER_CONFIG: Command = READ_ONLY[11];
pub const GET_CALIBRATION: Command = READ_ONLY[13];
pub const GET_BINDINGS: Command = READ_ONLY[14];
pub const GET_BINDINGS_CONFIG: Command = READ_ONLY[15];
pub const GET_LAYERS_LAYOUT: Command = READ_ONLY[10];
pub const GET_GLOBAL_LAYERS: Command = READ_ONLY[12];
pub const GET_MACROS: Command = READ_ONLY[16];

pub fn is_read_only(cmd: Command) -> bool {
    READ_ONLY.contains(&cmd)
}

/// Name of a command (or of a notification: `notification` is true for request ID 0),
/// as IO Center Web calls it. `None` for unknown ones.
// One arm per (feature, command): names repeat across features.
#[allow(clippy::match_same_arms)]
pub fn command_name(cmd: Command, notification: bool) -> Option<&'static str> {
    let name = match (cmd.feature, cmd.id, notification) {
        (FEATURE_ROOT, 0x01, false) => "OpenSession",
        (FEATURE_ROOT, 0x02, false) => "CloseSession",
        (FEATURE_ROOT, 0x03, false) => "KeepAlive",
        (FEATURE_ROOT, 0x04, false) => "GetSupportedFeatures",
        (FEATURE_ROOT, 0x05, false) => "GetQLinkVersion",
        (FEATURE_ROOT, 0x06, false) => "GetActiveSessionInfo",
        (FEATURE_ROOT, 0x07, false) => "RequestStateChange",
        (FEATURE_ROOT, 0x08, false) => "SendStateChangeDecision",
        (FEATURE_ROOT, 0x01, true) => "SessionStateChanged",
        (FEATURE_ROOT, 0x02, true) => "ActiveSessionChanged",
        (FEATURE_ROOT, 0x03, true) => "StateChangeRequested",
        (FEATURE_ROOT, 0x04, true) => "StateChangeDecision",
        (FEATURE_DFU, _, _) => "DFU (firmware update, never used)",
        (FEATURE_DEVICE_INFO, 0x01, false) => "GetDeviceInfo",
        (FEATURE_DEVICE_INFO, 0x02, false) => "GetSerialNumber",
        (FEATURE_DEVICE_INFO, 0x05, false) => "FactoryReset",
        (FEATURE_DEVICE_INFO, 0x01, true) => "DeviceInfoChanged",
        (FEATURE_STORAGE, 0x01, false) => "GetStorageInfo",
        (FEATURE_STORAGE, 0x02, false) => "WriteUserData",
        (FEATURE_STORAGE, 0x03, false) => "ReadUserData",
        (FEATURE_USB_DEVICE, 0x01, false) => "GetPollingRate",
        (FEATURE_USB_DEVICE, 0x02, false) => "SetPollingRate",
        (FEATURE_KEYBOARD, 0x01, false) => "GetLayout",
        (FEATURE_KEYBOARD, 0x02, false) => "GetConfig",
        (FEATURE_KEYBOARD, 0x03, false) => "SetConfig",
        (FEATURE_KEYBOARD, 0x04, false) => "GetState",
        (FEATURE_KEYBOARD, 0x05, false) => "SetState",
        (FEATURE_KEYBOARD, 0x06, false) => "GetSnapTapConfig",
        (FEATURE_KEYBOARD, 0x07, false) => "SetSnapTapConfig",
        (FEATURE_KEYBOARD, 0x01, true) => "ConfigChanged",
        (FEATURE_KEYBOARD, 0x02, true) => "StateChanged",
        (FEATURE_KEYBOARD, 0x03, true) => "SnapTapConfigChanged",
        (FEATURE_LIGHTINGS, 0x01, false) => "GetLightingMode",
        (FEATURE_LIGHTINGS, 0x02, false) => "SetLightingMode",
        (FEATURE_LIGHTINGS, 0x03, false) => "GetLayersLayout",
        (FEATURE_LIGHTINGS, 0x04, false) => "SetLayersLayout",
        (FEATURE_LIGHTINGS, 0x05, false) => "GetLayerConfig",
        (FEATURE_LIGHTINGS, 0x06, false) => "SetLayerConfig",
        (FEATURE_LIGHTINGS, 0x07, false) => "GetLayerMask",
        (FEATURE_LIGHTINGS, 0x08, false) => "SetLayerMask",
        (FEATURE_LIGHTINGS, 0x09, false) => "GetLayerName",
        (FEATURE_LIGHTINGS, 0x0a, false) => "SetLayerName",
        (FEATURE_LIGHTINGS, 0x0b, false) => "GetPlaybackState",
        (FEATURE_LIGHTINGS, 0x0c, false) => "SetPlaybackState",
        (FEATURE_LIGHTINGS, 0x0d, false) => "PerformRealtimeUpdates",
        (FEATURE_LIGHTINGS, 0x0e, false) => "GetGlobalLayers",
        (FEATURE_LIGHTINGS, 0x0f, false) => "GetCalibration",
        (FEATURE_LIGHTINGS, 0x10, false) => "SetCalibration",
        (FEATURE_BINDINGS, 0x01, false) => "GetBindings",
        (FEATURE_BINDINGS, 0x02, false) => "SetBinding",
        (FEATURE_BINDINGS, 0x03, false) => "ClearBinding",
        (FEATURE_BINDINGS, 0x04, false) => "GetConfig",
        (FEATURE_BINDINGS, 0x05, false) => "SetConfig",
        (FEATURE_BINDINGS, 0x06, false) => "GetBinding",
        (FEATURE_BINDINGS, 0x01, true) => "BindingChanged",
        (FEATURE_BINDINGS, 0x02, true) => "ActionFired",
        (FEATURE_BINDINGS, 0x03, true) => "ConfigChanged",
        (FEATURE_MACROS, 0x01, false) => "GetMacros",
        (FEATURE_MACROS, 0x02, false) => "SetMacros",
        (FEATURE_MACROS, 0x03, false) => "GetMacroConfig",
        (FEATURE_MACROS, 0x04, false) => "SetMacroConfig",
        (FEATURE_MACROS, 0x05, false) => "GetMacroEvents",
        (FEATURE_MACROS, 0x06, false) => "SetMacroEvents",
        (FEATURE_MACROS, 0x07, false) => "GetMacroName",
        (FEATURE_MACROS, 0x08, false) => "SetMacroName",
        (FEATURE_MACROS, 0x09, false) => "StartRecording",
        (FEATURE_MACROS, 0x0a, false) => "StopRecording",
        (FEATURE_MACROS, 0x0b, false) => "AbortPlayback",
        (FEATURE_MACROS, 0x05, true) => "InputEvent",
        _ => return None,
    };
    Some(name)
}

/// Name of a reply status code.
pub fn status_name(status: u8) -> Option<&'static str> {
    const NAMES: [&str; 14] = [
        "SUCCESS",
        "INVALID_SESSION_ID",
        "INVALID_FEATURE_ID",
        "INVALID_COMMAND_ID",
        "INVALID_REQUEST_ID",
        "INVALID_PARAMETER",
        "TIMEOUT",
        "BUSY",
        "NOT_AUTHORIZED",
        "NOT_ACTIVE",
        "INVALID_STATE",
        "INVALID_SIZE",
        "INSUFFICIENT_RESOURCES",
        "NOT_SUPPORTED",
    ];
    NAMES.get(usize::from(status)).copied()
}

/// Lighting mode: off.
pub const MODE_OFF: u8 = 0x00;
/// Lighting mode where the effects of the layers apply (bbossa: "effects").
pub const MODE_GENERAL: u8 = 0x01;
/// Lighting mode with stored per-key layers (not yet captured).
pub const MODE_CUSTOM: u8 = 0x02;
/// Lighting mode where `10:0d` per-LED colors apply, volatile (bbossa: "custom").
pub const MODE_REALTIME: u8 = 0x03;

/// Key event `data[1]` when Fn is held.
pub const FN_LAYER: u8 = 0x80;
/// Fn-layer function: lighting effect change (Fn+←/→).
pub const FN_EFFECT: u8 = 0x09;

pub const EFFECT_STATIC: u8 = 0x00;
pub const SPEED_DEFAULT: u8 = 0x32;
pub const COLOR_MODE_SINGLE: u8 = 0x00;

/// Status byte of a request, and of a successful reply.
pub const STATUS_SUCCESS: u8 = 0x00;

/// Realtime update operations (`10:0d`): `[03][index u16 LE][R][G][B]` and `[00]`.
const OP_SET_COLOR: u8 = 0x03;
const OP_COMMIT: u8 = 0x00;
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
                let mut body = vec![STATUS_SUCCESS, seq, cmd.feature, cmd.id];
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

/// Reply with the same request ID and command as `request`.
/// The status byte is not compared: error replies are replies too.
pub fn is_reply(resp: &Packet, request: &Packet) -> bool {
    fragment_index(resp) == 0 && resp[4..7] == request[4..7]
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

/// Request ID of a first fragment (0 for notifications).
pub fn request_id_of(raw: &Packet) -> u8 {
    raw[4]
}

/// Status byte of a first fragment: 0 in requests and successful replies.
pub fn status_of(raw: &Packet) -> u8 {
    raw[3]
}

/// True for a notification sent by the keyboard on its own (request ID 0).
pub fn is_notification(raw: &Packet) -> bool {
    fragment_index(raw) == 0 && request_id_of(raw) == 0
}

/// Data of a single-packet message (after the 4-byte header).
pub fn data(raw: &Packet) -> &[u8] {
    body(raw).get(4..).unwrap_or(&[])
}

impl Message {
    /// True for a notification sent by the keyboard on its own (request ID 0).
    pub fn is_notification(&self) -> bool {
        self.request_id == 0
    }

    /// Name of the command or notification, as IO Center Web calls it.
    pub fn name(&self) -> Option<&'static str> {
        command_name(self.cmd, self.is_notification())
    }
}

/// A message rebuilt from its fragments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub request_id: u8,
    pub session: u8,
    /// 0 in requests and successful replies.
    pub status: u8,
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
        request_id: request_id_of(first),
        session: first[2],
        status: status_of(first),
        cmd: command_of(first),
        data,
    })
}

// --- 01:01: OpenSession -----------------------------------------------------------

/// `01:01` as IO Center Web sends it: a random connection ID, no client type. [cap]
pub fn open_session(connection_id: u32, request_id: u8) -> Packet {
    short_frame(
        OPEN_SESSION,
        &connection_id.to_le_bytes(),
        request_id,
        SESSION_NONE,
    )
}

/// `01:01` reply: `[connection ID u32][session ID][state][timeout]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionInfo {
    pub connection_id: u32,
    /// Session byte for the following requests.
    pub session: u8,
    pub state: u8,
    pub timeout: u8,
}

pub fn parse_open_session(data: &[u8]) -> Option<SessionInfo> {
    let id = data.get(..4)?;
    Some(SessionInfo {
        connection_id: u32::from_le_bytes([id[0], id[1], id[2], id[3]]),
        session: *data.get(4)?,
        state: data.get(5).copied().unwrap_or_default(),
        timeout: data.get(6).copied().unwrap_or_default(),
    })
}

// --- 03:01 / 03:02: identification ------------------------------------------
// IO Center sends both before opening a session (01:01), with session byte 00.

pub fn get_device_info(seq: u8) -> Packet {
    short_frame(GET_DEVICE_INFO, &[], seq, SESSION_NONE)
}

/// `03:01` reply data: 8 bytes, identical in every capture (firmware version, presumably).
pub fn device_info_of(resp: &Packet) -> &[u8] {
    data(resp)
}

pub fn get_serial_number(seq: u8) -> Packet {
    short_frame(GET_SERIAL_NUMBER, &[], seq, SESSION_NONE)
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

pub fn get_lighting_mode(seq: u8, session: u8) -> Packet {
    short_frame(GET_LIGHTING_MODE, &[], seq, session)
}

pub fn lighting_mode_of(resp: &Packet) -> Option<u8> {
    data(resp).first().copied()
}

pub fn set_lighting_mode(mode: u8, seq: u8, session: u8) -> Packet {
    short_frame(SET_LIGHTING_MODE, &[mode], seq, session)
}

// --- 11:02: key events -------------------------------------------------------
//   plain key: [vendor key index][00][01][01][00][HID usage]
//   Fn layer:  [vendor key index][80][01][function]...   (Fn alone: function 1b)

/// Function code of an Fn-layer key event, or `None` for anything else (plain keys included).
pub fn fn_function(raw: &Packet) -> Option<u8> {
    if fragment_index(raw) != 0 || command_of(raw) != ACTION_FIRED || raw[0] < 10 {
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
    short_frame(SET_LAYER_CONFIG, &payload, seq, session)
}

// --- 10:06: SetLayerConfig (any effect) ------------------------------------------
//   [layer][effect][direction][brightness][speed][color mode][colors…]
//   Captured: static with one color, breathing with a 7-stop gradient. [cap]

/// Most gradient stops in one packet (6 header bytes + count + 4 per stop ≤ 55).
pub const MAX_GRADIENT_STOPS: usize = 12;

/// Payload of `10:06`.
pub fn layer_config_payload(
    layer: u8,
    effect: u8,
    direction: u8,
    brightness: u8,
    speed: u8,
    colors: &crate::info::Colors,
) -> Result<Vec<u8>, ProtocolError> {
    use crate::info::Colors;
    let mut payload = vec![layer, effect, direction, brightness, speed];
    let stops = |payload: &mut Vec<u8>, stops: &[(Rgb, u8)]| -> Result<(), ProtocolError> {
        let count = u8::try_from(stops.len())
            .ok()
            .filter(|&n| usize::from(n) <= MAX_GRADIENT_STOPS)
            .ok_or(ProtocolError::TooManyEntries {
                len: stops.len(),
                max: MAX_GRADIENT_STOPS,
            })?;
        payload.push(count);
        for (color, position) in stops {
            payload.extend_from_slice(&[color.r, color.g, color.b, *position]);
        }
        Ok(())
    };
    match colors {
        Colors::Single(c) => payload.extend_from_slice(&[COLOR_MODE_SINGLE, c.r, c.g, c.b]),
        Colors::Dual(a, b) => payload.extend_from_slice(&[0x01, a.r, a.g, a.b, b.r, b.g, b.b]),
        Colors::Gradient(s) => {
            payload.push(0x02);
            stops(&mut payload, s)?;
        }
        Colors::OrientedGradient {
            kind,
            angle,
            stops: s,
        } => {
            payload.extend_from_slice(&[0x03, *kind]);
            payload.extend_from_slice(&angle.to_le_bytes());
            stops(&mut payload, s)?;
        }
    }
    Ok(payload)
}

// --- 11:02 SetBinding, 07:03 game mode flags, 01:03 keep-alive ----------------------

/// Action type "media" of a binding.
pub const ACTION_MEDIA: u8 = 0x02;

/// `11:02` payload binding a media action to a key on the normal layer:
/// `[key ID][layer 0][02][action]`. [cap]
pub fn media_binding_payload(key_id: u8, action: u8) -> [u8; 4] {
    [key_id, 0x00, ACTION_MEDIA, action]
}

// --- 10:0d: PerformRealtimeUpdates (per-LED colors) ---------------------------
//   [N] then N operations: SetColor [03][index u16 LE][R][G][B], Commit [00]
//   IO Center commits once, at the end of the last message of an update, so
//   N = number of colors, +1 in the last message for the commit.

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
            OP_SET_COLOR,
            led.index,
            0x00,
            led.color.r,
            led.color.g,
            led.color.b,
        ]);
    }
    if last {
        payload.push(OP_COMMIT);
    }
    Ok(payload)
}

/// Full update: messages of 40 colors, the last one committed, request ID incremented per message.
pub fn update(entries: &[Led], request_id: u8, session: u8) -> Vec<Vec<Packet>> {
    let batches: Vec<&[Led]> = entries.chunks(ENTRIES_PER_MESSAGE).collect();
    let count = batches.len();
    let mut id = request_id;
    batches
        .into_iter()
        .enumerate()
        .map(|(i, batch)| {
            let payload =
                leds_payload(batch, i + 1 == count).expect("40 entries fit in one message");
            let msg = message(PERFORM_REALTIME_UPDATES, &payload, id, session)
                .expect("40 entries fit in one message");
            id = next_request_id(id);
            msg
        })
        .collect()
}

/// The request ID after `id`: 1..=255, skipping 0, which marks notifications.
pub fn next_request_id(id: u8) -> u8 {
    id.checked_add(1).unwrap_or(1)
}

/// A decoded `10:0d` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedMessage {
    pub request_id: u8,
    pub session: u8,
    pub entries: Vec<Led>,
    pub last: bool,
}

/// Decode a `10:0d` message from its fragments.
pub fn parse_leds(packets: &[Packet]) -> Option<LedMessage> {
    let msg = reassemble(packets)?;
    if msg.cmd != PERFORM_REALTIME_UPDATES {
        return None;
    }
    let (&n, rest) = msg.data.split_first()?;
    let entries: Vec<Led> = rest
        .chunks_exact(ENTRY_SIZE)
        .take_while(|e| e[0] == OP_SET_COLOR)
        .map(|e| Led::new(e[1], Rgb::new(e[3], e[4], e[5])))
        .collect();
    let last = usize::from(n) == entries.len() + 1;
    Some(LedMessage {
        request_id: msg.request_id,
        session: msg.session,
        entries,
        last,
    })
}
