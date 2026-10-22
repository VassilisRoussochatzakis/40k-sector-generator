//! `sites` — planetary points-of-interest per world.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

#[derive(Debug, clap::Args)]
pub(super) struct SitesArgs {
    #[arg(long)]
    pub project: Option<Utf8PathBuf>,
    #[arg(long)]
    pub sector: Option<Utf8PathBuf>,
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub player: bool,
}

pub(crate) fn run_sites(args: SitesArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let SitesArgs {
        project,
        sector,
        out,
        json,
        player,
    } = args;
    let project = project.as_ref();
    let sector = sector.as_ref();
    let out = out.as_ref();

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
