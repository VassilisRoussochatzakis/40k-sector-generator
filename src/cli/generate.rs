//! `generate` and `generate-system`.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use sectorforge::sector_model::HexCoord;

use super::common::{
    log_export_progress, log_progress, log_sector_progress, parse_heatmap, print_validation_report,
    resolve_formats, to_json_pretty,
};

#[derive(Debug, clap::Args)]
pub(super) struct GenerateArgs {
    #[arg(long)]
    pub project: Utf8PathBuf,
    /// Override seed from sectorforge.toml.
    #[arg(long)]
    pub seed: Option<String>,
    /// Override output directory.
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    /// Continue if validation produced warnings (but not errors).
    #[arg(long)]
    pub allow_warnings: bool,
    /// PNG heatmap mode (§10). One of: off, control, military, trade,
    /// industrial, covert, faith, threat, intel, tension, trade_volume,
    /// food, tithe, supply, conflict. Overrides the project's
    /// `outputs.bitmap.heatmap`.
    #[arg(long)]
    pub heatmap: Option<String>,
    /// Disable per-system faction tint in the PNG export (§8).
    #[arg(long)]
    pub no_faction_fill: bool,
    /// PNG map theme (§13). One of: gm_dark, print_mono,
    /// imperial_archive, navis_tactical, inquisition_redacted,
    /// subsector_political.
    #[arg(long)]
    pub theme: Option<String>,
    /// §15 NEW2.md: Constraint file to satisfy during generation.
    #[arg(long)]
    pub constraints: Option<Utf8PathBuf>,
    /// §15 NEW2.md: Maximum number of candidate seeds to evaluate.
    #[arg(long)]
    pub max_candidates: Option<u32>,
    /// Comma-separated output formats. Overrides
    /// `[outputs].formats` in `sectorforge.toml`. Tokens: `json`,
    /// `markdown`, `png` (alias for `bitmap`), `svg`, `html`.
    #[arg(long, value_delimiter = ',')]
    pub formats: Option<Vec<String>>,
    /// Drop render-only artifacts (html, png, svg, markdown), keeping the
    /// machine-readable `json` the viewer loads. Subtracts from `--formats`
    /// / `[outputs].formats`.
    #[arg(long)]
    pub light: bool,
    /// Comma-separated formats to *exclude* from the effective set. `json`
    /// is load-bearing (the viewer reads `out/sector.json`) and cannot be
    /// excluded. Applies after `--formats` / `--light`.
    #[arg(long, value_delimiter = ',')]
    pub exclude: Option<Vec<String>>,
}

#[derive(Debug, clap::Args)]
pub(super) struct GenerateSystemArgs {
    #[arg(long)]
    pub project: Utf8PathBuf,
    /// Override seed from sectorforge.toml.
    #[arg(long)]
    pub seed: Option<String>,
    /// 1-based system index.
    #[arg(long, default_value_t = 1)]
    pub index: usize,
    /// Axial hex coord q (defaults to 0).
    #[arg(long, default_value_t = 0)]
    pub coord_q: i32,
    /// Axial hex coord r (defaults to 0).
    #[arg(long, default_value_t = 0)]
    pub coord_r: i32,
    /// Output path for the system JSON (defaults to stdout).
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    /// Also write a Markdown snippet alongside the JSON.
    #[arg(long)]
    pub markdown: bool,
}

pub(crate) fn run_generate(args: GenerateArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let GenerateArgs {
        project,
        seed,
        out,
        allow_warnings,
        heatmap,
        no_faction_fill,
        theme,
        constraints,
        max_candidates,
        formats,
        light,
        exclude,
    } = args;

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
    input.config.outputs.formats = resolve_formats(
        input.config.outputs.formats.clone(),
        formats,
        exclude,
        light,
    )?;
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
    sectorforge::export_sector_with_progress(
        &sector,
        &output_cfg,
        &output_dir,
        log_export_progress,
    )?;
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
    args: GenerateSystemArgs,
) -> Result<ExitCode, sectorforge::SectorError> {
    let GenerateSystemArgs {
        project,
        seed,
        index,
        coord_q,
        coord_r,
        out,
        markdown,
    } = args;

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
