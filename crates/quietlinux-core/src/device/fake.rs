//! A simulated Light Mount for tests (and later for a GUI demo mode).
//!
//! Enabled in tests and with the `fake` feature. It answers like the real keyboard,
//! as far as its protocol is known, and records everything it receives.

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::time::Duration;

use super::{KNOWN_VERSIONS, Transport};
use crate::Rgb;
use crate::protocol::{self as p, Packet};

/// Answers like the Light Mount, from what is known of its protocol.
#[derive(Debug, Clone)]
pub struct FakeKeyboard {
    /// `03:01` reply data.
    pub version: Vec<u8>,
    /// Lighting mode (`10:01` / `10:02`).
    pub mode: u8,
    /// Current per-LED colors (`10:0d`); cleared when switching to custom mode.
    pub leds: BTreeMap<u8, Rgb>,
    /// Every packet received, in order.
    pub received: Vec<Packet>,
    /// Packets waiting to be read by the host.
    pub pending: VecDeque<Packet>,
    fragments: Vec<Packet>,
}

impl FakeKeyboard {
    pub fn new(version: &[u8], mode: u8) -> Self {
        Self {
            version: version.to_vec(),
            mode,
            leds: BTreeMap::new(),
            received: Vec::new(),
            pending: VecDeque::new(),
            fragments: Vec::new(),
        }
    }

    /// A keyboard with the tested firmware version, in effects mode.
    pub fn tested() -> Self {
        Self::new(&KNOWN_VERSIONS[0], p::MODE_GENERAL)
    }

    /// Commands of the messages received (first fragments only), like `"10:0d"`.
    pub fn commands(&self) -> Vec<String> {
        self.received
            .iter()
            .filter(|r| p::fragment_index(r) == 0)
            .map(|r| p::command_of(r).to_string())
            .collect()
    }

    fn reply(&mut self, request: &Packet, data: &[u8]) {
        let resp = p::frame(
            p::command_of(request),
            data,
            p::request_id_of(request),
            request[2],
        )
        .expect("short reply");
        self.pending.push_back(resp);
    }

    fn handle(&mut self, packet: &Packet) {
        self.received.push(*packet);
        self.fragments.push(*packet);
        if p::has_more(packet) {
            return;
        }
        let fragments = std::mem::take(&mut self.fragments);
        let Some(msg) = p::reassemble(&fragments) else {
            return;
        };
        match msg.cmd {
            p::GET_DEVICE_INFO => {
                let version = self.version.clone();
                self.reply(&fragments[0], &version);
            }
            p::GET_LIGHTING_MODE => {
                let mode = self.mode;
                self.reply(&fragments[0], &[mode]);
            }
            p::SET_LIGHTING_MODE => {
                self.mode = msg.data.first().copied().unwrap_or_default();
                if self.mode == p::MODE_REALTIME {
                    self.leds.clear();
                }
            }
            p::SET_LAYER_CONFIG => self.reply(&fragments[0], &[]),
            p::PERFORM_REALTIME_UPDATES => {
                if let Some(update) = p::parse_leds(&fragments) {
                    self.leds
                        .extend(update.entries.iter().map(|l| (l.index, l.color)));
                }
            }
            _ => {}
        }
    }
}

impl Transport for &mut FakeKeyboard {
    fn send(&mut self, packet: &Packet) -> io::Result<()> {
        self.handle(packet);
        Ok(())
    }

    fn recv(&mut self, _timeout: Duration) -> io::Result<Option<Packet>> {
        Ok(self.pending.pop_front())
    }
}
