//! `quietlinux info` and `quietlinux events`: what the keyboard reports, read-only.

use std::fmt::Write as _;
use std::time::Duration;

use quietlinux_core::device::{Error, Session, Transport};
use quietlinux_core::info::{
    self, Calibration, DeviceInfo, KeyEvent, LayerConfig, Layout, direction_name, effect_name,
    feature_name, game_mode_blocks,
};
use quietlinux_core::keymap;
use quietlinux_core::protocol::{self as p, Message};

/// Most `11:01` requests to read all bindings (each returns as many as fit).
const MAX_BINDING_REQUESTS: usize = 32;

fn hex(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn push(out: &mut Vec<String>, label: &str, value: impl Into<String>) {
    let value: String = value.into();
    out.push(format!("{label:<15} {value}"));
}

fn unavailable(e: &Error) -> String {
    format!("unavailable ({e})")
}

fn decoded(what: Option<String>, data: &[u8]) -> String {
    what.unwrap_or_else(|| format!("cannot decode: {}", hex(data)))
}

/// One line per finding; requests the keyboard refuses become "unavailable" lines.
pub fn info_lines<T: Transport>(session: &mut Session<T>) -> Vec<String> {
    let mut out = Vec::new();
    // Like IO Center: some requests answer INVALID_STATE without a session.
    match session.open_session() {
        Ok(s) => push(
            &mut out,
            "Session:",
            format!("{} (timeout {})", s.session, s.timeout),
        ),
        Err(e) => push(&mut out, "Session:", unavailable(&e)),
    }
    device_lines(session, &mut out);
    lighting_lines(session, &mut out);
    keyboard_lines(session, &mut out);
    out
}

/// Model, firmware, protocol version, features, layout, polling rate.
fn device_lines<T: Transport>(session: &mut Session<T>, out: &mut Vec<String>) {
    let identity = session.identity().clone();
    match identity.info() {
        Some(DeviceInfo {
            model_id,
            revision,
            firmware,
        }) => {
            let name = DeviceInfo::parse(&identity.version).and_then(|i| i.model_name());
            push(
                out,
                "Model:",
                format!(
                    "{} ({model_id:#06x}), revision {revision}",
                    name.unwrap_or("unknown model")
                ),
            );
            let versions: Vec<String> = firmware.iter().map(ToString::to_string).collect();
            let tested = if identity.known() {
                "tested"
            } else {
                "untested, read-only"
            };
            push(
                out,
                "Firmware:",
                format!("{} ({tested})", versions.join(", ")),
            );
        }
        None => push(
            out,
            "Device info:",
            format!("cannot decode: {}", hex(&identity.version)),
        ),
    }

    match session.query(p::GET_QLINK_VERSION, &[]) {
        Ok(m) => push(
            out,
            "QLink:",
            decoded(info::parse_qlink_version(&m.data), &m.data),
        ),
        Err(e) => push(out, "QLink:", unavailable(&e)),
    }
    match session.query(p::GET_SUPPORTED_FEATURES, &[]) {
        Ok(m) => {
            let features = info::parse_features(&m.data).map(|list| {
                list.iter()
                    .map(|&(id, version)| {
                        let name =
                            feature_name(id).map_or_else(|| format!("{id:#04x}"), str::to_owned);
                        format!("{name} v{version}")
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            push(out, "Features:", decoded(features, &m.data));
        }
        Err(e) => push(out, "Features:", unavailable(&e)),
    }
    match session.query(p::GET_LAYOUT, &[]) {
        Ok(m) => {
            let layout = Layout::parse(&m.data).map(|l| {
                format!(
                    "{}, {}",
                    l.physical_name().unwrap_or("unknown"),
                    l.visual_name().unwrap_or("unknown")
                )
            });
            push(out, "Layout:", decoded(layout, &m.data));
        }
        Err(e) => push(out, "Layout:", unavailable(&e)),
    }
    match session.query(p::GET_POLLING_RATE, &[]) {
        Ok(m) => {
            let rate = m
                .data
                .get(..2)
                .map(|b| format!("{} Hz", u16::from_le_bytes([b[0], b[1]])));
            push(out, "Polling rate:", decoded(rate, &m.data));
        }
        Err(e) => push(out, "Polling rate:", unavailable(&e)),
    }
}

fn describe_layer(c: &LayerConfig) -> String {
    let mut text = format!(
        "{}, colors {}, brightness {}, speed {}, direction {}",
        effect_name(c.effect).map_or_else(|| format!("effect {}", c.effect), str::to_owned),
        c.colors,
        c.brightness,
        c.speed,
        direction_name(c.direction).unwrap_or("?"),
    );
    if let Some([width, range, fade]) = c.ripple {
        let _ = write!(text, ", ripple width {width} range {range} fade-out {fade}");
    }
    text
}

/// Lighting mode, layers, color calibration.
fn lighting_lines<T: Transport>(session: &mut Session<T>, out: &mut Vec<String>) {
    match session.lighting_mode() {
        Ok(mode) => push(out, "Lighting mode:", mode.to_string()),
        Err(e) => push(out, "Lighting mode:", unavailable(&e)),
    }
    let mut layers = vec![0];
    for (label, cmd) in [
        ("Layers:", p::GET_LAYERS_LAYOUT),
        ("Global layers:", p::GET_GLOBAL_LAYERS),
    ] {
        match session.query(cmd, &[]) {
            Ok(m) => {
                let ids = info::parse_id_list(&m.data);
                if cmd == p::GET_LAYERS_LAYOUT {
                    if let Some(ids) = ids.as_ref().filter(|ids| !ids.is_empty()) {
                        layers.clone_from(ids);
                    }
                }
                let text = ids.map(|ids| format!("{ids:?}"));
                push(out, label, decoded(text, &m.data));
            }
            Err(e) => push(out, label, unavailable(&e)),
        }
    }
    for layer in layers {
        let label = format!("Layer {layer}:");
        match session.query(p::GET_LAYER_CONFIG, &[layer]) {
            Ok(m) => push(
                out,
                &label,
                decoded(
                    LayerConfig::parse(&m.data).map(|c| describe_layer(&c)),
                    &m.data,
                ),
            ),
            Err(e) => push(out, &label, unavailable(&e)),
        }
    }
    match session.query(p::GET_CALIBRATION, &[]) {
        Ok(m) => {
            let calibration = Calibration::parse(&m.data).map(|c| {
                format!(
                    "{}: red {:.0} %, green {:.0} %, blue {:.0} %",
                    if c.enabled { "on" } else { "off" },
                    c.red * 100.0,
                    c.green * 100.0,
                    c.blue * 100.0
                )
            });
            push(out, "Calibration:", decoded(calibration, &m.data));
        }
        Err(e) => push(out, "Calibration:", unavailable(&e)),
    }
}

/// Game mode, quick tap, key bindings.
fn keyboard_lines<T: Transport>(session: &mut Session<T>, out: &mut Vec<String>) {
    let state = session.query(p::GET_GAME_MODE_STATE, &[]);
    let flags = session.query(p::GET_GAME_MODE_CONFIG, &[]);
    let game_mode = match (&state, &flags) {
        (Ok(s), Ok(f)) => match (s.data.first(), f.data.first()) {
            (Some(&s), Some(&f)) => {
                let blocks = game_mode_blocks(f);
                format!(
                    "{}, blocks {}",
                    if s & 1 == 1 { "on" } else { "off" },
                    if blocks.is_empty() {
                        "nothing".to_owned()
                    } else {
                        blocks.join(", ")
                    }
                )
            }
            _ => format!("cannot decode: {} / {}", hex(&s.data), hex(&f.data)),
        },
        (Err(e), _) | (_, Err(e)) => unavailable(e),
    };
    push(out, "Game mode:", game_mode);

    match session.query(p::GET_SNAP_TAP_CONFIG, &[]) {
        Ok(m) => {
            let pairs = info::parse_snap_tap(&m.data).map(|pairs| {
                if pairs.is_empty() {
                    return "no pairs".to_owned();
                }
                let name = |id: u16| {
                    u8::try_from(id)
                        .ok()
                        .and_then(keymap::code_for_key_id)
                        .map_or_else(|| format!("key {id:#06x}"), str::to_owned)
                };
                pairs
                    .iter()
                    .map(|pair| {
                        format!(
                            "{}/{} ({})",
                            name(pair.first),
                            name(pair.second),
                            pair.priority_name().unwrap_or("?")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            push(out, "Quick tap:", decoded(pairs, &m.data));
        }
        Err(e) => push(out, "Quick tap:", unavailable(&e)),
    }

    let enabled = session
        .query(p::GET_BINDINGS_CONFIG, &[])
        .map(|m| m.data.first().copied());
    match read_bindings(session) {
        Ok((total, bindings)) => {
            let state = match enabled {
                Ok(Some(1)) => "on",
                Ok(Some(_)) => "off",
                _ => "state unknown",
            };
            push(
                out,
                "Key bindings:",
                format!("{state}, {} of {total} decoded", bindings.len()),
            );
            for b in bindings {
                let key = format!("{}{}", if b.fn_layer { "Fn+" } else { "" }, b.key_name());
                out.push(format!("  {key:<20} {}", b.action));
            }
        }
        Err(e) => push(out, "Key bindings:", unavailable(&e)),
    }
    // GetMacros: layout unknown so far, shown as hex.
    match session.query(p::GET_MACROS, &[]) {
        Ok(m) => push(
            out,
            "Macros (raw):",
            if m.data.is_empty() {
                "(empty)".to_owned()
            } else {
                hex(&m.data)
            },
        ),
        Err(e) => push(out, "Macros:", unavailable(&e)),
    }
}

/// All bindings, read in as many `11:01` requests as needed.
fn read_bindings<T: Transport>(
    session: &mut Session<T>,
) -> Result<(usize, Vec<info::Binding>), Error> {
    let mut all = Vec::new();
    let mut total = 0;
    for _ in 0..MAX_BINDING_REQUESTS {
        let index = u16::try_from(all.len()).unwrap_or(u16::MAX);
        let reply = session.query(p::GET_BINDINGS, &index.to_le_bytes())?;
        let Some((reply_total, entries)) = info::parse_bindings(&reply.data) else {
            break;
        };
        total = reply_total;
        if entries.is_empty() {
            break;
        }
        all.extend(entries);
        if all.len() >= total {
            break;
        }
    }
    Ok((total, all))
}

/// Keycap legends of the layout the keyboard reports (`07:01`), if QuietLinux has them.
pub fn device_legends<T: Transport>(session: &mut Session<T>) -> Option<keymap::Layout> {
    let reply = session.query(p::GET_LAYOUT, &[]).ok()?;
    Layout::parse(&reply.data)?.legends()
}

/// Describe a notification: key presses decoded, anything else as hex.
pub fn describe_notification(message: &Message, legends: keymap::Layout) -> String {
    if message.cmd == p::ACTION_FIRED {
        if let Some(event) = KeyEvent::parse(&message.data) {
            let name = event.key_name();
            let legend = keymap::code_for_key_id(event.key_id)
                .map(|code| legends.legend(code))
                .filter(|legend| *legend != name)
                .map_or_else(String::new, |legend| format!(" [{legend}]"));
            let fn_prefix = if event.fn_layer && name != "Fn" {
                "Fn+"
            } else {
                ""
            };
            let key = format!("{fn_prefix}{name}{legend} ({:02x})", event.key_id);
            let action = event
                .action
                .map_or_else(|| "?".to_owned(), |a| a.to_string());
            return format!("{key:<24} {action:<36} [{}]", hex(&message.data));
        }
    }
    format!(
        "{} {}  [{}]",
        message.cmd,
        message.name().unwrap_or("?"),
        hex(&message.data)
    )
}

/// Print notifications until `count` were seen (or forever).
pub fn watch<T: Transport>(
    session: &mut Session<T>,
    count: Option<usize>,
    legends: keymap::Layout,
    mut print: impl FnMut(String),
) -> Result<(), Error> {
    let mut seen = 0;
    while count.is_none_or(|n| seen < n) {
        if let Some(message) = session.next_notification(Duration::from_secs(1))? {
            print(describe_notification(&message, legends));
            seen += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use quietlinux_core::device::fake::FakeKeyboard;

    use super::*;

    #[test]
    fn info_of_a_default_keyboard() {
        let mut kb = FakeKeyboard::tested();
        kb.answers.insert(p::GET_QLINK_VERSION, vec![3, 0, 2, 1]);
        kb.answers
            .insert(p::GET_SUPPORTED_FEATURES, vec![2, 0x01, 1, 0x10, 1]);
        kb.answers.insert(p::GET_LAYOUT, vec![1, 1]);
        kb.answers.insert(p::GET_POLLING_RATE, vec![0xe8, 0x03]);
        kb.answers.insert(
            p::GET_LAYER_CONFIG,
            vec![0, 0, 100, 50, 0, 0xe6, 0x30, 0x00],
        );
        kb.answers.insert(p::GET_GAME_MODE_STATE, vec![0]);
        kb.answers.insert(p::GET_GAME_MODE_CONFIG, vec![0x04]);
        kb.answers.insert(p::GET_SNAP_TAP_CONFIG, vec![0]);
        kb.answers.insert(p::GET_BINDINGS_CONFIG, vec![1]);
        kb.answers.insert(
            p::GET_BINDINGS,
            vec![2, 0, 0x02, 0x00, 0x02, 7, 0x6f, 0x00, 0x02, 2],
        );
        let mut session = Session::new(&mut kb, false).unwrap();
        let lines = info_lines(&mut session);
        let text = lines.join("\n");
        for expected in [
            "Session:        2 (timeout 3)",
            "Model:          Light Mount (0x0002), revision 1",
            "Firmware:       2.3.0 (tested)",
            "QLink:          1.2.3",
            "Features:       root v1, lighting v1",
            "Layout:         ISO, German",
            "Polling rate:   1000 Hz",
            "Lighting mode:  effect",
            "Layers:         unavailable",
            "Layer 0:        static, colors e63000, brightness 100, speed 50, direction up",
            "Calibration:    unavailable",
            "Game mode:      off, blocks Win",
            "Quick tap:      no pairs",
            "Key bindings:   on, 2 of 2 decoded",
            "  M1                   media: play/pause",
            "  KnobLeft             media: volume down",
            "Macros:         unavailable",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in\n{text}");
        }
        // only read-only requests were sent
        assert!(
            kb.commands()
                .iter()
                .all(|c| c != "10:02" && c != "10:06" && c != "10:0d")
        );
    }

    #[test]
    fn key_events_show_the_legend() {
        let message = Message {
            request_id: 0,
            session: 1,
            status: 0,
            cmd: p::ACTION_FIRED,
            data: vec![0x28, 0x00, 0x01, 0x01, 0x00, 0x1c],
        };
        let line = describe_notification(&message, keymap::Layout::De);
        assert!(line.starts_with("KeyY [Z] (28)"), "{line}");
        let line = describe_notification(&message, keymap::Layout::Uk);
        assert!(line.starts_with("KeyY [Y] (28)"), "{line}");
    }

    #[test]
    fn notifications_are_described() {
        let mut kb = FakeKeyboard::tested();
        let mut session = Session::new(&mut kb, false).unwrap();
        session
            .transport_mut()
            .notify(p::ACTION_FIRED, &[0x5c, 0x80, 0x01, 0x09, 0x03]);
        session
            .transport_mut()
            .notify(p::Command::new(0x07, 0x02), &[1]);
        let mut lines = Vec::new();
        watch(&mut session, Some(2), keymap::Layout::De, |l| lines.push(l)).unwrap();
        assert!(
            lines[0].starts_with("Fn+ArrowRight [→] (5c)"),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains("lighting: previous effect"));
        assert!(lines[1].starts_with("07:02 StateChanged"), "{}", lines[1]);
    }
}
