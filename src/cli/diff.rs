//! `diff` — deterministic sector diff.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::print_json;

pub struct DiffArgs {
    pub before: Option<Utf8PathBuf>,
    pub after: Option<Utf8PathBuf>,
    pub project: Option<Utf8PathBuf>,
    pub ticks: Option<u32>,
    pub out: Option<Utf8PathBuf>,
    pub json: bool,
    pub skip_worlds: bool,
    pub skip_routes: bool,
}

pub fn run_diff(args: DiffArgs) -> Result<ExitCode, sectorforge::SectorError> {
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
    if let Some(dir) = &args.out {
        sectorforge::write_diff(dir, &diff)?;
        println!("Wrote {dir}/diff.md and {dir}/diff.json");
    } else if args.json {
        print_json(&diff)?;
    } else {
        let md = sectorforge::render_diff_markdown(&diff);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}
