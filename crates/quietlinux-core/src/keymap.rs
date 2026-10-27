//! Physical keys of the Light Mount: LED index, layout-independent name and position.
//!
//! Keys are named with the W3C `KeyboardEvent.code` values
//! (<https://www.w3.org/TR/uievents-code/>), which name the physical position, not
//! the printed legend: `KeyQ` is the key right of Tab on every layout, even where it
//! reads "A" (French). Keys without a W3C name use `M1`–`M5`, `Fn` and `Knob`.
//!
//! LED indexes and positions come from `lightmount/keymap.py` of
//! `bbossa/keyboard_bequiet_lightmount_rgb`: `LampArray` positions (interface 3)
//! matched row by row with the vendor indexes, confirmed key by key on the
//! keyboard. French and German ISO keyboards share the same physical layout, only
//! the legends differ ([`Layout`]).

use std::fmt;
use std::str::FromStr;

/// A physical key (or the knob) with its LED.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    /// Vendor LED index (`10:0d`).
    pub led: u8,
    /// Layout-independent name (W3C `KeyboardEvent.code`, or `M1`–`M5`, `Fn`, `Knob`).
    pub code: &'static str,
    /// Vendor key ID (`KEY_ID2` in IO Center): bindings, quick tap, key events.
    pub key_id: u8,
    /// Key center in mm from the top left corner of the keyboard.
    pub x: f32,
    pub y: f32,
}

const fn key(led: u8, code: &'static str, key_id: u8, x: f32, y: f32) -> Key {
    Key {
        led,
        code,
        key_id,
        x,
        y,
    }
}

/// The knob and every key with an LED, in LED order.
pub const KEYS: [Key; 111] = [
    key(0x2d, "Knob", 0x70, 8.0, 17.4),
    // row 0
    key(0x2e, "Escape", 0x01, 19.0, 17.4),
    key(0x2f, "F1", 0x16, 57.0, 17.4),
    key(0x30, "F2", 0x1c, 76.0, 17.4),
    key(0x31, "F3", 0x21, 95.0, 17.4),
    key(0x32, "F4", 0x26, 114.0, 17.4),
    key(0x33, "F5", 0x31, 142.5, 17.4),
    key(0x34, "F6", 0x36, 161.5, 17.4),
    key(0x35, "F7", 0x3b, 180.5, 17.4),
    key(0x36, "F8", 0x40, 199.5, 17.4),
    key(0x37, "F9", 0x45, 228.0, 17.4),
    key(0x38, "F10", 0x4a, 247.0, 17.4),
    key(0x39, "F11", 0x4c, 266.0, 17.4),
    key(0x3a, "F12", 0x50, 285.0, 17.4),
    key(0x3b, "PrintScreen", 0x54, 310.9, 17.4),
    key(0x3c, "ScrollLock", 0x59, 329.9, 17.4),
    key(0x3d, "Pause", 0x5d, 348.9, 17.4),
    // row 1
    key(0x3e, "M1", 0x02, 8.0, 42.7),
    key(0x3f, "Backquote", 0x07, 19.0, 42.7),
    key(0x40, "Digit1", 0x0c, 38.0, 42.7),
    key(0x41, "Digit2", 0x11, 57.0, 42.7),
    key(0x42, "Digit3", 0x17, 76.0, 42.7),
    key(0x43, "Digit4", 0x1d, 95.0, 42.7),
    key(0x44, "Digit5", 0x22, 114.0, 42.7),
    key(0x45, "Digit6", 0x27, 133.0, 42.7),
    key(0x46, "Digit7", 0x2c, 152.0, 42.7),
    key(0x47, "Digit8", 0x32, 171.0, 42.7),
    key(0x48, "Digit9", 0x37, 190.0, 42.7),
    key(0x49, "Digit0", 0x3c, 209.0, 42.7),
    key(0x4a, "Minus", 0x41, 228.0, 42.7),
    key(0x4b, "Equal", 0x46, 247.0, 42.7),
    key(0x4c, "Backspace", 0x4d, 275.5, 42.7),
    key(0x4d, "Insert", 0x51, 310.9, 42.7),
    key(0x4e, "Home", 0x55, 329.9, 42.7),
    key(0x4f, "PageUp", 0x5b, 348.9, 42.7),
    key(0x50, "NumLock", 0x5e, 374.8, 42.7),
    key(0x51, "NumpadDivide", 0x63, 393.8, 42.7),
    key(0x52, "NumpadMultiply", 0x67, 412.8, 42.7),
    key(0x53, "NumpadSubtract", 0x6c, 431.8, 42.7),
    // row 2 (0x62 is the ANSI backslash slot: no LED on ISO)
    key(0x54, "M2", 0x03, 8.0, 61.7),
    key(0x55, "Tab", 0x08, 23.7, 61.7),
    key(0x56, "KeyQ", 0x0d, 47.5, 61.7),
    key(0x57, "KeyW", 0x12, 66.5, 61.7),
    key(0x58, "KeyE", 0x18, 85.5, 61.7),
    key(0x59, "KeyR", 0x1e, 104.5, 61.7),
    key(0x5a, "KeyT", 0x23, 123.5, 61.7),
    key(0x5b, "KeyY", 0x28, 142.5, 61.7),
    key(0x5c, "KeyU", 0x2d, 161.5, 61.7),
    key(0x5d, "KeyI", 0x33, 180.5, 61.7),
    key(0x5e, "KeyO", 0x38, 199.5, 61.7),
    key(0x5f, "KeyP", 0x3d, 218.5, 61.7),
    key(0x60, "BracketLeft", 0x42, 237.5, 61.7),
    key(0x61, "BracketRight", 0x47, 256.5, 61.7),
    key(0x63, "Delete", 0x52, 310.9, 61.7),
    key(0x64, "End", 0x56, 329.9, 61.7),
    key(0x65, "PageDown", 0x5a, 348.9, 61.7),
    key(0x66, "Numpad7", 0x5f, 374.8, 61.7),
    key(0x67, "Numpad8", 0x64, 393.8, 61.7),
    key(0x68, "Numpad9", 0x68, 412.8, 61.7),
    key(0x69, "NumpadAdd", 0x6d, 431.8, 61.7),
    // row 3 (the ISO Enter LED sits on this row)
    key(0x6a, "M3", 0x04, 8.0, 80.7),
    key(0x6b, "CapsLock", 0x09, 26.1, 80.7),
    key(0x6c, "KeyA", 0x0e, 52.2, 80.7),
    key(0x6d, "KeyS", 0x13, 71.2, 80.7),
    key(0x6e, "KeyD", 0x19, 90.2, 80.7),
    key(0x6f, "KeyF", 0x1f, 109.2, 80.7),
    key(0x70, "KeyG", 0x24, 128.2, 80.7),
    key(0x71, "KeyH", 0x29, 147.2, 80.7),
    key(0x72, "KeyJ", 0x2e, 166.2, 80.7),
    key(0x73, "KeyK", 0x34, 185.2, 80.7),
    key(0x74, "KeyL", 0x39, 204.2, 80.7),
    key(0x75, "Semicolon", 0x3e, 223.2, 80.7),
    key(0x76, "Quote", 0x43, 242.2, 80.7),
    key(0x77, "Backslash", 0x71, 263.1, 80.7),
    key(0x78, "Enter", 0x0a, 273.1, 80.7),
    key(0x79, "Numpad4", 0x60, 374.8, 80.7),
    key(0x7a, "Numpad5", 0x65, 393.8, 80.7),
    key(0x7b, "Numpad6", 0x69, 412.8, 80.7),
    // row 4 (0x89 is never written by IO Center)
    key(0x7c, "M4", 0x05, 8.0, 99.7),
    key(0x7d, "ShiftLeft", 0x1b, 30.9, 99.7),
    key(0x7e, "IntlBackslash", 0x72, 45.9, 99.7),
    key(0x7f, "KeyZ", 0x0f, 61.7, 99.7),
    key(0x80, "KeyX", 0x14, 80.7, 99.7),
    key(0x81, "KeyC", 0x1a, 99.7, 99.7),
    key(0x82, "KeyV", 0x20, 118.7, 99.7),
    key(0x83, "KeyB", 0x25, 137.7, 99.7),
    key(0x84, "KeyN", 0x2b, 156.7, 99.7),
    key(0x85, "KeyM", 0x2f, 175.7, 99.7),
    key(0x86, "Comma", 0x35, 194.7, 99.7),
    key(0x87, "Period", 0x3a, 213.7, 99.7),
    key(0x88, "Slash", 0x3f, 232.7, 99.7),
    key(0x8a, "ShiftRight", 0x4e, 268.4, 99.7),
    key(0x8b, "ArrowUp", 0x57, 329.9, 99.7),
    key(0x8c, "Numpad1", 0x61, 374.8, 99.7),
    key(0x8d, "Numpad2", 0x66, 393.8, 99.7),
    key(0x8e, "Numpad3", 0x6a, 412.8, 99.7),
    key(0x8f, "NumpadEnter", 0x48, 431.8, 99.7),
    // row 5
    key(0x90, "M5", 0x06, 8.0, 118.7),
    key(0x91, "ControlLeft", 0x0b, 21.3, 118.7),
    key(0x92, "MetaLeft", 0x10, 45.1, 118.7),
    key(0x93, "AltLeft", 0x15, 68.8, 118.7),
    key(0x94, "Space", 0x2a, 140.1, 118.7),
    key(0x95, "AltRight", 0x44, 211.3, 118.7),
    key(0x96, "MetaRight", 0x49, 235.1, 118.7),
    key(0x97, "Fn", 0x4b, 258.9, 118.7),
    key(0x98, "ControlRight", 0x4f, 282.6, 118.7),
    key(0x99, "ArrowLeft", 0x53, 310.9, 118.7),
    key(0x9a, "ArrowDown", 0x58, 329.9, 118.7),
    key(0x9b, "ArrowRight", 0x5c, 348.9, 118.7),
    key(0x9c, "Numpad0", 0x62, 384.3, 118.7),
    key(0x9d, "NumpadDecimal", 0x6b, 412.8, 118.7),
];

/// Controls without an LED, with their key IDs: the knob's turns.
pub const KNOB_TURNS: [(&str, u8); 2] = [("KnobLeft", 0x6f), ("KnobRight", 0x6e)];

/// Name of a key or control (`KnobLeft`, `KnobRight`) by its vendor key ID.
pub fn code_for_key_id(key_id: u8) -> Option<&'static str> {
    KEYS.iter()
        .find(|k| k.key_id == key_id)
        .map(|k| k.code)
        .or_else(|| {
            KNOB_TURNS
                .iter()
                .find(|(_, id)| *id == key_id)
                .map(|(c, _)| *c)
        })
}

/// Vendor key ID of a key or control by its name.
pub fn key_id_for_code(code: &str) -> Option<u8> {
    by_code(code).map(|k| k.key_id).or_else(|| {
        KNOB_TURNS
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, id)| *id)
    })
}

/// HID keyboard usage a key sends, by its name (W3C names, plus `F13`–`F24`, which
/// the Light Mount has no keys for but can send). `None` for keys without one
/// (M1–M5, Fn, the knob) and for modifiers, which are bits, see [`modifier_bit`].
pub fn hid_usage(code: &str) -> Option<u8> {
    const NAMED: [(&str, u8); 47] = [
        ("Enter", 0x28),
        ("Escape", 0x29),
        ("Backspace", 0x2a),
        ("Tab", 0x2b),
        ("Space", 0x2c),
        ("Minus", 0x2d),
        ("Equal", 0x2e),
        ("BracketLeft", 0x2f),
        ("BracketRight", 0x30),
        ("Backslash", 0x32), // the ISO key left of Enter (HID "non-US #")
        ("Semicolon", 0x33),
        ("Quote", 0x34),
        ("Backquote", 0x35),
        ("Comma", 0x36),
        ("Period", 0x37),
        ("Slash", 0x38),
        ("CapsLock", 0x39),
        ("PrintScreen", 0x46),
        ("ScrollLock", 0x47),
        ("Pause", 0x48),
        ("Insert", 0x49),
        ("Home", 0x4a),
        ("PageUp", 0x4b),
        ("Delete", 0x4c),
        ("End", 0x4d),
        ("PageDown", 0x4e),
        ("ArrowRight", 0x4f),
        ("ArrowLeft", 0x50),
        ("ArrowDown", 0x51),
        ("ArrowUp", 0x52),
        ("NumLock", 0x53),
        ("NumpadDivide", 0x54),
        ("NumpadMultiply", 0x55),
        ("NumpadSubtract", 0x56),
        ("NumpadAdd", 0x57),
        ("NumpadEnter", 0x58),
        ("Numpad1", 0x59),
        ("Numpad2", 0x5a),
        ("Numpad3", 0x5b),
        ("Numpad4", 0x5c),
        ("Numpad5", 0x5d),
        ("Numpad6", 0x5e),
        ("Numpad7", 0x5f),
        ("Numpad8", 0x60),
        ("Numpad9", 0x61),
        ("Numpad0", 0x62),
        ("NumpadDecimal", 0x63),
    ];
    if let Some(&(_, usage)) = NAMED.iter().find(|(name, _)| *name == code) {
        return Some(usage);
    }
    if code == "IntlBackslash" {
        return Some(0x64);
    }
    if let Some(letter) = code.strip_prefix("Key").filter(|l| l.len() == 1) {
        let c = letter.as_bytes()[0];
        return c.is_ascii_uppercase().then(|| 0x04 + (c - b'A'));
    }
    if let Some(digit) = code.strip_prefix("Digit").filter(|d| d.len() == 1) {
        let d = digit.as_bytes()[0];
        return match d {
            b'1'..=b'9' => Some(0x1e + (d - b'1')),
            b'0' => Some(0x27),
            _ => None,
        };
    }
    let n: u8 = code.strip_prefix('F')?.parse().ok()?;
    match n {
        1..=12 => Some(0x3a + n - 1),
        13..=24 => Some(0x68 + n - 13),
        _ => None,
    }
}

/// Modifier bit of a modifier key (HID order): left Ctrl, Shift, Alt, Win, then right.
pub fn modifier_bit(code: &str) -> Option<u8> {
    const MODIFIERS: [&str; 8] = [
        "ControlLeft",
        "ShiftLeft",
        "AltLeft",
        "MetaLeft",
        "ControlRight",
        "ShiftRight",
        "AltRight",
        "MetaRight",
    ];
    MODIFIERS.iter().position(|m| *m == code).map(|i| 1 << i)
}

/// The key with this layout-independent name (case-sensitive, like the W3C names).
pub fn by_code(code: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.code == code)
}

/// The key lit by this LED index.
pub fn by_led(led: u8) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.led == led)
}

/// Legends printed on the keycaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Layout {
    /// English (UK) ISO.
    #[default]
    Uk,
    /// German ISO (QWERTZ).
    De,
    /// French ISO (AZERTY).
    Fr,
}

impl Layout {
    pub const ALL: [Layout; 3] = [Layout::Uk, Layout::De, Layout::Fr];

    pub const fn name(self) -> &'static str {
        match self {
            Layout::Uk => "uk",
            Layout::De => "de",
            Layout::Fr => "fr",
        }
    }

    /// Legend of a key on this layout. Unknown codes are returned unchanged.
    pub fn legend(self, code: &'static str) -> &'static str {
        self.own_legend(code)
            .or_else(|| common_legend(code))
            .unwrap_or(code)
    }

    /// Legends of the character keys, which differ between layouts.
    // One table per layout reads better than arms merged across layouts.
    #[allow(clippy::match_same_arms)]
    fn own_legend(self, code: &str) -> Option<&'static str> {
        let legend = match (self, code) {
            (Layout::Uk, "Backquote") => "` ¬ ¦",
            (Layout::Uk, "Digit1") => "1 !",
            (Layout::Uk, "Digit2") => "2 \"",
            (Layout::Uk, "Digit3") => "3 £",
            (Layout::Uk, "Digit4") => "4 $ €",
            (Layout::Uk, "Digit5") => "5 %",
            (Layout::Uk, "Digit6") => "6 ^",
            (Layout::Uk, "Digit7") => "7 &",
            (Layout::Uk, "Digit8") => "8 *",
            (Layout::Uk, "Digit9") => "9 (",
            (Layout::Uk, "Digit0") => "0 )",
            (Layout::Uk, "Minus") => "- _",
            (Layout::Uk, "Equal") => "= +",
            (Layout::Uk, "BracketLeft") => "[ {",
            (Layout::Uk, "BracketRight") => "] }",
            (Layout::Uk, "Semicolon") => "; :",
            (Layout::Uk, "Quote") => "' @",
            (Layout::Uk, "Backslash") => "# ~",
            (Layout::Uk, "IntlBackslash") => "\\ |",
            (Layout::Uk, "Comma") => ", <",
            (Layout::Uk, "Period") => ". >",
            (Layout::Uk, "Slash") => "/ ?",

            (Layout::De, "Backquote") => "^ °",
            (Layout::De, "Digit1") => "1 !",
            (Layout::De, "Digit2") => "2 \" ²",
            (Layout::De, "Digit3") => "3 § ³",
            (Layout::De, "Digit4") => "4 $",
            (Layout::De, "Digit5") => "5 %",
            (Layout::De, "Digit6") => "6 &",
            (Layout::De, "Digit7") => "7 / {",
            (Layout::De, "Digit8") => "8 ( [",
            (Layout::De, "Digit9") => "9 ) ]",
            (Layout::De, "Digit0") => "0 = }",
            (Layout::De, "Minus") => "ß ? \\",
            (Layout::De, "Equal") => "´ `",
            (Layout::De, "KeyQ") => "Q @",
            (Layout::De, "KeyE") => "E €",
            (Layout::De, "KeyY") => "Z",
            (Layout::De, "BracketLeft") => "Ü",
            (Layout::De, "BracketRight") => "+ * ~",
            (Layout::De, "Semicolon") => "Ö",
            (Layout::De, "Quote") => "Ä",
            (Layout::De, "Backslash") => "# '",
            (Layout::De, "IntlBackslash") => "< > |",
            (Layout::De, "KeyZ") => "Y",
            (Layout::De, "KeyM") => "M µ",
            (Layout::De, "Comma") => ", ;",
            (Layout::De, "Period") => ". :",
            (Layout::De, "Slash") => "- _",
            (Layout::De, "NumpadDecimal") => ",",

            // from bbossa's keymap.py (product image of the French keyboard)
            (Layout::Fr, "Backquote") => "²",
            (Layout::Fr, "Digit1") => "1 &",
            (Layout::Fr, "Digit2") => "2 é ~",
            (Layout::Fr, "Digit3") => "3 \" #",
            (Layout::Fr, "Digit4") => "4 ' {",
            (Layout::Fr, "Digit5") => "5 ( [",
            (Layout::Fr, "Digit6") => "6 - |",
            (Layout::Fr, "Digit7") => "7 è `",
            (Layout::Fr, "Digit8") => "8 _ \\",
            (Layout::Fr, "Digit9") => "9 ç ^",
            (Layout::Fr, "Digit0") => "0 à @",
            (Layout::Fr, "Minus") => "° ) ]",
            (Layout::Fr, "Equal") => "+ = }",
            (Layout::Fr, "KeyQ") => "A",
            (Layout::Fr, "KeyW") => "Z",
            (Layout::Fr, "KeyE") => "E €",
            (Layout::Fr, "BracketLeft") => "¨ ^",
            (Layout::Fr, "BracketRight") => "£ $ ¤",
            (Layout::Fr, "KeyA") => "Q",
            (Layout::Fr, "Semicolon") => "M",
            (Layout::Fr, "Quote") => "% ù",
            (Layout::Fr, "Backslash") => "µ *",
            (Layout::Fr, "IntlBackslash") => "< >",
            (Layout::Fr, "KeyZ") => "W",
            (Layout::Fr, "KeyM") => "? ,",
            (Layout::Fr, "Comma") => ". ;",
            (Layout::Fr, "Period") => "/ :",
            (Layout::Fr, "Slash") => "§ !",
            (Layout::Fr, "NumpadDecimal") => ",",
            _ => return None,
        };
        Some(legend)
    }
}

/// Legends shared by every layout. Letters, digits and numpad digits come from the name.
fn common_legend(code: &'static str) -> Option<&'static str> {
    if let Some(rest) = code
        .strip_prefix("Key")
        .or_else(|| code.strip_prefix("Digit"))
        .or_else(|| code.strip_prefix("Numpad").filter(|r| r.len() == 1))
    {
        return Some(rest);
    }
    let legend = match code {
        "Escape" => "Esc",
        "PrintScreen" => "Print",
        "ScrollLock" => "Scroll",
        "Insert" => "Ins",
        "PageUp" => "PgUp",
        "Delete" => "Del",
        "PageDown" => "PgDn",
        "NumLock" => "Num",
        "NumpadDivide" => "/",
        "NumpadMultiply" => "*",
        "NumpadSubtract" => "-",
        "NumpadAdd" => "+",
        "NumpadEnter" => "Enter",
        "NumpadDecimal" => ".",
        "CapsLock" => "Caps",
        "ShiftLeft" | "ShiftRight" => "Shift",
        "ControlLeft" | "ControlRight" => "Ctrl",
        "MetaLeft" | "MetaRight" => "Win",
        "AltLeft" => "Alt",
        "AltRight" => "Alt Gr",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        _ => return None,
    };
    Some(legend)
}

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Layout {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Layout::ALL
            .into_iter()
            .find(|l| l.name() == s)
            .ok_or_else(|| format!("unknown layout {s:?} (known: uk, de, fr)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Zone;

    #[test]
    fn keys_cover_knob_and_key_zone() {
        let mut leds: Vec<u8> = KEYS.iter().map(|k| k.led).collect();
        let expected: Vec<u8> = Zone::Knob.leds().chain(Zone::Keys.leds()).collect();
        assert_eq!(leds, expected, "KEYS must be in LED order");
        leds.dedup();
        assert_eq!(leds.len(), KEYS.len());
    }

    #[test]
    fn codes_are_unique() {
        for key in &KEYS {
            assert_eq!(by_code(key.code).unwrap().led, key.led, "{}", key.code);
        }
    }

    #[test]
    fn key_ids_are_unique() {
        let mut ids: Vec<u8> = KEYS
            .iter()
            .map(|k| k.key_id)
            .chain(KNOB_TURNS.iter().map(|t| t.1))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), KEYS.len() + KNOB_TURNS.len());
    }

    #[test]
    fn key_ids_match_captured_key_events() {
        // key events captured by bbossa: ← 53, → 5c, Fn 4b
        assert_eq!(key_id_for_code("ArrowLeft"), Some(0x53));
        assert_eq!(key_id_for_code("ArrowRight"), Some(0x5c));
        assert_eq!(key_id_for_code("Fn"), Some(0x4b));
        assert_eq!(code_for_key_id(0x02), Some("M1"));
        assert_eq!(code_for_key_id(0x6e), Some("KnobRight"));
        assert_eq!(code_for_key_id(0x30), None); // ANSI backslash: not on ISO
    }

    #[test]
    fn hid_usages() {
        assert_eq!(hid_usage("KeyA"), Some(0x04));
        assert_eq!(hid_usage("KeyZ"), Some(0x1d));
        assert_eq!(hid_usage("Digit1"), Some(0x1e));
        assert_eq!(hid_usage("Digit0"), Some(0x27));
        assert_eq!(hid_usage("F1"), Some(0x3a));
        assert_eq!(hid_usage("F12"), Some(0x45));
        assert_eq!(hid_usage("F13"), Some(0x68)); // captured: M2 bound to F13
        assert_eq!(hid_usage("ArrowLeft"), Some(0x50)); // captured key event
        assert_eq!(hid_usage("Backslash"), Some(0x32)); // captured key event (ISO #)
        assert_eq!(hid_usage("IntlBackslash"), Some(0x64)); // captured key event (ISO <)
        for none in ["M1", "Fn", "Knob", "ControlLeft", "F25", "Keya", "Digit10"] {
            assert_eq!(hid_usage(none), None, "{none}");
        }
        assert_eq!(modifier_bit("ControlLeft"), Some(0x01));
        assert_eq!(modifier_bit("MetaRight"), Some(0x80));
        // every key with a usage that the keyboard has, except modifiers and specials
        let without: Vec<&str> = KEYS
            .iter()
            .map(|k| k.code)
            .filter(|c| hid_usage(c).is_none() && modifier_bit(c).is_none())
            .collect();
        assert_eq!(without, ["Knob", "M1", "M2", "M3", "M4", "M5", "Fn"]);
    }

    #[test]
    fn lookups() {
        assert_eq!(by_code("Escape").unwrap().led, 0x2e);
        assert_eq!(by_led(0x7e).unwrap().code, "IntlBackslash");
        assert!(by_code("escape").is_none());
        assert!(by_led(0x62).is_none());
    }

    #[test]
    fn legends() {
        assert_eq!(Layout::De.legend("KeyY"), "Z");
        assert_eq!(Layout::De.legend("Quote"), "Ä");
        assert_eq!(Layout::Fr.legend("KeyQ"), "A");
        assert_eq!(Layout::Uk.legend("KeyQ"), "Q");
        assert_eq!(Layout::Uk.legend("Numpad7"), "7");
        assert_eq!(Layout::Uk.legend("NumpadAdd"), "+");
        assert_eq!(Layout::De.legend("Escape"), "Esc");
        assert_eq!(Layout::De.legend("M3"), "M3");
    }

    #[test]
    fn every_key_has_a_legend() {
        // keys whose legend is their name
        let unnamed = [
            "Knob",
            "M1",
            "M2",
            "M3",
            "M4",
            "M5",
            "Fn",
            "F1",
            "F2",
            "F3",
            "F4",
            "F5",
            "F6",
            "F7",
            "F8",
            "F9",
            "F10",
            "F11",
            "F12",
            "Pause",
            "Backspace",
            "Tab",
            "Enter",
            "Space",
            "Home",
            "End",
        ];
        for layout in Layout::ALL {
            for key in &KEYS {
                let legend = layout.legend(key.code);
                assert!(
                    legend != key.code || unnamed.contains(&key.code),
                    "{layout}: no legend for {}",
                    key.code
                );
            }
        }
    }
}
