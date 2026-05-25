//! `prose` — narrative gazetteer.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{load_or_regenerate, print_json};

pub fn run_prose(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
    dispatch: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project.cloned(), sector.cloned())?;
    let cfg = sectorforge::prose::ProseConfig {
        tone: if dispatch {
            sectorforge::prose::ProseTone::Dispatch
        } else {
            sectorforge::prose::ProseTone::Gazetteer
        },
        ..Default::default()
    };
    let report = sectorforge::derive_prose_with(&sec, &cfg);
    if let Some(dir) = out {
        sectorforge::write_prose(dir, &report)?;
        println!("Wrote {dir}/gazetteer.md and {dir}/gazetteer.json");
    } else if json {
        print_json(&report)?;
    } else {
        let md = sectorforge::prose::render_markdown(&report);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}
