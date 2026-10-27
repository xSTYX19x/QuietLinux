//! `quietlinux macro`: macros stored in the keyboard, and their text form.
//!
//! Events are written as tokens: `KeyA` taps a key (press, release), `ctrl+KeyC`
//! taps a combination, `+shift` / `-shift` press / release a key, `250ms` sets
//! the pause after the previous event. Without a pause token every event is
//! followed by the default delay (IO Center records 100 ms), the last one by none.

use std::fmt::Write as _;
use std::str::FromStr;
use std::time::Duration;

use crate::device::{Error, Session, Transport};
use crate::info::{self, Action, Macro};
use crate::keymap;
use crate::protocol::{self as p, MacroEvent};

use crate::settings::KeyCombo;

/// IO Center records macros with this pause after each event.
pub const DEFAULT_DELAY_MS: u16 = 100;
/// Longest name accepted (IO Center's limit is not known; its names were short).
const MAX_NAME: usize = 32;

const MODIFIERS: [&str; 8] = [
    "ctrl", "shift", "alt", "win", "rctrl", "rshift", "altgr", "rwin",
];
/// HID usage of the first modifier (left Ctrl); the others follow in bit order.
const FIRST_MODIFIER: u8 = 0xe0;

/// Repeat mode of a macro, with IO Center's options as names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepeatMode(pub u8);

const REPEAT_MODES: [(&str, u8); 3] = [
    ("once", p::MACRO_ONCE),
    ("while-held", p::MACRO_WHILE_HELD),
    ("repeat", p::MACRO_REPEAT),
];

impl FromStr for RepeatMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        REPEAT_MODES
            .iter()
            .find(|(name, _)| *name == s)
            .map(|&(_, v)| Self(v))
            .ok_or_else(|| format!("unknown repeat mode {s:?} (known: once, while-held, repeat)"))
    }
}

impl RepeatMode {
    /// The name `--repeat` takes, `None` for modes IO Center does not offer.
    pub fn name(self) -> Option<&'static str> {
        REPEAT_MODES
            .iter()
            .find(|(_, v)| *v == self.0)
            .map(|(name, _)| *name)
    }
}

/// HID usage of a key name or modifier name in a macro.
fn usage_of(name: &str) -> Result<u8, String> {
    if let Some(i) = MODIFIERS
        .iter()
        .position(|m| *m == name.to_ascii_lowercase())
    {
        return Ok(FIRST_MODIFIER + u8::try_from(i).expect("8 modifiers"));
    }
    if name.eq_ignore_ascii_case("ralt") {
        return Ok(FIRST_MODIFIER + 6);
    }
    if let Some(bit) = keymap::modifier_bit(name) {
        return Ok(FIRST_MODIFIER + u8::try_from(bit.trailing_zeros()).expect("bit < 8"));
    }
    keymap::hid_usage(name).ok_or_else(|| {
        format!(
            "{name:?} is neither a modifier (ctrl, shift, alt, win, rctrl, rshift, altgr, rwin) \
             nor a key name (KeyA, Digit1, F13, Enter, … see `quietlinux keys`)"
        )
    })
}

/// Name of a HID usage in a macro, `None` if it has none.
fn usage_name(usage: u8) -> Option<&'static str> {
    match usage.checked_sub(FIRST_MODIFIER) {
        Some(i @ 0..=7) => Some(MODIFIERS[usize::from(i)]),
        _ => keymap::code_for_hid_usage(usage),
    }
}

fn event(kind: u8, usage: u8, delay_ms: u16) -> MacroEvent {
    MacroEvent {
        kind,
        usage,
        delay_ms,
    }
}

/// Parse macro tokens (several per argument, separated by spaces or commas).
pub fn parse_events(args: &[String], delay_ms: u16) -> Result<Vec<MacroEvent>, String> {
    let mut events: Vec<MacroEvent> = Vec::new();
    // events whose pause was given explicitly
    let mut explicit = 0;
    for token in args
        .iter()
        .flat_map(|a| a.split([' ', ',']))
        .filter(|t| !t.is_empty())
    {
        if token.starts_with("--") {
            return Err(format!(
                "{token:?}: options go before the events (e.g. macro create NAME --repeat once KeyA)"
            ));
        }
        if let Some(ms) = token.strip_suffix("ms") {
            let ms: u16 = ms
                .parse()
                .map_err(|_| format!("invalid pause {token:?} (0ms to 65535ms)"))?;
            let last = events
                .last_mut()
                .ok_or_else(|| format!("{token:?}: a pause must follow an event"))?;
            last.delay_ms = ms;
            explicit = events.len();
        } else if let Some(key) = token.strip_prefix('+') {
            events.push(event(p::MACRO_KEY_DOWN, usage_of(key)?, delay_ms));
        } else if let Some(key) = token.strip_prefix('-').filter(|k| !k.is_empty()) {
            events.push(event(p::MACRO_KEY_UP, usage_of(key)?, delay_ms));
        } else {
            let combo: KeyCombo = token.parse()?;
            let modifiers: Vec<u8> = (0..8u8)
                .filter(|bit| combo.modifiers & (1 << bit) != 0)
                .map(|bit| FIRST_MODIFIER + bit)
                .collect();
            for &m in &modifiers {
                events.push(event(p::MACRO_KEY_DOWN, m, delay_ms));
            }
            if combo.usage != 0 {
                events.push(event(p::MACRO_KEY_DOWN, combo.usage, delay_ms));
                events.push(event(p::MACRO_KEY_UP, combo.usage, delay_ms));
            }
            for &m in modifiers.iter().rev() {
                events.push(event(p::MACRO_KEY_UP, m, delay_ms));
            }
        }
    }
    // like IO Center: no pause after the last event, unless one was given
    if explicit < events.len() {
        if let Some(last) = events.last_mut() {
            last.delay_ms = 0;
        }
    }
    if events.is_empty() {
        return Err("a macro needs at least one event".to_owned());
    }
    Ok(events)
}

/// Events as tokens, so that `parse_events(text, delay_ms)` gives them back.
/// Events without a name (other kinds or unnamed keys) are written as
/// `kind:usage` in hex; those cannot be parsed back.
pub fn events_text(events: &[MacroEvent], delay_ms: u16) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < events.len() {
        let e = events[i];
        let name = usage_name(e.usage);
        let tap = events.get(i + 1).is_some_and(|next| {
            e.kind == p::MACRO_KEY_DOWN
                && next.kind == p::MACRO_KEY_UP
                && next.usage == e.usage
                && e.delay_ms == delay_ms
        });
        let token = match (e.kind, name) {
            (p::MACRO_KEY_DOWN, Some(n)) if tap => n.to_owned(),
            (p::MACRO_KEY_DOWN, Some(n)) => format!("+{n}"),
            (p::MACRO_KEY_UP, Some(n)) => format!("-{n}"),
            _ => format!("{:02x}:{:02x}", e.kind, e.usage),
        };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&token);
        if tap {
            i += 1;
        }
        let shown = events[i];
        let last = i + 1 == events.len();
        let expected = if last { 0 } else { delay_ms };
        if shown.delay_ms != expected {
            let _ = write!(out, " {}ms", shown.delay_ms);
        }
        i += 1;
    }
    out
}

// --- recording ----------------------------------------------------------------------

/// The pause "none" stands for: IO Center's shortest.
pub const NO_PAUSE_MS: u16 = 50;

/// An input event the keyboard reported while recording (`12:05`), and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recorded {
    /// `02` key down, `03` key up, `01` modifiers changed (data: bit mask).
    pub kind: u8,
    pub data: u8,
    /// Time since the recording started.
    pub at: Duration,
}

/// The pauses between recorded events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pause {
    /// The same pause after every event, in ms ("no pause" is [`NO_PAUSE_MS`]).
    Fixed(u16),
    /// The pauses as they were typed.
    Recorded,
}

/// Record with the keyboard's own recording until `keep_going` (called with the
/// number of events so far, at least every 50 ms) returns `false`. The session
/// is kept open with keep-alives meanwhile.
pub fn record<T: Transport>(
    session: &mut Session<T>,
    mut keep_going: impl FnMut(usize) -> bool,
) -> Result<Vec<Recorded>, Error> {
    const KEEP_ALIVE: Duration = Duration::from_millis(1500);
    session.start_recording()?;
    let start = std::time::Instant::now();
    let mut last_alive = start;
    let mut recorded = Vec::new();
    while keep_going(recorded.len()) {
        if let Some((kind, data)) = session.next_input_event(Duration::from_millis(50))? {
            recorded.push(Recorded {
                kind,
                data,
                at: start.elapsed(),
            });
        }
        if last_alive.elapsed() >= KEEP_ALIVE {
            session.keep_alive()?;
            last_alive = std::time::Instant::now();
        }
    }
    Ok(recorded)
}

/// Leave out the Enter that stopped a recording (pressed last, maybe released).
pub fn drop_stopping_enter(recorded: &mut Vec<Recorded>) {
    const ENTER: [u8; 2] = [0x28, 0x58]; // Enter, numpad Enter
    for kind in [p::MACRO_KEY_UP, p::MACRO_KEY_DOWN] {
        if recorded
            .last()
            .is_some_and(|r| r.kind == kind && ENTER.contains(&r.data))
        {
            recorded.pop();
        }
    }
}

/// How a recording's pauses are given on the command line: `none`, `recorded`
/// or a number of ms.
impl FromStr for Pause {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Pause::Fixed(NO_PAUSE_MS)),
            "recorded" => Ok(Pause::Recorded),
            ms => ms
                .strip_suffix("ms")
                .unwrap_or(ms)
                .parse()
                .map(Pause::Fixed)
                .map_err(|_| format!("invalid pause {s:?}: none, recorded or a number of ms")),
        }
    }
}

/// Turn a recording into macro events:
/// - modifier masks (`01`) become presses and releases of the modifier keys;
/// - releases of keys pressed before the recording started are left out (the
///   Enter that started it), and keys still held at the end are released;
/// - every event gets the pause `pause`, the last one none.
pub fn from_recording(recorded: &[Recorded], pause: Pause) -> Vec<MacroEvent> {
    let mut events: Vec<(MacroEvent, Duration)> = Vec::new();
    let mut held: Vec<u8> = Vec::new();
    let mut modifiers = 0u8;
    let mut push = |kind: u8, usage: u8, at: Duration, held: &mut Vec<u8>| {
        match kind {
            p::MACRO_KEY_DOWN if !held.contains(&usage) => held.push(usage),
            p::MACRO_KEY_UP => {
                let Some(i) = held.iter().position(|u| *u == usage) else {
                    return; // pressed before the recording
                };
                held.remove(i);
            }
            _ => return, // repeated press while held
        }
        events.push((event(kind, usage, 0), at));
    };
    for r in recorded {
        match r.kind {
            p::MACRO_KEY_DOWN | p::MACRO_KEY_UP => push(r.kind, r.data, r.at, &mut held),
            p::INPUT_MODIFIERS => {
                let changed = modifiers ^ r.data;
                for bit in 0..8u8 {
                    if changed & (1 << bit) != 0 {
                        let kind = if r.data & (1 << bit) != 0 {
                            p::MACRO_KEY_DOWN
                        } else {
                            p::MACRO_KEY_UP
                        };
                        push(kind, FIRST_MODIFIER + bit, r.at, &mut held);
                    }
                }
                modifiers = r.data;
            }
            // other kinds (consumer keys, mouse) cannot be macro events yet
            _ => {}
        }
    }
    let end = recorded.last().map_or(Duration::ZERO, |r| r.at);
    for usage in held.into_iter().rev() {
        events.push((event(p::MACRO_KEY_UP, usage, 0), end));
    }
    let times: Vec<Duration> = events.iter().map(|(_, at)| *at).collect();
    let mut out: Vec<MacroEvent> = events.into_iter().map(|(e, _)| e).collect();
    let count = out.len();
    for (i, e) in out.iter_mut().enumerate() {
        e.delay_ms = match pause {
            _ if i + 1 == count => 0,
            Pause::Fixed(ms) => ms,
            Pause::Recorded => {
                let gap = times[i + 1].saturating_sub(times[i]).as_millis();
                u16::try_from(gap).unwrap_or(u16::MAX)
            }
        };
    }
    out
}

/// Events as a backup file stores them: tokens if they read back the same, else
/// the raw bytes as hex (`raw` is true then).
pub fn stored_events(events: &[MacroEvent]) -> (bool, String) {
    let text = events_text(events, DEFAULT_DELAY_MS);
    if parse_events(std::slice::from_ref(&text), DEFAULT_DELAY_MS).as_deref() == Ok(events) {
        return (false, text);
    }
    let hex: Vec<String> = events
        .iter()
        .map(|e| {
            let [lo, hi] = e.delay_ms.to_le_bytes();
            format!("{:02x} {:02x} {lo:02x} {hi:02x}", e.kind, e.usage)
        })
        .collect();
    (true, hex.join("  "))
}

/// Events from the raw hex of [`stored_events`]: 4 bytes per event.
pub fn parse_raw_events(hex: &str) -> Result<Vec<MacroEvent>, String> {
    let bytes = hex
        .split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).map_err(|_| format!("invalid byte {b:?}")))
        .collect::<Result<Vec<u8>, String>>()?;
    if bytes.len() % 4 != 0 {
        return Err("raw events: 4 bytes per event".to_owned());
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|e| event(e[0], e[1], u16::from_le_bytes([e[2], e[3]])))
        .collect())
}

fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > MAX_NAME
        || !name.chars().all(|c| c.is_ascii_graphic() || c == ' ')
    {
        return Err(format!(
            "invalid macro name {name:?}: 1 to {MAX_NAME} ASCII letters, digits, signs or spaces"
        ));
    }
    Ok(())
}

/// Write all macros, then read them back: the keyboard must report what was written.
pub fn write_verified<T: Transport>(
    session: &mut Session<T>,
    macros: &[Macro],
) -> Result<(), Box<dyn std::error::Error>> {
    session.write_macros(macros)?;
    let stored = session.read_macros()?;
    if stored != macros {
        return Err("the keyboard reports other macros than were written; \
                    check them with `quietlinux macro list` and please report this"
            .into());
    }
    Ok(())
}

fn repeat_text(mode: u8) -> String {
    info::repeat_name(mode).map_or_else(|| format!("mode {mode:#04x}"), str::to_owned)
}

/// One line per macro: ID, name, repeat mode, number of events.
pub fn list<T: Transport>(session: &mut Session<T>) -> Result<Vec<String>, Error> {
    let macros = session.read_macros()?;
    if macros.is_empty() {
        return Ok(vec!["No macros stored.".to_owned()]);
    }
    Ok(macros
        .iter()
        .map(|m| {
            format!(
                "{:>3}  {:<20} {:<10} {} event(s)",
                m.id,
                format!("{:?}", m.name),
                repeat_text(m.repeat()),
                m.events.len()
            )
        })
        .collect())
}

pub fn show<T: Transport>(
    session: &mut Session<T>,
    id: u16,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let ids = session.macro_ids()?;
    if !ids.contains(&id) {
        return Err(format!("the keyboard has no macro {id} (macros: {ids:?})").into());
    }
    let m = session.read_macro(id)?;
    Ok(vec![
        format!("Macro {}: {:?}, {}", m.id, m.name, repeat_text(m.repeat())),
        format!("Events: {}", events_text(&m.events, DEFAULT_DELAY_MS)),
    ])
}

/// What to change in a macro.
#[derive(Debug, Default)]
pub struct Changes {
    pub name: Option<String>,
    pub repeat: Option<RepeatMode>,
    pub events: Option<Vec<MacroEvent>>,
}

/// Store a new macro; returns its ID (the smallest free one, like IO Center).
pub fn create<T: Transport>(
    session: &mut Session<T>,
    name: &str,
    repeat: RepeatMode,
    events: Vec<MacroEvent>,
) -> Result<u16, Box<dyn std::error::Error>> {
    check_name(name)?;
    // all macros are written again: read them completely before writing anything
    let mut macros = session.read_macros()?;
    let id = (1..=u16::MAX)
        .find(|id| macros.iter().all(|m| m.id != *id))
        .ok_or("no free macro ID")?;
    let mut new = Macro::new(id, name, events);
    new.set_repeat(repeat.0);
    macros.push(new);
    write_verified(session, &macros)?;
    Ok(id)
}

pub fn edit<T: Transport>(
    session: &mut Session<T>,
    id: u16,
    changes: Changes,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(name) = &changes.name {
        check_name(name)?;
    }
    let mut macros = session.read_macros()?;
    let ids: Vec<u16> = macros.iter().map(|m| m.id).collect();
    let m = macros
        .iter_mut()
        .find(|m| m.id == id)
        .ok_or_else(|| format!("the keyboard has no macro {id} (macros: {ids:?})"))?;
    if let Some(name) = changes.name {
        m.name = name;
    }
    if let Some(repeat) = changes.repeat {
        m.set_repeat(repeat.0);
    }
    if let Some(events) = changes.events {
        m.events = events;
    }
    write_verified(session, &macros)?;
    Ok(())
}

/// Delete a macro. Refused while a key is bound to it.
pub fn delete<T: Transport>(
    session: &mut Session<T>,
    id: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut macros = session.read_macros()?;
    let ids: Vec<u16> = macros.iter().map(|m| m.id).collect();
    if !ids.contains(&id) {
        return Err(format!("the keyboard has no macro {id} (macros: {ids:?})").into());
    }
    let (total, bindings) = session.read_bindings()?;
    if bindings.len() != total {
        return Err("could not read all key bindings: not deleting".into());
    }
    let bound: Vec<String> = bindings
        .iter()
        .filter(|b| b.action == Action::Macro(id))
        .map(|b| format!("{}{}", if b.fn_layer { "--fn " } else { "" }, b.key_name()))
        .collect();
    if !bound.is_empty() {
        return Err(format!(
            "macro {id} is bound to {}: bind those keys to something else first \
             (e.g. `quietlinux bind {} default`)",
            bound.join(", "),
            bound[0]
        )
        .into());
    }
    macros.retain(|m| m.id != id);
    write_verified(session, &macros)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::device::fake::FakeKeyboard;

    use super::*;

    fn args(text: &str) -> Vec<String> {
        vec![text.to_owned()]
    }

    fn steps(events: &[MacroEvent]) -> Vec<(u8, u8, u16)> {
        events
            .iter()
            .map(|e| (e.kind, e.usage, e.delay_ms))
            .collect()
    }

    const DOWN: u8 = p::MACRO_KEY_DOWN;
    const UP: u8 = p::MACRO_KEY_UP;

    #[test]
    fn taps_like_io_center_records_them() {
        // the captured test macro: A, B, C with 100 ms after each event but the last
        let events = parse_events(&args("KeyA KeyB KeyC"), 100).unwrap();
        assert_eq!(
            steps(&events),
            [
                (DOWN, 0x04, 100),
                (UP, 0x04, 100),
                (DOWN, 0x05, 100),
                (UP, 0x05, 100),
                (DOWN, 0x06, 100),
                (UP, 0x06, 0)
            ]
        );
        assert_eq!(events_text(&events, 100), "KeyA KeyB KeyC");
    }

    #[test]
    fn combos_presses_and_pauses() {
        let events = parse_events(&args("ctrl+KeyC 500ms,+shift KeyA -shift 20ms"), 30).unwrap();
        assert_eq!(
            steps(&events),
            [
                (DOWN, 0xe0, 30),
                (DOWN, 0x06, 30),
                (UP, 0x06, 30),
                (UP, 0xe0, 500),
                (DOWN, 0xe1, 30),
                (DOWN, 0x04, 30),
                (UP, 0x04, 30),
                (UP, 0xe1, 20)
            ]
        );
        let text = events_text(&events, 30);
        assert_eq!(text, "+ctrl KeyC -ctrl 500ms +shift KeyA -shift 20ms");
        assert_eq!(parse_events(&args(&text), 30).unwrap(), events);
    }

    #[test]
    fn text_round_trips() {
        let odd = [
            event(DOWN, 0x04, 7),
            event(UP, 0x04, 100),
            event(DOWN, 0x28, 100),
            event(UP, 0x28, 0),
            event(UP, 0x05, 0),
            event(DOWN, 0xe6, 100),
        ];
        let text = events_text(&odd, 100);
        assert_eq!(text, "+KeyA 7ms -KeyA Enter 0ms -KeyB 0ms +altgr 100ms");
        assert_eq!(parse_events(&args(&text), 100).unwrap(), odd);
        assert_eq!(events_text(&[event(6, 0xe9, 0)], 100), "06:e9");
    }

    #[test]
    fn parse_errors() {
        for (text, expected) in [
            ("", "at least one"),
            ("100ms KeyA", "must follow"),
            ("KeyA 70000ms", "invalid pause"),
            ("+Fn", "neither"),
            ("KeyAA", "neither"),
            ("KeyA --repeat once", "options go before"),
        ] {
            let err = parse_events(&args(text), 100).unwrap_err();
            assert!(err.contains(expected), "{text:?}: {err}");
        }
        assert!(check_name("test").is_ok());
        assert!(check_name("").is_err());
        assert!(check_name("Grüße").is_err());
        assert!(check_name(&"x".repeat(33)).is_err());
    }

    #[test]
    fn create_edit_delete_on_a_simulated_keyboard() {
        let mut kb = FakeKeyboard::tested();
        kb.macros
            .insert(1, Macro::new(1, "Makro 1", vec![event(DOWN, 4, 0)]));
        kb.answers.insert(p::GET_BINDINGS, vec![0, 0]);
        let mut session = Session::new(&mut kb, false).unwrap();
        let events = parse_events(&args("KeyA KeyB"), 100).unwrap();
        let id = create(&mut session, "test", RepeatMode(p::MACRO_REPEAT), events).unwrap();
        assert_eq!(id, 2);
        assert_eq!(
            list(&mut session).unwrap(),
            [
                "  1  \"Makro 1\"            once       1 event(s)",
                "  2  \"test\"               repeat     4 event(s)"
            ]
        );
        let changes = Changes {
            name: Some("abc".into()),
            repeat: Some(RepeatMode(p::MACRO_ONCE)),
            events: Some(parse_events(&args("KeyC"), 100).unwrap()),
        };
        edit(&mut session, 2, changes).unwrap();
        assert_eq!(
            show(&mut session, 2).unwrap(),
            ["Macro 2: \"abc\", once", "Events: KeyC"]
        );
        assert!(edit(&mut session, 9, Changes::default()).is_err());
        delete(&mut session, 2).unwrap();
        assert_eq!(session.macro_ids().unwrap(), [1]);
        assert!(show(&mut session, 2).is_err());
        // macro 1 was written back unchanged every time
        assert_eq!(
            kb.macros[&1],
            Macro::new(1, "Makro 1", vec![event(DOWN, 4, 0)])
        );
    }

    fn rec(kind: u8, data: u8, ms: u64) -> Recorded {
        Recorded {
            kind,
            data,
            at: Duration::from_millis(ms),
        }
    }

    #[test]
    fn recordings_become_macros() {
        let typed = [
            rec(UP, 0x28, 5), // the Enter that started it
            rec(DOWN, 0x04, 100),
            rec(UP, 0x04, 180),
            rec(p::INPUT_MODIFIERS, 0x02, 400), // left Shift down
            rec(DOWN, 0x05, 420),
            rec(DOWN, 0x05, 450), // repeated while held
            rec(UP, 0x05, 500),
            rec(p::INPUT_MODIFIERS, 0x00, 520), // Shift up
            rec(DOWN, 0x06, 900),               // still held at the end
            rec(0x06, 0xe9, 950),               // a consumer key: left out
        ];
        let steps = |pause| steps(&from_recording(&typed, pause));
        assert_eq!(
            steps(Pause::Recorded),
            [
                (DOWN, 0x04, 80),
                (UP, 0x04, 220),
                (DOWN, 0xe1, 20),
                (DOWN, 0x05, 80),
                (UP, 0x05, 20),
                (UP, 0xe1, 380),
                (DOWN, 0x06, 50),
                (UP, 0x06, 0)
            ]
        );
        let fixed = steps(Pause::Fixed(NO_PAUSE_MS));
        assert!(fixed[..7].iter().all(|s| s.2 == 50) && fixed[7].2 == 0);
        assert_eq!(
            events_text(&from_recording(&typed, Pause::Fixed(100)), 100),
            "KeyA +shift KeyB -shift KeyC"
        );
        assert_eq!(from_recording(&[], Pause::Recorded), []);
        // the Enter that stopped it
        let mut stopped = vec![
            rec(DOWN, 0x04, 1),
            rec(UP, 0x04, 2),
            rec(DOWN, 0x28, 3),
            rec(UP, 0x28, 4),
        ];
        drop_stopping_enter(&mut stopped);
        assert_eq!(stopped.len(), 2);
        assert_eq!("none".parse(), Ok(Pause::Fixed(NO_PAUSE_MS)));
        assert_eq!("recorded".parse(), Ok(Pause::Recorded));
        assert_eq!("120ms".parse(), Ok(Pause::Fixed(120)));
        assert!("often".parse::<Pause>().is_err());
    }

    #[test]
    fn recording_on_a_simulated_keyboard() {
        let mut kb = FakeKeyboard::tested();
        kb.recording = vec![(DOWN, 0x0b), (UP, 0x0b), (DOWN, 0x0c), (UP, 0x0c)];
        let mut session = Session::new(&mut kb, false).unwrap();
        // stop once all four arrived
        let recorded = record(&mut session, |n| n < 4).unwrap();
        let got: Vec<(u8, u8)> = recorded.iter().map(|r| (r.kind, r.data)).collect();
        assert_eq!(got, [(DOWN, 0x0b), (UP, 0x0b), (DOWN, 0x0c), (UP, 0x0c)]);
        assert!(kb.commands().contains(&"12:09".to_owned()));
    }

    #[test]
    fn a_bound_macro_is_not_deleted() {
        let mut kb = FakeKeyboard::tested();
        kb.macros
            .insert(2, Macro::new(2, "x", vec![event(DOWN, 4, 0)]));
        // GetBindings: M5 bound to macro 2
        kb.answers
            .insert(p::GET_BINDINGS, vec![1, 0, 0x06, 0x00, 0x0a, 0x02, 0x00]);
        let mut session = Session::new(&mut kb, false).unwrap();
        let err = delete(&mut session, 2).unwrap_err().to_string();
        assert!(err.contains("bound to M5"), "{err}");
        assert!(kb.macros.contains_key(&2));
    }
}
