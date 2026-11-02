//! `qlcap diff`: what differs between two captures in which one setting was
//! changed. Times, request IDs and session bytes differ in every capture and are
//! left out; so are keep-alives and (unless asked for) key events. Messages are
//! matched like lines in `diff` (longest common subsequence); a message that
//! was removed and one with the same command added at the same place are shown
//! as one changed message, with the changed bytes marked.

use std::fmt::Write as _;

use quietlinux_core::protocol::{self as p, Command, Message};

use crate::decode::{self, Line};
use crate::usb::Dir;

/// What to compare.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Only requests that change the keyboard (no reads, replies or notifications).
    pub writes_only: bool,
    pub key_events: bool,
    /// Also the messages both captures have.
    pub all: bool,
}

/// A message without what differs from capture to capture.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    dir: Dir,
    cmd: Command,
    name: &'static str,
    status: u8,
    notification: bool,
    /// The data as hex, or a placeholder for hidden contents.
    data: String,
    /// The data bytes, if shown.
    bytes: Option<Vec<u8>>,
}

impl Item {
    fn head(&self) -> String {
        let status = match self.status {
            p::STATUS_SUCCESS => String::new(),
            s => format!(
                " status {}",
                p::status_name(s).map_or_else(|| format!("{s:#04x}"), str::to_owned)
            ),
        };
        let kind = if self.notification { " notify" } else { "" };
        format!(
            "{}  {} {}{kind}{status}",
            decode::arrow(self.dir),
            self.cmd,
            self.name
        )
    }
}

fn item(dir: Dir, message: &Message, data: Option<&[u8]>, placeholder: String) -> Item {
    Item {
        dir,
        cmd: message.cmd,
        name: message.name().unwrap_or("?"),
        status: message.status,
        notification: message.is_notification(),
        data: data.map_or(placeholder, decode::hex),
        bytes: data.map(<[u8]>::to_vec),
    }
}

/// The messages to compare, and how many broken fragments were left out.
fn items(lines: &[Line], options: Options) -> (Vec<Item>, usize) {
    let mut out = Vec::new();
    let mut broken = 0;
    for line in lines {
        let (dir, message, hidden) = match line {
            Line::Message { dir, message, .. } => (*dir, message, false),
            Line::Hidden { dir, message, .. } => (*dir, message, true),
            Line::Broken { .. } => {
                broken += 1;
                continue;
            }
            Line::Note { .. } => continue,
        };
        if message.cmd == p::KEEP_ALIVE {
            continue;
        }
        if !options.key_events && decode::is_key_event(message) {
            continue;
        }
        if options.writes_only
            && (dir == Dir::In || message.is_notification() || p::is_read_only(message.cmd))
        {
            continue;
        }
        out.push(if hidden {
            let text = format!("<macro contents hidden, {} bytes>", message.data.len());
            item(dir, message, None, text)
        } else if decode::is_serial_reply(dir, message) {
            item(dir, message, None, "<serial number redacted>".to_owned())
        } else {
            item(dir, message, Some(&message.data), String::new())
        });
    }
    (out, broken)
}

/// One step of turning A into B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit {
    Same(usize, usize),
    Removed(usize),
    Added(usize),
}

/// Largest comparison table (messages of A times messages of B).
const MAX_CELLS: usize = 40_000_000;

/// The edits from `a` to `b` (longest common subsequence).
// a, b, i, j, n, m as in the usual notation of the algorithm
#[allow(clippy::many_single_char_names)]
fn edits(a: &[Item], b: &[Item]) -> Result<Vec<Edit>, String> {
    let (n, m) = (a.len(), b.len());
    if (n + 1).saturating_mul(m + 1) > MAX_CELLS {
        return Err(format!(
            "the captures are too long to compare ({n} and {m} messages): try --writes"
        ));
    }
    // lengths of the common subsequences of the suffixes
    let width = m + 1;
    let mut table = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * width + j] = if a[i] == b[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push(Edit::Same(i, j));
            i += 1;
            j += 1;
        } else if j < m && (i == n || table[i * width + j + 1] >= table[(i + 1) * width + j]) {
            out.push(Edit::Added(j));
            j += 1;
        } else {
            out.push(Edit::Removed(i));
            i += 1;
        }
    }
    Ok(out)
}

/// The data of two messages, one above the other, with the changed bytes marked.
fn changed_bytes(a: &Item, b: &Item) -> Vec<String> {
    let mut out = vec![
        format!("      A: {}", a.data),
        format!("      B: {}", b.data),
    ];
    if let (Some(x), Some(y)) = (&a.bytes, &b.bytes) {
        let marks: String = (0..x.len().max(y.len()))
            .map(|i| if x.get(i) == y.get(i) { "   " } else { "^^ " })
            .collect();
        let count = marks.matches('^').count() / 2;
        out.push(format!("         {}", marks.trim_end()));
        out.push(format!("         {count} byte(s) differ"));
    }
    out
}

/// Compare two decoded captures; the lines to print.
#[allow(clippy::many_single_char_names)]
pub fn diff(a: &[Line], b: &[Line], options: Options) -> Result<Vec<String>, String> {
    let (a, broken_a) = items(a, options);
    let (b, broken_b) = items(b, options);
    let edits = edits(&a, &b)?;
    let mut out = Vec::new();
    let (mut changed, mut removed, mut added) = (0, 0, 0);
    let mut k = 0;
    while k < edits.len() {
        if let Edit::Same(i, _) = edits[k] {
            if options.all {
                out.push(format!("  = {}  {}", a[i].head(), a[i].data));
            }
            k += 1;
            continue;
        }
        // a run of changes: pair removed and added messages of the same command
        let start = k;
        while k < edits.len() && !matches!(edits[k], Edit::Same(..)) {
            k += 1;
        }
        let run = &edits[start..k];
        let mut gone: Vec<usize> = run
            .iter()
            .filter_map(|e| match e {
                Edit::Removed(i) => Some(*i),
                _ => None,
            })
            .collect();
        for edit in run {
            match *edit {
                Edit::Added(j) => {
                    let pair = gone
                        .iter()
                        .position(|&i| (a[i].dir, a[i].cmd) == (b[j].dir, b[j].cmd));
                    if let Some(at) = pair {
                        let i = gone.remove(at);
                        changed += 1;
                        out.push(format!("  ~ {}", b[j].head()));
                        out.extend(changed_bytes(&a[i], &b[j]));
                    } else {
                        added += 1;
                        out.push(format!("  + {}  {}", b[j].head(), b[j].data));
                    }
                }
                Edit::Removed(_) | Edit::Same(..) => {}
            }
        }
        for i in gone {
            removed += 1;
            out.push(format!("  - {}  {}", a[i].head(), a[i].data));
        }
    }
    let mut summary = format!(
        "A: {} message(s), B: {}; {changed} changed, {removed} only in A (-), {added} only in B (+)",
        a.len(),
        b.len()
    );
    if broken_a + broken_b > 0 {
        let _ = write!(
            summary,
            "; broken fragments left out: {broken_a} in A, {broken_b} in B"
        );
    }
    out.push(summary);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(dir: Dir, cmd: Command, data: &[u8], request_id: u8) -> Line {
        let packet = p::frame(cmd, data, request_id, p::SESSION).unwrap();
        Line::Message {
            time: f64::from(request_id),
            dir,
            message: p::reassemble(&[packet]).unwrap(),
            packets: vec![packet],
        }
    }

    #[test]
    fn one_setting_changed() {
        let a = [
            line(Dir::Out, p::GET_LIGHTING_MODE, &[], 1),
            line(Dir::Out, p::KEEP_ALIVE, &[], 2),
            line(Dir::Out, p::SET_LAYER_CONFIG, &[0, 1, 0, 0x64, 0x32], 3),
            line(Dir::Out, p::GET_LAYOUT, &[], 4),
        ];
        // other request IDs and times, no keep-alive, a faster speed, one more write
        let b = [
            line(Dir::Out, p::GET_LIGHTING_MODE, &[], 9),
            line(Dir::Out, p::SET_LAYER_CONFIG, &[0, 1, 0, 0x64, 0x64], 10),
            line(Dir::Out, p::SET_LIGHTING_MODE, &[1], 11),
            line(Dir::Out, p::GET_LAYOUT, &[], 12),
        ];
        let out = diff(&a, &b, Options::default()).unwrap();
        assert_eq!(out.len(), 7, "{out:#?}");
        assert!(out[0].starts_with("  ~ →  10:06"), "{}", out[0]);
        assert_eq!(out[1], "      A: 00 01 00 64 32");
        assert_eq!(out[2], "      B: 00 01 00 64 64");
        assert_eq!(out[3], "                     ^^");
        assert_eq!(out[4], "         1 byte(s) differ");
        assert_eq!(out[5], "  + →  10:02 SetLightingMode  01");
        assert!(out[6].contains("1 changed, 0 only in A (-), 1 only in B (+)"));
        // only writes: the reads are gone; with --all, the same messages too
        let writes = diff(
            &a,
            &b,
            Options {
                writes_only: true,
                ..Options::default()
            },
        )
        .unwrap();
        assert!(writes.last().unwrap().starts_with("A: 1 message(s), B: 2"));
        let all = diff(
            &a,
            &b,
            Options {
                all: true,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(all.iter().filter(|l| l.starts_with("  = ")).count(), 2);
    }

    #[test]
    fn same_captures_and_removed_messages() {
        let a = [
            line(Dir::Out, p::SET_BINDING, &[3, 0, 8, 3], 1),
            line(Dir::Out, p::CLEAR_BINDING, &[3, 0], 2),
        ];
        let out = diff(&a, &a, Options::default()).unwrap();
        assert_eq!(
            out,
            ["A: 2 message(s), B: 2; 0 changed, 0 only in A (-), 0 only in B (+)"]
        );
        let out = diff(&a, &a[..1], Options::default()).unwrap();
        assert!(out[0].starts_with("  - →  11:03"), "{out:?}");
        // a different length of data: the missing bytes are marked
        let b = [
            line(Dir::Out, p::SET_BINDING, &[3, 0, 8, 1, 7], 5),
            a[1].clone(),
        ];
        let out = diff(&a, &b, Options::default()).unwrap();
        assert_eq!(out[3], "                  ^^ ^^");
    }
}
