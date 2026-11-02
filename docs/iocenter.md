# IO Center for Windows: profile files

`quietlinux profile import` (and **Import from IO Center …** in the window)
reads the profiles of IO Center for Windows, so that settings move from Windows
to Linux. This page describes their format as far as QuietLinux knows it. It
was read for interoperability from an export of the reference keyboard's
settings and checked against USB captures of the Windows app writing them; no
code of IO Center was used.

## Files

IO Center's backup is a zip archive. Among logs and caches it holds:

| File | Content |
|---|---|
| `profiles/{UUID}.ioprofile` | one profile each, JSON (see below) |
| `IO Center.ini` | app settings; `[Profiles]` names the default and last active profile and their order |
| `canvas/lightings.iodata` | saved lighting presets ("canvas"), JSON like the effects below |

QuietLinux reads the `.ioprofile` files, stored or deflated, from the zip or
one by one. The other files are not used.

## Profiles

A profile is JSON written by the C++ serialization library
[cereal](https://uscilab.github.io/cereal/):

```json
{"data": {
  "id": "{UUID}", "name": "Profil 1", "linkedAppPath": "", "disabledDevices": [],
  "properties": [
    {"polymorphic_name": "KeyBindingsProperty", "ptr_wrapper": {"data": {
      "subs": [{"key": "Light Mount", "value": {"ptr_wrapper": {"data": { … }}}}]}}},
    …]}}
```

- `properties` holds `CoolingStudioProperty` (not keyboard related),
  `LightingsProperty`, `MacrosProperty` and `KeyBindingsProperty`, in any
  order. Each has one entry per device in `subs`; QuietLinux reads the one
  named `Light Mount`.
- A variant (one of several kinds of binding, effect, macro event) is
  `{"impl": {"index": N, "data": { … }}}`. The index is IO Center's own
  numbering; only the indexes below are known.
- `cereal_class_version` fields appear on the first object of a kind only.
- `linkedAppPath`: a program that switches to the profile when it starts (not
  supported by QuietLinux).

### Key names

Keys are named `Key_…` after their **US HID meaning**, not their legend:
`Key_Y` is HID usage `1c`, the key with Z on German keyboards. Checked so far:
`Key_A`–`Key_Z`, `Key_0`–`Key_9`, `Key_Enter`, `Key_LeftShift`, `Key_F13`,
`Key_Insert`, `Key_Delete`, `Key_Home`, `Key_End`, `Key_PageUp`,
`Key_PageDown`, `Key_UpArrow`, `Key_DownArrow`, `Key_LeftArrow`,
`Key_RightArrow`; QuietLinux also reads `Key_F1`–`Key_F24`. The macro keys are
`Key_M1`–`Key_M5`, the knob `Key_Dial`, `Key_DialRotateLeft`,
`Key_DialRotateRight`.

### Key bindings

`KeyBindingsProperty`: `layerBindingsData` is a list of layers, `Common`
(normal) and `Fn1` (Fn held), each a list of `{"key": "Key_M2", "value":
{"impl": …}}`. A new profile lists IO Center's default bindings explicitly
(see [protocol.md](protocol.md), "IO Center's default bindings").

| Index | Binding | Data |
|---|---|---|
| 2 | key | `key` (key name), `modifiers` (encoding not known yet; 0 for none) |
| 4 | media | `action`: `IncreaseVolume`, `DecreaseVolume`, `Mute`, `MicMute`, `NextTrack`, `PrevTrack`, `PlayPause`, `Stop`; `audioDeviceId` |
| 12 | lighting | `action`: `NextEffect`, `PrevEffect`, `IncreaseBrightness`, `DecreaseBrightness`; `effect` (`None`) |
| 13 | macro | `macroId`: the UUID of a macro of the profile |

Not known yet: mouse buttons, websites, files and folders, Windows shortcuts,
profile switching, disabled keys, keys with modifiers.

### Macros

`MacrosProperty`: `macros` is a list of `{"id": "{UUID}", "name": …,
"config": {"playbackMode": "Once"}, "events": […]}`. An event is
`{"impl": {"index": 0 (key down) or 1 (key up), "data": {"key": "Key_…"}},
"delay": ms}`: the pause after it, as written to the keyboard (`12:06`). The
Windows app writes the macros in this order with the IDs 1, 2, …

Not known yet: the other playback modes (while held, repeat).

### Lighting

`LightingsProperty`: `enabled` (the on/off switch), `mode` (`General`: an
effect), `generalModeData.data[0]` the effect, `customModeData.layers` (empty
in the export).

| Index | Effect | Data |
|---|---|---|
| 10 | static | `color` (`#AARRGGBB`), `brightness` |
| 6 | reactive | `colorMode` (`Gradient` seen), `singleColor`, `dualColors` (`first`, `second`), `gradient` (list of `first`: color, `second`: position 0–1), `speed`, `brightness` |

Not known yet: color wave, tornado, breathing, matrix (and their direction
field), the other color modes' names, per-key lighting in a profile.

## What the import does

- One QuietLinux profile per IO Center profile, named after it (`Profil 1` →
  `Profil-1`), with `[bindings]`, the macros and `[effect]`/`[lighting]`; it
  has no `[game_mode]` section, so switching to it leaves game mode alone.
- Everything not known yet is left out and listed (`not imported: …`), also as
  comments at the top of the profile file. Keys whose binding was left out get
  their factory binding when switching to the profile.
- A profile without macros removes the keyboard's macros when switching to it,
  as IO Center does; they are saved in the profile that was active before.

**Help wanted:** a profile exported with other effects and bindings (without
private macros) fills the gaps above; see [contributing.md](contributing.md).
