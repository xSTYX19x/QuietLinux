//! Command line names for effects, directions, media actions, bindable keys and
//! game mode shortcuts, with their protocol values (see docs/protocol.md).

use std::fmt;
use std::str::FromStr;

use crate::Rgb;
use crate::keymap;
use crate::protocol::MAX_GRADIENT_STOPS;

fn lookup(table: &[(&str, u8)], s: &str, what: &str) -> Result<u8, String> {
    table
        .iter()
        .find(|(name, _)| *name == s)
        .map(|(_, v)| *v)
        .ok_or_else(|| {
            let names: Vec<&str> = table.iter().map(|(n, _)| *n).collect();
            format!("unknown {what} {s:?} (known: {})", names.join(", "))
        })
}

/// Effects IO Center offers for the Light Mount.
const EFFECTS: [(&str, u8); 6] = [
    ("static", 0),
    ("color-wave", 1),
    ("tornado", 2),
    ("breathing", 3),
    ("reactive", 4),
    ("matrix", 5),
];

const DIRECTIONS: [(&str, u8); 10] = [
    ("up", 0),
    ("down", 1),
    ("left", 2),
    ("right", 3),
    ("clockwise", 4),
    ("counterclockwise", 5),
    ("omnidirectional", 6),
    ("horizontal", 7),
    ("vertical", 8),
    ("cross", 9),
];

const MEDIA: [(&str, u8); 8] = [
    ("volume-up", 1),
    ("volume-down", 2),
    ("mute", 3),
    ("mic-mute", 4),
    ("next-track", 5),
    ("previous-track", 6),
    ("play-pause", 7),
    ("stop", 8),
];

const GAME_MODE: [(&str, u8); 5] = [
    ("shift-tab", 0x01),
    ("alt-f4", 0x02),
    ("win", 0x04),
    ("alt-tab", 0x08),
    ("caps-lock", 0x10),
];

const MOUSE_BUTTONS: [(&str, u8); 5] = [
    ("left", 0),
    ("right", 1),
    ("middle", 2),
    ("forward", 3),
    ("back", 4),
];

/// Lighting actions (selecting one effect is not captured yet).
const LIGHTING: [(&str, u8); 4] = [
    ("next-effect", 2),
    ("previous-effect", 3),
    ("brighter", 4),
    ("darker", 5),
];

const SHORTCUTS: [(&str, u8); 21] = [
    ("file-explorer", 1),
    ("calculator", 2),
    ("task-manager", 3),
    ("lock-pc", 4),
    ("shut-down", 5),
    ("sleep", 6),
    ("hibernate", 7),
    ("snipping-tool", 8),
    ("notepad", 9),
    ("paint", 10),
    ("airplane-mode", 11),
    ("xbox-game-bar", 12),
    ("system-settings", 13),
    ("refresh", 14),
    ("switch-app", 15),
    ("close-app", 16),
    ("copy", 17),
    ("paste", 18),
    ("cut", 19),
    ("web-browser", 20),
    ("email", 21),
];

/// Modifier names in key combinations (lowercase), and their bits.
const MODIFIERS: [(&str, u8); 13] = [
    ("ctrl", 0x01),
    ("lctrl", 0x01),
    ("shift", 0x02),
    ("lshift", 0x02),
    ("alt", 0x04),
    ("lalt", 0x04),
    ("win", 0x08),
    ("lwin", 0x08),
    ("rctrl", 0x10),
    ("rshift", 0x20),
    ("ralt", 0x40),
    ("altgr", 0x40),
    ("rwin", 0x80),
];

macro_rules! named_value {
    ($name:ident, $table:expr, $what:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name(pub u8);

        impl FromStr for $name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                lookup(&$table, s, $what).map(Self)
            }
        }

        impl $name {
            /// The command line name of the value, if it has one.
            #[allow(dead_code)]
            pub fn name(self) -> Option<&'static str> {
                $table.iter().find(|(_, v)| *v == self.0).map(|(n, _)| *n)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.name().unwrap_or("?"))
            }
        }
    };
}

named_value!(Effect, EFFECTS, "effect");
named_value!(Direction, DIRECTIONS, "direction");
named_value!(MediaAction, MEDIA, "media action");
named_value!(MouseButton, MOUSE_BUTTONS, "mouse button");
named_value!(LightingAction, LIGHTING, "lighting action");
named_value!(Shortcut, SHORTCUTS, "Windows shortcut");

impl Effect {
    pub const STATIC: Effect = Effect(0);
}

/// A key `bind` may change, with its vendor key ID: any key or knob control, except Fn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindKey {
    pub name: &'static str,
    pub key_id: u8,
}

impl FromStr for BindKey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "Fn" {
            return Err("the Fn key cannot be bound".to_owned());
        }
        let key_id = keymap::key_id_for_code(s).ok_or_else(|| {
            format!("unknown key {s:?} (see `quietlinux keys`; knob turns: KnobLeft, KnobRight)")
        })?;
        let name = keymap::code_for_key_id(key_id).unwrap_or("?");
        Ok(Self { name, key_id })
    }
}

/// A key combination for a binding: modifiers and at most one key, e.g. `ctrl+shift+KeyC`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyCombo {
    pub modifiers: u8,
    /// HID usage, 0 for modifiers alone.
    pub usage: u8,
}

impl FromStr for KeyCombo {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut combo = Self {
            modifiers: 0,
            usage: 0,
        };
        for part in s.split('+').map(str::trim) {
            let lower = part.to_ascii_lowercase();
            if let Some(&(_, bit)) = MODIFIERS.iter().find(|(name, _)| *name == lower) {
                combo.modifiers |= bit;
            } else if let Some(bit) = keymap::modifier_bit(part) {
                combo.modifiers |= bit;
            } else if let Some(usage) = keymap::hid_usage(part) {
                if combo.usage != 0 {
                    return Err(format!("{s:?}: only one key besides the modifiers"));
                }
                combo.usage = usage;
            } else {
                return Err(format!(
                    "{part:?} is neither a modifier (ctrl, shift, alt, win, rctrl, rshift, altgr, rwin) \
                     nor a key name (KeyA, Digit1, F13, Enter, … see `quietlinux keys`)"
                ));
            }
        }
        if combo
            == (Self {
                modifiers: 0,
                usage: 0,
            })
        {
            return Err("empty key combination".to_owned());
        }
        Ok(combo)
    }
}

impl KeyCombo {
    /// The combination as `bind` and config files write it (`ctrl+KeyC`), or `None`
    /// if the key has no name.
    pub fn text(self) -> Option<String> {
        const NAMES: [&str; 8] = [
            "ctrl", "shift", "alt", "win", "rctrl", "rshift", "altgr", "rwin",
        ];
        let mut parts: Vec<&str> = NAMES
            .iter()
            .enumerate()
            .filter(|(bit, _)| self.modifiers & (1 << bit) != 0)
            .map(|(_, name)| *name)
            .collect();
        if self.usage != 0 {
            parts.push(keymap::code_for_hid_usage(self.usage)?);
        }
        (!parts.is_empty()).then(|| parts.join("+"))
    }
}

/// Shortcuts game mode blocks, as flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameModeBlocks(pub u8);

impl GameModeBlocks {
    /// Names of the blocked shortcuts.
    pub fn names(self) -> Vec<&'static str> {
        GAME_MODE
            .iter()
            .filter(|(_, bit)| self.0 & bit != 0)
            .map(|(name, _)| *name)
            .collect()
    }
}

impl FromStr for GameModeBlocks {
    type Err = String;

    /// `none` or a comma-separated list, e.g. `win,alt-tab`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "none" {
            return Ok(Self(0));
        }
        s.split(',')
            .map(|name| lookup(&GAME_MODE, name.trim(), "game mode shortcut"))
            .try_fold(0, |flags, bit| bit.map(|b| flags | b))
            .map(Self)
    }
}

/// IO Center's default gradient (captured from IO Center Web).
pub fn rainbow() -> Vec<(Rgb, u8)> {
    [
        (Rgb::new(0xff, 0, 0), 0),
        (Rgb::new(0xff, 0xff, 0), 17),
        (Rgb::new(0, 0xff, 0), 33),
        (Rgb::new(0, 0xff, 0xff), 50),
        (Rgb::new(0, 0, 0xff), 67),
        (Rgb::new(0xff, 0, 0xff), 83),
        (Rgb::new(0xff, 0, 0), 100),
    ]
    .to_vec()
}

/// Two colors, `rrggbb,rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DualColor(pub Rgb, pub Rgb);

impl FromStr for DualColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (a, b) = s
            .split_once(',')
            .ok_or_else(|| format!("expected two colors like e63000,ffffff, got {s:?}"))?;
        let color = |c: &str| c.trim().parse::<Rgb>().map_err(|e| e.to_string());
        Ok(Self(color(a)?, color(b)?))
    }
}

/// Color stops of a gradient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gradient(pub Vec<(Rgb, u8)>);

impl FromStr for Gradient {
    type Err = String;

    /// `ff0000,00ff00,0000ff` (evenly spread) or `ff0000@0,0000ff@100`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(',').map(str::trim).collect();
        if !(2..=MAX_GRADIENT_STOPS).contains(&parts.len()) {
            return Err(format!(
                "a gradient needs 2 to {MAX_GRADIENT_STOPS} colors, got {}",
                parts.len()
            ));
        }
        let explicit = parts.iter().filter(|p| p.contains('@')).count();
        if explicit != 0 && explicit != parts.len() {
            return Err("give a position (@0 to @100) for every color or for none".to_owned());
        }
        let last = parts.len() - 1;
        parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                let (color, position) = match part.split_once('@') {
                    Some((color, pos)) => {
                        let pos: u8 = pos
                            .parse()
                            .ok()
                            .filter(|p| *p <= 100)
                            .ok_or_else(|| format!("invalid position in {part:?}"))?;
                        (color, pos)
                    }
                    // i ≤ last ≤ 11, so the result is at most 100
                    None => (
                        *part,
                        u8::try_from((i * 100 + last / 2) / last).unwrap_or(100),
                    ),
                };
                Ok((color.parse::<Rgb>().map_err(|e| e.to_string())?, position))
            })
            .collect::<Result<Vec<_>, String>>()
            .map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!("breathing".parse(), Ok(Effect(3)));
        assert!("ripple".parse::<Effect>().is_err()); // not offered for the Light Mount
        assert_eq!(Effect(3).to_string(), "breathing");
        assert_eq!("counterclockwise".parse(), Ok(Direction(5)));
        assert_eq!("stop".parse(), Ok(MediaAction(8)));
        let err = "louder".parse::<MediaAction>().unwrap_err();
        assert!(err.contains("volume-up"), "{err}");
    }

    #[test]
    fn bindable_keys() {
        assert_eq!(
            "M3".parse(),
            Ok(BindKey {
                name: "M3",
                key_id: 0x04
            })
        );
        assert_eq!("KnobLeft".parse::<BindKey>().unwrap().key_id, 0x6f);
        assert_eq!("CapsLock".parse::<BindKey>().unwrap().key_id, 0x09); // captured
        assert_eq!("F1".parse::<BindKey>().unwrap().key_id, 0x16); // captured
        assert!("Fn".parse::<BindKey>().is_err());
        assert!("capslock".parse::<BindKey>().is_err());
    }

    #[test]
    fn key_combos() {
        let combo = |modifiers, usage| Ok(KeyCombo { modifiers, usage });
        assert_eq!("KeyA".parse(), combo(0, 0x04));
        assert_eq!("ctrl+KeyC".parse(), combo(0x01, 0x06)); // captured
        assert_eq!("F13".parse(), combo(0, 0x68)); // captured
        assert_eq!("Ctrl + Shift + Escape".parse(), combo(0x03, 0x29));
        assert_eq!("ControlLeft".parse(), combo(0x01, 0));
        assert_eq!("altgr+KeyQ".parse(), combo(0x40, 0x14));
        for bad in ["", "ctrl+KeyA+KeyB", "hyper+KeyA", "M1", "keya"] {
            assert!(bad.parse::<KeyCombo>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn combos_round_trip() {
        for text in [
            "KeyA",
            "ctrl+KeyC",
            "F13",
            "ctrl+shift+Escape",
            "ctrl",
            "altgr+KeyQ",
            "rwin+F24",
        ] {
            let combo: KeyCombo = text.parse().unwrap();
            assert_eq!(combo.text().as_deref(), Some(text));
        }
        assert_eq!(
            KeyCombo {
                modifiers: 0,
                usage: 0x31
            }
            .text(),
            None
        );
        assert_eq!(GameModeBlocks(0x0c).names(), ["win", "alt-tab"]);
    }

    #[test]
    fn action_names() {
        assert_eq!("middle".parse(), Ok(MouseButton(2))); // captured
        assert_eq!("next-effect".parse(), Ok(LightingAction(2))); // captured
        assert_eq!("calculator".parse(), Ok(Shortcut(2))); // captured
        assert!("select-effect".parse::<LightingAction>().is_err());
    }

    #[test]
    fn game_mode() {
        assert_eq!("none".parse(), Ok(GameModeBlocks(0)));
        assert_eq!("win,alt-tab".parse(), Ok(GameModeBlocks(0x0c)));
        assert_eq!(
            "shift-tab, alt-f4, win, alt-tab, caps-lock".parse(),
            Ok(GameModeBlocks(0x1f))
        );
        assert!("win,escape".parse::<GameModeBlocks>().is_err());
    }

    #[test]
    fn dual_colors() {
        assert_eq!(
            "e63000,ffffff".parse(),
            Ok(DualColor(
                Rgb::new(0xe6, 0x30, 0),
                Rgb::new(0xff, 0xff, 0xff)
            ))
        );
        assert!("e63000".parse::<DualColor>().is_err());
        assert!("e63000,white".parse::<DualColor>().is_err());
    }

    #[test]
    fn gradients() {
        let g: Gradient = "ff0000,00ff00,0000ff".parse().unwrap();
        assert_eq!(g.0.iter().map(|s| s.1).collect::<Vec<_>>(), [0, 50, 100]);
        let g: Gradient = "ff0000@10,0000ff@90".parse().unwrap();
        assert_eq!(
            g.0,
            [(Rgb::new(0xff, 0, 0), 10), (Rgb::new(0, 0, 0xff), 90)]
        );
        assert_eq!(
            rainbow().iter().map(|s| s.1).collect::<Vec<_>>(),
            [0, 17, 33, 50, 67, 83, 100]
        );
        let six: Gradient = "000000,000000,000000,000000,000000,000000,000000"
            .parse()
            .unwrap();
        assert_eq!(
            six.0.iter().map(|s| s.1).collect::<Vec<_>>(),
            [0, 17, 33, 50, 67, 83, 100]
        );
        for bad in [
            "ff0000",
            "ff0000,00ff00@50",
            "ff0000@0,00ff00@101",
            "red,blue",
        ] {
            assert!(bad.parse::<Gradient>().is_err(), "{bad}");
        }
    }
}
