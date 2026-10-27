//! Where each key is drawn: a schematic ISO layout in key units (1 u = 19 mm).
//!
//! The positions in `keymap` are those of the LEDs, not always the key centers,
//! so the rectangles are laid out row by row like a full-size ISO keyboard: the
//! macro keys and the knob on the left, then the main block, the navigation keys
//! and the number pad.

/// A key's rectangle in key units, from the top left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cap {
    pub code: &'static str,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The knob, drawn as a circle.
    pub round: bool,
}

/// Width and height of the whole drawing, in key units.
pub const WIDTH: f32 = 24.25;
pub const HEIGHT: f32 = 6.25;

/// Left edges of the blocks.
const MACRO_X: f32 = 0.0;
const MAIN_X: f32 = 1.25;
const NAV_X: f32 = 16.75;
const PAD_X: f32 = 20.25;
/// Width of the narrow macro keys.
const MACRO_W: f32 = 0.75;

/// Top of each row: the function key row, then five rows after a gap.
fn row_y(row: usize) -> f32 {
    if row == 0 {
        0.0
    } else {
        // rows are counted from 0; 1..=5 follow a 0.25 u gap
        #[allow(clippy::cast_precision_loss)]
        let r = row as f32;
        0.25 + r
    }
}

/// Keys of one row of a block: (code, width in u); `None` leaves a gap.
type Row = &'static [(Option<&'static str>, f32)];

const MAIN: [Row; 6] = [
    &[
        (Some("Escape"), 1.0),
        (None, 1.0),
        (Some("F1"), 1.0),
        (Some("F2"), 1.0),
        (Some("F3"), 1.0),
        (Some("F4"), 1.0),
        (None, 0.5),
        (Some("F5"), 1.0),
        (Some("F6"), 1.0),
        (Some("F7"), 1.0),
        (Some("F8"), 1.0),
        (None, 0.5),
        (Some("F9"), 1.0),
        (Some("F10"), 1.0),
        (Some("F11"), 1.0),
        (Some("F12"), 1.0),
    ],
    &[
        (Some("Backquote"), 1.0),
        (Some("Digit1"), 1.0),
        (Some("Digit2"), 1.0),
        (Some("Digit3"), 1.0),
        (Some("Digit4"), 1.0),
        (Some("Digit5"), 1.0),
        (Some("Digit6"), 1.0),
        (Some("Digit7"), 1.0),
        (Some("Digit8"), 1.0),
        (Some("Digit9"), 1.0),
        (Some("Digit0"), 1.0),
        (Some("Minus"), 1.0),
        (Some("Equal"), 1.0),
        (Some("Backspace"), 2.0),
    ],
    &[
        (Some("Tab"), 1.5),
        (Some("KeyQ"), 1.0),
        (Some("KeyW"), 1.0),
        (Some("KeyE"), 1.0),
        (Some("KeyR"), 1.0),
        (Some("KeyT"), 1.0),
        (Some("KeyY"), 1.0),
        (Some("KeyU"), 1.0),
        (Some("KeyI"), 1.0),
        (Some("KeyO"), 1.0),
        (Some("KeyP"), 1.0),
        (Some("BracketLeft"), 1.0),
        (Some("BracketRight"), 1.0),
    ],
    &[
        (Some("CapsLock"), 1.75),
        (Some("KeyA"), 1.0),
        (Some("KeyS"), 1.0),
        (Some("KeyD"), 1.0),
        (Some("KeyF"), 1.0),
        (Some("KeyG"), 1.0),
        (Some("KeyH"), 1.0),
        (Some("KeyJ"), 1.0),
        (Some("KeyK"), 1.0),
        (Some("KeyL"), 1.0),
        (Some("Semicolon"), 1.0),
        (Some("Quote"), 1.0),
        (Some("Backslash"), 1.0),
    ],
    &[
        (Some("ShiftLeft"), 1.25),
        (Some("IntlBackslash"), 1.0),
        (Some("KeyZ"), 1.0),
        (Some("KeyX"), 1.0),
        (Some("KeyC"), 1.0),
        (Some("KeyV"), 1.0),
        (Some("KeyB"), 1.0),
        (Some("KeyN"), 1.0),
        (Some("KeyM"), 1.0),
        (Some("Comma"), 1.0),
        (Some("Period"), 1.0),
        (Some("Slash"), 1.0),
        (Some("ShiftRight"), 2.75),
    ],
    &[
        (Some("ControlLeft"), 1.25),
        (Some("MetaLeft"), 1.25),
        (Some("AltLeft"), 1.25),
        (Some("Space"), 6.25),
        (Some("AltRight"), 1.25),
        (Some("MetaRight"), 1.25),
        (Some("Fn"), 1.25),
        (Some("ControlRight"), 1.25),
    ],
];

const NAV: [Row; 6] = [
    &[
        (Some("PrintScreen"), 1.0),
        (Some("ScrollLock"), 1.0),
        (Some("Pause"), 1.0),
    ],
    &[
        (Some("Insert"), 1.0),
        (Some("Home"), 1.0),
        (Some("PageUp"), 1.0),
    ],
    &[
        (Some("Delete"), 1.0),
        (Some("End"), 1.0),
        (Some("PageDown"), 1.0),
    ],
    &[],
    &[(None, 1.0), (Some("ArrowUp"), 1.0)],
    &[
        (Some("ArrowLeft"), 1.0),
        (Some("ArrowDown"), 1.0),
        (Some("ArrowRight"), 1.0),
    ],
];

const PAD: [Row; 6] = [
    &[],
    &[
        (Some("NumLock"), 1.0),
        (Some("NumpadDivide"), 1.0),
        (Some("NumpadMultiply"), 1.0),
        (Some("NumpadSubtract"), 1.0),
    ],
    &[
        (Some("Numpad7"), 1.0),
        (Some("Numpad8"), 1.0),
        (Some("Numpad9"), 1.0),
        (Some("NumpadAdd"), 1.0),
    ],
    &[
        (Some("Numpad4"), 1.0),
        (Some("Numpad5"), 1.0),
        (Some("Numpad6"), 1.0),
    ],
    &[
        (Some("Numpad1"), 1.0),
        (Some("Numpad2"), 1.0),
        (Some("Numpad3"), 1.0),
        (Some("NumpadEnter"), 1.0),
    ],
    &[(Some("Numpad0"), 2.0), (Some("NumpadDecimal"), 1.0)],
];

/// Keys two rows high.
const TALL: [&str; 2] = ["NumpadAdd", "NumpadEnter"];

fn block(out: &mut Vec<Cap>, rows: &[Row; 6], left: f32) {
    for (row, keys) in rows.iter().enumerate() {
        let mut x = left;
        for &(code, w) in *keys {
            if let Some(code) = code {
                let h = if TALL.contains(&code) { 2.0 } else { 1.0 };
                out.push(Cap {
                    code,
                    x,
                    y: row_y(row),
                    w,
                    h,
                    round: false,
                });
            }
            x += w;
        }
    }
}

/// All keys and the knob.
pub fn caps() -> Vec<Cap> {
    let mut out = vec![Cap {
        code: "Knob",
        x: MACRO_X,
        y: 0.125,
        w: MACRO_W,
        h: MACRO_W,
        round: true,
    }];
    for (i, code) in ["M1", "M2", "M3", "M4", "M5"].into_iter().enumerate() {
        out.push(Cap {
            code,
            x: MACRO_X,
            y: row_y(i + 1),
            w: MACRO_W,
            h: 1.0,
            round: false,
        });
    }
    block(&mut out, &MAIN, MAIN_X);
    // ISO Enter: right of `]` in the row of Tab, reaching into the row below
    out.push(Cap {
        code: "Enter",
        x: MAIN_X + 13.75,
        y: row_y(2),
        w: 1.25,
        h: 2.0,
        round: false,
    });
    block(&mut out, &NAV, NAV_X);
    block(&mut out, &PAD, PAD_X);
    out
}

#[cfg(test)]
mod tests {
    use quietlinux_core::keymap::KEYS;

    use super::*;

    #[test]
    fn every_key_is_drawn_once() {
        let caps = caps();
        let mut drawn: Vec<&str> = caps.iter().map(|c| c.code).collect();
        drawn.sort_unstable();
        let mut keys: Vec<&str> = KEYS.iter().map(|k| k.code).collect();
        keys.sort_unstable();
        assert_eq!(drawn, keys);
    }

    #[test]
    fn keys_stay_inside_and_do_not_overlap() {
        let caps = caps();
        for c in &caps {
            assert!(c.x >= 0.0 && c.x + c.w <= WIDTH, "{}", c.code);
            assert!(c.y >= 0.0 && c.y + c.h <= HEIGHT, "{}", c.code);
        }
        for (i, a) in caps.iter().enumerate() {
            for b in &caps[i + 1..] {
                let apart = a.x + a.w <= b.x + 1e-3
                    || b.x + b.w <= a.x + 1e-3
                    || a.y + a.h <= b.y + 1e-3
                    || b.y + b.h <= a.y + 1e-3;
                assert!(apart, "{} overlaps {}", a.code, b.code);
            }
        }
    }
}
