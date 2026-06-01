//! ANALYTICS tab (§N1 / §N2). Phase E §A1..§A4 — the read-only analytics
//! dashboard over the live sector.
//!
//! §A1  Full [`sectorforge::analytics`] parity: faction Gini + per-faction
//!      projection share, contested-world ratio, average claims per world,
//!      claim-kind + dominance counts, world-type / star-colour / population /
//!      route-type / route-stability distributions, route-graph connectivity
//!      (component count, diameter, articulation points, isolated systems),
//!      per-subsector political variety, and the derived health-flag list.
//!      Rendered straight from the cached [`SectorAnalysis`] so the panel
//!      shows exactly what `sectorforge analyze` would emit.
//! §A2  `[analyze]` config editor: `warn_faction_share`,
//!      `warn_if_disconnected`, `warn_if_articulation`, `warn_contested_ratio`,
//!      `tiny_sector_threshold`. Editing any field clears the cached report so
//!      the dashboard never lies about which thresholds fed it. "Load project
//!      `[analyze]`" seeds the editor from the open project's config; "Reset to
//!      defaults" restores [`AnalyzeConfig::default`].
//! §A3  "Strict" toggle — treats every health flag as a failure for CI parity
//!      with `sectorforge analyze --strict` (which exits non-zero on any flag).
//!      The builder has no exit code, so this surfaces as a red banner plus a
//!      failing-flag count.
//! §A4  Export `analysis.md` + `analysis.json` to a chosen folder via
//!      [`sectorforge::write_analysis`] — the same artefacts the CLI writes.
//!
//! The panel never edits the cached report directly: "Analyze" rebuilds it
//! from scratch every press through [`sectorforge::analyze_sector_with`], so
//! the dashboard always matches the config the user just dialled in.

use std::collections::BTreeMap;
use std::sync::Arc;

use camino::Utf8PathBuf;
use egui::{Color32, RichText, Ui};

use sectorforge::analytics::{AnalyzeConfig, FlagSeverity, SectorAnalysis};

use crate::builder::{BuilderState, DerivationKind, ModalKind};

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    // §39 LD4: this overlay owns its `[analyze]` config builder, so it refreshes
    // its own cached report when a prior mutation left it stale (rather than
    // through the central `pump_derivations`). Cold reports stay cold — the user
    // still presses "Analyze" for the first run.
    if state.derivations.is_stale(DerivationKind::Analytics) && state.analytics.report.is_some() {
        recompute(state);
    }
    ui.heading("Analytics");
    ui.label(
        RichText::new("§A1..§A4 — faction balance, connectivity, distributions, health flags.")
            .small()
            .color(Color32::GRAY),
    );
    ui.separator();

    egui::ScrollArea::vertical()
        .id_salt("analytics_root_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_config(ui, state);
            ui.separator();
            show_actions(ui, state);
            ui.separator();
            show_dashboard(ui, state);
        });
}

// ── §A2 config editor ───────────────────────────────────────────────────────

fn show_config(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new(RichText::new("§A2 — [analyze] config").strong())
        .default_open(true)
        .show(ui, |ui| {
            let mut changed = false;
            let cfg = &mut state.analytics.config;
            egui::Grid::new("analytics_cfg_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.label("warn_faction_share")
                        .on_hover_text("Flag any faction whose share of total projection exceeds this.");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut cfg.warn_faction_share)
                                .speed(0.01)
                                .range(0.0..=1.0),
                        )
                        .changed();
                    ui.end_row();

                    ui.label("warn_contested_ratio")
                        .on_hover_text("Flag if the contested-world ratio exceeds this.");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut cfg.warn_contested_ratio)
                                .speed(0.01)
                                .range(0.0..=1.0),
                        )
                        .changed();
                    ui.end_row();

                    ui.label("tiny_sector_threshold").on_hover_text(
                        "Sectors with fewer systems than this mark connectivity metrics low-confidence.",
                    );
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.tiny_sector_threshold).range(0..=10_000))
                        .changed();
                    ui.end_row();

                    ui.label("warn_if_disconnected");
                    changed |= ui.checkbox(&mut cfg.warn_if_disconnected, "").changed();
                    ui.end_row();

                    ui.label("warn_if_articulation");
                    changed |= ui.checkbox(&mut cfg.warn_if_articulation, "").changed();
                    ui.end_row();
                });

            ui.horizontal(|ui| {
                if ui
                    .button("Load project [analyze]")
                    .on_hover_text("Seed the editor from the open project's [analyze] block.")
                    .clicked()
                {
                    let project = state.config.analyze.clone();
                    state.analytics.seed_from_project(&project);
                }
                if ui.button("Reset to defaults").clicked() {
                    state.analytics.config = AnalyzeConfig::default();
                    state.analytics.report = None;
                }
            });

            if changed {
                // A threshold moved — the cached report would no longer match.
                state.analytics.report = None;
            }
        });
}

// ── §A3 / §A4 actions ─────────────────────────────────────────────────────

fn show_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Analyze").clicked() {
            recompute(state);
        }
        // §A3 strict toggle — display-only failure gate.
        ui.checkbox(&mut state.analytics.strict, "Strict")
            .on_hover_text(
                "Treat every health flag as a failure (CI parity with `analyze --strict`).",
            );

        ui.separator();

        if ui.button("Choose export folder…").clicked() {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Choose analysis export folder")
                .pick_folder()
            {
                if let Ok(path) = Utf8PathBuf::from_path_buf(folder) {
                    state.analytics.export_dir = Some(path);
                }
            }
        }
        let has_report = state.analytics.report.is_some();
        let has_dir = state.analytics.export_dir.is_some();
        if ui
            .add_enabled(
                has_report && has_dir,
                egui::Button::new("Export analysis.md + analysis.json (§A4)"),
            )
            .on_hover_text(if has_report {
                "Write the cached analysis to the chosen folder."
            } else {
                "Click Analyze first."
            })
            .clicked()
        {
            export(state);
        }
    });

    let dir_label = state
        .analytics
        .export_dir
        .as_ref()
        .map(camino::Utf8PathBuf::to_string)
        .unwrap_or_else(|| "(no export folder picked)".to_string());
    ui.colored_label(Color32::DARK_GRAY, dir_label);

    if let Some(err) = state.analytics.error.as_ref() {
        ui.colored_label(Color32::from_rgb(220, 120, 110), err);
    }
}

fn recompute(state: &mut BuilderState) {
    let report = sectorforge::analyze_sector_with(&state.sector, &state.analytics.config);
    state.analytics.report = Some(report);
    state.mark_derivation_fresh(DerivationKind::Analytics);
}

fn export(state: &mut BuilderState) {
    let Some(dir) = state.analytics.export_dir.clone() else {
        return;
    };
    let Some(report) = state.analytics.report.clone() else {
        return;
    };
    match sectorforge::write_analysis(dir.as_path(), &report) {
        Ok(()) => {
            state.analytics.error = None;
            state.modal = Some(ModalKind::Message(format!(
                "Wrote analysis.md and analysis.json to {dir}"
            )));
        }
        Err(e) => {
            state.analytics.error = Some(format!("Analysis export failed: {e}"));
        }
    }
}

// ── §A1 dashboard ─────────────────────────────────────────────────────────

fn show_dashboard(ui: &mut Ui, state: &mut BuilderState) {
    let strict = state.analytics.strict;
    let failing = state.analytics.failing_flag_count();
    let Some(a) = state.analytics.report.as_ref() else {
        ui.colored_label(
            Color32::GRAY,
            "No analysis yet — click \"Analyze\" to score the live sector.",
        );
        return;
    };

    // Headline counts.
    ui.label(
        RichText::new(format!(
            "Systems {} · Worlds {} · Routes {} · Factions {}",
            a.system_count, a.world_count, a.route_count, a.faction_count
        ))
        .strong(),
    );
    if a.low_confidence {
        ui.colored_label(
            Color32::from_rgb(210, 170, 90),
            "⚠ Low-confidence sector (small system count); structural metrics may be degenerate.",
        );
    }

    // §A3 strict banner.
    if failing > 0 {
        let (col, word) = if strict {
            (Color32::from_rgb(220, 90, 90), "strict")
        } else {
            (Color32::from_rgb(220, 90, 90), "error")
        };
        ui.colored_label(
            col,
            format!("✖ {failing} {word}-level health flag(s) — sector would FAIL CI."),
        );
    } else if !a.health_flags.is_empty() {
        ui.colored_label(
            Color32::from_rgb(120, 180, 120),
            "✔ no failing flags (warnings/info only).",
        );
    } else {
        ui.colored_label(Color32::from_rgb(120, 180, 120), "✔ no health flags.");
    }

    ui.add_space(4.0);
    show_faction_balance(ui, a);
    show_world_stats(ui, a);
    show_distributions(ui, a);
    show_connectivity(ui, a);
    show_subsector_variety(ui, a);
    show_health_flags(ui, a, strict);
}

fn show_faction_balance(ui: &mut Ui, a: &SectorAnalysis) {
    egui::CollapsingHeader::new(format!(
        "Faction balance — Gini {:.3}",
        a.faction_balance.gini
    ))
    .default_open(true)
    .show(ui, |ui| {
        if a.faction_balance.top_factions.is_empty() {
            ui.colored_label(Color32::GRAY, "no factions");
            return;
        }
        egui::Grid::new("analytics_faction_grid")
            .num_columns(6)
            .striped(true)
            .spacing([12.0, 2.0])
            .show(ui, |ui| {
                for h in [
                    "Faction",
                    "Kind",
                    "Share",
                    "Projection",
                    "Worlds",
                    "Systems",
                ] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for f in a.faction_balance.top_factions.iter().take(20) {
                    ui.label(format!("{} ({})", f.name, f.faction_id));
                    ui.label(f.kind.as_ref());
                    ui.label(format!("{:.1}%", f.share * 100.0));
                    ui.label(format!("{:.1}", f.total_projection));
                    ui.label(f.world_presence_count.to_string());
                    ui.label(f.system_presence_count.to_string());
                    ui.end_row();
                }
            });
        if a.faction_balance.top_factions.len() > 20 {
            ui.colored_label(
                Color32::DARK_GRAY,
                format!(
                    "({} more — full list in analysis.json)",
                    a.faction_balance.top_factions.len() - 20
                ),
            );
        }
    });
}

fn show_world_stats(ui: &mut Ui, a: &SectorAnalysis) {
    egui::CollapsingHeader::new("World & claim stats")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(format!(
                "Contested world ratio: {:.1}%",
                a.contested_world_ratio * 100.0
            ));
            ui.label(format!(
                "Avg claims per world: {:.2}",
                a.avg_claims_per_world
            ));
            count_block(ui, "Claim kinds", &a.claim_kind_counts);
            count_block(ui, "Dominance buckets", &a.dominance_counts);
            count_block(ui, "System political states", &a.system_state_counts);
        });
}

fn show_distributions(ui: &mut Ui, a: &SectorAnalysis) {
    egui::CollapsingHeader::new("Distributions")
        .default_open(false)
        .show(ui, |ui| {
            dist_block(ui, "World types", &a.world_type_distribution);
            dist_block(ui, "Star colours", &a.star_colour_distribution);
            dist_block(ui, "Populations", &a.population_distribution);
            dist_block(ui, "Route types", &a.route_type_distribution);
            dist_block(ui, "Route stability", &a.route_stability_distribution);
        });
}

fn show_connectivity(ui: &mut Ui, a: &SectorAnalysis) {
    let c = &a.connectivity;
    egui::CollapsingHeader::new(format!(
        "Route-graph connectivity — {} component(s)",
        c.component_count
    ))
    .default_open(false)
    .show(ui, |ui| {
        ui.label(format!("Largest component: {}", c.largest_component_size));
        ui.label(match c.diameter_hops {
            Some(d) => format!("Diameter: {d} hops"),
            None => "Diameter: — (disconnected)".to_string(),
        });
        if c.articulation_point_ids.is_empty() {
            ui.colored_label(Color32::from_rgb(120, 180, 120), "No articulation points.");
        } else {
            ui.colored_label(
                Color32::from_rgb(210, 170, 90),
                format!(
                    "Articulation points ({}): {}",
                    c.articulation_point_ids.len(),
                    join_ids(&c.articulation_point_ids)
                ),
            );
        }
        if !c.isolated_system_ids.is_empty() {
            ui.colored_label(
                Color32::from_rgb(210, 170, 90),
                format!(
                    "Isolated systems ({}): {}",
                    c.isolated_system_ids.len(),
                    join_ids(&c.isolated_system_ids)
                ),
            );
        }
    });
}

fn show_subsector_variety(ui: &mut Ui, a: &SectorAnalysis) {
    if a.subsector_variety.is_empty() {
        return;
    }
    egui::CollapsingHeader::new("Subsector political variety")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("analytics_subsector_grid")
                .num_columns(3)
                .striped(true)
                .spacing([12.0, 2.0])
                .show(ui, |ui| {
                    for h in ["Subsector", "Unique dominants", "Contested worlds"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for v in &a.subsector_variety {
                        ui.label(format!("{} — {}", v.label, v.name));
                        ui.label(v.unique_dominants.to_string());
                        ui.label(v.contested_count.to_string());
                        ui.end_row();
                    }
                });
        });
}

fn show_health_flags(ui: &mut Ui, a: &SectorAnalysis, strict: bool) {
    egui::CollapsingHeader::new(format!("Health flags ({})", a.health_flags.len()))
        .default_open(true)
        .show(ui, |ui| {
            if a.health_flags.is_empty() {
                ui.colored_label(Color32::from_rgb(120, 180, 120), "(none)");
                return;
            }
            for f in &a.health_flags {
                // Under strict, a non-error flag is promoted to a failure tint.
                let fails = strict || f.severity == FlagSeverity::Error;
                let col = match f.severity {
                    FlagSeverity::Error => Color32::from_rgb(220, 90, 90),
                    FlagSeverity::Warning if fails => Color32::from_rgb(220, 120, 110),
                    FlagSeverity::Warning => Color32::from_rgb(210, 170, 90),
                    FlagSeverity::Info if fails => Color32::from_rgb(200, 140, 110),
                    _ => Color32::GRAY,
                };
                let tag = match f.severity {
                    FlagSeverity::Error => "ERROR",
                    FlagSeverity::Warning => "WARN",
                    _ => "INFO",
                };
                ui.colored_label(col, format!("[{tag}] {} — {}", f.code, f.message));
            }
        });
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn count_block(ui: &mut Ui, title: &str, map: &BTreeMap<Arc<str>, u32>) {
    if map.is_empty() {
        return;
    }
    ui.label(RichText::new(title).strong());
    for (k, v) in map {
        ui.label(format!("  {k}: {v}"));
    }
}

fn dist_block(ui: &mut Ui, title: &str, map: &BTreeMap<Arc<str>, u32>) {
    if map.is_empty() {
        return;
    }
    let total: u32 = map.values().sum();
    ui.label(RichText::new(format!("{title} (total {total})")).strong());
    for (k, v) in map {
        let pct = if total == 0 {
            0.0
        } else {
            f64::from(*v) * 100.0 / f64::from(total)
        };
        ui.label(format!("  {k}: {v} ({pct:.1}%)"));
    }
}

fn join_ids<T: AsRef<str>>(ids: &[T]) -> String {
    ids.iter()
        .map(std::convert::AsRef::as_ref)
        .collect::<Vec<_>>()
        .join(", ")
}
