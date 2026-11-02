//! `--json`: what the read-only commands show, for scripts.
//!
//! Field names are part of the interface: add fields, do not rename or remove
//! them (documented in docs/usage-cli.md).

use quietlinux_core::config::{self, Snapshot};
use quietlinux_core::device::{Identity, LightingMode, fmt_version};
use quietlinux_core::info::Macro;
use quietlinux_core::keymap::{self, KEYS, Layout};
use quietlinux_core::layout::Zone;
use quietlinux_core::macros::{self, RepeatMode};
use quietlinux_core::profile::Profiles;
use quietlinux_core::settings::GameModeBlocks;
use quietlinux_core::state::Layouts;
use serde_json::{Value, json};

/// Print a value, indented.
pub fn print(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

pub fn status(usb_path: &str, node: &str, identity: &Identity, mode: LightingMode) -> Value {
    json!({
        "usb_path": usb_path,
        "node": node,
        "firmware": fmt_version(&identity.version),
        "tested": identity.known(),
        "lighting_mode": mode.to_string(),
    })
}

/// What `info` reads, as `backup` stores it.
pub fn snapshot(s: &Snapshot) -> Value {
    let bindings: Vec<Value> = s
        .bindings
        .iter()
        .map(|b| {
            let key = keymap::code_for_key_id(b.key_id)
                .map_or_else(|| format!("key-{:02x}", b.key_id), str::to_owned);
            json!({
                "key": key,
                "key_id": b.key_id,
                "fn": b.fn_layer,
                // the syntax of `bind`, or null for bindings QuietLinux cannot write
                "action": config::action_text(&b.action),
                "description": b.action.to_string(),
                "raw": hex(&b.raw),
            })
        })
        .collect();
    json!({
        "model": s.model,
        "firmware": s.firmware,
        "layout": s.layout,
        "polling_rate": s.polling_rate,
        "lighting_mode": s.lighting_mode.to_string(),
        "game_mode": {
            "on": s.game_mode_on,
            "blocks": GameModeBlocks(s.game_mode_flags).names(),
        },
        "bindings_enabled": s.bindings_enabled,
        "bindings": bindings,
        "macros": s.macros.as_deref().map(macro_list),
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn macro_list(list: &[Macro]) -> Value {
    list.iter()
        .map(|m| {
            json!({
                "id": m.id,
                "name": m.name,
                "repeat": RepeatMode(m.repeat()).name(),
                "events": m.events.len(),
            })
        })
        .collect()
}

/// One macro with its events in the syntax of `macro create`.
pub fn macro_detail(m: &Macro) -> Value {
    json!({
        "id": m.id,
        "name": m.name,
        "repeat": RepeatMode(m.repeat()).name(),
        "events": macros::events_text(&m.events, macros::DEFAULT_DELAY_MS),
    })
}

pub fn keys(layout: Layout) -> Value {
    KEYS.iter()
        .map(|k| {
            json!({
                "led": format!("{:02x}", k.led),
                "name": k.code,
                "legend": layout.legend(k.code),
            })
        })
        .collect()
}

pub fn zones() -> Value {
    Zone::ALL
        .iter()
        .map(|z| {
            let leds: Vec<String> = z.leds().map(|l| format!("{l:02x}")).collect();
            json!({ "name": z.name(), "leds": leds })
        })
        .collect()
}

pub fn profiles(profiles: &Profiles) -> Value {
    json!({
        "active": profiles.active(),
        "profiles": profiles.names(),
        "folder": profiles.dir(),
    })
}

pub fn layouts(layouts: &Layouts) -> Value {
    json!({
        "layouts": layouts.names(),
        "folder": layouts.dir(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quietlinux_core::info::Action;
    use quietlinux_core::info::Binding;

    #[test]
    fn a_snapshot() {
        let s = Snapshot {
            model: Some("Light Mount"),
            firmware: Some("2.3.0".to_owned()),
            layout: None,
            polling_rate: Some(1000),
            lighting_mode: LightingMode::General,
            game_mode_flags: 0x04,
            game_mode_on: Some(false),
            bindings_enabled: None,
            bindings: vec![Binding {
                key_id: 0x03,
                fn_layer: false,
                action: Action::StandardKey {
                    modifiers: 0,
                    usage: 0x68,
                },
                raw: vec![0x01, 0x00, 0x68],
            }],
            macros: Some(vec![Macro::new(1, "hello", Vec::new())]),
        };
        let v = snapshot(&s);
        assert_eq!(v["bindings"][0]["key"], "M2");
        assert_eq!(v["bindings"][0]["action"], "key F13");
        assert_eq!(v["bindings"][0]["raw"], "01 00 68");
        assert_eq!(v["game_mode"]["blocks"], json!(["win"]));
        assert_eq!(v["macros"][0]["repeat"], "once");
        assert_eq!(v["layout"], Value::Null);
        assert_eq!(keys(Layout::De)[0]["led"], "2d");
        assert_eq!(zones()[0]["name"], "top");
    }
}
