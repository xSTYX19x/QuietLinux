# Command line usage

```text
quietlinux [--usb-path PATH] [--allow-unknown-firmware] <command>
```

`quietlinux --help` and `quietlinux <command> --help` show every option.

## Global options

| Option | Meaning |
|---|---|
| `--usb-path PATH` | Pick one keyboard when several are plugged in. `PATH` is the USB port path shown by `quietlinux status`, e.g. `1-2` or `3-1.4`. |
| `--allow-unknown-firmware` | Allow writing to a firmware version that was never tested. Without it, such keyboards are read-only. |

## `status`

Reads the keyboard's identity and lighting mode. Sends only read requests.

```console
$ quietlinux status
Light Mount at USB 1-2 (/dev/hidraw6)
Firmware version: 02 00 01 01 00 00 03 02 (tested)
Lighting mode:    effects
```

Lighting modes:

- `effects`: a firmware effect runs (set with `light static`, IO Center or Fn+←/→).
- `custom`: per-LED colors are shown (set with `light set`).

## `zones`

Lists the lighting zones and their vendor LED indexes. Does not touch the keyboard.

```console
$ quietlinux zones
top     45 LEDs  00-2c
knob     1 LEDs  2d-2d
keys   110 LEDs  2e-9d
left     5 LEDs  9e-a2
right    5 LEDs  a3-a7
```

## `light static COLOR [--brightness PERCENT]`

Switches to effects mode and sets a static color. **Stored in the keyboard**:
it survives a replug.

```sh
quietlinux light static ff2a00                  # orange, full brightness
quietlinux light static 00a0ff --brightness 30
```

Colors are 6 hex digits (`rrggbb`, a leading `#` is allowed). Brightness is
0 to 100 (default 100).

## `light set ZONE=COLOR... [--dim FACTOR]`

Switches to custom mode and sets one color per zone. **Volatile**: lost when the
keyboard is unplugged, then it shows its stored effect again.

```sh
quietlinux light set keys=ff2a00 top=f4dbae knob=ffffff
quietlinux light set all=ff0000 knob=00ff00     # later pairs win
quietlinux light set all=ffffff --dim 0.3       # 30 % brightness
quietlinux light set all=000000                 # everything off
```

- Zones: `all`, `top`, `knob`, `keys`, `left`, `right`.
- Zones not given are switched off.
- Custom mode has no brightness setting, so `--dim` (0.0 to 1.0) scales the colors.
- The Fn+←/→ effect keys switch back to effects mode and clear the custom colors.

Tip: too much green looks yellow on this keyboard (`ff6a00` looks yellow,
`ff2a00` orange), and pure white is harsh on the strips.

## Exit status

`0` on success, `1` on any error. Errors go to standard error, e.g.:

| Message | Fix |
|---|---|
| `Light Mount not found` | Keyboard unplugged or another model: `lsusb -d 373f:0002`. |
| `/dev/hidrawN: permission denied` | Install the udev rule, see [installation](installation.md). |
| `untested firmware version …, nothing written` | Please open an issue with the `quietlinux status` output. `--allow-unknown-firmware` forces writing. |
| `several Light Mount keyboards` | Choose one with `--usb-path`. |
| `no reply to 03:01` | Another program (e.g. an IO Center Web tab) is talking to the keyboard: close it and retry. |
