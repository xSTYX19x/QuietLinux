//! Vendor LED indexes (`10:0d`) per zone.
//!
//! Indexes from `lightmount/layout.py` of `bbossa/keyboard_bequiet_lightmount_rgb`,
//! checked visually on the keyboard. ISO and ANSI keyboards share the numbering;
//! only the keys that exist differ ([`unused`]).

use std::fmt;
use std::ops::RangeInclusive;
use std::str::FromStr;

use crate::keymap::Form;

/// Highest LED index.
pub const LAST: u8 = 0xa7;

/// Indexes inside the key range without a key on this form: `62` is the ANSI
/// backslash key, `77` and `7e` are the two ISO keys, `89` is never written by
/// IO Center.
pub const fn unused(form: Form) -> &'static [u8] {
    match form {
        Form::Iso => &[0x62, 0x89],
        Form::Ansi => &[0x77, 0x7e, 0x89],
    }
}

/// True if the keyboard of this form has an LED with this index.
pub fn exists(led: u8, form: Form) -> bool {
    led <= LAST && !unused(form).contains(&led)
}

/// True if a keyboard of some form has an LED with this index (files that may
/// come from another keyboard are checked with this).
pub fn exists_on_any(led: u8) -> bool {
    Form::ALL.into_iter().any(|form| exists(led, form))
}

/// A predefined lighting zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Zone {
    /// Top light bar (45 LEDs).
    Top,
    /// Volume knob.
    Knob,
    /// Keys, row by row, left to right.
    Keys,
    /// Left side strip.
    Left,
    /// Right side strip.
    Right,
}

impl Zone {
    pub const ALL: [Zone; 5] = [Zone::Top, Zone::Knob, Zone::Keys, Zone::Left, Zone::Right];

    pub const fn name(self) -> &'static str {
        match self {
            Zone::Top => "top",
            Zone::Knob => "knob",
            Zone::Keys => "keys",
            Zone::Left => "left",
            Zone::Right => "right",
        }
    }

    const fn range(self) -> RangeInclusive<u8> {
        match self {
            Zone::Top => 0x00..=0x2c,
            Zone::Knob => 0x2d..=0x2d,
            Zone::Keys => 0x2e..=0x9d,
            Zone::Left => 0x9e..=0xa2,
            Zone::Right => 0xa3..=0xa7,
        }
    }

    /// LED indexes of the zone on a keyboard of this form.
    pub fn leds(self, form: Form) -> impl Iterator<Item = u8> {
        self.range().filter(move |&i| exists(i, form))
    }
}

impl fmt::Display for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Zone {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Zone::ALL
            .into_iter()
            .find(|z| z.name() == s)
            .ok_or_else(|| format!("unknown zone {s:?} (known: top, knob, keys, left, right)"))
    }
}

/// Every LED index of a keyboard of this form (ISO 166, ANSI 165).
pub fn all_leds(form: Form) -> impl Iterator<Item = u8> {
    (0..=LAST).filter(move |&i| exists(i, form))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts() {
        assert_eq!(all_leds(Form::Iso).count(), 166);
        assert_eq!(all_leds(Form::Ansi).count(), 165);
        for form in Form::ALL {
            assert_eq!(Zone::Top.leds(form).count(), 45);
            assert_eq!(Zone::Knob.leds(form).collect::<Vec<_>>(), [0x2d]);
            assert_eq!(Zone::Left.leds(form).count(), 5);
            assert_eq!(Zone::Right.leds(form).count(), 5);
        }
        assert_eq!(Zone::Keys.leds(Form::Iso).count(), 0x9e - 0x2e - 2);
        assert_eq!(Zone::Keys.leds(Form::Ansi).count(), 0x9e - 0x2e - 3);
    }

    #[test]
    fn zones_cover_all_leds_once() {
        for form in Form::ALL {
            let mut leds: Vec<u8> = Zone::ALL.into_iter().flat_map(|z| z.leds(form)).collect();
            leds.sort_unstable();
            assert_eq!(leds, all_leds(form).collect::<Vec<_>>(), "{form:?}");
        }
    }

    #[test]
    fn forms_differ_in_three_keys() {
        // the ANSI backslash key, and the ISO keys left of Enter and right of Shift
        assert!(exists(0x62, Form::Ansi) && !exists(0x62, Form::Iso));
        for iso_only in [0x77, 0x7e] {
            assert!(exists(iso_only, Form::Iso) && !exists(iso_only, Form::Ansi));
        }
        assert!(!exists_on_any(0x89));
        assert!(!exists_on_any(LAST + 1));
        assert!(exists_on_any(0x62) && exists_on_any(0x77));
    }

    #[test]
    fn parse() {
        assert_eq!("knob".parse(), Ok(Zone::Knob));
        assert!("Knob".parse::<Zone>().is_err());
    }
}
