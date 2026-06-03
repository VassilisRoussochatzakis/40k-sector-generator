//! Sector analytics dashboard (§8 NEW.md). Lazy-computes a `SectorAnalysis`
//! from the currently-loaded sector and renders it as a scrollable panel with
//! a faction-share bar chart, a connectivity callout, and the health-flag list.

use egui::{Color32, RichText, Stroke, Ui};

use sectorforge::analytics::{self, AnalyzeConfig, FlagSeverity, SectorAnalysis};
use sectorforge::sector_model::GeneratedSector;

use super::palette::{self, faction_style_by_id};

#[derive(Default)]
pub struct DashboardState {
    analysis: Option<SectorAnalysis>,
    /// Cached sector id so we recompute when the user reloads.
    cached_sector_id: Option<std::sync::Arc<str>>,
    /// Cached seed so we also recompute after a re-roll (id stays the same).
    cached_seed: Option<std::sync::Arc<str>>,
}

impl DashboardState {
    pub fn ensure_for(&mut self, sector: &GeneratedSector) {
        let needs = match (&self.cached_sector_id, &self.cached_seed) {
            (Some(id), Some(seed)) => id != &sector.id || seed != &sector.seed,
            _ => true,
        };
        if needs || self.analysis.is_none() {
            self.analysis = Some(analytics::analyze_with(sector, &AnalyzeConfig::default()));
            self.cached_sector_id = Some(sector.id.clone());
            self.cached_seed = Some(sector.seed.clone());
        }
    }

    pub fn invalidate(&mut self) {
        self.analysis = None;
        self.cached_sector_id = None;
        self.cached_seed = None;
    }
}

pub fn show(ui: &mut Ui, sector: &GeneratedSector, state: &mut DashboardState) {
    state.ensure_for(sector);
    let Some(a) = state.analysis.as_ref() else {
        ui.label(RichText::new("analysis unavailable").color(palette::chrome_text_dim()));
        return;
    };

    ui.label(
        RichText::new(format!("DASHBOARD — {}", a.sector_id.to_uppercase()))
            .color(palette::chrome_text())
            .strong(),
    );
    ui.label(
        RichText::new(format!(
            "{} systems · {} worlds · {} routes · {} factions",
            a.system_count, a.world_count, a.route_count, a.faction_count
        ))
        .color(palette::chrome_text_dim()),
    );
    ui.add_space(8.0);

    if a.low_confidence {
        ui.label(
            RichText::new("⚠ low-confidence sector (few systems)")
                .color(Color32::from_rgb(235, 200, 90)),
        );
        ui.add_space(6.0);
    }

    // §UO P4: each analytic block is a titled `ui_kit::section` so the dashboard
    // reads as framed tier-2 boxes instead of one separator-divided wall.
    crate::ui_kit::section(ui, "FACTION BALANCE", |ui| {
        ui.label(
            RichText::new(format!(
                "Gini {:.3}   (0 = even, 1 = winner-take-all)",
                a.faction_balance.gini
            ))
            .color(palette::chrome_text_dim()),
        );
        ui.add_space(4.0);
        const TOP: usize = 12;
        for (i, f) in a.faction_balance.top_factions.iter().take(TOP).enumerate() {
            share_bar(ui, &sector.factions, &f.name, &f.faction_id, f.share, i);
        }
        let total = a.faction_balance.top_factions.len();
        if total > TOP {
            ui.label(
                RichText::new(format!("… {} more factions", total - TOP))
                    .color(palette::chrome_text_dim()),
            );
        }
    });

    crate::ui_kit::section(ui, "POLITICAL STATE", |ui| {
        ui.label(
            RichText::new(format!(
                "Contested worlds: {:.0}% · Avg claims/world: {:.2}",
                a.contested_world_ratio * 100.0,
                a.avg_claims_per_world
            ))
            .color(palette::chrome_text_dim()),
        );
        if !a.system_state_counts.is_empty() {
            for (k, v) in &a.system_state_counts {
                ui.label(RichText::new(format!("  {k}: {v}")).color(palette::chrome_text_dim()));
            }
        }
    });

    crate::ui_kit::section(ui, "CONNECTIVITY", |ui| {
        let c = &a.connectivity;
        let diameter_text = c
            .diameter_hops
            .map(|d| d.to_string())
            .unwrap_or_else(|| "—".to_string());
        ui.label(
            RichText::new(format!(
                "Components: {} · Largest: {} · Diameter: {}",
                c.component_count, c.largest_component_size, diameter_text
            ))
            .color(palette::chrome_text_dim()),
        );
        if !c.articulation_point_ids.is_empty() {
            ui.label(
                RichText::new(format!(
                    "Articulation points: {}",
                    c.articulation_point_ids.join(", ")
                ))
                .color(Color32::from_rgb(235, 200, 90)),
            );
        }
        if !c.isolated_system_ids.is_empty() {
            ui.label(
                RichText::new(format!("Isolated: {}", c.isolated_system_ids.join(", ")))
                    .color(palette::chrome_text_dim()),
            );
        }
    });

    crate::ui_kit::section(ui, "DISTRIBUTIONS", |ui| {
        dist_block(ui, "World types", &a.world_type_distribution);
        dist_block(ui, "Star colours", &a.star_colour_distribution);
        dist_block(ui, "Populations", &a.population_distribution);
        dist_block(ui, "Route types", &a.route_type_distribution);
        dist_block(ui, "Route stability", &a.route_stability_distribution);
    });

    if !a.subsector_variety.is_empty() {
        crate::ui_kit::section(ui, "SUBSECTOR VARIETY", |ui| {
            for v in &a.subsector_variety {
                ui.label(
                    RichText::new(format!(
                        "{} {} — {} unique dominants · {} contested",
                        v.label, v.name, v.unique_dominants, v.contested_count
                    ))
                    .color(palette::chrome_text_dim()),
                );
            }
        });
    }

    crate::ui_kit::section(ui, "HEALTH FLAGS", |ui| {
        if a.health_flags.is_empty() {
            ui.label(RichText::new("(none)").color(palette::chrome_text_dim()));
        } else {
            for f in &a.health_flags {
                let color = match f.severity {
                    FlagSeverity::Error => Color32::from_rgb(235, 90, 90),
                    FlagSeverity::Warning => Color32::from_rgb(240, 200, 90),
                    FlagSeverity::Info => palette::chrome_text_dim(),
                    _ => palette::chrome_text_dim(),
                };
                let tag = match f.severity {
                    FlagSeverity::Error => "ERROR",
                    FlagSeverity::Warning => "WARN",
                    FlagSeverity::Info => "INFO",
                    _ => "UNKNOWN",
                };
                ui.label(RichText::new(format!("[{tag}] {} — {}", f.code, f.message)).color(color));
            }
        }
    });
}

fn share_bar(
    ui: &mut Ui,
    factions: &[sectorforge::sector_model::GeneratedFaction],
    name: &str,
    faction_id: &str,
    share: f32,
    _i: usize,
) {
    let style = faction_style_by_id(factions, faction_id);
    let color = style.fill;
    let bar_w = 220.0_f32;
    let bar_h = 14.0_f32;
    ui.horizontal(|ui| {
        palette::draw_fraction_bar(
            ui,
            egui::vec2(bar_w, bar_h),
            share,
            color,
            Color32::from_gray(35),
            Stroke::new(1.0, Color32::from_gray(70)),
            1.0,
        );
        ui.label(
            RichText::new(format!("{:>5.1}%  {}", share * 100.0, name))
                .color(palette::chrome_text()),
        );
    });
}

fn dist_block(
    ui: &mut Ui,
    title: &str,
    map: &std::collections::BTreeMap<std::sync::Arc<str>, u32>,
) {
    if map.is_empty() {
        return;
    }
    let total: u32 = map.values().sum();
    ui.label(RichText::new(format!("{title} — {total} total")).color(palette::chrome_text_dim()));
    for (k, v) in map {
        let pct = if total == 0 {
            0.0
        } else {
            (*v as f32) * 100.0 / total as f32
        };
        ui.label(
            RichText::new(format!("  {k:<24} {v:>4}  ({pct:>4.1}%)")).color(palette::chrome_text()),
        );
    }
}
