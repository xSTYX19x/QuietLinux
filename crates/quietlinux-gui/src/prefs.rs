//! What the window remembers between starts: `$XDG_CONFIG_HOME/quietlinux/gui.toml`.
//!
//! The theme, and the per-key colors last painted (the keyboard does not keep
//! them). The last effect is in `last-effect.toml`, shared with the command line
//! (`quietlinux_core::config::last_effect`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use quietlinux_core::config;

/// Color themes: the index of the theme choice in the window.
pub const THEMES: [&str; 3] = ["system", "light", "dark"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prefs {
    /// Index into [`THEMES`].
    pub theme: i32,
    /// Painted keys: key name → color (hex).
    pub keys: BTreeMap<String, String>,
    /// Colors of the top bar and the left and right strips (hex, or empty: off).
    pub top: String,
    pub left: String,
    pub right: String,
}

fn path() -> Option<PathBuf> {
    Some(config::config_dir()?.join("gui.toml"))
}

pub fn load() -> Prefs {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

pub fn save(prefs: &Prefs) -> std::io::Result<()> {
    let Some(path) = path() else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text(prefs))
}

/// Only hex digits and key names are written: no quoting needed beyond `"…"`.
fn clean(s: &str) -> String {
    s.chars().filter(char::is_ascii_alphanumeric).collect()
}

fn text(prefs: &Prefs) -> String {
    let theme = usize::try_from(prefs.theme)
        .ok()
        .and_then(|i| THEMES.get(i))
        .copied()
        .unwrap_or(THEMES[0]);
    let mut t = format!("# Written by quietlinux-gui.\ntheme = \"{theme}\"\n\n[per_key]\n");
    for (name, value) in [
        ("top", &prefs.top),
        ("left", &prefs.left),
        ("right", &prefs.right),
    ] {
        let _ = writeln!(t, "{name} = \"{}\"", clean(value));
    }
    t.push_str("\n[per_key.keys]\n");
    for (key, color) in &prefs.keys {
        let _ = writeln!(t, "{} = \"{}\"", clean(key), clean(color));
    }
    t
}

fn parse(text: &str) -> Prefs {
    let Ok(table) = text.parse::<toml::Table>() else {
        return Prefs::default();
    };
    let string = |t: &toml::Table, key: &str| {
        t.get(key)
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_default()
    };
    let theme = THEMES
        .iter()
        .position(|t| *t == string(&table, "theme"))
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(0);
    let per_key = table.get("per_key").and_then(toml::Value::as_table);
    let keys = per_key
        .and_then(|p| p.get("keys"))
        .and_then(toml::Value::as_table)
        .map(|keys| {
            keys.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    let zone = |name: &str| per_key.map(|p| string(p, name)).unwrap_or_default();
    Prefs {
        theme,
        keys,
        top: zone("top"),
        left: zone("left"),
        right: zone("right"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let prefs = Prefs {
            theme: 2,
            keys: [("KeyW".to_owned(), "e63000".to_owned())].into(),
            top: "f4dbae".to_owned(),
            left: String::new(),
            right: "0000ff".to_owned(),
        };
        assert_eq!(parse(&text(&prefs)), prefs);
    }

    #[test]
    fn old_and_broken_files() {
        assert_eq!(parse("theme = \"dark\"\n").theme, 2);
        assert_eq!(parse("theme = \"purple\"").theme, 0);
        assert_eq!(parse("not toml ["), Prefs::default());
    }
}
