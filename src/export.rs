//! Sector export: JSON, Markdown, CSV, manifest. All file-creating code lives here.

use std::fs;

use camino::Utf8Path;

use crate::config::{OutputConfig, OutputFormat};
use crate::errors::SectorError;
use crate::render;
use crate::sector_model::{GeneratedRoute, GeneratedSector};

pub fn export_all(
    sector: &GeneratedSector,
    output_config: &OutputConfig,
    output_dir: &Utf8Path,
) -> Result<(), SectorError> {
    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;

    if output_config.write_manifest {
        write_manifest(sector, output_dir, output_config.pretty_json)?;
        write_validation_placeholder(sector, output_dir, output_config.pretty_json)?;
    }

    let bm = &output_config.bitmap;
    let mut wrote_image = false;
    for fmt in &output_config.formats {
        match fmt {
            OutputFormat::Json => write_json(sector, output_dir, output_config)?,
            OutputFormat::Markdown => write_markdown(sector, output_dir)?,
            OutputFormat::Csv => write_csv(sector, output_dir)?,
            OutputFormat::Bitmap => {
                let opts = crate::bitmap::RenderOptions {
                    faction_fill: bm.faction_fill,
                    heatmap: bm.heatmap,
                };
                crate::bitmap::write_bitmap_with(sector, output_dir, bm.sector_scale, None, opts)?;
                wrote_image = true;
            }
        }
    }

    // System maps are written whenever any image format is enabled and
    // `bitmap.render_systems` is on.
    if wrote_image && bm.render_systems {
        crate::system_map::write_system_maps(sector, output_dir, bm.system_scale)?;
    }

    Ok(())
}

fn write_json(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    cfg: &OutputConfig,
) -> Result<(), SectorError> {
    let sector_path = output_dir.join("sector.json");
    let text = if cfg.pretty_json {
        serde_json::to_string_pretty(sector)
    } else {
        serde_json::to_string(sector)
    }
    .map_err(|e| SectorError::export(sector_path.as_str(), e.to_string()))?;
    fs::write(&sector_path, text).map_err(|e| SectorError::io(sector_path.as_str(), e))?;

    if cfg.write_per_system_files {
        let systems_dir = output_dir.join("systems");
        fs::create_dir_all(&systems_dir).map_err(|e| SectorError::io(systems_dir.as_str(), e))?;
        for sys in &sector.systems {
            let path = systems_dir.join(format!("{}.json", sys.id));
            let text = if cfg.pretty_json {
                serde_json::to_string_pretty(sys)
            } else {
                serde_json::to_string(sys)
            }
            .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
            fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
        }
    }
    Ok(())
}

fn write_manifest(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    pretty: bool,
) -> Result<(), SectorError> {
    let path = output_dir.join("manifest.json");
    let text = if pretty {
        serde_json::to_string_pretty(&sector.manifest)
    } else {
        serde_json::to_string(&sector.manifest)
    }
    .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}

fn write_validation_placeholder(
    _sector: &GeneratedSector,
    output_dir: &Utf8Path,
    pretty: bool,
) -> Result<(), SectorError> {
    // Validation is run before generation; we just record that it succeeded.
    let path = output_dir.join("validation_report.json");
    let stub = serde_json::json!({
        "ok": true,
        "note": "validation was run successfully before generation",
    });
    let text = if pretty {
        serde_json::to_string_pretty(&stub)
    } else {
        serde_json::to_string(&stub)
    }
    .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}

/// Export sector.json + systems/*.json to the given directory.
/// Simpler than `write_json` — always writes per-system files.
pub fn export_json(sector: &GeneratedSector, output_dir: &Utf8Path) -> Result<(), SectorError> {
    let sector_path = output_dir.join("sector.json");
    let text = serde_json::to_string_pretty(sector)
        .map_err(|e| SectorError::export(sector_path.as_str(), e.to_string()))?;
    fs::write(&sector_path, text).map_err(|e| SectorError::io(sector_path.as_str(), e))?;

    let systems_dir = output_dir.join("systems");
    fs::create_dir_all(&systems_dir).map_err(|e| SectorError::io(systems_dir.as_str(), e))?;
    for sys in &sector.systems {
        let path = systems_dir.join(format!("{}.json", sys.id));
        let text = serde_json::to_string_pretty(sys)
            .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
        fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
    }
    Ok(())
}

/// Export everything for the sector EXCEPT images: all JSONs (sector,
/// manifest, validation, per-system), markdown, CSVs, plus a recursive copy
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
    write_manifest(sector, &out_dir, true)?;
    write_validation_placeholder(sector, &out_dir, true)?;
    write_markdown(sector, &out_dir)?;
    write_csv(sector, &out_dir)?;

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

// ── CSV ───────────────────────────────────────────────────────────────────────

fn write_csv(sector: &GeneratedSector, output_dir: &Utf8Path) -> Result<(), SectorError> {
    let csv_dir = output_dir.join("csv");
    fs::create_dir_all(&csv_dir).map_err(|e| SectorError::io(csv_dir.as_str(), e))?;
    write_systems_csv(sector, &csv_dir)?;
    write_worlds_csv(sector, &csv_dir)?;
    write_routes_csv(&sector.routes, &csv_dir)?;
    Ok(())
}

fn write_systems_csv(sector: &GeneratedSector, dir: &Utf8Path) -> Result<(), SectorError> {
    let path = dir.join("systems.csv");
    let mut s = String::new();
    s.push_str("id,index,name,q,r,star_colour_code,star_colour_name,spectral_type,world_count,primary_factions,tags\n");
    for sys in &sector.systems {
        s.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{}\n",
            csv_escape(&sys.id),
            sys.index,
            csv_escape(&sys.name),
            sys.coord.q,
            sys.coord.r,
            csv_escape(&sys.star.colour_code),
            csv_escape(&sys.star.colour_name),
            csv_escape(sys.star.spectral_type.as_deref().unwrap_or("")),
            sys.worlds.len(),
            csv_escape(&sys.primary_factions.join(";")),
            csv_escape(&sys.tags.join(";")),
        ));
    }
    fs::write(&path, s).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}

fn write_worlds_csv(sector: &GeneratedSector, dir: &Utf8Path) -> Result<(), SectorError> {
    let path = dir.join("worlds.csv");
    let mut s = String::new();
    s.push_str("id,system_id,index,name,orbit,source_row_index,star_colour_code,world_type,atmosphere,temperature,biosphere,population,tech_level,government,notable_features,factions,tags\n");
    for sys in &sector.systems {
        for w in &sys.worlds {
            let factions: Vec<String> = w.factions.iter().map(|f| f.faction_id.clone()).collect();
            s.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                csv_escape(&w.id),
                csv_escape(&sys.id),
                w.index,
                csv_escape(&w.name),
                w.orbit,
                w.source_row_index,
                csv_escape(&w.world.star_colour_code),
                csv_escape(&w.world.world_type),
                csv_escape(&w.world.atmosphere),
                csv_escape(&w.world.temperature),
                csv_escape(&w.world.biosphere),
                csv_escape(&w.world.population),
                csv_escape(&w.world.tech_level),
                csv_escape(&w.world.government),
                csv_escape(&w.world.notable_features.join(";")),
                csv_escape(&factions.join(";")),
                csv_escape(&w.tags.join(";")),
            ));
        }
    }
    fs::write(&path, s).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}

fn write_routes_csv(routes: &[GeneratedRoute], dir: &Utf8Path) -> Result<(), SectorError> {
    let path = dir.join("routes.csv");
    let mut s = String::new();
    s.push_str("id,from_system_id,to_system_id,distance,route_type,stability,tags\n");
    for r in routes {
        s.push_str(&format!(
            "{},{},{},{},{:?},{:?},{}\n",
            csv_escape(&r.id),
            csv_escape(&r.from_system_id),
            csv_escape(&r.to_system_id),
            r.distance,
            r.route_type,
            r.stability,
            csv_escape(&r.tags.join(";")),
        ));
    }
    fs::write(&path, s).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let inner = s.replace('"', "\"\"");
        format!("\"{inner}\"")
    } else {
        s.to_string()
    }
}
