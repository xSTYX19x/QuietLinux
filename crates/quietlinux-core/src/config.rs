//! `quietlinux backup` and `quietlinux apply`: the keyboard's settings as a TOML file.
//!
//! A backup holds what the keyboard reports and QuietLinux can write back: the key
//! bindings that differ from the factory ones, the game mode shortcuts and the
//! macros. The
//! Light Mount does not report its effect, so an `[effect]` section is only applied
//! when the user writes one. See docs/usage-cli.md for the format.

use std::collections::{BTreeMap, HashMap};
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::device::{Error, LightingMode, Session, Transport};
use crate::info::{self, Action, Binding, Colors, Layout, Macro, parse_action};
use crate::keymap;
use crate::protocol as p;
use serde::Deserialize;

use crate::macros::{self, RepeatMode};
use crate::settings::{
    BindKey, Direction, DualColor, Effect, GameModeBlocks, Gradient, KeyCombo, LightingAction,
    MediaAction, MouseButton, Shortcut, rainbow,
};

/// Version of the file format.
pub const VERSION: u32 = 1;

// --- reading the keyboard ---------------------------------------------------------

/// What the keyboard reports, read with read-only requests.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub model: Option<&'static str>,
    pub firmware: Option<String>,
    pub layout: Option<String>,
    pub polling_rate: Option<u16>,
    pub lighting_mode: LightingMode,
    pub game_mode_flags: u8,
    pub game_mode_on: Option<bool>,
    pub bindings_enabled: Option<bool>,
    /// Bindings that differ from the factory ones.
    pub bindings: Vec<Binding>,
    /// The macros stored in the keyboard, if it reports them.
    pub macros: Option<Vec<Macro>>,
}

pub fn read_snapshot<T: Transport>(session: &mut Session<T>) -> Result<Snapshot, Error> {
    let identity = session.identity().info();
    let layout = session
        .query(p::GET_LAYOUT, &[])
        .ok()
        .and_then(|m| Layout::parse(&m.data))
        .map(|l| {
            format!(
                "{}, {}",
                l.physical_name().unwrap_or("unknown"),
                l.visual_name().unwrap_or("unknown")
            )
        });
    let polling_rate = session
        .query(p::GET_POLLING_RATE, &[])
        .ok()
        .and_then(|m| m.data.get(..2).map(|b| u16::from_le_bytes([b[0], b[1]])));
    let lighting_mode = session.lighting_mode()?;
    let flags = session.query(p::GET_GAME_MODE_CONFIG, &[])?;
    let game_mode_flags = *flags
        .data
        .first()
        .ok_or(Error::InvalidReply(p::GET_GAME_MODE_CONFIG))?;
    let first = |m: Result<p::Message, Error>| m.ok().and_then(|m| m.data.first().copied());
    let game_mode_on = first(session.query(p::GET_GAME_MODE_STATE, &[])).map(|s| s & 1 == 1);
    let bindings_enabled = first(session.query(p::GET_BINDINGS_CONFIG, &[])).map(|e| e == 1);
    let (_, bindings) = session.read_bindings()?;
    let macros = session.read_macros().ok();
    Ok(Snapshot {
        model: identity.as_ref().and_then(info::DeviceInfo::model_name),
        firmware: identity.map(|i| {
            i.firmware
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        }),
        layout,
        polling_rate,
        lighting_mode,
        game_mode_flags,
        game_mode_on,
        bindings_enabled,
        bindings,
        macros,
    })
}

// --- binding texts ------------------------------------------------------------------

/// Key name in config files: `M3`, `Fn+F1`.
fn binding_key(key_id: u8, fn_layer: bool) -> String {
    let name =
        keymap::code_for_key_id(key_id).map_or_else(|| format!("key-{key_id:02x}"), str::to_owned);
    if fn_layer { format!("Fn+{name}") } else { name }
}

/// A binding action in the syntax of `quietlinux bind`, or `None` if it has none
/// (types QuietLinux cannot write yet).
pub fn action_text(action: &Action) -> Option<String> {
    Some(match action {
        Action::Disabled => "disable".to_owned(),
        Action::StandardKey { modifiers, usage } => {
            format!(
                "key {}",
                KeyCombo {
                    modifiers: *modifiers,
                    usage: *usage
                }
                .text()?
            )
        }
        Action::Media(a) => format!("media {}", MediaAction(*a).name()?),
        Action::MouseButton {
            button,
            flags: 0,
            auto_fire: 0,
        } => {
            format!("mouse {}", MouseButton(*button).name()?)
        }
        Action::Backlight {
            action,
            effect: None,
        } => format!("lighting {}", LightingAction(*action).name()?),
        Action::WindowsShortcut(a) => format!("shortcut {}", Shortcut(*a).name()?),
        Action::OpenWebsite(url) if info::website_ok(url) => format!("website {url}"),
        Action::Macro(id) => format!("macro {id}"),
        _ => return None,
    })
}

/// A binding as a config file states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desired {
    Action(Action),
    /// The factory binding.
    Default,
    /// Action bytes as the keyboard reported them (types QuietLinux cannot write).
    Raw(Vec<u8>),
}

/// Parse a binding text: the actions of `quietlinux bind`, or `raw` and hex bytes.
pub fn parse_desired(text: &str) -> Result<Desired, String> {
    let (kind, arg) = text
        .trim()
        .split_once(' ')
        .map_or((text.trim(), ""), |(k, a)| (k, a.trim()));
    let action = match kind {
        "key" => {
            let combo: KeyCombo = arg.parse()?;
            Action::StandardKey {
                modifiers: combo.modifiers,
                usage: combo.usage,
            }
        }
        "media" => Action::Media(arg.parse::<MediaAction>()?.0),
        "mouse" => Action::MouseButton {
            button: arg.parse::<MouseButton>()?.0,
            flags: 0,
            auto_fire: 0,
        },
        "lighting" => Action::Backlight {
            action: arg.parse::<LightingAction>()?.0,
            effect: None,
        },
        "shortcut" => Action::WindowsShortcut(arg.parse::<Shortcut>()?.0),
        "website" if info::website_ok(arg) => Action::OpenWebsite(arg.to_owned()),
        "website" => {
            return Err(format!(
                "invalid website {arg:?}: http:// or https://, at most {} characters, no spaces",
                info::MAX_WEBSITE
            ));
        }
        "macro" => Action::Macro(
            arg.parse()
                .map_err(|_| format!("invalid macro ID {arg:?} in {text:?}"))?,
        ),
        "disable" if arg.is_empty() => Action::Disabled,
        "default" if arg.is_empty() => return Ok(Desired::Default),
        "raw" => {
            let bytes = arg
                .split_whitespace()
                .map(|b| {
                    u8::from_str_radix(b, 16).map_err(|_| format!("invalid byte {b:?} in {text:?}"))
                })
                .collect::<Result<Vec<u8>, String>>()?;
            return match parse_action(&bytes) {
                Some((_, used)) if used == bytes.len() && !bytes.is_empty() => {
                    Ok(Desired::Raw(bytes))
                }
                _ => Err(format!("{text:?} is not a complete binding action")),
            };
        }
        _ => {
            return Err(format!(
                "invalid binding {text:?}: expected key, media, mouse, lighting, shortcut, website, macro, disable, default or raw"
            ));
        }
    };
    Ok(Desired::Action(action))
}

// --- the file ---------------------------------------------------------------------

/// Commented-out `[effect]` section at the end of a backup.
const EFFECT_TEMPLATE: &str = r#"
# The Light Mount does not report its effect. To set one with `apply`, remove
# the # signs and adjust (values as for `quietlinux light effect`):
# [effect]
# effect = "static"     # static, color-wave, tornado, breathing, reactive, matrix
# color = "e63000"      # or dual = ["e63000", "ffffff"], or gradient = ["ff0000@0", "0000ff@100"]
# brightness = 100
# speed = 50
# direction = "up"     # color-wave, matrix: up, down, left, right; tornado: clockwise, counterclockwise
"#;

/// Text of a backup file.
pub fn backup_text(s: &Snapshot) -> String {
    let mut t = String::new();
    let _ = writeln!(
        t,
        "# QuietLinux settings of a be quiet! Light Mount, written by `quietlinux backup`."
    );
    let _ = writeln!(
        t,
        "# Restore them with `quietlinux apply FILE` (try `--dry-run` first)."
    );
    let _ = writeln!(t, "version = {VERSION}");
    if s.macros.as_ref().is_some_and(Vec::is_empty) {
        let _ = writeln!(
            t,
            "# The keyboard has no macros: `apply` deletes any it finds."
        );
        let _ = writeln!(t, "macros = []");
    }
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "# What the keyboard reported. `apply` does not change these."
    );
    let _ = writeln!(t, "[keyboard]");
    let mut info = |key: &str, value: Option<String>| {
        if let Some(value) = value {
            let _ = writeln!(t, "{key} = {value}");
        }
    };
    let quoted = |s: &str| format!("\"{s}\"");
    info("model", s.model.map(quoted));
    info("firmware", s.firmware.as_deref().map(quoted));
    info("layout", s.layout.as_deref().map(quoted));
    info("polling_rate", s.polling_rate.map(|r| r.to_string()));
    info("lighting_mode", Some(quoted(&s.lighting_mode.to_string())));
    info(
        "game_mode",
        s.game_mode_on
            .map(|on| quoted(if on { "on" } else { "off" })),
    );
    info(
        "bindings_enabled",
        s.bindings_enabled.map(|e| e.to_string()),
    );
    let _ = writeln!(t);
    let _ = writeln!(t, "[game_mode]");
    let _ = writeln!(
        t,
        "# Shortcuts game mode blocks: shift-tab, alt-f4, win, alt-tab, caps-lock"
    );
    let blocks: Vec<String> = GameModeBlocks(s.game_mode_flags)
        .names()
        .iter()
        .map(|n| quoted(n))
        .collect();
    let _ = writeln!(t, "blocks = [{}]", blocks.join(", "));
    let _ = writeln!(t);
    let _ = writeln!(
        t,
        "# Keys whose binding differs from the factory one, in the syntax of"
    );
    let _ = writeln!(
        t,
        "# `quietlinux bind`; \"Fn+KEY\" for the Fn layer. Keys not listed get their"
    );
    let _ = writeln!(
        t,
        "# factory binding back, except bindings QuietLinux cannot write (raw)."
    );
    let _ = writeln!(t, "[bindings]");
    for b in &s.bindings {
        let key = binding_key(b.key_id, b.fn_layer);
        let key = if key.contains('+') { quoted(&key) } else { key };
        if let Some(text) = action_text(&b.action) {
            let _ = writeln!(t, "{key} = \"{text}\"");
        } else {
            let raw: Vec<String> = b.raw.iter().map(|byte| format!("{byte:02x}")).collect();
            let _ = writeln!(
                t,
                "{key} = \"raw {}\"  # {}: QuietLinux cannot write this yet; `apply` keeps it",
                raw.join(" "),
                b.action
            );
        }
    }
    match &s.macros {
        Some(list) if !list.is_empty() => macro_sections(&mut t, list),
        Some(_) => {}
        None => {
            let _ = writeln!(t);
            let _ = writeln!(
                t,
                "# The keyboard did not report its macros: `apply` leaves them alone."
            );
        }
    }
    t.push_str(EFFECT_TEMPLATE);
    t
}

/// A TOML basic string.
fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn macro_sections(t: &mut String, list: &[Macro]) {
    let _ = writeln!(t);
    for line in [
        "# Macros stored in the keyboard, events in the syntax of `quietlinux macro",
        "# create` (100 ms after each event unless a pause is given). `apply` makes the",
        "# keyboard's macros exactly these: macros not listed are deleted. Without",
        "# [[macros]], `apply` leaves the macros alone. This is what the macros type:",
        "# keep the file private.",
    ] {
        let _ = writeln!(t, "{line}");
    }
    for m in list {
        let _ = writeln!(t, "[[macros]]");
        let _ = writeln!(t, "id = {}", m.id);
        let _ = writeln!(t, "name = {}", toml_string(&m.name));
        match RepeatMode(m.repeat()).name() {
            Some(name) if m.config[1..] == p::MACRO_DEFAULT_CONFIG[1..] => {
                let _ = writeln!(t, "repeat = \"{name}\"");
            }
            _ => {
                let hex: Vec<String> = m.config.iter().map(|b| format!("{b:02x}")).collect();
                let _ = writeln!(t, "config = \"{}\"", hex.join(" "));
            }
        }
        let (raw, text) = macros::stored_events(&m.events);
        let key = if raw { "raw_events" } else { "events" };
        let _ = writeln!(t, "{key} = {}", toml_string(&text));
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    /// Informational, ignored.
    #[allow(dead_code)]
    keyboard: Option<toml::Value>,
    game_mode: Option<GameModeSection>,
    bindings: Option<BTreeMap<String, String>>,
    effect: Option<EffectSection>,
    macros: Option<Vec<MacroSection>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MacroSection {
    id: u16,
    name: String,
    repeat: Option<String>,
    /// All 4 config bytes as hex, instead of `repeat`.
    config: Option<String>,
    events: Option<String>,
    raw_events: Option<String>,
}

fn macro_setting(m: MacroSection) -> Result<Macro, String> {
    if m.id == 0 {
        return Err("id 0: macro IDs start at 1".to_owned());
    }
    if m.name.len() > usize::from(u8::MAX) {
        return Err("name: at most 255 bytes".to_owned());
    }
    let events = match (m.events, m.raw_events) {
        (Some(text), None) => macros::parse_events(&[text], macros::DEFAULT_DELAY_MS)?,
        (None, Some(hex)) => macros::parse_raw_events(&hex)?,
        _ => return Err("give one of events, raw_events".to_owned()),
    };
    let mut new = Macro::new(m.id, &m.name, events);
    match (m.repeat, m.config) {
        (Some(repeat), None) => new.set_repeat(repeat.parse::<RepeatMode>()?.0),
        (None, Some(hex)) => {
            let bytes = hex
                .split_whitespace()
                .map(|b| {
                    u8::from_str_radix(b, 16).map_err(|_| format!("config: invalid byte {b:?}"))
                })
                .collect::<Result<Vec<u8>, String>>()?;
            new.config = bytes.try_into().map_err(|_| "config: 4 bytes".to_owned())?;
        }
        (None, None) => {}
        (Some(_), Some(_)) => return Err("give only one of repeat, config".to_owned()),
    }
    Ok(new)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GameModeSection {
    blocks: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectSection {
    effect: String,
    color: Option<String>,
    dual: Option<[String; 2]>,
    gradient: Option<Vec<String>>,
    brightness: Option<u8>,
    speed: Option<u8>,
    direction: Option<String>,
}

/// An effect to set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSetting {
    pub effect: Effect,
    pub direction: Direction,
    pub brightness: u8,
    pub speed: u8,
    pub colors: Colors,
}

/// A key binding target in a config file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub key_id: u8,
    pub fn_layer: bool,
}

/// A validated config file. `None` sections are left alone by `apply`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub game_mode: Option<u8>,
    pub bindings: Option<Vec<(Target, Desired)>>,
    pub effect: Option<EffectSetting>,
    /// All macros, sorted by ID.
    pub macros: Option<Vec<Macro>>,
}

/// Parse and check a config file; nothing is sent to the keyboard.
pub fn parse(text: &str) -> Result<Settings, String> {
    let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
    if file.version != VERSION {
        return Err(format!(
            "unsupported file version {} (this QuietLinux reads version {VERSION})",
            file.version
        ));
    }
    let game_mode = file
        .game_mode
        .map(|g| {
            if g.blocks.is_empty() {
                Ok(0)
            } else {
                g.blocks.join(",").parse::<GameModeBlocks>().map(|b| b.0)
            }
        })
        .transpose()
        .map_err(|e| format!("[game_mode] blocks: {e}"))?;
    let bindings = file
        .bindings
        .map(|map| {
            map.into_iter()
                .map(|(key, text)| {
                    let (fn_layer, name) = key
                        .strip_prefix("Fn+")
                        .map_or((false, key.as_str()), |n| (true, n));
                    let target: BindKey =
                        name.parse().map_err(|e| format!("[bindings] {key}: {e}"))?;
                    let desired =
                        parse_desired(&text).map_err(|e| format!("[bindings] {key}: {e}"))?;
                    Ok((
                        Target {
                            name: key.clone(),
                            key_id: target.key_id,
                            fn_layer,
                        },
                        desired,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?;
    let effect = file
        .effect
        .map(effect_setting)
        .transpose()
        .map_err(|e| format!("[effect] {e}"))?;
    let macros = file
        .macros
        .map(|list| {
            let mut list = list
                .into_iter()
                .map(|m| {
                    let id = m.id;
                    macro_setting(m).map_err(|e| format!("[[macros]] id {id}: {e}"))
                })
                .collect::<Result<Vec<Macro>, String>>()?;
            list.sort_by_key(|m| m.id);
            if let Some(w) = list.windows(2).find(|w| w[0].id == w[1].id) {
                return Err(format!("[[macros]]: id {} given twice", w[0].id));
            }
            Ok(list)
        })
        .transpose()?;
    Ok(Settings {
        game_mode,
        bindings,
        effect,
        macros,
    })
}

fn effect_setting(e: EffectSection) -> Result<EffectSetting, String> {
    let effect: Effect = e.effect.parse()?;
    let wanted: Option<Direction> = e.direction.as_deref().map(str::parse).transpose()?;
    // a direction given for an effect that does not move is left out (older files)
    let direction = if effect.directions().is_empty() {
        Direction(0)
    } else {
        effect.direction(wanted)?
    };
    let percent = |name: &str, v: Option<u8>, default: u8| match v.unwrap_or(default) {
        v @ 0..=100 => Ok(v),
        v => Err(format!("{name} {v}: expected 0 to 100")),
    };
    let colors = match (e.color, e.dual, e.gradient) {
        (Some(c), None, None) => Colors::Single(c.parse().map_err(|err| format!("color: {err}"))?),
        (None, Some([a, b]), None) => {
            let DualColor(a, b) = format!("{a},{b}").parse()?;
            Colors::Dual(a, b)
        }
        (None, None, Some(g)) => Colors::Gradient(g.join(",").parse::<Gradient>()?.0),
        (None, None, None) if effect == Effect::STATIC => {
            Colors::Single(crate::Rgb::new(0xe6, 0x30, 0x00))
        }
        (None, None, None) => Colors::Gradient(rainbow()),
        _ => return Err("give only one of color, dual, gradient".to_owned()),
    };
    Ok(EffectSetting {
        effect,
        direction,
        brightness: percent("brightness", e.brightness, 100)?,
        speed: percent("speed", e.speed, 50)?,
        colors,
    })
}

// --- the last effect ------------------------------------------------------------------
//
// The Light Mount does not report its effect (`10:05` answers `00`), so QuietLinux
// remembers the last one it set, in a settings file that `apply` takes as it is.

/// `$XDG_CONFIG_HOME/quietlinux` (or `~/.config/quietlinux`).
pub fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("quietlinux"))
}

/// The file with the last effect QuietLinux set.
pub fn last_effect_path() -> Option<PathBuf> {
    Some(config_dir()?.join("last-effect.toml"))
}

/// An `[effect]` section, or `None` for colors a file cannot hold (oriented gradients).
pub fn effect_section(e: &EffectSetting) -> Option<String> {
    let hex = |c: &crate::Rgb| format!("\"{c}\"");
    let colors = match &e.colors {
        Colors::Single(c) => format!("color = {}", hex(c)),
        Colors::Dual(a, b) => format!("dual = [{}, {}]", hex(a), hex(b)),
        Colors::Gradient(stops) => {
            let stops: Vec<String> = stops
                .iter()
                .map(|(c, pos)| format!("\"{c}@{pos}\""))
                .collect();
            format!("gradient = [{}]", stops.join(", "))
        }
        Colors::OrientedGradient { .. } => return None,
    };
    let direction = if e.effect.directions().is_empty() {
        String::new()
    } else {
        format!("direction = \"{}\"\n", e.direction)
    };
    Some(format!(
        "[effect]\neffect = \"{}\"\n{colors}\nbrightness = {}\nspeed = {}\n{direction}",
        e.effect, e.brightness, e.speed
    ))
}

/// Remember `e` as the last effect, in `path`.
pub fn save_effect(path: &Path, e: &EffectSetting) -> std::io::Result<()> {
    let Some(section) = effect_section(e) else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = format!(
        "# The last effect QuietLinux set (the keyboard does not report it).\n\
         # `quietlinux apply` takes this file as it is.\nversion = {VERSION}\n\n{section}"
    );
    std::fs::write(path, text)
}

/// The effect remembered in `path`, if any.
pub fn load_effect(path: &Path) -> Option<EffectSetting> {
    let text = std::fs::read_to_string(path).ok()?;
    parse(&text).ok()?.effect
}

/// Remember `e` as the last effect ([`last_effect_path`]).
pub fn save_last_effect(e: &EffectSetting) -> std::io::Result<()> {
    match last_effect_path() {
        Some(path) => save_effect(&path, e),
        None => Ok(()),
    }
}

/// The last effect QuietLinux set, if it remembers one.
pub fn last_effect() -> Option<EffectSetting> {
    load_effect(&last_effect_path()?)
}

// --- applying -------------------------------------------------------------------------

/// One change `apply` makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    GameMode(u8),
    Bind {
        name: String,
        key_id: u8,
        fn_layer: bool,
        action: Action,
    },
    Reset {
        name: String,
        key_id: u8,
        fn_layer: bool,
    },
    Effect(EffectSetting),
    /// Write all macros (`summary` says what changes, without the contents).
    Macros {
        macros: Vec<Macro>,
        summary: String,
    },
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::GameMode(flags) => {
                let names = GameModeBlocks(*flags).names();
                write!(
                    f,
                    "game mode blocks: {}",
                    if names.is_empty() {
                        "nothing".to_owned()
                    } else {
                        names.join(", ")
                    }
                )
            }
            Op::Bind { name, action, .. } => write!(f, "{name}: {action}"),
            Op::Reset { name, .. } => write!(f, "{name}: factory binding"),
            Op::Macros { summary, .. } => write!(f, "macros: {summary}"),
            Op::Effect(e) => write!(
                f,
                "effect: {}, colors {}, brightness {}, speed {}{}",
                e.effect,
                e.colors,
                e.brightness,
                e.speed,
                if e.effect.directions().is_empty() {
                    String::new()
                } else {
                    format!(", direction {}", e.direction)
                }
            ),
        }
    }
}

/// Changes, and notes about what is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub ops: Vec<Op>,
    pub notes: Vec<String>,
}

impl Plan {
    /// The effect the plan sets, if any.
    pub fn effect(&self) -> Option<&EffectSetting> {
        self.ops.iter().find_map(|op| match op {
            Op::Effect(e) => Some(e),
            _ => None,
        })
    }
}

/// The changes that make the keyboard match `settings`.
pub fn plan(settings: &Settings, current: &Snapshot) -> Plan {
    let mut plan = Plan::default();
    if let Some(flags) = settings.game_mode {
        if flags != current.game_mode_flags {
            plan.ops.push(Op::GameMode(flags));
        }
    }
    let macro_ids = plan_macros(&mut plan, settings, current);
    if let Some(bindings) = &settings.bindings {
        let mut now: HashMap<(u8, bool), &Binding> = current
            .bindings
            .iter()
            .map(|b| ((b.key_id, b.fn_layer), b))
            .collect();
        for (target, desired) in bindings {
            let existing = now.remove(&(target.key_id, target.fn_layer));
            let current_raw = existing.map(|b| b.raw.as_slice());
            let (name, key_id, fn_layer) = (target.name.clone(), target.key_id, target.fn_layer);
            match desired {
                Desired::Default => {
                    if existing.is_some() {
                        plan.ops.push(Op::Reset {
                            name,
                            key_id,
                            fn_layer,
                        });
                    }
                }
                Desired::Action(Action::Macro(id))
                    if macro_ids.as_ref().is_some_and(|m| !m.contains(id)) =>
                {
                    plan.notes.push(format!(
                        "{name}: the keyboard has no macro {id}, left as it is"
                    ));
                }
                Desired::Action(action) => {
                    if current_raw != action.to_bytes().as_deref() {
                        plan.ops.push(Op::Bind {
                            name,
                            key_id,
                            fn_layer,
                            action: action.clone(),
                        });
                    }
                }
                Desired::Raw(bytes) => {
                    if current_raw == Some(bytes.as_slice()) {
                        continue;
                    }
                    match parse_action(bytes).map(|(a, _)| a) {
                        Some(action) if action.to_bytes().as_deref() == Some(bytes.as_slice()) => {
                            plan.ops.push(Op::Bind {
                                name,
                                key_id,
                                fn_layer,
                                action,
                            });
                        }
                        _ => plan.notes.push(format!(
                            "{name}: cannot write \"raw\" bindings yet, left as it is"
                        )),
                    }
                }
            }
        }
        let mut rest: Vec<&Binding> = now.into_values().collect();
        rest.sort_by_key(|b| (b.fn_layer, b.key_id));
        for b in rest {
            let name = binding_key(b.key_id, b.fn_layer);
            if b.action.to_bytes().is_some() {
                plan.ops.push(Op::Reset {
                    name,
                    key_id: b.key_id,
                    fn_layer: b.fn_layer,
                });
            } else {
                let key = binding_key(b.key_id, false);
                let fn_flag = if b.fn_layer { "--fn " } else { "" };
                plan.notes.push(format!(
                    "{name}: kept ({}); QuietLinux cannot recreate it, `quietlinux bind {fn_flag}{key} default` resets it",
                    b.action
                ));
            }
        }
    }
    if let Some(effect) = &settings.effect {
        plan.ops.push(Op::Effect(effect.clone()));
    }
    plan
}

/// The macro ID a binding will name, if any.
fn bound_macro(desired: &Desired) -> Option<u16> {
    match desired {
        Desired::Action(Action::Macro(id)) => Some(*id),
        Desired::Raw(bytes) => match parse_action(bytes) {
            Some((Action::Macro(id), _)) => Some(id),
            _ => None,
        },
        _ => None,
    }
}

/// Plan the macros; returns the IDs the keyboard will have, if known.
fn plan_macros(plan: &mut Plan, settings: &Settings, current: &Snapshot) -> Option<Vec<u16>> {
    let now = current.macros.as_ref();
    let Some(wanted) = &settings.macros else {
        return now.map(|list| list.iter().map(|m| m.id).collect());
    };
    let Some(now) = now else {
        plan.notes
            .push("macros: the keyboard did not report its macros, left alone".to_owned());
        return None;
    };
    // macros keys stay bound to are not deleted
    let bound: Vec<u16> = match &settings.bindings {
        Some(bindings) => bindings
            .iter()
            .filter_map(|(_, d)| bound_macro(d))
            .collect(),
        None => current
            .bindings
            .iter()
            .filter_map(|b| match b.action {
                Action::Macro(id) => Some(id),
                _ => None,
            })
            .collect(),
    };
    let mut wanted = wanted.clone();
    for m in now {
        if bound.contains(&m.id) && wanted.iter().all(|w| w.id != m.id) {
            plan.notes.push(format!(
                "macro {} ({:?}): kept, a key stays bound to it",
                m.id, m.name
            ));
            wanted.push(m.clone());
        }
    }
    wanted.sort_by_key(|m| m.id);
    if wanted != *now {
        let changed = wanted
            .iter()
            .filter(|w| now.iter().any(|n| n.id == w.id && n != *w))
            .count();
        let new = wanted
            .iter()
            .filter(|w| now.iter().all(|n| n.id != w.id))
            .count();
        let deleted = now
            .iter()
            .filter(|n| wanted.iter().all(|w| w.id != n.id))
            .count();
        plan.ops.push(Op::Macros {
            summary: format!(
                "{changed} changed, {new} new, {deleted} deleted ({} in all)",
                wanted.len()
            ),
            macros: wanted.clone(),
        });
    }
    Some(wanted.iter().map(|m| m.id).collect())
}

/// Make the changes.
pub fn execute<T: Transport>(
    session: &mut Session<T>,
    plan: &Plan,
) -> Result<(), Box<dyn std::error::Error>> {
    for op in &plan.ops {
        match op {
            Op::Macros { macros, .. } => macros::write_verified(session, macros)?,
            Op::GameMode(flags) => session.set_game_mode_blocks(*flags)?,
            Op::Bind {
                key_id,
                fn_layer,
                action,
                ..
            } => session.set_binding(*key_id, *fn_layer, action)?,
            Op::Reset {
                key_id, fn_layer, ..
            } => session.clear_binding(*key_id, *fn_layer)?,
            Op::Effect(e) => {
                session.set_effect(e.effect.0, e.direction.0, e.brightness, e.speed, &e.colors)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::device::fake::FakeKeyboard;
    use crate::info::parse_bindings;

    use super::*;

    /// The reference keyboard as captured: M1–M5 changed, M5 bound to a macro.
    const CAPTURED_BINDINGS: [u8; 24] = [
        0x05, 0x00, 0x02, 0x00, 0x02, 0x04, 0x03, 0x00, 0x01, 0x00, 0x68, 0x04, 0x00, 0x02, 0x05,
        0x05, 0x00, 0x02, 0x07, 0x06, 0x00, 0x0a, 0x02, 0x00,
    ];

    fn snapshot() -> Snapshot {
        Snapshot {
            model: Some("Light Mount"),
            firmware: Some("2.3.0".to_owned()),
            layout: Some("ISO, German".to_owned()),
            polling_rate: Some(1000),
            lighting_mode: LightingMode::General,
            game_mode_flags: 0x1f,
            game_mode_on: Some(false),
            bindings_enabled: Some(true),
            bindings: parse_bindings(&CAPTURED_BINDINGS).unwrap().1,
            macros: Some(vec![
                Macro::new(
                    1,
                    "Makro 1",
                    macros::parse_events(&["KeyA KeyB".into()], 100).unwrap(),
                ),
                Macro::new(
                    2,
                    "q\"x",
                    macros::parse_events(&["ctrl+KeyC 7ms".into()], 100).unwrap(),
                ),
            ]),
        }
    }

    #[test]
    fn backup_of_the_reference_keyboard() {
        let text = backup_text(&snapshot());
        for line in [
            "version = 1",
            "model = \"Light Mount\"",
            "polling_rate = 1000",
            "blocks = [\"shift-tab\", \"alt-f4\", \"win\", \"alt-tab\", \"caps-lock\"]",
            "M1 = \"media mic-mute\"",
            "M2 = \"key F13\"",
            "M3 = \"media next-track\"",
            "M4 = \"media play-pause\"",
            "M5 = \"macro 2\"",
            "# [effect]",
        ] {
            assert!(text.contains(line), "missing {line:?} in\n{text}");
        }
    }

    #[test]
    fn backup_contains_the_macros() {
        let mut current = snapshot();
        let macros = current.macros.as_mut().unwrap();
        macros[1].set_repeat(p::MACRO_WHILE_HELD);
        macros.push(Macro {
            id: 5,
            name: "odd".into(),
            config: [9, 9, 0, 0],
            events: vec![p::MacroEvent {
                kind: 6,
                usage: 0xe9,
                delay_ms: 300,
            }],
        });
        let text = backup_text(&current);
        for line in [
            "[[macros]]\nid = 1\nname = \"Makro 1\"\nrepeat = \"once\"\nevents = \"KeyA KeyB\"\n",
            "id = 2\nname = \"q\\\"x\"\nrepeat = \"while-held\"\nevents = \"+ctrl KeyC -ctrl 7ms\"\n",
            "id = 5\nname = \"odd\"\nconfig = \"09 09 00 00\"\nraw_events = \"06 e9 2c 01\"\n",
            "keep the file private",
        ] {
            assert!(text.contains(line), "missing {line:?} in\n{text}");
        }
        assert!(!text.contains("macros = []"));
        let settings = parse(&text).unwrap();
        assert_eq!(settings.macros.as_ref(), current.macros.as_ref());
        assert_eq!(plan(&settings, &current), Plan::default());
    }

    #[test]
    fn no_macros() {
        let mut current = snapshot();
        current.macros = Some(Vec::new());
        current.bindings.clear();
        let text = backup_text(&current);
        assert!(text.contains("\nmacros = []\n"), "{text}");
        assert_eq!(parse(&text).unwrap().macros, Some(Vec::new()));
        // a keyboard that does not report them: nothing in the file, apply leaves them
        current.macros = None;
        let text = backup_text(&current);
        assert!(text.contains("did not report its macros"));
        assert_eq!(parse(&text).unwrap().macros, None);
    }

    #[test]
    fn plan_macros() {
        // the file has macro 1 (changed) and a new 3, not 2: M5 stays bound to 2
        let text = "version = 1\n\
                    [[macros]]\nid = 1\nname = \"Makro 1\"\nevents = \"KeyC\"\n\
                    [[macros]]\nid = 3\nname = \"new\"\nrepeat = \"repeat\"\nevents = \"KeyA\"\n";
        let settings = parse(text).unwrap();
        let plan = plan(&settings, &snapshot());
        let shown: Vec<String> = plan.ops.iter().map(ToString::to_string).collect();
        assert_eq!(shown, ["macros: 1 changed, 1 new, 0 deleted (3 in all)"]);
        assert_eq!(
            plan.notes,
            ["macro 2 (\"q\\\"x\"): kept, a key stays bound to it"]
        );
        let Op::Macros { macros, .. } = &plan.ops[0] else {
            panic!()
        };
        assert_eq!(macros.iter().map(|m| m.id).collect::<Vec<_>>(), [1, 2, 3]);
        // with bindings that no longer name macro 2, it is deleted; M4 may name the new 3
        let text = format!(
            "{text}[bindings]\nM1 = \"media mic-mute\"\nM2 = \"key F13\"\nM3 = \"media next-track\"\n\
             M4 = \"macro 3\"\nM5 = \"macro 9\"\n"
        );
        let plan = super::plan(&parse(&text).unwrap(), &snapshot());
        let shown: Vec<String> = plan.ops.iter().map(ToString::to_string).collect();
        assert_eq!(
            shown,
            [
                "macros: 1 changed, 1 new, 1 deleted (2 in all)",
                "M4: macro 3"
            ]
        );
        assert_eq!(
            plan.notes,
            ["M5: the keyboard has no macro 9, left as it is"]
        );
    }

    #[test]
    fn macro_parse_errors() {
        for (section, expected) in [
            ("id = 0\nname = \"a\"\nevents = \"KeyA\"", "IDs start at 1"),
            ("id = 1\nname = \"a\"", "one of events, raw_events"),
            (
                "id = 1\nname = \"a\"\nevents = \"KeyA\"\nrepeat = \"often\"",
                "often",
            ),
            ("id = 1\nname = \"a\"\nraw_events = \"02 04 00\"", "4 bytes"),
            (
                "id = 1\nname = \"a\"\nevents = \"KeyA\"\nconfig = \"00 02\"",
                "config: 4 bytes",
            ),
        ] {
            let err = parse(&format!("version = 1\n[[macros]]\n{section}\n")).unwrap_err();
            assert!(err.contains(expected), "{section:?}: {err}");
        }
        let twice = "version = 1\n[[macros]]\nid = 1\nname = \"a\"\nevents = \"KeyA\"\n\
                     [[macros]]\nid = 1\nname = \"b\"\nevents = \"KeyB\"\n";
        assert!(parse(twice).unwrap_err().contains("given twice"));
    }

    #[test]
    fn macros_restored_on_a_simulated_keyboard() {
        let mut kb = FakeKeyboard::tested();
        kb.answers.insert(p::GET_GAME_MODE_CONFIG, vec![0x1f]);
        kb.answers.insert(p::GET_BINDINGS, vec![0, 0]);
        for m in snapshot().macros.unwrap() {
            kb.macros.insert(m.id, m);
        }
        let mut session = Session::new(&mut kb, false).unwrap();
        let text = backup_text(&read_snapshot(&mut session).unwrap());
        // the macros change, then the backup is applied
        let mut changed = snapshot().macros.unwrap();
        changed[0].name = "other".into();
        changed.remove(1);
        macros::write_verified(&mut session, &changed).unwrap();
        let current = read_snapshot(&mut session).unwrap();
        let plan = plan(&parse(&text).unwrap(), &current);
        execute(&mut session, &plan).unwrap();
        assert_eq!(session.read_macros().unwrap(), snapshot().macros.unwrap());
    }

    #[test]
    fn a_backup_applied_to_the_same_keyboard_changes_nothing() {
        let settings = parse(&backup_text(&snapshot())).unwrap();
        assert_eq!(settings.effect, None);
        assert_eq!(plan(&settings, &snapshot()), Plan::default());
    }

    #[test]
    fn plan_changes() {
        let text = "version = 1\n\
                    [game_mode]\nblocks = [\"win\"]\n\
                    [bindings]\n\
                    M1 = \"media mic-mute\"\n\
                    M3 = \"media stop\"\n\
                    M4 = \"key ctrl+KeyC\"\n\
                    \"Fn+F1\" = \"media play-pause\"\n\
                    CapsLock = \"default\"\n\
                    [effect]\neffect = \"breathing\"\ndual = [\"e63000\", \"ffffff\"]\n";
        let settings = parse(text).unwrap();
        let plan = plan(&settings, &snapshot());
        let shown: Vec<String> = plan.ops.iter().map(ToString::to_string).collect();
        assert_eq!(
            shown,
            [
                "game mode blocks: win",
                "Fn+F1: media: play/pause",
                "M3: media: stop",
                "M4: key LCtrl+C",
                "M2: factory binding",
                "M5: factory binding",
                "effect: breathing, colors e63000 and ffffff, brightness 100, speed 50",
            ]
        );
        assert_eq!(plan.notes, Vec::<String>::new());
    }

    #[test]
    fn kept_binding_note_names_the_command() {
        let mut current = snapshot();
        current.bindings = parse_bindings(&[1, 0, 0x55, 0x80, 0x08, 0x03]).unwrap().1;
        let plan = plan(&parse("version = 1\n[bindings]\n").unwrap(), &current);
        assert_eq!(plan.ops, []);
        assert_eq!(
            plan.notes,
            [
                "Fn+Home: kept (profile: next); QuietLinux cannot recreate it, \
              `quietlinux bind --fn Home default` resets it"
            ]
        );
    }

    #[test]
    fn missing_macro_is_not_bound() {
        let text = "version = 1\n[bindings]\nM1 = \"macro 7\"\nM5 = \"macro 1\"\n";
        let plan = plan(&parse(text).unwrap(), &snapshot());
        let shown: Vec<String> = plan.ops.iter().map(ToString::to_string).collect();
        assert_eq!(
            shown,
            [
                "M5: macro 1",
                "M2: factory binding",
                "M3: factory binding",
                "M4: factory binding"
            ]
        );
        assert_eq!(
            plan.notes,
            ["M1: the keyboard has no macro 7, left as it is"]
        );
    }

    #[test]
    fn last_effect_round_trips() {
        let dir = std::env::temp_dir().join(format!("quietlinux-last-{}", std::process::id()));
        let path = dir.join("last-effect.toml");
        assert_eq!(load_effect(&path), None);
        for colors in [
            Colors::Single(crate::Rgb::new(0xe6, 0x30, 0)),
            Colors::Dual(crate::Rgb::new(1, 2, 3), crate::Rgb::new(4, 5, 6)),
            Colors::Gradient(rainbow()),
        ] {
            let e = EffectSetting {
                effect: Effect(1),
                direction: Direction(3),
                brightness: 70,
                speed: 20,
                colors,
            };
            save_effect(&path, &e).unwrap();
            assert_eq!(load_effect(&path), Some(e));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_errors() {
        for (text, expected) in [
            ("version = 2", "version 2"),
            ("version = 1\n[bindings]\nKeyAA = \"disable\"", "KeyAA"),
            ("version = 1\n[bindings]\nM1 = \"media louder\"", "louder"),
            ("version = 1\n[bindings]\nFn = \"disable\"", "Fn key"),
            ("version = 1\n[bindings]\nM1 = \"raw 0a\"", "not a complete"),
            (
                "version = 1\n[bindings]\nM1 = \"macro x\"",
                "invalid macro ID",
            ),
            ("version = 1\n[game_mode]\nblocks = [\"escape\"]", "escape"),
            ("version = 1\n[effect]\neffect = \"ripple\"", "ripple"),
            (
                "version = 1\n[effect]\neffect = \"static\"\nbrightness = 150",
                "brightness 150",
            ),
            (
                "version = 1\n[effect]\neffect = \"static\"\ncolor = \"ff0000\"\ndual = [\"ff0000\", \"00ff00\"]",
                "only one",
            ),
            ("version = 1\n[lighting]\nmode = 1", "unknown field"),
        ] {
            let err = parse(text).unwrap_err();
            assert!(err.contains(expected), "{text:?}: {err}");
        }
    }

    #[test]
    fn action_texts_round_trip() {
        for text in [
            "disable",
            "key ctrl+KeyC",
            "key F13",
            "media stop",
            "mouse middle",
            "lighting next-effect",
            "shortcut calculator",
            "macro 2",
            "website https://example.org",
        ] {
            let Desired::Action(action) = parse_desired(text).unwrap() else {
                panic!("{text}")
            };
            assert_eq!(action_text(&action).as_deref(), Some(text));
        }
        assert_eq!(action_text(&Action::ProfileSelect([0; 16])), None);
        assert_eq!(parse_desired("default"), Ok(Desired::Default));
        assert_eq!(
            parse_desired("raw 0a 02 00"),
            Ok(Desired::Raw(vec![0x0a, 0x02, 0x00]))
        );
    }

    #[test]
    fn backup_and_apply_on_a_simulated_keyboard() {
        let mut kb = FakeKeyboard::tested();
        kb.answers.insert(p::GET_GAME_MODE_CONFIG, vec![0x1f]);
        kb.answers
            .insert(p::GET_BINDINGS, CAPTURED_BINDINGS.to_vec());
        let mut session = Session::new(&mut kb, false).unwrap();
        let current = read_snapshot(&mut session).unwrap();
        assert_eq!(current.bindings.len(), 5);
        let settings =
            parse("version = 1\n[game_mode]\nblocks = []\n[bindings]\nM3 = \"media stop\"\n")
                .unwrap();
        execute(&mut session, &plan(&settings, &current)).unwrap();
        assert_eq!(kb.game_mode_flags, 0);
        assert_eq!(kb.bindings[&(0x04, p::LAYER_NORMAL)], [0x02, 0x08]);
        // M1, M2, M4, M5 reset to their factory bindings
        let resets = kb
            .received
            .iter()
            .filter(|r| p::command_of(r) == p::CLEAR_BINDING)
            .count();
        assert_eq!(resets, 4);
    }
}
