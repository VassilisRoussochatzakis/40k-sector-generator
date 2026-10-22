//! `briefing` — audience-targeted redaction pack.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::load_or_regenerate;

#[derive(Debug, clap::Args)]
pub(super) struct BriefingArgs {
    #[arg(long)]
    pub project: Option<Utf8PathBuf>,
    #[arg(long)]
    pub sector: Option<Utf8PathBuf>,
    #[arg(long)]
    pub out: Utf8PathBuf,
    /// Built-in preset to apply.
    #[arg(long)]
    pub preset: String,
    /// Observer faction id (defaults to none for GM / public).
    #[arg(long)]
    pub observer: Option<String>,
    /// Override the preset's intel confidence cutoff (0..=100).
    #[arg(long)]
    pub min_confidence: Option<u8>,
}

pub(crate) fn run_briefing(args: BriefingArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let BriefingArgs {
        project,
        sector,
        out,
        preset,
        observer,
        min_confidence,
    } = args;
    let project = project.as_ref();
    let sector = sector.as_ref();
    let out = &out;

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
