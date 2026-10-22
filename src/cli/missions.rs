//! `missions` — deterministic mission / quest seeds.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

#[derive(Debug, clap::Args)]
pub(super) struct MissionsArgs {
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

pub(crate) fn run_missions(args: MissionsArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let MissionsArgs {
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
        resolve_sector_with_cfg(project, sector, |input| input.catalogs.missions.clone())?;
    cfg.player_edition = player;
    let report = sectorforge::derive_missions_with(&sec, &cfg);
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_missions(dir, &report, &cfg)?;
            println!("Wrote {dir}/missions.md and {dir}/missions.json");
            Ok(())
        },
        || sectorforge::missions::render_markdown(&report, &cfg),
    )?;
    Ok(ExitCode::SUCCESS)
}
