//! `validate`, `validate-sector`, `render-markdown`, `inspect-worlds`.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{
    print_invariant_report, print_json, print_validation_report, print_workbook_stats,
};

pub fn run_validate(
    project: &Utf8PathBuf,
    json: bool,
    strict: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let input = sectorforge::load_project(project)?;
    let report = sectorforge::validate_project(&input)?;
    if json {
        print_json(&report)?;
    } else {
        print_validation_report(&report);
    }
    let has_blocking = !report.ok || (strict && !report.warnings.is_empty());
    Ok(if has_blocking {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

pub fn run_validate_sector(
    sector: &Utf8PathBuf,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = sectorforge::load_sector_json(sector)?;
    let report = sectorforge::validate_sector(&sec);
    if json {
        print_json(&report)?;
    } else {
        print_invariant_report(&report);
    }
    Ok(if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

pub fn run_render_markdown(
    sector: &Utf8PathBuf,
    out: Option<&Utf8PathBuf>,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = sectorforge::load_sector_json(sector)?;
    let md = sectorforge::render_sector_markdown(&sec);
    match out {
        Some(p) => sectorforge::write_sector_markdown(p, &sec)?,
        None => print!("{md}"),
    }
    Ok(ExitCode::SUCCESS)
}

pub fn run_inspect_worlds(data_dir: &str) -> Result<ExitCode, sectorforge::SectorError> {
    let stats = sectorforge::inspect_world_workbook(data_dir)?;
    print_workbook_stats(&stats);
    Ok(ExitCode::SUCCESS)
}
