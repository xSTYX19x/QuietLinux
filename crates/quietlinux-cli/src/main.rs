//! `quietlinux`: command line tool for the be quiet! Light Mount keyboard.

mod calibrate;
mod report;

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use clap::{Parser, Subcommand};
use quietlinux_core::Rgb;
use quietlinux_core::device::{self, fmt_version};
use quietlinux_core::info::{Action, Colors};
use quietlinux_core::keymap::{self, KEYS, Key, Layout};
use quietlinux_core::layout::{self, Zone};
use quietlinux_core::protocol::Led;
use quietlinux_core::{config, macros, settings};

use crate::calibrate::Answer;

#[derive(Debug, Parser)]
#[command(
    name = "quietlinux",
    version,
    about = "Configure the be quiet! Light Mount keyboard"
)]
struct Cli {
    /// USB path of the keyboard to use when several are plugged in (see `quietlinux status`)
    #[arg(long, global = true, value_name = "PATH")]
    usb_path: Option<String>,

    /// Allow writing to a firmware version this tool was never tested with
    #[arg(long, global = true)]
    allow_unknown_firmware: bool,

    /// Keycap legends to show: uk, de or fr (default: the layout the keyboard reports, else uk)
    #[arg(long, global = true)]
    layout: Option<Layout>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show the keyboard, its firmware version and its lighting mode (read-only)
    Status,
    /// Show everything the keyboard reports: features, layout, lighting, game mode,
    /// quick tap, key bindings (read-only)
    Info,
    /// Show key presses the keyboard reports, with their key IDs (read-only; Ctrl+C to stop)
    ///
    /// Shows what you type on the Light Mount while it runs: do not type anything private.
    Events {
        /// Stop after this many events
        #[arg(long)]
        count: Option<usize>,
    },
    /// List the lighting zones and their LED indexes (does not touch the keyboard)
    Zones,
    /// List the keys: LED index, name and legend (does not touch the keyboard)
    Keys,
    /// Change the lighting
    #[command(subcommand)]
    Light(Light),
    /// Change what a key does (stored in the keyboard)
    ///
    /// Examples: quietlinux bind M2 key ctrl+KeyC; quietlinux bind M3 media stop;
    /// quietlinux bind --fn F1 media play-pause; quietlinux bind CapsLock disable;
    /// quietlinux bind CapsLock default. Keys: see `quietlinux keys`, plus KnobLeft
    /// and KnobRight for the knob's turns.
    Bind {
        /// Change the Fn layer (the key pressed together with Fn)
        #[arg(long = "fn")]
        fn_layer: bool,
        /// Allow changing Enter
        #[arg(long)]
        force: bool,
        key: settings::BindKey,
        #[command(subcommand)]
        action: BindAction,
    },
    /// Macros stored in the keyboard: list, show, create, edit, delete
    ///
    /// Events: KeyA taps a key, ctrl+KeyC a combination, +shift / -shift press /
    /// release a key, 250ms sets the pause after the previous event (default
    /// --delay after each event, none after the last).
    /// Example: quietlinux macro create select-copy ctrl+KeyA 50ms ctrl+KeyC
    #[command(subcommand)]
    Macro(MacroCommand),
    /// Save the keyboard's key bindings, game mode shortcuts and macros to a file (read-only)
    ///
    /// `-` writes to standard output. The effect cannot be read from the Light Mount;
    /// the file contains a template to add one.
    Backup {
        file: PathBuf,
        /// Overwrite an existing file
        #[arg(long)]
        force: bool,
    },
    /// Write the settings of a backup file to the keyboard
    ///
    /// Only what differs is sent. Sections missing from the file are left alone; in
    /// [bindings], keys not listed get their factory binding back, except bindings
    /// QuietLinux cannot recreate.
    Apply {
        file: PathBuf,
        /// Show the changes without making them
        #[arg(long)]
        dry_run: bool,
    },
    /// Choose the shortcuts game mode blocks (stored in the keyboard; Fn+Pause toggles game mode)
    GameMode {
        /// `none` or a comma-separated list of shift-tab, alt-f4, win, alt-tab, caps-lock
        #[arg(long, value_name = "SHORTCUTS")]
        block: settings::GameModeBlocks,
    },
    /// Light the keys one by one to check the key map
    ///
    /// Press Enter when the lit key is the one shown, otherwise type what is lit.
    /// The previous lighting mode is restored at the end.
    Calibrate {
        /// Start at this LED index (hex, see `quietlinux keys`)
        #[arg(long, value_name = "HEX", value_parser = parse_hex_u8)]
        start: Option<u8>,
    },
}

#[derive(Debug, Subcommand)]
enum BindAction {
    /// A key or key combination, e.g. KeyA, ctrl+KeyC, ctrl+shift+F13
    Key { combo: settings::KeyCombo },
    /// volume-up, volume-down, mute, mic-mute, next-track, previous-track, play-pause, stop
    Media { action: settings::MediaAction },
    /// A mouse button: left, right, middle, forward, back
    Mouse { button: settings::MouseButton },
    /// next-effect, previous-effect, brighter, darker
    Lighting { action: settings::LightingAction },
    /// A Windows shortcut (calculator, task-manager, …): performed by IO Center on
    /// Windows; on Linux nothing happens yet
    Shortcut { name: settings::Shortcut },
    /// A macro stored in the keyboard, by its ID (`quietlinux info` lists them);
    /// the keyboard plays it itself
    Macro { id: u16 },
    /// The key does nothing
    Disable,
    /// Back to the factory binding
    Default,
}

impl BindAction {
    /// The action to store, or `None` for the factory default.
    fn action(&self) -> Option<Action> {
        Some(match self {
            BindAction::Key { combo } => Action::StandardKey {
                modifiers: combo.modifiers,
                usage: combo.usage,
            },
            BindAction::Media { action } => Action::Media(action.0),
            BindAction::Mouse { button } => Action::MouseButton {
                button: button.0,
                flags: 0,
                auto_fire: 0,
            },
            BindAction::Lighting { action } => Action::Backlight {
                action: action.0,
                effect: None,
            },
            BindAction::Shortcut { name } => Action::WindowsShortcut(name.0),
            BindAction::Macro { id } => Action::Macro(*id),
            BindAction::Disable => Action::Disabled,
            BindAction::Default => return None,
        })
    }
}

#[derive(Debug, Subcommand)]
enum MacroCommand {
    /// List the macros: ID, name, repeat mode, number of events (read-only)
    List,
    /// Show a macro's events, in the syntax of `macro create` (read-only)
    Show { id: u16 },
    /// Store a new macro in the keyboard; bind it with `quietlinux bind KEY macro ID`
    Create {
        name: String,
        /// Events (see `quietlinux macro --help`); options go before them
        #[arg(required = true, allow_hyphen_values = true)]
        events: Vec<String>,
        /// once, while-held or repeat
        #[arg(long, default_value = "once")]
        repeat: macros::RepeatMode,
        /// Pause after each event, in ms
        #[arg(long, default_value_t = macros::DEFAULT_DELAY_MS)]
        delay: u16,
    },
    /// Change a macro's name, repeat mode or events
    Edit {
        id: u16,
        #[arg(long)]
        name: Option<String>,
        /// once, while-held or repeat
        #[arg(long)]
        repeat: Option<macros::RepeatMode>,
        /// New events, replacing all (see `quietlinux macro --help`); give it last
        #[arg(long, num_args = 1.., allow_hyphen_values = true)]
        events: Option<Vec<String>>,
        /// Pause after each new event, in ms
        #[arg(long, default_value_t = macros::DEFAULT_DELAY_MS)]
        delay: u16,
    },
    /// Delete a macro (refused while a key is bound to it)
    Delete { id: u16 },
}

fn run_macro<T: device::Transport>(
    session: &mut device::Session<T>,
    command: MacroCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    let lines = match command {
        MacroCommand::List => macros::list(session)?,
        MacroCommand::Show { id } => macros::show(session, id)?,
        MacroCommand::Create {
            name,
            events,
            repeat,
            delay,
        } => {
            let events = macros::parse_events(&events, delay)?;
            let count = events.len();
            let id = macros::create(session, &name, repeat, events)?;
            vec![format!(
                "Macro {id} {name:?} stored ({count} event(s)). Bind it: quietlinux bind KEY macro {id}"
            )]
        }
        MacroCommand::Edit {
            id,
            name,
            repeat,
            events,
            delay,
        } => {
            let events = events
                .map(|e| macros::parse_events(&e, delay))
                .transpose()?;
            if name.is_none() && repeat.is_none() && events.is_none() {
                return Err("nothing to change: give --name, --repeat or --events".into());
            }
            let changes = macros::Changes {
                name,
                repeat,
                events,
            };
            macros::edit(session, id, changes)?;
            vec![format!("Macro {id} changed.")]
        }
        MacroCommand::Delete { id } => {
            macros::delete(session, id)?;
            vec![format!("Macro {id} deleted.")]
        }
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

#[derive(Debug, Subcommand)]
enum Light {
    /// Static color effect, stored in the keyboard
    Static {
        /// Color as 6 hex digits, e.g. ff2a00
        color: Rgb,
        /// Brightness in percent
        #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u8).range(0..=100))]
        brightness: u8,
    },
    /// Effect stored in the keyboard: static, color-wave, tornado, breathing, reactive, matrix
    ///
    /// Examples: quietlinux light effect breathing; quietlinux light effect static --color ff1493
    Effect {
        effect: settings::Effect,
        /// One color (default for static: e63000; for the others IO Center's rainbow gradient)
        #[arg(long, conflicts_with_all = ["gradient", "dual"])]
        color: Option<Rgb>,
        /// Two colors, e.g. e63000,ffffff
        #[arg(long, value_name = "COLOR,COLOR", conflicts_with = "gradient")]
        dual: Option<settings::DualColor>,
        /// Gradient: colors spread evenly (ff0000,0000ff) or with positions (ff0000@0,0000ff@100)
        #[arg(long)]
        gradient: Option<settings::Gradient>,
        /// Brightness in percent
        #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u8).range(0..=100))]
        brightness: u8,
        /// Speed in percent
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u8).range(0..=100))]
        speed: u8,
        /// up, down, left, right, clockwise, counterclockwise, omnidirectional, horizontal, vertical, cross
        #[arg(long, default_value = "up")]
        direction: settings::Direction,
    },
    /// Show the effect stored in the keyboard again (leaves realtime mode or "off")
    Stored,
    /// Switch the lighting off, like IO Center's on/off switch; `light stored` switches it on
    Off,
    /// Per-LED colors by zone or key (volatile: lost when the keyboard is unplugged)
    ///
    /// LEDs not given are switched off; later pairs win.
    /// Example: quietlinux light set keys=ff2a00 top=f4dbae KeyW=00ff00
    Set {
        /// TARGET=COLOR pairs. Targets: zones (all, top, knob, keys, left, right)
        /// or key names (see `quietlinux keys`)
        #[arg(required = true, value_name = "TARGET=COLOR")]
        assignments: Vec<Assignment>,
        /// Scale all colors (0.0 to 1.0): per-LED colors have no brightness setting
        #[arg(long, default_value_t = 1.0)]
        dim: f32,
    },
}

fn parse_hex_u8(s: &str) -> Result<u8, String> {
    u8::from_str_radix(s.trim_start_matches("0x"), 16)
        .map_err(|_| format!("expected a hex byte like 2e, got {s:?}"))
}

/// Target of a `TARGET=COLOR` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    All,
    Zone(Zone),
    /// A single LED, by key name.
    Led(u8),
}

impl Target {
    fn contains(self, index: u8) -> bool {
        match self {
            Target::All => true,
            Target::Zone(zone) => zone.leds().any(|i| i == index),
            Target::Led(led) => led == index,
        }
    }
}

impl FromStr for Target {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "all" {
            return Ok(Target::All);
        }
        if let Ok(zone) = s.parse() {
            return Ok(Target::Zone(zone));
        }
        keymap::by_code(s)
            .map(|k| Target::Led(k.led))
            .ok_or_else(|| {
                format!(
                    "unknown target {s:?}: use a zone (all, top, knob, keys, left, right) \
                 or a key name like Escape or KeyW (see `quietlinux keys`)"
                )
            })
    }
}

#[derive(Debug, Clone, Copy)]
struct Assignment {
    target: Target,
    color: Rgb,
}

impl FromStr for Assignment {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (target, color) = s
            .split_once('=')
            .ok_or_else(|| format!("expected TARGET=COLOR, got {s:?}"))?;
        Ok(Self {
            target: target.parse()?,
            color: color.parse().map_err(|e| format!("{e}"))?,
        })
    }
}

/// One color per LED; later assignments win, LEDs not assigned are off.
fn leds_for(assignments: &[Assignment], dim: f32) -> Vec<Led> {
    layout::all_leds()
        .map(|index| {
            let color = assignments
                .iter()
                .rev()
                .find(|a| a.target.contains(index))
                .map_or(Rgb::BLACK, |a| a.color.scaled(dim));
            Led::new(index, color)
        })
        .collect()
}

/// Ask on the terminal about one lit key.
fn ask_terminal(key: &Key, layout: Layout) -> Answer {
    print!(
        "{:02x}  {:<15} {:<8} ",
        key.led,
        key.code,
        layout.legend(key.code)
    );
    let _ = io::stdout().flush();
    let mut line = String::new();
    match io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => Answer::Stop, // end of input (Ctrl+D)
        Ok(_) => match line.trim() {
            "" => Answer::Match,
            "q" => Answer::Stop,
            other => Answer::Lit(other.to_owned()),
        },
    }
}

// One arm per command: splitting the dispatch would not make it clearer.
#[allow(clippy::too_many_lines)]
fn run(cli: Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let usb_path = cli.usb_path.as_deref();
    match cli.command {
        Command::Zones => {
            for zone in Zone::ALL {
                let leds: Vec<u8> = zone.leds().collect();
                let (first, last) = (leds[0], leds[leds.len() - 1]);
                println!(
                    "{:<6} {:3} LEDs  {first:02x}-{last:02x}",
                    zone.name(),
                    leds.len()
                );
            }
        }
        Command::Keys => {
            let layout = cli.layout.unwrap_or_default();
            println!("LED  {:<15} LEGEND ({layout})", "NAME");
            for key in &KEYS {
                println!(
                    "{:02x}   {:<15} {}",
                    key.led,
                    key.code,
                    layout.legend(key.code)
                );
            }
        }
        Command::Status => {
            let (keyboard, mut session) = device::open(usb_path, cli.allow_unknown_firmware)?;
            let identity = session.identity().clone();
            println!(
                "Light Mount at USB {} ({})",
                keyboard.usb_path,
                keyboard.vendor_node().display()
            );
            let tested = if identity.known() {
                "tested"
            } else {
                "untested, read-only"
            };
            println!(
                "Firmware version: {} ({tested})",
                fmt_version(&identity.version)
            );
            println!("Lighting mode:    {}", session.lighting_mode()?);
        }
        Command::Info => {
            let (keyboard, mut session) = device::open(usb_path, cli.allow_unknown_firmware)?;
            println!(
                "{:<15} USB {} ({})",
                "Light Mount:",
                keyboard.usb_path,
                keyboard.vendor_node().display()
            );
            for line in report::info_lines(&mut session) {
                println!("{line}");
            }
        }
        Command::Events { count } => {
            let (_, mut session) = device::open(usb_path, cli.allow_unknown_firmware)?;
            let layout = cli
                .layout
                .or_else(|| report::device_legends(&mut session))
                .unwrap_or_default();
            eprintln!(
                "Press keys on the Light Mount (Ctrl+C to stop). Only keys the keyboard reports appear."
            );
            report::watch(&mut session, count, layout, |line| println!("{line}"))?;
        }
        Command::Light(Light::Static { color, brightness }) => {
            let (_, mut session) = device::open(usb_path, cli.allow_unknown_firmware)?;
            session.set_static(color, brightness)?;
        }
        Command::Light(Light::Effect {
            effect,
            color,
            dual,
            gradient,
            brightness,
            speed,
            direction,
        }) => {
            let colors = match (color, dual, gradient) {
                (Some(c), _, _) => Colors::Single(c),
                (None, Some(settings::DualColor(a, b)), _) => Colors::Dual(a, b),
                (None, None, Some(g)) => Colors::Gradient(g.0),
                (None, None, None) if effect == settings::Effect::STATIC => {
                    Colors::Single(Rgb::new(0xe6, 0x30, 0x00))
                }
                (None, None, None) => Colors::Gradient(settings::rainbow()),
            };
            let mut session = open_session(usb_path, cli.allow_unknown_firmware)?;
            session.set_effect(effect.0, direction.0, brightness, speed, &colors)?;
        }
        Command::Bind {
            fn_layer,
            force,
            key,
            action,
        } => {
            if key.name == "Enter" && !force {
                return Err(Box::new(device::Error::Unsupported(
                    "changing Enter could leave you unable to confirm commands: add --force"
                        .to_owned(),
                )));
            }
            let mut session = open_session(usb_path, cli.allow_unknown_firmware)?;
            if let BindAction::Macro { id } = action {
                let ids = session.macro_ids()?;
                if !ids.contains(&id) {
                    return Err(format!("the keyboard has no macro {id} (macros: {ids:?})").into());
                }
            }
            let prefix = if fn_layer { "Fn+" } else { "" };
            if let Some(action) = action.action() {
                session.set_binding(key.key_id, fn_layer, &action)?;
                println!("{prefix}{}: {action}", key.name);
            } else {
                session.clear_binding(key.key_id, fn_layer)?;
                println!("{prefix}{}: factory binding", key.name);
            }
        }
        Command::Backup { file, force } => {
            let to_stdout = file.as_os_str() == "-";
            if !to_stdout && file.exists() && !force {
                return Err(
                    format!("{} exists: add --force to overwrite it", file.display()).into(),
                );
            }
            let mut session = open_session(usb_path, cli.allow_unknown_firmware)?;
            let snapshot = config::read_snapshot(&mut session)?;
            let text = config::backup_text(&snapshot);
            if to_stdout {
                print!("{text}");
            } else {
                std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
                let macros = snapshot.macros.as_ref().map_or_else(
                    || "macros not reported".to_owned(),
                    |m| format!("{} macro(s)", m.len()),
                );
                eprintln!(
                    "Saved to {}: {} binding(s), game mode shortcuts, {macros}. The effect is not \
                     included (the keyboard does not report it). The file holds what your macros \
                     type: keep it private.",
                    file.display(),
                    snapshot.bindings.len()
                );
            }
        }
        Command::Apply { file, dry_run } => {
            let text =
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let settings = config::parse(&text).map_err(|e| format!("{}: {e}", file.display()))?;
            let mut session = open_session(usb_path, cli.allow_unknown_firmware)?;
            let current = config::read_snapshot(&mut session)?;
            let plan = config::plan(&settings, &current);
            for note in &plan.notes {
                println!("  note: {note}");
            }
            if plan.ops.is_empty() {
                println!("Nothing to change.");
            } else {
                for op in &plan.ops {
                    println!("  {op}");
                }
                if dry_run {
                    println!("{} change(s); nothing written (--dry-run).", plan.ops.len());
                } else {
                    config::execute(&mut session, &plan)?;
                    println!("{} change(s) written.", plan.ops.len());
                }
            }
        }
        Command::GameMode { block } => {
            let mut session = open_session(usb_path, cli.allow_unknown_firmware)?;
            session.set_game_mode_blocks(block.0)?;
            let blocked = quietlinux_core::info::game_mode_blocks(block.0);
            println!(
                "Game mode blocks: {}",
                if blocked.is_empty() {
                    "nothing".to_owned()
                } else {
                    blocked.join(", ")
                }
            );
        }
        Command::Light(Light::Stored) => {
            let (_, mut session) = device::open(usb_path, cli.allow_unknown_firmware)?;
            session.show_stored_effect()?;
        }
        Command::Light(Light::Off) => {
            let (_, mut session) = device::open(usb_path, cli.allow_unknown_firmware)?;
            session.set_lighting_mode(device::LightingMode::Off)?;
        }
        Command::Macro(command) => {
            let mut session = open_session(usb_path, cli.allow_unknown_firmware)?;
            run_macro(&mut session, command)?;
        }
        Command::Light(Light::Set { assignments, dim }) => {
            let (_, mut session) = device::open(usb_path, cli.allow_unknown_firmware)?;
            session.set_leds(&leds_for(&assignments, dim))?;
        }
        Command::Calibrate { start } => {
            let keys: Vec<&Key> = KEYS
                .iter()
                .filter(|k| k.led >= start.unwrap_or(0))
                .collect();
            let (_, mut session) = device::open(usb_path, cli.allow_unknown_firmware)?;
            let layout = cli
                .layout
                .or_else(|| report::device_legends(&mut session))
                .unwrap_or_default();
            println!(
                "{} keys, one at a time ({} legends). Enter = the lit key is the one shown, \
                 otherwise type what is lit; q = stop.",
                keys.len(),
                layout
            );
            let report =
                calibrate::calibrate(&mut session, &keys, |key| ask_terminal(key, layout))?;
            for m in &report.mismatches {
                println!(
                    "{:02x} {}: expected {:?}, lit: {:?}",
                    m.key.led,
                    m.key.code,
                    layout.legend(m.key.code),
                    m.lit
                );
            }
            println!(
                "{} checked, {} mismatch(es)",
                report.checked,
                report.mismatches.len()
            );
            if report.colors_lost {
                println!(
                    "Realtime colors were cleared: set them again with `quietlinux light set`."
                );
            }
            if !report.mismatches.is_empty() {
                return Ok(ExitCode::FAILURE);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Open the keyboard and a session, as IO Center does before changing settings.
fn open_session(
    usb_path: Option<&str>,
    allow_unknown_firmware: bool,
) -> Result<device::Session<device::Hidraw>, device::Error> {
    let (_, mut session) = device::open(usb_path, allow_unknown_firmware)?;
    session.open_session()?;
    Ok(session)
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("quietlinux: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn macro_events_on_the_command_line() {
        let cli = Cli::try_parse_from([
            "quietlinux",
            "macro",
            "create",
            "--repeat",
            "while-held",
            "x",
            "+shift",
            "KeyA",
            "-shift",
            "50ms",
        ])
        .unwrap();
        let Command::Macro(MacroCommand::Create { events, repeat, .. }) = cli.command else {
            panic!()
        };
        assert_eq!(events, ["+shift", "KeyA", "-shift", "50ms"]);
        assert_eq!(repeat, macros::RepeatMode(1));
        let cli = Cli::try_parse_from([
            "quietlinux",
            "macro",
            "edit",
            "3",
            "--events",
            "-ctrl",
            "KeyC",
        ])
        .unwrap();
        let Command::Macro(MacroCommand::Edit { events, .. }) = cli.command else {
            panic!()
        };
        assert_eq!(events.unwrap(), ["-ctrl", "KeyC"]);
    }

    #[test]
    fn assignment_parsing() {
        let a: Assignment = "keys=ff2a00".parse().unwrap();
        assert_eq!(
            (a.target, a.color),
            (Target::Zone(Zone::Keys), Rgb::new(0xff, 0x2a, 0))
        );
        assert_eq!(
            "all=000000".parse::<Assignment>().unwrap().target,
            Target::All
        );
        assert_eq!(
            "Escape=000000".parse::<Assignment>().unwrap().target,
            Target::Led(0x2e)
        );
        assert!("keys".parse::<Assignment>().is_err());
        assert!("wheel=ff0000".parse::<Assignment>().is_err());
        assert!("escape=ff0000".parse::<Assignment>().is_err());
        assert!("keys=red".parse::<Assignment>().is_err());
    }

    #[test]
    fn hex_byte() {
        assert_eq!(parse_hex_u8("2e"), Ok(0x2e));
        assert_eq!(parse_hex_u8("0x9D"), Ok(0x9d));
        assert!(parse_hex_u8("100").is_err());
    }

    #[test]
    fn later_assignments_win() {
        let assignments = [
            "all=111111".parse().unwrap(),
            "knob=ff0000".parse().unwrap(),
            "KeyW=00ff00".parse().unwrap(),
        ];
        let leds = leds_for(&assignments, 1.0);
        assert_eq!(leds.len(), 166);
        for led in leds {
            let expected = match led.index {
                0x2d => Rgb::new(0xff, 0, 0),
                0x57 => Rgb::new(0, 0xff, 0),
                _ => Rgb::new(0x11, 0x11, 0x11),
            };
            assert_eq!(led.color, expected, "LED {:02x}", led.index);
        }
    }

    #[test]
    fn unassigned_leds_are_off() {
        let leds = leds_for(&["knob=ff0000".parse().unwrap()], 0.5);
        assert!(leds.iter().all(|l| l.color
            == if l.index == 0x2d {
                Rgb::new(0x80, 0, 0)
            } else {
                Rgb::BLACK
            }));
    }
}
