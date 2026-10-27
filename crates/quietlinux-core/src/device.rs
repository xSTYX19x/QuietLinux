//! Finding the Light Mount and talking to its vendor interface over hidraw.
//!
//! Based on `lightmount/device.py` of `bbossa/keyboard_bequiet_lightmount_rgb`.
//! Nothing is written unless the device is the expected one:
//!
//! 1. USB ID `373f:0002`, with the vendor interface (2, usage page `0xff00`) and the
//!    `LampArray` interface (3, usage page `0x59`) present: checked in sysfs only;
//! 2. a single keyboard, or the one chosen by its USB path;
//! 3. a known firmware version (`03:01` reply), unless writing to an unknown
//!    version is explicitly allowed.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustix::event::{PollFd, PollFlags, Timespec, poll};

use crate::Rgb;
use crate::hid_descriptor::{Usage, top_level_usage};
use crate::protocol::{self as p, Command, Led, PACKET_SIZE, Packet, ProtocolError};

#[cfg(any(test, feature = "fake"))]
pub mod fake;

pub const SYSFS: &str = "/sys";
const HID_ID: &str = "0003:0000373F:00000002";
pub const VENDOR_INTERFACE: u8 = 2;
pub const LAMPARRAY_INTERFACE: u8 = 3;
const EXPECTED_USAGES: [(u8, Usage); 2] = [
    (VENDOR_INTERFACE, Usage::new(0xff00, 0x01)),
    (LAMPARRAY_INTERFACE, Usage::new(0x59, 0x01)),
];

/// `03:01` replies of tested keyboards.
pub const KNOWN_VERSIONS: &[[u8; 8]] = &[[0x02, 0x00, 0x01, 0x01, 0x00, 0x00, 0x03, 0x02]];

pub const REPLY_TIMEOUT: Duration = Duration::from_secs(1);
/// Pause after `10:02`, which gets no reply.
const MODE_SWITCH_DELAY: Duration = Duration::from_millis(50);
/// Pause between the messages of a per-LED update.
const LED_MESSAGE_DELAY: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub enum Error {
    NotFound,
    NotFoundAt {
        wanted: String,
        present: Vec<String>,
    },
    Several(Vec<String>),
    InterfaceMissing {
        usb_path: String,
        interface: u8,
    },
    UnexpectedDescriptor {
        usb_path: String,
        interface: u8,
        actual: Option<Usage>,
        expected: Usage,
    },
    PermissionDenied(PathBuf),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Transport(io::Error),
    NoReply(Command),
    InvalidReply(Command),
    /// The keyboard answered with an error status.
    Status {
        cmd: Command,
        status: u8,
    },
    /// A request this project does not send.
    Unsupported(String),
    UnknownFirmware(Vec<u8>),
    Protocol(ProtocolError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

fn fmt_usage(usage: Option<Usage>) -> String {
    usage.map_or_else(
        || "none".to_owned(),
        |u| format!("{:#06x}/{:#04x}", u.page, u.id),
    )
}

/// Firmware version bytes as `02 00 01 …`.
pub fn fmt_version(version: &[u8]) -> String {
    version
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "Light Mount not found (USB 373f:0002)"),
            Self::NotFoundAt { wanted, present } => {
                write!(
                    f,
                    "Light Mount not found at USB path {wanted:?} (present: {})",
                    present.join(", ")
                )
            }
            Self::Several(paths) => write!(
                f,
                "several Light Mount keyboards ({}): choose one with --usb-path",
                paths.join(", ")
            ),
            Self::InterfaceMissing {
                usb_path,
                interface,
            } => {
                write!(f, "Light Mount {usb_path}: interface {interface} missing")
            }
            Self::UnexpectedDescriptor {
                usb_path,
                interface,
                actual,
                expected,
            } => write!(
                f,
                "Light Mount {usb_path}: unexpected report descriptor on interface {interface} \
                 (usage {}, expected {})",
                fmt_usage(*actual),
                fmt_usage(Some(*expected))
            ),
            Self::PermissionDenied(path) => write!(
                f,
                "{}: permission denied. Install the udev rule (see docs/installation.md), then replug the keyboard",
                path.display()
            ),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Transport(e) => write!(f, "Light Mount: {e}"),
            Self::NoReply(cmd) => write!(f, "Light Mount: no reply to {cmd}"),
            Self::InvalidReply(cmd) => {
                write!(f, "Light Mount: invalid reply to {cmd} (CRC mismatch)")
            }
            Self::Unsupported(what) => write!(f, "not supported: {what}"),
            Self::Status { cmd, status } => write!(
                f,
                "Light Mount: {cmd} failed: {}",
                p::status_name(*status)
                    .map_or_else(|| format!("status {status:#04x}"), str::to_owned)
            ),
            Self::UnknownFirmware(version) => write!(
                f,
                "Light Mount: untested firmware version {}, nothing written \
                 (--allow-unknown-firmware to force)",
                fmt_version(version)
            ),
            Self::Protocol(e) => write!(f, "protocol: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::Transport(source) => Some(source),
            Self::Protocol(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ProtocolError> for Error {
    fn from(e: ProtocolError) -> Self {
        Self::Protocol(e)
    }
}

fn io_error(path: &Path, source: io::Error) -> Error {
    if source.kind() == io::ErrorKind::PermissionDenied {
        Error::PermissionDenied(path.to_owned())
    } else {
        Error::Io {
            path: path.to_owned(),
            source,
        }
    }
}

// --- discovery (sysfs only, writes nothing) ------------------------------------

/// One HID interface of a keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interface {
    /// `/dev/hidrawN`
    pub node: PathBuf,
    /// sysfs directory of the HID device (holds `uevent` and `report_descriptor`).
    pub hid_dir: PathBuf,
}

/// Light Mount HID interfaces grouped by USB device: `{usb_path: {interface: Interface}}`.
pub fn scan(sysfs: &Path) -> BTreeMap<String, BTreeMap<u8, Interface>> {
    let mut found: BTreeMap<String, BTreeMap<u8, Interface>> = BTreeMap::new();
    let Ok(entries) = fs::read_dir(sysfs.join("class/hidraw")) else {
        return found;
    };
    for entry in entries.flatten() {
        // Unplugged while scanning, or not a USB interface: skip.
        let Some((usb_path, interface, iface)) = inspect(&entry.path()) else {
            continue;
        };
        found.entry(usb_path).or_default().insert(interface, iface);
    }
    found
}

fn inspect(node_dir: &Path) -> Option<(String, u8, Interface)> {
    let hid_dir = fs::canonicalize(node_dir.join("device")).ok()?;
    let uevent = fs::read_to_string(hid_dir.join("uevent")).ok()?;
    if !uevent.lines().any(|l| l == format!("HID_ID={HID_ID}")) {
        return None;
    }
    let interface_dir = hid_dir.parent()?;
    let number = fs::read_to_string(interface_dir.join("bInterfaceNumber")).ok()?;
    let interface = u8::from_str_radix(number.trim(), 16).ok()?;
    let usb_path = interface_dir.parent()?.file_name()?.to_str()?.to_owned();
    let node = Path::new("/dev").join(node_dir.file_name()?);
    Some((usb_path, interface, Interface { node, hid_dir }))
}

fn check_interfaces(usb_path: &str, interfaces: &BTreeMap<u8, Interface>) -> Result<()> {
    for (interface, expected) in EXPECTED_USAGES {
        let Some(iface) = interfaces.get(&interface) else {
            return Err(Error::InterfaceMissing {
                usb_path: usb_path.to_owned(),
                interface,
            });
        };
        let path = iface.hid_dir.join("report_descriptor");
        let descriptor = fs::read(&path).map_err(|e| io_error(&path, e))?;
        let actual = top_level_usage(&descriptor);
        if actual != Some(expected) {
            return Err(Error::UnexpectedDescriptor {
                usb_path: usb_path.to_owned(),
                interface,
                actual,
                expected,
            });
        }
    }
    Ok(())
}

/// A Light Mount found in sysfs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyboard {
    /// sysfs name of the USB device (port path), e.g. `1-7.2`.
    pub usb_path: String,
    /// Interface number → hidraw node.
    pub interfaces: BTreeMap<u8, Interface>,
}

impl Keyboard {
    /// hidraw node of the vendor interface.
    pub fn vendor_node(&self) -> &Path {
        &self.interfaces[&VENDOR_INTERFACE].node
    }
}

/// The keyboard to drive, checked against its report descriptors. Writes nothing.
pub fn find(sysfs: &Path, usb_path: Option<&str>) -> Result<Keyboard> {
    let mut found = scan(sysfs);
    let present: Vec<String> = found.keys().cloned().collect();
    let usb_path = match (usb_path, present.as_slice()) {
        (_, []) => return Err(Error::NotFound),
        (Some(wanted), _) if !found.contains_key(wanted) => {
            return Err(Error::NotFoundAt {
                wanted: wanted.to_owned(),
                present,
            });
        }
        (Some(wanted), _) => wanted.to_owned(),
        (None, [only]) => only.clone(),
        (None, _) => return Err(Error::Several(present)),
    };
    let interfaces = found.remove(&usb_path).unwrap_or_default();
    check_interfaces(&usb_path, &interfaces)?;
    Ok(Keyboard {
        usb_path,
        interfaces,
    })
}

// --- transport -----------------------------------------------------------------

/// Sends and receives vendor packets. Implemented by [`Hidraw`] and by test doubles.
pub trait Transport {
    fn send(&mut self, packet: &Packet) -> io::Result<()>;
    /// Next packet from the keyboard, or `None` after `timeout`.
    fn recv(&mut self, timeout: Duration) -> io::Result<Option<Packet>>;
}

/// The vendor interface opened through `/dev/hidrawN`.
#[derive(Debug)]
pub struct Hidraw {
    file: File,
    path: PathBuf,
}

impl Hidraw {
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| io_error(path, e))?;
        Ok(Self {
            file,
            path: path.to_owned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Transport for Hidraw {
    fn send(&mut self, packet: &Packet) -> io::Result<()> {
        // Report ID 0: the interface declares none.
        let mut report = [0u8; PACKET_SIZE + 1];
        report[1..].copy_from_slice(packet);
        let written = self.file.write(&report)?;
        if written == report.len() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::WriteZero,
                format!("short write: {written} bytes"),
            ))
        }
    }

    fn recv(&mut self, timeout: Duration) -> io::Result<Option<Packet>> {
        let timeout = Timespec::try_from(timeout)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let mut fds = [PollFd::new(&self.file, PollFlags::IN)];
        if poll(&mut fds, Some(&timeout))? == 0 {
            return Ok(None);
        }
        // hidraw returns one report per read; shorter reports are zero-padded.
        let mut packet = [0u8; PACKET_SIZE];
        let _ = self.file.read(&mut packet)?;
        Ok(Some(packet))
    }
}

// --- session -------------------------------------------------------------------

/// What the keyboard reports about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub version: Vec<u8>,
}

impl Identity {
    /// True for a firmware version this project was tested with.
    pub fn known(&self) -> bool {
        KNOWN_VERSIONS.iter().any(|v| v.as_slice() == self.version)
    }
}

/// Lighting mode (`10:01` / `10:02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightingMode {
    Off,
    /// The layers' effects (`10:06`), stored in the keyboard.
    General,
    /// Stored per-key layers.
    Custom,
    /// Per-LED colors (`10:0d`), volatile.
    Realtime,
    Other(u8),
}

impl From<u8> for LightingMode {
    fn from(mode: u8) -> Self {
        match mode {
            p::MODE_OFF => Self::Off,
            p::MODE_GENERAL => Self::General,
            p::MODE_CUSTOM => Self::Custom,
            p::MODE_REALTIME => Self::Realtime,
            other => Self::Other(other),
        }
    }
}

impl LightingMode {
    /// The value of `10:01` / `10:02`.
    pub const fn value(self) -> u8 {
        match self {
            Self::Off => p::MODE_OFF,
            Self::General => p::MODE_GENERAL,
            Self::Custom => p::MODE_CUSTOM,
            Self::Realtime => p::MODE_REALTIME,
            Self::Other(mode) => mode,
        }
    }
}

impl fmt::Display for LightingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => f.write_str("off"),
            Self::General => f.write_str("effect"),
            Self::Custom => f.write_str("custom"),
            Self::Realtime => f.write_str("realtime"),
            Self::Other(mode) => write!(f, "unknown ({mode:#04x})"),
        }
    }
}

/// An identified connection to the keyboard.
#[derive(Debug)]
pub struct Session<T> {
    transport: T,
    /// Request ID of the next request (never 0).
    request_id: u8,
    identity: Identity,
    allow_unknown_firmware: bool,
}

/// First request ID: changes between runs, as with IO Center. Never 0.
fn initial_request_id() -> u8 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    match secs.to_le_bytes()[0] {
        0 => 1,
        id => id,
    }
}

impl<T: Transport> Session<T> {
    /// Identify the keyboard (`03:01`), as IO Center does first. Sends only read requests.
    pub fn new(transport: T, allow_unknown_firmware: bool) -> Result<Self> {
        Self::with_request_id(transport, allow_unknown_firmware, initial_request_id())
    }

    fn with_request_id(transport: T, allow_unknown_firmware: bool, request_id: u8) -> Result<Self> {
        debug_assert_ne!(request_id, 0, "request ID 0 marks notifications");
        let mut session = Self {
            transport,
            request_id,
            identity: Identity {
                version: Vec::new(),
            },
            allow_unknown_firmware,
        };
        let id = session.next_request_id();
        let resp = session.request(&p::get_device_info(id))?;
        session.identity.version = p::device_info_of(&resp).to_vec();
        Ok(session)
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// True when writing is allowed: known firmware, or explicitly forced.
    pub fn writable(&self) -> bool {
        self.identity.known() || self.allow_unknown_firmware
    }

    fn check_writable(&self) -> Result<()> {
        if self.writable() {
            Ok(())
        } else {
            Err(Error::UnknownFirmware(self.identity.version.clone()))
        }
    }

    fn next_request_id(&mut self) -> u8 {
        let id = self.request_id;
        self.request_id = p::next_request_id(id);
        id
    }

    fn send(&mut self, packets: &[Packet]) -> Result<()> {
        for packet in packets {
            self.transport.send(packet).map_err(Error::Transport)?;
        }
        Ok(())
    }

    /// Send one packet and wait for the reply with the same request ID and command.
    /// Other packets (notifications such as key events) are skipped.
    fn request(&mut self, packet: &Packet) -> Result<Packet> {
        let cmd = p::command_of(packet);
        self.send(std::slice::from_ref(packet))?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::NoReply(cmd));
            }
            let resp = self.transport.recv(left).map_err(Error::Transport)?;
            match resp {
                None => return Err(Error::NoReply(cmd)),
                Some(resp) if p::is_reply(&resp, packet) => {
                    if !p::crc_ok(&resp) {
                        return Err(Error::InvalidReply(cmd));
                    }
                    return match p::status_of(&resp) {
                        p::STATUS_SUCCESS => Ok(resp),
                        status => Err(Error::Status { cmd, status }),
                    };
                }
                Some(_) => {}
            }
        }
    }

    /// Current lighting mode (`10:01`), read like IO Center does before any change.
    pub fn lighting_mode(&mut self) -> Result<LightingMode> {
        let id = self.next_request_id();
        let resp = self.request(&p::get_lighting_mode(id, p::SESSION))?;
        p::lighting_mode_of(&resp)
            .map(LightingMode::from)
            .ok_or(Error::InvalidReply(p::GET_LIGHTING_MODE))
    }

    /// Switch the lighting mode (`10:02`). Values 1 (general) and 3 (realtime) are
    /// captured from IO Center; 0 (off) and 2 (custom) come from IO Center Web.
    pub fn set_lighting_mode(&mut self, mode: LightingMode) -> Result<()> {
        self.check_writable()?;
        if let LightingMode::Other(value) = mode {
            return Err(Error::Unsupported(format!("lighting mode {value:#04x}")));
        }
        self.send_lighting_mode(mode.value())
    }

    fn send_lighting_mode(&mut self, mode: u8) -> Result<()> {
        let id = self.next_request_id();
        self.send(&[p::set_lighting_mode(mode, id, p::SESSION)])?;
        sleep(MODE_SWITCH_DELAY);
        Ok(())
    }

    /// Per-LED colors (`10:0d`, volatile), switching to realtime mode if needed.
    pub fn set_leds(&mut self, leds: &[Led]) -> Result<()> {
        self.check_writable()?;
        // Switching to realtime mode resets every LED: only do it when needed.
        if self.lighting_mode()? != LightingMode::Realtime {
            self.send_lighting_mode(p::MODE_REALTIME)?;
        }
        for message in p::update(leds, self.request_id, p::SESSION) {
            self.next_request_id();
            self.send(&message)?;
            sleep(LED_MESSAGE_DELAY);
        }
        Ok(())
    }

    /// Switch to general mode: the effect stored in the keyboard shows again.
    pub fn show_stored_effect(&mut self) -> Result<()> {
        self.set_lighting_mode(LightingMode::General)
    }

    /// Static color effect (`10:06`, stored in the keyboard), after switching to general mode.
    pub fn set_static(&mut self, color: Rgb, brightness: u8) -> Result<()> {
        self.check_writable()?;
        self.send_lighting_mode(p::MODE_GENERAL)?;
        let id = self.next_request_id();
        self.request(&p::static_color(color, brightness, id, p::SESSION))?;
        Ok(())
    }
}

/// Find the keyboard and open an identified session on its vendor interface.
pub fn open(
    usb_path: Option<&str>,
    allow_unknown_firmware: bool,
) -> Result<(Keyboard, Session<Hidraw>)> {
    let keyboard = find(Path::new(SYSFS), usb_path)?;
    let hidraw = Hidraw::open(keyboard.vendor_node())?;
    let session = Session::new(hidraw, allow_unknown_firmware)?;
    Ok((keyboard, session))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::fake::FakeKeyboard;
    use super::*;

    const KNOWN: [u8; 8] = KNOWN_VERSIONS[0];

    #[test]
    fn identify_and_read_mode() {
        let mut kb = FakeKeyboard::new(&KNOWN, p::MODE_GENERAL);
        let mut session = Session::with_request_id(&mut kb, false, 0xfe).unwrap();
        assert!(session.identity().known());
        assert_eq!(session.lighting_mode().unwrap(), LightingMode::General);
        assert_eq!(kb.commands(), ["03:01", "10:01"]);
        assert_eq!(kb.received[0][2], p::SESSION_NONE);
        assert_eq!(p::request_id_of(&kb.received[1]), 0xff);
    }

    #[test]
    fn key_events_are_skipped_while_waiting() {
        let mut kb = FakeKeyboard::new(&KNOWN, p::MODE_REALTIME);
        kb.pending.push_back(
            p::frame(
                p::ACTION_FIRED,
                &[0x5c, 0x00, 0x01, 0x01, 0x00, 0x4f],
                0,
                p::SESSION,
            )
            .unwrap(),
        );
        assert!(Session::with_request_id(&mut kb, false, 1).is_ok());
    }

    #[test]
    fn no_reply() {
        struct Silent;
        impl Transport for Silent {
            fn send(&mut self, _: &Packet) -> io::Result<()> {
                Ok(())
            }
            fn recv(&mut self, _: Duration) -> io::Result<Option<Packet>> {
                Ok(None)
            }
        }
        assert!(matches!(
            Session::new(Silent, false),
            Err(Error::NoReply(p::GET_DEVICE_INFO))
        ));
    }

    #[test]
    fn corrupted_reply() {
        struct Corrupt;
        impl Transport for Corrupt {
            fn send(&mut self, _: &Packet) -> io::Result<()> {
                Ok(())
            }
            fn recv(&mut self, _: Duration) -> io::Result<Option<Packet>> {
                let mut resp = p::frame(p::GET_DEVICE_INFO, &KNOWN, 1, p::SESSION_NONE).unwrap();
                resp[10] ^= 1;
                Ok(Some(resp))
            }
        }
        assert!(matches!(
            Session::with_request_id(Corrupt, false, 1),
            Err(Error::InvalidReply(p::GET_DEVICE_INFO))
        ));
    }

    #[test]
    fn error_status_reply() {
        #[derive(Debug)]
        struct Busy;
        impl Transport for Busy {
            fn send(&mut self, _: &Packet) -> io::Result<()> {
                Ok(())
            }
            fn recv(&mut self, _: Duration) -> io::Result<Option<Packet>> {
                let mut resp = p::frame(p::GET_DEVICE_INFO, &[], 1, p::SESSION_NONE).unwrap();
                resp[3] = 0x07; // BUSY
                let crc = p::crc16_modbus(&resp[..62]).to_le_bytes();
                resp[62..].copy_from_slice(&crc);
                Ok(Some(resp))
            }
        }
        let err = Session::with_request_id(Busy, false, 1).unwrap_err();
        assert!(matches!(err, Error::Status { status: 0x07, .. }));
        assert_eq!(err.to_string(), "Light Mount: 03:01 failed: BUSY");
    }

    #[test]
    fn request_ids_skip_zero() {
        let mut kb = FakeKeyboard::new(&KNOWN, p::MODE_GENERAL);
        let mut session = Session::with_request_id(&mut kb, false, 0xff).unwrap();
        session.lighting_mode().unwrap();
        let ids: Vec<u8> = kb.received.iter().map(p::request_id_of).collect();
        assert_eq!(ids, [0xff, 0x01]);
    }

    #[test]
    fn set_lighting_mode() {
        let mut kb = FakeKeyboard::new(&KNOWN, p::MODE_GENERAL);
        let mut session = Session::with_request_id(&mut kb, false, 1).unwrap();
        session.set_lighting_mode(LightingMode::Off).unwrap();
        assert!(matches!(
            session.set_lighting_mode(LightingMode::Other(9)),
            Err(Error::Unsupported(_))
        ));
        assert_eq!(kb.mode, p::MODE_OFF);
        assert_eq!(kb.commands(), ["03:01", "10:02"]);
    }

    #[test]
    fn unknown_firmware_is_read_only() {
        let mut kb = FakeKeyboard::new(&[9, 9, 9, 9, 9, 9, 9, 9], p::MODE_GENERAL);
        let mut session = Session::with_request_id(&mut kb, false, 1).unwrap();
        assert!(!session.writable());
        assert!(matches!(
            session.set_static(Rgb::new(1, 2, 3), 100),
            Err(Error::UnknownFirmware(_))
        ));
        assert!(matches!(
            session.set_leds(&[]),
            Err(Error::UnknownFirmware(_))
        ));
        assert_eq!(kb.commands(), ["03:01"]);
    }

    #[test]
    fn unknown_firmware_forced() {
        let mut kb = FakeKeyboard::new(&[9; 8], p::MODE_GENERAL);
        let mut session = Session::with_request_id(&mut kb, true, 1).unwrap();
        session.set_static(Rgb::new(1, 2, 3), 100).unwrap();
        assert_eq!(kb.commands(), ["03:01", "10:02", "10:06"]);
    }

    #[test]
    fn set_static() {
        let mut kb = FakeKeyboard::new(&KNOWN, p::MODE_REALTIME);
        let mut session = Session::with_request_id(&mut kb, false, 1).unwrap();
        session.set_static(Rgb::new(0xe6, 0x30, 0x00), 50).unwrap();
        assert_eq!(kb.commands(), ["03:01", "10:02", "10:06"]);
        assert_eq!(kb.mode, p::MODE_GENERAL);
        assert_eq!(
            kb.received[2],
            p::static_color(Rgb::new(0xe6, 0x30, 0x00), 50, 3, p::SESSION)
        );
    }

    #[test]
    fn set_leds_switches_to_custom_only_when_needed() {
        let leds: Vec<Led> = crate::layout::all_leds()
            .map(|i| Led::new(i, Rgb::new(1, 2, 3)))
            .collect();

        let mut kb = FakeKeyboard::new(&KNOWN, p::MODE_GENERAL);
        let mut session = Session::with_request_id(&mut kb, false, 1).unwrap();
        session.set_leds(&leds).unwrap();
        assert_eq!(
            kb.commands(),
            [
                "03:01", "10:01", "10:02", "10:0d", "10:0d", "10:0d", "10:0d", "10:0d"
            ]
        );

        let mut kb = FakeKeyboard::new(&KNOWN, p::MODE_REALTIME);
        let mut session = Session::with_request_id(&mut kb, false, 1).unwrap();
        session.set_leds(&leds).unwrap();
        assert_eq!(
            kb.commands(),
            [
                "03:01", "10:01", "10:0d", "10:0d", "10:0d", "10:0d", "10:0d"
            ]
        );
        // seq keeps counting across the messages of the update
        let seqs: Vec<u8> = kb
            .received
            .iter()
            .filter(|r| p::fragment_index(r) == 0)
            .map(p::request_id_of)
            .collect();
        assert_eq!(seqs, [1, 2, 3, 4, 5, 6, 7]);
    }

    // --- fake sysfs ------------------------------------------------------------

    const DESCRIPTORS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/descriptors");

    /// Tree shaped like /sys: class/hidraw/hidrawN/device -> devices/.../<usb>/<usb>:1.<i>/<hid instance>.
    struct FakeSysfs {
        root: PathBuf,
        count: usize,
    }

    impl FakeSysfs {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "quietlinux-sysfs-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("class/hidraw")).unwrap();
            Self { root, count: 0 }
        }

        fn add(&mut self, usb_path: &str, interfaces: &[u8], hid_id: &str) {
            for &i in interfaces {
                let hid_dir = self.root.join(format!(
                    "devices/usb1/{usb_path}/{usb_path}:1.{i}/0003:373F:0002.{:04X}",
                    self.count
                ));
                fs::create_dir_all(&hid_dir).unwrap();
                fs::write(hid_dir.join("../bInterfaceNumber"), format!("{i:02x}\n")).unwrap();
                fs::write(
                    hid_dir.join("uevent"),
                    format!("DRIVER=hid-generic\nHID_ID={hid_id}\n"),
                )
                .unwrap();
                fs::copy(
                    format!("{DESCRIPTORS}/if{i}.bin"),
                    hid_dir.join("report_descriptor"),
                )
                .unwrap();
                let node = self.root.join(format!("class/hidraw/hidraw{}", self.count));
                fs::create_dir_all(&node).unwrap();
                std::os::unix::fs::symlink(&hid_dir, node.join("device")).unwrap();
                self.count += 1;
            }
        }
    }

    impl Drop for FakeSysfs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn find_single_keyboard() {
        let mut sysfs = FakeSysfs::new();
        sysfs.add("1-2", &[0, 1, 2, 3], HID_ID);
        sysfs.add("1-3", &[0], "0003:0000046D:0000C548"); // another device
        let kb = find(&sysfs.root, None).unwrap();
        assert_eq!(kb.usb_path, "1-2");
        assert_eq!(kb.interfaces.len(), 4);
        assert_eq!(kb.vendor_node(), Path::new("/dev/hidraw2"));
    }

    #[test]
    fn find_errors() {
        let sysfs = FakeSysfs::new();
        assert!(matches!(find(&sysfs.root, None), Err(Error::NotFound)));

        let mut sysfs = FakeSysfs::new();
        sysfs.add("1-2", &[0, 1, 2, 3], HID_ID);
        sysfs.add("1-4", &[0, 1, 2, 3], HID_ID);
        assert!(matches!(find(&sysfs.root, None), Err(Error::Several(p)) if p == ["1-2", "1-4"]));
        assert_eq!(find(&sysfs.root, Some("1-4")).unwrap().usb_path, "1-4");
        assert!(matches!(
            find(&sysfs.root, Some("1-9")),
            Err(Error::NotFoundAt { .. })
        ));

        let mut sysfs = FakeSysfs::new();
        sysfs.add("1-2", &[0, 1, 2], HID_ID);
        assert!(matches!(
            find(&sysfs.root, None),
            Err(Error::InterfaceMissing { interface: 3, .. })
        ));
    }

    #[test]
    fn find_rejects_unexpected_descriptor() {
        let mut sysfs = FakeSysfs::new();
        sysfs.add("1-2", &[0, 1, 2, 3], HID_ID);
        // interface 2 now looks like a boot keyboard
        let hid_dir = fs::canonicalize(sysfs.root.join("class/hidraw/hidraw2/device")).unwrap();
        fs::copy(
            format!("{DESCRIPTORS}/if0.bin"),
            hid_dir.join("report_descriptor"),
        )
        .unwrap();
        assert!(matches!(
            find(&sysfs.root, None),
            Err(Error::UnexpectedDescriptor {
                interface: 2,
                actual: Some(Usage { page: 1, id: 6 }),
                ..
            })
        ));
    }
}
