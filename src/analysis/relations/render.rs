//! Markdown render + report writer: turns a [`RelationsReport`] into the
//! `relations.md` digest (factions-at-war / hostile / full matrix / per-faction
//! dossiers) and writes `relations.md` + `relations.json` to disk.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use camino::Utf8Path;

use super::config::{FactionRelation, RelationsReport, Stance};
use crate::errors::SectorError;

#[must_use]
pub fn render_markdown(report: &RelationsReport) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Diplomacy — {}", report.sector_id);
    let _ = writeln!(s, "\nSeed: `{}`", report.seed);
    let _ = writeln!(s, "\nTotal pairs: **{}**", report.matrix.pairs.len());

    // Factions at war digest first — the headline.
    let at_war: Vec<&FactionRelation> = report
        .matrix
        .pairs
        .iter()
        .filter(|p| p.stance == Stance::AtWar)
        .collect();
    if !at_war.is_empty() {
        let _ = writeln!(s, "\n## Factions at war");
        for r in at_war {
            let _ = writeln!(
                s,
                "- **{} ↔ {}** — {} / {} (tension {:.0}; {})",
                r.a,
                r.b,
                r.public_attitude.label(),
                r.secret_attitude.label(),
                r.tension,
                r.cause
            );
        }
    }
    let hot: Vec<&FactionRelation> = report
        .matrix
        .pairs
        .iter()
        .filter(|p| p.stance == Stance::Hostile)
        .collect();
    if !hot.is_empty() {
        let _ = writeln!(s, "\n## Hostile pairs");
        for r in hot {
            let _ = writeln!(
                s,
                "- {} ↔ {} — {} / {} (tension {:.0}; {})",
                r.a,
                r.b,
                r.public_attitude.label(),
                r.secret_attitude.label(),
                r.tension,
                r.cause
            );
        }
    }

    let _ = writeln!(s, "\n## Full matrix");
    let _ = writeln!(
        s,
        "\n| A | B | Public | Secret | Treaty | Trust | Fear | Rivalry | Ideology | Econ | Military | Covert | Tension | Cause |"
    );
    let _ = writeln!(
        s,
        "|---|---|--------|--------|--------|------:|-----:|--------:|---------:|-----:|---------:|-------:|--------:|-------|"
    );
    for r in &report.matrix.pairs {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.0} | {} |",
            r.a,
            r.b,
            r.public_attitude.label(),
            r.secret_attitude.label(),
            r.treaty_status.label(),
            r.metrics.trust,
            r.metrics.fear,
            r.metrics.rivalry,
            r.metrics.ideological_distance,
            r.metrics.economic_dependency,
            r.metrics.military_pressure,
            r.metrics.covert_activity,
            r.tension,
            r.cause
        );
    }
    let _ = writeln!(s, "\n## Faction dossiers");
    let mut by_faction: BTreeMap<&str, Vec<&FactionRelation>> = BTreeMap::new();
    for r in &report.matrix.pairs {
        by_faction.entry(r.a.as_str()).or_default().push(r);
        by_faction.entry(r.b.as_str()).or_default().push(r);
    }
    for (fid, rels) in by_faction {
        let _ = writeln!(s, "\n### {fid}");
        for r in rels.into_iter().take(8) {
            let other = if r.a == fid { &r.b } else { &r.a };
            let view = if r.a == fid { &r.a_to_b } else { &r.b_to_a };
            let _ = writeln!(
                s,
                "- {}: public {}, secret {}, trust {}, fear {}, rivalry {}",
                other,
                view.public_attitude.label(),
                view.secret_attitude.label(),
                view.metrics.trust,
                view.metrics.fear,
                view.metrics.rivalry
            );
        }
    }
    s
}

/// Write `relations.md` + `relations.json` into a directory.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_report(output_dir: &Utf8Path, report: &RelationsReport) -> Result<(), SectorError> {
    crate::export::write_md_and_json(output_dir, "relations", &render_markdown(report), report)
}
