//! Disk operations for the editor: list projects under `examples/`, load and
//! save sector JSON.

use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum EditorFileError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid project name: {0}")]
    InvalidProjectName(String),
}

const EXAMPLES_DIR: &str = "examples";

/// Discover project directories that contain `out/sector.json`.
pub(crate) fn list_projects() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(EXAMPLES_DIR) else {
        return out;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let json = entry.path().join("out").join("sector.json");
        if json.exists() {
            if let Some(name) = entry.file_name().to_str() {
                out.push(name.to_string());
            }
        }
    }
    out.sort_unstable();
    out
}

pub(crate) fn project_sector_path(name: &str) -> PathBuf {
    PathBuf::from(EXAMPLES_DIR)
        .join(name)
        .join("out")
        .join("sector.json")
}

pub(crate) fn load_project_sector(
    name: &str,
) -> Result<
    (
        sectorforge::sector_model::GeneratedSector,
        Option<sectorforge::input::ProjectInput>,
        String,
    ),
    EditorFileError,
> {
    let path = project_sector_path(name);
    let text = fs::read_to_string(&path)?;
    let sector: sectorforge::sector_model::GeneratedSector = serde_json::from_str(&text)?;

    let project_root = PathBuf::from(EXAMPLES_DIR).join(name);
    let mut input = None;
    if let Ok(utf8_root) = camino::Utf8PathBuf::from_path_buf(project_root) {
        if let Ok(pi) = sectorforge::input::load_project(&utf8_root) {
            input = Some(pi);
        }
    }

    Ok((sector, input, path.to_string_lossy().to_string()))
}

pub(crate) fn save_project_sector(
    name: &str,
    sector: &sectorforge::sector_model::GeneratedSector,
) -> Result<String, EditorFileError> {
    if name.trim().is_empty() {
        return Err(EditorFileError::InvalidProjectName(
            "project name is empty".to_string(),
        ));
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(EditorFileError::InvalidProjectName(
            "project name contains forbidden characters".to_string(),
        ));
    }
    let path = project_sector_path(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(sector)?;
    fs::write(&path, text)?;
    Ok(path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::{empty_sector, GeneratedSector};
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    // The process CWD is global and shared across all tests in the binary.
    // `list_projects` / `load_project_sector` / `save_project_sector` resolve
    // `examples/` RELATIVE to CWD, so any test that drives the happy path must
    // `set_current_dir` into a private scratch dir. Two such tests running
    // concurrently would corrupt each other, so every CWD-mutating test holds
    // this lock for its whole body. (`unwrap_or_else(into_inner)` tolerates a
    // poisoned lock from an unrelated panicking test so failures don't cascade.)
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    // Unique scratch dir under the OS temp root, removed on drop. (Viewer has no
    // `tempfile` dev-dependency, so this is hand-rolled.)
    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new(tag: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!(
                "sectorforge-viewer-{tag}-{}-{n}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            ScratchDir(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // RAII CWD guard: capture current dir, switch to `new`, restore on drop
    // (restores even on panic, so the lock-holder always leaves CWD as it found
    // it for the next lock-holder).
    struct CwdGuard {
        prev: PathBuf,
    }

    impl CwdGuard {
        fn enter(new: &Path) -> Self {
            let prev = std::env::current_dir().expect("cwd readable");
            std::env::set_current_dir(new).expect("set_current_dir into scratch");
            CwdGuard { prev }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.prev);
        }
    }

    // Gap 222: name guard — empty / contains-separator / dot-dot all rejected
    // with InvalidProjectName. These take the early-return path and touch NO
    // disk, so they need neither the scratch dir nor the CWD lock.
    #[test]
    fn save_project_sector_rejects_empty_name() {
        let sector = empty_sector("p", "P", "s", 8, 8);
        let err = save_project_sector("", &sector).unwrap_err();
        assert!(matches!(err, EditorFileError::InvalidProjectName(_)));
        let msg = err.to_string();
        assert!(msg.contains("invalid project name"));
        assert!(msg.contains("empty"));
    }

    #[test]
    fn save_project_sector_rejects_forbidden_chars() {
        let sector = empty_sector("p", "P", "s", 8, 8);
        for name in ["a/b", "a\\b", ".", ".."] {
            let err = save_project_sector(name, &sector).unwrap_err();
            assert!(
                matches!(err, EditorFileError::InvalidProjectName(_)),
                "name {name:?} should be rejected"
            );
            let msg = err.to_string();
            assert!(msg.starts_with("invalid project name: "), "msg was {msg:?}");
            assert!(msg.contains("forbidden"), "msg was {msg:?}");
        }
    }

    // Gaps 222 (happy path) + 228 (list_projects + load_project_sector +
    // save→load round-trip). All CWD-relative I/O lives in ONE locked test so
    // there is exactly one CWD-holding code path — no inter-test CWD race.
    #[test]
    fn save_list_load_round_trip_under_examples() {
        let _lock = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let scratch = ScratchDir::new("examples-cwd");
        let _cwd = CwdGuard::enter(scratch.path());

        // --- discovery filtering: alpha/beta valid, gamma has no out/sector.json ---
        let ex = scratch.path().join(EXAMPLES_DIR);
        for name in ["alpha", "beta"] {
            let out = ex.join(name).join("out");
            fs::create_dir_all(&out).unwrap();
            let text =
                serde_json::to_string_pretty(&empty_sector(name, name, "s", 8, 8)).unwrap();
            fs::write(out.join("sector.json"), text).unwrap();
        }
        fs::create_dir_all(ex.join("gamma")).unwrap(); // no out/sector.json -> skipped

        let projects = list_projects();
        assert_eq!(projects, vec!["alpha".to_string(), "beta".to_string()]);

        // --- load an existing project ---
        let (sector, _input, path) = load_project_sector("alpha").unwrap();
        assert_eq!(&*sector.id, "alpha");
        assert!(path.ends_with("examples/alpha/out/sector.json"), "path was {path}");

        // --- happy-path save writes parseable JSON under examples/<name> ---
        let saved = save_project_sector("delta", &empty_sector("delta", "D", "s", 8, 8)).unwrap();
        assert!(Path::new(&saved).exists());
        let text = fs::read_to_string(&saved).unwrap();
        assert!(serde_json::from_str::<GeneratedSector>(&text).is_ok());

        // --- save → list → load round-trip: delta now discoverable + loadable ---
        let projects = list_projects();
        assert!(projects.contains(&"delta".to_string()));
        let (delta, _input, _path) = load_project_sector("delta").unwrap();
        assert_eq!(&*delta.id, "delta");
    }
}
