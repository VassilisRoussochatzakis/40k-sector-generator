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
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;

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
        RichText::new("Score the live sector: faction balance, connectivity, distributions, and health flags.")
            .color(Color32::DARK_GRAY),
    );
    ui.add_space(4.0);
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
    ui_kit::collapsing_section(ui, "an_config", "Warning thresholds (§A2)", true, |ui| {
        let mut changed = false;
        let cfg = &mut state.analytics.config;

        labeled(
            ui,
            "Max faction share",
            "Flag any faction whose share of total projection rises above this fraction (schema: warn_faction_share). 0.5 = half the sector.",
            |ui| {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut cfg.warn_faction_share)
                            .speed(0.01)
                            .range(0.0..=1.0),
                    )
                    .changed();
            },
        );
        labeled(
            ui,
            "Max contested ratio",
            "Flag when the share of worlds claimed by more than one faction rises above this (schema: warn_contested_ratio).",
            |ui| {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut cfg.warn_contested_ratio)
                            .speed(0.01)
                            .range(0.0..=1.0),
                    )
                    .changed();
            },
        );
        labeled(
            ui,
            "Tiny-sector size",
            "Sectors with fewer systems than this mark their connectivity metrics as low-confidence (schema: tiny_sector_threshold).",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.tiny_sector_threshold).range(0..=10_000))
                    .changed();
            },
        );
        labeled(
            ui,
            "Flag if disconnected",
            "Raise a flag when the route graph splits into more than one piece (schema: warn_if_disconnected).",
            |ui| {
                changed |= ui.checkbox(&mut cfg.warn_if_disconnected, "").changed();
            },
        );
        labeled(
            ui,
            "Flag choke points",
            "Raise a flag when removing a single system would split the route graph (schema: warn_if_articulation).",
            |ui| {
                changed |= ui.checkbox(&mut cfg.warn_if_articulation, "").changed();
            },
        );

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button("📥  Load from project")
                .on_hover_text("Copy the thresholds saved in the open project into this editor.")
                .clicked()
            {
                let project = state.config.analyze.clone();
                state.analytics.seed_from_project(&project);
            }
            if ui
                .button("↺  Reset to defaults")
                .on_hover_text("Restore the standard thresholds and clear the current report.")
                .clicked()
            {
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
        if ui
            .button("📊  Analyze")
            .on_hover_text("Score the live sector with the thresholds above.")
            .clicked()
        {
            recompute(state);
        }
        // Strict toggle — display-only failure gate.
        ui.checkbox(&mut state.analytics.strict, "Strict")
            .on_hover_text("Treat every health flag as a failure, so even warnings show as errors.");

        ui.separator();

        if ui
            .button("📂  Choose folder…")
            .on_hover_text("Pick where to save the exported report.")
            .clicked()
        {
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
            .add_enabled(has_report && has_dir, egui::Button::new("💾  Export report"))
            .on_hover_text(if !has_report {
                "Run Analyze first."
            } else if !has_dir {
                "Choose an export folder first."
            } else {
                "Save the report as analysis.md and analysis.json in the chosen folder."
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
        .unwrap_or_else(|| "No export folder chosen yet.".to_string());
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
        ui_kit::placeholder(
            ui,
            "No report yet — click Analyze to score the live sector.",
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
            "⚠ Few systems — connectivity numbers below may not mean much yet.",
        );
    }

    // Strict banner — a count of flags currently treated as failures.
    if failing > 0 {
        let detail = if strict {
            "(every flag counts while Strict is on)"
        } else {
            "(error-level)"
        };
        ui.colored_label(
            Color32::from_rgb(220, 90, 90),
            format!("✖ {failing} failing health flag(s) {detail}."),
        );
    } else if !a.health_flags.is_empty() {
        ui.colored_label(
            Color32::from_rgb(120, 180, 120),
            "✔ No failing flags — only warnings and notes.",
        );
    } else {
        ui.colored_label(Color32::from_rgb(120, 180, 120), "✔ No health flags.");
    }

    ui.add_space(4.0);
    // §COLUMNS — flow the metric cards across a responsive 3-column board so a
    // wide window reads as a dashboard instead of a 6-row vertical stack; the
    // closure handles `cols.len() == 1` (narrow window) via the `% n` round-robin.
    ui_kit::columns_responsive(ui, 3, 280.0, |cols| {
        let n = cols.len();
        let mut next = 0usize;
        macro_rules! col {
            () => {{
                let c = &mut cols[next % n];
                next += 1;
                c
            }};
        }
        show_faction_balance(col!(), a);
        show_world_stats(col!(), a);
        show_distributions(col!(), a);
        show_connectivity(col!(), a);
        show_subsector_variety(col!(), a);
        show_health_flags(col!(), a, strict);
        let _ = next; // final col!() bump is intentionally unread
    });
}

fn show_faction_balance(ui: &mut Ui, a: &SectorAnalysis) {
    ui_kit::collapsing_section(
        ui,
        "an_faction_balance",
        &format!("Faction balance — Gini {:.3}", a.faction_balance.gini),
        true,
        |ui| {
            if a.faction_balance.top_factions.is_empty() {
                ui_kit::placeholder(ui, "No factions with map presence yet.");
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
        },
    );
}

fn show_world_stats(ui: &mut Ui, a: &SectorAnalysis) {
    ui_kit::collapsing_section(ui, "an_world_stats", "World & claim stats", false, |ui| {
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
    ui_kit::collapsing_section(ui, "an_distributions", "Distributions", false, |ui| {
        dist_block(ui, "World types", &a.world_type_distribution);
        dist_block(ui, "Star colours", &a.star_colour_distribution);
        dist_block(ui, "Populations", &a.population_distribution);
        dist_block(ui, "Route types", &a.route_type_distribution);
        dist_block(ui, "Route stability", &a.route_stability_distribution);
    });
}

fn show_connectivity(ui: &mut Ui, a: &SectorAnalysis) {
    let c = &a.connectivity;
    ui_kit::collapsing_section(
        ui,
        "an_connectivity",
        &format!(
            "Route-graph connectivity — {} component(s)",
            c.component_count
        ),
        false,
        |ui| {
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
        },
    );
}

fn show_subsector_variety(ui: &mut Ui, a: &SectorAnalysis) {
    if a.subsector_variety.is_empty() {
        return;
    }
    ui_kit::collapsing_section(
        ui,
        "an_subsector_variety",
        "Subsector political variety",
        false,
        |ui| {
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
        },
    );
}

fn show_health_flags(ui: &mut Ui, a: &SectorAnalysis, strict: bool) {
    ui_kit::collapsing_section(
        ui,
        "an_health_flags",
        &format!("Health flags ({})", a.health_flags.len()),
        true,
        |ui| {
            if a.health_flags.is_empty() {
                ui.colored_label(
                    Color32::from_rgb(120, 180, 120),
                    "No health flags — the sector looks clean.",
                );
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
        },
    );
}

// ── helpers ─────────────────────────────────────────────────────────────────

/// Aligned label-left / control-right row with a hover tooltip. The visible
/// label reads in human terms ("Max faction share") while the tooltip names the
/// underlying `[analyze]` field plus a plain-language note, so power users keep
/// the schema mapping. Friendlier replacement for the old bare `egui::Grid`
/// whose row labels *were* the raw schema names.
fn labeled(ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let h = ui.spacing().interact_size.y;
        ui.add_sized(
            [140.0, h],
            egui::Label::new(RichText::new(label).color(palette::chrome_text_dim())),
        )
        .on_hover_text(help);
        add(ui);
    });
}

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
