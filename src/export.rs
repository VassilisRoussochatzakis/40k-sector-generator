//! Sector export: JSON, Markdown, manifest. All file-creating code lives here.

use std::fs;

use camino::Utf8Path;
use serde::Serialize;

use crate::config::{OutputConfig, OutputFormat};
use crate::errors::SectorError;
use crate::render;
use crate::sector_model::GeneratedSector;

/// How to render JSON. Replaces positional `pretty: bool` arguments so call
/// sites read self-documenting (`JsonFormat::Pretty` instead of `true`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonFormat {
    Pretty,
    Compact,
}

impl JsonFormat {
    fn render<T: Serialize>(self, value: &T) -> serde_json::Result<String> {
        match self {
            JsonFormat::Pretty => serde_json::to_string_pretty(value),
            JsonFormat::Compact => serde_json::to_string(value),
        }
    }

    /// Pick a format from an existing `OutputConfig::pretty_json` flag at the
    /// boundary so the bool stays in config-land only.
    pub(crate) fn from_flag(pretty: bool) -> Self {
        if pretty {
            Self::Pretty
        } else {
            Self::Compact
        }
    }
}

/// Shared writer for sub-system reports: create dir, write `<base_name>.md`
/// from `md`, then write `<base_name>.json` from a pretty serialization of
/// `json_payload`. Used by every `<module>::write_report` to eliminate the
/// near-identical 8-line bodies they used to carry.
pub(crate) fn write_md_and_json<T: Serialize>(
    output_dir: &Utf8Path,
    base_name: &str,
    md: &str,
    json_payload: &T,
) -> Result<(), SectorError> {
    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;
    let md_path = output_dir.join(format!("{base_name}.md"));
    fs::write(&md_path, md).map_err(|e| SectorError::io(md_path.as_str(), e))?;
    let json_path = output_dir.join(format!("{base_name}.json"));
    let json = serde_json::to_string_pretty(json_payload)
        .map_err(|e| SectorError::export(json_path.as_str(), e.to_string()))?;
    fs::write(&json_path, json).map_err(|e| SectorError::io(json_path.as_str(), e))
}

pub fn export_all(
    sector: &GeneratedSector,
    output_config: &OutputConfig,
    output_dir: &Utf8Path,
) -> Result<(), SectorError> {
    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;

    if output_config.write_manifest {
        let fmt = JsonFormat::from_flag(output_config.pretty_json);
        write_manifest(sector, output_dir, fmt)?;
        write_validation_placeholder(sector, output_dir, fmt)?;
    }

    let bm = &output_config.bitmap;
    let map_theme = crate::map_theme::resolve_map_theme(&bm.theme)
        .map_err(|e| SectorError::InvalidConfig(format!("outputs.bitmap.theme: {e}")))?;
    let mut wrote_image = false;
    for fmt in &output_config.formats {
        match fmt {
            OutputFormat::Json => write_json(sector, output_dir, output_config)?,
            OutputFormat::Markdown => write_markdown(sector, output_dir)?,
            OutputFormat::Bitmap => {
                let opts = crate::bitmap::RenderOptions {
                    faction_fill: bm.faction_fill,
                    heatmap: bm.heatmap,
                    theme: map_theme.clone(),
                    route_view_mode: crate::sector_model::RouteViewMode::default(),
                };
                crate::bitmap::write_bitmap_with(sector, output_dir, bm.sector_scale, None, opts)?;
                wrote_image = true;
            }
            OutputFormat::Html => {
                crate::html_export::write_html(sector, output_dir, &output_config.html)?;
            }
        }
    }

    // System maps are written whenever any image format is enabled and
    // `bitmap.render_systems` is on.
    if wrote_image && bm.render_systems {
        let sys_opts = crate::system_map::SystemRenderOptions {
            faction_fill: bm.faction_fill,
            theme: map_theme,
        };
        crate::system_map::write_system_maps(sector, output_dir, bm.system_scale, sys_opts)?;
    }

    Ok(())
}

fn write_json(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    cfg: &OutputConfig,
) -> Result<(), SectorError> {
    let format = JsonFormat::from_flag(cfg.pretty_json);
    write_sector_json_file(sector, output_dir, format)?;

    if cfg.write_per_system_files {
        write_per_system_json_files(sector, output_dir, format)?;
    } else {
        remove_per_system_json_files(sector, output_dir)?;
    }
    Ok(())
}

fn write_sector_json_file(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    format: JsonFormat,
) -> Result<(), SectorError> {
    let sector_path = output_dir.join("sector.json");
    let text = format
        .render(sector)
        .map_err(|e| SectorError::export(sector_path.as_str(), e.to_string()))?;
    fs::write(&sector_path, text).map_err(|e| SectorError::io(sector_path.as_str(), e))?;
    Ok(())
}

fn write_per_system_json_files(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    format: JsonFormat,
) -> Result<(), SectorError> {
    let systems_dir = output_dir.join("systems");
    fs::create_dir_all(&systems_dir).map_err(|e| SectorError::io(systems_dir.as_str(), e))?;
    for sys in &sector.systems {
        let path = systems_dir.join(format!("{}.json", sys.id));
        let text = format
            .render(sys)
            .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
        fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
    }
    Ok(())
}

fn remove_per_system_json_files(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
) -> Result<(), SectorError> {
    let systems_dir = output_dir.join("systems");
    for sys in &sector.systems {
        let path = systems_dir.join(format!("{}.json", sys.id));
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(SectorError::io(path.as_str(), e)),
        }
    }
    Ok(())
}

fn write_manifest(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    format: JsonFormat,
) -> Result<(), SectorError> {
    let path = output_dir.join("manifest.json");
    let text = format
        .render(&sector.manifest)
        .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}

fn write_validation_placeholder(
    _sector: &GeneratedSector,
    output_dir: &Utf8Path,
    format: JsonFormat,
) -> Result<(), SectorError> {
    // Validation is run before generation; we just record that it succeeded.
    let path = output_dir.join("validation_report.json");
    let stub = serde_json::json!({
        "ok": true,
        "note": "validation was run successfully before generation",
    });
    let text = format
        .render(&stub)
        .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}

/// Export the canonical `sector.json` to the given directory.
/// Per-system JSON files duplicate `sector.json.systems[]`; use a full
/// [`OutputConfig`] with `write_per_system_files = true` when callers need
/// those convenience files.
pub fn export_json(sector: &GeneratedSector, output_dir: &Utf8Path) -> Result<(), SectorError> {
    write_sector_json_file(sector, output_dir, JsonFormat::Pretty)?;
    remove_per_system_json_files(sector, output_dir)
}

/// Export everything for the sector EXCEPT images: sector JSON,
/// manifest, validation, markdown, plus a recursive copy
/// of the source data folder. Files with image extensions
/// (.png/.bmp/.jpg/.jpeg/.gif/.webp/.tiff/.tif/.svg/.ico) are skipped during
/// the data-folder copy.
pub fn export_bundle(
    sector: &GeneratedSector,
    data_dir: Option<&Utf8Path>,
    output_dir: &Utf8Path,
) -> Result<(), SectorError> {
    let sector_dir = output_dir.join(sanitize_dir_name(&sector.id));
    fs::create_dir_all(&sector_dir).map_err(|e| SectorError::io(sector_dir.as_str(), e))?;

    let out_dir = sector_dir.join("out");
    fs::create_dir_all(&out_dir).map_err(|e| SectorError::io(out_dir.as_str(), e))?;

    export_json(sector, &out_dir)?;
    write_manifest(sector, &out_dir, JsonFormat::Pretty)?;
    write_validation_placeholder(sector, &out_dir, JsonFormat::Pretty)?;
    write_markdown(sector, &out_dir)?;

    if let Some(src) = data_dir {
        let dest = sector_dir.join("data");
        copy_dir_filtered(src, &dest)?;
    }
    Ok(())
}

fn sanitize_dir_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "sector".to_string()
    } else {
        cleaned
    }
}

fn copy_dir_filtered(src: &Utf8Path, dest: &Utf8Path) -> Result<(), SectorError> {
    fs::create_dir_all(dest).map_err(|e| SectorError::io(dest.as_str(), e))?;
    let entries = fs::read_dir(src).map_err(|e| SectorError::io(src.as_str(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| SectorError::io(src.as_str(), e))?;
        let file_name = entry.file_name();
        let name = match file_name.to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let src_path = src.join(&name);
        let dest_path = dest.join(&name);
        let file_type = entry
            .file_type()
            .map_err(|e| SectorError::io(src_path.as_str(), e))?;
        if file_type.is_dir() {
            copy_dir_filtered(&src_path, &dest_path)?;
        } else if file_type.is_file() {
            if is_image_path(&src_path) {
                continue;
            }
            fs::copy(&src_path, &dest_path).map_err(|e| SectorError::io(src_path.as_str(), e))?;
        }
    }
    Ok(())
}

fn is_image_path(path: &Utf8Path) -> bool {
    match path.extension() {
        Some(ext) => matches!(
            ext.to_ascii_lowercase().as_str(),
            "png" | "bmp" | "jpg" | "jpeg" | "gif" | "webp" | "tiff" | "tif" | "svg" | "ico"
        ),
        None => false,
    }
}

// ── Markdown ─────────────────────────────────────────────────────────────────

fn write_markdown(sector: &GeneratedSector, output_dir: &Utf8Path) -> Result<(), SectorError> {
    let text = render::render_sector_markdown(sector);
    let path = output_dir.join("sector.md");
    fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}
