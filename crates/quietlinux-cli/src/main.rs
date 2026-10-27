//! `quietlinux`: command line tool for the be quiet! Light Mount keyboard.

use std::process::ExitCode;
use std::str::FromStr;

use clap::{Parser, Subcommand};
use quietlinux_core::Rgb;
use quietlinux_core::device::{self, fmt_version};
use quietlinux_core::layout::{self, Zone};
use quietlinux_core::protocol::Led;

#[derive(Debug, Parser)]
#[command(
    name = "quietlinux",
    version,
    about = "Configure the be quiet! Light Mount keyboard"
)]
struct Cli {
    /// USB path of the keyboard to use when several are plugged in (see `quietlinux status`)
    #[arg(long, global = true, value_name = "PATH")]
    usb_path: Option<String>,

    /// Allow writing to a firmware version this tool was never tested with
    #[arg(long, global = true)]
    allow_unknown_firmware: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show the keyboard, its firmware version and its lighting mode (read-only)
    Status,
    /// List the lighting zones and their LED indexes (does not touch the keyboard)
    Zones,
    /// Change the lighting
    #[command(subcommand)]
    Light(Light),
}

#[derive(Debug, Subcommand)]
enum Light {
    /// Static color effect, stored in the keyboard
    Static {
        /// Color as 6 hex digits, e.g. ff2a00
        color: Rgb,
        /// Brightness in percent
        #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u8).range(0..=100))]
        brightness: u8,
    },
    /// Per-LED colors by zone (volatile: lost when the keyboard is unplugged)
    ///
    /// Zones not given are switched off. Example: quietlinux light set keys=ff2a00 top=f4dbae
    Set {
        /// ZONE=COLOR pairs; zones: all, top, knob, keys, left, right
        #[arg(required = true, value_name = "ZONE=COLOR")]
        assignments: Vec<Assignment>,
        /// Scale all colors (0.0 to 1.0): per-LED colors have no brightness setting
        #[arg(long, default_value_t = 1.0)]
        dim: f32,
    },
}

/// Target of a `ZONE=COLOR` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    All,
    Zone(Zone),
}

#[derive(Debug, Clone, Copy)]
struct Assignment {
    target: Target,
    color: Rgb,
}

impl FromStr for Assignment {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (zone, color) = s
            .split_once('=')
            .ok_or_else(|| format!("expected ZONE=COLOR, got {s:?}"))?;
        let target = if zone == "all" {
            Target::All
        } else {
            Target::Zone(zone.parse()?)
        };
        Ok(Self {
            target,
            color: color.parse().map_err(|e| format!("{e}"))?,
        })
    }
}

/// One color per LED; later assignments win, LEDs not assigned are off.
fn leds_for(assignments: &[Assignment], dim: f32) -> Vec<Led> {
    layout::all_leds()
        .map(|index| {
            let color = assignments
                .iter()
                .rev()
                .find(|a| match a.target {
                    Target::All => true,
                    Target::Zone(zone) => zone.leds().any(|i| i == index),
                })
                .map_or(Rgb::BLACK, |a| a.color.scaled(dim));
            Led::new(index, color)
        })
        .collect()
}

fn run(cli: Cli) -> Result<(), device::Error> {
    let usb_path = cli.usb_path.as_deref();
    match cli.command {
        Command::Zones => {
            for zone in Zone::ALL {
                let leds: Vec<u8> = zone.leds().collect();
                let (first, last) = (leds[0], leds[leds.len() - 1]);
                println!(
                    "{:<6} {:3} LEDs  {first:02x}-{last:02x}",
                    zone.name(),
                    leds.len()
                );
            }
        }
        Command::Status => {
            let (keyboard, mut session) = device::open(usb_path, cli.allow_unknown_firmware)?;
            let identity = session.identity().clone();
            println!(
                "Light Mount at USB {} ({})",
                keyboard.usb_path,
                keyboard.vendor_node().display()
            );
            let tested = if identity.known() {
                "tested"
            } else {
                "untested, read-only"
            };
            println!(
                "Firmware version: {} ({tested})",
                fmt_version(&identity.version)
            );
            println!("Lighting mode:    {}", session.lighting_mode()?);
        }
        Command::Light(Light::Static { color, brightness }) => {
            let (_, mut session) = device::open(usb_path, cli.allow_unknown_firmware)?;
            session.set_static(color, brightness)?;
        }
        Command::Light(Light::Set { assignments, dim }) => {
            let (_, mut session) = device::open(usb_path, cli.allow_unknown_firmware)?;
            session.set_leds(&leds_for(&assignments, dim))?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("quietlinux: {e}");
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
    fn assignment_parsing() {
        let a: Assignment = "keys=ff2a00".parse().unwrap();
        assert_eq!(
            (a.target, a.color),
            (Target::Zone(Zone::Keys), Rgb::new(0xff, 0x2a, 0))
        );
        assert_eq!(
            "all=000000".parse::<Assignment>().unwrap().target,
            Target::All
        );
        assert!("keys".parse::<Assignment>().is_err());
        assert!("wheel=ff0000".parse::<Assignment>().is_err());
        assert!("keys=red".parse::<Assignment>().is_err());
    }

    #[test]
    fn later_assignments_win() {
        let assignments = [
            "all=111111".parse().unwrap(),
            "knob=ff0000".parse().unwrap(),
        ];
        let leds = leds_for(&assignments, 1.0);
        assert_eq!(leds.len(), 166);
        assert!(leds.iter().all(|l| l.color
            == if l.index == 0x2d {
                Rgb::new(0xff, 0, 0)
            } else {
                Rgb::new(0x11, 0x11, 0x11)
            }));
    }

    #[test]
    fn unassigned_leds_are_off() {
        let leds = leds_for(&["knob=ff0000".parse().unwrap()], 0.5);
        assert!(leds.iter().all(|l| l.color
            == if l.index == 0x2d {
                Rgb::new(0x80, 0, 0)
            } else {
                Rgb::BLACK
            }));
    }
}
