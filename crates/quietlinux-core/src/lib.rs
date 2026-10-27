//! Protocol and device access for the be quiet! Light Mount keyboard (USB `373f:0002`).
//!
//! - [`protocol`]: vendor frames (interface 2), pure functions without I/O.
//! - [`hid_descriptor`]: minimal HID report descriptor parsing.
//! - [`layout`]: vendor LED indexes per zone.
//! - [`color`]: RGB colors.
//! - [`device`]: finding the keyboard in sysfs and talking to it over hidraw.
//!
//! The protocol layer is a port of
//! [bbossa/keyboard_bequiet_lightmount_rgb](https://github.com/bbossa/keyboard_bequiet_lightmount_rgb)
//! (GPL-2.0-or-later), see `NOTICE`.

pub mod color;
pub mod device;
pub mod hid_descriptor;
pub mod layout;
pub mod protocol;

pub use color::Rgb;
