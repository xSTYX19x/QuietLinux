# QuietLinux – Plan & TODO

Goal: a free, public Linux replacement for be quiet! **IO Center** for the
**be quiet! Light Mount** (USB `373f:0002`), with a **CLI** and a **GUI**,
well documented and with a clear notice about AI assistance.

Status: **Phase 0 mostly done** (as of 2026-10-27). All basic decisions are made (1.4).

---

## 1. Starting point

### 1.1 Hardware

| Interface | Type | Endpoints | Purpose |
|---|---|---|---|
| 0 | HID boot keyboard | EP1 IN, 8 B | normal key presses |
| 1 | HID keyboard, system control, consumer, mouse | EP2 IN, 24 B | key presses, media keys, mouse actions |
| 2 | HID vendor (usage page `0xFF00`) | EP3 IN / EP4 OUT, 64 B | **vendor protocol** (lighting, key bindings, macros, …) |
| 3 | HID LampArray (usage page `0x59`) | EP5 IN, 4 B | Windows *Dynamic Lighting* / standard RGB |

Details in [protocol.md](protocol.md).

### 1.2 Prior work

**bbossa/keyboard_bequiet_lightmount_rgb** (Python, GPL-2.0-or-later, as of 2026-10-24)
- Decoded the packet format of the vendor interface: 64-byte packets
  `[len][idx|more][session][body…][CRC-16/MODBUS]`, fragmentation, sessions.
- Known commands: `03:01` version, `03:02` serial number, `10:01/10:02` read/set
  lighting mode, `10:06` effect (only "static" used), `10:0d` per-LED color
  (volatile), `11:02` key events (spontaneous, including the Fn layer).
- LED indexes: `00–2c` top bar (45), `2d` knob, `2e–9d` keys, `9e–a2` left,
  `a3–a7` right side strip.
- Good safety principles: only send verified frames, only write to known
  firmware, never update firmware.
- **Missing:** all other effects, key bindings, Fn layer, macros, profiles,
  game mode, quick tap, GUI.

**echtzeit-solutions/monsgeek-akko-linux** (Rust, GPL-3.0)
- Different chip (RongYuan RY5088/YC3121), **different protocol**: its code is
  not directly usable for us.
- Useful as a **model for structure and method**: separate transport / protocol /
  front ends, pcap analyzer, macro sequences, device database, feature map
  against the vendor software, protocol extraction from the web app's JavaScript.

### 1.3 Reuse

- **bbossa:** we build on it. The protocol layer (`protocol.py`), device
  discovery (`device.py`), LED indexes and test fixtures (reference frames from
  captures of the vendor software) are **ported to Rust**; the fixtures are reused
  unchanged as test vectors. Our code is therefore a derivative of bbossa's code:
  credit in README, `docs/protocol.md`, `NOTICE` and the header of each ported
  file, with a link to the repository.
- **monsgeek:** no code dependency. Only ideas for architecture and method,
  credited in the README as inspiration.
- **License:** **GPL-3.0-or-later**, compatible with both projects
  (GPL-2.0-*or-later* code may be used under GPL-3).

### 1.4 Decisions

| Topic | Decision |
|---|---|
| Name | **QuietLinux** (repository and project). Binaries: `quietlinux` (CLI), `quietlinux-gui`. "LightMountLinux" was rejected: too close to bbossa's tool `lightmount` and to the product name. |
| Language | **English** for everything in the repository: code, comments, docs, this file, commit messages, issues. |
| Stack | **Rust** for core, CLI and daemon; **Slint** for the GUI (reasons in 6.1) |
| Authorship | Maintainer as Git author (GitHub noreply address), Claude as `Co-Authored-By` in commits |
| Reference device | Light Mount **Silent Linear, German ISO** (L-shaped Enter, Ä/Ö/Ü), M1–M5, knob with push button. Other layouts are welcome through contributed key maps. |
| Firmware | **No** firmware update: never sent, captured or replayed |
| Windows captures | a real Windows PC (no VM), captured with USBPcap, files transferred to the development machine |
| Daemon | after the basic version (core, CLI, GUI) |
| Upstream | bbossa is credited in the repository; no separate notification |

---

## 2. IO Center features (goal: parity)

Source: reviews (guru3d, HWBusters, igor'sLAB), to be checked against the real
Windows app and the web app in phase 2. ✔ = already known thanks to bbossa.

### 2.1 Lighting
- [ ] Effects: static ✔, color wave, tornado, breathing, matrix, reactive, ripple, off
- [ ] Parameters per effect: color(s) / color mode (single, rainbow, …), brightness, speed, direction
- [ ] Zones: keys, top bar, knob, left/right side strip, separately or as one group
- [ ] Custom mode: color per key/LED ✔ (volatile, "realtime" mode `03`). A **stored** variant exists: lighting mode `02` "custom" with layers and layer masks [web]; layer mask format still unknown
- [ ] Custom mode with up to 10 layers: `10:03`–`10:0a` (layers layout, config, mask, name) exist in the protocol [web]; to be captured from the Windows app
- [ ] LED color calibration
- [ ] Interaction with LampArray / Dynamic Lighting (interface 3)

### 2.2 Key bindings
- [ ] Remap any key to: another key, key combination/shortcut, media command, mouse action, macro, profile switch, lighting control, disabled
- [ ] Fn layer (second binding per key)
- [ ] Knob: press, turn left, turn right, each assignable
- [ ] Macro keys M1–M5
- [ ] **Host-side actions** (open a program, file, folder or website): the keyboard cannot do this itself. On Linux this needs a background service that reacts to the vendor key events (`11:02`).
- [ ] Restore the factory key bindings

### 2.3 Macros
- [ ] Macro library, up to 999 events (key down/up, delays)
- [ ] Recording, editing, repeat modes (find out: once, n times, while held, toggle?)
- [ ] Find out: are macros stored in the device or played back by the host? IO Center Web does not support macros on the full-size Light Mount at all and only records them on the TKL: probably host-side (binding type `0a` + `11:02` ActionFired). Check with the Windows app

### 2.4 Profiles
- [ ] Find out how many profiles the device holds; switch, read, write. No profile commands in the protocol [web]: profiles are probably kept by the host app and written to the keyboard when switched (binding type `08` profile actions)
- [ ] Export/import profiles as files (own documented format, e.g. TOML)

### 2.5 Product settings
- [ ] Show the firmware version (**read-only**, see 7)
- [ ] Game mode (block Win key, Alt+Tab, Alt+F4, …; find out what exactly is configurable)
- [ ] Quick tap / SOCD: up to 5 key pairs
- [ ] Any other setting found in the app (polling rate? debounce? factory reset?)

---

## 3. Phase 0 – Project foundations

- [x] Settle the open questions
- [x] `git init`, `.gitignore` (captures, `*.pcapng`, VM images, firmware files, …)
- [x] Git identity with the GitHub noreply address only, no private data in commits
- [x] `LICENSE` (GPL-3.0-or-later), `NOTICE` with credits
- [x] `README.md` with
  - [x] **clear AI notice** at the top (section 8)
  - [x] "not affiliated with be quiet!, not official software, use at your own risk"
  - [x] credits/links to bbossa and monsgeek
- [x] Project structure (section 6)
- [x] Cargo workspace, port bbossa's protocol and fixtures, tests passing
- [x] CI (GitHub Actions): `cargo fmt`, `clippy`, tests without keyboard, `cargo deny`
- [x] Planning file in English
- [x] First commit
- [x] Key map with W3C key names and legends (uk, de, fr); same physical layout as bbossa's verified French ISO map
- [x] `calibrate` command that lights the keys one by one
- [x] Run `calibrate --layout de` on the reference keyboard: 111 of 111 keys and legends match (2026-10-27)

## 4. Phase 1 – Capture setup

Goal: reproducible, well-labeled captures to read the protocol from.

### 4.1 WebHID (IO Center Web in Chrome, directly on Linux)
- [x] udev rule so that Chrome (WebHID) and our tools can open hidraw without root (`packaging/udev/70-quietlinux.rules`, interfaces 2 and 3 only)
- [x] Install the rule on the development machine and remove conflicting older rules
- [x] Guide for usbmon + Wireshark/tshark in `docs/capturing.md`
- [x] **WebHID hook in DevTools** (`tools/webhid-logger.js`): small JS snippet that logs `HIDDevice.sendReport`, `sendFeatureReport`, `receiveFeatureReport` and `inputreport` (with timestamps and a plain-text note). Gives clean logs without USB overhead that can be matched with what was just clicked in the UI
- [x] Check whether the web app ships its command tables in its JavaScript: **yes**. Features, commands, status codes, enumerations, payload layouts and key IDs are documented in `protocol.md`, marked [web] until captured. Read only for interoperability, no code copied

### 4.2 Windows PC (IO Center.exe)
A real Windows PC is simpler and more reliable than a VM: no USB passthrough and
no reset problems when handing the device over.
- [ ] Install Wireshark for Windows with **USBPcap** (included in the Wireshark installer)
- [ ] Install IO Center, note the firmware version (read only)
- [ ] **Disconnect the network while capturing** and never accept update prompts (section 7)
- [ ] **Turn off Windows Dynamic Lighting** (Settings › Personalization › Dynamic Lighting), otherwise Windows takes over the lighting
- [ ] Capture on the USBPcap root hub of the keyboard, save as `.pcapng`, transfer with a note
- [ ] Mind bbossa's warning: IO Center sessions may reset key bindings. Note the state beforehand
- [x] Write the step-by-step guide as `docs/capturing.md`, so that other users can contribute captures

### 4.3 Analysis tools (`tools/`)
- [x] `qlcap frames`: pcap/pcapng (usbmon, USBPcap) or WebHID log → vendor messages (direction, time, `cmd:sub`, fragments reassembled, serial redacted)
- [ ] `framediff`: compare two captures in which exactly one setting was changed (for now: `diff` of two `qlcap frames` outputs, see `docs/capturing.md`)
- [x] `qlcap fixtures`: anonymized fixture lines (no `03:02` replies, no key events)

## 5. Phase 2 – Data collection

Method: **one change per capture**, each with a short note
(`captures/<area>/<nr>-<description>.pcapng` + `.md`). Captures stay local and
never enter the repository; only small anonymized fixtures do.

Since the IO Center Web analysis, captures mainly **confirm** what `protocol.md`
marks [web] before QuietLinux sends it, and fill the gaps (layer masks, the
Windows app's macros and profiles).

Checklist (extended while collecting):
- [ ] Connection setup: everything the app sends when opened (expected sequence in `protocol.md`)
- [ ] Each effect, then each parameter on its own (color, brightness, speed, direction, color mode)
- [ ] Zones separate vs. together
- [ ] Custom mode: stored vs. preview; layers
- [ ] Remap a key to: key, combination, media, mouse, disabled, profile switch, lighting control
- [ ] Fn layer, knob (3 actions), M1–M5
- [ ] Create, change, delete, assign a macro; long macro (fragmentation); each repeat mode
- [ ] Switch and rename profiles, values per profile
- [ ] Game mode, quick tap (1 pair, 5 pairs)
- [ ] LED calibration
- [ ] Factory reset (only once the starting state is known for sure)
- [ ] Key events (`11:02`) for all special keys
- [ ] LampArray (interface 3): read the attributes

Result: **[protocol.md](protocol.md)**, the complete protocol description
(commands, fields, value ranges, examples, source of each finding).

## 6. Phases 3–7 – Implementation

### 6.1 Stack & architecture

**Why Rust + Slint** (requirements: little RAM/CPU, efficient, secure, maintainable):
- **Rust:** memory-safe without a garbage collector, starts instantly, small
  single binaries without a runtime. The vendor interface needs only
  `read`/`write` on `/dev/hidraw*` (no ioctl), so the core needs **no `unsafe`
  at all** (`unsafe_code = "forbid"` for the whole workspace).
- **Slint:** lightweight declarative GUI (UI in `.slint` files, logic in Rust),
  native on Wayland, redraws only on changes, typically a few tens of MB of RAM
  instead of 100+ MB for Qt/Python or Electron. License: GPLv3 option,
  compatible with ours.
- **Daemon** (optional): blocks waiting for events (`poll`), 0 % CPU when idle, a few MB of RAM.
- **Rejected alternatives:** Python + PySide6 (bbossa's code usable as is, but
  Python runtime + Qt ≈ 100–150 MB RAM and slow start), Electron/Tauri (heavy,
  or a webview attack surface), GTK4-rs (good, but heavier than Slint and a
  weaker fit on KDE).

```
crates/
  quietlinux-core/     library
    protocol.rs        pure functions: build/parse frames, CRC, fragmentation (no I/O, ported from bbossa)
    device.rs          sysfs discovery, interface checks, hidraw transport, identified session
    device/fake.rs     simulated keyboard for tests (feature `fake`)
    hid_descriptor.rs  minimal HID report descriptor parser
    layout.rs          LED indexes per zone
    keymap.rs          physical keys: W3C names, LED index, position; keycap legends per layout
    color.rs           RGB colors
    model/             (planned) profiles, effects, key map, macros (serde, TOML)
  quietlinux-cli/      binary `quietlinux` (clap)
  quietlinux-gui/      (planned) binary `quietlinux-gui` (Slint)
  quietlinux-daemon/   (planned) host-side actions, reapply after replug/resume
tools/qlcap/           capture decoder (pcap, pcapng, WebHID logs)
tools/webhid-logger.js DevTools snippet that logs WebHID reports
packaging/udev/        udev rule
docs/
```

CLI, GUI and daemon share the core library. The GUI contains no protocol
logic. Dependencies stay minimal (today: clap, rustix) and are checked in CI
(`cargo deny`).

### 6.2 Phase 3 – Core library
- [x] Protocol layer + tests against bbossa's fixtures
- [x] Transport + device checks (VID/PID, interfaces, report descriptors, known firmware)
- [x] Protocol names from IO Center Web (features, commands, status codes); reply status checked; request ID never 0
- [ ] Vendor key IDs (`KEY_ID2`) in the key map, and the knob's turn left/right as binding targets
- [ ] `quietlinux events`: show key IDs of pressed keys (read-only, from `11:02` notifications), to confirm K42/K45 and the key ID table
- [ ] Read-only commands first: `01:04` features, `01:05` QLink version, `07:01` layout (auto-select legends), `07:02`/`07:04` game mode, `06:01` polling rate, `10:05` layer config, `11:01` bindings, `11:04` bindings enabled
- [ ] Protocol layer for every command confirmed in phase 2, with fixtures
- [ ] **Read** the device state first, write afterwards
- [ ] Backup/restore: save the complete device state to a file before writing
- [ ] Fuzz the parsers (`cargo fuzz`)

### 6.3 Phase 4 – CLI
- [x] `status`, `zones`, `keys`, `calibrate`, `light static`, `light stored`, `light set` (zones and single keys)
- [ ] `backup`, `restore`
- [ ] `light effect …` for every effect
- [ ] `keymap get|set|reset`, `macro list|add|edit|delete|assign`
- [ ] `profile list|switch|export|import`
- [ ] `settings game-mode|quick-tap …`
- [ ] Shell completions (bash/zsh/fish), `--json` output for scripts

### 6.4 Phase 5 – GUI
- [ ] Keyboard view (layout from LampArray positions + zones), click a key → binding/color
- [ ] Sections like IO Center: lighting, key bindings, macros, profiles, product settings
- [ ] Live preview, "save to keyboard"
- [ ] Macro editor with recording
- [ ] Own look, no imitation of be quiet! branding or logos

### 6.5 Phase 6 – Daemon (optional)
- [ ] Host-side actions (open program/URL, shortcuts) through vendor events
- [ ] Reapply settings after replug/resume
- [ ] systemd user service

### 6.6 Phase 7 – Release
- [ ] Docs: `README.md`, `docs/installation.md` ✔, `docs/usage-cli.md` ✔, `docs/usage-gui.md`, `docs/protocol.md` ✔, `docs/troubleshooting.md`, `docs/contributing.md` (including how to capture for another firmware version or layout)
- [ ] Install script for binary + udev rule
- [ ] Packages: AUR (PKGBUILD), `cargo install`, prebuilt binaries in releases, maybe .deb/Flatpak
- [ ] GitHub release, issue template "device report"

## 7. Safety rules (apply to the whole project)

1. **Never send firmware updates or replay firmware-related frames.** Firmware updates are explicitly out of scope.
2. Only send frames whose format is backed by captures and documented.
3. Write only to known firmware versions, read-only otherwise (as bbossa does).
4. Before writing key maps or macros for the first time: know the way back (factory reset command or complete backup/restore).
5. Mind flash wear: frequent changes (preview, alerts) only through volatile commands.
6. No personal data in the repository: no serial numbers, no raw captures, no user names or home paths, no private email addresses.

**Security of the user's system:**

7. Runs entirely without root. A single udev rule (`TAG+="uaccess"`) gives the logged-in user access to interfaces 2 and 3 of `373f:0002` only; the interfaces that carry key presses stay root-only. No setuid, no polkit rules.
8. No network access, no open ports, no web/gRPC server. If GUI and daemon need to talk: a Unix socket in `$XDG_RUNTIME_DIR` with mode `0600`, or the D-Bus session bus.
9. Start host-side actions (open program/URL) directly with an argument list, **never through a shell** (`sh -c`), so that config files cannot inject commands. Only run actions the user configured.
10. Treat everything coming from the keyboard as untrusted: check lengths and CRC, never panic on broken frames, fuzz the parsers.
11. `unsafe_code = "forbid"` in the whole workspace, few dependencies, `cargo deny` in CI.

## 8. AI notice (as used in the README)

> **AI notice:** This project is developed with the help of AI (Claude by
> Anthropic). Planning, protocol analysis, code and documentation are wholly or
> partly AI-generated. They are reviewed by the maintainer and tested on real
> hardware, but mistakes are possible. Use at your own risk.

In addition, commits with AI contributions carry a `Co-Authored-By: Claude …` line.
