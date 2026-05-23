//! Scenario presets / template library (§9 NEW.md).
//!
//! Pure-data overlays: each preset is a directory under `presets/<name>/`
//! containing a complete project tree (`sectorforge.toml` + `data/`). The
//! scaffolder copies the tree into a fresh destination, optionally overriding
//! the seed. Determinism is preserved — presets only feed inputs.

use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;

/// One line per preset in `sectorforge list-presets` output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetEntry {
    pub id: String,
    pub description: String,
    pub title: String,
}

/// `preset.toml` schema — co-located with each preset's project tree.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PresetMeta {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Optional human-readable tag list.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Suggested seed when not overridden by `--seed`.
    #[serde(default)]
    pub default_seed: Option<String>,
    /// Optional path (relative to `presets_dir`) of a base project tree to
    /// overlay onto. The preset's own files take precedence on overlap. Lets
    /// thin variant presets reuse one shared data tree.
    #[serde(default)]
    pub inherits: Option<String>,
}

/// Walk `presets_dir`, returning one entry per immediate sub-directory that
/// contains a `sectorforge.toml`. Stable alphabetical order.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if `presets_dir` cannot be read. Sub-directories
/// without a `sectorforge.toml` are skipped silently.
pub fn list(presets_dir: &Utf8Path) -> Result<Vec<PresetEntry>, SectorError> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(presets_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(SectorError::io(presets_dir.as_str(), e)),
    };
    for entry in entries {
        let entry = entry.map_err(|e| SectorError::io(presets_dir.as_str(), e))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        // Hide internal bundles by convention (e.g. `_base`).
        if name.starts_with('_') {
            continue;
        }
        let meta = read_meta(&presets_dir.join(&name))?;
        if !path.join("sectorforge.toml").exists() && meta.inherits.is_none() {
            continue;
        }
        out.push(PresetEntry {
            id: name.clone(),
            description: meta
                .description
                .unwrap_or_else(|| "(no description)".to_string()),
            title: meta.title.unwrap_or_else(|| name.clone()),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn read_meta(preset_dir: &Utf8Path) -> Result<PresetMeta, SectorError> {
    let meta_path = preset_dir.join("preset.toml");
    if !meta_path.exists() {
        return Ok(PresetMeta::default());
    }
    let text = fs::read_to_string(meta_path.as_path())
        .map_err(|e| SectorError::io(meta_path.as_str(), e))?;
    toml::from_str::<PresetMeta>(&text)
        .map_err(|e| SectorError::config_parse(meta_path.as_str(), e.to_string()))
}

/// Materialise `<presets_dir>/<preset>` into `<dest>`. The destination must
/// not already exist. If `seed_override` is provided, also rewrites the
/// `seed = "..."` line of the destination `sectorforge.toml`.
///
/// `preset.toml` is intentionally NOT copied — it is metadata for the gallery
/// only, not part of the generated project.
///
/// # Errors
///
/// Returns [`SectorError::InvalidConfig`] when the preset is missing, the
/// destination exists, or copying produces an invalid project tree.
/// [`SectorError::Io`] for filesystem failures.
pub fn scaffold(
    presets_dir: &Utf8Path,
    preset_id: &str,
    dest: &Utf8Path,
    seed_override: Option<&str>,
) -> Result<(), SectorError> {
    let src = presets_dir.join(preset_id);
    let meta = read_meta(&src)?;
    let has_own_toml = src.join("sectorforge.toml").exists();
    if !src.is_dir() {
        return Err(SectorError::InvalidConfig(format!(
            "preset '{preset_id}' not found under {presets_dir} (expected {src}/)"
        )));
    }
    if !has_own_toml && meta.inherits.is_none() {
        return Err(SectorError::InvalidConfig(format!(
            "preset '{preset_id}' has neither a sectorforge.toml nor an `inherits` base"
        )));
    }
    if dest.exists() {
        return Err(SectorError::InvalidConfig(format!(
            "destination {dest} already exists; choose a fresh path"
        )));
    }
    fs::create_dir_all(dest).map_err(|e| SectorError::io(dest.as_str(), e))?;

    if let Some(base_rel) = &meta.inherits {
        let base = presets_dir.join(base_rel);
        if !base.is_dir() {
            return Err(SectorError::InvalidConfig(format!(
                "preset '{preset_id}' inherits from '{base_rel}' but that path does not exist"
            )));
        }
        copy_dir_recursive(base.as_path(), dest, &|name| name != "preset.toml")?;
    }
    copy_dir_recursive(src.as_path(), dest, &|name| name != "preset.toml")?;

    if !dest.join("sectorforge.toml").exists() {
        return Err(SectorError::InvalidConfig(format!(
            "preset '{preset_id}' produced no sectorforge.toml after overlay; check `inherits`"
        )));
    }

    if let Some(seed) = seed_override {
        let toml_path = dest.join("sectorforge.toml");
        let text = fs::read_to_string(toml_path.as_path())
            .map_err(|e| SectorError::io(toml_path.as_str(), e))?;
        let rewritten = rewrite_seed(&text, seed);
        fs::write(toml_path.as_path(), rewritten)
            .map_err(|e| SectorError::io(toml_path.as_str(), e))?;
    }
    Ok(())
}

fn copy_dir_recursive<F>(src: &Utf8Path, dest: &Utf8Path, accept: &F) -> Result<(), SectorError>
where
    F: Fn(&str) -> bool,
{
    fs::create_dir_all(dest).map_err(|e| SectorError::io(dest.as_str(), e))?;
    let entries = fs::read_dir(src).map_err(|e| SectorError::io(src.as_str(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| SectorError::io(src.as_str(), e))?;
        let name = match entry.file_name().to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        if !accept(&name) {
            continue;
        }
        let src_path = src.join(&name);
        let dest_path = dest.join(&name);
        let file_type = entry
            .file_type()
            .map_err(|e| SectorError::io(src_path.as_str(), e))?;
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dest_path, accept)?;
        } else if file_type.is_file() {
            fs::copy(src_path.as_path(), dest_path.as_path())
                .map_err(|e| SectorError::io(src_path.as_str(), e))?;
        }
    }
    Ok(())
}

/// Replace the first top-level `seed = "..."` (in the `[generation]` section)
/// with the given value. Falls back to leaving the file unchanged if no match.
fn rewrite_seed(text: &str, new_seed: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_generation = false;
    let mut replaced = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_generation = trimmed.starts_with("[generation]");
        }
        if !replaced && in_generation && trimmed.starts_with("seed") {
            // Preserve leading indent.
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            out.push_str(indent);
            out.push_str(&format!("seed = \"{new_seed}\"\n"));
            replaced = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Read every `<presets_dir>/<id>/preset.toml` into a single map. Useful for
/// the GUI gallery which needs metadata for every preset up front.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if `presets_dir` cannot be read.
pub fn read_all_meta(presets_dir: &Utf8Path) -> Result<BTreeMap<String, PresetMeta>, SectorError> {
    let mut out = BTreeMap::new();
    let entries = match fs::read_dir(presets_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(SectorError::io(presets_dir.as_str(), e)),
    };
    for entry in entries {
        let entry = entry.map_err(|e| SectorError::io(presets_dir.as_str(), e))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let utf8 = match Utf8PathBuf::from_path_buf(path) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let meta = read_meta(&utf8)?;
        out.insert(name, meta);
    }
    Ok(out)
}

/// Convenience: resolve a preset directory path under `presets_dir`.
#[must_use]
pub fn preset_path(presets_dir: &Utf8Path, preset_id: &str) -> Utf8PathBuf {
    presets_dir.join(preset_id)
}

/// Default presets directory used when callers do not supply one explicitly.
/// Resolves first to `./presets`, falling back to a path next to the current
/// executable. Used by [`scaffold_to_dir`] and the GUI builder wizard
/// (BUILDER_REQS §P1).
#[must_use]
pub fn default_presets_dir() -> Utf8PathBuf {
    let local = Utf8PathBuf::from("presets");
    if local.is_dir() {
        return local;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if let Ok(p) = Utf8PathBuf::from_path_buf(parent.join("presets")) {
                if p.is_dir() {
                    return p;
                }
            }
        }
    }
    local
}

/// Thin wrapper over [`scaffold`] that resolves the presets directory via
/// [`default_presets_dir`]. Per BUILDER_REQS §P1 the GUI builder wizard uses
/// this helper so callers don't have to thread the presets path through
/// every entry point.
///
/// # Errors
///
/// Forwards every error from [`scaffold`].
pub fn scaffold_to_dir(
    preset_id: &str,
    dest: &Utf8Path,
    seed_override: Option<&str>,
) -> Result<(), SectorError> {
    let presets_dir = default_presets_dir();
    scaffold(&presets_dir, preset_id, dest, seed_override)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn td() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("tempdir")
    }

    #[test]
    fn rewrite_seed_replaces_first_match_in_generation() {
        let text = "[project]\nseed = \"ignored\"\n[generation]\nseed = \"old\"\nfoo = 1\n";
        let out = rewrite_seed(text, "new");
        assert!(out.contains("[project]\nseed = \"ignored\""));
        assert!(out.contains("[generation]\nseed = \"new\""));
        assert!(!out.contains("\"old\""));
    }

    #[test]
    fn rewrite_seed_no_match_passes_through() {
        let text = "[project]\nid = \"x\"\n";
        let out = rewrite_seed(text, "ignored");
        assert!(out.contains("id = \"x\""));
    }

    #[test]
    fn list_returns_empty_when_missing() {
        let entries =
            list(Utf8Path::new("/nonexistent/path/that/should/not/be/there")).expect("ok");
        assert!(entries.is_empty());
    }

    #[test]
    fn scaffold_refuses_to_overwrite() {
        let dir = td();
        let presets = Utf8PathBuf::from_path_buf(dir.path().join("presets")).unwrap();
        let preset = presets.join("p1");
        fs::create_dir_all(&preset).unwrap();
        fs::write(preset.join("sectorforge.toml"), "[project]\nid=\"p1\"\n").unwrap();
        let dest = Utf8PathBuf::from_path_buf(dir.path().join("existing")).unwrap();
        fs::create_dir_all(&dest).unwrap();
        let err = scaffold(&presets, "p1", &dest, None).unwrap_err();
        match err {
            SectorError::InvalidConfig(_) => {}
            e => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn scaffold_copies_tree_and_skips_preset_toml() {
        let dir = td();
        let presets = Utf8PathBuf::from_path_buf(dir.path().join("presets")).unwrap();
        let preset = presets.join("p1");
        fs::create_dir_all(preset.join("data/sub")).unwrap();
        fs::write(
            preset.join("sectorforge.toml"),
            "[project]\nid=\"p1\"\n[generation]\nseed = \"old\"\n",
        )
        .unwrap();
        fs::write(preset.join("preset.toml"), "title = \"P1\"\n").unwrap();
        fs::write(preset.join("data/sub/x.toml"), "a = 1\n").unwrap();
        let dest = Utf8PathBuf::from_path_buf(dir.path().join("dst")).unwrap();
        scaffold(&presets, "p1", &dest, Some("new")).unwrap();
        let toml = fs::read_to_string(dest.join("sectorforge.toml").as_path()).unwrap();
        assert!(toml.contains("seed = \"new\""));
        assert!(dest.join("data/sub/x.toml").exists());
        assert!(!dest.join("preset.toml").exists());
    }
}
