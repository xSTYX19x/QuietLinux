//! Profiles: named settings files in `$XDG_CONFIG_HOME/quietlinux/profiles`.
//!
//! The Light Mount holds no profiles: IO Center for Windows keeps them on the PC
//! and writes all settings of a profile when it switches (docs/protocol.md).
//! A QuietLinux profile is a settings file as `quietlinux backup` writes it, plus
//! the effect and the lighting, which the keyboard does not report. Switching
//! first saves the keyboard's settings to the active profile, so that changes
//! made since are kept, then writes what differs (as `quietlinux apply`).
//!
//! A key bound to "select profile" names the profile with 16 bytes (IO Center:
//! the profile's UUID). QuietLinux puts the name there, zero-padded: names have
//! at most 16 characters.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::config::{self, EffectSetting, Plan, Settings, Snapshot};
use crate::device::{Session, Transport};
use crate::state::{self, Lighting};

/// Longest profile name: the size of a profile ID in a binding.
pub const MAX_NAME: usize = 16;

/// `next` and `previous` choose a neighbour: they are no names.
const RESERVED: [&str; 2] = ["next", "previous"];

/// The file in the profile folder that names the active profile.
const ACTIVE_FILE: &str = "active";

/// Check a profile name: 1 to 16 characters `A-Z a-z 0-9 - _`.
pub fn check_name(name: &str) -> Result<(), String> {
    let allowed = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
    if name.is_empty() || name.len() > MAX_NAME || !name.bytes().all(allowed) {
        return Err(format!(
            "invalid profile name {name:?}: 1 to {MAX_NAME} characters A-Z, a-z, 0-9, - and _"
        ));
    }
    if RESERVED.contains(&name) {
        return Err(format!(
            "{name:?} is no profile name: it chooses the {name} profile"
        ));
    }
    Ok(())
}

/// The profile ID of a "select profile" binding: the name, zero-padded.
pub fn id(name: &str) -> [u8; MAX_NAME] {
    let mut id = [0; MAX_NAME];
    let bytes = &name.as_bytes()[..name.len().min(MAX_NAME)];
    id[..bytes.len()].copy_from_slice(bytes);
    id
}

/// The profile a binding's ID names, or `None` for IDs QuietLinux did not write
/// (IO Center's UUIDs).
pub fn name_from_id(id: &[u8; MAX_NAME]) -> Option<String> {
    let len = id.iter().position(|&b| b == 0).unwrap_or(MAX_NAME);
    if id[len..].iter().any(|&b| b != 0) {
        return None;
    }
    let name = std::str::from_utf8(&id[..len]).ok()?;
    check_name(name).ok()?;
    Some(name.to_owned())
}

/// `base`, or with a number (`base-2`, …) if `taken` says it is in use; at
/// most 16 characters.
pub fn numbered(base: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut name = base.to_owned();
    let mut n = 2;
    while taken(&name) {
        let suffix = format!("-{n}");
        let keep = base.len().min(MAX_NAME - suffix.len());
        name = format!("{}{suffix}", base[..keep].trim_end_matches('-'));
        n += 1;
    }
    name
}

/// A profile to switch to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The next one in alphabetical order (after the last: the first).
    Next,
    Previous,
    Name(String),
}

impl FromStr for Target {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "next" => Ok(Target::Next),
            "previous" => Ok(Target::Previous),
            name => check_name(name).map(|()| Target::Name(name.to_owned())),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Next => f.write_str("next"),
            Target::Previous => f.write_str("previous"),
            Target::Name(name) => f.write_str(name),
        }
    }
}

/// What the keyboard does not report and a profile holds: the last effect and
/// the lighting QuietLinux set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Remembered {
    pub effect: Option<EffectSetting>,
    pub lighting: Option<Lighting>,
}

impl Remembered {
    /// From `last-effect.toml` and `lighting.toml`.
    pub fn load() -> Self {
        Self {
            effect: config::last_effect(),
            lighting: Some(state::load()),
        }
    }
}

/// A profile switch that was made (or, with `dry_run`, would be).
#[derive(Debug, Clone)]
pub struct Switch {
    pub name: String,
    /// The profile that was active, saved before switching.
    pub saved: Option<String>,
    pub plan: Plan,
}

/// The profile folder.
#[derive(Debug, Clone)]
pub struct Profiles {
    dir: PathBuf,
}

impl Profiles {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// `$XDG_CONFIG_HOME/quietlinux/profiles` (or `~/.config/quietlinux/profiles`).
    pub fn standard() -> Option<Self> {
        Some(Self::new(config::config_dir()?.join("profiles")))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.toml"))
    }

    pub fn exists(&self, name: &str) -> bool {
        check_name(name).is_ok() && self.path(name).is_file()
    }

    /// The profiles, in alphabetical order.
    pub fn names(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| {
                let name = e.ok()?.file_name().into_string().ok()?;
                let name = name.strip_suffix(".toml")?;
                self.exists(name).then(|| name.to_owned())
            })
            .collect();
        names.sort();
        names
    }

    /// The active profile: the one switched to or saved last, if it still exists.
    pub fn active(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.dir.join(ACTIVE_FILE)).ok()?;
        let name = text.trim();
        self.exists(name).then(|| name.to_owned())
    }

    pub fn set_active(&self, name: Option<&str>) -> std::io::Result<()> {
        let path = self.dir.join(ACTIVE_FILE);
        match name {
            Some(name) => {
                self.create_dir()?;
                std::fs::write(path, format!("{name}\n"))
            }
            None => match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            },
        }
    }

    /// The name of the profile `target` chooses.
    pub fn resolve(&self, target: &Target) -> Result<String, String> {
        let names = self.names();
        let step = match target {
            Target::Name(name) if self.exists(name) => return Ok(name.clone()),
            Target::Name(name) => {
                return Err(if names.is_empty() {
                    format!("no profile {name:?}: there are no profiles yet")
                } else {
                    format!("no profile {name:?} (profiles: {})", names.join(", "))
                });
            }
            Target::Next => 1,
            Target::Previous => names.len().saturating_sub(1),
        };
        if names.is_empty() {
            return Err(
                "there are no profiles yet: save one with `quietlinux profile save NAME`"
                    .to_owned(),
            );
        }
        let index = match self
            .active()
            .and_then(|a| names.iter().position(|n| *n == a))
        {
            Some(active) => (active + step) % names.len(),
            // none active: next is the first, previous the last
            None if *target == Target::Next => 0,
            None => names.len() - 1,
        };
        Ok(names[index].clone())
    }

    /// Read and check a profile.
    pub fn load(&self, name: &str) -> Result<Settings, String> {
        let path = self.path(name);
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        config::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Write a profile's text (readable only by its owner: it holds what the macros type).
    pub fn write(&self, name: &str, text: &str) -> std::io::Result<()> {
        check_name(name).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        self.create_dir()?;
        config::write_backup(&self.path(name), text, true)
    }

    pub fn delete(&self, name: &str) -> std::io::Result<()> {
        check_name(name).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let active = self.active();
        std::fs::remove_file(self.path(name))?;
        if active.as_deref() == Some(name) {
            self.set_active(None)?;
        }
        Ok(())
    }

    fn create_dir(&self) -> std::io::Result<()> {
        use std::os::unix::fs::DirBuilderExt as _;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.dir)
    }

    /// Save the keyboard's settings, with `remembered`, as profile `name`; it
    /// becomes the active profile.
    pub fn save<T: Transport>(
        &self,
        session: &mut Session<T>,
        name: &str,
        remembered: &Remembered,
    ) -> Result<Snapshot, Box<dyn std::error::Error>> {
        check_name(name)?;
        let snapshot = config::read_snapshot(session)?;
        let text = config::profile_text(
            &snapshot,
            remembered.effect.as_ref(),
            remembered.lighting.as_ref(),
        );
        self.write(name, &text)?;
        self.set_active(Some(name))?;
        Ok(snapshot)
    }

    /// Switch to a profile: save the keyboard's settings to the active profile
    /// (not when switching to it: that writes it again), then write what differs.
    /// `remember`: the effect and lighting QuietLinux remembers are saved with the
    /// active profile and replaced by the new profile's (not in the demo mode).
    /// `dry_run`: nothing is saved or written.
    pub fn switch<T: Transport>(
        &self,
        session: &mut Session<T>,
        target: &Target,
        remember: bool,
        dry_run: bool,
    ) -> Result<Switch, Box<dyn std::error::Error>> {
        let name = self.resolve(target)?;
        // a broken profile is reported before anything is changed
        let settings = self.load(&name)?;
        let snapshot = config::read_snapshot(session)?;
        let saved = match self.active() {
            Some(active) if active != name && !dry_run => {
                let remembered = if remember {
                    Remembered::load()
                } else {
                    Remembered::default()
                };
                let text = config::profile_text(
                    &snapshot,
                    remembered.effect.as_ref(),
                    remembered.lighting.as_ref(),
                );
                self.write(&active, &text)
                    .map_err(|e| format!("saving profile {active:?}: {e}"))?;
                Some(active)
            }
            _ => None,
        };
        let mut plan = config::plan(&settings, &snapshot);
        if !dry_run {
            config::execute(session, &plan)?;
            if remember {
                if let Err(e) = config::remember(&settings, &plan) {
                    plan.notes
                        .push(format!("could not remember the effect and lighting: {e}"));
                }
            }
            if let Err(e) = self.set_active(Some(&name)) {
                plan.notes
                    .push(format!("could not remember the active profile: {e}"));
            }
        }
        Ok(Switch { name, saved, plan })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rgb;
    use crate::device::fake::FakeKeyboard;
    use crate::info::Action;
    use crate::protocol as p;
    use crate::state::Mode;

    /// A profile folder of its own, removed at the end.
    struct Folder(Profiles);

    impl Folder {
        fn new(test: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("quietlinux-profiles-{test}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(Profiles::new(dir))
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.dir());
        }
    }

    #[test]
    fn names_and_ids() {
        for name in ["gaming", "Work_2", "a", "sixteen-chars-ok"] {
            assert_eq!(check_name(name), Ok(()), "{name}");
            assert_eq!(name_from_id(&id(name)).as_deref(), Some(name));
        }
        for name in [
            "",
            "seventeen-chars-x",
            "with space",
            "ümlaut",
            "a/b",
            "..",
            "next",
        ] {
            assert!(check_name(name).is_err(), "{name}");
        }
        assert_eq!(&id("ab")[..3], b"ab\0");
        // IO Center's UUIDs are no names
        let uuid = [
            0x43, 0xd0, 0x54, 0x4b, 0x88, 0x25, 0x4f, 0x64, 0xb2, 0xd2, 0xb1, 0x76, 0x87, 0xad,
            0x17, 0x3d,
        ];
        assert_eq!(name_from_id(&uuid), None);
        assert_eq!(name_from_id(&[0; 16]), None);
        let mut gap = id("ab");
        gap[5] = b'c';
        assert_eq!(name_from_id(&gap), None);
        assert_eq!(numbered("work", |n| n == "work" || n == "work-2"), "work-3");
        assert_eq!(
            numbered("sixteen-chars-ok", |n| n == "sixteen-chars-ok"),
            "sixteen-chars-2"
        );
        assert_eq!("next".parse(), Ok(Target::Next));
        assert_eq!("work".parse(), Ok(Target::Name("work".to_owned())));
        assert!("no way".parse::<Target>().is_err());
    }

    #[test]
    fn next_and_previous() {
        use std::os::unix::fs::PermissionsExt as _;
        let folder = Folder::new("order");
        let profiles = &folder.0;
        assert!(profiles.resolve(&Target::Next).is_err());
        for name in ["work", "gaming", "default"] {
            profiles.write(name, "version = 1\n").unwrap();
        }
        // not a profile: other files, invalid names
        std::fs::write(profiles.dir().join("notes.txt"), "").unwrap();
        std::fs::write(profiles.dir().join("bad name.toml"), "").unwrap();
        assert_eq!(profiles.names(), ["default", "gaming", "work"]);
        assert_eq!(profiles.active(), None);
        let resolve = |t: Target| profiles.resolve(&t).unwrap();
        assert_eq!(resolve(Target::Next), "default");
        assert_eq!(resolve(Target::Previous), "work");
        profiles.set_active(Some("work")).unwrap();
        assert_eq!(resolve(Target::Next), "default");
        assert_eq!(resolve(Target::Previous), "gaming");
        assert!(
            profiles
                .resolve(&Target::Name("nope".to_owned()))
                .unwrap_err()
                .contains("default, gaming, work")
        );
        profiles.delete("work").unwrap();
        assert_eq!(profiles.active(), None);
        assert_eq!(profiles.names(), ["default", "gaming"]);
        // only its owner can read a profile
        let mode = std::fs::metadata(profiles.path("gaming"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn switching_saves_the_active_profile_first() {
        let folder = Folder::new("switch");
        let profiles = &folder.0;
        let mut kb = FakeKeyboard::tested();
        kb.bindings
            .insert((0x03, p::LAYER_NORMAL), vec![0x02, 0x07]); // M2 play/pause
        let mut session = Session::new(&mut kb, false).unwrap();
        // "work": M2 play/pause, M5 next profile
        session
            .set_binding(0x06, false, &Action::Profile(3))
            .unwrap();
        profiles
            .save(&mut session, "work", &Remembered::default())
            .unwrap();
        assert_eq!(profiles.active().as_deref(), Some("work"));
        // "gaming": M2 F13, per-key colors, M5 selects "work"
        let lighting = Lighting {
            mode: Mode::PerKey,
            leds: [(0x2e, Rgb::new(255, 0, 0))].into(),
        };
        profiles
            .write(
                "gaming",
                "version = 1\n[bindings]\nM2 = \"key F13\"\nM5 = \"profile work\"\n\
                 [lighting]\nmode = \"per-key\"\n[lighting.leds]\n2e = \"ff0000\"\n",
            )
            .unwrap();
        assert_eq!(profiles.load("gaming").unwrap().lighting, Some(lighting));
        // a change made in "work" since it was saved
        session.set_binding(0x04, false, &Action::Media(8)).unwrap();
        let dry = profiles
            .switch(&mut session, &Target::Next, false, true)
            .unwrap();
        assert_eq!((dry.name.as_str(), dry.saved), ("gaming", None)); // after the last: the first
        let switch = profiles
            .switch(
                &mut session,
                &Target::Name("gaming".to_owned()),
                false,
                false,
            )
            .unwrap();
        assert_eq!(switch.saved.as_deref(), Some("work"));
        assert_eq!(profiles.active().as_deref(), Some("gaming"));
        let ops: Vec<String> = switch.plan.ops.iter().map(ToString::to_string).collect();
        assert_eq!(
            ops,
            [
                "M2: key F13",
                "M5: profile: select \"work\"",
                "M3: factory binding",
                "lighting: per-key colors (1 LEDs)",
            ]
        );
        drop(session);
        assert_eq!(
            kb.bindings.get(&(0x03, p::LAYER_NORMAL)),
            Some(&vec![0x01, 0x00, 0x68])
        );
        assert_eq!(kb.bindings.get(&(0x04, p::LAYER_NORMAL)), None);
        assert_eq!(kb.mode, p::MODE_REALTIME);
        assert_eq!(kb.leds.get(&0x2e), Some(&Rgb::new(255, 0, 0)));
        // "work" kept the change made since it was saved
        let work = profiles.load("work").unwrap();
        let bindings: Vec<String> = work
            .bindings
            .unwrap()
            .iter()
            .map(|(t, d)| format!("{} {d:?}", t.name))
            .collect();
        assert!(
            bindings.iter().any(|b| b.starts_with("M3 ")),
            "{bindings:?}"
        );
        // back: M5 is "next profile" again
        let mut session = Session::new(&mut kb, false).unwrap();
        let back = profiles
            .switch(&mut session, &Target::Next, false, false)
            .unwrap();
        assert_eq!(
            (back.name.as_str(), back.saved.as_deref()),
            ("work", Some("gaming"))
        );
        drop(session);
        assert_eq!(
            kb.bindings.get(&(0x06, p::LAYER_NORMAL)),
            Some(&vec![0x08, 0x03])
        );
    }
}
