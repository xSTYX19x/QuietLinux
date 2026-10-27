//! Vendor LED indexes (`10:0d`) per zone.
//!
//! Indexes from `lightmount/layout.py` of `bbossa/keyboard_bequiet_lightmount_rgb`,
//! checked visually on the keyboard.

use std::fmt;
use std::ops::RangeInclusive;
use std::str::FromStr;

/// Indexes inside the key range that IO Center never writes.
pub const UNUSED: [u8; 2] = [0x62, 0x89];
/// Highest LED index.
pub const LAST: u8 = 0xa7;

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

    /// LED indexes of the zone.
    pub fn leds(self) -> impl Iterator<Item = u8> {
        self.range().filter(|i| !UNUSED.contains(i))
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

/// Every LED index (166).
pub fn all_leds() -> impl Iterator<Item = u8> {
    (0..=LAST).filter(|i| !UNUSED.contains(i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts() {
        assert_eq!(all_leds().count(), 166);
        assert_eq!(Zone::Top.leds().count(), 45);
        assert_eq!(Zone::Knob.leds().collect::<Vec<_>>(), [0x2d]);
        assert_eq!(Zone::Keys.leds().count(), 0x9e - 0x2e - 2);
        assert_eq!(Zone::Left.leds().count(), 5);
        assert_eq!(Zone::Right.leds().count(), 5);
    }

    #[test]
    fn zones_cover_all_leds_once() {
        let mut leds: Vec<u8> = Zone::ALL.into_iter().flat_map(Zone::leds).collect();
        leds.sort_unstable();
        assert_eq!(leds, all_leds().collect::<Vec<_>>());
    }

    #[test]
    fn parse() {
        assert_eq!("knob".parse(), Ok(Zone::Knob));
        assert!("Knob".parse::<Zone>().is_err());
    }
}
