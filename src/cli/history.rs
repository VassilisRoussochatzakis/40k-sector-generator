//! `history` — derive chronicle of in-universe events.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

#[derive(Debug, clap::Args)]
pub(super) struct HistoryArgs {
    #[arg(long)]
    pub project: Option<Utf8PathBuf>,
    #[arg(long)]
    pub sector: Option<Utf8PathBuf>,
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    #[arg(long)]
    pub json: bool,
}

pub(crate) fn run_history(args: HistoryArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let HistoryArgs {
        project,
        sector,
        out,
        json,
    } = args;
    let project = project.as_ref();
    let sector = sector.as_ref();
    let out = out.as_ref();

    let (sec, mut cfg) =
        resolve_sector_with_cfg(project, sector, |input| input.catalogs.history.clone())?;
    cfg.enabled = true;
    let report = sectorforge::derive_history_with(&sec, &cfg);
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_history(dir, &report, &cfg)?;
            println!("Wrote {dir}/history.md and {dir}/history.json");
            Ok(())
        },
        || sectorforge::history::render_markdown(&report, &cfg),
    )?;
    Ok(ExitCode::SUCCESS)
}
