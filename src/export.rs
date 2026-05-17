//! Sector export: JSON, Markdown, CSV, manifest. All file-creating code lives here.

use std::fs;

use camino::Utf8Path;

use crate::config::{OutputConfig, OutputFormat};
use crate::errors::SectorError;
use crate::sector_model::{GeneratedRoute, GeneratedSector, GeneratedSystem, GeneratedWorld};

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
                crate::bitmap::write_bitmap(sector, output_dir, bm.sector_scale)?;
                wrote_image = true;
            }
            OutputFormat::Bmp => {
                crate::bitmap::write_bmp(sector, output_dir, bm.sector_scale)?;
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

// ── Markdown ─────────────────────────────────────────────────────────────────

fn write_markdown(sector: &GeneratedSector, output_dir: &Utf8Path) -> Result<(), SectorError> {
    let mut s = String::new();
    s.push_str(&format!("# {} — {}\n\n", sector.id, sector.title));
    s.push_str(&format!("Seed: `{}`\n\n", sector.seed));
    s.push_str(&format!(
        "Generator: {} v{}\n\n",
        sector.generator_name, sector.generator_version
    ));
    s.push_str(&format!(
        "- **Sector size:** {}×{}\n",
        sector.width, sector.height
    ));
    s.push_str(&format!("- **Systems:** {}\n", sector.systems.len()));
    s.push_str(&format!(
        "- **Worlds:** {}\n",
        sector.systems.iter().map(|s| s.worlds.len()).sum::<usize>()
    ));
    s.push_str(&format!("- **Routes:** {}\n", sector.routes.len()));
    s.push_str(&format!("- **Factions:** {}\n\n", sector.factions.len()));

    s.push_str("## Sector map\n\n");
    s.push_str(&format_sector_map(sector));
    s.push('\n');

    s.push_str("## System index\n\n");
    s.push_str("| ID | Name | Coord | Star | Worlds |\n");
    s.push_str("|---|---|---|---|---:|\n");
    for sys in &sector.systems {
        s.push_str(&format!(
            "| {} | {} | (q={}, r={}) | {} / {} | {} |\n",
            sys.id,
            sys.name,
            sys.coord.q,
            sys.coord.r,
            sys.star.colour_code,
            sys.star.colour_name,
            sys.worlds.len()
        ));
    }
    s.push('\n');

    for sys in &sector.systems {
        s.push_str(&format_system_section(sys));
    }

    if !sector.routes.is_empty() {
        s.push_str("## Routes\n\n");
        s.push_str("| ID | From | To | Distance | Type | Stability |\n");
        s.push_str("|---|---|---|---:|---|---|\n");
        for r in &sector.routes {
            s.push_str(&format!(
                "| {} | {} | {} | {} | {:?} | {:?} |\n",
                r.id, r.from_system_id, r.to_system_id, r.distance, r.route_type, r.stability
            ));
        }
        s.push('\n');
    }

    if !sector.factions.is_empty() {
        s.push_str("## Factions\n\n");
        s.push_str("| ID | Name | Kind | Systems | Worlds |\n");
        s.push_str("|---|---|---|---:|---:|\n");
        for f in &sector.factions {
            s.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                f.id,
                f.name,
                f.kind,
                f.system_presence.len(),
                f.world_presence.len()
            ));
        }
        s.push('\n');
    }

    let path = output_dir.join("sector.md");
    fs::write(&path, s).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}

fn format_sector_map(sector: &GeneratedSector) -> String {
    use std::collections::HashMap;
    let mut at: HashMap<(i32, i32), &str> = HashMap::new();
    for s in &sector.systems {
        at.insert((s.coord.q, s.coord.r), s.star.colour_code.as_str());
    }
    let mut out = String::new();
    out.push_str("```\n");
    for r in 0..(sector.height as i32) {
        if r % 2 == 1 {
            out.push(' ');
        }
        for q in 0..(sector.width as i32) {
            match at.get(&(q, r)) {
                Some(code) => {
                    out.push_str(code);
                    out.push(' ');
                }
                None => out.push_str(". "),
            }
        }
        out.push('\n');
    }
    out.push_str("```\n");
    out
}

fn format_system_section(sys: &GeneratedSystem) -> String {
    let mut s = String::new();
    s.push_str(&format!("## {} — {}\n\n", sys.id.to_uppercase(), sys.name));
    s.push_str(&format!(
        "- **Coordinates:** q={}, r={}\n",
        sys.coord.q, sys.coord.r
    ));
    s.push_str(&format!(
        "- **Star:** {} / {} / {}\n",
        sys.star.colour_code,
        sys.star.colour_name,
        sys.star.spectral_type.as_deref().unwrap_or("?")
    ));
    if !sys.primary_factions.is_empty() {
        s.push_str(&format!(
            "- **Primary factions:** {}\n",
            sys.primary_factions.join(", ")
        ));
    }
    s.push('\n');

    if !sys.worlds.is_empty() {
        s.push_str(
            "| Orbit | World | Type | Atmosphere | Population | Tech | Government | Features |\n",
        );
        s.push_str("|---:|---|---|---|---|---|---|---|\n");
        for w in &sys.worlds {
            let features = w.world.notable_features.join("; ");
            s.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
                w.orbit,
                w.name,
                w.world.world_type,
                w.world.atmosphere,
                w.world.population,
                w.world.tech_level,
                w.world.government,
                features
            ));
        }
        s.push('\n');
    }
    s
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
    let _ = std::marker::PhantomData::<GeneratedWorld>;
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
        format!("\"{}\"", inner)
    } else {
        s.to_string()
    }
}
