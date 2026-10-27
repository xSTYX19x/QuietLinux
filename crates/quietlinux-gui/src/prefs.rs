//! What the window remembers between starts: `$XDG_CONFIG_HOME/quietlinux/gui.toml`.
//!
//! Only the theme. The last effect is in `last-effect.toml` (shared with the
//! command line), the per-key colors in `lighting.toml` (shared with the
//! background service).

use std::path::PathBuf;

use quietlinux_core::config;

/// Color themes: the index of the theme choice in the window.
pub const THEMES: [&str; 3] = ["system", "light", "dark"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prefs {
    /// Index into [`THEMES`].
    pub theme: i32,
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

fn text(prefs: &Prefs) -> String {
    let theme = usize::try_from(prefs.theme)
        .ok()
        .and_then(|i| THEMES.get(i))
        .copied()
        .unwrap_or(THEMES[0]);
    format!("# Written by quietlinux-gui.\ntheme = \"{theme}\"\n")
}

fn parse(text: &str) -> Prefs {
    let theme = text
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("theme")?.as_str().map(str::to_owned))
        .and_then(|name| THEMES.iter().position(|t| *t == name))
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(0);
    Prefs { theme }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let prefs = Prefs { theme: 2 };
        assert_eq!(parse(&text(&prefs)), prefs);
    }

    #[test]
    fn old_and_broken_files() {
        // gui.toml of an earlier version, with per-key colors: only the theme counts
        assert_eq!(parse("theme = \"dark\"\n[per_key]\ntop = \"\"\n").theme, 2);
        assert_eq!(parse("theme = \"purple\"").theme, 0);
        assert_eq!(parse("not toml ["), Prefs::default());
    }
}
