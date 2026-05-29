//! `generate` and `generate-system`.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use sectorforge::sector_model::HexCoord;

use super::common::{
    log_progress, log_sector_progress, parse_heatmap, print_validation_report, to_json_pretty,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_generate(
    project: Utf8PathBuf,
    seed: Option<String>,
    out: Option<Utf8PathBuf>,
    allow_warnings: bool,
    heatmap: Option<String>,
    no_faction_fill: bool,
    theme: Option<String>,
    constraints: Option<Utf8PathBuf>,
    max_candidates: Option<u32>,
    formats: Option<Vec<String>>,
) -> Result<ExitCode, sectorforge::SectorError> {
    log_progress(format_args!("sector: loading project {project}"));
    let mut input = sectorforge::load_project(&project)?;

    // §15 NEW2.md: run constraint search if requested.
    if let Some(c_path) = constraints {
        log_progress(format_args!(
            "sector: constraint search requested from {c_path}"
        ));
        let mut wishes = sectorforge::search::load_wishes(&c_path)?;
        if let Some(budget) = max_candidates {
            wishes.search.budget = budget;
        }
        if let Some(s) = seed.clone() {
            wishes.search.base_seed = Some(s);
        }

        let outcome = sectorforge::run_seed_search(&input, &wishes)?;
        if !outcome.preflight_errors.is_empty() {
            for e in &outcome.preflight_errors {
                eprintln!("preflight: {e}");
            }
            return Ok(ExitCode::from(1));
        }

        if let Some(winning) = outcome.winning {
            log_progress(format_args!(
                "sector: found seed satisfying constraints: {}",
                winning.seed
            ));
            input.config.generation.seed = winning.seed;

            // §15 NEW2.md: record search metadata in config for manifest.
            input.config.generation.search_base_seed = Some(outcome.base_seed);
            input.config.generation.search_candidate_index = Some(winning.n);
            let constraints_text = std::fs::read_to_string(&c_path).ok();
            if let Some(text) = constraints_text {
                input.config.generation.search_constraints_digest = Some(format!(
                    "blake3:{}",
                    sectorforge::rng::hex(blake3::hash(text.as_bytes()).as_bytes())
                ));
            }
        } else {
            eprintln!(
                "error: no candidate seed satisfied constraints from {c_path} within budget of {}",
                wishes.search.budget
            );
            if let Some(best) = outcome.near_misses.first() {
                eprintln!(
                    "best near-miss: {} (total miss: {:.3})",
                    best.seed, best.total_miss
                );
            }
            return Ok(ExitCode::from(1));
        }
    } else if let Some(s) = seed {
        input.config.generation.seed = s;
    }

    if let Some(h) = heatmap.as_deref() {
        input.config.outputs.bitmap.heatmap = parse_heatmap(h)?;
    }
    if no_faction_fill {
        input.config.outputs.bitmap.faction_fill = false;
    }
    if let Some(t) = theme {
        input.config.outputs.bitmap.theme.name = Some(t);
    }
    if let Some(tokens) = formats {
        let mut parsed = Vec::with_capacity(tokens.len());
        for tok in &tokens {
            let Some(fmt) = sectorforge::config::OutputFormat::parse_token(tok) else {
                return Err(sectorforge::SectorError::InvalidConfig(format!(
                    "unknown format '{tok}' (use json, markdown, png, svg, or html)"
                )));
            };
            if !parsed.contains(&fmt) {
                parsed.push(fmt);
            }
        }
        input.config.outputs.formats = parsed;
    }
    log_progress("sector: validating inputs");
    let report = sectorforge::validate_project(&input)?;
    if !report.ok {
        print_validation_report(&report);
        return Err(sectorforge::SectorError::ValidationFailed {
            error_count: report.errors.len(),
            warning_count: report.warnings.len(),
        });
    }
    if !allow_warnings && !report.warnings.is_empty() {
        eprintln!(
            "validation produced {} warning(s); rerun with --allow-warnings or fix them",
            report.warnings.len()
        );
        print_validation_report(&report);
        return Err(sectorforge::SectorError::ValidationFailed {
            error_count: 0,
            warning_count: report.warnings.len(),
        });
    }
    log_progress(format_args!(
        "sector: validation OK ({} warning(s), {} usable world candidates)",
        report.warnings.len(),
        report.world_workbook.usable_candidate_count
    ));

    let output_dir = out.unwrap_or_else(|| project.join(&input.config.outputs.directory));
    let output_cfg = input.config.outputs.clone();
    let project_id = input.config.project.id.clone();
    log_progress(format_args!("sector: generating '{project_id}'"));
    let sector = sectorforge::generate_sector_with_progress(input, log_sector_progress)?;

    // Spec §11.11: check invariants before writing.
    log_progress("sector: checking post-generation invariants");
    let inv = sectorforge::validate_sector(&sector);
    if !inv.ok {
        eprintln!(
            "post-generation invariants failed ({} violation(s)):",
            inv.violations.len()
        );
        for v in &inv.violations {
            eprintln!("  {} {}", v.code, v.message);
        }
        return Err(sectorforge::SectorError::ValidationFailed {
            error_count: inv.violations.len(),
            warning_count: 0,
        });
    }

    log_progress(format_args!("sector: exporting to {output_dir}"));
    sectorforge::export_sector(&sector, &output_cfg, &output_dir)?;
    log_progress("sector: export complete");

    println!(
        "Generated sector '{}' (seed: {}) — {} systems, {} worlds, {} routes",
        sector.id,
        sector.seed,
        sector.systems.len(),
        sector.manifest.world_count,
        sector.routes.len()
    );
    println!("Output written to: {output_dir}");
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn run_generate_system(
    project: Utf8PathBuf,
    seed: Option<String>,
    index: usize,
    coord_q: i32,
    coord_r: i32,
    out: Option<Utf8PathBuf>,
    markdown: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let mut input = sectorforge::load_project(&project)?;
    if let Some(s) = seed {
        input.config.generation.seed = s;
    }
    let coord = HexCoord {
        q: coord_q,
        r: coord_r,
    };
    let system = sectorforge::generate_system_standalone(input, index, coord)?;
    let json = to_json_pretty(&system)?;
    match &out {
        Some(p) => sectorforge::write_system_json(p, &system)?,
        None => println!("{json}"),
    }
    if markdown {
        let md = sectorforge::render_system_markdown(&system);
        match &out {
            Some(p) => {
                let md_path = p.with_extension("md");
                std::fs::write(&md_path, md)
                    .map_err(|e| sectorforge::SectorError::io(md_path.as_str(), e))?;
            }
            None => println!("\n{md}"),
        }
    }
    Ok(ExitCode::SUCCESS)
}
