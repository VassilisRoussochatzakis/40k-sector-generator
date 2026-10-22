//! `relations` — inter-faction diplomacy matrix.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

#[derive(Debug, clap::Args)]
pub(super) struct RelationsArgs {
    #[arg(long)]
    pub project: Option<Utf8PathBuf>,
    #[arg(long)]
    pub sector: Option<Utf8PathBuf>,
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    #[arg(long)]
    pub json: bool,
}

pub(crate) fn run_relations(args: RelationsArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let RelationsArgs {
        project,
        sector,
        out,
        json,
    } = args;
    let project = project.as_ref();
    let sector = sector.as_ref();
    let out = out.as_ref();

    // When using --project the per-project relations.toml is honoured;
    // --sector falls back to the built-in defaults.
    let (sec, cfg) =
        resolve_sector_with_cfg(project, sector, |input| input.catalogs.relations.clone())?;
    let matrix = sectorforge::derive_relations_with(&sec, &cfg);
    let report = sectorforge::relations::RelationsReport {
        sector_id: sec.id.to_string(),
        seed: sec.seed.to_string(),
        matrix,
    };
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_relations(dir, &report)?;
            println!("Wrote {dir}/relations.md and {dir}/relations.json");
            Ok(())
        },
        || sectorforge::relations::render_markdown(&report),
    )?;
    Ok(ExitCode::SUCCESS)
}
