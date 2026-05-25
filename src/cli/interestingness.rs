//! `interestingness` — scorecard against a target profile.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{load_or_regenerate, print_json};

pub fn run_interestingness(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: Option<&Utf8PathBuf>,
    json: bool,
    profile: Option<String>,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project.cloned(), sector.cloned())?;
    let mut cfg = sectorforge::interestingness::InterestingnessConfig::default();
    if let Some(p) = profile.as_deref() {
        cfg.profile = parse_interestingness_profile(p)?;
    }
    let report = sectorforge::derive_interestingness_with(&sec, &cfg);
    if let Some(dir) = &out {
        sectorforge::write_interestingness(dir, &report)?;
        println!("Wrote {dir}/interestingness.md and {dir}/interestingness.json");
    } else if json {
        print_json(&report)?;
    } else {
        print!("{}", sectorforge::interestingness::render_markdown(&report));
    }
    Ok(ExitCode::SUCCESS)
}

fn parse_interestingness_profile(
    s: &str,
) -> Result<sectorforge::interestingness::ProfileId, sectorforge::SectorError> {
    use sectorforge::interestingness::ProfileId;
    match s.to_ascii_lowercase().as_str() {
        "political_sandbox" | "sandbox" => Ok(ProfileId::PoliticalSandbox),
        "grim_collapse" | "collapse" | "grim" => Ok(ProfileId::GrimCollapse),
        "mercantile" | "trade" => Ok(ProfileId::Mercantile),
        "villainous" | "villain" => Ok(ProfileId::Villainous),
        "frontier" => Ok(ProfileId::Frontier),
        other => Err(sectorforge::SectorError::InvalidConfig(format!(
            "unknown interestingness profile '{other}' (expected political_sandbox|grim_collapse|mercantile|villainous|frontier)"
        ))),
    }
}
