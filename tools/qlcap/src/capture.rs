//! Reading pcap and pcapng files: only what USB captures need.
//!
//! Formats: <https://www.ietf.org/archive/id/draft-ietf-opsawg-pcap-04.html> (pcap)
//! and <https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html> (pcapng).

/// One captured packet.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// Capture time in milliseconds.
    pub time: f64,
    /// Link-layer header type of the interface.
    pub linktype: u16,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Pcap,
    Pcapng,
    /// Anything else: read as a WebHID log.
    Text,
}

const PCAP_MAGIC_US: u32 = 0xa1b2_c3d4;
const PCAP_MAGIC_NS: u32 = 0xa1b2_3c4d;
const PCAPNG_SHB: u32 = 0x0a0d_0d0a;
const PCAPNG_BYTE_ORDER: u32 = 0x1a2b_3c4d;
const PCAPNG_IDB: u32 = 1;
const PCAPNG_SPB: u32 = 3;
const PCAPNG_EPB: u32 = 6;
const OPT_END: u16 = 0;
const OPT_IF_TSRESOL: u16 = 9;

pub fn detect(bytes: &[u8]) -> Format {
    let Some(magic) = bytes.get(..4) else {
        return Format::Text;
    };
    let le = u32::from_le_bytes(magic.try_into().unwrap());
    let be = u32::from_be_bytes(magic.try_into().unwrap());
    if le == PCAPNG_SHB {
        Format::Pcapng
    } else if [le, be]
        .iter()
        .any(|&m| m == PCAP_MAGIC_US || m == PCAP_MAGIC_NS)
    {
        Format::Pcap
    } else {
        Format::Text
    }
}

/// Bounds-checked reads in a given byte order.
#[derive(Debug, Clone, Copy)]
struct Bytes<'a> {
    buf: &'a [u8],
    big_endian: bool,
}

impl Bytes<'_> {
    fn u16(&self, at: usize) -> Option<u16> {
        let b: [u8; 2] = self.buf.get(at..at + 2)?.try_into().ok()?;
        Some(if self.big_endian {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    }

    fn u32(&self, at: usize) -> Option<u32> {
        let b: [u8; 4] = self.buf.get(at..at + 4)?.try_into().ok()?;
        Some(if self.big_endian {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    }

    fn usize(&self, at: usize) -> Option<usize> {
        usize::try_from(self.u32(at)?).ok()
    }
}

fn truncated() -> String {
    "truncated or malformed capture file".to_owned()
}

pub fn read_pcap(buf: &[u8]) -> Result<Vec<Record>, String> {
    let magic = Bytes {
        buf,
        big_endian: false,
    }
    .u32(0)
    .ok_or_else(truncated)?;
    let big_endian = magic != PCAP_MAGIC_US && magic != PCAP_MAGIC_NS;
    let r = Bytes { buf, big_endian };
    let nanos = r.u32(0) == Some(PCAP_MAGIC_NS);
    let linktype = u16::try_from(r.u32(20).ok_or_else(truncated)? & 0xffff).unwrap_or_default();
    let mut records = Vec::new();
    let mut at = 24;
    while at < buf.len() {
        let secs = f64::from(r.u32(at).ok_or_else(truncated)?);
        let frac = f64::from(r.u32(at + 4).ok_or_else(truncated)?);
        let len = r.usize(at + 8).ok_or_else(truncated)?;
        let data = buf.get(at + 16..at + 16 + len).ok_or_else(truncated)?;
        let time = secs * 1e3 + if nanos { frac / 1e6 } else { frac / 1e3 };
        records.push(Record {
            time,
            linktype,
            data: data.to_vec(),
        });
        at += 16 + len;
    }
    Ok(records)
}

#[derive(Debug, Clone, Copy)]
struct Interface {
    linktype: u16,
    /// Seconds per timestamp unit.
    resolution: f64,
}

pub fn read_pcapng(buf: &[u8]) -> Result<Vec<Record>, String> {
    let mut records = Vec::new();
    let mut interfaces: Vec<Interface> = Vec::new();
    let mut r = Bytes {
        buf,
        big_endian: false,
    };
    let mut at = 0;
    while at + 12 <= buf.len() {
        if r.u32(at) == Some(PCAPNG_SHB) {
            // A new section: byte order and interfaces start over.
            let order = Bytes {
                buf,
                big_endian: false,
            }
            .u32(at + 8)
            .ok_or_else(truncated)?;
            r.big_endian = order != PCAPNG_BYTE_ORDER;
            interfaces.clear();
        }
        let kind = r.u32(at).ok_or_else(truncated)?;
        let len = r.usize(at + 4).ok_or_else(truncated)?;
        if len < 12 || len % 4 != 0 || at + len > buf.len() {
            return Err(truncated());
        }
        let body = at + 8..at + len - 4;
        match kind {
            PCAPNG_IDB => {
                let linktype = r.u16(body.start).ok_or_else(truncated)?;
                let resolution = tsresol(&r, body.start + 8, body.end).unwrap_or(1e-6);
                interfaces.push(Interface {
                    linktype,
                    resolution,
                });
            }
            PCAPNG_EPB => {
                let iface = r.usize(body.start).ok_or_else(truncated)?;
                let high = u64::from(r.u32(body.start + 4).ok_or_else(truncated)?);
                let low = u64::from(r.u32(body.start + 8).ok_or_else(truncated)?);
                let caplen = r.usize(body.start + 12).ok_or_else(truncated)?;
                let data = buf
                    .get(body.start + 20..body.start + 20 + caplen)
                    .ok_or_else(truncated)?;
                let interface = interfaces.get(iface).ok_or_else(truncated)?;
                // Precision loss only beyond 2^53 timestamp units: irrelevant here.
                #[allow(clippy::cast_precision_loss)]
                let time = ((high << 32) | low) as f64 * interface.resolution * 1e3;
                records.push(Record {
                    time,
                    linktype: interface.linktype,
                    data: data.to_vec(),
                });
            }
            PCAPNG_SPB => {
                let interface = interfaces.first().ok_or_else(truncated)?;
                let data = buf.get(body.start + 4..body.end).ok_or_else(truncated)?;
                records.push(Record {
                    time: 0.0,
                    linktype: interface.linktype,
                    data: data.to_vec(),
                });
            }
            _ => {}
        }
        at += len;
    }
    Ok(records)
}

/// `if_tsresol` option of an interface description block, in seconds per unit.
fn tsresol(r: &Bytes<'_>, mut at: usize, end: usize) -> Option<f64> {
    while at + 4 <= end {
        let code = r.u16(at)?;
        let len = usize::from(r.u16(at + 2)?);
        if code == OPT_END {
            break;
        }
        if code == OPT_IF_TSRESOL && len >= 1 {
            let value = *r.buf.get(at + 4)?;
            let exp = i32::from(value & 0x7f);
            return Some(if value & 0x80 == 0 {
                10f64.powi(-exp)
            } else {
                2f64.powi(-exp)
            });
        }
        at += 4 + len.div_ceil(4) * 4;
    }
    None
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A little-endian pcapng file with one interface and the given packets.
    pub fn pcapng(linktype: u16, packets: &[(u64, &[u8])]) -> Vec<u8> {
        fn block(out: &mut Vec<u8>, kind: u32, body: &[u8]) {
            let len = u32::try_from(12 + body.len().div_ceil(4) * 4).unwrap();
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(body);
            out.resize(out.len() + (4 - body.len() % 4) % 4, 0);
            out.extend_from_slice(&len.to_le_bytes());
        }
        let mut out = Vec::new();
        let mut shb = PCAPNG_BYTE_ORDER.to_le_bytes().to_vec();
        shb.extend_from_slice(&[1, 0, 0, 0]); // version 1.0
        shb.extend_from_slice(&u64::MAX.to_le_bytes()); // section length unknown
        block(&mut out, PCAPNG_SHB, &shb);
        let mut idb = linktype.to_le_bytes().to_vec();
        idb.extend_from_slice(&[0, 0, 0, 0, 1, 0]); // reserved, snaplen 65536
        idb.extend_from_slice(&OPT_IF_TSRESOL.to_le_bytes());
        idb.extend_from_slice(&[1, 0, 9, 0, 0, 0]); // length 1: nanoseconds
        idb.extend_from_slice(&[0, 0, 0, 0]);
        block(&mut out, PCAPNG_IDB, &idb);
        for (ts, data) in packets {
            let mut epb = 0u32.to_le_bytes().to_vec();
            epb.extend_from_slice(&u32::try_from(ts >> 32).unwrap().to_le_bytes());
            epb.extend_from_slice(&u32::try_from(ts & 0xffff_ffff).unwrap().to_le_bytes());
            let len = u32::try_from(data.len()).unwrap().to_le_bytes();
            epb.extend_from_slice(&len);
            epb.extend_from_slice(&len);
            epb.extend_from_slice(data);
            block(&mut out, PCAPNG_EPB, &epb);
        }
        out
    }

    #[test]
    fn pcapng_round_trip() {
        let file = pcapng(220, &[(1_000_000, b"abc"), (3_500_000, b"defgh")]);
        assert_eq!(detect(&file), Format::Pcapng);
        let records = read_pcapng(&file).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].linktype, 220);
        assert_eq!(records[1].data, b"defgh");
        assert!((records[1].time - 3.5).abs() < 1e-9, "{}", records[1].time);
    }

    #[test]
    fn pcap() {
        let mut file = PCAP_MAGIC_US.to_le_bytes().to_vec();
        file.extend_from_slice(&[2, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]);
        file.extend_from_slice(&249u32.to_le_bytes());
        for v in [2u32, 500, 2, 2] {
            file.extend_from_slice(&v.to_le_bytes());
        }
        file.extend_from_slice(b"hi");
        assert_eq!(detect(&file), Format::Pcap);
        let records = read_pcap(&file).unwrap();
        assert_eq!(
            records,
            [Record {
                time: 2000.5,
                linktype: 249,
                data: b"hi".to_vec()
            }]
        );
    }

    #[test]
    fn malformed_files_are_errors_not_panics() {
        let file = pcapng(220, &[(0, b"abc")]);
        for cut in 0..file.len() {
            let _ = read_pcapng(&file[..cut]);
        }
        assert!(read_pcapng(&file[..file.len() - 2]).is_err());
        assert_eq!(detect(b"12.5 out 373f:0002 0 00"), Format::Text);
    }
}
