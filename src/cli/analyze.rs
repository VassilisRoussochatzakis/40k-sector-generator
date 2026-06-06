//! `analyze` — read-only analytics dashboard.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

pub(crate) fn run_analyze(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
    strict: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, cfg) =
        resolve_sector_with_cfg(project, sector, |input| input.config.analyze.clone())?;
    let analysis = sectorforge::analyze_sector_with(&sec, &cfg);
    emit_report(
        out,
        json,
        &analysis,
        |dir| {
            sectorforge::write_analysis(dir, &analysis)?;
            println!("Wrote {dir}/analysis.md and {dir}/analysis.json");
            Ok(())
        },
        || sectorforge::render_analysis_markdown(&analysis),
    )?;
    let has_flags = !analysis.health_flags.is_empty();
    if strict && has_flags {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}
