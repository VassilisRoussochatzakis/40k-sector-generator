//! `analyze` — read-only analytics dashboard.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

#[derive(Debug, clap::Args)]
pub(super) struct AnalyzeArgs {
    #[arg(long)]
    pub project: Option<Utf8PathBuf>,
    #[arg(long)]
    pub sector: Option<Utf8PathBuf>,
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    /// Emit only the JSON to stdout (overrides Markdown).
    #[arg(long)]
    pub json: bool,
    /// Exit with status 1 if any health flag fires (useful in CI).
    #[arg(long)]
    pub strict: bool,
}

pub(crate) fn run_analyze(args: AnalyzeArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let AnalyzeArgs {
        project,
        sector,
        out,
        json,
        strict,
    } = args;
    let project = project.as_ref();
    let sector = sector.as_ref();
    let out = out.as_ref();

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
