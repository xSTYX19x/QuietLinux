//! What QuietLinux last showed on the keyboard: `$XDG_CONFIG_HOME/quietlinux/lighting.toml`.
//!
//! The keyboard keeps its stored effect, but per-key colors (realtime mode) are
//! gone after it is unplugged or the computer sleeps. The window and the command
//! line write here what they show; the background service shows it again.
//!
//! Per-key colors can also be saved under a name ([`Layouts`]).

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

    /// The mode named `name` (`effect`, `per-key`, `off`).
    pub fn from_name(name: &str) -> Option<Mode> {
        Self::NAMES
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(m, _)| *m)
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
        .and_then(Mode::from_name)
        .unwrap_or_default();
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

/// Longest layout name.
pub const MAX_LAYOUT_NAME: usize = 32;

/// Named per-key color layouts: `$XDG_CONFIG_HOME/quietlinux/layouts/NAME.toml`,
/// each with a `[leds]` table as in `lighting.toml`.
#[derive(Debug, Clone)]
pub struct Layouts {
    dir: PathBuf,
}

impl Layouts {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// `$XDG_CONFIG_HOME/quietlinux/layouts`.
    pub fn standard() -> Option<Self> {
        Some(Self::new(config::config_dir()?.join("layouts")))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.toml"))
    }

    pub fn exists(&self, name: &str) -> bool {
        config::check_name("layout", name, MAX_LAYOUT_NAME).is_ok() && self.path(name).is_file()
    }

    /// The layouts, in alphabetical order.
    pub fn names(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| {
                let name = e.ok()?.file_name().into_string().ok()?;
                let name = name.strip_suffix(".toml")?;
                self.exists(name).then(|| name.to_owned())
            })
            .collect();
        names.sort();
        names
    }

    /// The colors of a layout, by LED index.
    pub fn load(&self, name: &str) -> Result<BTreeMap<u8, Rgb>, String> {
        config::check_name("layout", name, MAX_LAYOUT_NAME)?;
        let path = self.path(name);
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        text.parse::<toml::Table>()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(parse(&text).leds)
    }

    pub fn save(&self, name: &str, leds: &BTreeMap<u8, Rgb>) -> std::io::Result<()> {
        config::check_name("layout", name, MAX_LAYOUT_NAME)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        std::fs::create_dir_all(&self.dir)?;
        let mut t = String::from(
            "# A per-key color layout of QuietLinux (`quietlinux light layout`), by LED\n\
             # index (`quietlinux keys`, `quietlinux zones`); LEDs not listed are off.\n[leds]\n",
        );
        for (led, color) in leds {
            let _ = writeln!(t, "{led:02x} = \"{color}\"");
        }
        std::fs::write(self.path(name), t)
    }

    pub fn delete(&self, name: &str) -> std::io::Result<()> {
        config::check_name("layout", name, MAX_LAYOUT_NAME)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        std::fs::remove_file(self.path(name))
    }
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
    fn layouts() {
        let dir = std::env::temp_dir().join(format!("quietlinux-layouts-{}", std::process::id()));
        let layouts = Layouts::new(&dir);
        assert_eq!(layouts.names(), Vec::<String>::new());
        let leds: BTreeMap<u8, Rgb> =
            [(0x2e, Rgb::new(255, 0, 0)), (0x00, Rgb::new(0, 0, 9))].into();
        layouts.save("wasd", &leds).unwrap();
        layouts
            .save("all-red", &[(0x2e, Rgb::new(255, 0, 0))].into())
            .unwrap();
        assert_eq!(layouts.names(), ["all-red", "wasd"]);
        assert_eq!(layouts.load("wasd").unwrap(), leds);
        assert!(layouts.load("nope").is_err());
        assert!(layouts.save("../x", &leds).is_err());
        std::fs::write(dir.join("broken.toml"), "[leds\n").unwrap();
        assert!(layouts.load("broken").is_err());
        layouts.delete("wasd").unwrap();
        assert_eq!(layouts.names(), ["all-red", "broken"]);
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
