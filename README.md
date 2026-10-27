# QuietLinux

> [!WARNING]
> **AI notice:** This project is developed with the help of AI (Claude by
> Anthropic). Planning, protocol analysis, code and documentation are wholly or
> partly AI-generated. They are reviewed by the maintainer and tested on real
> hardware, but mistakes are possible. Use at your own risk.

A free Linux tool to configure the **be quiet! Light Mount** keyboard
(USB `373f:0002`) without the vendor's IO Center: lighting, key bindings,
macros and profiles, with a command line tool and (later) a graphical one.

**Status: early development.** What works today is listed below; the rest is
planned in [docs/todo.md](docs/todo.md).

Not affiliated with or endorsed by be quiet!. See [Safety](#safety).

## Features

| Feature | Status |
|---|---|
| Find the keyboard; read firmware, layout, lighting, game mode, quick tap and key bindings (`info`) | ✅ |
| Show the key presses the keyboard reports (`events`) | ✅ |
| Static color effect with brightness (stored in the keyboard) | ✅ |
| Per-LED colors by zone (keys, top bar, knob, side strips) or by key (volatile) | ✅ |
| Key map with layout-independent key names, legends for UK, German and French ISO | ✅ |
| Other effects (wave, breathing, ripple, …) | planned |
| Key remapping, Fn layer, knob, M1–M5 | planned |
| Macros | planned |
| Profiles, game mode, quick tap | planned |
| Graphical interface | planned |
| Firmware updates | **never**, see [Safety](#safety) |

Tested with: Light Mount Silent Linear, German ISO layout, firmware
`02 00 01 01 00 00 03 02`. Other firmware versions are read-only unless you
pass `--allow-unknown-firmware`.

## Quick start

```sh
git clone https://github.com/xSTYX19x/QuietLinux.git
cd QuietLinux
cargo build --release
sudo install -m 644 packaging/udev/70-quietlinux.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
# replug the keyboard, then:
./target/release/quietlinux status
./target/release/quietlinux light set keys=ff2a00 top=f4dbae knob=ffffff
./target/release/quietlinux light stored   # back to the stored effect
```

Details: [installation](docs/installation.md), [command line usage](docs/usage-cli.md).

## Documentation

- [docs/installation.md](docs/installation.md): build, udev rule, permissions
- [docs/usage-cli.md](docs/usage-cli.md): every command with examples
- [docs/protocol.md](docs/protocol.md): what is known about the keyboard's protocol
- [docs/capturing.md](docs/capturing.md): how to record and decode the vendor software's traffic
- [docs/todo.md](docs/todo.md): roadmap

## Safety

- **No firmware update is ever sent**, and no firmware-related frame is replayed.
- Only frames whose format is backed by captures of the vendor software are sent.
- Writing is refused on untested firmware versions unless explicitly forced.
- No root needed: a udev rule gives the logged-in user access to the keyboard's
  vendor and lighting interfaces only. The interfaces that carry your key
  presses stay root-only.
- No network access, no open ports, no background service (yet).
- Per-LED colors are volatile and do not wear the keyboard's flash memory.

## Credits

- [bbossa/keyboard_bequiet_lightmount_rgb](https://github.com/bbossa/keyboard_bequiet_lightmount_rgb)
  decoded the packet format and the lighting commands. QuietLinux's protocol
  layer is a Rust port of that project, and its test fixtures are reused
  unchanged. Thank you!
- [echtzeit-solutions/monsgeek-akko-linux](https://github.com/echtzeit-solutions/monsgeek-akko-linux)
  inspired the structure and the capture-based approach (no code used).

Details in [NOTICE](NOTICE).

## License

[GPL-3.0-or-later](LICENSE).
