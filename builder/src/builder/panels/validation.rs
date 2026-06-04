//! Validation panel (§V1). Renders the pre-generation
//! [`sectorforge::validation::ValidationReport`] as a file-anchored tree:
//! errors first, warnings under their own collapsing header, and a footer
//! workbook-stats line. Each leaf is a button that updates
//! [`crate::builder::BuilderState::selected_file`] so the §P4 project tree
//! and (Phase E) TOML editor can route the user to the offending row.
//!
//! Each row leads with the human-readable `message`; the diagnostic rule code
//! (e.g. `GEN_SECTOR_NOT_SQUARE`) is the real identifier, so it rides along as a
//! dim secondary token rather than the headline — mirroring the analytics
//! health-flag idiom.
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
use egui::{Color32, RichText};
use sectorforge::validation::{Severity, ValidationIssue, ValidationReport};
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;

use crate::builder::BuilderState;

/// §COLUMNS — view-state key identifying the focused issue. Stored in
/// `ui.data_mut` temp under this id so the right pane can re-find the matching
/// issue from the live report; never persisted, never a model field.
const SELECTED_KEY_ID: &str = "validation_selected_issue";

/// Severity tint for the "error" rows / pips (red).
const COLOUR_ERROR: Color32 = Color32::from_rgb(220, 80, 80);
/// Severity tint for the "warning" rows / pips (amber).
const COLOUR_WARNING: Color32 = Color32::from_rgb(220, 180, 60);
/// Positive status tint for a clean report (green).
const COLOUR_OK: Color32 = Color32::from_rgb(120, 180, 120);

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Validation");
    ui.label(
        RichText::new("Problems found in your config before generating — fix these for a clean run.")
            .color(Color32::DARK_GRAY),
    );
    ui.separator();

    let Some(report) = state.validation_report.clone() else {
        ui_kit::placeholder(
            ui,
            "No checks run yet. Hit Re-validate, or edit any input to kick one off.",
        );
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
    ui.horizontal_wrapped(|ui| {
        if report.ok && !strict_fail {
            ui.colored_label(COLOUR_OK, "✓  All clear");
        } else {
            let txt = if strict_fail && report.errors.is_empty() {
                "✗  Warnings block (strict mode)"
            } else {
                "✗  Problems found"
            };
            ui.colored_label(COLOUR_ERROR, txt);
        }
        ui.label(
            RichText::new(format!(
                "· {} error(s), {} warning(s)",
                report.errors.len(),
                report.warnings.len()
            ))
            .color(Color32::DARK_GRAY),
        );
        if strict {
            ui.colored_label(COLOUR_WARNING, "· strict: warnings count as errors");
        }
    });
}

// ── issue list (left rail) ──────────────────────────────────────────────────

fn show_issue_list(ui: &mut egui::Ui, state: &mut BuilderState, report: &ValidationReport) {
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            if report.errors.is_empty() && report.warnings.is_empty() {
                ui_kit::placeholder(ui, "No problems found — your config is coherent.");
            } else {
                render_group(
                    ui,
                    state,
                    "Errors",
                    &report.errors,
                    Severity::Error,
                    COLOUR_ERROR,
                );
                render_group(
                    ui,
                    state,
                    "Warnings",
                    &report.warnings,
                    Severity::Warning,
                    COLOUR_WARNING,
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
        // Severity dot up front; the headline is the human message, with the
        // file/row anchor prefixed when present. The diagnostic rule code rides
        // along after as a dim secondary token (real identifier, but not the
        // thing a reader scans for).
        ui.colored_label(colour, "●");
        let label = match (&issue.path, issue.row) {
            (Some(p), Some(r)) => format!("{} ({p}, row {r})", issue.message),
            (Some(p), None) => format!("{} ({p})", issue.message),
            (None, _) => issue.message.clone(),
        };
        // Selecting a row pins the issue into the right detail pane and also
        // routes the §P4 project tree to the offending .toml (existing jump).
        let resp = ui
            .selectable_label(is_selected, label)
            .on_hover_text(format!("Rule {} — click to inspect and jump", issue.code));
        if resp.clicked() {
            set_selected_key(ui, key.clone());
            jump_to(state, issue);
        }
        ui.label(
            RichText::new(&issue.code)
                .small()
                .color(palette::chrome_text_dim()),
        );
    });
}

// ── issue detail (right pane) ───────────────────────────────────────────────

fn show_issue_detail(ui: &mut egui::Ui, state: &mut BuilderState, report: &ValidationReport) {
    // Right-pane controls: re-validate + strict toggle live here now.
    ui.horizontal(|ui| {
        if ui
            .button("🔄  Re-validate")
            .on_hover_text("Re-run the checks now instead of waiting for the auto-refresh")
            .clicked()
        {
            state.revalidate_now();
        }
        // §V4 — strict toggle: promote warnings to errors for the health pip
        // and the §V6 pre-export gate (parity with `generate --strict`).
        ui.checkbox(&mut state.validation_strict, "Strict")
            .on_hover_text(
                "Treat warnings as errors for the health pip and the pre-export gate \
                 (schema: strict). Mirrors `sectorforge generate --strict`.",
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
                ui_kit::placeholder(ui, "No problems found — your config is coherent.");
            } else {
                ui_kit::placeholder(ui, "Pick a problem on the left to see the details.");
            }

            ui.separator();
            render_workbook(ui, report);
        });
}

fn render_detail_card(ui: &mut egui::Ui, state: &mut BuilderState, issue: &ValidationIssue) {
    let (colour, sev_label) = match issue.severity {
        Severity::Error => (COLOUR_ERROR, "Error"),
        Severity::Warning => (COLOUR_WARNING, "Warning"),
        Severity::Info => (Color32::GRAY, "Info"),
        _ => (Color32::GRAY, "Info"),
    };
    // The visible section title reads as the severity; the diagnostic code is
    // surfaced as a dim secondary token inside, not used as the headline.
    ui_kit::section(ui, sev_label, |ui| {
        ui_kit::reading_column(ui, 720.0, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(colour, format!("● {sev_label}"));
                ui.label(
                    RichText::new(&issue.code)
                        .small()
                        .color(palette::chrome_text_dim()),
                )
                .on_hover_text("Diagnostic rule code");
            });
            ui.add_space(4.0);
            ui.label(&issue.message);
            ui.add_space(6.0);

            if let Some(path) = &issue.path {
                ui_kit::kv(ui, "Location", path);
            }
            if let Some(row) = issue.row {
                ui_kit::kv(ui, "Line", &row.to_string());
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
                    .button(format!("▸  Open {rel}"))
                    .on_hover_text("Jump the project tree / TOML editor to this file")
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
    ui_kit::collapsing_section(ui, "val_workbook", "World pool summary", false, |ui| {
        let w = &report.world_workbook;
        ui_kit::placeholder(
            ui,
            "What the generator drew from your world data — how many rows it kept, and why it dropped the rest.",
        );
        ui.add_space(4.0);
        ui_kit::kv(ui, "Total rows", &w.row_count.to_string());
        ui_kit::kv(ui, "Usable candidates", &w.usable_candidate_count.to_string());
        ui_kit::kv(ui, "Excluded rows", &w.excluded_row_count.to_string());
        if !w.exclusion_reasons.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new("Excluded because:").color(Color32::DARK_GRAY));
            for (k, v) in &w.exclusion_reasons {
                ui.label(format!("  {k}: {v}"));
            }
        }
        if !w.key_table_counts.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new("Lookup tables:").color(Color32::DARK_GRAY));
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
