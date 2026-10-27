//! Logs written by `tools/webhid-logger.js`.
//!
//! ```text
//! # quietlinux webhid log v1
//! <ms> <out|in|feature-out|feature-in> <vid:pid> <report id> <hex data>
//! <ms> note <free text>
//! ```

use crate::decode::Event;
use crate::usb::{Dir, Transfer};

pub fn parse(text: &str) -> Result<Vec<Event>, String> {
    let mut events = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || format!("line {}: not a WebHID log line: {line:?}", n + 1);
        let mut fields = line.splitn(3, ' ');
        let time: f64 = fields.next().and_then(|t| t.parse().ok()).ok_or_else(bad)?;
        let kind = fields.next().ok_or_else(bad)?;
        let rest = fields.next().unwrap_or("");
        if kind == "note" {
            events.push(Event::Note {
                time,
                text: rest.to_owned(),
            });
            continue;
        }
        let dir = match kind {
            "out" | "feature-out" => Dir::Out,
            "in" | "feature-in" => Dir::In,
            _ => return Err(bad()),
        };
        let mut fields = rest.split(' ');
        let device = fields.next().ok_or_else(bad)?.to_owned();
        let _report_id = fields.next().ok_or_else(bad)?;
        let data = hex(fields.next().unwrap_or("")).ok_or_else(bad)?;
        events.push(Event::Transfer(Transfer {
            time,
            device,
            dir,
            data,
        }));
    }
    Ok(events)
}

fn hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_log() {
        let log = "# quietlinux webhid log v1\n\
                   0.000 note brightness 50\n\
                   12.500 out 373f:0002 0 0a0b\n\
                   13.000 in 373f:0002 0 ff\n";
        let events = parse(log).unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], Event::Note { text, .. } if text == "brightness 50"));
        let Event::Transfer(t) = &events[1] else {
            panic!()
        };
        assert_eq!(
            (t.time, t.device.as_str(), t.dir, t.data.as_slice()),
            (12.5, "373f:0002", Dir::Out, &[0x0a, 0x0b][..])
        );
    }

    #[test]
    fn bad_lines() {
        assert!(parse("abc out 373f:0002 0 00").is_err());
        assert!(parse("1.0 sideways 373f:0002 0 00").is_err());
        assert!(parse("1.0 out 373f:0002 0 0").is_err());
        assert!(parse("1.0 out 373f:0002 0 zz").is_err());
        assert!(parse("1.0 out").is_err());
    }
}
