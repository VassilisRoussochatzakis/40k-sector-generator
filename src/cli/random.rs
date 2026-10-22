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

#[derive(Debug, clap::Args)]
pub(super) struct RandomArgs {
    /// Sector size: small | medium | large | vast | massive | huge.
    /// Ignored when --width/--height are given. Defaults to medium.
    #[arg(long)]
    pub size: Option<String>,
    /// Explicit square grid side length (sectors must be square). When
    /// both --width and --height are given they must be equal.
    #[arg(long)]
    pub width: Option<u32>,
    /// Explicit square grid side length (sectors must be square). When
    /// both --width and --height are given they must be equal.
    #[arg(long)]
    pub height: Option<u32>,
    /// Reproducibility seed. Omit to mint a fresh one (echoed on success).
    #[arg(long)]
    pub seed: Option<String>,
    /// Project directory to create (must not exist). Defaults to
    /// `./random-<seed>`.
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    /// Source directory holding presets (must contain the chosen
    /// `--baseline`, `_full` by default). Defaults to `./presets`.
    #[arg(long, default_value = "presets")]
    pub presets_dir: Utf8PathBuf,
    /// Baseline preset whose themed data tree seeds the sector's content
    /// (worlds, factions, relations, regions, economy, history, personae,
    /// sites, hooks, missions, prose). The layout — placement, density,
    /// routes, sizing — is still fully rolled from the seed. Use a gallery
    /// preset (e.g. `dead-sector`, `mercantile-crossroads`) or `_full` (the
    /// default: every feature, balanced).
    #[arg(long, default_value = "_full")]
    pub baseline: String,
    /// Comma-separated output formats to export. Defaults to all five
    /// (json, markdown, png, svg, html).
    #[arg(long, value_delimiter = ',')]
    pub formats: Option<Vec<String>>,
    /// Drop render-only artifacts (html, png, svg, markdown), keeping the
    /// machine-readable `json` the viewer loads. Subtracts from `--formats`
    /// / the synthesised config's formats.
    #[arg(long)]
    pub light: bool,
    /// Comma-separated formats to *exclude* from the effective set. `json`
    /// is load-bearing (the viewer reads `out/sector.json`) and cannot be
    /// excluded. Applies after `--formats` / `--light`.
    #[arg(long, value_delimiter = ',')]
    pub exclude: Option<Vec<String>>,
}

pub(crate) fn run_random(args: RandomArgs) -> Result<ExitCode, SectorError> {
    let RandomArgs {
        size,
        width,
        height,
        seed,
        out,
        presets_dir,
        baseline,
        formats,
        light,
        exclude,
    } = args;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Match an `Err(InvalidConfig(msg))` whose message contains `needle`.
    fn assert_invalid(res: Result<SectorSize, SectorError>, needle: &str) {
        match res {
            Err(SectorError::InvalidConfig(m)) => assert!(
                m.contains(needle),
                "message {m:?} did not contain {needle:?}"
            ),
            other => panic!("expected InvalidConfig containing {needle:?}, got {other:?}"),
        }
    }

    #[test]
    fn rejects_non_square_width_height() {
        // The square-sector invariant: when both sides are given they must be
        // equal. 8 != 6 is the deliberate rejection path.
        assert_invalid(resolve_size(None, Some(8), Some(6)), "must be square");
    }

    #[test]
    fn rejects_zero_dimension() {
        assert_invalid(resolve_size(None, Some(0), None), ">= 1");
    }

    #[test]
    fn rejects_dimension_above_max() {
        // MAX_CUSTOM_DIM == 80, so 81 is the first rejected value (strict `>`).
        assert_invalid(resolve_size(None, Some(81), None), "<= 80");
    }

    #[test]
    fn single_side_becomes_custom() {
        // Either --width alone or --height alone fills the single square side.
        assert_eq!(
            resolve_size(None, Some(20), None).unwrap(),
            SectorSize::Custom { dim: 20 }
        );
        assert_eq!(
            resolve_size(None, None, Some(20)).unwrap(),
            SectorSize::Custom { dim: 20 }
        );
    }

    #[test]
    fn equal_width_height_becomes_custom() {
        assert_eq!(
            resolve_size(None, Some(16), Some(16)).unwrap(),
            SectorSize::Custom { dim: 16 }
        );
    }

    #[test]
    fn boundary_dimensions_are_allowed() {
        // 1 is the smallest allowed (>= 1) and 80 == MAX_CUSTOM_DIM is allowed
        // (strict `>` rejection means equal-to-max passes).
        assert_eq!(
            resolve_size(None, Some(1), None).unwrap(),
            SectorSize::Custom { dim: 1 }
        );
        assert_eq!(
            resolve_size(None, Some(80), None).unwrap(),
            SectorSize::Custom { dim: 80 }
        );
    }

    #[test]
    fn no_dims_no_token_defaults_to_medium() {
        assert_eq!(resolve_size(None, None, None).unwrap(), SectorSize::Medium);
    }

    #[test]
    fn size_token_resolves_preset_or_errors() {
        assert_eq!(
            resolve_size(Some("large"), None, None).unwrap(),
            SectorSize::Large
        );
        assert_invalid(resolve_size(Some("nope"), None, None), "unknown --size");
    }
}
