//! `diff` — deterministic sector diff.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::emit_report;

#[derive(Debug, clap::Args)]
pub(super) struct DiffArgs {
    #[arg(long)]
    pub before: Option<Utf8PathBuf>,
    #[arg(long)]
    pub after: Option<Utf8PathBuf>,
    #[arg(long)]
    pub project: Option<Utf8PathBuf>,
    #[arg(long)]
    pub ticks: Option<u32>,
    /// Write diff.md + diff.json into this directory.
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    /// Print JSON to stdout instead of Markdown.
    #[arg(long)]
    pub json: bool,
    /// Skip world-level detail in the report.
    #[arg(long)]
    pub skip_worlds: bool,
    /// Skip per-route detail.
    #[arg(long)]
    pub skip_routes: bool,
}

pub(crate) fn run_diff(args: DiffArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let cfg = sectorforge::diff::DiffConfig {
        skip_worlds: args.skip_worlds,
        skip_routes: args.skip_routes,
        ..Default::default()
    };
    let diff = match (args.before, args.after, args.project, args.ticks) {
        (Some(a), Some(b), None, None) => {
            let sa = sectorforge::load_sector_json(&a)?;
            let sb = sectorforge::load_sector_json(&b)?;
            sectorforge::diff_sectors_with(&sa, &sb, &cfg)
        }
        (None, None, Some(proj), Some(n)) => {
            let input = sectorforge::load_project(&proj)?;
            let (d, _, _) = sectorforge::diff::diff_after_ticks(input, n, &cfg)?;
            d
        }
        _ => {
            return Err(sectorforge::SectorError::InvalidConfig(
                "pass either --before <a.json> --after <b.json> or --project <dir> --ticks <N>"
                    .into(),
            ));
        }
    };
    emit_report(
        args.out.as_ref(),
        args.json,
        &diff,
        |dir| {
            sectorforge::write_diff(dir, &diff, &cfg)?;
            println!("Wrote {dir}/diff.md and {dir}/diff.json");
            Ok(())
        },
        || sectorforge::render_diff_markdown(&diff, &cfg),
    )?;
    Ok(ExitCode::SUCCESS)
}
