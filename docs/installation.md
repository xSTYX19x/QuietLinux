# Installation

## Requirements

- A be quiet! Light Mount keyboard (USB `373f:0002`). Check with `lsusb -d 373f:0002`.
- Linux with udev and systemd-logind (any current desktop distribution).
- To build: Rust 1.85 or newer for the command line tool, 1.92 or newer for the
  graphical interface (`rustup` or your distribution's `rust`/`cargo` package).
- For the graphical interface also the fontconfig headers and `pkg-config`
  (Debian/Ubuntu: `libfontconfig-dev pkg-config`, Fedora: `fontconfig-devel`,
  Arch: `fontconfig pkgconf`). It runs on Wayland and X11 and needs no GPU.

## Build

```sh
git clone https://github.com/xSTYX19x/QuietLinux.git
cd QuietLinux
cargo build --release
```

This builds `target/release/quietlinux` (the command line tool, under 1 MB, no
runtime dependencies) and `target/release/quietlinux-gui` (the graphical
interface). Only the command line tool: `cargo build --release -p quietlinux-cli`.
To have them in your `PATH`:

```sh
install -Dm755 target/release/quietlinux ~/.local/bin/quietlinux
install -Dm755 target/release/quietlinux-gui ~/.local/bin/quietlinux-gui
```

## Permissions (udev rule)

By default only root may open the keyboard's raw HID devices (`/dev/hidraw*`).
The rule in [`packaging/udev/70-quietlinux.rules`](../packaging/udev/70-quietlinux.rules)
gives the user logged in at the computer (not every account) access to:

| Interface | Content | Access |
|---|---|---|
| 0 | boot keyboard (key presses) | root only |
| 1 | keyboard, media keys, mouse (key presses) | root only |
| 2 | vendor protocol (lighting, key bindings, …); **also reports every key press** | logged-in user |
| 3 | LampArray (standard lighting interface) | logged-in user |

What this means: other accounts and system services cannot touch the
keyboard, but programs running as **you** can read what you type on the Light
Mount through interface 2, because the keyboard reports every key press there.
On X11 such programs can read your keyboard anyway; on Wayland this is a
real, if small, loss of isolation. The same holds for IO Center Web and any
other tool that uses the vendor interface. A future QuietLinux background
service may keep this access to itself (see [todo.md](todo.md)).

Install it:

```sh
sudo install -m 644 packaging/udev/70-quietlinux.rules /etc/udev/rules.d/
sudo udevadm control --reload
sudo udevadm trigger --subsystem-match=hidraw
```

Then replug the keyboard (or log out and in). Check:

```sh
quietlinux status
```

### Rules from other tools

Some guides suggest a rule with `MODE="0666"` for be quiet! devices. **Remove
it:** it lets every process on the system read all four interfaces, including
the ones that carry your key presses. Look for such rules with:

```sh
grep -l -i 373f /etc/udev/rules.d/* /usr/lib/udev/rules.d/* 2>/dev/null
```

A rule that sets `MODE:="0000"` (sometimes added to stop the browser or another
tool from grabbing the keyboard) blocks QuietLinux as well: `:=` cannot be
overridden by later rules.

IO Center Web (WebHID in Chrome/Chromium) uses the same vendor interface, so
the QuietLinux rule is enough for it too. Do not run both at the same time:
whichever writes last wins.

## Uninstall

```sh
rm ~/.local/bin/quietlinux ~/.local/bin/quietlinux-gui
sudo rm /etc/udev/rules.d/70-quietlinux.rules
sudo udevadm control --reload
```

The keyboard keeps its stored effect. Per-LED colors are gone after a replug.
