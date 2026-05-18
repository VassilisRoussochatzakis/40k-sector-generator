//! Pure-data per-system heatmap aggregation (§9.5 / §10).
//!
//! Each mode reduces a sector down to one scalar per hex, with an optional
//! dominant faction id for `Control` mode. The GUI viewport and the PNG
//! bitmap exporter share this scoring; only the renderer differs.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::sector_model::{GeneratedSector, GeneratedSystem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeatmapMode {
    Off,
    Control,
    Military,
    Trade,
    Industrial,
    Covert,
    Faith,
    Threat,
    Intel,
}

impl Default for HeatmapMode {
    fn default() -> Self {
        Self::Off
    }
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
        }
    }

    /// Base tint for scalar heatmaps (RGB). `Control` ignores this and pulls
    /// colour from the dominant faction's style.
    #[must_use]
    pub fn base_color_rgb(self) -> (u8, u8, u8) {
        match self {
            HeatmapMode::Military | HeatmapMode::Threat => (235, 90, 90),
            HeatmapMode::Trade => (240, 200, 90),
            HeatmapMode::Industrial => (220, 110, 50),
            HeatmapMode::Covert => (150, 90, 220),
            HeatmapMode::Faith => (230, 220, 90),
            HeatmapMode::Intel => (120, 200, 240),
            _ => (28, 26, 38),
        }
    }
}

/// Per-system raw score + optional dominant faction id (for `Control` mode).
#[derive(Debug, Clone)]
pub struct SystemScore {
    pub system_id: String,
    pub score: f32,
    pub dominant: Option<String>,
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
            let (score, dominant) = score_system(sys, mode);
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
pub fn compute_rgb(sector: &GeneratedSector, mode: HeatmapMode) -> HashMap<String, HeatCellRgb> {
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

fn score_system(sys: &GeneratedSystem, mode: HeatmapMode) -> (f32, Option<String>) {
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
                    if matches!(p.relationship_to_government.as_str(), "hostile" | "zealous") {
                        p.dimensions.military * 1.2 + p.dimensions.covert * 0.4
                    } else {
                        0.0
                    }
                }
                HeatmapMode::Intel => 100.0 - p.dimensions.visibility,
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
