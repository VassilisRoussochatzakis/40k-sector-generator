//! `search` — deterministic constraint-directed seed search.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::emit_report;

#[derive(Debug, clap::Args)]
pub(super) struct SearchArgs {
    #[arg(long)]
    pub project: Utf8PathBuf,
    #[arg(long)]
    pub wishes: Utf8PathBuf,
    /// Override the base seed from wishes/project (n=0 candidate).
    #[arg(long)]
    pub base_seed: Option<String>,
    /// Override the search budget.
    #[arg(long)]
    pub budget: Option<u32>,
    /// Write search.md + search.json into this directory.
    #[arg(long)]
    pub out: Option<Utf8PathBuf>,
    /// Print JSON to stdout instead of Markdown.
    #[arg(long)]
    pub json: bool,
    /// Exit 1 if no candidate satisfied the constraints.
    #[arg(long)]
    pub strict: bool,
}

pub(crate) fn run_search(args: SearchArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let SearchArgs {
        project,
        wishes,
        base_seed,
        budget,
        out,
        json,
        strict,
    } = args;
    let project = &project;
    let wishes = &wishes;
    let out = out.as_ref();

    let input = sectorforge::load_project(project)?;
    let mut wishes_file = sectorforge::search::load_wishes(wishes)?;
    if let Some(b) = base_seed {
        wishes_file.search.base_seed = Some(b);
    }
    if let Some(n) = budget {
        wishes_file.search.budget = n;
    }
    let outcome = sectorforge::run_seed_search(&input, &wishes_file)?;
    if !outcome.preflight_errors.is_empty() {
        for e in &outcome.preflight_errors {
            eprintln!("preflight: {e}");
        }
        return Ok(ExitCode::from(1));
    }
    emit_report(
        out,
        json,
        &outcome,
        |dir| {
            sectorforge::write_search_outcome(dir, &outcome)?;
            println!("Wrote {dir}/search.md and {dir}/search.json");
            Ok(())
        },
        || sectorforge::search::render_outcome_markdown(&outcome),
    )?;
    if strict && outcome.winning.is_none() {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}
