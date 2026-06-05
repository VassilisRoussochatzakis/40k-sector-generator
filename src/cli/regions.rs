//! `regions` — regional warp-phenomena overlay.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::emit_report;

pub(crate) fn run_regions(
    project: &Utf8PathBuf,
    out: Option<&Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
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
