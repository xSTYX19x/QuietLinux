# Troubleshooting

Start with `quietlinux status`: it says whether the keyboard is found, which
firmware it has and whether QuietLinux may write to it.

## The keyboard is not found

- Check that Linux sees it: `lsusb -d 373f:0002` must print a line. If not,
  try another cable or port (a hub can be the problem).
- Several Light Mounts: `quietlinux status` lists their USB paths; choose one
  with `--usb-path PATH` (all programs take it).

## "Permission denied"

The keyboard's vendor interface (`/dev/hidraw*`) is root-only until the udev
rule is installed:

- Install it (`./install.sh` does) and **replug the keyboard** or log out and
  in. Check with `ls -l /dev/hidraw*` and `getfacl /dev/hidrawN`: your user
  needs an ACL entry.
- The rule gives access to the user **logged in at the computer** (`uaccess`).
  Over SSH, or from a system service, there is no access: run QuietLinux in
  your desktop session.
- Another rule may block it: a rule with `MODE:="0000"` (sometimes added for
  other tools) cannot be overridden. Look for be quiet! rules with
  `grep -l -i 373f /etc/udev/rules.d/* /usr/lib/udev/rules.d/*`.
  Remove rules with `MODE="0666"` too: they open the keyboard (and what you
  type on it) to every process.

## "read-only: firmware not tested"

QuietLinux writes only to firmware versions it was tested with (2.3.0 so far):
an untested version may answer differently. Reading works. If you want to help,
open an issue with the output of `quietlinux status` and `quietlinux info`
(see [contributing.md](contributing.md)). `--allow-unknown-firmware` writes
anyway, at your own risk.

## Changes do not stick, or are undone

- **IO Center** (Windows) writes its own profile to the keyboard every time it
  starts, and **IO Center Web** may change settings while it is open. Do not
  use them at the same time as QuietLinux.
- **Per-key colors** are not stored in the keyboard: unplugging it, sleep or
  applying an effect ends them. The background service shows them again
  ([installation](installation.md#background-service-optional)).
- On Windows, **Dynamic Lighting** takes over the lighting; it has no effect on
  Linux.

## The window shows another effect than the keyboard

The Light Mount does not report its effect. The window shows the last effect
QuietLinux set (`~/.config/quietlinux/last-effect.toml`); an effect set with IO
Center is unknown to it. Choose the effect again to have both match.

## The window does not start

- It needs Wayland or X11 (a desktop session). Run `quietlinux-gui` in a
  terminal to see the error.
- Building it needs Rust 1.92 or newer and the fontconfig headers
  ([installation](installation.md#requirements)); `./install.sh` skips the
  window if they are missing and says so.
- Try `quietlinux-gui --demo`: it uses a simulated keyboard. If that works, the
  problem is the access to the keyboard (see above).

## The background service

- Its log: `journalctl --user -u quietlinux-daemon`. It never logs key presses.
- **A website key does nothing:** the service must be running
  (`systemctl --user status quietlinux-daemon`), `xdg-open` must be installed,
  and the systemd user environment needs `WAYLAND_DISPLAY` or `DISPLAY`
  (`systemctl --user show-environment`; if missing:
  `systemctl --user import-environment WAYLAND_DISPLAY DISPLAY`).
- **Colors do not come back after sleep:** check the log for a line after
  waking up; please report it if there is none.

## Recording a macro records nothing

Type on the Light Mount itself, not on another keyboard. If no event arrives
at all, please open an issue with your firmware version.

## Back to the factory settings

- A key: `quietlinux bind KEY default` (or "Factory binding" in the window).
  All keys: apply a backup file with an empty `[bindings]` section.
- QuietLinux has no factory reset: it never sends firmware or reset commands.
  IO Center can reset the keyboard.

## Reporting a problem

Open an issue on GitHub with the output of `quietlinux status` and, if it
works, `quietlinux info`, your distribution and what you did. Do not attach
backup files or raw captures: they contain what your macros type, or what you
typed.
