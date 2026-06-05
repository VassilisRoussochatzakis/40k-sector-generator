//! `economy` — trade, tithe, strategic-resource snapshot.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::emit_report;

pub(crate) fn run_economy(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, cfg) = match (project, sector) {
        (Some(p), None) => {
            let input = sectorforge::load_project(p)?;
            let cfg = sectorforge::economy::EconomyConfig {
                enabled: true,
                ..input.catalogs.economy.clone()
            };
            (sectorforge::generate_sector(input)?, cfg)
        }
        (None, Some(s)) => {
            let cfg = sectorforge::economy::EconomyConfig {
                enabled: true,
                ..Default::default()
            };
            (sectorforge::load_sector_json(s)?, cfg)
        }
        _ => {
            return Err(sectorforge::SectorError::InvalidConfig(
                "pass exactly one of --project <dir> or --sector <path>".into(),
            ));
        }
    };
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
