# Installation

## Requirements

- A be quiet! Light Mount keyboard (USB `373f:0002`). Check with `lsusb -d 373f:0002`.
- Linux with udev and systemd-logind (any current desktop distribution).
- To build: Rust 1.85 or newer for the command line tool, 1.92 or newer for the
  graphical interface (`rustup` or your distribution's `rust`/`cargo` package).
- For the graphical interface also the fontconfig headers and `pkg-config`
  (Debian/Ubuntu: `libfontconfig-dev pkg-config`, Fedora: `fontconfig-devel`,
  Arch: `fontconfig pkgconf`). It runs on Wayland and X11 and needs no GPU.

## Quick install

```sh
git clone https://github.com/xSTYX19x/QuietLinux.git
cd QuietLinux
./install.sh             # command line tool, window, udev rule (asks sudo for it)
./install.sh --service   # the same, and the background service started now and at login
```

It builds the programs, installs them to `~/.local/bin`, adds QuietLinux to the
application menu and installs the [udev rule](#permissions-udev-rule).
`--no-gui` leaves out the window, `--no-udev` the rule, `--prefix DIR` installs
elsewhere. Run it as your normal user, not with `sudo`: it asks sudo itself,
only for the udev rule. `./uninstall.sh` removes everything again (your
settings in `~/.config/quietlinux` stay). The sections below do the same by
hand.

## Build

```sh
git clone https://github.com/xSTYX19x/QuietLinux.git
cd QuietLinux
cargo build --release
```

This builds `target/release/quietlinux` (the command line tool, under 1 MB, no
runtime dependencies), `target/release/quietlinux-gui` (the graphical
interface) and `target/release/quietlinux-daemon` (the background service). Only the command line tool: `cargo build --release -p quietlinux-cli`.
To have them in your `PATH`:

```sh
install -Dm755 target/release/quietlinux ~/.local/bin/quietlinux
install -Dm755 target/release/quietlinux-gui ~/.local/bin/quietlinux-gui
install -Dm755 target/release/quietlinux-daemon ~/.local/bin/quietlinux-daemon
```

## Background service (optional)

`quietlinux-daemon` does what the keyboard cannot do by itself:

- **Per-key colors again.** They are not stored in the keyboard and are gone
  after it is unplugged or the computer sleeps. When the keyboard appears or
  the computer woke up, the service shows what QuietLinux showed last
  (`~/.config/quietlinux/lighting.toml`, written by the window and by
  `quietlinux light set/off/effect`). With an effect it sends nothing: the
  keyboard shows it by itself.
- **Website keys.** Keys bound to "open website" (`quietlinux bind KEY website
  URL`) only report the press; the service opens the address with `xdg-open`
  (without a shell, only `http://` and `https://` addresses, at most one a
  second).

To see those presses, it reads the key events of the keyboard's vendor
interface, which reports **every** key press (see
[Permissions](#permissions-udev-rule)). It drops every press that is not a
website key at once; nothing is stored or logged. It needs the desktop's
`WAYLAND_DISPLAY`/`DISPLAY` in the systemd user environment to start the
browser (desktops set it up; if not: `systemctl --user import-environment
WAYLAND_DISPLAY DISPLAY`).

Install it as a systemd user service:

```sh
install -Dm644 packaging/systemd/quietlinux-daemon.service ~/.config/systemd/user/quietlinux-daemon.service
systemctl --user daemon-reload
systemctl --user enable --now quietlinux-daemon
journalctl --user -u quietlinux-daemon   # what it did
```

`quietlinux-daemon --once` shows the last lighting once and exits.

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
systemctl --user disable --now quietlinux-daemon
rm ~/.config/systemd/user/quietlinux-daemon.service
rm ~/.local/bin/quietlinux ~/.local/bin/quietlinux-gui ~/.local/bin/quietlinux-daemon
sudo rm /etc/udev/rules.d/70-quietlinux.rules
sudo udevadm control --reload
```

The keyboard keeps its stored effect. Per-LED colors are gone after a replug.
