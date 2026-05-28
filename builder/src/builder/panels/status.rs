//! Status-bar panel (§N4). First concrete instance of the R10 panel contract:
//! a free function taking `&mut egui::Ui` and `&mut BuilderState`, with no
//! module-level mutable state.
//!
//! §V3: the panel surfaces a single tri-coloured health pip combining the
//! pre-generation validation report and the post-generation invariant report
//! (see [`BuilderState::health_level`]). Green = both clean, yellow = warnings
//! or no report yet, red = at least one error or violation.

use crate::builder::state::HealthLevel;
use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        match &state.project_path {
            Some(p) => ui.label(format!("project: {p}")),
            None => ui.label("project: (unsaved)"),
        };
        ui.separator();
        ui.label(if state.dirty { "● dirty" } else { "clean" });
        ui.separator();
        render_health(ui, state);
        ui.separator();
        ui.label(format!(
            "cmd {}/{}",
            state.command_cursor,
            state.command_log.len()
        ));
        ui.separator();
        ui.label(format!("cache: {}", state.derivation_cache.entries.len()));
        if !state.pending_jobs.is_empty() {
            ui.separator();
            ui.spinner();
            ui.label(format!("jobs: {}", state.pending_jobs.len()));
        }
        // §CTX1 Phase 7 — right-click menu telemetry tail. Shows the most
        // recent menu activation as `ctx_menu: <schema> :: <item>` so the
        // status bar gives a single-line trace of what the menu dispatched.
        if let Some(label) = state.last_menu_action.as_ref() {
            ui.separator();
            ui.label(format!("ctx_menu: {label}"));
        }
        if let Some(err) = state.last_save_error.as_ref() {
            ui.separator();
            ui.colored_label(egui::Color32::RED, format!("save: {err}"));
        }
        if let Some(err) = state.last_catalog_error.as_ref() {
            ui.separator();
            ui.colored_label(egui::Color32::RED, format!("reload: {err}"));
        }
        if let Some(err) = state.last_subsector_error.as_ref() {
            ui.separator();
            ui.colored_label(egui::Color32::RED, format!("subsectors: {err}"));
        }
    });
}

fn render_health(ui: &mut egui::Ui, state: &BuilderState) {
    let level = state.health_level();
    let (colour, glyph) = match level {
        HealthLevel::Green => (egui::Color32::GREEN, "✓"),
        HealthLevel::Yellow => (egui::Color32::from_rgb(220, 180, 60), "!"),
        HealthLevel::Red => (egui::Color32::RED, "✗"),
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
    ui.colored_label(
        colour,
        format!(
            "{glyph} validation: {v_errors} err / {v_warnings} warn · invariants: {inv_violations}"
        ),
    );
}
