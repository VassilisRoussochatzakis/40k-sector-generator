//! `history` — derive chronicle of in-universe events.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::print_json;

pub fn run_history(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, mut cfg) = match (project, sector) {
        (Some(project), None) => {
            let input = sectorforge::load_project(project)?;
            let cfg = input.history.clone();
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
    if let Some(dir) = &out {
        sectorforge::write_history(dir, &report, &cfg)?;
        println!("Wrote {dir}/history.md and {dir}/history.json");
    } else if json {
        print_json(&report)?;
    } else {
        let md = sectorforge::history::render_markdown(&report, &cfg);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}
