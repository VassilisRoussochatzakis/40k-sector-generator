//! Status-bar panel (§N4). First concrete instance of the R10 panel contract:
//! a free function taking `&mut egui::Ui` and `&mut BuilderState`, with no
//! module-level mutable state.

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
        match &state.invariant_report {
            Some(r) if r.ok => ui.colored_label(egui::Color32::GREEN, "✓ invariants"),
            Some(r) => ui.colored_label(
                egui::Color32::RED,
                format!("✗ {} invariant violation(s)", r.violations.len()),
            ),
            None => ui.colored_label(egui::Color32::GRAY, "invariants: not run"),
        };
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
    });
}
