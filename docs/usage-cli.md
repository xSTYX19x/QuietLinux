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
| `--layout LAYOUT` | Keycap legends to show: `uk`, `de` or `fr`. Default: the layout the keyboard reports (`events`, `calibrate`), otherwise `uk`. Only changes what is displayed. |

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

## `info`

Opens a session like IO Center does (`01:01`, changes no setting), then reads
everything the keyboard reports, with read-only requests only:

```console
$ quietlinux info
Light Mount:    USB 1-2 (/dev/hidraw6)
Session:        2 (timeout 3)
Model:          Light Mount (0x0002), revision 1
Firmware:       2.3.0 (tested)
QLink:          …
Features:       root v1, device info v1, keyboard v1, lighting v1, USB device v1, bindings v1
Layout:         ISO, German
Polling rate:   1000 Hz
Lighting mode:  effect
Layers:         …
Global layers:  …
Layer 0:        static, colors e63000, brightness 100, speed 50, direction up
Calibration:    …
Game mode:      off, blocks nothing
Quick tap:      no pairs
Key bindings:   on, 15 of 15 decoded
  M1                   media: play/pause
  KnobLeft             media: volume down
  Fn+ArrowUp           lighting: brighter
  …
Macros (raw):   …
```

(Values above are examples.) A line reads `unavailable (…)` when the keyboard
refuses or ignores a request; please report such lines in an issue together
with the firmware version.

## `events [--count N]`

Shows the key presses the keyboard reports on its vendor interface, with the
key name, the legend in brackets, the key ID and the action (the HID usage the
key sends, named like on a US keyboard), until Ctrl+C (or `N` events). The
keyboard reports every key press, no releases. Useful to check key
IDs and to see what the keyboard sends for special keys.

```console
$ quietlinux events
Press keys on the Light Mount (Ctrl+C to stop). Only keys the keyboard reports appear.
KeyY [Z] (28)            key Y                                [28 00 01 01 00 1c]
Fn+ArrowRight [→] (5c)   lighting: previous effect                [5c 80 01 09 03]
```

It shows what you type on the Light Mount while it runs: do not type anything
private. Nothing is stored or sent anywhere.

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
