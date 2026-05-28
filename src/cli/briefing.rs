//! `briefing` — audience-targeted redaction pack.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::load_or_regenerate;

pub fn run_briefing(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    out: &Utf8PathBuf,
    preset: String,
    observer: Option<String>,
    min_confidence: Option<u8>,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project.cloned(), sector.cloned())?;
    let audience = sectorforge::briefing::parse_preset(&preset).ok_or_else(|| {
        sectorforge::SectorError::InvalidConfig(format!(
            "unknown briefing preset '{preset}' (expected gm|navy|inquisition|trader|governor|public)"
        ))
    })?;
    let mut profile = sectorforge::briefing::preset(audience);
    if let Some(obs) = observer {
        profile.observer_faction = Some(obs.into());
    }
    if let Some(m) = min_confidence {
        profile.minimum_intel_confidence = m;
    }
    let pack = sectorforge::apply_briefing(&sec, &profile);
    sectorforge::write_briefing(out, &pack, &profile)?;
    println!(
        "Wrote {out}/briefing-{}.md and {out}/briefing-{}.json",
        profile.id, profile.id
    );
    Ok(ExitCode::SUCCESS)
}
