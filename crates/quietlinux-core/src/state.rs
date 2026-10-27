//! What QuietLinux last showed on the keyboard: `$XDG_CONFIG_HOME/quietlinux/lighting.toml`.
//!
//! The keyboard keeps its stored effect, but per-key colors (realtime mode) are
//! gone after it is unplugged or the computer sleeps. The window and the command
//! line write here what they show; the background service shows it again.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::Rgb;
use crate::config;
use crate::protocol::Led;

/// The lighting the user chose last.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// The stored effect: the keyboard shows it by itself.
    #[default]
    Effect,
    /// Per-key colors, [`Lighting::leds`].
    PerKey,
    Off,
}

impl Mode {
    const NAMES: [(Mode, &'static str); 3] = [
        (Mode::Effect, "effect"),
        (Mode::PerKey, "per-key"),
        (Mode::Off, "off"),
    ];

    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(m, _)| *m == self)
            .map_or("effect", |(_, n)| n)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lighting {
    pub mode: Mode,
    /// Per-key colors by LED index (keys, knob, light bars); LEDs not listed are off.
    pub leds: BTreeMap<u8, Rgb>,
}

impl Lighting {
    pub fn led_list(&self) -> Vec<Led> {
        self.leds
            .iter()
            .map(|(&index, &color)| Led::new(index, color))
            .collect()
    }
}

pub fn path() -> Option<PathBuf> {
    Some(config::config_dir()?.join("lighting.toml"))
}

/// The state in `path`; the default (effect, no colors) if there is none.
pub fn load_from(path: &Path) -> Lighting {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| parse(&text))
        .unwrap_or_default()
}

pub fn save_to(path: &Path, lighting: &Lighting) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text(lighting))
}

/// The state ([`path`]).
pub fn load() -> Lighting {
    path().map(|p| load_from(&p)).unwrap_or_default()
}

pub fn save(lighting: &Lighting) -> std::io::Result<()> {
    match path() {
        Some(p) => save_to(&p, lighting),
        None => Ok(()),
    }
}

/// Change only the mode (the per-key colors stay for later).
pub fn save_mode(mode: Mode) -> std::io::Result<()> {
    let mut lighting = load();
    lighting.mode = mode;
    save(&lighting)
}

fn text(lighting: &Lighting) -> String {
    let mut t = format!(
        "# Written by QuietLinux: what the keyboard showed last. The background\n\
         # service shows per-key colors again after replugging or sleep.\n\
         mode = \"{}\"\n\n[leds]\n",
        lighting.mode.name()
    );
    for (led, color) in &lighting.leds {
        let _ = writeln!(t, "{led:02x} = \"{color}\"");
    }
    t
}

fn parse(text: &str) -> Lighting {
    let Ok(table) = text.parse::<toml::Table>() else {
        return Lighting::default();
    };
    let mode = table
        .get("mode")
        .and_then(toml::Value::as_str)
        .and_then(|name| Mode::NAMES.iter().find(|(_, n)| *n == name))
        .map_or(Mode::Effect, |(m, _)| *m);
    let leds = table
        .get("leds")
        .and_then(toml::Value::as_table)
        .map(|leds| {
            leds.iter()
                .filter_map(|(led, color)| {
                    Some((
                        u8::from_str_radix(led, 16).ok()?,
                        color.as_str()?.parse().ok()?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    Lighting { mode, leds }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join(format!("quietlinux-state-{}", std::process::id()));
        let path = dir.join("lighting.toml");
        assert_eq!(load_from(&path), Lighting::default());
        let lighting = Lighting {
            mode: Mode::PerKey,
            leds: [(0x57, Rgb::new(255, 0, 0)), (0x02, Rgb::new(1, 2, 3))].into(),
        };
        save_to(&path, &lighting).unwrap();
        assert_eq!(load_from(&path), lighting);
        assert_eq!(lighting.led_list().len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn broken_files() {
        assert_eq!(parse("mode = \"sparkle\""), Lighting::default());
        assert_eq!(parse("not toml ["), Lighting::default());
        let odd =
            parse("mode = \"off\"\n[leds]\nzz = \"ff0000\"\n57 = \"nope\"\n2e = \"00ff00\"\n");
        assert_eq!(odd.mode, Mode::Off);
        assert_eq!(odd.leds, [(0x2e, Rgb::new(0, 255, 0))].into());
    }
}
