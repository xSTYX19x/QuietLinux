# Command line usage

```text
quietlinux [--usb-path PATH] [--allow-unknown-firmware] [--layout LAYOUT] <command>
```

`quietlinux --help` and `quietlinux <command> --help` show every option.

## Global options

| Option | Meaning |
|---|---|
| `--usb-path PATH` | Pick one keyboard when several are plugged in. `PATH` is the USB port path shown by `quietlinux status`, e.g. `1-2` or `3-1.4`. |
| `--allow-unknown-firmware` | Allow writing to a firmware version that was never tested. Without it, such keyboards are read-only. |
| `--layout LAYOUT` | Keycap legends to show: `uk` (default), `de` or `fr`. Only changes what is displayed. |

## Key names

Keys are named by their **physical position**, with the W3C
[`KeyboardEvent.code`](https://www.w3.org/TR/uievents-code/) names, so the same
name works on every layout: `KeyQ` is the key right of Tab, whether it reads Q
(English, German) or A (French). Keys without a W3C name are `M1`–`M5`, `Fn`
and `Knob`. Names are case-sensitive. `quietlinux keys` lists them all.

## `status`

Reads the keyboard's identity and lighting mode. Sends only read requests.

```console
$ quietlinux status
Light Mount at USB 1-2 (/dev/hidraw6)
Firmware version: 02 00 01 01 00 00 03 02 (tested)
Lighting mode:    effect
```

Lighting modes:

- `effect`: the effect stored in the keyboard runs (set with `light static`, IO Center or Fn+←/→).
- `realtime`: per-LED colors are shown (set with `light set`), until the keyboard is unplugged.
- `custom`: stored per-key colors (set with IO Center; not supported by QuietLinux yet).
- `off`: lighting off.

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

## `keys`

Lists every key with its LED index, name and legend. Does not touch the keyboard.

```console
$ quietlinux keys --layout de
LED  NAME            LEGEND (de)
2d   Knob            Knob
2e   Escape          Esc
…
75   Semicolon       Ö
76   Quote           Ä
77   Backslash       # '
…
```

## `light static COLOR [--brightness PERCENT]`

Switches to effect mode and sets a static color. **Stored in the keyboard**:
it survives a replug.

```sh
quietlinux light static ff2a00                  # orange, full brightness
quietlinux light static 00a0ff --brightness 30
```

Colors are 6 hex digits (`rrggbb`, a leading `#` is allowed). Brightness is
0 to 100 (default 100).

## `light set TARGET=COLOR... [--dim FACTOR]`

Switches to realtime mode and sets colors per zone or per key. **Volatile**: lost
when the keyboard is unplugged, then it shows its stored effect again.

```sh
quietlinux light set keys=ff2a00 top=f4dbae knob=ffffff
quietlinux light set all=ff0000 knob=00ff00     # later pairs win
quietlinux light set keys=202020 KeyW=00ff00 KeyA=00ff00 KeyS=00ff00 KeyD=00ff00
quietlinux light set all=ffffff --dim 0.3       # 30 % brightness
quietlinux light set all=000000                 # everything off
```

- Targets: the zones `all`, `top`, `knob`, `keys`, `left`, `right`, or a
  [key name](#key-names).
- LEDs not given are switched off.
- Realtime mode has no brightness setting, so `--dim` (0.0 to 1.0) scales the colors.
- The Fn+←/→ effect keys switch back to effect mode and clear the colors.

Tip: too much green looks yellow on this keyboard (`ff6a00` looks yellow,
`ff2a00` orange), and pure white is harsh on the strips.

## `light stored`

Leaves realtime mode: the effect stored in the keyboard shows again. Use it to
undo `light set` without unplugging the keyboard.

## `calibrate [--start HEX]`

Lights the knob and the keys one at a time, everything else off, and shows the
name and legend of the key that should be lit. Use it to check the key map on
your keyboard, or to contribute the legends of another layout.

```console
$ quietlinux calibrate --layout de
111 keys, one at a time (de legends). Enter = the lit key is the one shown, otherwise type what is lit; q = stop.
2d  Knob            Knob
2e  Escape          Esc
2f  F1              F1
…
111 checked, 0 mismatch(es)
```

- **Enter**: the lit key is the one shown.
- Anything else, then Enter: the lit key is a different one; type what you see.
- **q** (or Ctrl+D): stop.
- `--start 6a` resumes at an LED index.

At the end the previous lighting mode is restored. If the keyboard was in
realtime mode before, its colors cannot be read back: set them again with `light set`.
Exit status `1` if there was any mismatch.

## Exit status

`0` on success, `1` on any error. Errors go to standard error, e.g.:

| Message | Fix |
|---|---|
| `Light Mount not found` | Keyboard unplugged or another model: `lsusb -d 373f:0002`. |
| `/dev/hidrawN: permission denied` | Install the udev rule, see [installation](installation.md). |
| `untested firmware version …, nothing written` | Please open an issue with the `quietlinux status` output. `--allow-unknown-firmware` forces writing. |
| `several Light Mount keyboards` | Choose one with `--usb-path`. |
| `no reply to 03:01` | Another program (e.g. an IO Center Web tab) is talking to the keyboard: close it and retry. |
| `… failed: BUSY` (or another status name) | The keyboard refused the request; the name says why (see [protocol.md](protocol.md#status-codes)). |
