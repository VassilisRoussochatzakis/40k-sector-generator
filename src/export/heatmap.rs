//! Pure-data per-system heatmap aggregation (§9.5 / §10).
//!
//! Each mode reduces a sector down to one scalar per hex, with an optional
//! dominant faction id for `Control` mode. The GUI viewport and the PNG
//! bitmap exporter share this scoring; only the renderer differs.

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::sector_model::{GeneratedSector, GeneratedSystem};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HeatmapMode {
    #[default]
    Off,
    Control,
    Military,
    Trade,
    Industrial,
    Covert,
    Faith,
    Threat,
    Intel,
    /// §4: per-system tension derived from faction stance matrix +
    /// hostile/at-war pair co-occurrence.
    Tension,
    /// §12: per-system trade activity = sum of route volumes touching the
    /// system. Uses the economy derivation in `sector.economy`.
    TradeVolume,
    /// §4 NEW2.md: per-system strategic food output.
    FoodOutput,
    /// §4 NEW2.md: tithe stress, high = delinquent/failed/falsified.
    TitheStress,
    /// §4 NEW2.md: supply vulnerability, high = disrupted/collapsing.
    SupplyVulnerability,
    /// §28 CF6 / §35 T3: per-system conflict intensity sourced from
    /// `GeneratedSystem::conflict.intensity` (0..=100).
    ConflictIntensity,
}

impl HeatmapMode {
    pub const ALL: &'static [HeatmapMode] = &[
        HeatmapMode::Off,
        HeatmapMode::Control,
        HeatmapMode::Military,
        HeatmapMode::Trade,
        HeatmapMode::Industrial,
        HeatmapMode::Covert,
        HeatmapMode::Faith,
        HeatmapMode::Threat,
        HeatmapMode::Intel,
        HeatmapMode::Tension,
        HeatmapMode::TradeVolume,
        HeatmapMode::FoodOutput,
        HeatmapMode::TitheStress,
        HeatmapMode::SupplyVulnerability,
        HeatmapMode::ConflictIntensity,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HeatmapMode::Off => "OFF",
            HeatmapMode::Control => "CONTROL",
            HeatmapMode::Military => "MILITARY",
            HeatmapMode::Trade => "TRADE",
            HeatmapMode::Industrial => "INDUSTRY",
            HeatmapMode::Covert => "COVERT",
            HeatmapMode::Faith => "FAITH",
            HeatmapMode::Threat => "THREAT",
            HeatmapMode::Intel => "INTEL",
            HeatmapMode::Tension => "TENSION",
            HeatmapMode::TradeVolume => "TRADE VOL",
            HeatmapMode::FoodOutput => "FOOD",
            HeatmapMode::TitheStress => "TITHE",
            HeatmapMode::SupplyVulnerability => "SUPPLY",
            HeatmapMode::ConflictIntensity => "CONFLICT",
        }
    }

    /// Base tint for scalar heatmaps (RGB). `Control` ignores this and pulls
    /// colour from the dominant faction's style.
    #[must_use]
    pub fn base_color_rgb(self) -> (u8, u8, u8) {
        match self {
            HeatmapMode::Military | HeatmapMode::Threat | HeatmapMode::Tension => (235, 90, 90),
            HeatmapMode::Trade => (240, 200, 90),
            HeatmapMode::TradeVolume => (255, 175, 60),
            HeatmapMode::FoodOutput => (80, 190, 120),
            HeatmapMode::TitheStress => (220, 80, 70),
            HeatmapMode::SupplyVulnerability => (235, 120, 60),
            HeatmapMode::ConflictIntensity => (215, 70, 90),
            HeatmapMode::Industrial => (220, 110, 50),
            HeatmapMode::Covert => (150, 90, 220),
            HeatmapMode::Faith => (230, 220, 90),
            HeatmapMode::Intel => (120, 200, 240),
            _ => (28, 26, 38),
        }
    }
}

enum_slug!(HeatmapMode {
    Off => "off",
    Control => "control",
    Military => "military",
    Trade => "trade",
    Industrial => "industrial",
    Covert => "covert",
    Faith => "faith",
    Threat => "threat",
    Intel => "intel",
    Tension => "tension",
    TradeVolume => "trade_volume",
    FoodOutput => "food_output",
    TitheStress => "tithe_stress",
    SupplyVulnerability => "supply_vulnerability",
    ConflictIntensity => "conflict_intensity",
});

/// §35 T3: the per-dimension stability heatmap family. Each variant maps to one
/// 0.0..=100.0 field of [`crate::stability::StabilityState`].
///
/// Kept deliberately separate from [`HeatmapMode`] so the PNG/SVG bitmap
/// exporter enum — and its byte-stable golden output — stays untouched. This
/// family is a GUI-only overlay computed straight off the finalised sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StabilityDimension {
    PublicOrder,
    Corruption,
    Fear,
    RebellionRisk,
    XenosThreat,
    WarpInstability,
    FamineResourceStress,
}

impl StabilityDimension {
    pub const ALL: &'static [StabilityDimension] = &[
        StabilityDimension::PublicOrder,
        StabilityDimension::Corruption,
        StabilityDimension::Fear,
        StabilityDimension::RebellionRisk,
        StabilityDimension::XenosThreat,
        StabilityDimension::WarpInstability,
        StabilityDimension::FamineResourceStress,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::PublicOrder => "PUBLIC ORDER",
            Self::Corruption => "CORRUPTION",
            Self::Fear => "FEAR",
            Self::RebellionRisk => "REBELLION",
            Self::XenosThreat => "XENOS THREAT",
            Self::WarpInstability => "WARP INSTABILITY",
            Self::FamineResourceStress => "FAMINE / RESOURCE",
        }
    }

    /// Pull this dimension's scalar (0.0..=100.0) out of a stability snapshot.
    #[must_use]
    pub fn value(self, s: &crate::stability::StabilityState) -> f32 {
        match self {
            Self::PublicOrder => s.public_order,
            Self::Corruption => s.corruption,
            Self::Fear => s.fear,
            Self::RebellionRisk => s.rebellion_risk,
            Self::XenosThreat => s.xenos_threat,
            Self::WarpInstability => s.warp_instability,
            Self::FamineResourceStress => s.famine_or_resource_stress,
        }
    }

    /// Tint for this dimension's scalar heatmap (RGB).
    #[must_use]
    pub fn base_color_rgb(self) -> (u8, u8, u8) {
        match self {
            Self::PublicOrder => (80, 190, 120),
            Self::Corruption => (150, 90, 220),
            Self::Fear => (200, 60, 90),
            Self::RebellionRisk => (235, 120, 60),
            Self::XenosThreat => (120, 200, 90),
            Self::WarpInstability => (190, 70, 160),
            Self::FamineResourceStress => (220, 170, 70),
        }
    }
}

enum_slug!(StabilityDimension {
    PublicOrder => "public_order",
    Corruption => "corruption",
    Fear => "fear",
    RebellionRisk => "rebellion_risk",
    XenosThreat => "xenos_threat",
    WarpInstability => "warp_instability",
    FamineResourceStress => "famine_resource_stress",
});

/// Per-system raw score + optional dominant faction id (for `Control` mode).
#[derive(Debug, Clone)]
pub struct SystemScore {
    pub system_id: crate::ids::SystemId,
    pub score: f32,
    pub dominant: Option<crate::ids::FactionId>,
}

/// Per-system normalised intensity (0..=1) and the colour the renderer should
/// use for the tint, given the sector's faction list.
#[derive(Debug, Clone, Copy)]
pub struct HeatCellRgb {
    pub rgb: (u8, u8, u8),
    pub intensity: f32,
}

/// Score every system. Empty map for `HeatmapMode::Off`.
#[must_use]
pub fn score_sector(sector: &GeneratedSector, mode: HeatmapMode) -> Vec<SystemScore> {
    if matches!(mode, HeatmapMode::Off) {
        return Vec::new();
    }
    sector
        .systems
        .iter()
        .map(|sys| {
            let (score, dominant) = score_system_in(sector, sys, mode);
            SystemScore {
                system_id: sys.id.clone(),
                score,
                dominant,
            }
        })
        .collect()
}

/// Normalise per-system scores to [0, 1] and resolve a tint per cell.
/// Returns an empty map when `mode == Off`.
#[must_use]
pub fn compute_rgb(
    sector: &GeneratedSector,
    mode: HeatmapMode,
) -> HashMap<crate::ids::SystemId, HeatCellRgb> {
    let mut out = HashMap::new();
    if matches!(mode, HeatmapMode::Off) {
        return out;
    }
    let scores = score_sector(sector, mode);
    let max = scores
        .iter()
        .map(|s| s.score)
        .fold(0.0_f32, |a, b| a.max(b))
        .max(0.0001);
    for SystemScore {
        system_id,
        score,
        dominant,
    } in scores
    {
        let intensity = (score / max).clamp(0.0, 1.0);
        let rgb = match (mode, dominant.as_deref()) {
            (HeatmapMode::Control, Some(d)) => {
                crate::faction_style::faction_style_rgb_by_id(&sector.factions, d).fill
            }
            _ => mode.base_color_rgb(),
        };
        out.insert(system_id, HeatCellRgb { rgb, intensity });
    }
    out
}

/// §35 T3: per-dimension stability heatmap. Scores every system from its
/// finalised `stability` snapshot, normalised to [0, 1] against the sector
/// maximum, tinted with [`StabilityDimension::base_color_rgb`].
#[must_use]
pub fn compute_stability_rgb(
    sector: &GeneratedSector,
    dim: StabilityDimension,
) -> HashMap<crate::ids::SystemId, HeatCellRgb> {
    let mut out = HashMap::new();
    let max = sector
        .systems
        .iter()
        .map(|sys| dim.value(&sys.stability))
        .fold(0.0_f32, f32::max)
        .max(0.0001);
    let rgb = dim.base_color_rgb();
    for sys in &sector.systems {
        let intensity = (dim.value(&sys.stability) / max).clamp(0.0, 1.0);
        out.insert(sys.id.clone(), HeatCellRgb { rgb, intensity });
    }
    out
}

fn score_system_in(
    sector: &GeneratedSector,
    sys: &GeneratedSystem,
    mode: HeatmapMode,
) -> (f32, Option<crate::ids::FactionId>) {
    // Modes that need sector-wide context, not just per-world faction power.
    match mode {
        HeatmapMode::Tension => {
            let mut ids: BTreeSet<&str> = BTreeSet::new();
            for w in &sys.worlds {
                for p in &w.factions {
                    ids.insert(p.faction_id.as_str());
                }
            }
            let mut score = 0.0_f32;
            for pair in &sector.relations.pairs {
                if pair.stance.is_hot()
                    && ids.contains(pair.a.as_str())
                    && ids.contains(pair.b.as_str())
                {
                    score += pair.tension;
                }
            }
            return (score, None);
        }
        HeatmapMode::TradeVolume => {
            let mut score = 0.0_f32;
            for r in &sector.economy.routes {
                if r.from_system_id == sys.id || r.to_system_id == sys.id {
                    score += r.volume;
                }
            }
            return (score, None);
        }
        HeatmapMode::FoodOutput => {
            let score = sector
                .economy
                .systems
                .iter()
                .find(|s| s.system_id == sys.id)
                .map(|s| s.strategic_output.food)
                .unwrap_or(0.0);
            return (score, None);
        }
        HeatmapMode::TitheStress => {
            let score = sector
                .economy
                .systems
                .iter()
                .find(|s| s.system_id == sys.id)
                .map(|s| match s.tithe_status {
                    crate::economy::TitheStatus::Surplus => 0.0,
                    crate::economy::TitheStatus::Adequate => 10.0,
                    crate::economy::TitheStatus::Strained => 35.0,
                    crate::economy::TitheStatus::Delinquent => 65.0,
                    crate::economy::TitheStatus::Failed => 100.0,
                    crate::economy::TitheStatus::Falsified => 80.0,
                })
                .unwrap_or(0.0);
            return (score, None);
        }
        HeatmapMode::SupplyVulnerability => {
            let score = sector
                .economy
                .systems
                .iter()
                .find(|s| s.system_id == sys.id)
                .map(|s| match s.supply_risk {
                    crate::economy::SupplyRisk::Stable => 0.0,
                    crate::economy::SupplyRisk::Vulnerable => 35.0,
                    crate::economy::SupplyRisk::Disrupted => 70.0,
                    crate::economy::SupplyRisk::Collapsing => 100.0,
                })
                .unwrap_or(0.0);
            return (score, None);
        }
        HeatmapMode::ConflictIntensity => {
            return (f32::from(sys.conflict.intensity), None);
        }
        _ => {}
    }

    let mut score = 0.0_f32;
    for w in &sys.worlds {
        for p in &w.factions {
            let v = match mode {
                HeatmapMode::Off => 0.0,
                HeatmapMode::Control => p.dimensions.local_control_score(),
                HeatmapMode::Military => p.dimensions.military,
                HeatmapMode::Trade => p.dimensions.economic,
                HeatmapMode::Industrial => p.dimensions.industrial,
                HeatmapMode::Covert => p.dimensions.covert,
                HeatmapMode::Faith => p.dimensions.ideological,
                HeatmapMode::Threat => {
                    if matches!(p.relationship_to_government.as_ref(), "hostile" | "zealous") {
                        p.dimensions
                            .military
                            .mul_add(1.2, p.dimensions.covert * 0.4)
                    } else {
                        0.0
                    }
                }
                HeatmapMode::Intel => 100.0 - p.dimensions.visibility,
                HeatmapMode::Tension
                | HeatmapMode::TradeVolume
                | HeatmapMode::FoodOutput
                | HeatmapMode::TitheStress
                | HeatmapMode::SupplyVulnerability
                | HeatmapMode::ConflictIntensity => 0.0,
            };
            score += v;
        }
    }
    let dom = if matches!(mode, HeatmapMode::Control) {
        sys.control.dominant.clone()
    } else {
        None
    };
    (score, dom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SystemId;
    use crate::sector_model::{GeneratedSystem, HexCoord};

    #[test]
    fn stability_dimension_all_covers_seven_and_round_trips() {
        assert_eq!(StabilityDimension::ALL.len(), 7);
        for dim in StabilityDimension::ALL {
            assert!(!dim.label().is_empty());
            assert!(!dim.as_slug().is_empty());
            // slug is the snake_case serde name.
            let json = serde_json::to_string(dim).unwrap();
            assert_eq!(json, format!("\"{}\"", dim.as_slug()));
        }
    }

    #[test]
    fn stability_value_reads_the_matching_field() {
        let s = crate::stability::StabilityState {
            fear: 42.0,
            corruption: 7.0,
            ..Default::default()
        };
        assert_eq!(StabilityDimension::Fear.value(&s), 42.0);
        assert_eq!(StabilityDimension::Corruption.value(&s), 7.0);
        assert_eq!(StabilityDimension::PublicOrder.value(&s), 0.0);
    }

    #[test]
    fn compute_stability_rgb_normalises_against_sector_max() {
        let mut sector = GeneratedSector::empty("s", "S", "seed", 4, 4);
        let id_a = SystemId::new("sys-0001");
        let id_b = SystemId::new("sys-0002");
        let mut a = GeneratedSystem::new_at(id_a.clone(), 0, HexCoord { q: 0, r: 0 }, "A");
        let mut b = GeneratedSystem::new_at(id_b.clone(), 1, HexCoord { q: 1, r: 0 }, "B");
        a.stability.fear = 50.0;
        b.stability.fear = 100.0;
        sector.systems.push(a);
        sector.systems.push(b);

        let cells = compute_stability_rgb(&sector, StabilityDimension::Fear);
        assert!((cells[&id_a].intensity - 0.5).abs() < 1e-6);
        assert!((cells[&id_b].intensity - 1.0).abs() < 1e-6);
        assert_eq!(cells[&id_a].rgb, StabilityDimension::Fear.base_color_rgb());
    }

    #[test]
    fn compute_stability_rgb_is_empty_for_empty_sector() {
        let sector = GeneratedSector::empty("s", "S", "seed", 4, 4);
        assert!(compute_stability_rgb(&sector, StabilityDimension::PublicOrder).is_empty());
    }
}
