//! `sites` — planetary points-of-interest per world.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

pub(crate) fn run_sites(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
    player: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, mut cfg) =
        resolve_sector_with_cfg(project, sector, |input| input.catalogs.sites.clone())?;
    cfg.player_edition = player;
    let report = sectorforge::derive_sites_with(&sec, &cfg);
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_sites(dir, &report, &cfg)?;
            println!("Wrote {dir}/sites.md and {dir}/sites.json");
            Ok(())
        },
        || sectorforge::sites::render_markdown(&report, &cfg),
    )?;
    Ok(ExitCode::SUCCESS)
}
