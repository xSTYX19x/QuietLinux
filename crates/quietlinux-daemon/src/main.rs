//! `quietlinux-daemon`: shows the Light Mount's per-key colors again.
//!
//! Per-key colors (realtime mode) are not stored in the keyboard: they are gone
//! after it is unplugged or the computer sleeps. This service looks for the
//! keyboard every few seconds and, when it appears or the computer woke up,
//! shows again what QuietLinux showed last (`~/.config/quietlinux/lighting.toml`,
//! written by the window and the command line). It sends nothing otherwise.
//!
//! Run it as a systemd user service (packaging/systemd/quietlinux-daemon.service).

use std::path::Path;
use std::process::ExitCode;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

use clap::Parser;
use quietlinux_core::device::{self, LightingMode};
use quietlinux_core::state::{self, Mode};

#[derive(Debug, Parser)]
#[command(
    name = "quietlinux-daemon",
    version,
    about = "Show the Light Mount's per-key colors again after replugging or sleep"
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

fn run(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let usb_path = cli.usb_path.as_deref();
    if cli.once {
        println!("{}", show_again(usb_path)?);
        return Ok(());
    }
    eprintln!(
        "quietlinux-daemon: watching for the Light Mount every {} s",
        cli.interval
    );
    let mut watch = Watch::new(SystemTime::now(), Instant::now());
    loop {
        let present = device::find(Path::new(device::SYSFS), usb_path).is_ok();
        if watch.step(present, SystemTime::now(), Instant::now()) {
            sleep(SETTLE);
            match show_again(usb_path) {
                Ok(done) => eprintln!("quietlinux-daemon: {done}"),
                Err(e) => eprintln!("quietlinux-daemon: {e}"),
            }
        }
        sleep(Duration::from_secs(cli.interval));
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
