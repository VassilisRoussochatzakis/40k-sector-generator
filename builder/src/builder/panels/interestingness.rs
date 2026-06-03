//! INTERESTINGNESS tab — Phase D §INT1..§INT4.
//!
//! §INT1  Profile picker: a `ComboBox` over the five built-in
//!        [`ProfileId`] variants (`PoliticalSandbox`, `GrimCollapse`,
//!        `Mercantile`, `Villainous`, `Frontier`). Switching the profile
//!        clears the cached scorecard so the next "Score sector" pass
//!        rebuilds against the new target bands.
//! §INT2  "Score sector" runs [`sectorforge::derive_interestingness_with`]
//!        with the live profile plus any §INT4 per-profile overrides.
//!        Shows the overall `0..=100` score plus strengths / weaknesses
//!        line items pulled straight from the cached report.
//! §INT3  Per-metric bar chart: one row per [`MetricScore`] in the cached
//!        report. Each row paints a horizontal scale `[0..vmax]`, shades
//!        the `target_low..=target_high` band in green, marks the observed
//!        value with a black tick, and prints the fit percentage on the
//!        right. `vmax` is `max(target_high * 1.5, observed * 1.2, 1.0)`
//!        so the band always has room to breathe.
//! §INT4  Custom-profile editor: per-active-profile threshold overrides
//!        live in [`BuilderState::interestingness_custom_overrides`],
//!        keyed by the snake-case profile id. The "Add override" row picks
//!        a metric name (drawn from a fixed catalog covering every metric
//!        emitted by `interestingness::observed_metrics`) and seeds it
//!        from the profile's built-in band; subsequent rows expose
//!        `DragValue` editors for `low / high / floor / ceil / weight`
//!        plus a "Remove" button that drops the override back to the
//!        built-in band. Overrides survive switching to another profile
//!        and back.
//!
//! The panel never edits the cached `interestingness_report` directly.
//! All mutations flow through the panel's local fields on
//! [`BuilderState`]; the "Score sector" button rebuilds the cached
//! report from scratch every press, so the chart always matches the
//! controls the user just dialled in.

use std::collections::BTreeMap;

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use sectorforge::interestingness::{
    InterestingnessConfig, InterestingnessReport, MetricScore, MetricTarget, ProfileId,
};
use sectorforge_gui_core::palette::{paint_rect_filled, paint_rect_stroke};
use sectorforge_gui_core::ui_kit;

use crate::builder::{BuilderState, DerivationKind};

const PROFILE_VARIANTS: &[ProfileId] = &[
    ProfileId::PoliticalSandbox,
    ProfileId::GrimCollapse,
    ProfileId::Mercantile,
    ProfileId::Villainous,
    ProfileId::Frontier,
];

/// Full catalog of metric names the `observed_metrics` table publishes.
/// Used by the §INT4 "Add override" picker so every overridable metric is
/// reachable even when the active profile does not seed it by default.
const METRIC_CATALOG: &[&str] = &[
    "faction_gini",
    "contested_world_ratio",
    "avg_claims_per_world",
    "route_components",
    "articulation_points",
    "isolated_systems",
    "route_diameter",
    "world_type_diversity",
    "warzone_count",
    "blockaded_count",
    "infiltrated_count",
    "asymmetric_control_worlds",
    "faction_count",
];

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    // §39 LD4: re-score when a prior mutation left the cached report stale.
    // The profile/override config builder is panel-local, so this overlay
    // self-heals here. No score yet ⇒ stay cold until the user scores once.
    if state.derivations.is_stale(DerivationKind::Interestingness)
        && state.interestingness_report.is_some()
    {
        rescore(state);
    }
    ui.heading("Interestingness");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "profile picker, score, per-metric chart, per-profile threshold overrides.",
    );
    ui.separator();

    egui::ScrollArea::vertical()
        .id_salt("int_root_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            // §COLUMNS — RC-2: the §INT1 profile / §INT2 score / §INT4 override
            // editor flow down the left column; the §INT3 per-metric band chart
            // (which wants horizontal room for its bars) fills the right column.
            // Collapses to one stacked column on a narrow window.
            ui_kit::columns_responsive(ui, 2, 360.0, |cols| {
                let n = cols.len();
                {
                    let left = &mut cols[0];
                    show_profile_row(left, state);
                    left.separator();
                    show_score_row(left, state);
                    left.separator();
                    show_custom_editor(left, state);
                }
                {
                    // Right column, or the same single column when collapsed.
                    let right = &mut cols[if n > 1 { 1 } else { 0 }];
                    if n == 1 {
                        right.separator();
                    }
                    show_metrics_chart(right, state);
                }
            });
        });
}

// ── §INT1 profile picker ──────────────────────────────────────────────────

fn show_profile_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("profile").strong());
        let prev = state.interestingness_profile;
        ui_kit::combo("int1_profile", profile_label(state.interestingness_profile)).show_ui(
            ui,
            |ui| {
                for p in PROFILE_VARIANTS {
                    ui.selectable_value(&mut state.interestingness_profile, *p, profile_label(*p));
                }
            },
        );
        if state.interestingness_profile != prev {
            state.interestingness_report = None;
            state.interestingness_custom_pick.clear();
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            profile_blurb(state.interestingness_profile),
        );
    });
}

// ── §INT2 score sector ────────────────────────────────────────────────────

fn show_score_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Score sector").clicked() {
            rescore(state);
        }
        match state.interestingness_report.as_ref() {
            Some(r) => {
                ui.label(
                    RichText::new(format!("Overall: {} / 100", r.overall))
                        .strong()
                        .color(overall_color(r.overall)),
                );
            }
            None => {
                ui.colored_label(Color32::GRAY, "no scorecard yet — click \"Score sector\"");
            }
        }
    });

    let Some(report) = state.interestingness_report.as_ref() else {
        return;
    };

    if !report.strengths.is_empty() {
        ui.label(RichText::new("Strengths").strong());
        for s in &report.strengths {
            ui.colored_label(Color32::from_rgb(80, 180, 110), format!("• {s}"));
        }
    }
    if !report.weaknesses.is_empty() {
        ui.label(RichText::new("Weaknesses").strong());
        for w in &report.weaknesses {
            ui.colored_label(Color32::from_rgb(220, 120, 110), format!("• {w}"));
        }
    }
}

fn rescore(state: &mut BuilderState) {
    let cfg = build_config(state);
    let report = sectorforge::derive_interestingness_with(&state.sector, &cfg);
    state.interestingness_report = Some(report);
    state.mark_derivation_fresh(DerivationKind::Interestingness);
}

fn build_config(state: &BuilderState) -> InterestingnessConfig {
    let mut cfg = InterestingnessConfig {
        profile: state.interestingness_profile,
        custom: BTreeMap::new(),
    };
    if let Some(overrides) = state
        .interestingness_custom_overrides
        .get(profile_key(state.interestingness_profile))
    {
        cfg.custom = overrides.clone();
    }
    cfg
}

// ── §INT3 per-metric bar chart ────────────────────────────────────────────

fn show_metrics_chart(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("metric bands").strong());
    let Some(report) = state.interestingness_report.as_ref() else {
        ui.colored_label(
            Color32::GRAY,
            "Score the sector to see per-metric bands and observed values.",
        );
        return;
    };
    if report.metric_scores.is_empty() {
        ui.colored_label(Color32::GRAY, "No metrics in the active profile.");
        return;
    }
    for score in &report.metric_scores {
        draw_metric_row(ui, score);
    }
}

fn draw_metric_row(ui: &mut Ui, score: &MetricScore) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [200.0, 18.0],
            egui::Label::new(RichText::new(&score.name).monospace()).wrap(),
        );
        let bar_size = Vec2::new(320.0, 18.0);
        let (rect, _) = ui.allocate_exact_size(bar_size, Sense::hover());

        let vmax = (score.target_high * 1.5)
            .max(score.observed * 1.2)
            .max(1.0_f32);
        paint_rect_filled(ui, rect, rect, 2.0, Color32::from_gray(45));

        let band_left = lerp_x(rect, score.target_low / vmax);
        let band_right = lerp_x(rect, score.target_high / vmax);
        let band_rect = Rect::from_min_max(
            Pos2::new(band_left.min(band_right), rect.min.y),
            Pos2::new(band_left.max(band_right), rect.max.y),
        );
        paint_rect_filled(ui, rect, band_rect, 2.0, Color32::from_rgb(60, 130, 75));

        let obs_x = lerp_x(rect, (score.observed / vmax).clamp(0.0, 1.0));
        let tick_rect = Rect::from_min_max(
            Pos2::new(obs_x - 1.0, rect.min.y),
            Pos2::new(obs_x + 1.0, rect.max.y),
        );
        paint_rect_filled(ui, rect, tick_rect, 0.0, Color32::WHITE);

        paint_rect_stroke(
            ui,
            rect,
            rect,
            2.0,
            Stroke::new(1.0, Color32::from_gray(120)),
        );

        ui.label(
            RichText::new(format!(
                "obs {:.2} · band {:.2}..={:.2} · fit {:>3.0}% · w {:.1}",
                score.observed,
                score.target_low,
                score.target_high,
                score.fit * 100.0,
                score.weight
            ))
            .monospace()
            .color(fit_color(score.fit)),
        );
    });
}

fn lerp_x(rect: Rect, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    rect.min.x + (rect.max.x - rect.min.x) * t
}

// ── §INT4 custom profile editor ───────────────────────────────────────────

fn show_custom_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("threshold overrides for this profile").strong());
    ui.colored_label(
        Color32::DARK_GRAY,
        "Overrides survive switching profiles — each profile keeps its own table.",
    );

    show_add_override_row(ui, state);

    let key = profile_key(state.interestingness_profile).to_string();
    let entries: Vec<(String, MetricTarget)> = state
        .interestingness_custom_overrides
        .get(&key)
        .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
        .unwrap_or_default();

    if entries.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "no overrides yet — use the picker above to seed the profile's first override.",
        );
        return;
    }

    let mut removed: Option<String> = None;
    let mut updates: Vec<(String, MetricTarget)> = Vec::new();
    for (name, target) in &entries {
        let mut current = *target;
        ui.horizontal(|ui| {
            ui.add_sized(
                [200.0, 18.0],
                egui::Label::new(RichText::new(name).monospace()).wrap(),
            );
            ui.label("low");
            ui.add(egui::DragValue::new(&mut current.low).speed(0.05));
            ui.label("high");
            ui.add(egui::DragValue::new(&mut current.high).speed(0.05));
            ui.label("floor");
            ui.add(egui::DragValue::new(&mut current.floor).speed(0.05));
            ui.label("ceil");
            if current.ceil.is_infinite() {
                if ui.button("set finite ceil").clicked() {
                    current.ceil = (current.high * 2.0).max(current.high + 1.0);
                }
            } else {
                ui.add(egui::DragValue::new(&mut current.ceil).speed(0.05));
                if ui.button("∞").clicked() {
                    current.ceil = f32::INFINITY;
                }
            }
            ui.label("weight");
            ui.add(
                egui::DragValue::new(&mut current.weight)
                    .speed(0.05)
                    .range(0.0..=10.0),
            );
            if ui.button("Remove").clicked() {
                removed = Some(name.clone());
            }
        });
        if current.low != target.low
            || current.high != target.high
            || current.floor != target.floor
            || current.ceil != target.ceil
            || current.weight != target.weight
        {
            updates.push((name.clone(), current));
        }
    }

    let mut mutated = false;
    if !updates.is_empty() {
        let map = state
            .interestingness_custom_overrides
            .entry(key.clone())
            .or_default();
        for (n, t) in updates {
            map.insert(n, t);
        }
        mutated = true;
    }
    if let Some(name) = removed {
        if let Some(map) = state.interestingness_custom_overrides.get_mut(&key) {
            map.remove(&name);
            if map.is_empty() {
                state.interestingness_custom_overrides.remove(&key);
            }
        }
        mutated = true;
    }
    if mutated {
        state.interestingness_report = None;
    }
}

fn show_add_override_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        ui.label("Add override:");
        let key = profile_key(state.interestingness_profile).to_string();
        let already: Vec<&'static str> = METRIC_CATALOG
            .iter()
            .copied()
            .filter(|m| {
                state
                    .interestingness_custom_overrides
                    .get(&key)
                    .map(|map| map.contains_key(*m))
                    .unwrap_or(false)
            })
            .collect();
        let label = if state.interestingness_custom_pick.is_empty() {
            "(pick a metric)".to_string()
        } else {
            state.interestingness_custom_pick.clone()
        };
        ui_kit::combo("int4_metric_pick", label).show_ui(ui, |ui| {
            for m in METRIC_CATALOG {
                if already.contains(m) {
                    continue;
                }
                ui.selectable_value(&mut state.interestingness_custom_pick, (*m).to_string(), *m);
            }
        });
        let pick = state.interestingness_custom_pick.clone();
        let can_add = !pick.is_empty() && !already.iter().any(|m| *m == pick);
        if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
            let seed = seed_target(state.interestingness_profile, pick.as_str());
            state
                .interestingness_custom_overrides
                .entry(key)
                .or_default()
                .insert(pick.clone(), seed);
            state.interestingness_custom_pick.clear();
            state.interestingness_report = None;
        }
    });
}

/// Look up the profile's built-in band for `metric`, falling back to a
/// neutral `[0, 1]` target so the user can dial in something for metrics
/// the active profile does not seed by default.
fn seed_target(profile: ProfileId, metric: &str) -> MetricTarget {
    // `profile_targets` is private to the library, so we reconstruct the
    // band by asking the library for a one-metric custom config and
    // letting the scorer hand us the resolved target through the
    // `MetricScore`. Skipping the round-trip would just duplicate the
    // built-in table — the library is the single source of truth.
    let cfg = InterestingnessConfig {
        profile,
        custom: BTreeMap::new(),
    };
    let empty = empty_sector();
    let report = sectorforge::derive_interestingness_with(&empty, &cfg);
    if let Some(found) = report.metric_scores.iter().find(|s| s.name == metric) {
        return MetricTarget {
            low: found.target_low,
            high: found.target_high,
            floor: 0.0,
            ceil: (found.target_high * 2.0).max(found.target_high + 1.0),
            weight: found.weight,
        };
    }
    MetricTarget {
        low: 0.0,
        high: 1.0,
        floor: 0.0,
        ceil: 2.0,
        weight: 1.0,
    }
}

fn empty_sector() -> sectorforge::sector_model::GeneratedSector {
    sectorforge::sector_model::GeneratedSector::empty("int-seed", "Int Seed", "0", 1, 1)
}

// ── label / colour helpers ────────────────────────────────────────────────

fn profile_label(p: ProfileId) -> &'static str {
    match p {
        ProfileId::PoliticalSandbox => "Political sandbox",
        ProfileId::GrimCollapse => "Grim collapse",
        ProfileId::Mercantile => "Mercantile",
        ProfileId::Villainous => "Villainous",
        ProfileId::Frontier => "Frontier",
        _ => "Unknown",
    }
}

fn profile_blurb(p: ProfileId) -> &'static str {
    match p {
        ProfileId::PoliticalSandbox => {
            "Balanced multi-faction sandbox with several contested worlds."
        }
        ProfileId::GrimCollapse => "Bleak collapse: high contested ratio, fragmented routes.",
        ProfileId::Mercantile => "Trade-heavy: high connectivity, low warzone count.",
        ProfileId::Villainous => "One dominant villain: extreme Gini, low contested ratio.",
        ProfileId::Frontier => "Sparse pioneers: isolated systems, few factions.",
        _ => "",
    }
}

fn profile_key(p: ProfileId) -> &'static str {
    match p {
        ProfileId::PoliticalSandbox => "political_sandbox",
        ProfileId::GrimCollapse => "grim_collapse",
        ProfileId::Mercantile => "mercantile",
        ProfileId::Villainous => "villainous",
        ProfileId::Frontier => "frontier",
        _ => "unknown",
    }
}

fn overall_color(overall: u8) -> Color32 {
    if overall >= 75 {
        Color32::from_rgb(120, 200, 130)
    } else if overall >= 50 {
        Color32::from_rgb(220, 200, 110)
    } else {
        Color32::from_rgb(220, 120, 110)
    }
}

fn fit_color(fit: f32) -> Color32 {
    if fit >= 0.9 {
        Color32::from_rgb(120, 200, 130)
    } else if fit >= 0.5 {
        Color32::from_rgb(220, 200, 110)
    } else {
        Color32::from_rgb(220, 120, 110)
    }
}

// silence unused warning for the report alias when feature-gated tests are off
#[allow(dead_code)]
fn _force_report_use(_: &InterestingnessReport) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_state() -> BuilderState {
        BuilderState::new_blank("t", "T", "s", 4, 4)
    }

    #[test]
    fn defaults_seed_political_sandbox_with_no_report() {
        let state = seeded_state();
        assert_eq!(state.interestingness_profile, ProfileId::PoliticalSandbox);
        assert!(state.interestingness_report.is_none());
        assert!(state.interestingness_custom_overrides.is_empty());
        assert!(state.interestingness_custom_pick.is_empty());
    }

    #[test]
    fn rescore_populates_report() {
        let mut state = seeded_state();
        rescore(&mut state);
        assert!(state.interestingness_report.is_some());
        let r = state.interestingness_report.as_ref().unwrap();
        assert_eq!(r.profile, "political_sandbox");
    }

    #[test]
    fn build_config_layers_per_profile_overrides() {
        let mut state = seeded_state();
        let key = profile_key(state.interestingness_profile).to_string();
        let mut over = BTreeMap::new();
        over.insert(
            "faction_gini".to_string(),
            MetricTarget {
                low: 0.10,
                high: 0.20,
                floor: 0.0,
                ceil: 1.0,
                weight: 2.0,
            },
        );
        state.interestingness_custom_overrides.insert(key, over);

        let cfg = build_config(&state);
        let band = cfg.custom.get("faction_gini").expect("override missing");
        assert!((band.low - 0.10).abs() < 1e-6);
        assert!((band.high - 0.20).abs() < 1e-6);
        assert!((band.weight - 2.0).abs() < 1e-6);
    }

    #[test]
    fn switching_profile_clears_cached_report_but_keeps_overrides() {
        let mut state = seeded_state();
        rescore(&mut state);
        assert!(state.interestingness_report.is_some());

        let key = profile_key(state.interestingness_profile).to_string();
        state
            .interestingness_custom_overrides
            .entry(key)
            .or_default()
            .insert(
                "warzone_count".to_string(),
                MetricTarget {
                    low: 0.0,
                    high: 1.0,
                    floor: 0.0,
                    ceil: 2.0,
                    weight: 1.0,
                },
            );

        // Simulate the picker switching profiles + clearing the cached
        // report (matches the §INT1 row body).
        state.interestingness_profile = ProfileId::Frontier;
        state.interestingness_report = None;

        // Frontier has its own override slot (empty) but the prior
        // political_sandbox table is untouched.
        let political_overrides = state
            .interestingness_custom_overrides
            .get("political_sandbox")
            .expect("political_sandbox overrides dropped");
        assert!(political_overrides.contains_key("warzone_count"));
    }

    #[test]
    fn seed_target_uses_profile_band() {
        let target = seed_target(ProfileId::PoliticalSandbox, "faction_gini");
        // PoliticalSandbox seeds faction_gini at 0.30..=0.55 / weight 1.0.
        assert!((target.low - 0.30).abs() < 1e-4);
        assert!((target.high - 0.55).abs() < 1e-4);
        assert!((target.weight - 1.0).abs() < 1e-4);
    }
}
