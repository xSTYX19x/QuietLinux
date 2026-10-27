//! RGB colors.

use std::fmt;
use std::str::FromStr;

/// An 8-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const fn to_bytes(self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }

    /// Scale every channel by `factor` (clamped to 0..=1).
    ///
    /// The per-LED command has no brightness, so dimming is done on the color.
    #[must_use]
    pub fn scaled(self, factor: f32) -> Self {
        let factor = factor.clamp(0.0, 1.0);
        // The product stays within 0..=255, so the cast cannot truncate.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let scale = |c: u8| (f32::from(c) * factor).round() as u8;
        Self::new(scale(self.r), scale(self.g), scale(self.b))
    }
}

impl From<[u8; 3]> for Rgb {
    fn from([r, g, b]: [u8; 3]) -> Self {
        Self::new(r, g, b)
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Error for a color that is not 6 hex digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseRgbError(String);

impl fmt::Display for ParseRgbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid color {:?}: expected 6 hex digits like ff2a00",
            self.0
        )
    }
}

impl std::error::Error for ParseRgbError {}

impl FromStr for Rgb {
    type Err = ParseRgbError;

    /// Parse `rrggbb`, with or without a leading `#`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.strip_prefix('#').unwrap_or(s);
        if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(ParseRgbError(s.to_owned()));
        }
        let channel = |i: usize| {
            u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ParseRgbError(s.to_owned()))
        };
        Ok(Self::new(channel(0)?, channel(2)?, channel(4)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display() {
        assert_eq!("ff2a00".parse(), Ok(Rgb::new(0xff, 0x2a, 0x00)));
        assert_eq!("#F4DBAE".parse(), Ok(Rgb::new(0xf4, 0xdb, 0xae)));
        assert_eq!(Rgb::new(1, 2, 3).to_string(), "010203");
    }

    #[test]
    fn parse_rejects_garbage() {
        for bad in [
            "", "fff", "ff2a0", "ff2a000", "gg0000", "#", "+f2a00", "ff 2a0",
        ] {
            assert!(bad.parse::<Rgb>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn scaled() {
        assert_eq!(Rgb::new(200, 100, 0).scaled(0.5), Rgb::new(100, 50, 0));
        assert_eq!(Rgb::new(200, 100, 0).scaled(2.0), Rgb::new(200, 100, 0));
        assert_eq!(Rgb::new(200, 100, 0).scaled(-1.0), Rgb::BLACK);
    }
}
