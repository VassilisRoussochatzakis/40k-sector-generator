//! `hooks` — adventure / plot hooks.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{load_or_regenerate, print_json};

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
    if let Some(dir) = &out {
        sectorforge::write_hooks(dir, &report, &cfg)?;
        println!("Wrote {dir}/hooks.md and {dir}/hooks.json");
    } else if json {
        print_json(&report)?;
    } else {
        let md = sectorforge::hooks::render_markdown(&report, &cfg);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}
