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
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
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
        match self.rx.try_recv() {
            Ok(ev) => Some(ev),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
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

fn poll_loop(
    root: Utf8PathBuf,
    mut baseline: BTreeMap<String, SystemTime>,
    tx: Sender<FileChange>,
    cancel: Arc<AtomicBool>,
) {
    let tick = Duration::from_millis(1000);
    let mut shutdown_check = Duration::from_millis(0);
    while !cancel.load(Ordering::Acquire) {
        // Walk just the files we already know about. New files added on disk
        // are picked up on the next reload (open_project rebuilds the map).
        for (rel, last) in baseline.clone().iter() {
            if cancel.load(Ordering::Acquire) {
                return;
            }
            let abs = root.join(rel);
            let Ok(meta) = std::fs::metadata(Path::new(abs.as_str())) else {
                continue;
            };
            let Ok(now) = meta.modified() else { continue };
            if now > *last {
                baseline.insert(rel.clone(), now);
                let ev = FileChange {
                    rel_path: rel.clone(),
                    mtime: now,
                };
                if tx.send(ev).is_err() {
                    return;
                }
            }
        }
        // Sleep in small slices so cancel signals are honoured quickly.
        for _ in 0..10 {
            if cancel.load(Ordering::Acquire) {
                return;
            }
            thread::sleep(tick / 10);
            shutdown_check += tick / 10;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn detects_mtime_bump() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let target = root.join("sectorforge.toml");
        fs::write(target.as_std_path(), b"a = 1\n").unwrap();
        let mtime = fs::metadata(target.as_std_path())
            .unwrap()
            .modified()
            .unwrap();
        let mut baseline = BTreeMap::new();
        baseline.insert("sectorforge.toml".to_string(), mtime);

        let watcher = FileWatcher::spawn(root.clone(), baseline);
        // Sleep past the filesystem mtime resolution then rewrite.
        thread::sleep(Duration::from_millis(1200));
        let mut f = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(target.as_std_path())
            .unwrap();
        writeln!(f, "a = 2").unwrap();
        f.sync_all().unwrap();
        drop(f);

        // Give the watcher a couple of ticks.
        let mut seen = None;
        for _ in 0..30 {
            if let Some(ev) = watcher.try_recv() {
                seen = Some(ev);
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }
        let ev = seen.expect("watcher should report a change");
        assert_eq!(ev.rel_path, "sectorforge.toml");
    }
}
