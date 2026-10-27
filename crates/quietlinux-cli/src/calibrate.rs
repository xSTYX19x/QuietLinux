//! `quietlinux calibrate`: light the keys one by one to check the key map.

use quietlinux_core::Rgb;
use quietlinux_core::device::{Error, LightingMode, Session, Transport};
use quietlinux_core::keymap::Key;
use quietlinux_core::layout;
use quietlinux_core::protocol::Led;

const ON: Rgb = Rgb::new(0xff, 0xff, 0xff);
const OFF: Rgb = Rgb::BLACK;

/// What the user says about the lit key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The lit key is the one shown.
    Match,
    /// Another key is lit: what the user sees.
    Lit(String),
    /// Stop calibrating.
    Stop,
}

/// A key whose LED lit something else.
#[derive(Debug, Clone, PartialEq)]
pub struct Mismatch {
    pub key: &'static Key,
    pub lit: String,
}

/// Outcome of a calibration run.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub checked: usize,
    pub mismatches: Vec<Mismatch>,
    /// The keyboard was in custom mode before: its colors could not be restored.
    pub colors_lost: bool,
}

/// Light `keys` one at a time (everything else off) and ask about each one.
/// Afterwards the stored effect is shown again if it was active before.
pub fn calibrate<T: Transport>(
    session: &mut Session<T>,
    keys: &[&'static Key],
    mut ask: impl FnMut(&Key) -> Answer,
) -> Result<Report, Error> {
    let previous = session.lighting_mode()?;
    let all_off: Vec<Led> = layout::all_leds().map(|i| Led::new(i, OFF)).collect();
    session.set_leds(&all_off)?;

    let mut report = Report {
        checked: 0,
        mismatches: Vec::new(),
        colors_lost: previous != LightingMode::Effects,
    };
    let mut lit: Option<u8> = None;
    for &key in keys {
        let mut update: Vec<Led> = lit.map(|i| Led::new(i, OFF)).into_iter().collect();
        update.push(Led::new(key.led, ON));
        session.set_leds(&update)?;
        lit = Some(key.led);
        match ask(key) {
            Answer::Match => {}
            Answer::Lit(what) => report.mismatches.push(Mismatch { key, lit: what }),
            Answer::Stop => break,
        }
        report.checked += 1;
    }
    if let Some(i) = lit {
        session.set_leds(&[Led::new(i, OFF)])?;
    }
    if previous == LightingMode::Effects {
        session.show_stored_effect()?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use quietlinux_core::device::fake::FakeKeyboard;
    use quietlinux_core::keymap::{self, KEYS};
    use quietlinux_core::protocol::{self as p, Packet};

    use super::*;

    /// LEDs switched on, message by message.
    fn lit_sequence(received: &[Packet]) -> Vec<Vec<u8>> {
        received
            .iter()
            .filter(|r| p::fragment_index(r) == 0 && !p::has_more(r))
            .filter_map(|r| p::parse_leds(std::slice::from_ref(r)))
            .map(|m| {
                m.entries
                    .iter()
                    .filter(|l| l.color == ON)
                    .map(|l| l.index)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn lights_one_key_at_a_time_and_restores_the_effect() {
        let mut kb = FakeKeyboard::tested();
        let keys: Vec<&Key> = ["Escape", "KeyQ", "Knob"]
            .iter()
            .map(|c| keymap::by_code(c).unwrap())
            .collect();
        let mut answers = vec![Answer::Match, Answer::Lit("A".into()), Answer::Match].into_iter();
        let mut session = Session::new(&mut kb, false).unwrap();
        let report = calibrate(&mut session, &keys, |_| answers.next().unwrap()).unwrap();

        assert_eq!(report.checked, 3);
        assert!(!report.colors_lost);
        assert_eq!(
            report.mismatches,
            [Mismatch {
                key: keys[1],
                lit: "A".into()
            }]
        );
        let lit: Vec<Vec<u8>> = lit_sequence(&kb.received)
            .into_iter()
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(lit, [vec![0x2e], vec![0x56], vec![0x2d]]);
        assert!(
            kb.leds.values().all(|&c| c == OFF),
            "everything off at the end"
        );
        assert_eq!(kb.mode, p::MODE_EFFECTS);
    }

    #[test]
    fn stop() {
        let mut kb = FakeKeyboard::tested();
        let keys: Vec<&Key> = KEYS.iter().collect();
        let mut answers = vec![Answer::Match, Answer::Stop].into_iter();
        let mut session = Session::new(&mut kb, false).unwrap();
        let report = calibrate(&mut session, &keys, |_| answers.next().unwrap()).unwrap();
        assert_eq!(report.checked, 1);
        assert_eq!(report.mismatches.len(), 0);
        assert!(kb.leds.values().all(|&c| c == OFF));
    }

    #[test]
    fn custom_mode_colors_are_reported_lost() {
        let mut kb = FakeKeyboard::tested();
        kb.mode = p::MODE_CUSTOM;
        let mut session = Session::new(&mut kb, false).unwrap();
        let report = calibrate(&mut session, &[], |_| Answer::Stop).unwrap();
        assert!(report.colors_lost);
        assert_eq!(kb.mode, p::MODE_CUSTOM);
    }
}
