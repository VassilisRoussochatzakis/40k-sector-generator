//! `economy` — trade, tithe, strategic-resource snapshot.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

pub(crate) fn run_economy(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, mut cfg) =
        resolve_sector_with_cfg(project, sector, |input| input.catalogs.economy.clone())?;
    // Both arms force the economy derivation on, matching the prior inline code.
    cfg.enabled = true;
    let report = sectorforge::derive_economy_with(&sec, &cfg);
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_economy(dir, &sec.id, &report)?;
            println!("Wrote {dir}/economy.md and {dir}/economy.json");
            Ok(())
        },
        || sectorforge::economy::render_markdown(&sec.id, &report),
    )?;
    Ok(ExitCode::SUCCESS)
}
