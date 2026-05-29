//! `analyze` — read-only analytics dashboard.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::print_json;

pub(crate) fn run_analyze(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
    strict: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, cfg) = match (project, sector) {
        (Some(project), None) => {
            let input = sectorforge::load_project(project)?;
            let cfg = input.config.analyze.clone();
            let sec = sectorforge::generate_sector(input)?;
            (sec, cfg)
        }
        (None, Some(sector)) => {
            let sec = sectorforge::load_sector_json(sector)?;
            (sec, sectorforge::analytics::AnalyzeConfig::default())
        }
        (Some(_), Some(_)) | (None, None) => {
            return Err(sectorforge::SectorError::InvalidConfig(
                "pass exactly one of --project <dir> or --sector <path>".into(),
            ));
        }
    };
    let analysis = sectorforge::analyze_sector_with(&sec, &cfg);
    if let Some(dir) = out {
        sectorforge::write_analysis(dir, &analysis)?;
        println!("Wrote {dir}/analysis.md and {dir}/analysis.json");
    } else if json {
        print_json(&analysis)?;
    } else {
        let md = sectorforge::render_analysis_markdown(&analysis);
        print!("{md}");
    }
    let has_flags = !analysis.health_flags.is_empty();
    if strict && has_flags {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}
