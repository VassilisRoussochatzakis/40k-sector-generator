//! Per-system heatmap aggregation for the sector map (design doc §9.5).
//!
//! Each mode reduces a sector down to one scalar per hex (and, for `Control`,
//! a faction id whose `FactionStyle` fill is used). The renderer normalises the
//! values across the sector to keep tints comparable run-to-run.

use std::collections::HashMap;

use egui::Color32;

use crate::sector_model::GeneratedSector;

use super::palette::{faction_style_by_id, HEX_EMPTY};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// Base tint for scalar heatmaps. `Control` ignores this and pulls colour
    /// from the dominant faction's style instead.
    fn base_color(self) -> Color32 {
        match self {
            HeatmapMode::Military | HeatmapMode::Threat => Color32::from_rgb(235, 90, 90),
            HeatmapMode::Trade => Color32::from_rgb(240, 200, 90),
            HeatmapMode::Industrial => Color32::from_rgb(220, 110, 50),
            HeatmapMode::Covert => Color32::from_rgb(150, 90, 220),
            HeatmapMode::Faith => Color32::from_rgb(230, 220, 90),
            HeatmapMode::Intel => Color32::from_rgb(120, 200, 240),
            _ => HEX_EMPTY,
        }
    }
}

/// One per-hex sample: tint colour plus a 0..=1 intensity. Intensity collapses
/// to 0 for the `Off` mode and for empty systems.
#[derive(Debug, Clone, Copy)]
pub struct HeatCell {
    pub color: Color32,
    pub intensity: f32,
}

/// Computed heatmap values keyed by system id.
pub fn compute(sector: &GeneratedSector, mode: HeatmapMode) -> HashMap<String, HeatCell> {
    let mut out = HashMap::new();
    if matches!(mode, HeatmapMode::Off) {
        return out;
    }

    // First pass: per-system raw score + (optional) dominant faction id.
    let mut raw: Vec<(String, f32, Option<String>)> = Vec::with_capacity(sector.systems.len());
    for sys in &sector.systems {
        let (score, dom) = score_system(sys, mode);
        raw.push((sys.id.clone(), score, dom));
    }
    let max = raw
        .iter()
        .map(|(_, s, _)| *s)
        .fold(0.0_f32, |a, b| a.max(b))
        .max(0.0001);

    for (id, score, dom) in raw {
        let intensity = (score / max).clamp(0.0, 1.0);
        let color = match (mode, dom.as_deref()) {
            (HeatmapMode::Control, Some(d)) => faction_style_by_id(&sector.factions, d).fill,
            _ => mode.base_color(),
        };
        out.insert(id, HeatCell { color, intensity });
    }
    out
}

fn score_system(
    sys: &crate::sector_model::GeneratedSystem,
    mode: HeatmapMode,
) -> (f32, Option<String>) {
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
