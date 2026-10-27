//! USB transfers out of captured packets: Linux usbmon and Windows USBPcap.

use crate::capture::Record;

/// Linux usbmon, 48-byte header.
const LINKTYPE_USB_LINUX: u16 = 189;
/// Linux usbmon, 64-byte header (what Wireshark captures on `usbmonN`).
const LINKTYPE_USB_LINUX_MMAPPED: u16 = 220;
/// Windows USBPcap.
const LINKTYPE_USBPCAP: u16 = 249;

/// Direction of a transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Host to device.
    Out,
    /// Device to host.
    In,
}

/// The data of one transfer, from the event that carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct Transfer {
    pub time: f64,
    /// `bus.address`, e.g. `1.3`.
    pub device: String,
    pub dir: Dir,
    pub data: Vec<u8>,
}

/// The transfer data in a captured packet, or `None` (other link types, events without data).
///
/// OUT data travels with the request (submission), IN data with the completion.
pub fn transfer(record: &Record) -> Option<Transfer> {
    match record.linktype {
        LINKTYPE_USB_LINUX => usbmon(record, 48),
        LINKTYPE_USB_LINUX_MMAPPED => usbmon(record, 64),
        LINKTYPE_USBPCAP => usbpcap(record),
        _ => None,
    }
}

fn usbmon(record: &Record, header: usize) -> Option<Transfer> {
    let d = &record.data;
    let kind = *d.get(8)?;
    let endpoint = *d.get(10)?;
    let address = *d.get(11)?;
    let bus = u16::from_le_bytes(d.get(12..14)?.try_into().ok()?);
    let dir = if endpoint & 0x80 == 0 {
        Dir::Out
    } else {
        Dir::In
    };
    let carries_data = match dir {
        Dir::Out => kind == b'S',
        Dir::In => kind == b'C',
    };
    let data = d.get(header..)?;
    (carries_data && !data.is_empty()).then(|| Transfer {
        time: record.time,
        device: format!("{bus}.{address}"),
        dir,
        data: data.to_vec(),
    })
}

fn usbpcap(record: &Record) -> Option<Transfer> {
    let d = &record.data;
    let header = usize::from(u16::from_le_bytes(d.get(0..2)?.try_into().ok()?));
    let completion = d.get(16)? & 0x01 != 0;
    let bus = u16::from_le_bytes(d.get(17..19)?.try_into().ok()?);
    let address = u16::from_le_bytes(d.get(19..21)?.try_into().ok()?);
    let endpoint = *d.get(21)?;
    let len = usize::try_from(u32::from_le_bytes(d.get(23..27)?.try_into().ok()?)).ok()?;
    let dir = if endpoint & 0x80 == 0 {
        Dir::Out
    } else {
        Dir::In
    };
    let carries_data = match dir {
        Dir::Out => !completion,
        Dir::In => completion,
    };
    let data = d.get(header..header.checked_add(len)?)?;
    (carries_data && !data.is_empty()).then(|| Transfer {
        time: record.time,
        device: format!("{bus}.{address}"),
        dir,
        data: data.to_vec(),
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// usbmon (64-byte header) event: `kind` b'S' or b'C'.
    pub fn usbmon_packet(kind: u8, endpoint: u8, address: u8, data: &[u8]) -> Vec<u8> {
        let mut p = vec![0u8; 64];
        p[8] = kind;
        p[9] = 1; // interrupt
        p[10] = endpoint;
        p[11] = address;
        p[12..14].copy_from_slice(&1u16.to_le_bytes());
        p[36..40].copy_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        p.extend_from_slice(data);
        p
    }

    /// USBPcap interrupt transfer.
    pub fn usbpcap_packet(completion: bool, endpoint: u8, address: u16, data: &[u8]) -> Vec<u8> {
        let mut p = vec![0u8; 27];
        p[0..2].copy_from_slice(&27u16.to_le_bytes());
        p[16] = u8::from(completion);
        p[17..19].copy_from_slice(&1u16.to_le_bytes());
        p[19..21].copy_from_slice(&address.to_le_bytes());
        p[21] = endpoint;
        p[22] = 1; // interrupt
        p[23..27].copy_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        p.extend_from_slice(data);
        p
    }

    fn record(linktype: u16, data: Vec<u8>) -> Record {
        Record {
            time: 1.0,
            linktype,
            data,
        }
    }

    #[test]
    fn usbmon_directions() {
        let out = transfer(&record(220, usbmon_packet(b'S', 0x04, 3, b"out"))).unwrap();
        assert_eq!(
            (out.device.as_str(), out.dir, out.data.as_slice()),
            ("1.3", Dir::Out, &b"out"[..])
        );
        let inp = transfer(&record(220, usbmon_packet(b'C', 0x83, 3, b"in"))).unwrap();
        assert_eq!(inp.dir, Dir::In);
        // the completion of an OUT transfer and the submission of an IN one carry no data
        assert!(transfer(&record(220, usbmon_packet(b'C', 0x04, 3, b"x"))).is_none());
        assert!(transfer(&record(220, usbmon_packet(b'S', 0x83, 3, b"x"))).is_none());
    }

    #[test]
    fn usbpcap_directions() {
        let out = transfer(&record(249, usbpcap_packet(false, 0x04, 7, b"out"))).unwrap();
        assert_eq!((out.device.as_str(), out.dir), ("1.7", Dir::Out));
        let inp = transfer(&record(249, usbpcap_packet(true, 0x83, 7, b"in"))).unwrap();
        assert_eq!((inp.dir, inp.data.as_slice()), (Dir::In, &b"in"[..]));
        assert!(transfer(&record(249, usbpcap_packet(true, 0x04, 7, b"x"))).is_none());
    }

    #[test]
    fn garbage_is_ignored() {
        for linktype in [189, 220, 249, 1] {
            for len in [0, 5, 27, 63] {
                let _ = transfer(&record(linktype, vec![0xff; len]));
            }
        }
    }
}
