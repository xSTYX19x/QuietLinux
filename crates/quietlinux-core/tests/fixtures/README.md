# Test fixtures

Reference frames used by the protocol tests. They need no keyboard.

| File | Content |
|---|---|
| `frames-out.hex` | single-packet frames sent by IO Center (`10:01`, `10:02`, `10:06`) |
| `leds-10-0d.hex` | per-LED messages (`10:0d`), one fragmented message per line |
| `requests-03.hex` | identification requests (`03:01`, `03:02`) |
| `responses-03-01.hex` | firmware version replies |
| `responses-10-01.hex` | lighting mode replies |
| `descriptors/if0..3.bin` | HID report descriptors of the four interfaces |
| `connect-web.hex` | IO Center Web connecting (identification, `01:01` OpenSession, reading the state), requests and replies |

Format of the `.hex` files: one message per line, its 64-byte packets as hex
separated by spaces; `#` starts a comment.

## Origin

`connect-web.hex` comes from a capture of IO Center Web made for QuietLinux
with `tools/webhid-logger.js`, extracted with `qlcap fixtures` (no serial
number reply, no key events). Comments name the direction and the command.

All other files in this directory are copied unchanged from
[bbossa/keyboard_bequiet_lightmount_rgb](https://github.com/bbossa/keyboard_bequiet_lightmount_rgb)
(`tests/fixtures/`, GPL-2.0-or-later). They were extracted from captures of the
vendor software. They contain no serial number.

New fixtures from our own captures must be anonymized first: no `03:02` replies
(serial number) and no key events from real typing.
