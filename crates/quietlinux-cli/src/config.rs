//! `quietlinux backup` and `quietlinux apply`: the keyboard's settings as a TOML file.
//!
//! A backup holds what the keyboard reports and QuietLinux can write back: the key
//! bindings that differ from the factory ones and the game mode shortcuts. The
//! Light Mount does not report its effect, so an `[effect]` section is only applied
//! when the user writes one. See docs/usage-cli.md for the format.

use std::collections::{BTreeMap, HashMap};
use std::fmt::{self, Write as _};

use quietlinux_core::device::{Error, LightingMode, Session, Transport};
use quietlinux_core::info::{self, Action, Binding, Colors, Layout, parse_action};
use quietlinux_core::keymap;
use quietlinux_core::protocol as p;
use serde::Deserialize;

use crate::report::read_bindings;
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
    let (_, bindings) = read_bindings(session)?;
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
                "invalid binding {text:?}: expected key, media, mouse, lighting, shortcut, disable, default or raw"
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
# direction = "up"
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
    t.push_str(EFFECT_TEMPLATE);
    t
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
    Ok(Settings {
        game_mode,
        bindings,
        effect,
    })
}

fn effect_setting(e: EffectSection) -> Result<EffectSetting, String> {
    let effect: Effect = e.effect.parse()?;
    let direction: Direction = e.direction.as_deref().unwrap_or("up").parse()?;
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
            Colors::Single(quietlinux_core::Rgb::new(0xe6, 0x30, 0x00))
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
            Op::Effect(e) => write!(
                f,
                "effect: {}, colors {}, brightness {}, speed {}, direction {}",
                e.effect, e.colors, e.brightness, e.speed, e.direction
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

/// The changes that make the keyboard match `settings`.
pub fn plan(settings: &Settings, current: &Snapshot) -> Plan {
    let mut plan = Plan::default();
    if let Some(flags) = settings.game_mode {
        if flags != current.game_mode_flags {
            plan.ops.push(Op::GameMode(flags));
        }
    }
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
                plan.notes.push(format!(
                    "{name}: kept ({}); QuietLinux cannot recreate it, `quietlinux bind {name} default` resets it",
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

/// Make the changes.
pub fn execute<T: Transport>(session: &mut Session<T>, plan: &Plan) -> Result<(), Error> {
    for op in &plan.ops {
        match op {
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
    use quietlinux_core::device::fake::FakeKeyboard;
    use quietlinux_core::info::parse_bindings;

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
            "M5 = \"raw 0a 02 00\"  # macro 0 (action 2)",
            "# [effect]",
        ] {
            assert!(text.contains(line), "missing {line:?} in\n{text}");
        }
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
                "effect: breathing, colors e63000 and ffffff, brightness 100, speed 50, direction up",
            ]
        );
        // the macro on M5 is not in the file, but cannot be recreated: kept
        assert_eq!(plan.notes.len(), 1);
        assert!(plan.notes[0].starts_with("M5: kept"), "{}", plan.notes[0]);
    }

    #[test]
    fn parse_errors() {
        for (text, expected) in [
            ("version = 2", "version 2"),
            ("version = 1\n[bindings]\nKeyAA = \"disable\"", "KeyAA"),
            ("version = 1\n[bindings]\nM1 = \"media louder\"", "louder"),
            ("version = 1\n[bindings]\nFn = \"disable\"", "Fn key"),
            ("version = 1\n[bindings]\nM1 = \"raw 0a\"", "not a complete"),
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
        ] {
            let Desired::Action(action) = parse_desired(text).unwrap() else {
                panic!("{text}")
            };
            assert_eq!(action_text(&action).as_deref(), Some(text));
        }
        assert_eq!(action_text(&Action::Macro { action: 2, id: 0 }), None);
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
        // M1, M2, M4 reset to their factory bindings, M5 (macro) untouched
        let resets = kb
            .received
            .iter()
            .filter(|r| p::command_of(r) == p::CLEAR_BINDING)
            .count();
        assert_eq!(resets, 3);
    }
}
