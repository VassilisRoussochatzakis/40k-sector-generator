//! Markdown rendering + on-disk report writer for the derived economy: the
//! sector-balance / strategic-output / systems / dependency-edge / stranded /
//! top-trade-lane tables, and `write_report` which emits `economy.md` +
//! `economy.json`.

use std::fmt::Write as _;

use camino::Utf8Path;

use crate::errors::SectorError;

use super::config::{EconomyReport, RouteEconomy, RESOURCE_KEYS, STRATEGIC_RESOURCE_KEYS};

// ── Markdown ───────────────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(sector_id: &str, report: &EconomyReport) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Economy — {sector_id}");
    if !report.enabled {
        let _ = writeln!(
            s,
            "\n_Economy derivation disabled. Enable in `economy.toml`._"
        );
        return s;
    }
    let _ = writeln!(s, "\n## Sector balance");
    let _ = writeln!(s, "| Resource | Net |");
    let _ = writeln!(s, "|----------|-----|");
    for k in RESOURCE_KEYS {
        let _ = writeln!(s, "| {k} | {:.1} |", report.sector_balance.get(k));
    }

    let _ = writeln!(s, "\n## Strategic output");
    let _ = writeln!(s, "| Output | Score |");
    let _ = writeln!(s, "|--------|------:|");
    for k in STRATEGIC_RESOURCE_KEYS {
        let _ = writeln!(s, "| {k} | {:.1} |", report.strategic_output.get(k));
    }

    let _ = writeln!(s, "\n## Systems");
    let _ = writeln!(
        s,
        "| System | Tithe | Supply | Priority | Surplus | Shortage |"
    );
    let _ = writeln!(
        s,
        "|--------|-------|--------|----------|---------|----------|"
    );
    for sy in &report.systems {
        let _ = writeln!(
            s,
            "| {} | {:?} | {:?} | {:?} | {} | {} |",
            sy.system_id,
            sy.tithe_status,
            sy.supply_risk,
            sy.strategic_priority,
            if sy.surplus_resources.is_empty() {
                "—".to_string()
            } else {
                sy.surplus_resources.join(", ")
            },
            if sy.shortage_resources.is_empty() {
                "—".to_string()
            } else {
                sy.shortage_resources.join(", ")
            },
        );
    }

    if !report.dependency_edges.is_empty() {
        let _ = writeln!(s, "\n## Dependency edges");
        let _ = writeln!(
            s,
            "| Supplier | Consumer | Resource | Route | Risk | Score |"
        );
        let _ = writeln!(
            s,
            "|----------|----------|----------|-------|------|------:|"
        );
        for e in &report.dependency_edges {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {:?} | {:.1} |",
                e.from_system_id,
                e.to_system_id,
                e.resource,
                e.route_id.as_deref().unwrap_or("—"),
                e.risk,
                e.score
            );
        }
    }

    let mut stranded_iter = report.worlds.iter().filter(|w| w.stranded).peekable();
    if stranded_iter.peek().is_some() {
        let _ = writeln!(s, "\n## Stranded worlds");
        for w in stranded_iter {
            let _ = writeln!(
                s,
                "- `{}` in `{}` — shortages: {}",
                w.world_id,
                w.system_id,
                if w.shortages.is_empty() {
                    "(systemic)".into()
                } else {
                    w.shortages.join(", ")
                }
            );
        }
    }

    // Top 10 routes by volume.
    let mut top: Vec<&RouteEconomy> = report.routes.iter().collect();
    top.sort_by(|a, b| crate::analysis::cmp_f32_desc(a.volume, b.volume));
    let _ = writeln!(s, "\n## Top trade lanes");
    for r in top.iter().take(10) {
        let _ = writeln!(
            s,
            "- {} → {} — volume {:.1} (friction {:.2})",
            r.from_system_id, r.to_system_id, r.volume, r.friction
        );
    }
    s
}

/// Write `economy.md` + `economy.json` into a dir.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_report(
    output_dir: &Utf8Path,
    sector_id: &str,
    report: &EconomyReport,
) -> Result<(), SectorError> {
    crate::export::write_md_and_json(
        output_dir,
        "economy",
        &render_markdown(sector_id, report),
        report,
    )
}
