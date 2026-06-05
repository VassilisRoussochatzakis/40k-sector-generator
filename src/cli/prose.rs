//! `prose` — narrative gazetteer.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, load_or_regenerate};

pub(crate) fn run_prose(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
    dispatch: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    // §PR1..§PR4: when a project is supplied, honour any `[inputs].prose`
    // entry so per-sector / per-system overrides authored via the builder
    // survive on the CLI path too. Falls back to defaults when no project is
    // available or when the project leaves `inputs.prose` unset.
    let (sec, base_cfg) = match project.cloned() {
        Some(project_dir) => {
            let input = sectorforge::load_project(&project_dir)?;
            let cfg = input.catalogs.prose.clone();
            let sec = sectorforge::generate_sector(input)?;
            (sec, cfg)
        }
        None => (
            load_or_regenerate(None, sector.cloned())?,
            sectorforge::prose::ProseConfig::default(),
        ),
    };
    let cfg = sectorforge::prose::ProseConfig {
        tone: if dispatch {
            sectorforge::prose::ProseTone::Dispatch
        } else {
            sectorforge::prose::ProseTone::Gazetteer
        },
        ..base_cfg
    };
    let report = sectorforge::derive_prose_with(&sec, &cfg);
    emit_report(
        out,
        json,
        &report,
        |dir| {
            sectorforge::write_prose(dir, &report)?;
            println!("Wrote {dir}/gazetteer.md and {dir}/gazetteer.json");
            Ok(())
        },
        || sectorforge::prose::render_markdown(&report),
    )?;
    Ok(ExitCode::SUCCESS)
}
