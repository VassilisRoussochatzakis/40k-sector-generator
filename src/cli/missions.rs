//! `missions` — deterministic mission / quest seeds.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, load_or_regenerate};

pub(crate) fn run_missions(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
    player: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project.cloned(), sector.cloned())?;
    let cfg = sectorforge::missions::MissionsConfig {
        player_edition: player,
        ..Default::default()
    };
    let report = sectorforge::derive_missions_with(&sec, &cfg);
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_missions(dir, &report, &cfg)?;
            println!("Wrote {dir}/missions.md and {dir}/missions.json");
            Ok(())
        },
        || sectorforge::missions::render_markdown(&report, &cfg),
    )?;
    Ok(ExitCode::SUCCESS)
}
