//! Talking to the keyboard, on a thread of its own so that the window never waits.
//!
//! Every request opens a session like the CLI does, makes its change and reads the
//! state again. The demo mode talks to a simulated keyboard instead.

use quietlinux_core::config::Desired;
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
}

/// The answer to a request: the new state and a message, or an error.
pub type Outcome = Result<(State, String), String>;

pub enum Backend {
    Hardware { usb_path: Option<String> },
    Demo(Box<FakeKeyboard>),
}

impl Backend {
    /// A simulated Light Mount with a few bindings and macros, for trying the window.
    pub fn demo() -> Self {
        let mut kb = FakeKeyboard::tested();
        kb.answers.insert(p::GET_LAYOUT, vec![0x01, 0x01]); // ISO, German
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
}

fn handle<T: Transport>(session: &mut Session<T>, request: Request) -> Outcome {
    let message = change(session, request).map_err(|e| e.to_string())?;
    let state = read_state(session).map_err(|e| e.to_string())?;
    Ok((state, message))
}

fn change<T: Transport>(
    session: &mut Session<T>,
    request: Request,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(match request {
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
    Ok(State {
        title,
        legends,
        writable: session.writable(),
        bindings,
        macros,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_keyboard_changes_and_reports() {
        let mut backend = Backend::demo();
        let (state, _) = backend.handle(Request::Refresh).unwrap();
        assert_eq!(state.title, "Light Mount · firmware 2.3.0");
        assert_eq!(state.legends, Some(Layout::De));
        assert_eq!(state.bindings.len(), 3);
        assert_eq!(state.macros.as_ref().map(Vec::len), Some(2));
        let (state, message) = backend
            .handle(Request::Bind {
                key_id: 0x04,
                fn_layer: false,
                desired: Desired::Action(Action::Media(8)),
            })
            .unwrap();
        assert_eq!(message, "M3: media: stop.");
        assert_eq!(state.bindings.len(), 4);
        let err = backend
            .handle(Request::Bind {
                key_id: 0x04,
                fn_layer: false,
                desired: Desired::Action(Action::Macro(9)),
            })
            .unwrap_err();
        assert!(err.contains("no macro 9"), "{err}");
        let (state, message) = backend
            .handle(Request::SaveMacro {
                id: None,
                name: "new".into(),
                repeat: RepeatMode(p::MACRO_ONCE),
                events: macros::parse_events(&["KeyA".into()], 100).unwrap(),
            })
            .unwrap();
        assert_eq!(message, "Macro 3 stored.");
        assert_eq!(state.macros.unwrap().len(), 3);
        // macro 1 is bound to M5
        assert!(backend.handle(Request::DeleteMacro(1)).is_err());
    }
}
