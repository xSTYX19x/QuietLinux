//! `quietlinux-gui`: the graphical interface of QuietLinux.
//!
//! The window (ui/app.slint) shows the state; [`Controller`] turns the user's
//! actions into [`Request`]s for the backend thread and puts its answers back
//! into the window.

mod backend;
mod keyboard;
mod prefs;
mod screenshot;
mod wheel;

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc;

use clap::Parser;
use quietlinux_core::Rgb;
use quietlinux_core::config;
use quietlinux_core::info::Colors;
use quietlinux_core::keymap::{self, Layout};
use quietlinux_core::macros::{self, RepeatMode};
use quietlinux_core::protocol as p;
use quietlinux_core::settings::{
    self, Direction, Effect, LightingAction, MediaAction, MouseButton,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, StandardListViewItem, VecModel};

use crate::backend::{Backend, Outcome, Request, State};
use crate::ui::WheelPick;

/// The types generated from ui/app.slint.
mod ui {
    #![allow(missing_debug_implementations)]
    slint::include_modules!();
}

pub use ui::{AppWindow, KeyCap};

#[derive(Debug, Parser)]
#[command(
    name = "quietlinux-gui",
    version,
    about = "Graphical interface for the be quiet! Light Mount keyboard"
)]
struct Cli {
    /// USB path of the keyboard to use when several are plugged in (see `quietlinux status`)
    #[arg(long, value_name = "PATH")]
    usb_path: Option<String>,
    /// Use a simulated keyboard instead of the real one
    #[arg(long)]
    demo: bool,
    /// Draw the window of the demo mode into an image (PPM) and quit; for the docs
    #[arg(long, value_name = "FILE", hide = true)]
    screenshot: Option<PathBuf>,
    /// Tab for --screenshot: 0 lighting, 1 keys, 2 macros
    #[arg(long, default_value_t = 0, hide = true)]
    page: i32,
    /// Theme for --screenshot: 0 system, 1 light, 2 dark
    #[arg(long, default_value_t = 1, hide = true)]
    theme: i32,
}

/// Binding kinds of the key editor: the word `bind` takes, and the label.
const BIND_KINDS: [(&str, &str); 7] = [
    ("default", "Factory binding"),
    ("key", "Key or combination"),
    ("media", "Media"),
    ("mouse", "Mouse button"),
    ("lighting", "Lighting"),
    ("macro", "Macro"),
    ("disable", "Disabled"),
];

/// The default backup file.
const DEFAULT_FILE: &str = "~/lightmount.toml";

/// What the request on its way is, for its answer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Pending {
    #[default]
    Other,
    Refresh,
    /// Checking the file as typed in the window.
    Check(String),
    ApplyFile,
}

/// The window's state on the UI thread.
struct Controller {
    ui: slint::Weak<AppWindow>,
    requests: Option<mpsc::Sender<Request>>,
    state: State,
    pending: Pending,
    /// Key and target the key editor shows (to tell a target change from a kind change).
    editor_key: String,
    editor_target: i32,
}

impl Controller {
    fn new(ui: &AppWindow, requests: Option<mpsc::Sender<Request>>) -> Self {
        Self {
            ui: ui.as_weak(),
            requests,
            state: State::default(),
            pending: Pending::Other,
            editor_key: String::new(),
            editor_target: 0,
        }
    }
}

/// What a key's bindings can be set for: a label, the key ID, the Fn layer.
fn targets(code: &str) -> Vec<(&'static str, u8, bool)> {
    if code == "Knob" {
        return [
            ("Press", "Knob"),
            ("Turn left", "KnobLeft"),
            ("Turn right", "KnobRight"),
        ]
        .into_iter()
        .filter_map(|(label, code)| Some((label, keymap::key_id_for_code(code)?, false)))
        .collect();
    }
    keymap::key_id_for_code(code)
        .map(|id| vec![("Normal", id, false), ("With Fn", id, true)])
        .unwrap_or_default()
}

thread_local! {
    static CONTROLLER: RefCell<Option<Controller>> = const { RefCell::new(None) };
}

fn with_controller(f: impl FnOnce(&mut Controller, &AppWindow)) {
    CONTROLLER.with(|c| {
        // a callback the window fires while the controller changes it: nothing to do
        let Ok(mut guard) = c.try_borrow_mut() else {
            return;
        };
        if let Some(c) = guard.as_mut()
            && let Some(ui) = c.ui.upgrade()
        {
            f(c, &ui);
        }
    });
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.into_iter().map(SharedString::from).collect();
    ModelRc::new(VecModel::from(items))
}

impl Controller {
    fn send(&mut self, ui: &AppWindow, request: Request) {
        let Some(requests) = &self.requests else {
            return;
        };
        self.pending = match &request {
            Request::Refresh => Pending::Refresh,
            Request::CheckFile(_) => Pending::Check(ui.get_file().to_string()),
            Request::ApplyFile(_) => Pending::ApplyFile,
            _ => Pending::Other,
        };
        ui.set_busy(true);
        if requests.send(request).is_err() {
            ui.set_busy(false);
            show_error(ui, "the backend thread stopped");
        }
    }

    /// Put an answer of the backend into the window.
    fn apply(&mut self, ui: &AppWindow, outcome: Outcome) {
        ui.set_busy(false);
        let pending = std::mem::take(&mut self.pending);
        match outcome {
            Ok(reply) => {
                let state = reply.state;
                ui.set_connected(true);
                let title = if state.writable {
                    state.title.clone()
                } else {
                    format!("{} (read-only: firmware not tested)", state.title)
                };
                ui.set_device_title(title.into());
                ui.set_status_error(false);
                ui.set_status(reply.message.into());
                if let Some(details) = reply.details {
                    ui.set_file_details(details.into());
                }
                match pending {
                    Pending::Check(file) => ui.set_checked_file(file.into()),
                    // check again before the next apply
                    Pending::ApplyFile => ui.set_checked_file("".into()),
                    Pending::Refresh | Pending::Other => {}
                }
                let flags = state.game_mode_flags;
                ui.set_block_shift_tab(flags & 0x01 != 0);
                ui.set_block_alt_f4(flags & 0x02 != 0);
                ui.set_block_win(flags & 0x04 != 0);
                ui.set_block_alt_tab(flags & 0x08 != 0);
                ui.set_block_caps_lock(flags & 0x10 != 0);
                self.state = state;
                self.show_keys(ui);
                self.show_macros(ui);
                let selected = ui.get_selected_key();
                if !selected.is_empty() {
                    self.select_key(ui, &selected);
                }
            }
            Err(e) => {
                if pending == Pending::Refresh {
                    ui.set_connected(false);
                    ui.set_device_title("No Light Mount connected".into());
                }
                show_error(ui, &e);
            }
        }
    }

    fn legends(&self) -> Layout {
        self.state.legends.unwrap_or(Layout::Uk)
    }

    fn binding(&self, key_id: u8, fn_layer: bool) -> Option<&quietlinux_core::info::Binding> {
        self.state
            .bindings
            .iter()
            .find(|b| b.key_id == key_id && b.fn_layer == fn_layer)
    }

    fn show_keys(&self, ui: &AppWindow) {
        let legends = self.legends();
        let caps: Vec<KeyCap> = keyboard::caps()
            .into_iter()
            .map(|c| {
                let changed = targets(c.code)
                    .iter()
                    .any(|&(_, id, fn_layer)| self.binding(id, fn_layer).is_some());
                KeyCap {
                    code: c.code.into(),
                    // the knob is drawn as a circle, without text
                    legend: if c.round { "" } else { legends.legend(c.code) }.into(),
                    x: c.x,
                    y: c.y,
                    w: c.w,
                    h: c.h,
                    round: c.round,
                    changed,
                }
            })
            .collect();
        ui.set_keys(ModelRc::new(VecModel::from(caps)));
    }

    fn binding_text(&self, key_id: u8, fn_layer: bool) -> String {
        self.binding(key_id, fn_layer)
            .map_or_else(|| "factory binding".to_owned(), |b| b.action.to_string())
    }

    fn select_key(&mut self, ui: &AppWindow, code: &str) {
        ui.set_selected_key(code.into());
        let legend = keymap::by_code(code).map(|k| self.legends().legend(k.code));
        let title = match legend {
            Some(legend) if legend != code => format!("{code}  [{legend}]"),
            _ => code.to_owned(),
        };
        ui.set_key_title(title.into());
        let targets = targets(code);
        let summary: Vec<String> = targets
            .iter()
            .map(|&(label, id, fn_layer)| format!("{label}: {}", self.binding_text(id, fn_layer)))
            .collect();
        ui.set_binding_summary(summary.join("\n").into());
        let note = match code {
            "Fn" => "The Fn key cannot be bound.",
            "Enter" => {
                "Enter can only be changed with the command line (`quietlinux bind --force Enter …`), \
                 so that it is not changed by mistake."
            }
            _ => "",
        };
        ui.set_key_note(note.into());
        ui.set_key_editable(!targets.is_empty() && code != "Fn" && code != "Enter");
        ui.set_bind_targets(strings(
            targets.iter().map(|(label, ..)| (*label).to_owned()),
        ));
        // another key: start with its first target
        if self.editor_key != code {
            code.clone_into(&mut self.editor_key);
            ui.set_bind_target(0);
        }
        self.load_editor(ui);
    }

    /// The target the key editor shows.
    fn target(ui: &AppWindow) -> Option<(u8, bool)> {
        let targets = targets(&ui.get_selected_key());
        let (_, id, fn_layer) = *targets.get(usize::try_from(ui.get_bind_target()).ok()?)?;
        Some((id, fn_layer))
    }

    /// Fill the key editor with the binding of the selected key and target.
    fn load_editor(&mut self, ui: &AppWindow) {
        self.editor_target = ui.get_bind_target();
        let Some((key_id, fn_layer)) = Self::target(ui) else {
            return;
        };
        let text = self
            .binding(key_id, fn_layer)
            .map_or(Some("default".to_owned()), |b| {
                config::action_text(&b.action)
            });
        let Some(text) = text else {
            // a binding QuietLinux cannot write: start from the factory binding
            ui.set_bind_kind(0);
            self.kind_changed(ui);
            return;
        };
        let (word, value) = text.split_once(' ').unwrap_or((text.as_str(), ""));
        let kind = BIND_KINDS.iter().position(|(w, _)| *w == word).unwrap_or(0);
        ui.set_bind_kind(i32::try_from(kind).unwrap_or(0));
        self.kind_changed(ui);
        if kind == 1 {
            ui.set_bind_text(value.into());
        } else {
            let values = ui.get_bind_values();
            let index = (0..values.row_count())
                .find(|&i| {
                    values.row_data(i).is_some_and(|v| {
                        v == value || v.split_once(':').is_some_and(|(id, _)| id == value)
                    })
                })
                .unwrap_or(0);
            ui.set_bind_value_index(i32::try_from(index).unwrap_or(0));
        }
    }

    fn editor_changed(&mut self, ui: &AppWindow) {
        if ui.get_bind_target() == self.editor_target {
            self.kind_changed(ui);
        } else {
            self.load_editor(ui);
        }
    }

    /// The value choices of the selected binding kind.
    fn kind_changed(&self, ui: &AppWindow) {
        let names =
            |n: Vec<Option<&'static str>>| n.into_iter().flatten().map(str::to_owned).collect();
        let values: Vec<String> = match ui.get_bind_kind() {
            2 => names(
                MediaAction::all()
                    .into_iter()
                    .map(MediaAction::name)
                    .collect(),
            ),
            3 => names(
                MouseButton::all()
                    .into_iter()
                    .map(MouseButton::name)
                    .collect(),
            ),
            4 => names(
                LightingAction::all()
                    .into_iter()
                    .map(LightingAction::name)
                    .collect(),
            ),
            5 => self
                .state
                .macros
                .iter()
                .flatten()
                .map(|m| format!("{}: {}", m.id, m.name))
                .collect(),
            _ => Vec::new(),
        };
        ui.set_bind_values(strings(values));
        ui.set_bind_value_index(0);
    }

    fn apply_binding(&mut self, ui: &AppWindow) {
        let Some((key_id, fn_layer)) = Self::target(ui) else {
            return;
        };
        let kind = usize::try_from(ui.get_bind_kind()).unwrap_or(0);
        let (word, _) = BIND_KINDS[kind.min(BIND_KINDS.len() - 1)];
        let value = match kind {
            1 => ui.get_bind_text().to_string(),
            2..=5 => {
                let values = ui.get_bind_values();
                let Some(v) = usize::try_from(ui.get_bind_value_index())
                    .ok()
                    .and_then(|i| values.row_data(i))
                else {
                    show_error(ui, "choose a value first");
                    return;
                };
                // macros are listed as "ID: name"
                v.split_once(':')
                    .map_or(v.to_string(), |(id, _)| id.to_owned())
            }
            _ => String::new(),
        };
        let text = format!("{word} {value}");
        match config::parse_desired(text.trim()) {
            Ok(desired) => {
                self.send(
                    ui,
                    Request::Bind {
                        key_id,
                        fn_layer,
                        desired,
                    },
                );
            }
            Err(e) => show_error(ui, &e),
        }
    }

    fn show_macros(&self, ui: &AppWindow) {
        let items: Vec<StandardListViewItem> = self
            .state
            .macros
            .iter()
            .flatten()
            .map(|m| StandardListViewItem::from(format!("{}   {}", m.id, m.name).as_str()))
            .collect();
        ui.set_macro_items(ModelRc::new(VecModel::from(items)));
        let current = ui.get_macro_current();
        let count = self.state.macros.as_ref().map_or(0, Vec::len);
        if usize::try_from(current).is_ok_and(|c| c < count) {
            self.select_macro(ui, current);
        } else {
            ui.set_macro_current(-1);
        }
    }

    fn select_macro(&self, ui: &AppWindow, index: i32) {
        let Some(m) = usize::try_from(index)
            .ok()
            .and_then(|i| self.state.macros.as_ref()?.get(i))
        else {
            return;
        };
        ui.set_macro_name(m.name.as_str().into());
        let repeat = match m.repeat() {
            p::MACRO_WHILE_HELD => 1,
            p::MACRO_REPEAT => 2,
            _ => 0,
        };
        ui.set_macro_repeat(repeat);
        ui.set_macro_events(macros::events_text(&m.events, macros::DEFAULT_DELAY_MS).into());
    }

    fn save_macro(&mut self, ui: &AppWindow) {
        let events = match macros::parse_events(
            &[ui.get_macro_events().to_string()],
            macros::DEFAULT_DELAY_MS,
        ) {
            Ok(events) => events,
            Err(e) => {
                show_error(ui, &e);
                return;
            }
        };
        let repeat = [p::MACRO_ONCE, p::MACRO_WHILE_HELD, p::MACRO_REPEAT]
            .get(usize::try_from(ui.get_macro_repeat()).unwrap_or(0))
            .copied()
            .unwrap_or(p::MACRO_ONCE);
        let id = usize::try_from(ui.get_macro_current())
            .ok()
            .and_then(|i| self.state.macros.as_ref()?.get(i))
            .map(|m| m.id);
        self.send(
            ui,
            Request::SaveMacro {
                id,
                name: ui.get_macro_name().to_string(),
                repeat: RepeatMode(repeat),
                events,
            },
        );
    }

    fn delete_macro(&mut self, ui: &AppWindow) {
        let id = usize::try_from(ui.get_macro_current())
            .ok()
            .and_then(|i| self.state.macros.as_ref()?.get(i))
            .map(|m| m.id);
        if let Some(id) = id {
            ui.set_macro_current(-1);
            self.send(ui, Request::DeleteMacro(id));
        }
    }

    fn store_effect(&mut self, ui: &AppWindow) {
        let index = |i: i32| usize::try_from(i).unwrap_or(0);
        let Some(effect) = Effect::all().get(index(ui.get_effect_index())).copied() else {
            return;
        };
        let direction = Direction::all()
            .get(index(ui.get_direction_index()))
            .copied()
            .unwrap_or(Direction(0));
        let color = |hex: SharedString| {
            hex.trim()
                .parse::<Rgb>()
                .map_err(|e| format!("color {hex:?}: {e}"))
        };
        let colors = match ui.get_color_mode() {
            0 => color(ui.get_color1()).map(Colors::Single),
            1 => color(ui.get_color1())
                .and_then(|a| color(ui.get_color2()).map(|b| Colors::Dual(a, b))),
            _ => Ok(Colors::Gradient(settings::rainbow())),
        };
        let colors = match colors {
            Ok(colors) => colors,
            Err(e) => {
                show_error(ui, &e);
                return;
            }
        };
        let percent = |v: f32| v.round().clamp(0.0, 100.0);
        // clamped to 0..=100 above
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (brightness, speed) = (
            percent(ui.get_brightness()) as u8,
            percent(ui.get_speed()) as u8,
        );
        self.send(
            ui,
            Request::Effect {
                effect: effect.0,
                direction: direction.0,
                brightness,
                speed,
                colors,
            },
        );
    }

    fn apply_game_mode(&mut self, ui: &AppWindow) {
        let flags = [
            (ui.get_block_shift_tab(), 0x01),
            (ui.get_block_alt_f4(), 0x02),
            (ui.get_block_win(), 0x04),
            (ui.get_block_alt_tab(), 0x08),
            (ui.get_block_caps_lock(), 0x10),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .fold(0, |flags, (_, bit)| flags | bit);
        self.send(ui, Request::GameMode(flags));
    }

    /// The backup file typed in the window, `~/` expanded.
    fn file(ui: &AppWindow) -> Option<PathBuf> {
        let text = ui.get_file();
        let text = text.trim();
        if text.is_empty() {
            show_error(ui, "type the name of a file first");
            return None;
        }
        match (text.strip_prefix("~/"), std::env::var_os("HOME")) {
            (Some(rest), Some(home)) => Some(PathBuf::from(home).join(rest)),
            _ => Some(PathBuf::from(text)),
        }
    }

    fn save_backup(&mut self, ui: &AppWindow) {
        if let Some(path) = Self::file(ui) {
            let overwrite = ui.get_overwrite();
            self.send(ui, Request::Backup { path, overwrite });
        }
    }

    fn check_file(&mut self, ui: &AppWindow) {
        if let Some(path) = Self::file(ui) {
            self.send(ui, Request::CheckFile(path));
        }
    }

    fn apply_file(&mut self, ui: &AppWindow) {
        if let Some(path) = Self::file(ui) {
            self.send(ui, Request::ApplyFile(path));
        }
    }
}

fn wheel_pick(p: wheel::Pick) -> WheelPick {
    WheelPick {
        hex: p.color.to_string().into(),
        x: p.x,
        y: p.y,
        value: p.value,
    }
}

fn wheel_image() -> slint::Image {
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(wheel::SIZE, wheel::SIZE);
    for (out, [r, g, b, a]) in buffer.make_mut_slice().iter_mut().zip(wheel::pixels()) {
        *out = slint::Rgba8Pixel { r, g, b, a };
    }
    slint::Image::from_rgba8(buffer)
}

fn show_error(ui: &AppWindow, message: &str) {
    ui.set_status_error(true);
    ui.set_status(message.into());
}

fn swatch(hex: &str) -> slint::Color {
    hex.trim()
        .parse::<Rgb>()
        .map_or(slint::Color::from_argb_u8(0, 0, 0, 0), |c| {
            slint::Color::from_rgb_u8(c.r, c.g, c.b)
        })
}

/// Static content and callbacks of a new window.
fn set_up(ui: &AppWindow) {
    ui.set_effects(strings(
        Effect::all()
            .into_iter()
            .filter_map(Effect::name)
            .map(str::to_owned),
    ));
    ui.set_directions(strings(
        Direction::all()
            .into_iter()
            .filter_map(Direction::name)
            .map(str::to_owned),
    ));
    ui.set_bind_kinds(strings(
        BIND_KINDS.iter().map(|(_, label)| (*label).to_owned()),
    ));
    ui.set_keyboard_width(keyboard::WIDTH);
    ui.set_keyboard_height(keyboard::HEIGHT);
    ui.on_swatch(|hex| swatch(&hex));
    ui.set_wheel(wheel_image());
    ui.on_wheel_pick(|x, y, value| wheel_pick(wheel::pick(x, y, value)));
    ui.on_wheel_find(|hex| {
        // an unreadable color: the center of the wheel, white
        let color = hex.trim().parse::<Rgb>().unwrap_or(Rgb::new(255, 255, 255));
        wheel_pick(wheel::find(color))
    });
    ui.set_file(DEFAULT_FILE.into());
    ui.on_apply_game_mode(|| with_controller(Controller::apply_game_mode));
    ui.on_save_backup(|| with_controller(Controller::save_backup));
    ui.on_check_file(|| with_controller(Controller::check_file));
    ui.on_apply_file(|| with_controller(Controller::apply_file));
    ui.on_refresh(|| with_controller(|c, ui| c.send(ui, Request::Refresh)));
    ui.on_store_effect(|| with_controller(Controller::store_effect));
    ui.on_light_off(|| with_controller(|c, ui| c.send(ui, Request::LightOff)));
    ui.on_light_on(|| with_controller(|c, ui| c.send(ui, Request::LightOn)));
    ui.on_key_clicked(|code| with_controller(|c, ui| c.select_key(ui, &code)));
    ui.on_editor_changed(|| with_controller(Controller::editor_changed));
    ui.on_apply_binding(|| with_controller(Controller::apply_binding));
    ui.on_macro_selected(|i| with_controller(|c, ui| c.select_macro(ui, i)));
    ui.on_macro_new(|| {
        with_controller(|_, ui| {
            ui.set_macro_current(-1);
            ui.set_macro_name("".into());
            ui.set_macro_repeat(0);
            ui.set_macro_events("".into());
        });
    });
    ui.on_macro_save(|| with_controller(Controller::save_macro));
    ui.on_macro_delete(|| with_controller(Controller::delete_macro));
}

/// Start the backend thread; its answers go back to the UI thread.
fn spawn_backend(mut backend: Backend) -> mpsc::Sender<Request> {
    let (tx, rx) = mpsc::channel::<Request>();
    std::thread::spawn(move || {
        for request in rx {
            let outcome = backend.handle(request);
            let delivered = slint::invoke_from_event_loop(move || {
                with_controller(|c, ui| c.apply(ui, outcome));
            });
            if delivered.is_err() {
                break;
            }
        }
    });
    tx
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(file) = cli.screenshot {
        return screenshot::run(&file, cli.page, cli.theme);
    }
    let ui = AppWindow::new()?;
    set_up(&ui);
    ui.set_theme(prefs::theme());
    ui.on_theme_chosen(|| {
        with_controller(|_, ui| {
            if let Err(e) = prefs::save_theme(ui.get_theme()) {
                show_error(ui, &format!("theme not saved: {e}"));
            }
        });
    });
    let backend = if cli.demo {
        ui.set_device_title("Demo: simulated Light Mount".into());
        Backend::demo()
    } else {
        Backend::Hardware {
            usb_path: cli.usb_path,
        }
    };
    let requests = spawn_backend(backend);
    CONTROLLER.with(|c| {
        *c.borrow_mut() = Some(Controller::new(&ui, Some(requests)));
    });
    with_controller(|c, ui| c.send(ui, Request::Refresh));
    ui.run()?;
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("quietlinux-gui: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use quietlinux_core::config::Desired;
    use quietlinux_core::info::Action;

    use super::*;

    #[test]
    fn cli_definition() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn bind_kind_words_parse() {
        for (word, _) in BIND_KINDS {
            let text = match word {
                "key" => "key ctrl+KeyC",
                "media" => "media stop",
                "mouse" => "mouse left",
                "lighting" => "lighting brighter",
                "macro" => "macro 1",
                other => other,
            };
            assert!(config::parse_desired(text).is_ok(), "{text}");
        }
        assert_eq!(
            config::parse_desired("macro 2"),
            Ok(Desired::Action(Action::Macro(2)))
        );
    }
}
