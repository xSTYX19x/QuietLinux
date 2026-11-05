# Changelog

All notable changes to QuietLinux. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/).

## [0.2.1] - unreleased

Updates of two libraries the window is built with, one of them for a
security advisory. QuietLinux itself is unchanged.

### Security

- pulldown-cmark, a Markdown parser that the window's toolkit Slint compiles
  in, is updated from 0.13.5 to 0.13.6: it fixes
  [RUSTSEC-2026-0433](https://rustsec.org/advisories/RUSTSEC-2026-0433.html),
  slow (quadratic) parsing of nested image wikilinks. QuietLinux was not
  affected in practice: the window uses no Markdown text, and Slint parses
  Markdown without wikilinks. Only the window (`quietlinux-gui`) contains the
  crate; the command line and the background service do not

### Changed

- async-executor, part of the D-Bus library (zbus) the window uses for desktop
  settings and the file chooser, is updated from 1.14.0 to 1.14.1: its authors
  withdrew (yanked) 1.14.0, and the dependency check refuses withdrawn releases

## [0.2.0] - 2026-11-02

Profiles, the import of IO Center for Windows profiles and a live preview of
the effect. Tested with the Light Mount Silent Linear, German ISO layout,
firmware 2.3.0.

### Added

- **Profiles**: named sets of key bindings, macros, game mode shortcuts, effect
  and lighting, kept in `~/.config/quietlinux/profiles` (the keyboard holds
  none). `quietlinux profile list|save|switch|delete`; switching saves the
  keyboard's settings to the active profile first and writes only what differs
- Profile keys: `quietlinux bind KEY profile next|previous|NAME` (also in the
  window's key editor); the background service switches the profile
- The window: the active profile in the title line, profiles in the settings
- The window: a live preview of the effect on the effect page (animated in the
  window only; nothing is sent to the keyboard)
- Import of IO Center for Windows profiles: `quietlinux profile import FILE`
  (its backup `.zip` or an `.ioprofile`) and "Import from IO Center …" in the
  window. Key, media, lighting and macro bindings, macros, the static and
  reactive effects; what is not known yet is listed (see docs/iocenter.md)
- The window: the desktop's file chooser (XDG desktop portal) for backups
  ("Choose …", "Save as …") and the import
- Per-key color layouts saved under a name: `quietlinux light layout
  list|save|load|delete`, and in the window's per-key page
- `--json` for status, info, keys, zones, macro list/show, profile list and
  light layout list
- `qlcap diff A B`: what differs between two captures in which one setting
  was changed (for contributors); `qlcap` also takes folders of captures
- The parsers are fuzzed: random and damaged input in `cargo test`,
  cargo-fuzz targets (`fuzz/`) and a weekly CI job
- `quietlinux completions bash|zsh|fish`; `install.sh` and the packages install
  the completions
- Settings files may have a `[lighting]` section: the stored effect, per-key
  colors or off
- The command line offers the names of effects, directions, media actions,
  mouse buttons, lighting actions and shortcuts in `--help` and completions

### Changed

- Static has one color and no speed, as on the keyboard: two colors, a
  gradient and a speed are refused (command line, settings files) or greyed
  out (window); an older file's speed for static is left out
- `apply` remembers the lighting it sets, for the window and the background
  service

## [0.1.0] - 2026-10-28

The first version. Tested with the Light Mount Silent Linear, German ISO
layout, firmware 2.3.0; other firmware versions are read-only unless
`--allow-unknown-firmware` is given.

### Added

- **Command line tool** `quietlinux`:
  - `status`, `info`, `events`, `zones`, `keys`, `calibrate` (read-only, or
    restoring the lighting afterwards)
  - Lighting: the effects static, color wave, tornado, breathing, reactive and
    matrix with colors (one, two or a gradient), brightness, speed and direction;
    lighting off and on; per-key and light bar colors (`light set`)
  - Key bindings for every key on both layers, M1–M5 and the knob (press, turn
    left, turn right): keys and combinations, media, mouse buttons, lighting
    actions, macros, websites, disable, factory binding
  - Macros stored in the keyboard: list, show, create, edit, delete, and record
    them by typing (pauses: none, fixed or as recorded); every write is read
    back and compared
  - Game mode: the shortcuts it blocks
  - `backup` and `apply`: key bindings, game mode shortcuts and macros as a
    TOML file, with an optional effect; `apply --dry-run` shows the changes
- **Window** `quietlinux-gui`: lighting (effect and per-key colors with a color
  wheel, changes shown at once), key bindings on a keyboard view, macros with
  recording, game mode, backup and apply; light, dark or the system's theme;
  `--demo` with a simulated keyboard
- **Background service** `quietlinux-daemon` (optional, systemd user service):
  shows per-key colors again after replugging or sleep, and opens the websites
  of website keys
- The last effect and the per-key colors are remembered
  (`~/.config/quietlinux`), as the keyboard does not report them
- `install.sh` and `uninstall.sh`; udev rule giving the logged-in user access
  to the keyboard's vendor and LampArray interfaces only
- Documentation: installation, command line, window, troubleshooting,
  contributing, and the keyboard's protocol (`docs/protocol.md`)
- Capture tools for contributors: `qlcap`, a WebHID logger, a Windows capture
  script

### Security

- Runs without root; no network access; never sends firmware updates
- Backups are readable only by their owner (they contain what macros type)
- Website keys open only `http(s)` addresses, without a shell, at most one a
  second

[0.2.0]: https://github.com/xSTYX19x/QuietLinux/releases/tag/v0.2.0
[0.1.0]: https://github.com/xSTYX19x/QuietLinux/releases/tag/v0.1.0
