//! `random` — size-only → fully-complete sector (RANDOM.md).
//!
//! Synthesises a fresh, fully-randomised project under `--out`, generates it,
//! runs the five post-generation derivations the orchestrator skips, and
//! exports the bundle plus those five reports. Mirrors the per-overlay report
//! writers used by the `personae`/`sites`/… subcommands.

use std::process::ExitCode;

use camino::Utf8PathBuf;
use sectorforge::random_sector::{self, SectorSize};
use sectorforge::SectorError;

use super::common::{log_export_progress, log_progress, resolve_formats};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_random(
    size: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    seed: Option<String>,
    out: Option<Utf8PathBuf>,
    presets_dir: Utf8PathBuf,
    baseline: String,
    formats: Option<Vec<String>>,
    light: bool,
    exclude: Option<Vec<String>>,
) -> Result<ExitCode, SectorError> {
    let size = resolve_size(size.as_deref(), width, height)?;
    // Mint here (not inside the core) so we can name the project dir after the
    // seed and echo a reproduce line even when the caller passed no seed.
    let seed = seed.unwrap_or_else(random_sector::mint_seed);
    let dest = out.unwrap_or_else(|| Utf8PathBuf::from(format!("random-{}", dir_slug(&seed))));

    log_progress(format_args!(
        "random: synthesising {} sector from baseline '{baseline}' (seed: {seed}) in {dest}",
        size.as_slug()
    ));
    let report = random_sector::generate_random_sector_from(
        size,
        Some(seed.clone()),
        &baseline,
        &presets_dir,
        &dest,
    )?;

    // Export the bundle into <dest>/out. The synthesised config already turns
    // on all five formats; --formats picks a subset, --light/--exclude drop
    // render artifacts. `json` is always kept (the viewer reads sector.json).
    let mut output_cfg = report.input.config.outputs.clone();
    output_cfg.formats = resolve_formats(output_cfg.formats.clone(), formats, exclude, light)?;
    let output_dir = dest.join(&output_cfg.directory);
    log_progress(format_args!("random: exporting bundle to {output_dir}"));
    sectorforge::export_sector_with_progress(
        &report.sector,
        &output_cfg,
        &output_dir,
        log_export_progress,
    )?;

    // The five post-generation reports alongside the bundle, using the configs
    // the project was loaded with.
    sectorforge::write_personae(&output_dir, &report.personae)?;
    sectorforge::write_sites(&output_dir, &report.sites, &report.input.catalogs.sites)?;
    sectorforge::write_hooks(&output_dir, &report.hooks, &report.input.catalogs.hooks)?;
    sectorforge::write_missions(
        &output_dir,
        &report.missions,
        &report.input.catalogs.missions,
    )?;
    sectorforge::write_prose(&output_dir, &report.prose)?;

    let s = &report.sector;
    println!(
        "Generated random sector '{}' (seed: {}) — {} systems, {} worlds, {} routes",
        s.id,
        report.seed,
        s.systems.len(),
        s.manifest.world_count,
        s.routes.len()
    );
    println!("Project:  {dest}");
    println!("Outputs:  {output_dir} (bundle + personae/sites/hooks/missions/gazetteer)");
    println!(
        "Reproduce: {}",
        reproduce_hint(size, &baseline, &report.seed)
    );
    Ok(ExitCode::SUCCESS)
}

/// Resolve the one user input. Sectors are **square**, so a custom size is a
/// single side length: pass `--width` and/or `--height` (when both are given
/// they must be equal) ⇒ `Custom`; otherwise a `--size` token; otherwise
/// `Medium`.
fn resolve_size(
    size: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<SectorSize, SectorError> {
    let dim = match (width, height) {
        (Some(w), Some(h)) => {
            if w != h {
                return Err(SectorError::InvalidConfig(format!(
                    "sectors must be square: --width ({w}) must equal --height ({h})"
                )));
            }
            Some(w)
        }
        (Some(d), None) | (None, Some(d)) => Some(d),
        (None, None) => None,
    };

    if let Some(d) = dim {
        if d == 0 {
            return Err(SectorError::InvalidConfig(
                "--width / --height must be >= 1".into(),
            ));
        }
        if d > random_sector::MAX_CUSTOM_DIM {
            return Err(SectorError::InvalidConfig(format!(
                "--width / --height must be <= {}",
                random_sector::MAX_CUSTOM_DIM
            )));
        }
        return Ok(SectorSize::Custom { dim: d });
    }

    match size {
        Some(token) => SectorSize::parse_token(token).ok_or_else(|| {
            SectorError::InvalidConfig(format!(
                "unknown --size '{token}' (expected small | medium | large | vast | massive | huge)"
            ))
        }),
        None => Ok(SectorSize::Medium),
    }
}

fn reproduce_hint(size: SectorSize, baseline: &str, seed: &str) -> String {
    let base = match size {
        SectorSize::Custom { dim } => {
            format!("sectorforge random --width {dim} --height {dim} --seed {seed}")
        }
        other => format!(
            "sectorforge random --size {} --seed {seed}",
            other.as_slug()
        ),
    };
    // Echo the baseline only when it is not the implicit default, so the common
    // case stays terse.
    if baseline == random_sector::FULL_PRESET_ID {
        base
    } else {
        format!("{base} --baseline {baseline}")
    }
}

/// Filesystem-safe slug for the default project directory name.
fn dir_slug(seed: &str) -> String {
    let s: String = seed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(16)
        .collect();
    if s.is_empty() {
        "sector".to_string()
    } else {
        s
    }
}
