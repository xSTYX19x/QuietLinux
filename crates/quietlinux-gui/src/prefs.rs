//! Settings of the window itself (not of the keyboard): `$XDG_CONFIG_HOME/quietlinux/gui.toml`.

use std::path::PathBuf;

/// Color themes: the index of the theme choice in the window.
pub const THEMES: [&str; 3] = ["system", "light", "dark"];

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("quietlinux").join("gui.toml"))
}

/// The saved theme (index into [`THEMES`]); the system's if none is saved.
pub fn theme() -> i32 {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map_or(0, |text| parse_theme(&text))
}

fn parse_theme(text: &str) -> i32 {
    text.parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("theme")?.as_str().map(str::to_owned))
        .and_then(|name| THEMES.iter().position(|t| *t == name))
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(0)
}

pub fn save_theme(index: i32) -> std::io::Result<()> {
    let name = usize::try_from(index)
        .ok()
        .and_then(|i| THEMES.get(i))
        .copied()
        .unwrap_or(THEMES[0]);
    let Some(path) = path() else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, format!("theme = \"{name}\"\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes() {
        assert_eq!(parse_theme("theme = \"dark\"\n"), 2);
        assert_eq!(parse_theme("theme = \"light\""), 1);
        assert_eq!(parse_theme("theme = \"purple\""), 0);
        assert_eq!(parse_theme("not toml ["), 0);
    }
}
