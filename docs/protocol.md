# Light Mount protocol

What is known about how the vendor software talks to the be quiet! Light Mount
(USB `373f:0002`). IO Center Web calls the protocol **QLink**.

## Sources

Every finding is marked with its source:

| Mark | Source | Trust |
|---|---|---|
| **[cap]** | captures of IO Center (Windows or web), replayed on the keyboard: by [bbossa](https://github.com/bbossa/keyboard_bequiet_lightmount_rgb) or this project | verified |
| **[web]** | names, numbers and message layouts read from the JavaScript that IO Center Web serves to every browser (`iocenter.bequiet.com`, bundle `index-Zx3Q0vHz.js`, read 2026-10-27) | documented by the vendor's own client, **not yet verified on the keyboard** |

The web source was read only to learn the protocol, for interoperability. No
code from it is part of this project.

QuietLinux sends **[web]**-only messages only if they are read-only requests
(the list `READ_ONLY` in `crates/quietlinux-core/src/protocol.rs`; they cannot
change the keyboard, and the library refuses anything else through that path).
Every request that changes the keyboard is first captured from IO Center and
turned into a test fixture (see [capturing.md](capturing.md)).

## USB interfaces [cap]

| # | Usage page / usage | Endpoints | Content |
|---|---|---|---|
| 0 | `0x01/0x06` keyboard (boot) | IN 8 B | key presses |
| 1 | keyboard (report 1), system control (3), consumer (4), mouse (10) | IN 24 B | key presses, media keys, mouse actions |
| 2 | `0xff00/0x01` vendor | IN 64 B, OUT 64 B | **QLink**, described below |
| 3 | `0x59/0x01` LampArray (reports 1–6) | IN 4 B | standard HID lighting (Windows Dynamic Lighting) |

## Packets

Every report on interface 2 is 64 bytes, without report ID (on Linux, writes to
hidraw are prefixed with a `00` byte).

```text
byte   0        1         2         3        4           5         6         7 .. 61   62 .. 63
       length   sequence  session   status   request ID  feature   command   data      CRC
```

| Field | Meaning | Source |
|---|---|---|
| length | index of the last meaningful byte: 6 + data length in a first fragment | [cap] [web] |
| sequence | fragment number (bits 0–6); bit 7 set when more fragments follow; 0 for single-packet messages | [cap] [web] |
| session | `00` before a session is opened, then the session ID from `01:01`; `01` is accepted without opening one | [cap] |
| status | `00` in requests; in replies `00` = success, otherwise an [error code](#status-codes) | [web] |
| request ID | 1–255, chosen by the host, echoed by the reply; IO Center increments it and skips 0. **0 marks a notification** sent by the keyboard on its own | [cap] [web] |
| feature, command | what the message is about, written `feature:command` in hex in this document (`10:0d`) | [cap] [web] |
| CRC | CRC-16/MODBUS (poly `0x8005` reflected, init `0xffff`) of bytes 0–61, little endian | [cap] [web] |

**Fragments.** A first fragment carries up to 55 data bytes, each following
fragment up to 59 (data starts right after the session byte). [cap]

**Replies** have the same request ID, feature and command as the request.
IO Center matches replies by feature and command only. Some commands (e.g.
`10:02`) get no reply. [cap] [web]

Multi-byte numbers are little endian everywhere.

### Status codes [web]

| Code | Name | Code | Name |
|---|---|---|---|
| `00` | SUCCESS | `07` | BUSY |
| `01` | INVALID_SESSION_ID | `08` | NOT_AUTHORIZED |
| `02` | INVALID_FEATURE_ID | `09` | NOT_ACTIVE |
| `03` | INVALID_COMMAND_ID | `0a` | INVALID_STATE |
| `04` | INVALID_REQUEST_ID | `0b` | INVALID_SIZE |
| `05` | INVALID_PARAMETER | `0c` | INSUFFICIENT_RESOURCES |
| `06` | TIMEOUT | `0d` | NOT_SUPPORTED |

## Features

Features IO Center Web uses for each keyboard [web]:

| ID | Feature | Purpose | Light Mount | Light Mount TKL |
|---|---|---|---|---|
| `01` | ROOT | sessions, supported features | ✓ | ✓ |
| `02` | DFU | **firmware update: never used by QuietLinux** | (updates offered) | ✓ |
| `03` | DEVICE_INFO | model, firmware version, serial number, factory reset | ✓ | ✓ |
| `04` | STORAGE | user data storage (IO Center keeps a settings file there) | | ✓ |
| `06` | USB_DEVICE | polling rate | ✓ | ✓ |
| `07` | KEYBOARD | layout, game mode, quick tap (SnapTap) | ✓ | ✓ |
| `10` | LIGHTINGS | lighting | ✓ | ✓ |
| `11` | BINDINGS | key bindings | ✓ | ✓ |
| `12` | MACROS | macro recording | | ✓ |
| `31` | CONTROLS | (not analyzed yet) | | ✓ |

The keyboard itself reports its features with `01:04` (GetSupportedFeatures,
see below); that answer is the authority. The reference keyboard (firmware
2.3.0) reports more than IO Center Web uses: root, DFU, device info,
**storage**, USB device, keyboard, lighting, bindings and **macros**, all
version 1. [cap]

Observations with QuietLinux on the reference keyboard, without opening a
session (session byte `01`; `quietlinux info` now opens one first) [cap]:

| Request | Answer |
|---|---|
| `01:04`, `01:05`, `06:01`, `07:01`, `07:02`, `07:04`, `10:01`, `11:01`, `11:04` | as documented below (QLink version 0.0.23) |
| `10:05` GetLayerConfig, layer 0 | a single byte `00` |
| `10:0f` GetCalibration, `07:06` GetSnapTapConfig | status INVALID_STATE |

They answer INVALID_STATE also within a session opened with `01:01`, and
`10:03` GetLayersLayout and `10:0e` GetGlobalLayers do too: the full-size Light
Mount apparently does not support layers, calibration and quick tap in general
mode (IO Center Web does not send these requests to it either). [cap]

To restore a binding, IO Center Web sends the factory action with `11:02`
(not `11:03` ClearBinding). [cap]

### What IO Center Web does when it connects to a Light Mount [cap]

Captured from IO Center Web and replayed in the tests (`connect-web.hex`):

1. With session `00`: `03:02` serial number, `01:05` QLink version, `03:01`
   device info (sent twice; IO Center requires firmware 1.0.1 or later),
   `01:04` supported features, then `01:01` OpenSession.
2. With the new session (`02`): `07:02` game mode flags, `07:01` layout,
   `06:01` polling rate, `10:01` lighting mode.
3. **If the lighting mode is above 1, it switches to `01` general** (`10:02`) [web]:
   this is why an IO Center session clears custom and realtime colors.
4. `10:05` layer 0 (the answer is a single `00`; IO Center then shows its
   defaults), `11:01` bindings from index 0, `11:04` bindings enabled,
   `07:04` game mode state.
5. While connected: `01:06` GetActiveSessionInfo from time to time.

### Factory defaults of the Light Mount [web]

| Setting | Default |
|---|---|
| Lighting | general mode, layer 0: static, color `E63000`, speed 50, brightness 100 |
| M1 … M5 | play/pause, next track, previous track, stop, mic mute |
| Knob | press: mute; turn left/right: volume down/up |
| Fn + ↑ / ↓ | brighter / darker |
| Fn + ← / → | previous / next effect |
| Fn + knob | same as without Fn |
| Game mode blocks | nothing (Shift+Tab, Alt+F4, Win, Alt+Tab all off) |
| Polling rate | 1000 Hz |

`01:04` GetSupportedFeatures returns `[count]` then `count × [feature][version]`. [web]

## Commands

Payloads are what follows the 4-byte header. "Reply" means the data of the reply.

### `01` ROOT [web]

| Command | Name | Payload | Reply |
|---|---|---|---|
| `01:01` | OpenSession | `[connection ID u32]`, optionally `[client type]` (0 unspecified, 1 QControl, 2 web); IO Center Web picks a random ID in 10000–99999 and sends no client type [cap] | `[connection ID u32][session ID][state][timeout]`; the reference keyboard echoes the ID and answers session `02`, state 0, timeout 3 [cap] |
| `01:02` | CloseSession | | |
| `01:03` | KeepAlive | | (empty); IO Center Web sends it every 1.5 s, the session timeout is 3 (seconds) [cap] |
| `01:04` | GetSupportedFeatures | | see [Features](#features) |
| `01:05` | GetQLinkVersion | | `[patch u16][minor][major]`; reference keyboard: `17 00 00 00` = 0.0.23 [cap] |
| `01:06` | GetActiveSessionInfo | | `[session ID][timeout][client type]`; reference keyboard: `02 03 00` [cap] |
| `01:07` | RequestStateChange | `[state]` | |
| `01:08` | SendStateChangeDecision | `[session ID][decision: 0 reject, 1 accept, 2 wait]` | |

Notifications: `01:01` SessionStateChanged, `01:02` ActiveSessionChanged,
`01:03` StateChangeRequested, `01:04` StateChangeDecision.

### `03` DEVICE_INFO

| Command | Name | Payload | Reply | Source |
|---|---|---|---|---|
| `03:01` | GetDeviceInfo | | `[model ID u16][revision][MCU count]` then per MCU `[minor][middle hi][middle lo][major]` | [cap] layout [web] |
| `03:02` | GetSerialNumber | | `[length][ASCII…]` | [cap] |
| `03:05` | FactoryReset | | | [web] |

Model IDs: `0001` Dark Mount, `0002` Light Mount. The reference keyboard
answers `02 00 01 01 00 00 03 02`: model `0002`, revision 1, one MCU, firmware
**2.3.0** (IO Center reads each version byte's hex digits as decimal digits). [cap] [web]

Notification `03:01` DeviceInfoChanged. Commands `03:03`/`03:04` (set/reset
the serial number) are factory commands and never sent.

### `04` STORAGE [web]

| Command | Name | Payload | Reply |
|---|---|---|---|
| `04:01` | GetStorageInfo | | size (u32) |
| `04:02` | WriteUserData | `[offset u16][length u16][data…]` | |
| `04:03` | ReadUserData | `[offset u16][length u16]` | data |

IO Center stores a settings file there: a u32 length at offset 0, then text.

### `06` USB_DEVICE [web]

| Command | Name | Payload | Reply |
|---|---|---|---|
| `06:01` | GetPollingRate | | rate in Hz (u16) |
| `06:02` | SetPollingRate | `[rate u16]` | | (IO Center Web offers no polling rate setting for the Light Mount: not captured) |

### `07` KEYBOARD [web]

| Command | Name | Payload | Reply |
|---|---|---|---|
| `07:01` | GetLayout | | `[physical: 0 ANSI, 1 ISO, 2 JIS][visual: 0 US, 1 DE, 2 UK, 3 FR, 4 NO]` |
| `07:02` | GetConfig | | `[game mode flags]` |
| `07:03` | SetConfig | `[game mode flags]` [cap] | (empty) |
| `07:04` | GetState | | `[state]` (0 inactive, 1 active; presumably game mode on/off) |
| `07:05` | SetState | `[state]` | |
| `07:06` | GetSnapTapConfig | | like the payload of `07:07` |
| `07:07` | SetSnapTapConfig | `[count]` then `count × [key ID u16][key ID u16][priority]` | |

Game mode flags (keys blocked while game mode is active): `01` Shift+Tab,
`02` Alt+F4, `04` Win, `08` Alt+Tab, `10` Caps Lock.

Quick tap ("SnapTap") priorities: 0 last input, 1 first key always wins,
2 second key always wins, 3 neutral (both released). IO Center offers up to
5 pairs.

Notifications: `07:01` ConfigChanged, `07:02` StateChanged, `07:03`
SnapTapConfigChanged. Switching game mode with Fn+Pause sends `07:01` with
`[11][flags]`, then `07:02` with `[11][state]` (`01` on, `00` off); the meaning
of `11` is unknown. [cap] IO Center Web has no switch for game mode, so `07:05`
SetState is not captured. The Light Mount's game mode
offers Shift+Tab, Alt+F4, Win and Alt+Tab (no Caps Lock).

### `10` LIGHTINGS

| Command | Name | Payload | Reply | Source |
|---|---|---|---|---|
| `10:01` | GetLightingMode | | `[mode]` | [cap] |
| `10:02` | SetLightingMode | `[mode]` | none | [cap] modes 1 and 3 |
| `10:03` | GetLayersLayout | | `[count][layer IDs…]` | [web] |
| `10:04` | SetLayersLayout | `[count][layer IDs…]` | | [web] |
| `10:05` | GetLayerConfig | `[layer]` | layer config, as below | [web] |
| `10:06` | SetLayerConfig | layer config, see below | (empty) | [cap] single color and gradient; [web] dual color, oriented gradient, ripple |
| `10:07` / `10:08` | Get/SetLayerMask | | | [web] (layout not analyzed) |
| `10:09` / `10:0a` | Get/SetLayerName | `[layer]` / `[layer][length][ASCII…]` | `[length][ASCII…]` | [web] |
| `10:0b` / `10:0c` | Get/SetPlaybackState | | | [web] (layout not analyzed) |
| `10:0d` | PerformRealtimeUpdates | see below | none | [cap] |
| `10:0e` | GetGlobalLayers | | `[count][layer IDs…]` | [web] |
| `10:0f` | GetCalibration | | 14 bytes, as below | [web] |
| `10:10` | SetCalibration | `[version][red f32][green f32][blue f32][enabled]` | | [web] |

**Lighting modes:** `00` off, `01` general (the layers' effects, stored),
`02` custom (stored per-key layers), `03` realtime (per-LED colors from
`10:0d`, volatile). Switching to realtime clears every LED. [cap] modes 1, 3; [web] all

**Layer config** (`10:06` payload, `10:05` reply without the first byte):

```text
[layer][effect][direction][brightness][speed][color mode][colors…][ripple: width, range, fade-out]
```

- Layers: the Light Mount has one, `0` (whole keyboard). The Light Mount TKL has
  `0` top and `1` bars (side strips). [web]
- Effects: `0` static, `1` color wave, `2` tornado, `3` breathing, `4` reactive,
  `5` matrix, `6` off, `7` ripple. [web] (static [cap]) IO Center Web offers the
  first six for the Light Mount; off and ripple appear for the TKL.
- Directions: `0` up, `1` down, `2` left, `3` right, `4` clockwise,
  `5` counterclockwise, `6` omnidirectional, `7` horizontal, `8` vertical, `9` cross. [web]
- Brightness and speed: 0–100 percent (captured 10–100); defaults 100 and 50 [cap].
  (IO Center Web converts a brightness it reads back with `/255*100`, apparently
  a quirk of the app.)
- IO Center's default gradient for the animated effects (captured with
  breathing): `ff0000@0 ffff00@17 00ff00@33 00ffff@50 0000ff@67 ff00ff@83 ff0000@100`. [cap]
- Color modes and their colors [web] (single, dual, gradient [cap]):
  - `0` single color: `[R][G][B]`
  - `1` dual color: `[R][G][B][R][G][B]`
  - `2` gradient: `[count]` then `count × [R][G][B][position]`
  - `3` oriented gradient: `[type: 0 linear, 1 radial, 2 conic][angle u16][count]` then `count × [R][G][B][position]`
- Ripple only, at the end: `[width][range][fade-out]` (IO Center's defaults: 5, 7, 0). [web]

**Realtime updates** (`10:0d`):

```text
[N] then N operations:   SetColor  [03][LED index u16][R][G][B]
                         Commit    [00]
```

IO Center sends 40 SetColor operations per message and commits once, at the
end of the last message, so `N` is the number of colors, +1 in the last
message. [cap] Other operations: `01` SetColors `[first index u16][RGB…]`,
`02` SetRangeColor `[first u16][last u16][R][G][B]`. [web] (seen for ARGB
devices; not yet for the Light Mount)

LED indexes [cap]:

| Indexes | Zone |
|---|---|
| `00`–`2c` | top light bar (45) |
| `2d` | knob |
| `2e`–`9d` | keys, row by row, left to right (`62` and `89` do not exist) |
| `9e`–`a2` | left side strip |
| `a3`–`a7` | right side strip |

Which key each LED belongs to is in `crates/quietlinux-core/src/keymap.rs`
[cap]: the key LEDs are numbered row by row like the LampArray lamps. French and
German ISO keyboards share the same physical layout, so the mapping holds for
both; `quietlinux calibrate` checks it on a keyboard (German ISO: 111 of 111
keys confirmed).

**Color calibration** (`10:0f` reply, 14 bytes): `[version][red f32][green f32][blue f32][enabled: bit 0]`,
the factors as fractions of 1. IO Center's default is red 1.00, green 0.59,
blue 0.43, which explains why unscaled colors look greenish. [web]

### `11` BINDINGS [web]

| Command | Name | Payload | Reply |
|---|---|---|---|
| `11:01` | GetBindings | `[index u16]` (index of the first entry to read) | `[2 count bytes]` then binding entries |
| `11:02` | SetBinding | `[key ID][layer][action type][action data…]`; captured: `04 00 02 08` (M3, normal layer, media, stop) [cap] | (empty) |
| `11:03` | ClearBinding | `[key ID][layer]` (back to the default) [cap] | (empty) |
| `11:04` / `11:05` | Get/SetConfig | `[enabled]` | `[enabled]`: 1 = custom bindings active |
| `11:06` | GetBinding | | |

- Key IDs: see [Key IDs](#key-ids). Layer byte: `00` normal, `80` Fn held
  (captured; IO Center Web's code suggests `01`). [cap]
- To bind a key, IO Center Web sends `11:02`; before binding a combination it
  first resets the key with `11:03`. [cap]
- A GetBindings entry is `[key ID][flags: bit 7 = Fn layer][action type][action data…]`.
- GetBindings returns only bindings that differ from the factory defaults: the
  reference keyboard answers with its 5 changed M keys (one of them bound to
  macro 0, action 2). [cap]

Action types and their data:

| Type | Action | Data |
|---|---|---|
| `00` | disabled (in SetBinding [cap] and in GetBindings) | |
| `01` | standard key | `[modifiers][HID usage]`; keys from IO Center's extra function key list: `[00][HID usage]` |
| `02` | media | `[action]`: 1 volume up, 2 volume down, 3 mute, 4 mic mute, 5 next, 6 previous, 7 play/pause, 8 stop, 9 open volume mixer, 10 specific sound |
| `03` | mouse | button: `[01][button \| flags][auto-fire rate]` (buttons 0 left, 1 right, 2 middle, 3 forward, 4 back; flags `40` while pressed, `80` double click); scroll: `[02][0 up, 1 down, 2 left, 3 right]` |
| `04` | open folder | `[length][path, ASCII]` |
| `05` | open file | `[length][path, ASCII]` |
| `06` | open website | `[length][URL, ASCII]` |
| `07` | Windows shortcut | `[action]` (file explorer, calculator, task manager, lock, …) |
| `08` | profile | `[action]`: 1 select (`[1][profile u16]`), 2 cycle, 3 next, 4 previous |
| `09` | backlight | `[action]`: 1 select effect (`[1][effect]`), 2 next effect, 3 previous effect, 4 brighter, 5 darker |
| `0a` | macro | `[action][macro ID u16]` |
| `0b` | special key ("alt code") | `[02][code u16]` |
| `0c` | disable | |

Modifiers: `01` left Ctrl, `02` left Shift, `04` left Alt, `08` left Win,
`10` right Ctrl, `20` right Shift, `40` right Alt, `80` right Win (HID bit order).

Open folder/file/website, Windows shortcuts and probably macros run **on the
host**: the keyboard only reports the key press (`11:02` ActionFired
notification) and IO Center performs the action. On Linux this needs a
background service.

Notifications: `11:01` BindingChanged, **`11:02` ActionFired** [cap], `11:03` ConfigChanged.

`11:02` ActionFired data: `[key ID][flags: bit 7 = Fn layer][state][action type][action data…]`,
the action in the same format as in bindings. [cap] (layout matched with [web])

| Captured | Meaning |
|---|---|
| `53 00 01 01 00 50` | ← (`53`), state 1, standard key, no modifiers, HID usage `50` (←) |
| `5c 80 01 09 03` | Fn+→, backlight action 3 (IO Center's table calls 3 "previous effect", its default binding for Fn+→ is "next effect") |
| `53 80 01 09 02` | Fn+←, backlight action 2 (so the keyboard sends 2 for ← and 3 for →) |
| `4b 80 01 1b` | Fn alone, action type `1b` (unknown) |
| `6e 00 01 02 01`, `6f 00 01 02 02`, `70 00 01 02 03` | knob right (volume up), left (volume down), press (mute) |
| `71 00 01 01 00 32`, `72 00 01 01 00 64` | the ISO keys `#` (HID usage `32`) and `<` (usage `64`) |
| `02 00 00 02 04` | M1 bound to mic mute: state byte `00` here, `01` for every other key (meaning unknown) |
| `06 00 01 0a 02 00` | M5 bound to macro 0: then the keys of the macro follow as events, typed by the keyboard itself |

The keyboard sends this notification for **every** key press, also for plain
keys and modifiers alone (e.g. `0b 00 01 01 01 00`: left Ctrl, modifiers `01`,
no usage), and none for releases. [cap] `quietlinux events` shows these
notifications live.

Captured bindings (all with an empty success reply) [cap]:

| Payload | Meaning |
|---|---|
| `03 00 01 00 04` | M2 → key A |
| `03 00 01 01 06` | M2 → Ctrl+C |
| `03 00 01 00 68` | M2 → F13 |
| `04 00 02 08` | M3 → media stop |
| `03 00 03 01 02 00` | M2 → middle mouse button |
| `03 00 09 02` | M2 → next effect |
| `03 00 07 02` | M2 → Windows shortcut "calculator" |
| `09 00 00` | Caps Lock disabled |
| `16 80 02 07` | Fn+F1 → play/pause |

### `12` MACROS

The reference keyboard reports this feature (version 1), and a macro bound to
M5 is **played by the keyboard itself**: its key presses arrive as `11:02`
events. So macros are stored in the keyboard. `12:01` GetMacros without payload
answers `02 00 01 00 02 00` (layout unknown). [cap]

IO Center Web does not list this feature for the (full-size) Light Mount. For
the TKL it uses only `12:09` StartRecording (`[count][key IDs…]`) and
`12:0a` StopRecording; while recording, the keyboard sends `12:05` InputEvent
notifications. The other commands exist (`12:01`–`12:08` get/set macros, their
configuration, events and names, `12:0b` AbortPlayback) but IO Center Web
does not send them, which suggests that macros are stored and played by the
host application. To be checked with captures of the Windows app.

Input event types: 1 modifiers changed, 2/3 key press/release, 4/5 power key,
6/7 consumer key, 8/9 physical key press/release, 10 buttons, 11/12 scroll,
13 pointer movement.

## Key IDs [web]

Key IDs (`KEY_ID2` in IO Center) identify keys in bindings, quick tap pairs and
key events. They are numbered column by column. The Light Mount uses this
table: the key events captured by bbossa (`53` ←, `5c` →, `4b` Fn) match it. [cap]

¹ K42 and K45 are the two extra ISO keys (standard key position numbers),
confirmed with `quietlinux events` on the German keyboard: K42 is `#`, K45 is `<`. [cap]

| ID | IO Center name | Key |
|---|---|---|
| `01` | `ESCAPE` | Escape |
| `02` | `M1` | M1 |
| `03` | `M2` | M2 |
| `04` | `M3` | M3 |
| `05` | `M4` | M4 |
| `06` | `M5` | M5 |
| `07` | `GAT` | Backquote |
| `08` | `TAB` | Tab |
| `09` | `CAPLOCK` | CapsLock |
| `0a` | `ENTER` | Enter |
| `0b` | `LEFTCTRL` | ControlLeft |
| `0c` | `1` | Digit1 |
| `0d` | `Q` | KeyQ |
| `0e` | `A` | KeyA |
| `0f` | `Z` | KeyZ |
| `10` | `LEFTGUI` | MetaLeft |
| `11` | `2` | Digit2 |
| `12` | `W` | KeyW |
| `13` | `S` | KeyS |
| `14` | `X` | KeyX |
| `15` | `LEFTALT` | AltLeft |
| `16` | `F1` | F1 |
| `17` | `3` | Digit3 |
| `18` | `E` | KeyE |
| `19` | `D` | KeyD |
| `1a` | `C` | KeyC |
| `1b` | `LEFTSHIFT` | ShiftLeft |
| `1c` | `F2` | F2 |
| `1d` | `4` | Digit4 |
| `1e` | `R` | KeyR |
| `1f` | `F` | KeyF |
| `20` | `V` | KeyV |
| `21` | `F3` | F3 |
| `22` | `5` | Digit5 |
| `23` | `T` | KeyT |
| `24` | `G` | KeyG |
| `25` | `B` | KeyB |
| `26` | `F4` | F4 |
| `27` | `6` | Digit6 |
| `28` | `Y` | KeyY |
| `29` | `H` | KeyH |
| `2a` | `SPACEBAR` | Space |
| `2b` | `N` | KeyN |
| `2c` | `7` | Digit7 |
| `2d` | `U` | KeyU |
| `2e` | `J` | KeyJ |
| `2f` | `M` | KeyM |
| `30` | `BSL` | (ANSI backslash, not on ISO) |
| `31` | `F5` | F5 |
| `32` | `8` | Digit8 |
| `33` | `I` | KeyI |
| `34` | `K` | KeyK |
| `35` | `CMA` | Comma |
| `36` | `F6` | F6 |
| `37` | `9` | Digit9 |
| `38` | `O` | KeyO |
| `39` | `L` | KeyL |
| `3a` | `DOT` | Period |
| `3b` | `F7` | F7 |
| `3c` | `0` | Digit0 |
| `3d` | `P` | KeyP |
| `3e` | `COL` | Semicolon |
| `3f` | `SL` | Slash |
| `40` | `F8` | F8 |
| `41` | `MIS` | Minus |
| `42` | `OQO` | BracketLeft |
| `43` | `CC` | Quote |
| `44` | `RIGHTALT` | AltRight |
| `45` | `F9` | F9 |
| `46` | `EQU` | Equal |
| `47` | `EQO` | BracketRight |
| `48` | `PADENTER` | NumpadEnter |
| `49` | `RIGHTGUI` | MetaRight |
| `4a` | `F10` | F10 |
| `4b` | `FN` | Fn |
| `4c` | `F11` | F11 |
| `4d` | `BACKSPACE` | Backspace |
| `4e` | `RIGHTSHIFT` | ShiftRight |
| `4f` | `RIGHTCTRL` | ControlRight |
| `50` | `F12` | F12 |
| `51` | `INSERT` | Insert |
| `52` | `DELETE` | Delete |
| `53` | `LEFT` | ArrowLeft |
| `54` | `PRINTSCREEN` | PrintScreen |
| `55` | `HOME` | Home |
| `56` | `END` | End |
| `57` | `UP` | ArrowUp |
| `58` | `DOWN` | ArrowDown |
| `59` | `SCROLLLOCK` | ScrollLock |
| `5a` | `PAGEDOWN` | PageDown |
| `5b` | `PAGEUP` | PageUp |
| `5c` | `RIGHT` | ArrowRight |
| `5d` | `PAUSE` | Pause |
| `5e` | `PADNUMLOCK` | NumLock |
| `5f` | `PAD7` | Numpad7 |
| `60` | `PAD4` | Numpad4 |
| `61` | `PAD1` | Numpad1 |
| `62` | `PAD0` | Numpad0 |
| `63` | `PADNSL` | NumpadDivide |
| `64` | `PAD8` | Numpad8 |
| `65` | `PAD5` | Numpad5 |
| `66` | `PAD2` | Numpad2 |
| `67` | `PADNMU` | NumpadMultiply |
| `68` | `PAD9` | Numpad9 |
| `69` | `PAD6` | Numpad6 |
| `6a` | `PAD3` | Numpad3 |
| `6b` | `PADDEL` | NumpadDecimal |
| `6c` | `PADNMI` | NumpadSubtract |
| `6d` | `PADNPL` | NumpadAdd |
| `6e` | `DIAL_ROTATE_CW` | knob: turn right |
| `6f` | `DIAL_ROTATE_CCW` | knob: turn left |
| `70` | `DIAL_PRESS` | Knob (press) |
| `71` | `K42` | Backslash (ISO key left of Enter) ¹ |
| `72` | `K45` | IntlBackslash (ISO key right of left Shift) ¹ |

## Unknown so far

- Payload layouts of `10:07`/`10:08` (layer masks), `10:0b`/`10:0c` (playback), `11:04`/`11:05`, `11:06`, the `31` CONTROLS feature.
- Whether the Windows app stores macros in the keyboard.
- Everything marked **[web]** only: to be captured before use. See [todo.md](todo.md).
