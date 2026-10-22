//! `validate`, `validate-sector`, `render-markdown`, `inspect-worlds`.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{
    print_invariant_report, print_json, print_validation_report, print_workbook_stats,
};

#[derive(Debug, clap::Args)]
pub(super) struct ValidateArgs {
    #[arg(long)]
    pub project: Utf8PathBuf,
    /// Emit validation report as JSON to stdout.
    #[arg(long)]
    pub json: bool,
    /// Treat warnings as errors.
    #[arg(long)]
    pub strict: bool,
}

#[derive(Debug, clap::Args)]
pub(super) struct ValidateSectorArgs {
    #[arg(long)]
    pub sector: Utf8PathBuf,
    /// Emit report as JSON to stdout.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub(super) struct RenderMarkdownArgs {
    #[arg(long)]
    pub sector: Utf8PathBuf,
    /// Output path for the Markdown (defaults to stdout).
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
}

#[derive(Debug, clap::Args)]
pub(super) struct InspectWorldsArgs {
    #[arg(long)]
    pub data_dir: String,
}

pub(crate) fn run_validate(args: ValidateArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let ValidateArgs {
        project,
        json,
        strict,
    } = args;
    let project = &project;

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

pub(crate) fn run_validate_sector(
    args: ValidateSectorArgs,
) -> Result<ExitCode, sectorforge::SectorError> {
    let ValidateSectorArgs { sector, json } = args;
    let sector = &sector;

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

pub(crate) fn run_render_markdown(
    args: RenderMarkdownArgs,
) -> Result<ExitCode, sectorforge::SectorError> {
    let RenderMarkdownArgs { sector, out } = args;
    let sector = &sector;
    let out = out.as_ref();

    let sec = sectorforge::load_sector_json(sector)?;
    let md = sectorforge::render_sector_markdown(&sec);
    match out {
        Some(p) => sectorforge::write_sector_markdown(p, &sec)?,
        None => print!("{md}"),
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn run_inspect_worlds(
    args: InspectWorldsArgs,
) -> Result<ExitCode, sectorforge::SectorError> {
    let InspectWorldsArgs { data_dir } = args;
    let data_dir = &data_dir;

    let stats = sectorforge::inspect_world_workbook(data_dir)?;
    print_workbook_stats(&stats);
    Ok(ExitCode::SUCCESS)
}
