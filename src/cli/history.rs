//! `history` — derive chronicle of in-universe events.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

pub(crate) fn run_history(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
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
