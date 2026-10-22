//! `prose` — narrative gazetteer.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{emit_report, resolve_sector_with_cfg};

#[derive(Debug, clap::Args)]
pub(super) struct ProseArgs {
    #[arg(long)]
    pub project: Option<Utf8PathBuf>,
    #[arg(long)]
    pub sector: Option<Utf8PathBuf>,
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    #[arg(long)]
    pub json: bool,
    /// Use the terse Administratum-dispatch tone instead of the
    /// florid gazetteer voice.
    #[arg(long)]
    pub dispatch: bool,
}

pub(crate) fn run_prose(args: ProseArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let ProseArgs {
        project,
        sector,
        out,
        json,
        dispatch,
    } = args;
    let project = project.as_ref();
    let sector = sector.as_ref();
    let out = out.as_ref();

    // §PR1..§PR4: when a project is supplied, honour any `[inputs].prose`
    // entry so per-sector / per-system overrides authored via the builder
    // survive on the CLI path too. Falls back to defaults when no project is
    // available or when the project leaves `inputs.prose` unset.
    let (sec, base_cfg) =
        resolve_sector_with_cfg(project, sector, |input| input.catalogs.prose.clone())?;
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
