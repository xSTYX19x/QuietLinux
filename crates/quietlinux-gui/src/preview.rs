//! Live preview of an effect on the keyboard view: an approximation of the
//! keyboard's own animation, drawn in the window only. Nothing is sent to the
//! keyboard for it.

use quietlinux_core::Rgb;
use quietlinux_core::config::EffectSetting;
use quietlinux_core::info::Colors;

use crate::keyboard;

/// Length of one color wave, in key units.
const WAVE_LENGTH: f32 = 16.0;
/// Breathing with a gradient: animation cycles for one pass through its colors.
const GRADIENT_BREATHS: f32 = 3.0;
/// Reactive: how often a key is "pressed", in seconds.
const PRESS_INTERVAL: f32 = 0.3;
/// Matrix: length of a drop's tail, and how far a drop runs in one animation
/// cycle, in key units.
const TAIL: f32 = 3.0;
const DROP_RUN: f32 = 12.0;

/// Seconds of one animation cycle at `speed` percent: 6 s slow, 1 s fast.
fn period(speed: u8) -> f32 {
    6.0 - 5.0 * f32::from(speed.min(100)) / 100.0
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let channel = |x: u8, y: u8| {
        let v = f32::from(x) + (f32::from(y) - f32::from(x)) * t;
        // 0 to 255 after the clamp
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = v.round().clamp(0.0, 255.0) as u8;
        v
    };
    Rgb::new(channel(a.r, b.r), channel(a.g, b.g), channel(a.b, b.b))
}

fn scaled(c: Rgb, factor: f32) -> Rgb {
    mix(Rgb::BLACK, c, factor)
}

/// The color at `u` (0 to 1, repeating) along the effect's colors: one color,
/// two colors back and forth, or the gradient's stops (the last one blends back
/// into the first).
fn palette(colors: &Colors, u: f32) -> Rgb {
    let u = u.rem_euclid(1.0);
    match colors {
        Colors::Single(c) => *c,
        Colors::Dual(a, b) => mix(*a, *b, 1.0 - (2.0 * u - 1.0).abs()),
        Colors::Gradient(stops) | Colors::OrientedGradient { stops, .. } => {
            let Some(&(first, first_at)) = stops.first() else {
                return Rgb::BLACK;
            };
            let at = |p: u8| f32::from(p.min(100)) / 100.0;
            let mut prev = (first, at(first_at));
            for &(color, pos) in &stops[1..] {
                let pos = at(pos);
                if u <= pos {
                    let span = pos - prev.1;
                    let t = if span > 0.0 { (u - prev.1) / span } else { 1.0 };
                    return mix(prev.0, color, t);
                }
                prev = (color, pos);
            }
            // after the last stop: back to the first one
            let span = 1.0 - prev.1 + at(first_at);
            let t = if span > 0.0 { (u - prev.1) / span } else { 1.0 };
            mix(prev.0, first, t)
        }
    }
}

/// The first and second color: a key press and the background (reactive).
fn two_colors(colors: &Colors) -> (Rgb, Rgb) {
    match colors {
        Colors::Single(c) => (*c, Rgb::BLACK),
        Colors::Dual(a, b) => (*a, *b),
        gradient => (palette(gradient, 0.0), Rgb::BLACK),
    }
}

/// A number from 0 to 1 that looks random, the same for the same `n`.
fn noise(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    // the top 24 bits: exact in an f32
    #[allow(clippy::cast_precision_loss)]
    let v = (x >> 8) as f32 / (1 << 24) as f32;
    v
}

/// The color of the key at `index` (of `count`), centered at `pos` (x, y) in
/// key units, `time` seconds into the animation.
pub fn color(
    setting: &EffectSetting,
    index: usize,
    count: usize,
    pos: (f32, f32),
    time: f32,
) -> Rgb {
    let cycles = time / period(setting.speed);
    let direction = setting.direction.name();
    let colors = &setting.colors;
    let raw = match setting.effect.name().unwrap_or("static") {
        "color-wave" => wave(colors, direction, pos, cycles),
        "tornado" => tornado(colors, direction, pos, cycles),
        "breathing" => breathing(colors, cycles),
        "reactive" => reactive(colors, period(setting.speed) / 2.0, index, count, time),
        "matrix" => matrix(colors, direction, pos, cycles),
        // static: one color
        _ => two_colors(colors).0,
    };
    scaled(raw, f32::from(setting.brightness.min(100)) / 100.0)
}

fn wave(colors: &Colors, direction: Option<&str>, (x, y): (f32, f32), cycles: f32) -> Rgb {
    let along = match direction {
        Some("down") => -y,
        Some("left") => x,
        Some("right") => -x,
        _ => y, // up
    };
    palette(colors, along / WAVE_LENGTH + cycles)
}

fn tornado(colors: &Colors, direction: Option<&str>, (x, y): (f32, f32), cycles: f32) -> Rgb {
    let (cx, cy) = (keyboard::WIDTH / 2.0, keyboard::HEIGHT / 2.0);
    // the view's y axis points down: a growing angle turns clockwise
    let angle = (y - cy).atan2((x - cx) / 2.0) / std::f32::consts::TAU;
    let turn = if direction == Some("counterclockwise") {
        cycles
    } else {
        -cycles
    };
    match colors {
        // one color turns together with black, as on the keyboard
        Colors::Single(single) => palette(&Colors::Dual(*single, Rgb::BLACK), angle + turn),
        colors => palette(colors, angle + turn),
    }
}

/// One color, or two taking turns, glow up and go dark (color 1, black, color
/// 2, black, …); a gradient runs through its colors without going dark, as on
/// the keyboard.
fn breathing(colors: &Colors, cycles: f32) -> Rgb {
    let level = (1.0 - (cycles * std::f32::consts::TAU).cos()) / 2.0;
    let breath = cycles.floor();
    match colors {
        Colors::Single(single) => scaled(*single, level),
        Colors::Dual(first, second) => {
            let color = if breath.rem_euclid(2.0) < 1.0 {
                *first
            } else {
                *second
            };
            scaled(color, level)
        }
        gradient => palette(gradient, cycles / GRADIENT_BREATHS),
    }
}

/// Keys "pressed" one after another, glowing for `fade` seconds.
fn reactive(colors: &Colors, fade: f32, index: usize, count: usize, time: f32) -> Rgb {
    let (press, background) = two_colors(colors);
    // the newest press of this key, if it still glows
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (slot, back) = (
        (time / PRESS_INTERVAL).floor().max(0.0) as u32,
        (fade / PRESS_INTERVAL).ceil() as u32 + 1,
    );
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let pressed = (slot.saturating_sub(back)..=slot)
        .rev()
        .find(|&s| (noise(s) * count as f32) as usize == index)
        .map(|s| time - s as f32 * PRESS_INTERVAL);
    match pressed {
        Some(age) if age < fade => mix(press, background, age / fade),
        _ => background,
    }
}

/// Drops with a fading tail running in the direction across the keyboard: one
/// per lane (a row of keys, or a column one key wide), all at the same speed,
/// each lane starting at its own time.
fn matrix(colors: &Colors, direction: Option<&str>, (x, y): (f32, f32), cycles: f32) -> Rgb {
    let (along, across, length) = match direction {
        Some("down") => (y, x, keyboard::HEIGHT),
        Some("left") => (keyboard::WIDTH - x, y, keyboard::WIDTH),
        Some("right") => (x, y, keyboard::WIDTH),
        _ => (keyboard::HEIGHT - y, x, keyboard::HEIGHT), // up
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lane = across.floor().max(0.0) as u32;
    // the drop runs in, out with its tail, and after a pause (up to a keyboard's
    // width, also for the short columns: not every lane at once) in again
    let span = length + TAIL + noise(lane) * keyboard::WIDTH;
    let head = (cycles * DROP_RUN + noise(lane + 1000) * span).rem_euclid(span);
    let behind = head - along;
    let drop = match colors {
        Colors::Single(single) => *single,
        Colors::Dual(first, _) => *first,
        gradient => palette(gradient, noise(lane + 2000)),
    };
    if (0.0..TAIL).contains(&behind) {
        scaled(drop, 1.0 - behind / TAIL)
    } else if let Colors::Dual(_, background) = colors {
        scaled(*background, 0.25)
    } else {
        Rgb::BLACK
    }
}

/// Whether the effect changes over time (static does not).
pub fn moves(e: &EffectSetting) -> bool {
    e.effect.name() != Some("static")
}

#[cfg(test)]
mod tests {
    use super::*;
    use quietlinux_core::settings::{Direction, Effect};

    fn setting(name: &str, colors: Colors) -> EffectSetting {
        let effect: Effect = name.parse().unwrap();
        EffectSetting {
            effect,
            direction: effect.direction(None).unwrap(),
            brightness: 100,
            speed: 50,
            colors,
        }
    }

    const RED: Rgb = Rgb::new(255, 0, 0);
    const BLUE: Rgb = Rgb::new(0, 0, 255);

    #[test]
    fn palettes() {
        let dual = Colors::Dual(RED, BLUE);
        assert_eq!(palette(&dual, 0.0), RED);
        assert_eq!(palette(&dual, 0.5), BLUE);
        assert_eq!(palette(&dual, 1.0), RED);
        let gradient = Colors::Gradient(vec![(RED, 0), (BLUE, 50)]);
        assert_eq!(palette(&gradient, 0.5), BLUE);
        assert_eq!(palette(&gradient, 0.25), mix(RED, BLUE, 0.5));
        // after the last stop it blends back into the first
        assert_eq!(palette(&gradient, 0.75), mix(BLUE, RED, 0.5));
        assert_eq!(palette(&Colors::Gradient(Vec::new()), 0.3), Rgb::BLACK);
    }

    #[test]
    fn static_and_brightness() {
        let mut e = setting("static", Colors::Single(RED));
        assert_eq!(color(&e, 0, 10, (3.0, 2.0), 0.0), RED);
        assert_eq!(color(&e, 0, 10, (3.0, 2.0), 9.0), RED);
        assert!(!moves(&e));
        e.brightness = 0;
        assert_eq!(color(&e, 0, 10, (3.0, 2.0), 0.0), Rgb::BLACK);
    }

    #[test]
    fn effects_move() {
        let rainbow = Colors::Gradient(quietlinux_core::settings::rainbow());
        for name in ["color-wave", "tornado", "breathing", "reactive", "matrix"] {
            let e = setting(name, rainbow.clone());
            assert!(moves(&e), "{name}");
            // some key changes color over a few seconds
            let frames: Vec<Vec<Rgb>> = (0..40u8)
                .map(|f| {
                    (0..30)
                        .map(|i| {
                            #[allow(clippy::cast_precision_loss)]
                            let pos = ((i % 15) as f32 * 1.5, (i / 15) as f32 * 3.0);
                            color(&e, i, 30, pos, f32::from(f) * 0.1)
                        })
                        .collect()
                })
                .collect();
            assert!(
                frames.windows(2).any(|w| w[0] != w[1]),
                "{name} stands still"
            );
        }
    }

    #[test]
    fn breathing_a_gradient_stays_bright() {
        let rainbow = Colors::Gradient(quietlinux_core::settings::rainbow());
        let e = setting("breathing", rainbow);
        let frames: Vec<Rgb> = (0..60u8)
            .map(|f| color(&e, 0, 1, (0.0, 0.0), f32::from(f) * 0.2))
            .collect();
        // never dark, and the colors change
        assert!(
            frames.iter().all(|c| c.r.max(c.g).max(c.b) > 200),
            "{frames:?}"
        );
        assert!(frames.windows(2).any(|w| w[0] != w[1]));
    }

    #[test]
    fn breathing_goes_dark() {
        let e = setting("breathing", Colors::Single(RED));
        assert_eq!(color(&e, 0, 1, (0.0, 0.0), 0.0), Rgb::BLACK);
        let half = period(e.speed) / 2.0;
        assert_eq!(color(&e, 0, 1, (0.0, 0.0), half), RED);
    }

    #[test]
    fn tornado_with_one_color_turns_with_black() {
        let one = setting("tornado", Colors::Single(RED));
        let two = setting("tornado", Colors::Dual(RED, Rgb::BLACK));
        for (x, y) in [(0.0, 0.0), (12.0, 1.0), (20.0, 5.0), (3.0, 6.0)] {
            for t in [0.0, 0.7, 2.1] {
                assert_eq!(color(&one, 0, 1, (x, y), t), color(&two, 0, 1, (x, y), t));
            }
        }
    }

    #[test]
    fn matrix_drops_run_in_the_direction() {
        let mut e = setting("matrix", Colors::Single(RED));
        // a key one unit further on in the same lane shows the same a moment later
        let dt = period(e.speed) / DROP_RUN;
        for (direction, step) in [
            (0, (0.0, -1.0)),
            (1, (0.0, 1.0)),
            (2, (-1.0, 0.0)),
            (3, (1.0, 0.0)),
        ] {
            e.direction = Direction(direction);
            let (x, y) = (10.5, 3.5);
            let same = (0..100u8)
                .filter(|&f| {
                    let t = f32::from(f) * 0.13;
                    color(&e, 0, 1, (x, y), t) == color(&e, 0, 1, (x + step.0, y + step.1), t + dt)
                })
                .count();
            // only where a drop starts over it differs
            assert!(same >= 90, "direction {direction}: {same} of 100");
        }
        // somewhere in the lane a drop with its tail is lit
        let lit =
            (0..100u8).any(|f| color(&e, 0, 1, (10.5, 3.5), f32::from(f) * 0.13) != Rgb::BLACK);
        assert!(lit);
    }

    #[test]
    fn wave_direction() {
        let mut e = setting("color-wave", Colors::Dual(RED, BLUE));
        e.direction = Direction(0); // up
        // what is at row 4 now is at row 3 a moment later
        let dt = period(e.speed) / WAVE_LENGTH;
        assert_eq!(
            color(&e, 0, 1, (5.0, 4.0), 0.0),
            color(&e, 0, 1, (5.0, 3.0), dt)
        );
    }
}
