//! `relations` — inter-faction diplomacy matrix.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

pub(crate) fn run_relations(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
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
