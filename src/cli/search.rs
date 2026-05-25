//! `search` — deterministic constraint-directed seed search.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::print_json;

pub fn run_search(
    project: &Utf8PathBuf,
    wishes: &Utf8PathBuf,
    base_seed: Option<String>,
    budget: Option<u32>,
    out: Option<&Utf8PathBuf>,
    json: bool,
    strict: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
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
    if let Some(dir) = out {
        sectorforge::write_search_outcome(dir, &outcome)?;
        println!("Wrote {dir}/search.md and {dir}/search.json");
    } else if json {
        print_json(&outcome)?;
    } else {
        let md = sectorforge::search::render_outcome_markdown(&outcome);
        print!("{md}");
    }
    if strict && outcome.winning.is_none() {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}
