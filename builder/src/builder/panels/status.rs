//! Status-bar panel (§N4). First concrete instance of the R10 panel contract:
//! a free function taking `&mut egui::Ui` and `&mut BuilderState`, with no
//! module-level mutable state.
//!
//! §V3: the panel surfaces a single tri-coloured health pip combining the
//! pre-generation validation report and the post-generation invariant report
//! (see [`BuilderState::health_level`]). Green = both clean, yellow = warnings
//! or no report yet, red = at least one error or violation.
//!
//! §SPRUCE D4 / D7: the bar is telemetry, so every segment is set in **monospace**
//! (counts, cursor, cache, deriv tags align into a scannable strip), and every
//! status colour comes from the theme-aware [`palette`] status set — never a raw
//! amber/red literal — so the warning hue stays distinct from the brand accent.

use egui::RichText;

use sectorforge_gui_core::palette;

use crate::builder::state::HealthLevel;
use crate::builder::BuilderState;

/// Monospace status segment — the whole bar is tabular telemetry (§SPRUCE D4).
fn mono(s: impl Into<String>) -> RichText {
    RichText::new(s.into()).monospace()
}

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        match &state.project_path {
            Some(p) => ui.label(mono(format!("project: {p}"))),
            None => ui.label(mono("project: (unsaved)").color(palette::warning())),
        };
        ui.separator();
        if state.dirty {
            ui.label(mono("● dirty").color(palette::warning()));
        } else {
            ui.label(mono("clean").color(palette::success()));
        }
        ui.separator();
        render_counts(ui, state);
        ui.separator();
        render_health(ui, state);
        ui.separator();
        ui.label(mono(format!(
            "cmd {}/{}",
            state.command_cursor,
            state.command_log.len()
        )));
        ui.separator();
        ui.label(mono(format!(
            "cache: {}",
            state.derivation_cache.entries.len()
        )));
        // §39 LD3: live-derivation freshness — fresh count + stale/deriving tags.
        render_derivations(ui, state);
        if !state.pending_jobs.is_empty() {
            ui.separator();
            ui.spinner();
            ui.label(mono(format!("jobs: {}", state.pending_jobs.len())));
        }
        // §CTX1 Phase 7 — right-click menu telemetry tail. Shows the most
        // recent menu activation as `ctx_menu: <schema> :: <item>` so the
        // status bar gives a single-line trace of what the menu dispatched.
        if let Some(label) = state.last_menu_action.as_ref() {
            ui.separator();
            ui.label(mono(format!("ctx_menu: {label}")));
        }
        if let Some(err) = state.last_save_error.as_ref() {
            ui.separator();
            ui.label(mono(format!("save: {err}")).color(palette::danger()));
        }
        if let Some(err) = state.last_catalog_error.as_ref() {
            ui.separator();
            ui.label(mono(format!("reload: {err}")).color(palette::danger()));
        }
        if let Some(err) = state.last_subsector_error.as_ref() {
            ui.separator();
            ui.label(mono(format!("subsectors: {err}")).color(palette::danger()));
        }
    });
}

/// §COLUMNS §6.3 — sector entity counts, surfaced once in the status bar so
/// individual panels need not each re-render their own summary row.
fn render_counts(ui: &mut egui::Ui, state: &BuilderState) {
    let systems = state.sector.systems.len();
    let worlds: usize = state.sector.systems.iter().map(|s| s.worlds.len()).sum();
    let routes = state.sector.routes.len();
    let factions = state.sector.factions.len();
    ui.label(mono(format!(
        "{systems} sys · {worlds} wld · {routes} rt · {factions} fac"
    )))
    .on_hover_text("Sector entity counts (systems · worlds · routes · factions)");
}

/// §39 LD3 — live-derivation freshness readout. Shows how many tracked
/// overlays hold a cached value (`deriv N`), and tags any that a recent
/// mutation left stale (`stale M`) or that a background recompute is refreshing
/// (`deriving K`). Hovering lists the kinds involved.
fn render_derivations(ui: &mut egui::Ui, state: &BuilderState) {
    let led = &state.derivations;
    let derived = led.fingerprints.len();
    ui.separator();
    ui.label(mono(format!("deriv {derived}")))
        .on_hover_text("Live overlays (§39) with a cached value this session.");
    let stale = led.stale_count();
    if stale > 0 {
        let names: Vec<&str> = led.stale.iter().map(|k| k.key()).collect();
        ui.label(mono(format!("stale {stale}")).color(palette::warning()))
            .on_hover_text(format!("Out of date until viewed: {}", names.join(", ")));
    }
    let deriving = led.deriving.len();
    if deriving > 0 {
        ui.label(mono(format!("deriving {deriving}")).color(palette::info()));
    }
}

fn render_health(ui: &mut egui::Ui, state: &BuilderState) {
    let level = state.health_level();
    // §SPRUCE D7: health colour comes from the theme-aware status set, so the
    // warn tier is an orange clear of the brass accent and the clean tier is a
    // measured green rather than the harsh `Color32::GREEN`/`RED` primaries.
    let (colour, glyph) = match level {
        HealthLevel::Green => (palette::success(), "✓"),
        HealthLevel::Yellow => (palette::warning(), "!"),
        HealthLevel::Red => (palette::danger(), "✗"),
    };
    let v_errors = state
        .validation_report
        .as_ref()
        .map(|r| r.errors.len())
        .unwrap_or(0);
    let v_warnings = state
        .validation_report
        .as_ref()
        .map(|r| r.warnings.len())
        .unwrap_or(0);
    let inv_violations = state
        .invariant_report
        .as_ref()
        .map(|r| r.violations.len())
        .unwrap_or(0);
    ui.label(
        mono(format!(
            "{glyph} validation: {v_errors} err / {v_warnings} warn · invariants: {inv_violations}"
        ))
        .color(colour),
    );
}
