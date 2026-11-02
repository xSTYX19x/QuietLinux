# Changelog

All notable changes to QuietLinux. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
- `quietlinux completions bash|zsh|fish`; `install.sh` and the packages install
  the completions
- Settings files may have a `[lighting]` section: the stored effect, per-key
  colors or off
- The command line offers the names of effects, directions, media actions,
  mouse buttons, lighting actions and shortcuts in `--help` and completions

### Changed

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

[Unreleased]: https://github.com/xSTYX19x/QuietLinux/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/xSTYX19x/QuietLinux/releases/tag/v0.1.0
