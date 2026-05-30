//! `random` — size-only → fully-complete sector (RANDOM.md).
//!
//! Synthesises a fresh, fully-randomised project under `--out`, generates it,
//! runs the five post-generation derivations the orchestrator skips, and
//! exports the bundle plus those five reports. Mirrors the per-overlay report
//! writers used by the `personae`/`sites`/… subcommands.

use std::process::ExitCode;

use camino::Utf8PathBuf;
use sectorforge::config::OutputFormat;
use sectorforge::random_sector::{self, SectorSize};
use sectorforge::SectorError;

use super::common::log_progress;

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_random(
    size: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    seed: Option<String>,
    out: Option<Utf8PathBuf>,
    presets_dir: Utf8PathBuf,
    formats: Option<Vec<String>>,
) -> Result<ExitCode, SectorError> {
    let size = resolve_size(size.as_deref(), width, height)?;
    // Mint here (not inside the core) so we can name the project dir after the
    // seed and echo a reproduce line even when the caller passed no seed.
    let seed = seed.unwrap_or_else(random_sector::mint_seed);
    let dest = out.unwrap_or_else(|| Utf8PathBuf::from(format!("random-{}", dir_slug(&seed))));

    log_progress(format_args!(
        "random: synthesising {} sector (seed: {seed}) in {dest}",
        size.as_slug()
    ));
    let report =
        random_sector::generate_random_sector(size, Some(seed.clone()), &presets_dir, &dest)?;

    // Export the bundle into <dest>/out. The synthesised config already turns
    // on all five formats; --formats overrides which subset is written.
    let mut output_cfg = report.input.config.outputs.clone();
    if let Some(tokens) = formats {
        output_cfg.formats = parse_formats(&tokens)?;
    }
    let output_dir = dest.join(&output_cfg.directory);
    log_progress(format_args!("random: exporting bundle to {output_dir}"));
    sectorforge::export_sector(&report.sector, &output_cfg, &output_dir)?;

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
    println!("Reproduce: {}", reproduce_hint(size, &report.seed));
    Ok(ExitCode::SUCCESS)
}

/// Resolve the one user input. Explicit `--width`+`--height` ⇒ `Custom`;
/// otherwise a `--size` token; otherwise `Medium`.
fn resolve_size(
    size: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<SectorSize, SectorError> {
    match (width, height) {
        (Some(w), Some(h)) => {
            if w == 0 || h == 0 {
                return Err(SectorError::InvalidConfig(
                    "--width and --height must be >= 1".into(),
                ));
            }
            Ok(SectorSize::Custom {
                width: w,
                height: h,
            })
        }
        (None, None) => match size {
            Some(token) => SectorSize::parse_token(token).ok_or_else(|| {
                SectorError::InvalidConfig(format!(
                    "unknown --size '{token}' (expected small | medium | large | huge)"
                ))
            }),
            None => Ok(SectorSize::Medium),
        },
        _ => Err(SectorError::InvalidConfig(
            "provide both --width and --height for a custom size, or neither".into(),
        )),
    }
}

fn parse_formats(tokens: &[String]) -> Result<Vec<OutputFormat>, SectorError> {
    let mut out: Vec<OutputFormat> = Vec::new();
    for token in tokens {
        match OutputFormat::parse_token(token) {
            Some(f) if !out.contains(&f) => out.push(f),
            Some(_) => {}
            None => {
                return Err(SectorError::InvalidConfig(format!(
                    "unknown --formats token '{token}' (expected json|markdown|png|svg|html)"
                )))
            }
        }
    }
    if out.is_empty() {
        return Err(SectorError::InvalidConfig(
            "--formats listed no valid formats".into(),
        ));
    }
    Ok(out)
}

fn reproduce_hint(size: SectorSize, seed: &str) -> String {
    match size {
        SectorSize::Custom { width, height } => {
            format!("sectorforge random --width {width} --height {height} --seed {seed}")
        }
        other => format!(
            "sectorforge random --size {} --seed {seed}",
            other.as_slug()
        ),
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
