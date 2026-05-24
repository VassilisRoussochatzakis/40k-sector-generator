//! §P6: user preferences persisted as `~/.config/sectorforge/preferences.toml`.
//!
//! Currently the only field is the recent-projects MRU list. Loaders are
//! tolerant — a missing or unparseable file collapses to defaults rather
//! than erroring, so a fresh user install never blocks on this.

use std::fs;
use std::path::Path;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 10;
const REL_DIR: &str = ".config/sectorforge";
const FILE_NAME: &str = "preferences.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub recent_projects: Vec<Utf8PathBuf>,
}

impl Preferences {
    /// Resolve the default preferences path under the user's HOME. Returns
    /// `None` on platforms where `HOME` is not set (the GUI just keeps its
    /// in-memory defaults in that case).
    pub fn default_path() -> Option<Utf8PathBuf> {
        let home_os = std::env::var_os("HOME")?;
        let home = Utf8PathBuf::from_path_buf(home_os.into()).ok()?;
        Some(home.join(REL_DIR).join(FILE_NAME))
    }

    /// Read from [`Self::default_path`] or return [`Self::default`] on any
    /// error. Loaders intentionally swallow filesystem and parse errors so a
    /// hand-edited file with a typo cannot wedge the GUI.
    pub fn load() -> Self {
        let Some(path) = Self::default_path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(Path::new(path.as_str())) else {
            return Self::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }

    /// Persist to [`Self::default_path`]. Creates parent directories on
    /// demand. Returns the I/O error so callers can surface it; the GUI
    /// treats this as best-effort.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::default_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(Path::new(parent.as_str()))?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        fs::write(Path::new(path.as_str()), text)
    }

    /// Push a project path onto the MRU. Bumps an existing entry to the front
    /// rather than duplicating. Truncates the list to [`MAX_RECENT`].
    pub fn push_recent(&mut self, path: Utf8PathBuf) {
        self.recent_projects.retain(|p| p != &path);
        self.recent_projects.insert(0, path);
        self.recent_projects.truncate(MAX_RECENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_recent_dedups_and_bumps() {
        let mut p = Preferences::default();
        p.push_recent(Utf8PathBuf::from("/a"));
        p.push_recent(Utf8PathBuf::from("/b"));
        p.push_recent(Utf8PathBuf::from("/a"));
        assert_eq!(p.recent_projects.len(), 2);
        assert_eq!(p.recent_projects[0], Utf8PathBuf::from("/a"));
        assert_eq!(p.recent_projects[1], Utf8PathBuf::from("/b"));
    }

    #[test]
    fn push_recent_truncates_to_max() {
        let mut p = Preferences::default();
        for i in 0..(MAX_RECENT + 5) {
            p.push_recent(Utf8PathBuf::from(format!("/p{i}")));
        }
        assert_eq!(p.recent_projects.len(), MAX_RECENT);
        assert_eq!(
            p.recent_projects[0],
            Utf8PathBuf::from(format!("/p{}", MAX_RECENT + 4))
        );
    }

    #[test]
    fn round_trip_preferences_toml() {
        let mut p = Preferences::default();
        p.push_recent(Utf8PathBuf::from("/foo"));
        let text = toml::to_string_pretty(&p).unwrap();
        let back: Preferences = toml::from_str(&text).unwrap();
        assert_eq!(back.recent_projects, p.recent_projects);
    }
}
