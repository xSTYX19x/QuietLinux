//! `quietlinux-daemon`: shows the Light Mount's per-key colors again and opens website keys.
//!
//! Per-key colors (realtime mode) are not stored in the keyboard: they are gone
//! after it is unplugged or the computer sleeps. This service looks for the
//! keyboard every few seconds and, when it appears or the computer woke up,
//! shows again what QuietLinux showed last (`~/.config/quietlinux/lighting.toml`,
//! written by the window and the command line).
//!
//! While the keyboard is there, it also listens to its key events (interface 2
//! reports every key press) for keys bound to "open website", and opens those
//! with `xdg-open`. Every other key press is dropped at once: nothing is stored
//! or logged.
//!
//! Run it as a systemd user service (packaging/systemd/quietlinux-daemon.service).

use std::process::{Command, ExitCode, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

use clap::Parser;
use quietlinux_core::device::{self, LightingMode, Session, Transport};
use quietlinux_core::info::{self, Action, Binding, parse_action};
use quietlinux_core::keymap;
use quietlinux_core::protocol::{self as p, Message};
use quietlinux_core::state::{self, Mode};

#[derive(Debug, Parser)]
#[command(
    name = "quietlinux-daemon",
    version,
    about = "Background service for the be quiet! Light Mount: shows per-key colors again after replugging or sleep, opens the websites of website keys"
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

/// The action type "open website" in a binding and a key event.
const WEBSITE: u8 = 0x06;

/// The website a key event (`11:02` notification data) asks to open: from the
/// event itself if it carries the address, else from the key's binding.
fn website_for(event: &[u8], bindings: impl FnOnce() -> Vec<Binding>) -> Option<String> {
    let [key_id, flags, _state, action @ ..] = event else {
        return None;
    };
    if action.first() != Some(&WEBSITE) {
        return None;
    }
    let website = |action: Action| match action {
        Action::OpenWebsite(url) => Some(url),
        _ => None,
    };
    let fn_layer = flags & 0x80 != 0;
    let url = parse_action(action)
        .and_then(|(a, _)| website(a))
        .or_else(|| {
            bindings()
                .into_iter()
                .find(|b| b.key_id == *key_id && b.fn_layer == fn_layer)
                .and_then(|b| website(b.action))
        })?;
    // only what QuietLinux itself would bind: http(s), no spaces
    info::website_ok(&url).then_some(url)
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

/// Shortest time between two websites opened: a flood of key events (a stuck
/// key, a misbehaving keyboard) opens one browser tab a second at most.
const WEBSITE_PAUSE: Duration = Duration::from_secs(1);

/// Handle a notification: only key events of website keys do something; all
/// other key presses are dropped at once (nothing is stored or logged).
fn on_notification<T: Transport>(
    session: &mut Session<T>,
    note: &Message,
    last_opened: &mut Option<Instant>,
) {
    if note.cmd != p::ACTION_FIRED {
        return;
    }
    if last_opened.is_some_and(|t| t.elapsed() < WEBSITE_PAUSE) {
        return;
    }
    let read_bindings = || {
        // the event does not name the address: read it from the keyboard
        let _ = session.open_session();
        session.read_bindings().map(|(_, b)| b).unwrap_or_default()
    };
    if let Some(url) = website_for(&note.data, read_bindings) {
        let key = keymap::code_for_key_id(note.data[0]).unwrap_or("a key");
        eprintln!("quietlinux-daemon: opening the website bound to {key}");
        open_website(&url);
        *last_opened = Some(Instant::now());
    }
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
    let mut last_opened = None;
    loop {
        match device::open(usb_path, false) {
            Ok((_, mut session)) => {
                last_error.clear();
                if watch.step(true, SystemTime::now(), Instant::now()) {
                    log_show_again(usb_path);
                }
                // listen for website keys until the keyboard is gone
                loop {
                    match session.next_notification(interval) {
                        Ok(Some(note)) => on_notification(&mut session, &note, &mut last_opened),
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
        let bound = |key_id, fn_layer| Binding {
            key_id,
            fn_layer,
            action: Action::OpenWebsite(site.to_owned()),
            raw: Vec::new(),
        };
        // the event carries the address
        let mut event = vec![0x03, 0x00, 0x01, WEBSITE, 19];
        event.extend_from_slice(site.as_bytes());
        assert_eq!(website_for(&event, Vec::new).as_deref(), Some(site));
        // only the action type: from the binding, on the right layer
        assert_eq!(
            website_for(&[0x03, 0x00, 0x01, WEBSITE], || vec![bound(0x03, false)]).as_deref(),
            Some(site)
        );
        assert_eq!(
            website_for(&[0x03, 0x80, 0x01, WEBSITE], || vec![bound(0x03, false)]),
            None
        );
        // other key presses: the bindings are not even read
        let unread = || -> Vec<Binding> { panic!("bindings read for a plain key") };
        assert_eq!(
            website_for(&[0x0e, 0x00, 0x01, 0x01, 0x00, 0x04], unread),
            None
        );
        // an address QuietLinux would not bind is not opened
        let mut odd = vec![0x03, 0x00, 0x01, WEBSITE, 9];
        odd.extend_from_slice(b"file:///x");
        assert_eq!(website_for(&odd, Vec::new), None);
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
