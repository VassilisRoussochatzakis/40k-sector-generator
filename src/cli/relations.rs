//! `relations` — inter-faction diplomacy matrix.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::emit_report;

pub(crate) fn run_relations(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    // When using --project the per-project relations.toml is honoured;
    // --sector falls back to the built-in defaults.
    let (sec, cfg) = match (project, sector) {
        (Some(p), None) => {
            let input = sectorforge::load_project(p)?;
            let cfg = input.catalogs.relations.clone();
            (sectorforge::generate_sector(input)?, cfg)
        }
        (None, Some(s)) => (
            sectorforge::load_sector_json(s)?,
            sectorforge::relations::RelationsConfig::default(),
        ),
        _ => {
            return Err(sectorforge::SectorError::InvalidConfig(
                "pass exactly one of --project <dir> or --sector <path>".into(),
            ));
        }
    };
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
