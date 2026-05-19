//! §18 NEW2.md: Interestingness scorecard.
//!
//! Distinct from [`crate::analytics`]: instead of raw metrics + thresholds it
//! evaluates the sector against *target profiles* (political sandbox, grim
//! collapse, frontier expansion …). Each metric carries a weight + target
//! band; the final score is the weighted geometric of band fits.
//!
//! Pure read-only derivation. Same sector ⇒ same scorecard.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;

use camino::Utf8Path;
use serde::{Deserialize, Serialize};

use crate::analytics::{analyze, SectorAnalysis};
use crate::errors::SectorError;
use crate::sector_model::GeneratedSector;

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InterestingnessConfig {
    /// Built-in profile id. When `custom` is set, this is ignored.
    #[serde(default = "default_profile")]
    pub profile: ProfileId,
    /// Optional explicit metric targets that override the chosen profile.
    #[serde(default)]
    pub custom: BTreeMap<String, MetricTarget>,
}

impl Default for InterestingnessConfig {
    fn default() -> Self {
        Self {
            profile: default_profile(),
            custom: BTreeMap::new(),
        }
    }
}

fn default_profile() -> ProfileId {
    ProfileId::PoliticalSandbox
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProfileId {
    /// Balanced multi-faction with several contested worlds + connectivity.
    PoliticalSandbox,
    /// Bleak Imperial collapse: high contested ratio, fragmented routes.
    GrimCollapse,
    /// Heavy trade focus: high connectivity, low warzone count, surplus flows.
    Mercantile,
    /// One dominant villain: extreme Gini, low contested ratio.
    Villainous,
    /// Sparse pioneers: lots of isolated systems, few factions, low contested.
    Frontier,
}

/// One metric target band. `low..=high` defines the "ideal" range; outside it
/// the fit decays linearly to 0 at `floor`/`ceil`.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct MetricTarget {
    pub low: f32,
    pub high: f32,
    #[serde(default = "default_floor")]
    pub floor: f32,
    #[serde(default = "default_ceil")]
    pub ceil: f32,
    #[serde(default = "default_weight")]
    pub weight: f32,
}

fn default_floor() -> f32 {
    0.0
}
fn default_ceil() -> f32 {
    f32::INFINITY
}
fn default_weight() -> f32 {
    1.0
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterestingnessReport {
    pub sector_id: String,
    pub seed: String,
    pub profile: String,
    /// 0..=100 overall sector interestingness against the target profile.
    pub overall: u8,
    pub metric_scores: Vec<MetricScore>,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricScore {
    pub name: String,
    /// Raw observed value.
    pub observed: f32,
    pub target_low: f32,
    pub target_high: f32,
    /// 0..=1 fit against the target band.
    pub fit: f32,
    pub weight: f32,
}

// ── Entry points ───────────────────────────────────────────────────────────────

#[must_use]
pub fn derive(sector: &GeneratedSector) -> InterestingnessReport {
    derive_with(sector, &InterestingnessConfig::default())
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &InterestingnessConfig) -> InterestingnessReport {
    let analysis = analyze(sector);
    let mut targets = profile_targets(cfg.profile);
    for (k, v) in &cfg.custom {
        targets.insert(k.clone(), *v);
    }
    let mut scores: Vec<MetricScore> = Vec::new();
    let observed = observed_metrics(&analysis, sector);
    for (name, target) in &targets {
        let obs = observed.get(name.as_str()).copied().unwrap_or(0.0);
        let fit = band_fit(obs, target);
        scores.push(MetricScore {
            name: name.clone(),
            observed: obs,
            target_low: target.low,
            target_high: target.high,
            fit,
            weight: target.weight,
        });
    }
    scores.sort_by(|a, b| a.name.cmp(&b.name));

    let (overall, strengths, weaknesses) = aggregate(&scores);
    InterestingnessReport {
        sector_id: sector.id.clone(),
        seed: sector.seed.clone(),
        profile: format!("{:?}", cfg.profile),
        overall,
        metric_scores: scores,
        strengths,
        weaknesses,
    }
}

// ── Metric extraction ──────────────────────────────────────────────────────────

fn observed_metrics(a: &SectorAnalysis, sector: &GeneratedSector) -> BTreeMap<&'static str, f32> {
    let mut m: BTreeMap<&'static str, f32> = BTreeMap::new();
    m.insert("faction_gini", a.faction_balance.gini);
    m.insert("contested_world_ratio", a.contested_world_ratio);
    m.insert("avg_claims_per_world", a.avg_claims_per_world);
    m.insert("route_components", a.connectivity.component_count as f32);
    m.insert(
        "articulation_points",
        a.connectivity.articulation_point_ids.len() as f32,
    );
    m.insert(
        "isolated_systems",
        a.connectivity.isolated_system_ids.len() as f32,
    );
    m.insert(
        "route_diameter",
        a.connectivity.diameter_hops.unwrap_or(0) as f32,
    );
    m.insert(
        "world_type_diversity",
        a.world_type_distribution.len() as f32,
    );
    m.insert(
        "warzone_count",
        a.system_state_counts.get("Warzone").copied().unwrap_or(0) as f32,
    );
    m.insert(
        "blockaded_count",
        a.system_state_counts.get("Blockaded").copied().unwrap_or(0) as f32,
    );
    m.insert(
        "infiltrated_count",
        a.system_state_counts
            .get("Infiltrated")
            .copied()
            .unwrap_or(0) as f32,
    );
    // Asymmetric control: per-world dominant != sovereign != hidden_master.
    let asymmetric = sector
        .systems
        .iter()
        .flat_map(|s| s.worlds.iter())
        .filter(|w| {
            let d = w.control.dominant.as_deref();
            let sov = w.control.sovereign.as_deref();
            let hm = w.control.hidden_master.as_deref();
            (d.is_some() && sov.is_some() && d != sov) || (hm.is_some() && hm != d)
        })
        .count() as f32;
    m.insert("asymmetric_control_worlds", asymmetric);
    m.insert("faction_count", sector.factions.len() as f32);
    m
}

// ── Profile targets ────────────────────────────────────────────────────────────

fn profile_targets(p: ProfileId) -> BTreeMap<String, MetricTarget> {
    let mut m: BTreeMap<String, MetricTarget> = BTreeMap::new();
    match p {
        ProfileId::PoliticalSandbox => {
            band(&mut m, "faction_gini", 0.30, 0.55, 1.0, 0.7, 1.0);
            band(&mut m, "contested_world_ratio", 0.20, 0.50, 0.0, 1.0, 1.0);
            band(&mut m, "warzone_count", 1.0, 5.0, 0.0, 12.0, 0.6);
            band(
                &mut m,
                "asymmetric_control_worlds",
                3.0,
                20.0,
                0.0,
                80.0,
                0.8,
            );
            band(&mut m, "route_components", 1.0, 1.0, 1.0, 4.0, 0.7);
            band(&mut m, "articulation_points", 0.0, 3.0, 0.0, 10.0, 0.5);
            band(&mut m, "world_type_diversity", 6.0, 14.0, 1.0, 25.0, 0.5);
            band(&mut m, "faction_count", 5.0, 12.0, 1.0, 40.0, 0.6);
        }
        ProfileId::GrimCollapse => {
            band(&mut m, "faction_gini", 0.45, 0.75, 0.2, 1.0, 0.8);
            band(&mut m, "contested_world_ratio", 0.50, 0.90, 0.0, 1.0, 1.2);
            band(&mut m, "warzone_count", 4.0, 12.0, 0.0, 30.0, 1.0);
            band(&mut m, "infiltrated_count", 2.0, 8.0, 0.0, 30.0, 0.7);
            band(&mut m, "route_components", 2.0, 5.0, 1.0, 12.0, 0.6);
            band(&mut m, "isolated_systems", 1.0, 6.0, 0.0, 30.0, 0.5);
            band(
                &mut m,
                "asymmetric_control_worlds",
                5.0,
                30.0,
                0.0,
                100.0,
                0.7,
            );
        }
        ProfileId::Mercantile => {
            band(&mut m, "faction_gini", 0.20, 0.45, 0.0, 0.9, 0.7);
            band(&mut m, "contested_world_ratio", 0.05, 0.30, 0.0, 0.7, 0.6);
            band(&mut m, "warzone_count", 0.0, 2.0, 0.0, 10.0, 0.8);
            band(&mut m, "route_components", 1.0, 1.0, 1.0, 3.0, 1.2);
            band(&mut m, "articulation_points", 0.0, 1.0, 0.0, 8.0, 0.7);
            band(&mut m, "route_diameter", 3.0, 8.0, 1.0, 20.0, 0.7);
            band(&mut m, "world_type_diversity", 8.0, 18.0, 1.0, 25.0, 0.5);
        }
        ProfileId::Villainous => {
            band(&mut m, "faction_gini", 0.60, 0.95, 0.3, 1.0, 1.2);
            band(&mut m, "contested_world_ratio", 0.05, 0.25, 0.0, 1.0, 0.6);
            band(&mut m, "warzone_count", 0.0, 4.0, 0.0, 20.0, 0.5);
            band(&mut m, "infiltrated_count", 0.0, 6.0, 0.0, 30.0, 0.4);
            band(
                &mut m,
                "asymmetric_control_worlds",
                0.0,
                6.0,
                0.0,
                30.0,
                0.5,
            );
        }
        ProfileId::Frontier => {
            band(&mut m, "faction_count", 2.0, 5.0, 1.0, 20.0, 0.9);
            band(&mut m, "isolated_systems", 2.0, 10.0, 0.0, 30.0, 1.0);
            band(&mut m, "route_components", 2.0, 6.0, 1.0, 20.0, 0.8);
            band(&mut m, "contested_world_ratio", 0.0, 0.20, 0.0, 0.8, 0.6);
            band(&mut m, "world_type_diversity", 3.0, 10.0, 1.0, 20.0, 0.4);
        }
    }
    m
}

fn band(
    out: &mut BTreeMap<String, MetricTarget>,
    name: &str,
    low: f32,
    high: f32,
    floor: f32,
    ceil: f32,
    weight: f32,
) {
    out.insert(
        name.to_string(),
        MetricTarget {
            low,
            high,
            floor,
            ceil,
            weight,
        },
    );
}

// ── Scoring ────────────────────────────────────────────────────────────────────

fn band_fit(obs: f32, t: &MetricTarget) -> f32 {
    if obs.is_nan() {
        return 0.0;
    }
    if obs >= t.low && obs <= t.high {
        return 1.0;
    }
    if obs < t.low {
        if obs <= t.floor {
            return 0.0;
        }
        return ((obs - t.floor) / (t.low - t.floor)).clamp(0.0, 1.0);
    }
    // obs > high
    if t.ceil.is_infinite() {
        // Soft decay: each 100% over high → -10%.
        let over = (obs - t.high).max(0.0);
        let denom = t.high.abs().max(1.0);
        return (1.0 - over / (10.0 * denom)).clamp(0.0, 1.0);
    }
    if obs >= t.ceil {
        return 0.0;
    }
    ((t.ceil - obs) / (t.ceil - t.high)).clamp(0.0, 1.0)
}

fn aggregate(scores: &[MetricScore]) -> (u8, Vec<String>, Vec<String>) {
    let total_w: f32 = scores.iter().map(|s| s.weight).sum();
    let overall = if total_w > 0.0 {
        let weighted: f32 = scores.iter().map(|s| s.fit * s.weight).sum();
        ((weighted / total_w) * 100.0).round().clamp(0.0, 100.0) as u8
    } else {
        0
    };
    let mut strengths: Vec<_> = scores
        .iter()
        .filter(|s| s.fit >= 0.9)
        .map(|s| describe(s, true))
        .collect();
    let mut weaknesses: Vec<_> = scores
        .iter()
        .filter(|s| s.fit < 0.5)
        .map(|s| describe(s, false))
        .collect();
    strengths.sort();
    weaknesses.sort();
    (overall, strengths, weaknesses)
}

fn describe(s: &MetricScore, strong: bool) -> String {
    if strong {
        format!(
            "{}: {:.2} within target {:.2}..={:.2}",
            s.name, s.observed, s.target_low, s.target_high
        )
    } else if s.observed < s.target_low {
        format!(
            "{}: {:.2} below target band {:.2}..={:.2}",
            s.name, s.observed, s.target_low, s.target_high
        )
    } else {
        format!(
            "{}: {:.2} above target band {:.2}..={:.2}",
            s.name, s.observed, s.target_low, s.target_high
        )
    }
}

// ── Markdown rendering ─────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(r: &InterestingnessReport) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Interestingness Scorecard — {}", r.sector_id);
    let _ = writeln!(s, "\nSeed: `{}`  ·  Profile: **{}**", r.seed, r.profile);
    let _ = writeln!(s, "\n## Overall: **{} / 100**", r.overall);

    if !r.strengths.is_empty() {
        let _ = writeln!(s, "\n### Strengths");
        for line in &r.strengths {
            let _ = writeln!(s, "- {line}");
        }
    }
    if !r.weaknesses.is_empty() {
        let _ = writeln!(s, "\n### Weaknesses");
        for line in &r.weaknesses {
            let _ = writeln!(s, "- {line}");
        }
    }

    let _ = writeln!(s, "\n## Metrics");
    let _ = writeln!(s, "\n| Metric | Observed | Target band | Fit | Weight |");
    let _ = writeln!(s, "|---|---:|---:|---:|---:|");
    for ms in &r.metric_scores {
        let _ = writeln!(
            s,
            "| {} | {:.2} | {:.2}..={:.2} | {:.0}% | {:.1} |",
            ms.name,
            ms.observed,
            ms.target_low,
            ms.target_high,
            ms.fit * 100.0,
            ms.weight
        );
    }
    s
}

/// Write `interestingness.md` + `interestingness.json` into `output_dir`.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_report(
    output_dir: &Utf8Path,
    report: &InterestingnessReport,
) -> Result<(), SectorError> {
    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;
    let md_path = output_dir.join("interestingness.md");
    fs::write(&md_path, render_markdown(report))
        .map_err(|e| SectorError::io(md_path.as_str(), e))?;
    let json_path = output_dir.join("interestingness.json");
    let json = serde_json::to_string_pretty(report)
        .map_err(|e| SectorError::export(json_path.as_str(), e.to_string()))?;
    fs::write(&json_path, json).map_err(|e| SectorError::io(json_path.as_str(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::GenerationManifest;
    use std::collections::BTreeMap as Map;

    fn empty_sector() -> GeneratedSector {
        GeneratedSector {
            id: "test".into(),
            title: "Test".into(),
            seed: "seed".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0".into(),
            width: 4,
            height: 4,
            systems: vec![],
            routes: vec![],
            factions: vec![],
            manifest: GenerationManifest {
                project_id: "t".into(),
                generated_at_policy: "n".into(),
                generator_name: "sf".into(),
                generator_version: "0".into(),
                seed: "s".into(),
                seed_hash: "h".into(),
                profile: None,
                input_digests: Map::new(),
                settings_digest: "d".into(),
                system_count: 0,
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new(),
            economy: Default::default(),
        }
    }

    #[test]
    fn band_fit_inside_band_is_one() {
        let t = MetricTarget {
            low: 0.3,
            high: 0.5,
            floor: 0.0,
            ceil: 1.0,
            weight: 1.0,
        };
        assert!((band_fit(0.4, &t) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn band_fit_at_floor_is_zero() {
        let t = MetricTarget {
            low: 0.3,
            high: 0.5,
            floor: 0.0,
            ceil: 1.0,
            weight: 1.0,
        };
        assert_eq!(band_fit(0.0, &t), 0.0);
    }

    #[test]
    fn deterministic_score_on_empty_sector() {
        let s = empty_sector();
        let a = derive(&s);
        let b = derive(&s);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }
}
