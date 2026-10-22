//! `hooks` — adventure / plot hooks.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

#[derive(Debug, clap::Args)]
pub(super) struct HooksArgs {
    #[arg(long)]
    pub project: Option<Utf8PathBuf>,
    #[arg(long)]
    pub sector: Option<Utf8PathBuf>,
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    #[arg(long)]
    pub json: bool,
    /// Hide GM-only hooks (e.g. those derived from hidden-tier presences).
    #[arg(long)]
    pub player: bool,
}

pub(crate) fn run_hooks(args: HooksArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let HooksArgs {
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
        resolve_sector_with_cfg(project, sector, |input| input.catalogs.hooks.clone())?;
    cfg.hide_hidden_hooks = player;
    let report = sectorforge::derive_hooks_with(&sec, &cfg);
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_hooks(dir, &report, &cfg)?;
            println!("Wrote {dir}/hooks.md and {dir}/hooks.json");
            Ok(())
        },
        || sectorforge::hooks::render_markdown(&report, &cfg),
    )?;
    Ok(ExitCode::SUCCESS)
}
