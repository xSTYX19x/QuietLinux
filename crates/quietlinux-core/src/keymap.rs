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
    /// Key center in mm from the top left corner of the keyboard.
    pub x: f32,
    pub y: f32,
}

const fn key(led: u8, code: &'static str, x: f32, y: f32) -> Key {
    Key { led, code, x, y }
}

/// The knob and every key with an LED, in LED order.
pub const KEYS: [Key; 111] = [
    key(0x2d, "Knob", 8.0, 17.4),
    // row 0
    key(0x2e, "Escape", 19.0, 17.4),
    key(0x2f, "F1", 57.0, 17.4),
    key(0x30, "F2", 76.0, 17.4),
    key(0x31, "F3", 95.0, 17.4),
    key(0x32, "F4", 114.0, 17.4),
    key(0x33, "F5", 142.5, 17.4),
    key(0x34, "F6", 161.5, 17.4),
    key(0x35, "F7", 180.5, 17.4),
    key(0x36, "F8", 199.5, 17.4),
    key(0x37, "F9", 228.0, 17.4),
    key(0x38, "F10", 247.0, 17.4),
    key(0x39, "F11", 266.0, 17.4),
    key(0x3a, "F12", 285.0, 17.4),
    key(0x3b, "PrintScreen", 310.9, 17.4),
    key(0x3c, "ScrollLock", 329.9, 17.4),
    key(0x3d, "Pause", 348.9, 17.4),
    // row 1
    key(0x3e, "M1", 8.0, 42.7),
    key(0x3f, "Backquote", 19.0, 42.7),
    key(0x40, "Digit1", 38.0, 42.7),
    key(0x41, "Digit2", 57.0, 42.7),
    key(0x42, "Digit3", 76.0, 42.7),
    key(0x43, "Digit4", 95.0, 42.7),
    key(0x44, "Digit5", 114.0, 42.7),
    key(0x45, "Digit6", 133.0, 42.7),
    key(0x46, "Digit7", 152.0, 42.7),
    key(0x47, "Digit8", 171.0, 42.7),
    key(0x48, "Digit9", 190.0, 42.7),
    key(0x49, "Digit0", 209.0, 42.7),
    key(0x4a, "Minus", 228.0, 42.7),
    key(0x4b, "Equal", 247.0, 42.7),
    key(0x4c, "Backspace", 275.5, 42.7),
    key(0x4d, "Insert", 310.9, 42.7),
    key(0x4e, "Home", 329.9, 42.7),
    key(0x4f, "PageUp", 348.9, 42.7),
    key(0x50, "NumLock", 374.8, 42.7),
    key(0x51, "NumpadDivide", 393.8, 42.7),
    key(0x52, "NumpadMultiply", 412.8, 42.7),
    key(0x53, "NumpadSubtract", 431.8, 42.7),
    // row 2 (0x62 is the ANSI backslash slot: no LED on ISO)
    key(0x54, "M2", 8.0, 61.7),
    key(0x55, "Tab", 23.7, 61.7),
    key(0x56, "KeyQ", 47.5, 61.7),
    key(0x57, "KeyW", 66.5, 61.7),
    key(0x58, "KeyE", 85.5, 61.7),
    key(0x59, "KeyR", 104.5, 61.7),
    key(0x5a, "KeyT", 123.5, 61.7),
    key(0x5b, "KeyY", 142.5, 61.7),
    key(0x5c, "KeyU", 161.5, 61.7),
    key(0x5d, "KeyI", 180.5, 61.7),
    key(0x5e, "KeyO", 199.5, 61.7),
    key(0x5f, "KeyP", 218.5, 61.7),
    key(0x60, "BracketLeft", 237.5, 61.7),
    key(0x61, "BracketRight", 256.5, 61.7),
    key(0x63, "Delete", 310.9, 61.7),
    key(0x64, "End", 329.9, 61.7),
    key(0x65, "PageDown", 348.9, 61.7),
    key(0x66, "Numpad7", 374.8, 61.7),
    key(0x67, "Numpad8", 393.8, 61.7),
    key(0x68, "Numpad9", 412.8, 61.7),
    key(0x69, "NumpadAdd", 431.8, 61.7),
    // row 3 (the ISO Enter LED sits on this row)
    key(0x6a, "M3", 8.0, 80.7),
    key(0x6b, "CapsLock", 26.1, 80.7),
    key(0x6c, "KeyA", 52.2, 80.7),
    key(0x6d, "KeyS", 71.2, 80.7),
    key(0x6e, "KeyD", 90.2, 80.7),
    key(0x6f, "KeyF", 109.2, 80.7),
    key(0x70, "KeyG", 128.2, 80.7),
    key(0x71, "KeyH", 147.2, 80.7),
    key(0x72, "KeyJ", 166.2, 80.7),
    key(0x73, "KeyK", 185.2, 80.7),
    key(0x74, "KeyL", 204.2, 80.7),
    key(0x75, "Semicolon", 223.2, 80.7),
    key(0x76, "Quote", 242.2, 80.7),
    key(0x77, "Backslash", 263.1, 80.7),
    key(0x78, "Enter", 273.1, 80.7),
    key(0x79, "Numpad4", 374.8, 80.7),
    key(0x7a, "Numpad5", 393.8, 80.7),
    key(0x7b, "Numpad6", 412.8, 80.7),
    // row 4 (0x89 is never written by IO Center)
    key(0x7c, "M4", 8.0, 99.7),
    key(0x7d, "ShiftLeft", 30.9, 99.7),
    key(0x7e, "IntlBackslash", 45.9, 99.7),
    key(0x7f, "KeyZ", 61.7, 99.7),
    key(0x80, "KeyX", 80.7, 99.7),
    key(0x81, "KeyC", 99.7, 99.7),
    key(0x82, "KeyV", 118.7, 99.7),
    key(0x83, "KeyB", 137.7, 99.7),
    key(0x84, "KeyN", 156.7, 99.7),
    key(0x85, "KeyM", 175.7, 99.7),
    key(0x86, "Comma", 194.7, 99.7),
    key(0x87, "Period", 213.7, 99.7),
    key(0x88, "Slash", 232.7, 99.7),
    key(0x8a, "ShiftRight", 268.4, 99.7),
    key(0x8b, "ArrowUp", 329.9, 99.7),
    key(0x8c, "Numpad1", 374.8, 99.7),
    key(0x8d, "Numpad2", 393.8, 99.7),
    key(0x8e, "Numpad3", 412.8, 99.7),
    key(0x8f, "NumpadEnter", 431.8, 99.7),
    // row 5
    key(0x90, "M5", 8.0, 118.7),
    key(0x91, "ControlLeft", 21.3, 118.7),
    key(0x92, "MetaLeft", 45.1, 118.7),
    key(0x93, "AltLeft", 68.8, 118.7),
    key(0x94, "Space", 140.1, 118.7),
    key(0x95, "AltRight", 211.3, 118.7),
    key(0x96, "MetaRight", 235.1, 118.7),
    key(0x97, "Fn", 258.9, 118.7),
    key(0x98, "ControlRight", 282.6, 118.7),
    key(0x99, "ArrowLeft", 310.9, 118.7),
    key(0x9a, "ArrowDown", 329.9, 118.7),
    key(0x9b, "ArrowRight", 348.9, 118.7),
    key(0x9c, "Numpad0", 384.3, 118.7),
    key(0x9d, "NumpadDecimal", 412.8, 118.7),
];

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
