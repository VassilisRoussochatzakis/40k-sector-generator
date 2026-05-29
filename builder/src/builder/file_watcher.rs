//! §P5: background watcher for the project directory.
//!
//! The spec lists `notify` as the preferred backend ("propose addition" if
//! not present). R9 forbids new crates, so we instead poll mtimes on a
//! short-lived background thread and post change events through
//! [`std::sync::mpsc`]. The cost is one `stat` per tracked file per tick —
//! cheap for project trees with O(dozens) of TOML files.
//!
//! The main UI thread drains [`FileWatcher::recv_event`] each frame and:
//!
//!   * silently reloads catalogs when the in-memory buffer is clean, or
//!   * raises [`crate::builder::ModalKind::ConflictResolver`] when the
//!     in-memory buffer is dirty so the user can choose between reload and
//!     keep.
//!
//! The watcher owns its own thread + cancel flag and shuts down cleanly on
//! drop — the GUI does not need explicit teardown when switching projects.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use camino::{Utf8Path, Utf8PathBuf};

/// One change event posted by the polling thread. `mtime` lets the consumer
/// update its mtime baseline without a second `stat`.
#[derive(Debug, Clone)]
pub struct FileChange {
    /// Project-relative path of the file that changed.
    pub rel_path: String,
    /// New mtime as observed by the watcher.
    pub mtime: SystemTime,
}

/// Polling watcher rooted at a project directory. Drops cancel the background
/// thread.
pub struct FileWatcher {
    root: Utf8PathBuf,
    rx: Receiver<FileChange>,
    cancel: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl FileWatcher {
    /// Spawn a polling thread rooted at `root`. `baseline` is the starting
    /// mtime map (usually the snapshot taken at open time). Returns a handle
    /// the caller stores in [`crate::builder::BuilderState::file_watcher`].
    pub fn spawn(root: Utf8PathBuf, baseline: BTreeMap<String, SystemTime>) -> Self {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);
        let root_clone = root.clone();
        let join = thread::spawn(move || poll_loop(root_clone, baseline, tx, cancel_clone));
        Self {
            root,
            rx,
            cancel,
            join: Some(join),
        }
    }

    /// Drain the latest pending change, if any. Non-blocking — safe to call
    /// every frame from the UI loop.
    pub fn try_recv(&self) -> Option<FileChange> {
        self.rx.try_recv().ok()
    }

    pub fn root(&self) -> &Utf8Path {
        &self.root
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

/// Pure pass over `baseline`: returns the files whose mtime advanced and
/// updates `baseline` in-place with the new values. Side-effect-free w.r.t.
/// any thread / channel state, so tests can drive it directly without sleeps.
pub(crate) fn scan_once(
    root: &Utf8Path,
    baseline: &mut BTreeMap<String, SystemTime>,
) -> Vec<FileChange> {
    let mut out = Vec::new();
    let snapshot: Vec<(String, SystemTime)> =
        baseline.iter().map(|(k, v)| (k.clone(), *v)).collect();
    for (rel, last) in snapshot {
        let abs = root.join(&rel);
        let Ok(meta) = std::fs::metadata(Path::new(abs.as_str())) else {
            continue;
        };
        let Ok(now) = meta.modified() else { continue };
        if now > last {
            baseline.insert(rel.clone(), now);
            out.push(FileChange {
                rel_path: rel,
                mtime: now,
            });
        }
    }
    out
}

fn poll_loop(
    root: Utf8PathBuf,
    mut baseline: BTreeMap<String, SystemTime>,
    tx: Sender<FileChange>,
    cancel: Arc<AtomicBool>,
) {
    let tick = Duration::from_millis(1000);
    while !cancel.load(Ordering::Acquire) {
        for ev in scan_once(&root, &mut baseline) {
            if cancel.load(Ordering::Acquire) {
                return;
            }
            if tx.send(ev).is_err() {
                return;
            }
        }
        // Sleep in small slices so cancel signals are honoured quickly.
        for _ in 0..10 {
            if cancel.load(Ordering::Acquire) {
                return;
            }
            thread::sleep(tick / 10);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scan_once_reports_changed_file_and_advances_baseline() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let target = root.join("sectorforge.toml");
        fs::write(target.as_std_path(), b"a = 1\n").unwrap();
        // Baseline set to UNIX_EPOCH guarantees the on-disk mtime is newer
        // without sleeping past filesystem mtime resolution.
        let mut baseline = BTreeMap::new();
        baseline.insert("sectorforge.toml".to_string(), SystemTime::UNIX_EPOCH);

        let events = scan_once(&root, &mut baseline);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].rel_path, "sectorforge.toml");
        // Second pass with the advanced baseline returns nothing.
        let again = scan_once(&root, &mut baseline);
        assert!(again.is_empty());
    }

    #[test]
    fn scan_once_skips_files_with_no_baseline_entry() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        fs::write(root.join("untracked.toml").as_std_path(), b"x = 1\n").unwrap();
        let mut baseline: BTreeMap<String, SystemTime> = BTreeMap::new();
        assert!(scan_once(&root, &mut baseline).is_empty());
    }

    #[test]
    fn scan_once_skips_missing_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let mut baseline = BTreeMap::new();
        baseline.insert("never_existed.toml".to_string(), SystemTime::UNIX_EPOCH);
        assert!(scan_once(&root, &mut baseline).is_empty());
    }
}
