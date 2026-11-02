# Contributing

Help is welcome, especially from people with another firmware version,
another keyboard layout or the Light Mount TKL. Please read the
[safety rules](#safety-rules) first.

## Device reports

QuietLinux writes only to firmware it was tested with. A report helps to
support more versions:

1. `quietlinux status` and `quietlinux info` (both read-only). They contain no
   serial number; check them before posting anyway.
2. Your keyboard: model, layout (e.g. German ISO), where you bought it if it
   is a regional variant.
3. For a new layout: `quietlinux calibrate --layout uk` (or the closest one)
   lights one key at a time; note every key whose legend differs.

Open an issue with the **Device report** template, titled for example
"Device report: firmware 2.4.0, French ISO".

## Captures

New features need captures of what IO Center sends: see
[capturing.md](capturing.md). In short:

- Record one change per capture and write down what you changed.
- **Never share raw captures**: they contain the keyboard's serial number and
  everything typed while recording. Share the output of `qlcap frames` or
  `qlcap fixtures`, which leave out the serial number, key presses and macro
  contents, after reading it.
- IO Center rewrites all macros when one changes: use a test macro
  (`qlcap frames --macro ID` and `qlcap fixtures --macro ID` keep only that one).

## IO Center profiles

The import of IO Center for Windows profiles knows only some effects and
binding types ([iocenter.md](iocenter.md)). To help: in IO Center for Windows,
make a new profile without macros, set an effect or a few bindings of the
kinds still missing (note which), export a backup and attach only that
profile's `profiles/{UUID}.ioprofile` file. Read it first: it is plain JSON.
Do not attach profiles with your own macros, they contain what the macros type.

## Code

- The workspace builds with Rust 1.85 (core, command line, background service)
  and 1.92 (the window, because of Slint).
- Before a pull request:

  ```sh
  cargo fmt --all
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace --all-features
  cargo deny check
  ```

- No `unsafe` code (the workspace forbids it; the window's crate denies it,
  only Slint's generated code uses it).
- The protocol layer has no I/O; tests use captured fixtures and the simulated
  keyboard (`--demo`, `device::fake`).
- Everything from the keyboard or from files is untrusted: parsers return
  errors, never panic. `cargo test` runs them with random and damaged input
  (`crates/quietlinux-core/src/fuzzing.rs`; longer:
  `QUIETLINUX_FUZZ_ROUNDS=100 cargo test --release -p quietlinux-core fuzzing`).
  With a nightly toolchain and `cargo install cargo-fuzz`, `cd fuzz && cargo
  +nightly fuzz run frames` (also `replies`, `descriptor`, `text`, `iocenter`)
  searches with coverage guidance; CI does this once a week. A new parser gets
  an entry point there.
- `docs/protocol.md` marks every finding: **[cap]** (captured and replayed) or
  **[web]** (read from IO Center Web, not verified). Keep it that way.
- Documentation and code comments are in English.
- This project is developed with the help of AI (see the README). AI-assisted
  contributions are fine; you are responsible for what you submit, and it is
  tested like everything else.

## Safety rules

- **No firmware updates**, and no request of the firmware update (DFU)
  feature, not even read-only.
- Write only formats backed by captures of IO Center. Read-only requests
  documented by IO Center Web may be sent ([web], see [protocol.md](protocol.md#sources)).
- No factory reset until the way back is known.
- Host actions (websites) are started with an argument list, never through a
  shell, and only for what the user bound.
- Nothing about the user (serial numbers, typed text, macro contents) goes
  into the repository, logs or issues.
