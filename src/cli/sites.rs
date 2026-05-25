//! `sites` — planetary points-of-interest per world.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::print_json;

pub fn run_sites(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
    player: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, mut cfg) = match (project, sector) {
        (Some(p), None) => {
            let input = sectorforge::load_project(p)?;
            let cfg = input.sites.clone();
            (sectorforge::generate_sector(input)?, cfg)
        }
        (None, Some(s)) => (
            sectorforge::load_sector_json(s)?,
            sectorforge::sites::SitesConfig::default(),
        ),
        _ => {
            return Err(sectorforge::SectorError::InvalidConfig(
                "pass exactly one of --project <dir> or --sector <path>".into(),
            ));
        }
    };
    cfg.player_edition = player;
    let report = sectorforge::derive_sites_with(&sec, &cfg);
    if let Some(dir) = out {
        sectorforge::write_sites(dir, &report, &cfg)?;
        println!("Wrote {dir}/sites.md and {dir}/sites.json");
    } else if json {
        print_json(&report)?;
    } else {
        print!("{}", sectorforge::sites::render_markdown(&report, &cfg));
    }
    Ok(ExitCode::SUCCESS)
}
