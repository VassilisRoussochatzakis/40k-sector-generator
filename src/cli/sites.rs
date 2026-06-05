//! `sites` — planetary points-of-interest per world.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::emit_report;

pub(crate) fn run_sites(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
    player: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, mut cfg) = match (project, sector) {
        (Some(p), None) => {
            let input = sectorforge::load_project(p)?;
            let cfg = input.catalogs.sites.clone();
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
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_sites(dir, &report, &cfg)?;
            println!("Wrote {dir}/sites.md and {dir}/sites.json");
            Ok(())
        },
        || sectorforge::sites::render_markdown(&report, &cfg),
    )?;
    Ok(ExitCode::SUCCESS)
}
