//! `hooks` — adventure / plot hooks.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, load_or_regenerate};

pub(crate) fn run_hooks(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
    player: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project.cloned(), sector.cloned())?;
    let cfg = sectorforge::hooks::HooksConfig {
        hide_hidden_hooks: player,
        ..Default::default()
    };
    let report = sectorforge::derive_hooks_with(&sec, &cfg);
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_hooks(dir, &report, &cfg)?;
            println!("Wrote {dir}/hooks.md and {dir}/hooks.json");
            Ok(())
        },
        || sectorforge::hooks::render_markdown(&report, &cfg),
    )?;
    Ok(ExitCode::SUCCESS)
}
