//! `personae` — dramatis personae overlay.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::print_json;

pub(crate) fn run_personae(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, cfg) = match (project, sector) {
        (Some(p), None) => {
            let input = sectorforge::load_project(p)?;
            let cfg = input.catalogs.personae.clone();
            (sectorforge::generate_sector(input)?, cfg)
        }
        (None, Some(s)) => (
            sectorforge::load_sector_json(s)?,
            sectorforge::personae::PersonaeConfig::default(),
        ),
        _ => {
            return Err(sectorforge::SectorError::InvalidConfig(
                "pass exactly one of --project <dir> or --sector <path>".into(),
            ));
        }
    };
    let report = sectorforge::derive_personae_with(&sec, &cfg);
    if let Some(dir) = out {
        sectorforge::write_personae(dir, &report)?;
        println!("Wrote {dir}/personae.md and {dir}/personae.json");
    } else if json {
        print_json(&report)?;
    } else {
        let md = sectorforge::personae::render_markdown(&report);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}
