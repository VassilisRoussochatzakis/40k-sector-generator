//! `regions` — regional warp-phenomena overlay.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::print_json;

pub fn run_regions(
    project: &Utf8PathBuf,
    out: Option<&Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let input = sectorforge::load_project(project)?;
    let cfg = input.regions.clone();
    let regs = sectorforge::build_regions(
        &input.config.generation.seed,
        input.config.generation.sector_width,
        input.config.generation.sector_height,
        &cfg,
    );
    if let Some(dir) = out {
        sectorforge::write_regions(dir, &input.config.project.id, &regs)?;
        println!("Wrote {dir}/regions.md and {dir}/regions.json");
    } else if json {
        print_json(&regs)?;
    } else {
        let md = sectorforge::regions::render_markdown(&input.config.project.id, &regs);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}
