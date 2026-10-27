//! Command line names for effects, directions, media actions, bindable keys and
//! game mode shortcuts, with their protocol values (see docs/protocol.md).

use std::fmt;
use std::str::FromStr;

use quietlinux_core::Rgb;
use quietlinux_core::keymap;
use quietlinux_core::protocol::MAX_GRADIENT_STOPS;

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

/// Keys `bind` may change: their factory bindings are media actions too, so a
/// media binding can always be undone.
pub const BINDABLE: [&str; 8] = [
    "M1",
    "M2",
    "M3",
    "M4",
    "M5",
    "Knob",
    "KnobLeft",
    "KnobRight",
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

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let name = $table
                    .iter()
                    .find(|(_, v)| *v == self.0)
                    .map_or("?", |(n, _)| *n);
                f.write_str(name)
            }
        }
    };
}

named_value!(Effect, EFFECTS, "effect");
named_value!(Direction, DIRECTIONS, "direction");
named_value!(MediaAction, MEDIA, "media action");

impl Effect {
    pub const STATIC: Effect = Effect(0);
}

/// A key `bind` may change, with its vendor key ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindKey {
    pub name: &'static str,
    pub key_id: u8,
}

impl FromStr for BindKey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let name = BINDABLE.iter().find(|n| **n == s).ok_or_else(|| {
            format!(
                "{s:?} cannot be bound yet (possible: {})",
                BINDABLE.join(", ")
            )
        })?;
        let key_id =
            keymap::key_id_for_code(name).ok_or_else(|| format!("no key ID for {name}"))?;
        Ok(Self { name, key_id })
    }
}

/// Shortcuts game mode blocks, as flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameModeBlocks(pub u8);

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
        assert!("KeyA".parse::<BindKey>().is_err());
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
