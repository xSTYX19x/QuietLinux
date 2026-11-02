//! The desktop's file chooser, through the XDG desktop portal
//! (`org.freedesktop.portal.FileChooser` on the D-Bus session bus): GNOME, KDE
//! and the others show their own dialog. Blocking: call it from a thread of its
//! own, not from the window's.

use std::collections::HashMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

/// A filter of the dialog: its name and glob patterns.
pub type Filter = (&'static str, &'static [&'static str]);

/// Choose an existing file; `None` if the dialog was cancelled.
pub fn open_file(title: &str, filters: &[Filter]) -> Result<Option<PathBuf>, String> {
    let mut options = HashMap::new();
    options.insert("filters", filter_value(filters));
    choose("OpenFile", title, options)
}

/// Choose where to save a file, starting with `name` in `folder`; the dialog
/// asks before replacing an existing file. `None` if it was cancelled.
pub fn save_file(
    title: &str,
    name: &str,
    folder: Option<&Path>,
    filters: &[Filter],
) -> Result<Option<PathBuf>, String> {
    let mut options = HashMap::new();
    options.insert("filters", filter_value(filters));
    options.insert("current_name", Value::from(name.to_owned()));
    if let Some(folder) = folder {
        // a null-terminated byte string
        let mut bytes = folder.as_os_str().as_encoded_bytes().to_vec();
        bytes.push(0);
        options.insert("current_folder", Value::from(bytes));
    }
    choose("SaveFile", title, options)
}

/// `a(sa(us))`: name and patterns (type 0: glob).
fn filter_value(filters: &[Filter]) -> Value<'static> {
    let list: Vec<(String, Vec<(u32, String)>)> = filters
        .iter()
        .map(|(name, patterns)| {
            let patterns = patterns.iter().map(|p| (0, (*p).to_owned())).collect();
            ((*name).to_owned(), patterns)
        })
        .collect();
    Value::from(list)
}

fn choose(
    method: &str,
    title: &str,
    mut options: HashMap<&str, Value<'_>>,
) -> Result<Option<PathBuf>, String> {
    static REQUESTS: AtomicU32 = AtomicU32::new(0);
    let unavailable = |e: zbus::Error| format!("no file chooser (XDG desktop portal): {e}");
    let connection = Connection::session().map_err(unavailable)?;
    // the answer comes as a signal of a request object whose path is known
    // beforehand: listen to it before asking, so that it cannot be missed
    let token = format!(
        "quietlinux{}_{}",
        std::process::id(),
        REQUESTS.fetch_add(1, Ordering::Relaxed)
    );
    let sender = connection
        .unique_name()
        .ok_or("no name on the D-Bus session bus")?
        .trim_start_matches(':')
        .replace('.', "_");
    let path = format!("{PORTAL_PATH}/request/{sender}/{token}");
    let request = Proxy::new(
        &connection,
        PORTAL,
        path.as_str(),
        "org.freedesktop.portal.Request",
    )
    .map_err(unavailable)?;
    let mut answers = request.receive_signal("Response").map_err(unavailable)?;
    options.insert("handle_token", Value::from(token.clone()));
    options.insert("modal", Value::from(true));
    let chooser = Proxy::new(
        &connection,
        PORTAL,
        PORTAL_PATH,
        "org.freedesktop.portal.FileChooser",
    )
    .map_err(unavailable)?;
    let _: OwnedObjectPath = chooser
        .call(method, &("", title, options))
        .map_err(unavailable)?;
    let answer = answers.next().ok_or("the file chooser did not answer")?;
    let (code, mut results): (u32, HashMap<String, OwnedValue>) = answer
        .body()
        .deserialize()
        .map_err(|e| format!("file chooser: {e}"))?;
    match code {
        0 => {}
        1 => return Ok(None),
        _ => return Err("the file chooser failed".to_owned()),
    }
    let uris: Vec<String> = results
        .remove("uris")
        .and_then(|v| Vec::try_from(v).ok())
        .unwrap_or_default();
    let uri = uris.first().ok_or("the file chooser gave no file")?;
    file_path(uri)
        .map(Some)
        .ok_or_else(|| format!("not a local file: {uri}"))
}

/// The path of a `file://` URI (percent-encoded bytes decoded).
fn file_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // a host before the path: only the local one
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path.starts_with('/') {
        return None;
    }
    let mut bytes = Vec::with_capacity(path.len());
    let mut iter = path.bytes();
    while let Some(b) = iter.next() {
        if b == b'%' {
            let hex = [iter.next()?, iter.next()?];
            let hex = std::str::from_utf8(&hex).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
        } else {
            bytes.push(b);
        }
    }
    Some(PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris() {
        assert_eq!(
            file_path("file:///home/a/light%20mount.toml"),
            Some(PathBuf::from("/home/a/light mount.toml"))
        );
        assert_eq!(
            file_path("file://localhost/tmp/%C3%BC.zip"),
            Some(PathBuf::from("/tmp/ü.zip"))
        );
        assert_eq!(file_path("file://server/share/x"), None);
        assert_eq!(file_path("https://example.org/x"), None);
        assert_eq!(file_path("file:///bad%2"), None);
    }

    #[test]
    fn filters_have_the_portals_type() {
        let value = filter_value(&[("Settings", &["*.toml"])]);
        assert_eq!(value.value_signature().to_string(), "a(sa(us))");
    }
}
