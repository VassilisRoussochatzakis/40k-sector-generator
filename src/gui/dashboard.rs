//! Sector analytics dashboard (§8 NEW.md). Lazy-computes a `SectorAnalysis`
//! from the currently-loaded sector and renders it as a scrollable panel with
//! a faction-share bar chart, a connectivity callout, and the health-flag list.

use egui::{Color32, RichText, Stroke, Ui};

use crate::analytics::{self, AnalyzeConfig, FlagSeverity, SectorAnalysis};
use crate::sector_model::GeneratedSector;

use super::palette::{faction_style_by_id, TEXT, TEXT_DIM};

#[derive(Default)]
pub struct DashboardState {
    analysis: Option<SectorAnalysis>,
    /// Cached sector id so we recompute when the user reloads.
    cached_sector_id: Option<String>,
    /// Cached seed so we also recompute after a re-roll (id stays the same).
    cached_seed: Option<String>,
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
    let Some(a) = state.analysis.clone() else {
        ui.label(
            RichText::new("analysis unavailable")
                .color(TEXT_DIM)
                .monospace(),
        );
        return;
    };

    ui.label(
        RichText::new(format!("DASHBOARD — {}", a.sector_id.to_uppercase()))
            .color(TEXT)
            .monospace()
            .strong(),
    );
    ui.label(
        RichText::new(format!(
            "{} systems · {} worlds · {} routes · {} factions",
            a.system_count, a.world_count, a.route_count, a.faction_count
        ))
        .color(TEXT_DIM)
        .monospace(),
    );
    ui.add_space(8.0);

    if a.low_confidence {
        ui.label(
            RichText::new("⚠ low-confidence sector (few systems)")
                .color(Color32::from_rgb(235, 200, 90))
                .monospace(),
        );
        ui.add_space(6.0);
    }

    // ── Faction balance ─────────────────────────────────────────────────────
    ui.label(
        RichText::new("FACTION BALANCE")
            .color(TEXT)
            .monospace()
            .strong(),
    );
    ui.label(
        RichText::new(format!(
            "Gini {:.3}   (0 = even, 1 = winner-take-all)",
            a.faction_balance.gini
        ))
        .color(TEXT_DIM)
        .monospace(),
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
                .color(TEXT_DIM)
                .monospace(),
        );
    }

    ui.add_space(10.0);
    ui.separator();

    // ── Politics ────────────────────────────────────────────────────────────
    ui.label(
        RichText::new("POLITICAL STATE")
            .color(TEXT)
            .monospace()
            .strong(),
    );
    ui.label(
        RichText::new(format!(
            "Contested worlds: {:.0}% · Avg claims/world: {:.2}",
            a.contested_world_ratio * 100.0,
            a.avg_claims_per_world
        ))
        .color(TEXT_DIM)
        .monospace(),
    );
    if !a.system_state_counts.is_empty() {
        for (k, v) in &a.system_state_counts {
            ui.label(
                RichText::new(format!("  {k}: {v}"))
                    .color(TEXT_DIM)
                    .monospace(),
            );
        }
    }

    ui.add_space(10.0);
    ui.separator();

    // ── Connectivity ────────────────────────────────────────────────────────
    ui.label(
        RichText::new("CONNECTIVITY")
            .color(TEXT)
            .monospace()
            .strong(),
    );
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
        .color(TEXT_DIM)
        .monospace(),
    );
    if !c.articulation_point_ids.is_empty() {
        ui.label(
            RichText::new(format!(
                "Articulation points: {}",
                c.articulation_point_ids.join(", ")
            ))
            .color(Color32::from_rgb(235, 200, 90))
            .monospace(),
        );
    }
    if !c.isolated_system_ids.is_empty() {
        ui.label(
            RichText::new(format!("Isolated: {}", c.isolated_system_ids.join(", ")))
                .color(TEXT_DIM)
                .monospace(),
        );
    }

    ui.add_space(10.0);
    ui.separator();

    // ── Distributions ───────────────────────────────────────────────────────
    ui.label(
        RichText::new("DISTRIBUTIONS")
            .color(TEXT)
            .monospace()
            .strong(),
    );
    dist_block(ui, "World types", &a.world_type_distribution);
    dist_block(ui, "Star colours", &a.star_colour_distribution);
    dist_block(ui, "Populations", &a.population_distribution);
    dist_block(ui, "Route types", &a.route_type_distribution);
    dist_block(ui, "Route stability", &a.route_stability_distribution);

    ui.add_space(10.0);
    ui.separator();

    // ── Subsector variety ───────────────────────────────────────────────────
    if !a.subsector_variety.is_empty() {
        ui.label(
            RichText::new("SUBSECTOR VARIETY")
                .color(TEXT)
                .monospace()
                .strong(),
        );
        for v in &a.subsector_variety {
            ui.label(
                RichText::new(format!(
                    "{} {} — {} unique dominants · {} contested",
                    v.label, v.name, v.unique_dominants, v.contested_count
                ))
                .color(TEXT_DIM)
                .monospace(),
            );
        }
        ui.add_space(10.0);
        ui.separator();
    }

    // ── Flags ───────────────────────────────────────────────────────────────
    ui.label(
        RichText::new("HEALTH FLAGS")
            .color(TEXT)
            .monospace()
            .strong(),
    );
    if a.health_flags.is_empty() {
        ui.label(RichText::new("(none)").color(TEXT_DIM).monospace());
    } else {
        for f in &a.health_flags {
            let color = match f.severity {
                FlagSeverity::Error => Color32::from_rgb(235, 90, 90),
                FlagSeverity::Warning => Color32::from_rgb(240, 200, 90),
                FlagSeverity::Info => TEXT_DIM,
            };
            let tag = match f.severity {
                FlagSeverity::Error => "ERROR",
                FlagSeverity::Warning => "WARN",
                FlagSeverity::Info => "INFO",
            };
            ui.label(
                RichText::new(format!("[{tag}] {} — {}", f.code, f.message))
                    .color(color)
                    .monospace(),
            );
        }
    }
}

fn share_bar(
    ui: &mut Ui,
    factions: &[crate::sector_model::GeneratedFaction],
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
        let (rect, _) = ui.allocate_exact_size(egui::vec2(bar_w, bar_h), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 1.0, Color32::from_gray(35));
        let fill_w = (share.clamp(0.0, 1.0) * bar_w).max(1.0);
        let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, bar_h));
        painter.rect_filled(fill_rect, 1.0, color);
        painter.rect_stroke(rect, 1.0, Stroke::new(1.0, Color32::from_gray(70)));
        ui.label(
            RichText::new(format!("{:>5.1}%  {}", share * 100.0, name))
                .color(TEXT)
                .monospace(),
        );
    });
}

fn dist_block(ui: &mut Ui, title: &str, map: &std::collections::BTreeMap<String, u32>) {
    if map.is_empty() {
        return;
    }
    let total: u32 = map.values().sum();
    ui.label(
        RichText::new(format!("{title} — {total} total"))
            .color(TEXT_DIM)
            .monospace(),
    );
    for (k, v) in map {
        let pct = if total == 0 {
            0.0
        } else {
            (*v as f32) * 100.0 / total as f32
        };
        ui.label(
            RichText::new(format!("  {k:<24} {v:>4}  ({pct:>4.1}%)"))
                .color(TEXT)
                .monospace(),
        );
    }
}
