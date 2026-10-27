# QuietLinux

> [!WARNING]
> **AI notice:** This project is developed with the help of AI (Claude by
> Anthropic). Planning, protocol analysis, code and documentation are wholly or
> partly AI-generated. They are reviewed by the maintainer and tested on real
> hardware, but mistakes are possible. Use at your own risk.

A free Linux tool to configure the **be quiet! Light Mount** keyboard
(USB `373f:0002`) without the vendor's IO Center: lighting, key bindings,
macros and backups, with a command line tool, a graphical one and an optional
background service.

**Status: first version.** Everything below works on the reference keyboard;
what is still open is in [docs/todo.md](docs/todo.md).

Not affiliated with or endorsed by be quiet!. See [Safety](#safety).

## Features

| Feature | Status |
|---|---|
| Find the keyboard; read firmware, layout, lighting, game mode, quick tap and key bindings (`info`) | ✅ |
| Show the key presses the keyboard reports (`events`) | ✅ |
| Effects (static, color wave, tornado, breathing, reactive, matrix) with color or gradient, brightness, speed, direction (stored in the keyboard); lighting off | ✅ |
| Key bindings on both layers (normal and Fn) and the knob: keys and combinations, media, mouse buttons, lighting actions, macros, websites, disable, reset | ✅ |
| Game mode: which shortcuts it blocks | ✅ |
| Backup and restore of bindings, game mode and macros as a TOML file (`backup`, `apply`) | ✅ |
| Background service (`quietlinux-daemon`): per-key colors shown again after replugging or sleep; website keys | ✅ |
| Per-LED colors by zone (keys, top bar, knob, side strips) or by key (volatile) | ✅ |
| Key map with layout-independent key names, legends for UK, German and French ISO | ✅ |
| Macros stored in the keyboard: record by typing, list, show, create, edit, delete (`macro`) | ✅ |
| Keys that open files or folders, profile switching keys | planned |
| Graphical interface (`quietlinux-gui`): lighting with a color wheel, per-key colors, key bindings, macros, backup, game mode; demo mode with a simulated keyboard | ✅ first version |
| Firmware updates | **never**, see [Safety](#safety) |

Tested with: Light Mount Silent Linear, German ISO layout, firmware 2.3.0.
Other firmware versions are read-only unless you pass
`--allow-unknown-firmware`; a [device report](docs/contributing.md#device-reports)
helps to support them.

## Quick start

```sh
git clone https://github.com/xSTYX19x/QuietLinux.git
cd QuietLinux
./install.sh --service     # builds and installs for your user; asks sudo for the udev rule
# replug the keyboard, then:
quietlinux status
quietlinux-gui             # or "QuietLinux" in the application menu
```

Details: [installation](docs/installation.md), [command line usage](docs/usage-cli.md),
[graphical interface](docs/usage-gui.md).

![The keys tab of quietlinux-gui](docs/images/gui-keys.png)

## Documentation

- [docs/installation.md](docs/installation.md): install script, build, udev rule, permissions, background service
- [docs/usage-cli.md](docs/usage-cli.md): every command with examples
- [docs/usage-gui.md](docs/usage-gui.md): the graphical interface
- [docs/protocol.md](docs/protocol.md): what is known about the keyboard's protocol
- [docs/troubleshooting.md](docs/troubleshooting.md): when something does not work
- [docs/capturing.md](docs/capturing.md): how to record and decode the vendor software's traffic
- [docs/contributing.md](docs/contributing.md): device reports, captures, code
- [docs/todo.md](docs/todo.md): roadmap

## Safety

- **No firmware update is ever sent**, and no firmware-related frame is replayed.
- Only frames whose format is backed by captures of the vendor software are sent.
- Writing is refused on untested firmware versions unless explicitly forced.
- No root needed: a udev rule gives only the user logged in at the computer
  (not other accounts or system services) access to the keyboard's vendor and
  lighting interfaces. The standard keyboard interfaces stay root-only.
  **Note:** the keyboard also reports every key press on its vendor interface,
  so programs running as that user can read what is typed on the Light Mount.
  See [installation](docs/installation.md#permissions-udev-rule).
- QuietLinux makes no network connections and opens no ports. The optional
  background service shows the last per-key colors again after replugging or
  sleep and opens the websites bound to keys in your browser; it reads the key
  events for that and drops all others.
- Per-LED colors are volatile and do not wear the keyboard's flash memory.

## Credits

- [bbossa/keyboard_bequiet_lightmount_rgb](https://github.com/bbossa/keyboard_bequiet_lightmount_rgb)
  decoded the packet format and the lighting commands. QuietLinux's protocol
  layer is a Rust port of that project, and its test fixtures are reused
  unchanged. Thank you!
- [echtzeit-solutions/monsgeek-akko-linux](https://github.com/echtzeit-solutions/monsgeek-akko-linux)
  inspired the structure and the capture-based approach (no code used).
- The graphical interface is built with [Slint](https://slint.dev), used under
  its GPL-3.0 license option.

Details in [NOTICE](NOTICE).

## License

[GPL-3.0-or-later](LICENSE). The `quietlinux-gui` binary includes Slint
(GPL-3.0-only option) and is therefore distributed under GPL-3.0.
