//! `regions` — regional warp-phenomena overlay.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::emit_report;

#[derive(Debug, clap::Args)]
pub(super) struct RegionsArgs {
    #[arg(long)]
    pub project: Utf8PathBuf,
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    #[arg(long)]
    pub json: bool,
}

pub(crate) fn run_regions(args: RegionsArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let RegionsArgs { project, out, json } = args;
    let project = &project;
    let out = out.as_ref();

    let input = sectorforge::load_project(project)?;
    let cfg = input.catalogs.regions.clone();
    let regs = sectorforge::build_regions(
        &input.config.generation.seed,
        input.config.generation.sector_width,
        input.config.generation.sector_height,
        &cfg,
    );
    emit_report(
        out,
        json,
        &regs,
        |dir| {
            sectorforge::write_regions(dir, &input.config.project.id, &regs)?;
            println!("Wrote {dir}/regions.md and {dir}/regions.json");
            Ok(())
        },
        || sectorforge::regions::render_markdown(&input.config.project.id, &regs),
    )?;
    Ok(ExitCode::SUCCESS)
}
