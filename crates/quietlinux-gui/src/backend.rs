//! Talking to the keyboard, on a thread of its own so that the window never waits.
//!
//! Every request opens a session like the CLI does, makes its change and reads the
//! state again. The demo mode talks to a simulated keyboard instead.

use std::path::PathBuf;

use quietlinux_core::config::{self, Desired};
use quietlinux_core::device::fake::FakeKeyboard;
use quietlinux_core::device::{self, LightingMode, Session, Transport};
use quietlinux_core::info::{self, Action, Binding, Colors, Macro};
use quietlinux_core::keymap::{self, Layout};
use quietlinux_core::macros::{self, Changes, RepeatMode};
use quietlinux_core::protocol as p;

/// What the window asks for.
#[derive(Debug, Clone)]
pub enum Request {
    /// Read the state again.
    Refresh,
    Effect {
        effect: u8,
        direction: u8,
        brightness: u8,
        speed: u8,
        colors: Colors,
    },
    LightOff,
    LightOn,
    Bind {
        key_id: u8,
        fn_layer: bool,
        desired: Desired,
    },
    /// Create a macro (`id` `None`) or change one.
    SaveMacro {
        id: Option<u16>,
        name: String,
        repeat: RepeatMode,
        events: Vec<p::MacroEvent>,
    },
    DeleteMacro(u16),
    /// Shortcuts game mode blocks.
    GameMode(u8),
    /// Save a backup file; an existing one only with `overwrite`.
    Backup {
        path: PathBuf,
        overwrite: bool,
    },
    /// What applying a file would change (nothing is written).
    CheckFile(PathBuf),
    ApplyFile(PathBuf),
}

/// What the keyboard reported.
#[derive(Debug, Clone, Default)]
pub struct State {
    /// Model and firmware, for the title line.
    pub title: String,
    pub legends: Option<Layout>,
    pub writable: bool,
    pub bindings: Vec<Binding>,
    /// `None` if the keyboard did not report its macros.
    pub macros: Option<Vec<Macro>>,
    /// Shortcuts game mode blocks.
    pub game_mode_flags: u8,
    /// Game mode on or off, if the keyboard reported it.
    pub game_mode_on: Option<bool>,
}

/// What a request did.
#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub state: State,
    /// One line for the status bar.
    pub message: String,
    /// Longer text, e.g. the changes of a file (`None`: leave the old one).
    pub details: Option<String>,
}

/// The answer to a request, or an error.
pub type Outcome = Result<Reply, String>;

pub enum Backend {
    Hardware { usb_path: Option<String> },
    Demo(Box<FakeKeyboard>),
}

impl Backend {
    /// A simulated Light Mount with a few bindings and macros, for trying the window.
    pub fn demo() -> Self {
        let mut kb = FakeKeyboard::tested();
        kb.answers.insert(p::GET_LAYOUT, vec![0x01, 0x01]); // ISO, German
        kb.game_mode_flags = 0x0f; // Shift+Tab, Alt+F4, Win, Alt+Tab
        for (key, action) in [
            (0x02, vec![0x02, 0x04]),       // M1 mic mute
            (0x03, vec![0x01, 0x00, 0x68]), // M2 F13
            (0x06, vec![0x0a, 0x01, 0x00]), // M5 macro 1
        ] {
            kb.bindings.insert((key, p::LAYER_NORMAL), action);
        }
        let demo_macros = [
            (1, "hello", "shift+KeyH KeyE KeyL KeyL KeyO"),
            (2, "select-copy", "ctrl+KeyA 50ms ctrl+KeyC"),
        ];
        for (id, name, events) in demo_macros {
            let events = macros::parse_events(&[events.to_owned()], macros::DEFAULT_DELAY_MS)
                .expect("demo macro");
            kb.macros.insert(id, Macro::new(id, name, events));
        }
        Self::Demo(Box::new(kb))
    }

    pub fn handle(&mut self, request: Request) -> Outcome {
        match self {
            Self::Hardware { usb_path } => {
                let (_, mut session) =
                    device::open(usb_path.as_deref(), false).map_err(|e| e.to_string())?;
                session.open_session().map_err(|e| e.to_string())?;
                handle(&mut session, request)
            }
            Self::Demo(kb) => {
                let mut session = Session::new(&mut **kb, false).map_err(|e| e.to_string())?;
                handle(&mut session, request)
            }
        }
    }

    /// Only whether game mode is on (`07:04`): cheap enough to ask every few seconds.
    pub fn game_mode_on(&mut self) -> Result<Option<bool>, String> {
        match self {
            Self::Hardware { usb_path } => {
                let (_, mut session) =
                    device::open(usb_path.as_deref(), false).map_err(|e| e.to_string())?;
                session.open_session().map_err(|e| e.to_string())?;
                Ok(read_game_mode(&mut session))
            }
            Self::Demo(kb) => {
                let mut session = Session::new(&mut **kb, false).map_err(|e| e.to_string())?;
                Ok(read_game_mode(&mut session))
            }
        }
    }
}

fn read_game_mode<T: Transport>(session: &mut Session<T>) -> Option<bool> {
    let reply = session.query(p::GET_GAME_MODE_STATE, &[]).ok()?;
    reply.data.first().map(|state| state & 1 == 1)
}

fn handle<T: Transport>(session: &mut Session<T>, request: Request) -> Outcome {
    let (message, details) = match request {
        Request::Backup { path, overwrite } => {
            backup(session, &path, overwrite).map_err(|e| e.to_string())?
        }
        Request::CheckFile(path) => apply_file(session, &path, false).map_err(|e| e.to_string())?,
        Request::ApplyFile(path) => apply_file(session, &path, true).map_err(|e| e.to_string())?,
        request => (change(session, request).map_err(|e| e.to_string())?, None),
    };
    let state = read_state(session).map_err(|e| e.to_string())?;
    Ok(Reply {
        state,
        message,
        details,
    })
}

type Lines = (String, Option<String>);

fn backup<T: Transport>(
    session: &mut Session<T>,
    path: &std::path::Path,
    overwrite: bool,
) -> Result<Lines, Box<dyn std::error::Error>> {
    if path.exists() && !overwrite {
        return Err(format!(
            "{} exists: tick \"Overwrite\" to replace it",
            path.display()
        )
        .into());
    }
    let snapshot = config::read_snapshot(session)?;
    std::fs::write(path, config::backup_text(&snapshot))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let macros = snapshot.macros.as_ref().map_or_else(
        || "macros not reported".to_owned(),
        |m| format!("{} macro(s)", m.len()),
    );
    Ok((
        format!(
            "Saved to {}: {} binding(s), game mode shortcuts, {macros}.",
            path.display(),
            snapshot.bindings.len()
        ),
        None,
    ))
}

/// Check a settings file against the keyboard and, with `write`, apply it.
fn apply_file<T: Transport>(
    session: &mut Session<T>,
    path: &std::path::Path,
    write: bool,
) -> Result<Lines, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let settings = config::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let current = config::read_snapshot(session)?;
    let plan = config::plan(&settings, &current);
    let mut lines: Vec<String> = plan.notes.iter().map(|n| format!("note: {n}")).collect();
    lines.extend(plan.ops.iter().map(ToString::to_string));
    let count = plan.ops.len();
    let message = if count == 0 {
        "Nothing to change.".to_owned()
    } else if write {
        config::execute(session, &plan)?;
        format!("{count} change(s) written.")
    } else {
        format!("{count} change(s); nothing written yet.")
    };
    Ok((message, Some(lines.join("\n"))))
}

fn change<T: Transport>(
    session: &mut Session<T>,
    request: Request,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(match request {
        Request::GameMode(flags) => {
            session.set_game_mode_blocks(flags)?;
            "Game mode shortcuts stored.".to_owned()
        }
        Request::Backup { .. } | Request::CheckFile(_) | Request::ApplyFile(_) => {
            unreachable!("handled above")
        }
        Request::Refresh => String::new(),
        Request::Effect {
            effect,
            direction,
            brightness,
            speed,
            colors,
        } => {
            session.set_effect(effect, direction, brightness, speed, &colors)?;
            "Effect stored in the keyboard.".to_owned()
        }
        Request::LightOff => {
            session.set_lighting_mode(LightingMode::Off)?;
            "Lighting off.".to_owned()
        }
        Request::LightOn => {
            session.show_stored_effect()?;
            "Lighting on.".to_owned()
        }
        Request::Bind {
            key_id,
            fn_layer,
            desired,
        } => {
            let name = binding_name(key_id, fn_layer);
            match desired {
                Desired::Default => {
                    session.clear_binding(key_id, fn_layer)?;
                    format!("{name}: factory binding.")
                }
                Desired::Action(action) => {
                    if let Action::Macro(id) = action
                        && !session.macro_ids()?.contains(&id)
                    {
                        return Err(format!("the keyboard has no macro {id}").into());
                    }
                    session.set_binding(key_id, fn_layer, &action)?;
                    format!("{name}: {action}.")
                }
                Desired::Raw(_) => return Err("raw bindings cannot be written".into()),
            }
        }
        Request::SaveMacro {
            id: None,
            name,
            repeat,
            events,
        } => {
            let id = macros::create(session, &name, repeat, events)?;
            format!("Macro {id} stored.")
        }
        Request::SaveMacro {
            id: Some(id),
            name,
            repeat,
            events,
        } => {
            let changes = Changes {
                name: Some(name),
                repeat: Some(repeat),
                events: Some(events),
            };
            macros::edit(session, id, changes)?;
            format!("Macro {id} changed.")
        }
        Request::DeleteMacro(id) => {
            macros::delete(session, id)?;
            format!("Macro {id} deleted.")
        }
    })
}

fn binding_name(key_id: u8, fn_layer: bool) -> String {
    let code = keymap::code_for_key_id(key_id).unwrap_or("?");
    if fn_layer {
        format!("Fn+{code}")
    } else {
        code.to_owned()
    }
}

fn read_state<T: Transport>(session: &mut Session<T>) -> Result<State, device::Error> {
    let identity = session.identity().info();
    let title = identity.map_or_else(
        || "Light Mount (unknown firmware)".to_owned(),
        |i| {
            let firmware: Vec<String> = i.firmware.iter().map(ToString::to_string).collect();
            format!(
                "{} · firmware {}",
                i.model_name().unwrap_or("Unknown model"),
                firmware.join(", ")
            )
        },
    );
    let legends = session
        .query(p::GET_LAYOUT, &[])
        .ok()
        .and_then(|m| info::Layout::parse(&m.data))
        .and_then(info::Layout::legends);
    let (_, bindings) = session.read_bindings()?;
    let macros = session.read_macros().ok();
    let game_mode_flags = session
        .query(p::GET_GAME_MODE_CONFIG, &[])
        .ok()
        .and_then(|m| m.data.first().copied())
        .unwrap_or(0);
    let game_mode_on = read_game_mode(session);
    Ok(State {
        title,
        legends,
        writable: session.writable(),
        bindings,
        macros,
        game_mode_flags,
        game_mode_on,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_keyboard_changes_and_reports() {
        let mut backend = Backend::demo();
        let state = backend.handle(Request::Refresh).unwrap().state;
        assert_eq!(state.title, "Light Mount · firmware 2.3.0");
        assert_eq!(state.legends, Some(Layout::De));
        assert_eq!(state.bindings.len(), 3);
        assert_eq!(state.macros.as_ref().map(Vec::len), Some(2));
        let reply = backend
            .handle(Request::Bind {
                key_id: 0x04,
                fn_layer: false,
                desired: Desired::Action(Action::Media(8)),
            })
            .unwrap();
        assert_eq!(reply.message, "M3: media: stop.");
        assert_eq!(reply.state.bindings.len(), 4);
        let err = backend
            .handle(Request::Bind {
                key_id: 0x04,
                fn_layer: false,
                desired: Desired::Action(Action::Macro(9)),
            })
            .unwrap_err();
        assert!(err.contains("no macro 9"), "{err}");
        let reply = backend
            .handle(Request::SaveMacro {
                id: None,
                name: "new".into(),
                repeat: RepeatMode(p::MACRO_ONCE),
                events: macros::parse_events(&["KeyA".into()], 100).unwrap(),
            })
            .unwrap();
        assert_eq!(reply.message, "Macro 3 stored.");
        assert_eq!(reply.state.macros.unwrap().len(), 3);
        // macro 1 is bound to M5
        assert!(backend.handle(Request::DeleteMacro(1)).is_err());
        assert_eq!(state.game_mode_flags, 0x0f);
        assert_eq!(state.game_mode_on, Some(false));
        assert_eq!(backend.game_mode_on(), Ok(Some(false)));
        if let Backend::Demo(kb) = &mut backend {
            kb.game_mode_on = true; // Fn+Pause
        }
        assert_eq!(backend.game_mode_on(), Ok(Some(true)));
        let reply = backend.handle(Request::GameMode(0x04)).unwrap();
        assert_eq!(reply.state.game_mode_flags, 0x04);
    }

    #[test]
    fn backup_check_and_apply_on_the_demo_keyboard() {
        let dir = std::env::temp_dir().join(format!("quietlinux-gui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("backup.toml");
        let mut backend = Backend::demo();
        let reply = backend
            .handle(Request::Backup {
                path: path.clone(),
                overwrite: false,
            })
            .unwrap();
        assert!(reply.message.contains("3 binding(s)"), "{}", reply.message);
        // no second write without overwrite
        assert!(
            backend
                .handle(Request::Backup {
                    path: path.clone(),
                    overwrite: false
                })
                .is_err()
        );
        // change M1, then check and apply the backup
        backend
            .handle(Request::Bind {
                key_id: 0x02,
                fn_layer: false,
                desired: Desired::Default,
            })
            .unwrap();
        let check = backend.handle(Request::CheckFile(path.clone())).unwrap();
        assert_eq!(check.message, "1 change(s); nothing written yet.");
        assert_eq!(check.details.as_deref(), Some("M1: media: mic mute"));
        let applied = backend.handle(Request::ApplyFile(path.clone())).unwrap();
        assert_eq!(applied.message, "1 change(s) written.");
        let again = backend.handle(Request::CheckFile(path)).unwrap();
        assert_eq!(again.message, "Nothing to change.");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
