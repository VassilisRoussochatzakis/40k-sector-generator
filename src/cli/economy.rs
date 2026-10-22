//! `economy` — trade, tithe, strategic-resource snapshot.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

#[derive(Debug, clap::Args)]
pub(super) struct EconomyArgs {
    #[arg(long)]
    pub project: Option<Utf8PathBuf>,
    #[arg(long)]
    pub sector: Option<Utf8PathBuf>,
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    #[arg(long)]
    pub json: bool,
}

pub(crate) fn run_economy(args: EconomyArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let EconomyArgs {
        project,
        sector,
        out,
        json,
    } = args;
    let project = project.as_ref();
    let sector = sector.as_ref();
    let out = out.as_ref();

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
