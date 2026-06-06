//! `personae` — dramatis personae overlay.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

pub(crate) fn run_personae(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, cfg) =
        resolve_sector_with_cfg(project, sector, |input| input.catalogs.personae.clone())?;
    let report = sectorforge::derive_personae_with(&sec, &cfg);
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_personae(dir, &report)?;
            println!("Wrote {dir}/personae.md and {dir}/personae.json");
            Ok(())
        },
        || sectorforge::personae::render_markdown(&report),
    )?;
    Ok(ExitCode::SUCCESS)
}
