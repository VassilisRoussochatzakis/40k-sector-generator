//! Validation panel (§V1). Renders the pre-generation
//! [`sectorforge::validation::ValidationReport`] as a file-anchored tree:
//! errors first, warnings under their own collapsing header, and a footer
//! workbook-stats line. Each leaf is a button that updates
//! [`crate::builder::BuilderState::selected_file`] so the §P4 project tree
//! and (Phase E) TOML editor can route the user to the offending row.
//!
//! §COLUMNS — master-detail: the file-grouped error/rule list lives in a
//! persistent left rail (`SidePanel::left("validation_list")`); the selected
//! issue's detail, the focus deep-link, "Re-validate now", and the Strict
//! toggle live in the filling right `CentralPanel`. The header summary stays
//! full-width on top. Which issue is "selected" is pure view state — keyed in
//! `ui.data_mut` temp (no model state, no command bus); the right pane re-finds
//! the matching issue from the report each frame.
//!
//! The panel does not mutate the sector — it only reads
//! [`crate::builder::BuilderState::validation_report`] and writes the file
//! selection. A "Re-validate now" button forces an immediate
//! [`crate::builder::BuilderState::revalidate_now`] flush for users who would
//! rather not wait out the debounce.

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use sectorforge::validation::{Severity, ValidationIssue, ValidationReport};
use sectorforge_gui_core::ui_kit;

use crate::builder::BuilderState;

/// §COLUMNS — view-state key identifying the focused issue. Stored in
/// `ui.data_mut` temp under this id so the right pane can re-find the matching
/// issue from the live report; never persisted, never a model field.
const SELECTED_KEY_ID: &str = "validation_selected_issue";

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Validation");
    ui.separator();

    let Some(report) = state.validation_report.clone() else {
        ui.colored_label(egui::Color32::GRAY, "no validation report yet");
        return;
    };

    // Header summary stays full-width on top.
    render_summary(ui, &report, state.validation_strict);
    ui.separator();

    // §COLUMNS — master-detail. Two separate statements so the first &mut state
    // borrow (the list) ends before the second (the detail).
    egui::SidePanel::left("validation_list")
        .resizable(true)
        .default_width(320.0)
        .width_range(220.0..=520.0)
        .show_inside(ui, |ui| show_issue_list(ui, state, &report));

    egui::CentralPanel::default().show_inside(ui, |ui| show_issue_detail(ui, state, &report));
}

// ── selection key (view state) ──────────────────────────────────────────────

/// Build a stable selection key from an issue's content. `ValidationIssue`
/// has no unique id and does not derive `Hash`/`Eq`, so we key on the fields
/// that together identify a row (code + path + row + message).
fn issue_key(issue: &ValidationIssue) -> String {
    format!(
        "{}|{}|{}|{}",
        issue.code,
        issue.path.as_deref().unwrap_or(""),
        issue.row.map(|r| r.to_string()).unwrap_or_default(),
        issue.message,
    )
}

fn selected_key(ui: &egui::Ui) -> Option<String> {
    ui.data_mut(|d| d.get_temp::<String>(egui::Id::new(SELECTED_KEY_ID)))
}

fn set_selected_key(ui: &egui::Ui, key: String) {
    ui.data_mut(|d| d.insert_temp(egui::Id::new(SELECTED_KEY_ID), key));
}

// ── header summary (full width) ─────────────────────────────────────────────

fn render_summary(ui: &mut egui::Ui, report: &ValidationReport, strict: bool) {
    // §V4: under strict mode, warnings fail the report just like errors.
    let strict_fail = strict && !report.warnings.is_empty();
    ui.horizontal(|ui| {
        if report.ok && !strict_fail {
            ui.colored_label(egui::Color32::GREEN, "✓ ok");
        } else {
            let txt = if strict_fail && report.errors.is_empty() {
                "✗ warnings (strict)"
            } else {
                "✗ errors"
            };
            ui.colored_label(egui::Color32::RED, txt);
        }
        ui.label(format!(
            "{} error(s), {} warning(s)",
            report.errors.len(),
            report.warnings.len()
        ));
        if strict {
            ui.colored_label(
                egui::Color32::from_rgb(220, 180, 60),
                "· strict: warnings count as errors",
            );
        }
    });
}

// ── issue list (left rail) ──────────────────────────────────────────────────

fn show_issue_list(ui: &mut egui::Ui, state: &mut BuilderState, report: &ValidationReport) {
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            if report.errors.is_empty() && report.warnings.is_empty() {
                ui.colored_label(egui::Color32::GREEN, "✓ no validation issues");
            } else {
                render_group(
                    ui,
                    state,
                    "Errors",
                    &report.errors,
                    Severity::Error,
                    egui::Color32::from_rgb(220, 80, 80),
                );
                render_group(
                    ui,
                    state,
                    "Warnings",
                    &report.warnings,
                    Severity::Warning,
                    egui::Color32::from_rgb(220, 180, 60),
                );
            }
        });
}

fn render_group(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    title: &str,
    issues: &[ValidationIssue],
    severity: Severity,
    colour: egui::Color32,
) {
    if issues.is_empty() {
        return;
    }
    let by_file = group_by_file(issues);
    ui_kit::collapsing_section(
        ui,
        ("val_group", title),
        &format!("{title} ({})", issues.len()),
        severity == Severity::Error,
        |ui| {
            for (path, group) in by_file {
                ui_kit::collapsing_section(
                    ui,
                    ("val_file", title, path.as_str()),
                    &format!("{} ({})", path, group.len()),
                    false,
                    |ui| {
                        for issue in &group {
                            issue_row(ui, state, issue, colour);
                        }
                    },
                );
            }
        },
    );
}

fn issue_row(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    issue: &ValidationIssue,
    colour: egui::Color32,
) {
    let key = issue_key(issue);
    let is_selected = selected_key(ui).as_deref() == Some(key.as_str());
    ui.horizontal(|ui| {
        ui.colored_label(colour, &issue.code);
        let label = match (&issue.path, issue.row) {
            (Some(p), Some(r)) => format!("{p} (row {r}): {}", issue.message),
            (Some(p), None) => format!("{p}: {}", issue.message),
            (None, _) => issue.message.clone(),
        };
        // Selecting a row pins the issue into the right detail pane and also
        // routes the §P4 project tree to the offending .toml (existing jump).
        if ui.selectable_label(is_selected, label).clicked() {
            set_selected_key(ui, key.clone());
            jump_to(state, issue);
        }
    });
}

// ── issue detail (right pane) ───────────────────────────────────────────────

fn show_issue_detail(ui: &mut egui::Ui, state: &mut BuilderState, report: &ValidationReport) {
    // Right-pane controls: re-validate + strict toggle live here now.
    ui.horizontal(|ui| {
        if ui.button("Re-validate now").clicked() {
            state.revalidate_now();
        }
        // §V4 — strict toggle: promote warnings to errors for the health pip
        // and the §V6 pre-export gate (parity with `generate --strict`).
        ui.checkbox(&mut state.validation_strict, "Strict")
            .on_hover_text(
                "§V4 — treat validation warnings as errors for the health pip \
                 and the pre-export gate. Mirrors `sectorforge generate --strict`.",
            );
    });
    ui.separator();

    let selected = selected_key(ui);
    let issue = selected.as_deref().and_then(|key| {
        report
            .errors
            .iter()
            .chain(report.warnings.iter())
            .find(|i| issue_key(i) == key)
    });

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            if let Some(issue) = issue {
                render_detail_card(ui, state, issue);
            } else if report.errors.is_empty() && report.warnings.is_empty() {
                ui.colored_label(egui::Color32::GREEN, "✓ no validation issues");
            } else {
                ui_kit::placeholder(ui, "Select an issue from the list on the left.");
            }

            ui.separator();
            render_workbook(ui, report);
        });
}

fn render_detail_card(ui: &mut egui::Ui, state: &mut BuilderState, issue: &ValidationIssue) {
    let (colour, sev_label) = match issue.severity {
        Severity::Error => (egui::Color32::from_rgb(220, 80, 80), "error"),
        Severity::Warning => (egui::Color32::from_rgb(220, 180, 60), "warning"),
        Severity::Info => (egui::Color32::GRAY, "info"),
        _ => (egui::Color32::GRAY, "info"),
    };
    ui_kit::section(ui, &issue.code, |ui| {
        ui_kit::reading_column(ui, 720.0, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(colour, format!("● {sev_label}"));
                ui.monospace(&issue.code);
            });
            ui.add_space(4.0);
            ui.label(&issue.message);
            ui.add_space(6.0);

            if let Some(path) = &issue.path {
                ui_kit::kv(ui, "path", path);
            }
            if let Some(row) = issue.row {
                ui_kit::kv(ui, "row", &row.to_string());
            }

            // Focus deep-link: jump the §P4 project tree / TOML editor to the
            // offending file.
            if let Some(rel) = issue
                .path
                .as_deref()
                .and_then(|a| config_file_for(state, a))
            {
                ui.add_space(8.0);
                if ui
                    .button(format!("Open {rel}"))
                    .on_hover_text("Route the project tree / TOML editor to this file")
                    .clicked()
                {
                    state.selected_file = Some(Utf8PathBuf::from(rel));
                }
            }
        });
    });
}

fn jump_to(state: &mut BuilderState, issue: &ValidationIssue) {
    let Some(anchor) = issue.path.as_deref() else {
        return;
    };
    if let Some(rel) = config_file_for(state, anchor) {
        state.selected_file = Some(Utf8PathBuf::from(rel));
    }
}

/// Map a validation issue path like `factions[3].preferred_world_types` to the
/// project-relative TOML file the §P4 tree should highlight. Falls back to
/// `sectorforge.toml` for unrecognised prefixes.
fn config_file_for(state: &BuilderState, anchor: &str) -> Option<String> {
    let head = anchor.split(['[', '.']).next().unwrap_or("");
    let inputs = &state.config.inputs;
    match head {
        "factions" => inputs.factions.clone(),
        "routes" => inputs.route_rules.clone(),
        "relations" => inputs.relations.clone(),
        "regions" => inputs.regions.clone(),
        "economy" => inputs.economy.clone(),
        "history" => inputs.history.clone(),
        "names" | "system_names" | "world_names" => inputs
            .system_names
            .clone()
            .or_else(|| inputs.world_names.clone()),
        _ => Some("sectorforge.toml".to_string()),
    }
}

fn group_by_file(issues: &[ValidationIssue]) -> BTreeMap<String, Vec<ValidationIssue>> {
    let mut out: BTreeMap<String, Vec<ValidationIssue>> = BTreeMap::new();
    for i in issues {
        let key = match i.path.as_deref() {
            Some(p) => bucket_for(p),
            None => "(general)".to_string(),
        };
        out.entry(key).or_default().push(i.clone());
    }
    out
}

fn bucket_for(path: &str) -> String {
    path.split(['[', '.']).next().unwrap_or(path).to_string()
}

fn render_workbook(ui: &mut egui::Ui, report: &ValidationReport) {
    ui_kit::collapsing_section(ui, "val_workbook", "World workbook", false, |ui| {
        let w = &report.world_workbook;
        ui.label(format!("rows: {}", w.row_count));
        ui.label(format!("usable candidates: {}", w.usable_candidate_count));
        ui.label(format!("excluded rows: {}", w.excluded_row_count));
        if !w.exclusion_reasons.is_empty() {
            ui.label("excluded by reason:");
            for (k, v) in &w.exclusion_reasons {
                ui.label(format!("  {k}: {v}"));
            }
        }
        if !w.key_table_counts.is_empty() {
            ui.label("key tables:");
            for (k, v) in &w.key_table_counts {
                ui.label(format!("  {k}: {v}"));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::validation::{
        Severity, ValidationIssue, ValidationReport, WorldWorkbookValidation,
    };

    fn issue(code: &str, path: Option<&str>, severity: Severity) -> ValidationIssue {
        ValidationIssue {
            code: code.into(),
            message: "msg".into(),
            path: path.map(str::to_string),
            row: None,
            severity,
        }
    }

    #[test]
    fn group_buckets_by_prefix() {
        let report = ValidationReport {
            ok: false,
            errors: vec![
                issue(
                    "A",
                    Some("factions[1].preferred_world_types"),
                    Severity::Error,
                ),
                issue("B", Some("factions[2]"), Severity::Error),
                issue("C", Some("routes.modifiers[0]"), Severity::Error),
                issue("D", None, Severity::Error),
            ],
            warnings: Vec::new(),
            world_workbook: WorldWorkbookValidation {
                row_count: 0,
                usable_candidate_count: 0,
                excluded_row_count: 0,
                exclusion_reasons: Default::default(),
                key_table_counts: Default::default(),
            },
        };
        let grouped = group_by_file(&report.errors);
        assert_eq!(grouped.get("factions").map(Vec::len), Some(2));
        assert_eq!(grouped.get("routes").map(Vec::len), Some(1));
        assert_eq!(grouped.get("(general)").map(Vec::len), Some(1));
    }

    #[test]
    fn issue_key_disambiguates_rows() {
        let a = issue("DUP", Some("factions[1]"), Severity::Error);
        let mut b = issue("DUP", Some("factions[1]"), Severity::Error);
        b.row = Some(7);
        assert_ne!(issue_key(&a), issue_key(&b));
        // Identical content yields a stable key (selection survives re-derive).
        assert_eq!(
            issue_key(&a),
            issue_key(&issue("DUP", Some("factions[1]"), Severity::Error))
        );
    }
}
