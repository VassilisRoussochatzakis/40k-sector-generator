//! `history` — derive chronicle of in-universe events.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::emit_report;

pub(crate) fn run_history(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, mut cfg) = match (project, sector) {
        (Some(project), None) => {
            let input = sectorforge::load_project(project)?;
            let cfg = input.catalogs.history.clone();
            (sectorforge::generate_sector(input)?, cfg)
        }
        (None, Some(sector)) => (
            sectorforge::load_sector_json(sector)?,
            sectorforge::history::HistoryConfig::default(),
        ),
        (Some(_), Some(_)) | (None, None) => {
            return Err(sectorforge::SectorError::InvalidConfig(
                "pass exactly one of --project <dir> or --sector <path>".into(),
            ));
        }
    };
    cfg.enabled = true;
    let report = sectorforge::derive_history_with(&sec, &cfg);
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_history(dir, &report, &cfg)?;
            println!("Wrote {dir}/history.md and {dir}/history.json");
            Ok(())
        },
        || sectorforge::history::render_markdown(&report, &cfg),
    )?;
    Ok(ExitCode::SUCCESS)
}
