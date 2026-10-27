//! Protocol and device access for the be quiet! Light Mount keyboard (USB `373f:0002`).
//!
//! - [`protocol`]: vendor frames (interface 2), pure functions without I/O.
//! - [`hid_descriptor`]: minimal HID report descriptor parsing.
//! - [`info`]: decoding of read-only replies and key events.
//! - [`layout`]: vendor LED indexes per zone.
//! - [`keymap`]: physical keys (W3C names, LED index, position) and keycap legends.
//! - [`color`]: RGB colors.
//! - [`device`]: finding the keyboard in sysfs and talking to it over hidraw.
//! - [`settings`]: names of effects, directions, actions, … for command lines and files.
//! - [`macros`]: macros as text, and changing the macros stored in the keyboard.
//! - [`config`]: the keyboard's settings as a TOML file (backup and apply).
//!
//! The protocol layer is a port of
//! [bbossa/keyboard_bequiet_lightmount_rgb](https://github.com/bbossa/keyboard_bequiet_lightmount_rgb)
//! (GPL-2.0-or-later), see `NOTICE`.

pub mod color;
pub mod config;
pub mod device;
pub mod hid_descriptor;
pub mod info;
pub mod keymap;
pub mod layout;
pub mod macros;
pub mod protocol;
pub mod settings;

pub use color::Rgb;
