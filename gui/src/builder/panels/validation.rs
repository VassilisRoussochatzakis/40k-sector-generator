//! Validation panel (§V1). Renders the pre-generation
//! [`sectorforge::validation::ValidationReport`] as a file-anchored tree:
//! errors first, warnings under their own collapsing header, and a footer
//! workbook-stats line. Each leaf is a button that updates
//! [`crate::builder::BuilderState::selected_file`] so the §P4 project tree
//! and (Phase E) TOML editor can route the user to the offending row.
//!
//! The panel does not mutate the sector — it only reads
//! [`crate::builder::BuilderState::validation_report`] and writes the file
//! selection. A "Re-validate now" button forces an immediate
//! [`crate::builder::BuilderState::revalidate_now`] flush for users who would
//! rather not wait out the debounce.

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use sectorforge::validation::{Severity, ValidationIssue, ValidationReport};

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        ui.heading("Validation");
        if ui.button("Re-validate now").clicked() {
            state.revalidate_now();
        }
    });
    ui.separator();

    let Some(report) = state.validation_report.clone() else {
        ui.colored_label(egui::Color32::GRAY, "no validation report yet");
        return;
    };

    render_summary(ui, &report);
    ui.separator();

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

    ui.separator();
    render_workbook(ui, &report);
}

fn render_summary(ui: &mut egui::Ui, report: &ValidationReport) {
    ui.horizontal(|ui| {
        if report.ok {
            ui.colored_label(egui::Color32::GREEN, "✓ ok");
        } else {
            ui.colored_label(egui::Color32::RED, "✗ errors");
        }
        ui.label(format!(
            "{} error(s), {} warning(s)",
            report.errors.len(),
            report.warnings.len()
        ));
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
    egui::CollapsingHeader::new(format!("{title} ({})", issues.len()))
        .default_open(severity == Severity::Error)
        .show(ui, |ui| {
            for (path, group) in by_file {
                egui::CollapsingHeader::new(format!("{} ({})", path, group.len()))
                    .id_salt(format!("validation-{title}-{path}"))
                    .show(ui, |ui| {
                        for issue in &group {
                            issue_row(ui, state, issue, colour);
                        }
                    });
            }
        });
}

fn issue_row(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    issue: &ValidationIssue,
    colour: egui::Color32,
) {
    ui.horizontal(|ui| {
        ui.colored_label(colour, &issue.code);
        let label = match (&issue.path, issue.row) {
            (Some(p), Some(r)) => format!("{p} (row {r}): {}", issue.message),
            (Some(p), None) => format!("{p}: {}", issue.message),
            (None, _) => issue.message.clone(),
        };
        if ui.link(label).clicked() {
            jump_to(state, issue);
        }
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
    path.split(['[', '.'])
        .next()
        .unwrap_or(path)
        .to_string()
}

fn render_workbook(ui: &mut egui::Ui, report: &ValidationReport) {
    egui::CollapsingHeader::new("World workbook").show(ui, |ui| {
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
    use sectorforge::validation::{Severity, ValidationIssue, ValidationReport, WorldWorkbookValidation};

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
                issue("A", Some("factions[1].preferred_world_types"), Severity::Error),
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
}
