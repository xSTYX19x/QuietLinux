//! Import of IO Center for Windows profiles: an `.ioprofile` file, or the `.zip`
//! of IO Center's backup (its profiles are in `profiles/*.ioprofile`).
//!
//! A profile is JSON written by the C++ library cereal (see docs/iocenter.md):
//! a list of "properties", among them the key bindings, the macros and the
//! lighting of each device. Variants are stored as `{"index": N, "data": …}`.
//! Only what was checked against an export and a capture of the Windows app is
//! imported; everything else is listed in the import's notes, never guessed.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::Rgb;
use crate::config::{self, EffectSetting};
use crate::info::{Action, Binding, Colors, Macro};
use crate::keymap;
use crate::profile;
use crate::protocol as p;
use crate::settings::{BindKey, Direction, Effect};
use crate::state::{Lighting, Mode};

/// Largest file read, and largest profile unpacked from a backup.
const MAX_FILE: u64 = 64 << 20;
const MAX_PROFILE: usize = 4 << 20;

/// The device's name in IO Center's profiles.
const DEVICE: &str = "Light Mount";

/// A profile read from IO Center.
#[derive(Debug, Clone)]
pub struct Imported {
    /// The profile's name in IO Center.
    pub source: String,
    /// A QuietLinux profile name made from it.
    pub name: String,
    /// The QuietLinux profile file.
    pub text: String,
    /// What was not imported, and why.
    pub notes: Vec<String>,
}

/// Read an `.ioprofile` file or IO Center's backup (`.zip`).
pub fn read(path: &Path) -> Result<Vec<Imported>, String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();
    if size > MAX_FILE {
        return Err(format!("{}: larger than 64 MB", path.display()));
    }
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    from_bytes(&data).map_err(|e| format!("{}: {e}", path.display()))
}

/// The profiles in a backup (`.zip`) or a single profile (JSON).
pub fn from_bytes(data: &[u8]) -> Result<Vec<Imported>, String> {
    let mut list = if data.starts_with(b"PK\x03\x04") {
        let files = zip_profiles(data)?;
        if files.is_empty() {
            return Err("no profiles in this backup (profiles/*.ioprofile)".to_owned());
        }
        files
            .iter()
            .map(|(file, json)| profile(json).map_err(|e| format!("{file}: {e}")))
            .collect::<Result<Vec<_>, String>>()?
    } else {
        vec![profile(data)?]
    };
    list.sort_by(|a, b| a.source.cmp(&b.source));
    // two profiles with the same name: the later ones get a number
    let mut taken: Vec<String> = Vec::new();
    for imported in &mut list {
        imported.name = profile::numbered(&imported.name, |n| taken.iter().any(|t| t == n));
        taken.push(imported.name.clone());
    }
    Ok(list)
}

/// A QuietLinux profile name for an IO Center profile name: other characters
/// become `-`, at most 16 characters.
pub fn profile_name(source: &str) -> String {
    let mut name = String::new();
    for c in source.chars() {
        let c = if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            c
        } else {
            '-'
        };
        if !(c == '-' && (name.is_empty() || name.ends_with('-'))) {
            name.push(c);
        }
    }
    name.truncate(profile::MAX_NAME);
    let name = name.trim_end_matches('-').to_owned();
    if profile::check_name(&name).is_ok() {
        name
    } else {
        "iocenter".to_owned()
    }
}

// --- the zip file -----------------------------------------------------------------

fn u16_at(data: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(u16::from_le_bytes(
        data.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn u32_at(data: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?)).ok()
}

/// The `.ioprofile` files of a zip archive: name and contents. Stored and
/// deflated files; no ZIP64 (a backup is a few MB).
fn zip_profiles(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let broken = || "not a readable zip file".to_owned();
    // the end of central directory record: 22 bytes and a comment of up to 64 KB
    let search_from = data.len().saturating_sub(22 + 0xffff);
    let end = (search_from..data.len().saturating_sub(21))
        .rev()
        .find(|&i| data[i..].starts_with(b"PK\x05\x06"))
        .ok_or_else(broken)?;
    let count = u16_at(data, end + 10).ok_or_else(broken)?;
    let mut at = u32_at(data, end + 16).ok_or_else(broken)?;
    let mut files = Vec::new();
    for _ in 0..count {
        if !data.get(at..).is_some_and(|d| d.starts_with(b"PK\x01\x02")) {
            return Err(broken());
        }
        let field = |offset: usize| u16_at(data, at + offset).ok_or_else(broken);
        let (method, name_len, extra_len, comment_len) =
            (field(10)?, field(28)?, field(30)?, field(32)?);
        let size = |offset: usize| u32_at(data, at + offset).ok_or_else(broken);
        let (packed, unpacked, local) = (size(20)?, size(24)?, size(42)?);
        let name = data.get(at + 46..at + 46 + name_len).ok_or_else(broken)?;
        let name = String::from_utf8_lossy(name).into_owned();
        at += 46 + name_len + extra_len + comment_len;
        if !name.ends_with(".ioprofile") {
            continue;
        }
        if packed == 0xffff_ffff || unpacked == 0xffff_ffff {
            return Err(format!("{name}: ZIP64 is not supported"));
        }
        if unpacked > MAX_PROFILE {
            return Err(format!("{name}: larger than 4 MB"));
        }
        if !data
            .get(local..)
            .is_some_and(|d| d.starts_with(b"PK\x03\x04"))
        {
            return Err(broken());
        }
        let start = local
            + 30
            + u16_at(data, local + 26).ok_or_else(broken)?
            + u16_at(data, local + 28).ok_or_else(broken)?;
        let packed = data.get(start..start + packed).ok_or_else(broken)?;
        let contents = match method {
            0 => packed.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(packed, MAX_PROFILE)
                .map_err(|_| format!("{name}: cannot unpack"))?,
            m => return Err(format!("{name}: compression method {m} is not supported")),
        };
        if contents.len() != unpacked {
            return Err(format!("{name}: damaged"));
        }
        files.push((name, contents));
    }
    files.sort();
    Ok(files)
}

// --- the profile --------------------------------------------------------------------

/// The data of a cereal pointer (`{"ptr_wrapper": {"data": …}}`).
fn pointee(v: &Value) -> Option<&Value> {
    v.get("ptr_wrapper")?.get("data")
}

/// The Light Mount's part of a property (`subs`: one entry per device).
fn device_part(property: &Value) -> Option<&Value> {
    pointee(property)?
        .get("subs")?
        .as_array()?
        .iter()
        .find(|s| s.get("key").and_then(Value::as_str) == Some(DEVICE))
        .and_then(|s| pointee(s.get("value")?))
}

/// A variant: its index and data.
fn variant(v: &Value) -> Option<(u64, &Value)> {
    let imp = v.get("impl")?;
    Some((imp.get("index")?.as_u64()?, imp.get("data")?))
}

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

/// One IO Center profile (`.ioprofile`, JSON).
pub fn profile(json: &[u8]) -> Result<Imported, String> {
    let json = json.strip_prefix(b"\xef\xbb\xbf").unwrap_or(json);
    let root: Value =
        serde_json::from_slice(json).map_err(|e| format!("not an IO Center profile: {e}"))?;
    let data = root
        .get("data")
        .filter(|d| d.get("properties").is_some())
        .ok_or("not an IO Center profile: no \"properties\"")?;
    let source = text(data, "name").unwrap_or("IO Center").to_owned();
    let mut import = Import::default();
    let properties = data
        .get("properties")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let part = |name: &str| {
        properties
            .iter()
            .find(|p| text(p, "polymorphic_name") == Some(name))
            .and_then(device_part)
    };
    // macros first: the bindings name them
    if let Some(macros) = part("MacrosProperty") {
        import.macros(macros);
    }
    match part("KeyBindingsProperty") {
        Some(bindings) => import.bindings(bindings),
        None => import.note("no key bindings of the Light Mount in this profile"),
    }
    match part("LightingsProperty") {
        Some(lighting) => import.lighting(lighting),
        None => import.note("no lighting of the Light Mount in this profile"),
    }
    if text(data, "linkedAppPath").is_some_and(|p| !p.is_empty()) {
        import.note("the profile is linked to a program: QuietLinux does not switch by program");
    }
    let mut comment = vec![format!(
        "Imported from the IO Center for Windows profile {source:?} by `quietlinux profile import`."
    )];
    comment.extend(import.notes.iter().map(|n| format!("Not imported: {n}")));
    import.bindings.sort_by_key(|b| (b.fn_layer, b.key_id));
    let text = config::settings_text(
        &comment,
        &import.bindings,
        import.macro_list.as_deref(),
        import.effect.as_ref(),
        import.light.as_ref(),
    );
    // the result must be a profile QuietLinux reads
    config::parse(&text).map_err(|e| format!("import failed: {e}"))?;
    Ok(Imported {
        name: profile_name(&source),
        source,
        text,
        notes: import.notes,
    })
}

/// What is imported so far.
#[derive(Debug, Default)]
struct Import {
    bindings: Vec<Binding>,
    /// `None`: the profile has no macros property (the keyboard's stay).
    macro_list: Option<Vec<Macro>>,
    /// IO Center's macro IDs (UUIDs) and the IDs they get.
    macro_ids: HashMap<String, u16>,
    effect: Option<EffectSetting>,
    light: Option<Lighting>,
    notes: Vec<String>,
}

impl Import {
    fn note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }

    fn macros(&mut self, part: &Value) {
        let list = part.get("macros").and_then(Value::as_array);
        let mut imported = Vec::new();
        for m in list.into_iter().flatten() {
            let name = text(m, "name").unwrap_or("macro");
            // ASCII, as the keyboard stores it
            let name: String = name
                .chars()
                .map(|c| {
                    if c.is_ascii() && !c.is_ascii_control() {
                        c
                    } else {
                        '?'
                    }
                })
                .take(usize::from(u8::MAX))
                .collect();
            let events = m.get("events").and_then(Value::as_array);
            let events: Option<Vec<p::MacroEvent>> = events
                .into_iter()
                .flatten()
                .map(macro_event)
                .collect::<Result<_, String>>()
                .map_err(|e| self.note(format!("macro {name:?}: {e}")))
                .ok();
            let Some(events) = events else { continue };
            let Ok(id) = u16::try_from(imported.len() + 1) else {
                break;
            };
            let mut new = Macro::new(id, &name, events);
            match m.get("config").and_then(|c| text(c, "playbackMode")) {
                Some("Once") | None => {}
                Some(mode) => {
                    self.note(format!(
                        "macro {name:?}: playback mode {mode:?} is not known yet, imported as once"
                    ));
                }
            }
            new.set_repeat(p::MACRO_ONCE);
            if let Some(uuid) = text(m, "id") {
                self.macro_ids.insert(uuid.to_owned(), id);
            }
            imported.push(new);
        }
        self.macro_list = Some(imported);
    }

    fn bindings(&mut self, part: &Value) {
        let notes = self.notes.len();
        let layers = part.get("layerBindingsData").and_then(Value::as_array);
        for layer in layers.into_iter().flatten() {
            let fn_layer = match text(layer, "key") {
                Some("Common") => false,
                Some("Fn1") => true,
                other => {
                    self.note(format!("bindings of layer {other:?}: layer not known"));
                    continue;
                }
            };
            let prefix = if fn_layer { "Fn+" } else { "" };
            let list = layer.get("value").and_then(Value::as_array);
            for entry in list.into_iter().flatten() {
                let io_key = text(entry, "key").unwrap_or("?");
                let Some(key) = bind_key(io_key) else {
                    self.note(format!("{prefix}{io_key}: key not known"));
                    continue;
                };
                match entry.get("value").map(|v| self.action(v)) {
                    Some(Ok(action)) => {
                        let raw = action.to_bytes().unwrap_or_default();
                        self.bindings.push(Binding {
                            key_id: key.key_id,
                            fn_layer,
                            action,
                            raw,
                        });
                    }
                    Some(Err(e)) => self.note(format!("{prefix}{}: {e}", key.name)),
                    None => self.note(format!("{prefix}{}: no binding", key.name)),
                }
            }
        }
        if self.notes.len() > notes {
            self.note("keys whose binding was not imported get their factory binding when switching to the profile");
        }
    }

    /// A binding (variant).
    fn action(&self, v: &Value) -> Result<Action, String> {
        let (index, data) = variant(v).ok_or("binding not readable")?;
        let named = |key: &str, table: &[(&str, u8)]| {
            let name = text(data, key).unwrap_or("");
            table
                .iter()
                .find(|(n, _)| *n == name)
                .map(|&(_, v)| v)
                .ok_or_else(|| format!("{key} {name:?} not known yet"))
        };
        Ok(match index {
            // a key; modifiers: their encoding is not known yet
            2 => {
                let key = text(data, "key").unwrap_or("?");
                let usage = hid_usage(key).ok_or_else(|| format!("key {key:?} not known yet"))?;
                match data.get("modifiers").and_then(Value::as_u64) {
                    Some(0) | None => Action::StandardKey {
                        modifiers: 0,
                        usage,
                    },
                    Some(m) => return Err(format!("key with modifiers ({m}) not known yet")),
                }
            }
            4 => Action::Media(named("action", &MEDIA)?),
            12 => Action::Backlight {
                action: named("action", &LIGHTING)?,
                effect: None,
            },
            13 => {
                let uuid = text(data, "macroId").unwrap_or("");
                Action::Macro(
                    *self
                        .macro_ids
                        .get(uuid)
                        .ok_or("its macro was not imported")?,
                )
            }
            n => return Err(format!("binding type {n} not known yet")),
        })
    }

    fn lighting(&mut self, part: &Value) {
        let enabled = part.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        match text(part, "mode") {
            Some("General") => {}
            mode => {
                self.note(format!(
                    "lighting mode {mode:?} (per-key colors) not known yet"
                ));
                return;
            }
        }
        let effects = part
            .get("generalModeData")
            .and_then(|g| g.get("data"))
            .and_then(Value::as_array);
        match effects.and_then(|e| e.first()).map(effect) {
            Some(Ok(e)) => self.effect = Some(e),
            Some(Err(e)) => self.note(format!("effect: {e}")),
            None => self.note("no effect in the profile"),
        }
        self.light = Some(Lighting {
            mode: if enabled { Mode::Effect } else { Mode::Off },
            leds: std::collections::BTreeMap::new(),
        });
    }
}

/// IO Center's media actions and the keyboard's.
const MEDIA: [(&str, u8); 8] = [
    ("IncreaseVolume", 1),
    ("DecreaseVolume", 2),
    ("Mute", 3),
    ("MicMute", 4),
    ("NextTrack", 5),
    ("PrevTrack", 6),
    ("PlayPause", 7),
    ("Stop", 8),
];

/// IO Center's lighting actions and the keyboard's (next effect is `09 02`).
const LIGHTING: [(&str, u8); 4] = [
    ("NextEffect", 2),
    ("PrevEffect", 3),
    ("IncreaseBrightness", 4),
    ("DecreaseBrightness", 5),
];

/// A key IO Center names: by its US HID meaning (`Key_Y` is HID `1c`, the key
/// with Z on German keyboards), checked against the macros of an export and
/// their capture. Names not seen yet are not guessed.
fn hid_usage(name: &str) -> Option<u8> {
    let name = name.strip_prefix("Key_")?;
    if let [c] = name.as_bytes() {
        return match c {
            b'A'..=b'Z' => Some(0x04 + (c - b'A')),
            b'1'..=b'9' => Some(0x1e + (c - b'1')),
            b'0' => Some(0x27),
            _ => None,
        };
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
        return match n {
            1..=12 => Some(0x39 + n),
            13..=24 => Some(0x68 + n - 13),
            _ => None,
        };
    }
    Some(match name {
        "Enter" => 0x28,
        "LeftShift" => 0xe1,
        "Insert" => 0x49,
        "Home" => 0x4a,
        "PageUp" => 0x4b,
        "Delete" => 0x4c,
        "End" => 0x4d,
        "PageDown" => 0x4e,
        "RightArrow" => 0x4f,
        "LeftArrow" => 0x50,
        "DownArrow" => 0x51,
        "UpArrow" => 0x52,
        _ => return None,
    })
}

/// The key a binding is for.
fn bind_key(name: &str) -> Option<BindKey> {
    let code = match name {
        "Key_M1" => "M1",
        "Key_M2" => "M2",
        "Key_M3" => "M3",
        "Key_M4" => "M4",
        "Key_M5" => "M5",
        "Key_Dial" => "Knob",
        "Key_DialRotateLeft" => "KnobLeft",
        "Key_DialRotateRight" => "KnobRight",
        other => keymap::code_for_hid_usage(hid_usage(other)?)?,
    };
    code.parse().ok()
}

/// A macro event: key down (variant 0) or up (1) and the pause after it.
fn macro_event(v: &Value) -> Result<p::MacroEvent, String> {
    let (index, data) = variant(v).ok_or("event not readable")?;
    let kind = match index {
        0 => p::MACRO_KEY_DOWN,
        1 => p::MACRO_KEY_UP,
        n => return Err(format!("event type {n} not known yet")),
    };
    let key = text(data, "key").unwrap_or("?");
    let usage = hid_usage(key).ok_or_else(|| format!("key {key:?} not known yet"))?;
    let delay = v.get("delay").and_then(Value::as_u64).unwrap_or(0);
    Ok(p::MacroEvent {
        kind,
        usage,
        delay_ms: u16::try_from(delay).unwrap_or(u16::MAX),
    })
}

/// `#AARRGGBB` or `#RRGGBB`.
fn color(v: Option<&Value>) -> Result<Rgb, String> {
    let s = v.and_then(Value::as_str).unwrap_or("");
    let hex = s.strip_prefix('#').unwrap_or(s);
    let hex = if hex.len() == 8 { &hex[2..] } else { hex };
    hex.parse().map_err(|_| format!("color {s:?} not readable"))
}

fn percent(data: &Value, key: &str, default: u8) -> u8 {
    data.get(key)
        .and_then(Value::as_u64)
        .map_or(default, |v| u8::try_from(v.min(100)).unwrap_or(100))
}

/// An effect (variant): static (10) and reactive (6), checked against what the
/// Windows app wrote; other effects are not known yet.
fn effect(v: &Value) -> Result<EffectSetting, String> {
    let (index, data) = variant(v).ok_or("not readable")?;
    let brightness = percent(data, "brightness", 100);
    match index {
        10 => Ok(EffectSetting {
            effect: Effect::STATIC,
            direction: Direction(0),
            brightness,
            speed: p::SPEED_DEFAULT,
            colors: Colors::Single(color(data.get("color"))?),
        }),
        6 => Ok(EffectSetting {
            effect: Effect(4),
            direction: Direction(0),
            brightness,
            speed: percent(data, "speed", p::SPEED_DEFAULT),
            colors: effect_colors(data)?,
        }),
        n => Err(format!("IO Center effect type {n} not known yet")),
    }
}

/// The colors of an effect by its color mode ("Gradient" seen so far).
fn effect_colors(data: &Value) -> Result<Colors, String> {
    let mode = text(data, "colorMode").unwrap_or("");
    let lower = mode.to_ascii_lowercase();
    if lower.contains("gradient") {
        let stops = data
            .get("gradient")
            .and_then(Value::as_array)
            .ok_or("gradient not readable")?;
        let stops = stops
            .iter()
            .take(p::MAX_GRADIENT_STOPS)
            .map(|s| {
                let at = s.get("second").and_then(Value::as_f64).unwrap_or(0.0);
                // 0 to 100 after the clamp
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let at = (at * 100.0).round().clamp(0.0, 100.0) as u8;
                Ok((color(s.get("first"))?, at))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Colors::Gradient(stops))
    } else if lower.contains("dual") {
        let dual = data.get("dualColors").ok_or("colors not readable")?;
        Ok(Colors::Dual(
            color(dual.get("first"))?,
            color(dual.get("second"))?,
        ))
    } else if lower.contains("single") {
        Ok(Colors::Single(color(data.get("singleColor"))?))
    } else {
        Err(format!("color mode {mode:?} not known yet"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A self-made profile in the format of IO Center for Windows 2026 (not an
    /// export of a user's settings).
    const PROFILE: &str = include_str!("../tests/fixtures/iocenter-profile.json");

    #[test]
    fn a_profile() {
        let imported = profile(PROFILE.as_bytes()).unwrap();
        assert_eq!(imported.source, "Profil 1");
        assert_eq!(imported.name, "Profil-1");
        let settings = config::parse(&imported.text).unwrap();
        let bindings: Vec<String> = settings
            .bindings
            .unwrap()
            .iter()
            .map(|(t, d)| match d {
                config::Desired::Action(a) => {
                    format!("{} = {}", t.name, config::action_text(a).unwrap())
                }
                other => format!("{} = {other:?}", t.name),
            })
            .collect();
        assert_eq!(
            bindings,
            // in the file's (alphabetical) order
            [
                "Fn+ArrowRight = lighting next-effect",
                "Fn+ArrowUp = lighting brighter",
                "KnobLeft = media volume-down",
                "M1 = media play-pause",
                "M2 = key F13",
                "M5 = macro 1",
            ]
        );
        let macros = settings.macros.unwrap();
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].name, "Makro 1");
        // Key_Y is HID 1c (Z on German keyboards); key down, then up
        assert_eq!(
            macros[0].events,
            [
                p::MacroEvent {
                    kind: p::MACRO_KEY_DOWN,
                    usage: 0x1c,
                    delay_ms: 0
                },
                p::MacroEvent {
                    kind: p::MACRO_KEY_UP,
                    usage: 0x1c,
                    delay_ms: 30
                },
            ]
        );
        let effect = settings.effect.unwrap();
        assert_eq!(effect.effect, Effect::STATIC);
        assert_eq!(effect.colors, Colors::Single(Rgb::new(0xe6, 0x30, 0x00)));
        assert_eq!(settings.lighting.unwrap().mode, Mode::Effect);
        // the unknown binding type and key are notes, also in the file
        assert_eq!(imported.notes.len(), 3, "{:?}", imported.notes);
        assert!(imported.notes[0].contains("Key_Mystery"));
        assert!(imported.notes[1].contains("binding type 99"));
        assert!(
            imported
                .text
                .contains("# Not imported: Fn+ArrowLeft: binding type 99")
        );
    }

    #[test]
    fn without_macros_the_keyboards_stay() {
        let mut v: Value = serde_json::from_str(PROFILE).unwrap();
        let properties = v["data"]["properties"].as_array_mut().unwrap();
        properties.retain(|p| p["polymorphic_name"] != "MacrosProperty");
        let imported = profile(v.to_string().as_bytes()).unwrap();
        let settings = config::parse(&imported.text).unwrap();
        assert_eq!(settings.macros, None);
        // M5's macro is gone: a note
        assert!(imported.notes.iter().any(|n| n.contains("M5: its macro")));
    }

    #[test]
    fn reactive_with_a_gradient() {
        let v: Value = serde_json::from_str(
            r##"{"impl": {"index": 6, "data": {"colorMode": "Gradient",
            "gradient": [{"first": "#ffff0000", "second": 0.0}, {"first": "#ff0000ff", "second": 0.9999999}],
            "speed": 70, "brightness": 80}}}"##,
        )
        .unwrap();
        let e = effect(&v).unwrap();
        assert_eq!((e.effect, e.speed, e.brightness), (Effect(4), 70, 80));
        assert_eq!(
            e.colors,
            Colors::Gradient(vec![(Rgb::new(255, 0, 0), 0), (Rgb::new(0, 0, 255), 100)])
        );
        let unknown: Value = serde_json::from_str(r#"{"impl": {"index": 3, "data": {}}}"#).unwrap();
        assert!(effect(&unknown).unwrap_err().contains("type 3"));
    }

    #[test]
    fn key_names() {
        assert_eq!(hid_usage("Key_A"), Some(0x04));
        assert_eq!(hid_usage("Key_Y"), Some(0x1c));
        assert_eq!(hid_usage("Key_0"), Some(0x27));
        assert_eq!(hid_usage("Key_9"), Some(0x26));
        assert_eq!(hid_usage("Key_F1"), Some(0x3a));
        assert_eq!(hid_usage("Key_F13"), Some(0x68));
        assert_eq!(hid_usage("Key_F25"), None);
        assert_eq!(hid_usage("Key_LeftShift"), Some(0xe1));
        assert_eq!(hid_usage("Key_Escape"), None); // not seen yet
        assert_eq!(bind_key("Key_DialRotateRight").unwrap().name, "KnobRight");
        assert_eq!(bind_key("Key_Home").unwrap().name, "Home");
    }

    #[test]
    fn names() {
        assert_eq!(profile_name("Profil 1"), "Profil-1");
        assert_eq!(profile_name("Gaming / FPS!"), "Gaming-FPS");
        assert_eq!(profile_name("Ünïcode"), "n-code");
        assert_eq!(profile_name("a very long profile name"), "a-very-long-prof");
        assert_eq!(profile_name("???"), "iocenter");
        assert_eq!(profile_name("next"), "iocenter");
    }

    /// A zip archive with the given files, stored or deflated.
    fn zip(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for (name, data, deflate) in files {
            let packed = if *deflate {
                miniz_oxide::deflate::compress_to_vec(data, 6)
            } else {
                data.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let offset = u32::try_from(out.len()).unwrap();
            let sizes = |v: &mut Vec<u8>| {
                v.extend_from_slice(&0u32.to_le_bytes()); // CRC: not checked
                v.extend_from_slice(&u32::try_from(packed.len()).unwrap().to_le_bytes());
                v.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
            };
            out.extend_from_slice(b"PK\x03\x04\x14\x00\x00\x00");
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 4]);
            sizes(&mut out);
            out.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&packed);
            central.extend_from_slice(b"PK\x01\x02\x14\x00\x14\x00\x00\x00");
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&[0; 4]);
            sizes(&mut central);
            central.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = u32::try_from(out.len()).unwrap();
        let count = u16::try_from(files.len()).unwrap();
        out.extend_from_slice(&central);
        out.extend_from_slice(b"PK\x05\x06\x00\x00\x00\x00");
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&u32::try_from(central.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn a_backup() {
        let backup = zip(&[
            ("logs/IO Center.log", b"not a profile", true),
            ("profiles/a.ioprofile", PROFILE.as_bytes(), true),
            ("profiles/b.ioprofile", PROFILE.as_bytes(), false),
        ]);
        let list = from_bytes(&backup).unwrap();
        let names: Vec<&str> = list.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Profil-1", "Profil-1-2"]);
        // no profiles, broken archives
        assert!(
            from_bytes(&zip(&[("x.txt", b"x", false)]))
                .unwrap_err()
                .contains("no profiles")
        );
        let mut cut = backup.clone();
        cut.truncate(backup.len() / 2);
        assert!(from_bytes(&cut).is_err());
        let mut damaged = backup;
        // the unpacked size of the last profile
        let at = damaged
            .windows(4)
            .rposition(|w| w == b"PK\x01\x02")
            .unwrap();
        damaged[at + 24] ^= 0x01;
        assert!(from_bytes(&damaged).is_err());
        assert!(
            from_bytes(b"{\"data\": {}}")
                .unwrap_err()
                .contains("not an IO Center profile")
        );
    }
}
