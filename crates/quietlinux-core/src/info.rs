//! Decoding of replies to read-only requests and of key event notifications. No I/O.
//!
//! Layouts as documented in `docs/protocol.md` (mostly from IO Center Web, marked
//! [web] there). Everything comes from the keyboard, so every read is bounds-checked
//! and malformed data gives `None` instead of a panic.

use std::fmt;

use crate::Rgb;
use crate::keymap;
use crate::protocol::{self as p, MacroEvent};

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]))
}

fn rgb_at(data: &[u8], at: usize) -> Option<Rgb> {
    let c = data.get(at..at + 3)?;
    Some(Rgb::new(c[0], c[1], c[2]))
}

fn ascii(data: &[u8]) -> String {
    data.iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                char::from(b)
            } else {
                '?'
            }
        })
        .collect()
}

// --- 03:01 GetDeviceInfo ----------------------------------------------------------

/// A firmware version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub middle: u32,
    pub minor: u32,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.middle, self.minor)
    }
}

/// A version byte as IO Center reads it: its hex digits as a decimal number
/// (`0x17` is 17), or the plain value if a digit is above 9.
fn version_digits(b: u8) -> u32 {
    format!("{b:x}").parse().unwrap_or(u32::from(b))
}

/// `03:01` reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub model_id: u16,
    pub revision: u8,
    /// One version per microcontroller.
    pub firmware: Vec<Version>,
}

impl DeviceInfo {
    /// `[model u16][revision][MCU count]` then per MCU `[minor][middle hi][middle lo][major]`.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let model_id = u16_at(data, 0)?;
        let revision = *data.get(2)?;
        let count = usize::from(*data.get(3)?);
        let firmware = (0..count)
            .map(|i| {
                let v = data.get(4 + i * 4..8 + i * 4)?;
                let middle = format!("{}{}", version_digits(v[1]), version_digits(v[2]));
                Some(Version {
                    major: version_digits(v[3]),
                    middle: middle.parse().unwrap_or_default(),
                    minor: version_digits(v[0]),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            model_id,
            revision,
            firmware,
        })
    }

    pub fn model_name(&self) -> Option<&'static str> {
        match self.model_id {
            0x0001 => Some("Dark Mount"),
            0x0002 => Some("Light Mount"),
            _ => None,
        }
    }
}

// --- 01:05 GetQLinkVersion, 01:04 GetSupportedFeatures ------------------------------

/// `01:05` reply: `[patch u16][minor][major]`.
pub fn parse_qlink_version(data: &[u8]) -> Option<String> {
    Some(format!(
        "{}.{}.{}",
        data.get(3)?,
        data.get(2)?,
        u16_at(data, 0)?
    ))
}

/// `[count]` then `count` bytes: `10:03` layers layout, `10:0e` global layers.
pub fn parse_id_list(data: &[u8]) -> Option<Vec<u8>> {
    let count = usize::from(*data.first()?);
    data.get(1..1 + count).map(<[u8]>::to_vec)
}

/// A macro stored in the keyboard (`12:03`, `12:05`, `12:07`). [cap]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Macro {
    pub id: u16,
    pub name: String,
    /// `12:04` config: repeat mode, then 3 bytes kept as read (IO Center: `02 00 00`).
    pub config: [u8; 4],
    pub events: Vec<MacroEvent>,
}

impl Macro {
    /// A new macro with IO Center's default config (played once).
    pub fn new(id: u16, name: &str, events: Vec<MacroEvent>) -> Self {
        Self {
            id,
            name: name.to_owned(),
            config: p::MACRO_DEFAULT_CONFIG,
            events,
        }
    }

    pub fn repeat(&self) -> u8 {
        self.config[0]
    }

    pub fn set_repeat(&mut self, mode: u8) {
        self.config[0] = mode;
    }
}

/// Name of a repeat mode, like IO Center for Windows.
pub fn repeat_name(mode: u8) -> Option<&'static str> {
    Some(match mode {
        p::MACRO_ONCE => "once",
        p::MACRO_WHILE_HELD => "while held",
        p::MACRO_REPEAT => "repeat",
        _ => return None,
    })
}

/// Reply to `12:01` (the macro list): `[count u16]` then `count` macro IDs (u16). [cap]
pub fn parse_macro_ids(data: &[u8]) -> Option<Vec<u16>> {
    let count = usize::from(u16_at(data, 0)?);
    (0..count).map(|i| u16_at(data, 2 + i * 2)).collect()
}

/// `01:04` reply: `[count]` then `count × [feature][version]`.
pub fn parse_features(data: &[u8]) -> Option<Vec<(u8, u8)>> {
    let count = usize::from(*data.first()?);
    (0..count)
        .map(|i| Some((*data.get(1 + i * 2)?, *data.get(2 + i * 2)?)))
        .collect()
}

/// Name of a feature ID.
pub fn feature_name(feature: u8) -> Option<&'static str> {
    Some(match feature {
        0x01 => "root",
        0x02 => "firmware update (DFU)",
        0x03 => "device info",
        0x04 => "storage",
        0x05 => "hub",
        0x06 => "USB device",
        0x07 => "keyboard",
        0x08 => "mouse",
        0x09 => "battery",
        0x0a => "pairing",
        0x10 => "lighting",
        0x11 => "bindings",
        0x12 => "macros",
        0x20 => "numpad module",
        0x21 => "media dock module",
        0x30 => "sensors",
        0x31 => "controls",
        0x32 => "ARGB device",
        0x33 => "display unit",
        0x34 => "key-value storage",
        0x35 => "one-cord device",
        0x40 => "standalone display",
        _ => return None,
    })
}

// --- 07:01 GetLayout ---------------------------------------------------------------

/// `07:01` reply: physical and visual layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub physical: u8,
    pub visual: u8,
}

impl Layout {
    pub fn parse(data: &[u8]) -> Option<Self> {
        Some(Self {
            physical: *data.first()?,
            visual: *data.get(1)?,
        })
    }

    pub fn physical_name(self) -> Option<&'static str> {
        ["ANSI", "ISO", "JIS"]
            .get(usize::from(self.physical))
            .copied()
    }

    pub fn visual_name(self) -> Option<&'static str> {
        ["US", "German", "UK", "French", "Nordic"]
            .get(usize::from(self.visual))
            .copied()
    }

    /// Keycap legends QuietLinux has for this layout.
    pub fn legends(self) -> Option<keymap::Layout> {
        match self.visual {
            1 => Some(keymap::Layout::De),
            2 => Some(keymap::Layout::Uk),
            3 => Some(keymap::Layout::Fr),
            _ => None,
        }
    }
}

// --- 07:02 game mode flags, 07:06 quick tap ---------------------------------------

/// Shortcuts that game mode blocks (`07:02` reply byte).
pub fn game_mode_blocks(flags: u8) -> Vec<&'static str> {
    [
        (0x01, "Shift+Tab"),
        (0x02, "Alt+F4"),
        (0x04, "Win"),
        (0x08, "Alt+Tab"),
        (0x10, "Caps Lock"),
    ]
    .into_iter()
    .filter(|(bit, _)| flags & bit != 0)
    .map(|(_, name)| name)
    .collect()
}

/// A quick tap ("SnapTap") key pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapTapPair {
    pub first: u16,
    pub second: u16,
    pub priority: u8,
}

impl SnapTapPair {
    pub fn priority_name(self) -> Option<&'static str> {
        [
            "last input wins",
            "first key wins",
            "second key wins",
            "neutral",
        ]
        .get(usize::from(self.priority))
        .copied()
    }
}

/// `07:06` reply: `[count]` then `count × [key ID u16][key ID u16][priority]`.
pub fn parse_snap_tap(data: &[u8]) -> Option<Vec<SnapTapPair>> {
    let count = usize::from(*data.first()?);
    (0..count)
        .map(|i| {
            let at = 1 + i * 5;
            Some(SnapTapPair {
                first: u16_at(data, at)?,
                second: u16_at(data, at + 2)?,
                priority: *data.get(at + 4)?,
            })
        })
        .collect()
}

// --- 10:05 GetLayerConfig -----------------------------------------------------------

/// Colors of a lighting layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Colors {
    Single(Rgb),
    Dual(Rgb, Rgb),
    /// Color stops: color and position.
    Gradient(Vec<(Rgb, u8)>),
    OrientedGradient {
        kind: u8,
        angle: u16,
        stops: Vec<(Rgb, u8)>,
    },
}

impl fmt::Display for Colors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stops = |s: &[(Rgb, u8)]| {
            s.iter()
                .map(|(c, p)| format!("{c}@{p}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        match self {
            Colors::Single(c) => write!(f, "{c}"),
            Colors::Dual(a, b) => write!(f, "{a} and {b}"),
            Colors::Gradient(s) => write!(f, "gradient {}", stops(s)),
            Colors::OrientedGradient {
                kind,
                angle,
                stops: s,
            } => {
                let kind = ["linear", "radial", "conic"]
                    .get(usize::from(*kind))
                    .copied()
                    .unwrap_or("?");
                write!(f, "{kind} gradient at {angle}° {}", stops(s))
            }
        }
    }
}

/// `10:05` reply: `[effect][direction][brightness][speed][color mode][colors…][ripple…]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerConfig {
    pub effect: u8,
    pub direction: u8,
    pub brightness: u8,
    pub speed: u8,
    pub colors: Colors,
    /// Ripple only: width, range, fade-out.
    pub ripple: Option<[u8; 3]>,
}

pub const EFFECT_RIPPLE: u8 = 7;

impl LayerConfig {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let header = data.get(..5)?;
        let stops = |count_at: usize| -> Option<(Vec<(Rgb, u8)>, usize)> {
            let count = usize::from(*data.get(count_at)?);
            let stops = (0..count)
                .map(|i| {
                    let at = count_at + 1 + i * 4;
                    Some((rgb_at(data, at)?, *data.get(at + 3)?))
                })
                .collect::<Option<Vec<_>>>()?;
            Some((stops, count_at + 1 + count * 4))
        };
        let (colors, end) = match header[4] {
            0 => (Colors::Single(rgb_at(data, 5)?), 8),
            1 => (Colors::Dual(rgb_at(data, 5)?, rgb_at(data, 8)?), 11),
            2 => {
                let (s, end) = stops(5)?;
                (Colors::Gradient(s), end)
            }
            3 => {
                let (s, end) = stops(8)?;
                (
                    Colors::OrientedGradient {
                        kind: *data.get(5)?,
                        angle: u16_at(data, 6)?,
                        stops: s,
                    },
                    end,
                )
            }
            _ => return None,
        };
        let ripple = if header[0] == EFFECT_RIPPLE {
            data.get(end..end + 3).map(|r| [r[0], r[1], r[2]])
        } else {
            None
        };
        Some(Self {
            effect: header[0],
            direction: header[1],
            brightness: header[2],
            speed: header[3],
            colors,
            ripple,
        })
    }
}

/// `10:0f` reply: `[version][red f32][green f32][blue f32][flags: bit 0 = enabled]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    pub version: u8,
    /// Factors per channel, 1.0 = unchanged.
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub enabled: bool,
}

impl Calibration {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != 14 {
            return None;
        }
        let f =
            |at: usize| f32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        Some(Self {
            version: data[0],
            red: f(1),
            green: f(5),
            blue: f(9),
            enabled: data[13] & 1 == 1,
        })
    }
}

pub fn effect_name(effect: u8) -> Option<&'static str> {
    [
        "static",
        "color wave",
        "tornado",
        "breathing",
        "reactive",
        "matrix",
        "off",
        "ripple",
    ]
    .get(usize::from(effect))
    .copied()
}

pub fn direction_name(direction: u8) -> Option<&'static str> {
    [
        "up",
        "down",
        "left",
        "right",
        "clockwise",
        "counterclockwise",
        "omnidirectional",
        "horizontal",
        "vertical",
        "cross",
    ]
    .get(usize::from(direction))
    .copied()
}

// --- bindings: 11:01 GetBindings, 11:02 ActionFired ------------------------------------

/// What a key does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Disabled,
    StandardKey {
        modifiers: u8,
        usage: u8,
    },
    Media(u8),
    MediaSound(String),
    MouseButton {
        button: u8,
        flags: u8,
        auto_fire: u8,
    },
    MouseScroll(u8),
    OpenFolder(String),
    OpenFile(String),
    OpenWebsite(String),
    WindowsShortcut(u8),
    /// Profile cycle, next or previous (IO Center switches profiles on the host).
    Profile(u8),
    /// Select a profile: IO Center's ID of the profile (a UUID).
    ProfileSelect([u8; 16]),
    Backlight {
        action: u8,
        effect: Option<u8>,
    },
    /// Play a macro stored in the keyboard.
    Macro(u16),
    SpecialKey(u16),
    /// An action type this decoder does not know, with the bytes that follow.
    Unknown {
        kind: u8,
        data: Vec<u8>,
    },
}

/// Decode an action starting at its type byte. Returns the action and the bytes used,
/// or `None` if the data is too short. Unknown types use the rest of the data.
pub fn parse_action(data: &[u8]) -> Option<(Action, usize)> {
    let kind = *data.first()?;
    let arg = |i: usize| data.get(i).copied();
    let text = |len_at: usize| -> Option<(String, usize)> {
        let len = usize::from(arg(len_at)?);
        Some((
            ascii(data.get(len_at + 1..len_at + 1 + len)?),
            len_at + 1 + len,
        ))
    };
    Some(match kind {
        0x00 | 0x0c => (Action::Disabled, 1),
        0x01 => (
            Action::StandardKey {
                modifiers: arg(1)?,
                usage: arg(2)?,
            },
            3,
        ),
        0x02 if arg(1)? == 10 => {
            let (name, used) = text(2)?;
            (Action::MediaSound(name), used)
        }
        0x02 => (Action::Media(arg(1)?), 2),
        0x03 => match arg(1)? {
            1 => (
                Action::MouseButton {
                    button: arg(2)? & 0x1f,
                    flags: arg(2)? & 0xe0,
                    auto_fire: arg(3)?,
                },
                4,
            ),
            2 => (Action::MouseScroll(arg(2)?), 3),
            _ => return None,
        },
        0x04..=0x06 => {
            let (s, used) = text(1)?;
            let action = match kind {
                0x04 => Action::OpenFolder(s),
                0x05 => Action::OpenFile(s),
                _ => Action::OpenWebsite(s),
            };
            (action, used)
        }
        0x07 => (Action::WindowsShortcut(arg(1)?), 2),
        0x08 if arg(1)? == 1 => (Action::ProfileSelect(data.get(2..18)?.try_into().ok()?), 18),
        0x08 => (Action::Profile(arg(1)?), 2),
        0x09 if arg(1)? == 1 => (
            Action::Backlight {
                action: 1,
                effect: Some(arg(2)?),
            },
            3,
        ),
        0x09 => (
            Action::Backlight {
                action: arg(1)?,
                effect: None,
            },
            2,
        ),
        0x0a => (Action::Macro(u16_at(data, 1)?), 3),
        0x0b => (Action::SpecialKey(u16_at(data, 2)?), 4),
        _ => (
            Action::Unknown {
                kind,
                data: data[1..].to_vec(),
            },
            data.len(),
        ),
    })
}

impl Action {
    /// Bytes of a binding (`11:02`, after key ID and layer). Only the action types
    /// captured from IO Center are encoded; `None` for the others, which QuietLinux
    /// does not send yet.
    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        Some(match self {
            Action::Disabled => vec![0x00],
            Action::StandardKey { modifiers, usage } => vec![0x01, *modifiers, *usage],
            Action::Media(action) => vec![0x02, *action],
            Action::MouseButton {
                button,
                flags,
                auto_fire,
            } if *button < 0x20 && *flags == flags & 0xe0 => {
                vec![0x03, 0x01, button | flags, *auto_fire]
            }
            Action::Macro(id) => {
                let [lo, hi] = id.to_le_bytes();
                vec![0x0a, lo, hi]
            }
            Action::WindowsShortcut(action) => vec![0x07, *action],
            // [cap] `06 13 "https://example.org"`; only what fits one packet, as captured
            Action::OpenWebsite(url) if website_ok(url) => {
                let mut bytes = vec![0x06, u8::try_from(url.len()).ok()?];
                bytes.extend_from_slice(url.as_bytes());
                bytes
            }
            // next/previous effect, brighter, darker (selecting an effect is not captured)
            Action::Backlight {
                action: action @ 2..=5,
                effect: None,
            } => vec![0x09, *action],
            _ => return None,
        })
    }
}

/// Longest website address a binding takes: what fits one `11:02` packet with
/// key ID, layer, action type and length, like the captured binding.
pub const MAX_WEBSITE: usize = 51;

/// A website binding QuietLinux writes and opens: `http://` or `https://` and a
/// host, at most [`MAX_WEBSITE`] characters, only characters web addresses may
/// contain (RFC 3986; no spaces, quotes, backslashes or backticks).
pub fn website_ok(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    rest.is_some_and(|r| !r.is_empty() && !r.starts_with('/'))
        && url.len() <= MAX_WEBSITE
        && url
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'()*+,;=%".contains(c))
}

/// What [`website_ok`] accepts, for error messages.
pub const WEBSITE_RULES: &str =
    "http:// or https:// and a host, at most 51 characters, no spaces or quotes";

fn modifier_names(modifiers: u8) -> Vec<&'static str> {
    [
        "LCtrl", "LShift", "LAlt", "LWin", "RCtrl", "RShift", "RAlt", "RWin",
    ]
    .into_iter()
    .enumerate()
    .filter(|(bit, _)| modifiers & (1 << bit) != 0)
    .map(|(_, name)| name)
    .collect()
}

/// Name of a HID keyboard usage (common keys only).
pub fn usage_name(usage: u8) -> Option<String> {
    const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    Some(match usage {
        0x04..=0x1d => char::from(LETTERS[usize::from(usage - 0x04)]).to_string(),
        0x1e..=0x26 => (usage - 0x1d).to_string(),
        0x27 => "0".to_owned(),
        0x28 => "Enter".to_owned(),
        0x29 => "Esc".to_owned(),
        0x2a => "Backspace".to_owned(),
        0x2b => "Tab".to_owned(),
        0x2c => "Space".to_owned(),
        0x2d => "-".to_owned(),
        0x2e => "=".to_owned(),
        0x2f => "[".to_owned(),
        0x30 => "]".to_owned(),
        0x31 => "\\".to_owned(),
        0x32 => "ISO #".to_owned(),
        0x33 => ";".to_owned(),
        0x34 => "'".to_owned(),
        0x35 => "`".to_owned(),
        0x36 => ",".to_owned(),
        0x37 => ".".to_owned(),
        0x38 => "/".to_owned(),
        0x39 => "Caps Lock".to_owned(),
        0x3a..=0x45 => format!("F{}", usage - 0x39),
        0x4f => "→".to_owned(),
        0x50 => "←".to_owned(),
        0x51 => "↓".to_owned(),
        0x52 => "↑".to_owned(),
        0x64 => "ISO \\".to_owned(),
        0x68..=0x73 => format!("F{}", usage - 0x68 + 13),
        _ => return None,
    })
}

const MEDIA: [&str; 11] = [
    "none",
    "volume up",
    "volume down",
    "mute",
    "mic mute",
    "next track",
    "previous track",
    "play/pause",
    "stop",
    "volume mixer",
    "specific sound",
];
const MOUSE_BUTTONS: [&str; 5] = ["left", "right", "middle", "forward", "back"];
const SCROLL: [&str; 4] = ["up", "down", "left", "right"];
const WINDOWS_SHORTCUTS: [&str; 22] = [
    "none",
    "file explorer",
    "calculator",
    "task manager",
    "lock PC",
    "shut down",
    "sleep",
    "hibernate",
    "snipping tool",
    "notepad",
    "paint",
    "airplane mode",
    "Xbox Game Bar",
    "system settings",
    "refresh",
    "switch app",
    "close app",
    "copy",
    "paste",
    "cut",
    "web browser",
    "email",
];
const PROFILE: [&str; 5] = ["none", "select", "cycle", "next", "previous"];
const BACKLIGHT: [&str; 6] = [
    "none",
    "select effect",
    "next effect",
    "previous effect",
    "brighter",
    "darker",
];

fn named(table: &[&'static str], i: u8) -> String {
    table
        .get(usize::from(i))
        .map_or_else(|| format!("#{i}"), |s| (*s).to_owned())
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Disabled => f.write_str("disabled"),
            Action::StandardKey { modifiers, usage } => {
                let mut parts = modifier_names(*modifiers);
                let key = usage_name(*usage).unwrap_or_else(|| format!("usage {usage:#04x}"));
                if *usage != 0 || parts.is_empty() {
                    parts.push(&key);
                }
                write!(f, "key {}", parts.join("+"))
            }
            Action::Media(a) => write!(f, "media: {}", named(&MEDIA, *a)),
            Action::MediaSound(name) => write!(f, "media: sound {name:?}"),
            Action::MouseButton {
                button,
                flags,
                auto_fire,
            } => {
                write!(f, "mouse: {} button", named(&MOUSE_BUTTONS, *button))?;
                if flags & 0x80 != 0 {
                    f.write_str(", double click")?;
                }
                if flags & 0x40 != 0 {
                    f.write_str(", while pressed")?;
                }
                if *auto_fire != 0 {
                    write!(f, ", auto fire {auto_fire}")?;
                }
                Ok(())
            }
            Action::MouseScroll(d) => write!(f, "mouse: scroll {}", named(&SCROLL, *d)),
            Action::OpenFolder(p) => write!(f, "open folder {p:?}"),
            Action::OpenFile(p) => write!(f, "open file {p:?}"),
            Action::OpenWebsite(u) => write!(f, "open website {u:?}"),
            Action::WindowsShortcut(a) => {
                write!(f, "Windows shortcut: {}", named(&WINDOWS_SHORTCUTS, *a))
            }
            Action::Profile(a) => write!(f, "profile: {}", named(&PROFILE, *a)),
            Action::ProfileSelect(_) => write!(f, "profile: select"),
            Action::Backlight {
                action,
                effect: Some(e),
            } => write!(
                f,
                "lighting: {} {}",
                named(&BACKLIGHT, *action),
                effect_name(*e).map_or_else(|| format!("#{e}"), str::to_owned)
            ),
            Action::Backlight {
                action,
                effect: None,
            } => write!(f, "lighting: {}", named(&BACKLIGHT, *action)),
            Action::Macro(id) => write!(f, "macro {id}"),
            Action::SpecialKey(code) => write!(f, "special key {code:#06x}"),
            Action::Unknown { kind, data } => {
                write!(f, "action type {kind:#04x}")?;
                if !data.is_empty() {
                    write!(
                        f,
                        " ({})",
                        data.iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    )?;
                }
                Ok(())
            }
        }
    }
}

/// One key binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub key_id: u8,
    /// True for the Fn layer.
    pub fn_layer: bool,
    pub action: Action,
    /// The action bytes as the keyboard reported them (type and data).
    pub raw: Vec<u8>,
}

impl Binding {
    /// Name of the key (`KnobLeft` for the knob's turns), or its ID in hex.
    pub fn key_name(&self) -> String {
        keymap::code_for_key_id(self.key_id)
            .map_or_else(|| format!("key {:#04x}", self.key_id), str::to_owned)
    }
}

/// `11:01` reply: two count bytes, then entries `[key ID][flags: bit 7 = Fn][action…]`.
/// Returns the total count and the entries decoded; decoding stops at an action
/// type whose length is unknown.
pub fn parse_bindings(data: &[u8]) -> Option<(usize, Vec<Binding>)> {
    let total = usize::from(*data.first()?) + usize::from(*data.get(1)?);
    let mut entries = Vec::new();
    let mut at = 2;
    while at < data.len() && entries.len() < total {
        let key_id = data[at];
        let Some(&flags) = data.get(at + 1) else {
            break;
        };
        let Some((action, used)) = data.get(at + 2..).and_then(parse_action) else {
            break;
        };
        if matches!(action, Action::Unknown { .. }) {
            break;
        }
        entries.push(Binding {
            key_id,
            fn_layer: flags & 0x80 != 0,
            action,
            raw: data[at + 2..at + 2 + used].to_vec(),
        });
        at += 2 + used;
    }
    Some((total, entries))
}

/// A key press reported by an `11:02` ActionFired notification:
/// `[key ID][flags: bit 7 = Fn][state][action…]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub key_id: u8,
    pub fn_layer: bool,
    /// Third byte: 01 in every capture so far (pressed, presumably).
    pub state: u8,
    pub action: Option<Action>,
}

impl KeyEvent {
    pub fn parse(data: &[u8]) -> Option<Self> {
        Some(Self {
            key_id: *data.first()?,
            fn_layer: data.get(1)? & 0x80 != 0,
            state: *data.get(2)?,
            action: data.get(3..).and_then(parse_action).map(|(a, _)| a),
        })
    }

    pub fn key_name(&self) -> String {
        keymap::code_for_key_id(self.key_id)
            .map_or_else(|| format!("key {:#04x}", self.key_id), str::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_info_of_the_reference_keyboard() {
        let info = DeviceInfo::parse(&[0x02, 0x00, 0x01, 0x01, 0x00, 0x00, 0x03, 0x02]).unwrap();
        assert_eq!(
            (info.model_id, info.revision, info.model_name()),
            (2, 1, Some("Light Mount"))
        );
        assert_eq!(info.firmware.len(), 1);
        assert_eq!(info.firmware[0].to_string(), "2.3.0");
    }

    #[test]
    fn version_digits_like_io_center() {
        // 1.0.17 is sent as 17 00 00 01 (0x17 read as "17")
        let info = DeviceInfo::parse(&[0x02, 0, 1, 1, 0x17, 0x00, 0x00, 0x01]).unwrap();
        assert_eq!(info.firmware[0].to_string(), "1.0.17");
        assert_eq!(version_digits(0x1a), 26); // not decimal: plain value
    }

    #[test]
    fn features_layout_and_snap_tap() {
        assert_eq!(
            parse_features(&[2, 0x01, 1, 0x10, 2]).unwrap(),
            [(0x01, 1), (0x10, 2)]
        );
        assert_eq!(parse_features(&[2, 0x01]), None);
        let layout = Layout::parse(&[1, 1]).unwrap();
        assert_eq!(
            (
                layout.physical_name(),
                layout.visual_name(),
                layout.legends()
            ),
            (Some("ISO"), Some("German"), Some(keymap::Layout::De))
        );
        let pairs = parse_snap_tap(&[1, 0x1e, 0, 0x20, 0, 0]).unwrap();
        assert_eq!(
            pairs,
            [SnapTapPair {
                first: 0x1e,
                second: 0x20,
                priority: 0
            }]
        );
        assert_eq!(game_mode_blocks(0x05), ["Shift+Tab", "Win"]);
        assert_eq!(parse_qlink_version(&[3, 0, 2, 1]).unwrap(), "1.2.3");
    }

    #[test]
    fn layer_configs() {
        // the factory default of the Light Mount: static E63000, brightness 100, speed 50
        let config = LayerConfig::parse(&[0, 0, 100, 50, 0, 0xe6, 0x30, 0x00]).unwrap();
        assert_eq!(config.colors, Colors::Single(Rgb::new(0xe6, 0x30, 0)));
        assert_eq!(
            (effect_name(config.effect), config.brightness, config.speed),
            (Some("static"), 100, 50)
        );
        let gradient =
            LayerConfig::parse(&[1, 3, 100, 50, 2, 2, 255, 0, 0, 0, 0, 0, 255, 100]).unwrap();
        assert_eq!(gradient.colors.to_string(), "gradient ff0000@0 0000ff@100");
        let ripple = LayerConfig::parse(&[7, 6, 100, 50, 0, 1, 2, 3, 5, 7, 0]).unwrap();
        assert_eq!(ripple.ripple, Some([5, 7, 0]));
        assert_eq!(LayerConfig::parse(&[0, 0, 100, 50, 0, 0xe6]), None);
        assert_eq!(LayerConfig::parse(&[0, 0, 100, 50, 9, 0, 0, 0]), None);
    }

    #[test]
    fn calibration() {
        let mut data = vec![1];
        for v in [1.0f32, 0.59, 0.43] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.push(1);
        let c = Calibration::parse(&data).unwrap();
        assert!(c.enabled && (c.green - 0.59).abs() < 1e-6);
        assert_eq!(Calibration::parse(&data[..13]), None);
    }

    #[test]
    fn bindings() {
        // M1 play/pause, Fn+↑ brighter, knob right volume up, a disabled key
        let data = [
            4, 0, 0x02, 0x00, 0x02, 7, 0x57, 0x80, 0x09, 4, 0x6e, 0x00, 0x02, 1, 0x0d, 0x00, 0x00,
        ];
        let (total, entries) = parse_bindings(&data).unwrap();
        assert_eq!(total, 4);
        let shown: Vec<String> = entries
            .iter()
            .map(|b| {
                format!(
                    "{}{}: {}",
                    if b.fn_layer { "Fn+" } else { "" },
                    b.key_name(),
                    b.action
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                "M1: media: play/pause",
                "Fn+ArrowUp: lighting: brighter",
                "KnobRight: media: volume up",
                "KeyQ: disabled"
            ]
        );
    }

    #[test]
    fn actions() {
        let action = |d: &[u8]| parse_action(d).unwrap().0.to_string();
        assert_eq!(action(&[0x01, 0x05, 0x06]), "key LCtrl+LAlt+C");
        assert_eq!(action(&[0x01, 0x01, 0x00]), "key LCtrl"); // a modifier alone
        assert_eq!(action(&[0x01, 0x00, 0x68]), "key F13");
        assert_eq!(action(&[0x01, 0x00, 0x32]), "key ISO #");
        assert_eq!(action(&[0x01, 0x00, 0x64]), "key ISO \\");
        assert_eq!(
            action(&[0x03, 1, 0x41, 0]),
            "mouse: right button, while pressed"
        );
        assert_eq!(action(&[0x06, 3, b'a', b'.', b'b']), "open website \"a.b\"");
        assert_eq!(action(&[0x09, 1, 3]), "lighting: select effect breathing");
        assert_eq!(action(&[0x0a, 0x02, 0x00]), "macro 2");
        assert_eq!(parse_macro_ids(&[2, 0, 1, 0, 2, 0]), Some(vec![1, 2]));
        assert_eq!(parse_macro_ids(&[0, 0]), Some(vec![]));
        assert_eq!(parse_macro_ids(&[2, 0, 1, 0]), None);
        assert_eq!(action(&[0x08, 0x03]), "profile: next");
        let mut select = vec![0x08, 0x01];
        select.extend([0x11; 16]);
        assert_eq!(
            parse_action(&select).unwrap(),
            (Action::ProfileSelect([0x11; 16]), 18)
        );
        assert_eq!(parse_action(&select[..17]), None);
        assert_eq!(parse_action(&[0x06, 9, b'a']), None); // text longer than the data
        assert_eq!(parse_action(&[0x03, 7]), None); // unknown mouse action
    }

    #[test]
    fn actions_encode_like_io_center() {
        // captured SetBinding payloads after key ID and layer
        let captured: [&[u8]; 9] = [
            &[0x01, 0x00, 0x04],       // key A
            &[0x01, 0x01, 0x06],       // Ctrl+C
            &[0x01, 0x00, 0x68],       // F13
            &[0x02, 0x08],             // media stop
            &[0x03, 0x01, 0x02, 0x00], // middle mouse button
            &[0x07, 0x02],             // Windows shortcut: calculator
            &[0x09, 0x02],             // next effect
            &[0x00],                   // disabled
            &[0x0a, 0x03, 0x00],       // macro 3
        ];
        for bytes in captured {
            let (action, used) = parse_action(bytes).unwrap();
            assert_eq!(used, bytes.len());
            assert_eq!(action.to_bytes().as_deref(), Some(bytes), "{action}");
        }
        assert_eq!(Action::OpenWebsite("x".into()).to_bytes(), None);
        let site = Action::OpenWebsite("https://example.org".into());
        let mut captured = vec![0x06, 0x13];
        captured.extend_from_slice(b"https://example.org");
        assert_eq!(site.to_bytes(), Some(captured));
        let long = format!("https://example.org/{}", "a".repeat(40));
        assert_eq!(Action::OpenWebsite(long).to_bytes(), None);
        assert_eq!(
            Action::OpenWebsite("file:///etc/passwd".into()).to_bytes(),
            None
        );
        for bad in [
            "https://",
            "http:///x",
            "https://a b",
            "https://a\"b",
            "https://a`id`",
            "https://a\\b",
            "https://a<b>",
            "https://ä.de",
            "HTTPS://example.org",
        ] {
            assert!(!website_ok(bad), "{bad}");
        }
        for good in [
            "http://localhost:8080/",
            "https://example.org/a?b=c&d=%20#top",
            "https://[::1]/x",
        ] {
            assert!(website_ok(good), "{good}");
        }
        assert!(WEBSITE_RULES.contains(&MAX_WEBSITE.to_string()));
        assert_eq!(
            Action::Backlight {
                action: 1,
                effect: Some(3)
            }
            .to_bytes(),
            None
        );
    }

    #[test]
    fn key_events_captured_by_bbossa() {
        let fn_right = KeyEvent::parse(&[0x5c, 0x80, 0x01, 0x09, 0x03]).unwrap();
        assert_eq!(
            (fn_right.key_name().as_str(), fn_right.fn_layer),
            ("ArrowRight", true)
        );
        assert_eq!(
            fn_right.action,
            Some(Action::Backlight {
                action: 3,
                effect: None
            })
        );
        let fn_alone = KeyEvent::parse(&[0x4b, 0x80, 0x01, 0x1b]).unwrap();
        assert_eq!(fn_alone.key_name(), "Fn");
        assert!(matches!(
            fn_alone.action,
            Some(Action::Unknown { kind: 0x1b, .. })
        ));
        let left = KeyEvent::parse(&[0x53, 0x00, 0x01, 0x01, 0x00, 0x50]).unwrap();
        assert_eq!(left.action.unwrap().to_string(), "key ←");
        assert_eq!(KeyEvent::parse(&[0x53, 0x00]), None);
    }

    #[test]
    fn garbage_does_not_panic() {
        for len in 0..24 {
            let data: Vec<u8> = (0..len)
                .map(|i: u8| i.wrapping_mul(37).wrapping_add(0xa5))
                .collect();
            let _ = (
                DeviceInfo::parse(&data),
                parse_features(&data),
                Layout::parse(&data),
                parse_snap_tap(&data),
            );
            let _ = (
                LayerConfig::parse(&data),
                parse_bindings(&data),
                KeyEvent::parse(&data),
                parse_action(&data),
            );
        }
    }
}
