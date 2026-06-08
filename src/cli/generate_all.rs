//! `generate-all`: regenerate every example project under a directory.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::generate::run_generate;

/// Generate every sector project found directly under `dir` (default
/// `examples/`), writing each one's output into its own configured `out/`
/// folder and overwriting it. A "project" is any immediate subdirectory that
/// contains a `sectorforge.toml`. Subdirectories are processed in sorted order
/// (filesystem listing order is not relied upon); one project's failure is
/// reported but does not abort the rest. Exits non-zero if any project failed.
pub(crate) fn run_generate_all(
    dir: Utf8PathBuf,
    allow_warnings: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    // Collect immediate subdirectories that look like sector projects.
    let read = dir
        .read_dir_utf8()
        .map_err(|e| sectorforge::SectorError::io(dir.as_str(), e))?;
    let mut projects: Vec<Utf8PathBuf> = Vec::new();
    for entry in read {
        let entry = entry.map_err(|e| sectorforge::SectorError::io(dir.as_str(), e))?;
        let path = entry.path().to_owned();
        if path.is_dir() && path.join("sectorforge.toml").is_file() {
            projects.push(path);
        }
    }
    // Deterministic processing order — never rely on filesystem ordering.
    projects.sort();

    if projects.is_empty() {
        // Finding #29: progress/status diagnostics go through the `log` facade.
        log::info!("generate-all: no sector projects (sectorforge.toml) found under {dir}");
        return Ok(ExitCode::SUCCESS);
    }

    log::info!("generate-all: {} project(s) under {dir}", projects.len());

    let mut failed: Vec<(Utf8PathBuf, String)> = Vec::new();
    for project in &projects {
        log::info!("\n=== generate-all: {project} ===");
        // out = None -> each project's <outputs.directory> (its own `out/`),
        // overwriting it. All other knobs default so each project's own
        // sectorforge.toml drives seed and formats.
        let result = run_generate(
            project.clone(),
            None, // seed
            None, // out
            allow_warnings,
            None,  // heatmap
            false, // no_faction_fill
            None,  // theme
            None,  // constraints
            None,  // max_candidates
            None,  // formats
            false, // light
            None,  // exclude
        );
        if let Err(e) = result {
            // A single project's failure is non-fatal (the run continues), so
            // it is a warning in the progress stream rather than a hard error.
            log::warn!("generate-all: FAILED {project}: {e}");
            failed.push((project.clone(), e.to_string()));
        }
    }

    let total = projects.len();
    let ok = total - failed.len();
    println!("\ngenerate-all: {ok}/{total} project(s) generated");
    if failed.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        for (p, msg) in &failed {
            log::warn!("  failed: {p}: {msg}");
        }
        Ok(ExitCode::FAILURE)
    }
}
