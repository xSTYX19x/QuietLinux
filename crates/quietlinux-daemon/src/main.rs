//! `quietlinux-daemon`: shows the Light Mount's per-key colors again, opens website
//! keys and switches profiles.
//!
//! Per-key colors (realtime mode) are not stored in the keyboard: they are gone
//! after it is unplugged or the computer sleeps. This service looks for the
//! keyboard every few seconds and, when it appears or the computer woke up,
//! shows again what QuietLinux showed last (`~/.config/quietlinux/lighting.toml`,
//! written by the window and the command line).
//!
//! While the keyboard is there, it also listens to its key events (interface 2
//! reports every key press) for keys bound to "open website", which it opens
//! with `xdg-open`, and to profile keys, which switch the profile
//! (`~/.config/quietlinux/profiles`). Every other key press is dropped at once:
//! nothing is stored or logged.
//!
//! Run it as a systemd user service (packaging/systemd/quietlinux-daemon.service).

use std::process::{Command, ExitCode, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

use clap::Parser;
use quietlinux_core::device::{self, LightingMode, Session, Transport};
use quietlinux_core::info::{self, Action, Binding, parse_action};
use quietlinux_core::keymap;
use quietlinux_core::profile::{self, Profiles, Target};
use quietlinux_core::protocol::{self as p, Message};
use quietlinux_core::state::{self, Mode};

#[derive(Debug, Parser)]
#[command(
    name = "quietlinux-daemon",
    version,
    about = "Background service for the be quiet! Light Mount: shows per-key colors again after replugging or sleep, opens the websites of website keys, switches profiles with profile keys"
)]
struct Cli {
    /// USB path of the keyboard to watch when several are plugged in (see `quietlinux status`)
    #[arg(long, value_name = "PATH")]
    usb_path: Option<String>,
    /// Show the last lighting once and exit (instead of watching)
    #[arg(long)]
    once: bool,
    /// Seconds between two looks for the keyboard
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u64).range(1..=60))]
    interval: u64,
}

/// A wake-up from sleep: the wall clock went on, the monotonic clock (which
/// stops during sleep) did not. Smaller differences are clock adjustments.
const SLEPT: Duration = Duration::from_secs(5);
/// Time for a replugged keyboard to settle before it is used.
const SETTLE: Duration = Duration::from_secs(1);

/// Decides when to show the lighting again.
#[derive(Debug)]
struct Watch {
    present: bool,
    wall: SystemTime,
    mono: Instant,
}

impl Watch {
    fn new(wall: SystemTime, mono: Instant) -> Self {
        // not present yet: a keyboard found at the start is shown at once
        Self {
            present: false,
            wall,
            mono,
        }
    }

    /// One look: whether the lighting should be shown again now.
    fn step(&mut self, present: bool, wall: SystemTime, mono: Instant) -> bool {
        let wall_passed = wall.duration_since(self.wall).unwrap_or_default();
        let mono_passed = mono.saturating_duration_since(self.mono);
        let woke_up = wall_passed.saturating_sub(mono_passed) >= SLEPT;
        let appeared = present && !self.present;
        self.present = present;
        self.wall = wall;
        self.mono = mono;
        present && (appeared || woke_up)
    }
}

/// Show what QuietLinux showed last; a line for the log.
fn show_again(usb_path: Option<&str>) -> Result<String, Box<dyn std::error::Error>> {
    let lighting = state::load();
    match lighting.mode {
        // the keyboard shows its stored effect by itself
        Mode::Effect => Ok("effect: shown by the keyboard".to_owned()),
        Mode::PerKey if lighting.leds.is_empty() => Ok("per-key: no colors to show".to_owned()),
        Mode::PerKey => {
            let (_, mut session) = device::open(usb_path, false)?;
            session.open_session()?;
            session.set_leds(&lighting.led_list())?;
            Ok(format!(
                "per-key: {} LED color(s) shown again",
                lighting.leds.len()
            ))
        }
        Mode::Off => {
            let (_, mut session) = device::open(usb_path, false)?;
            session.open_session()?;
            session.set_lighting_mode(LightingMode::Off)?;
            Ok("lighting switched off again".to_owned())
        }
    }
}

/// The action types "open website" and "profile" in a binding and a key event.
const WEBSITE: u8 = 0x06;
const PROFILE: u8 = 0x08;

/// What a key press asks this service to do.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HostAction {
    Website(String),
    Profile(Target),
}

impl HostAction {
    /// Only what QuietLinux itself binds: http(s) without spaces, next, previous
    /// and profiles it named.
    fn from_action(action: Action) -> Option<Self> {
        match action {
            Action::OpenWebsite(url) if info::website_ok(&url) => Some(Self::Website(url)),
            Action::Profile(info::PROFILE_NEXT) => Some(Self::Profile(Target::Next)),
            Action::Profile(info::PROFILE_PREVIOUS) => Some(Self::Profile(Target::Previous)),
            Action::ProfileSelect(id) => {
                profile::name_from_id(&id).map(|name| Self::Profile(Target::Name(name)))
            }
            _ => None,
        }
    }
}

/// What a key event (`11:02` notification data) asks for: from the event itself
/// if it carries the whole action, else from the key's binding.
fn host_action(event: &[u8], bindings: impl FnOnce() -> Vec<Binding>) -> Option<HostAction> {
    let [key_id, flags, _state, action @ ..] = event else {
        return None;
    };
    let kind = *action.first()?;
    if kind != WEBSITE && kind != PROFILE {
        return None;
    }
    let fn_layer = flags & 0x80 != 0;
    let action = parse_action(action).map(|(a, _)| a).or_else(|| {
        bindings()
            .into_iter()
            .find(|b| b.key_id == *key_id && b.fn_layer == fn_layer)
            .map(|b| b.action)
    })?;
    HostAction::from_action(action)
}

/// Open a website in the user's browser, without a shell.
fn open_website(url: &str) {
    match Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        // reaped in the background: the browser may run on
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(e) => eprintln!("quietlinux-daemon: xdg-open: {e}"),
    }
}

/// Switch the profile, saving the keyboard's settings to the active one first.
fn switch_profile<T: Transport>(session: &mut Session<T>, target: &Target) {
    let Some(profiles) = Profiles::standard() else {
        eprintln!("quietlinux-daemon: no home folder: profiles cannot be found");
        return;
    };
    let _ = session.open_session();
    match profiles.switch(session, target, true, false) {
        Ok(switch) => {
            if let Some(saved) = &switch.saved {
                eprintln!("quietlinux-daemon: settings saved to profile {saved:?}");
            }
            for note in &switch.plan.notes {
                eprintln!("quietlinux-daemon: note: {note}");
            }
            eprintln!(
                "quietlinux-daemon: switched to profile {:?} ({} change(s))",
                switch.name,
                switch.plan.ops.len()
            );
        }
        Err(e) => eprintln!("quietlinux-daemon: profile {target}: {e}"),
    }
}

/// Shortest time between two actions: a flood of key events (a stuck key, a
/// misbehaving keyboard) opens one browser tab or switches the profile once a
/// second at most.
const ACTION_PAUSE: Duration = Duration::from_secs(1);

/// Handle a notification: only key events of website and profile keys do
/// something; all other key presses are dropped at once (nothing is stored or
/// logged).
fn on_notification<T: Transport>(
    session: &mut Session<T>,
    note: &Message,
    last_action: &mut Option<Instant>,
) {
    if note.cmd != p::ACTION_FIRED {
        return;
    }
    if last_action.is_some_and(|t| t.elapsed() < ACTION_PAUSE) {
        return;
    }
    let read_bindings = || {
        // the event does not carry the whole action: read the binding
        let _ = session.open_session();
        session.read_bindings().map(|(_, b)| b).unwrap_or_default()
    };
    let Some(action) = host_action(&note.data, read_bindings) else {
        return;
    };
    let key = keymap::code_for_key_id(note.data[0]).unwrap_or("a key");
    match action {
        HostAction::Website(url) => {
            eprintln!("quietlinux-daemon: opening the website bound to {key}");
            open_website(&url);
        }
        HostAction::Profile(target) => {
            eprintln!("quietlinux-daemon: {key}: profile {target}");
            switch_profile(session, &target);
        }
    }
    *last_action = Some(Instant::now());
}

fn log_show_again(usb_path: Option<&str>) {
    sleep(SETTLE);
    match show_again(usb_path) {
        Ok(done) => eprintln!("quietlinux-daemon: {done}"),
        Err(e) => eprintln!("quietlinux-daemon: {e}"),
    }
}

fn run(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let usb_path = cli.usb_path.as_deref();
    if cli.once {
        println!("{}", show_again(usb_path)?);
        return Ok(());
    }
    let interval = Duration::from_secs(cli.interval);
    eprintln!("quietlinux-daemon: watching for the Light Mount");
    let mut watch = Watch::new(SystemTime::now(), Instant::now());
    let mut last_error = String::new();
    let mut last_action = None;
    loop {
        match device::open(usb_path, false) {
            Ok((_, mut session)) => {
                last_error.clear();
                if watch.step(true, SystemTime::now(), Instant::now()) {
                    log_show_again(usb_path);
                }
                // listen for website and profile keys until the keyboard is gone
                loop {
                    match session.next_notification(interval) {
                        Ok(Some(note)) => on_notification(&mut session, &note, &mut last_action),
                        Ok(None) => {}
                        Err(_) => break,
                    }
                    if watch.step(true, SystemTime::now(), Instant::now()) {
                        log_show_again(usb_path);
                    }
                }
                eprintln!("quietlinux-daemon: the Light Mount is gone");
                watch.step(false, SystemTime::now(), Instant::now());
            }
            Err(e) => {
                // say it once, not every few seconds
                let error = e.to_string();
                if error != last_error && !matches!(e, device::Error::NotFound) {
                    eprintln!("quietlinux-daemon: {error}");
                }
                last_error = error;
                watch.step(false, SystemTime::now(), Instant::now());
                sleep(interval);
            }
        }
    }
}

fn main() -> ExitCode {
    match run(&Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("quietlinux-daemon: {e}");
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
    fn website_keys() {
        let site = "https://example.org";
        let website = || Some(HostAction::Website(site.to_owned()));
        let bound = |key_id, fn_layer| Binding {
            key_id,
            fn_layer,
            action: Action::OpenWebsite(site.to_owned()),
            raw: Vec::new(),
        };
        // the event carries the address
        let mut event = vec![0x03, 0x00, 0x01, WEBSITE, 19];
        event.extend_from_slice(site.as_bytes());
        assert_eq!(host_action(&event, Vec::new), website());
        // only the action type: from the binding, on the right layer
        assert_eq!(
            host_action(&[0x03, 0x00, 0x01, WEBSITE], || vec![bound(0x03, false)]),
            website()
        );
        assert_eq!(
            host_action(&[0x03, 0x80, 0x01, WEBSITE], || vec![bound(0x03, false)]),
            None
        );
        // other key presses: the bindings are not even read
        let unread = || -> Vec<Binding> { panic!("bindings read for a plain key") };
        assert_eq!(
            host_action(&[0x0e, 0x00, 0x01, 0x01, 0x00, 0x04], unread),
            None
        );
        // an address QuietLinux would not bind is not opened
        let mut odd = vec![0x03, 0x00, 0x01, WEBSITE, 9];
        odd.extend_from_slice(b"file:///x");
        assert_eq!(host_action(&odd, Vec::new), None);
    }

    #[test]
    fn profile_keys() {
        let profile = |name: &str| Some(HostAction::Profile(name.parse().unwrap()));
        assert_eq!(
            host_action(&[0x06, 0x00, 0x01, PROFILE, 3], Vec::new),
            profile("next")
        );
        assert_eq!(
            host_action(&[0x06, 0x00, 0x01, PROFILE, 4], Vec::new),
            profile("previous")
        );
        // a profile QuietLinux named, from the event or from the binding
        let mut select = vec![0x06, 0x00, 0x01, PROFILE, 1];
        select.extend_from_slice(&profile::id("gaming"));
        assert_eq!(host_action(&select, Vec::new), profile("gaming"));
        let bound = Binding {
            key_id: 0x06,
            fn_layer: false,
            action: Action::ProfileSelect(profile::id("work")),
            raw: Vec::new(),
        };
        assert_eq!(
            host_action(&[0x06, 0x00, 0x01, PROFILE, 1], || vec![bound]),
            profile("work")
        );
        // IO Center's profiles (UUIDs) and "cycle" are not switched
        let mut uuid = vec![0x06, 0x00, 0x01, PROFILE, 1];
        uuid.extend_from_slice(&[
            0x43, 0xd0, 0x54, 0x4b, 0x88, 0x25, 0x4f, 0x64, 0xb2, 0xd2, 0xb1, 0x76, 0x87, 0xad,
            0x17, 0x3d,
        ]);
        assert_eq!(host_action(&uuid, Vec::new), None);
        assert_eq!(host_action(&[0x06, 0x00, 0x01, PROFILE, 2], Vec::new), None);
    }

    #[test]
    fn shows_again_when_it_appears_or_after_sleep() {
        let wall = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let mono = Instant::now();
        let at = |s: u64, m: u64| (wall + Duration::from_secs(s), mono + Duration::from_secs(m));
        let mut watch = Watch::new(wall, mono);
        let mut step = |present, (w, m)| watch.step(present, w, m);
        assert!(step(true, at(2, 2))); // present at the start
        assert!(!step(true, at(4, 4)));
        assert!(!step(false, at(6, 6))); // unplugged
        assert!(step(true, at(8, 8))); // plugged in again
        assert!(!step(true, at(10, 10)));
        assert!(step(true, at(612, 12))); // slept 10 minutes
        assert!(!step(true, at(614, 14)));
        assert!(!step(true, at(617, 16))); // a clock adjustment of 1 s
        assert!(!step(false, at(1200, 18))); // slept while unplugged: nothing to show
        assert!(step(true, at(1202, 20)));
    }
}
