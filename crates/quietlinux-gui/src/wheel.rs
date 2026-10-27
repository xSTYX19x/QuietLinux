//! The color wheel: hue around the circle, saturation from the center outwards;
//! the value (brightness of the color) is a slider next to it.
//!
//! Points are given from -1 to 1 in both directions, the center at 0; red is at
//! the right, the hue grows counterclockwise.

// h, s, v and r, g, b are the usual names in color math
#![allow(clippy::many_single_char_names)]

use std::f32::consts::PI;

use quietlinux_core::Rgb;

/// Side of the wheel image in pixels.
pub const SIZE: u32 = 256;

/// A point on the wheel and the value: the color it stands for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick {
    pub color: Rgb,
    pub x: f32,
    pub y: f32,
    pub value: f32,
}

/// Color of hue `h` (degrees), saturation `s` and value `v` (0 to 1).
pub fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h {
        h if h < 1.0 => (c, x, 0.0),
        h if h < 2.0 => (x, c, 0.0),
        h if h < 3.0 => (0.0, c, x),
        h if h < 4.0 => (0.0, x, c),
        h if h < 5.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    // 0 to 255 after clamping
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb::new(byte(r), byte(g), byte(b))
}

/// Hue (degrees), saturation and value of a color.
// `max` is one of r, g, b: comparing them exactly finds which one
#[allow(clippy::float_cmp)]
pub fn to_hsv(c: Rgb) -> (f32, f32, f32) {
    let (r, g, b) = (
        f32::from(c.r) / 255.0,
        f32::from(c.g) / 255.0,
        f32::from(c.b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

/// The color at point (`x`, `y`), moved onto the wheel if outside, with `value`.
pub fn pick(x: f32, y: f32, value: f32) -> Pick {
    let r = x.hypot(y);
    let (x, y) = if r > 1.0 { (x / r, y / r) } else { (x, y) };
    let hue = (-y).atan2(x).to_degrees();
    let value = value.clamp(0.0, 1.0);
    Pick {
        color: hsv(hue, r.min(1.0), value),
        x,
        y,
        value,
    }
}

/// Where a color is on the wheel.
pub fn find(color: Rgb) -> Pick {
    let (h, s, v) = to_hsv(color);
    let angle = h * PI / 180.0;
    Pick {
        color,
        x: s * angle.cos(),
        y: -s * angle.sin(),
        value: v,
    }
}

/// The wheel at full value as RGBA pixels, `SIZE` × `SIZE`, transparent outside.
pub fn pixels() -> Vec<[u8; 4]> {
    // SIZE is small: exact in f32
    #[allow(clippy::cast_precision_loss)]
    let half = SIZE as f32 / 2.0;
    let mut out = Vec::with_capacity((SIZE * SIZE) as usize);
    for row in 0..SIZE {
        for col in 0..SIZE {
            // pixel centers, from -1 to 1
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (
                (col as f32 + 0.5 - half) / half,
                (row as f32 + 0.5 - half) / half,
            );
            let r = x.hypot(y);
            // a smooth edge, one pixel wide
            let alpha = ((1.0 - r) * half + 0.5).clamp(0.0, 1.0);
            let c = pick(x, y, 1.0).color;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let a = (alpha * 255.0).round() as u8;
            out.push([c.r, c.g, c.b, a]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_corners() {
        assert_eq!(hsv(0.0, 1.0, 1.0), Rgb::new(255, 0, 0));
        assert_eq!(hsv(120.0, 1.0, 1.0), Rgb::new(0, 255, 0));
        assert_eq!(hsv(240.0, 1.0, 1.0), Rgb::new(0, 0, 255));
        assert_eq!(hsv(0.0, 0.0, 1.0), Rgb::new(255, 255, 255));
        assert_eq!(hsv(0.0, 1.0, 0.0), Rgb::new(0, 0, 0));
    }

    #[test]
    fn points_of_the_wheel() {
        assert_eq!(pick(1.0, 0.0, 1.0).color, Rgb::new(255, 0, 0)); // right: red
        assert_eq!(pick(0.0, 0.0, 1.0).color, Rgb::new(255, 255, 255)); // center: white
        let outside = pick(3.0, 4.0, 1.0);
        assert!((outside.x - 0.6).abs() < 1e-6 && (outside.y - 0.8).abs() < 1e-6);
    }

    #[test]
    fn colors_find_their_point_again() {
        for hex in [
            "e63000", "ffffff", "000000", "1e90ff", "7fff00", "8a2be2", "123456",
        ] {
            let color: Rgb = hex.parse().unwrap();
            let p = find(color);
            let again = pick(p.x, p.y, p.value).color;
            let close = |a: u8, b: u8| a.abs_diff(b) <= 1;
            assert!(
                close(again.r, color.r) && close(again.g, color.g) && close(again.b, color.b),
                "{hex}: {again}"
            );
        }
    }

    #[test]
    fn image_is_round() {
        let px = pixels();
        assert_eq!(px.len(), (SIZE * SIZE) as usize);
        assert_eq!(px[0][3], 0); // corner: transparent
        let center = (SIZE / 2 * SIZE + SIZE / 2) as usize;
        assert_eq!(px[center][3], 255);
    }
}
