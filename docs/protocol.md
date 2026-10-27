# Light Mount protocol

What is known about how the vendor software talks to the be quiet! Light Mount
(USB `373f:0002`). Everything here comes from captures of IO Center (Windows)
and IO Center Web (WebHID), checked by replay.

Sources are marked: **[bbossa]** found by
[bbossa/keyboard_bequiet_lightmount_rgb](https://github.com/bbossa/keyboard_bequiet_lightmount_rgb),
**[QL]** found by this project.

## USB interfaces

| # | Usage page / usage | Endpoints | Content |
|---|---|---|---|
| 0 | `0x01/0x06` keyboard (boot) | IN 8 B | key presses |
| 1 | `0x01/0x06` keyboard (ID 1), `0x01/0x80` system control (ID 3), `0x0c/0x01` consumer (ID 4), `0x01/0x02` mouse (ID 10) | IN 24 B | key presses, media keys, mouse actions |
| 2 | `0xff00/0x01` vendor | IN 64 B, OUT 64 B | **vendor protocol**, described below |
| 3 | `0x59/0x01` LampArray (IDs 1–6) | IN 4 B | standard HID lighting (Windows Dynamic Lighting) |

[bbossa], interface 1 details [QL].

## Packets [bbossa]

Every report on interface 2 is 64 bytes, without report ID (on Linux, writes
to hidraw are prefixed with a `00` byte).

```text
offset  0     1      2        3 .. 61                 62 .. 63
        len   idx    session  body                    CRC-16/MODBUS (little endian)
```

- `len`: index of the last meaningful byte (= 2 + body length).
- `idx`: fragment number in bits 0–6; bit 7 is set when more fragments follow.
- `session`: `00` before a session is opened (identification), `01` afterwards.
  IO Center opens one with `01:01`; `01` is accepted without it.
- CRC-16/MODBUS (poly `0x8005` reflected, init `0xffff`) over bytes 0–61.

The body of the **first fragment** starts with a 4-byte header:

```text
[00][seq][cmd][sub][data ...]
```

- `seq`: one byte, incremented per message; replies echo it.
- `cmd:sub`: the command, written `10:0d` in this document.

Data capacity: 55 bytes in the first fragment, 59 in each following one
(continuation fragments carry data right after the session byte).

A reply has the same `seq` and `cmd:sub` as its request.

## Commands

| Command | Direction | Meaning | Source |
|---|---|---|---|
| `01:01` | → | open session | [bbossa], format not yet documented |
| `03:01` | → ← | firmware version: reply data is 8 bytes, e.g. `02 00 01 01 00 00 03 02` | [bbossa] |
| `03:02` | → ← | serial number: reply data `[length][ASCII…]` | [bbossa] |
| `10:01` | → ← | read lighting mode: reply data `[mode]` | [bbossa] |
| `10:02` | → | set lighting mode `[mode]`; **no reply** with the same seq | [bbossa] |
| `10:06` | → ← | lighting effect, stored in the keyboard | [bbossa], only static decoded |
| `10:0d` | → | per-LED colors, volatile | [bbossa] |
| `11:02` | ← | key event, sent by the keyboard on its own | [bbossa] |

Lighting modes: `01` effects (`10:06` applies), `03` custom (`10:0d` applies).
Switching to custom mode clears every LED.

### `10:06` lighting effect

```text
[00][effect][00][brightness][speed][color mode][R][G][B]
```

| Field | Known values |
|---|---|
| effect | `00` static |
| brightness | 0–100 (captured: 10, 50, 100) |
| speed | `32` (default) |
| color mode | `00` single color |

Other effects, speeds and color modes: to be captured.

### `10:0d` per-LED colors

```text
[N] N × ([03][index][00][R][G][B]) [00 only in the last message]
```

- `N`: number of entries, **+1 in the last message** of an update.
- IO Center sends 40 entries per message, `seq` incremented per message.

LED indexes:

| Indexes | Zone |
|---|---|
| `00`–`2c` | top light bar (45) |
| `2d` | knob |
| `2e`–`9d` | keys, row by row, left to right (`62` and `89` do not exist) |
| `9e`–`a2` | left side strip |
| `a3`–`a7` | right side strip |

Which key each LED belongs to is in `crates/quietlinux-core/src/keymap.rs` [bbossa]:
the key LEDs are numbered row by row like the LampArray lamps. French and
German ISO keyboards share the same physical layout, so the mapping holds for
both; `quietlinux calibrate` checks it on a keyboard (German ISO: 111 of 111
keys confirmed [QL]).

### `11:02` key events

```text
plain key: [vendor key index][00][01][01][00][HID usage]
Fn layer:  [vendor key index][80][01][function] ...
```

Known Fn functions: `09` lighting effect change (Fn+←/→), `1b` Fn alone.

## Unknown so far

Effects other than static, key remapping, Fn layer, knob, M1–M5, macros,
profiles, game mode, quick tap, LED calibration, factory reset, the
`01:01` session format and the LampArray interface. See [todo.md](todo.md).
