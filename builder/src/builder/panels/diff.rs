//! DIFF tab (§N1 / §N2). Phase E §DF1..§DF5 — two-sector + tick diff editor.
//!
//! §DF1 picks a `before`/`after` from the live sector, a named snapshot, or a
//! `sector.json` on disk. §DF2 clones the current sector, advances N conflict
//! ticks, and diffs the result. §DF3 renders the [`SectorDiff`] as a
//! click-to-expand tree of strata (systems / routes / factions / regions /
//! relations / economy). §DF4 exposes the `skip_worlds` / `skip_routes` /
//! `min_faction_delta` filters. §DF5 writes `diff.md` + `diff.json`.

use camino::Utf8PathBuf;
use egui::{Color32, RichText};

use sectorforge::diff::SectorDiff;
use sectorforge::sector_model::GeneratedSector;
use sectorforge_gui_core::ui_kit;

use crate::builder::diff_run::{DiffMode, LoadedFile, SlotKind};
use crate::builder::{BuilderState, ModalKind};

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Diff");
    ui.label(
        RichText::new("§DF1..§DF5 — compare two sectors or simulate conflict ticks.")
            .small()
            .color(Color32::GRAY),
    );
    ui.separator();

    show_mode(ui, state);
    ui.separator();

    match state.diff.mode {
        DiffMode::TwoSector => show_two_sector_inputs(ui, state),
        DiffMode::Ticks => show_tick_inputs(ui, state),
    }
    ui.separator();

    show_filters(ui, state);
    ui.separator();

    show_actions(ui, state);

    if let Some(err) = state.diff.error.clone() {
        ui.colored_label(Color32::RED, err);
    }
    ui.separator();

    show_report(ui, state);
}

// ── §DF1 / §DF2 mode toggle ────────────────────────────────────────────────

fn show_mode(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        ui.label("mode:");
        ui.selectable_value(
            &mut state.diff.mode,
            DiffMode::TwoSector,
            "Two-sector (§DF1)",
        );
        ui.selectable_value(
            &mut state.diff.mode,
            DiffMode::Ticks,
            "Tick simulation (§DF2)",
        );
    });
}

// ── §DF1 two-sector slot pickers ───────────────────────────────────────────

fn show_two_sector_inputs(ui: &mut egui::Ui, state: &mut BuilderState) {
    let snap_names: Vec<String> = state.snapshots.iter().map(|s| s.name.clone()).collect();

    ui.label(RichText::new("before").strong());
    let err_before = slot_picker(
        ui,
        "diff_before",
        &mut state.diff.before_kind,
        &mut state.diff.before_snapshot,
        &mut state.diff.before_file,
        &snap_names,
    );
    ui.add_space(4.0);
    ui.label(RichText::new("after").strong());
    let err_after = slot_picker(
        ui,
        "diff_after",
        &mut state.diff.after_kind,
        &mut state.diff.after_snapshot,
        &mut state.diff.after_file,
        &snap_names,
    );

    if let Some(e) = err_before.or(err_after) {
        state.diff.error = Some(e);
    }
}

/// One slot row: kind combo + (snapshot combo | file loader). Returns a
/// human-readable error if a file load failed.
fn slot_picker(
    ui: &mut egui::Ui,
    id_salt: &str,
    kind: &mut SlotKind,
    snap: &mut Option<String>,
    file: &mut Option<LoadedFile>,
    snap_names: &[String],
) -> Option<String> {
    let mut error = None;
    ui.horizontal(|ui| {
        ui_kit::combo(format!("{id_salt}_kind"), slot_kind_label(*kind)).show_ui(ui, |ui| {
            ui.selectable_value(kind, SlotKind::Current, "Current sector");
            ui.selectable_value(kind, SlotKind::Snapshot, "Snapshot");
            ui.selectable_value(kind, SlotKind::File, "Load file…");
        });

        match *kind {
            SlotKind::Current => {
                ui.colored_label(Color32::DARK_GRAY, "live in-memory sector");
            }
            SlotKind::Snapshot => {
                if snap_names.is_empty() {
                    ui.colored_label(Color32::GRAY, "no snapshots — create one in PROJECT");
                } else {
                    let current = snap.clone().unwrap_or_default();
                    ui_kit::combo(
                        format!("{id_salt}_snap"),
                        if current.is_empty() {
                            "(pick snapshot)".to_string()
                        } else {
                            current
                        },
                    )
                    .show_ui(ui, |ui| {
                        for name in snap_names {
                            let sel = snap.as_deref() == Some(name.as_str());
                            if ui.selectable_label(sel, name).clicked() {
                                *snap = Some(name.clone());
                            }
                        }
                    });
                }
            }
            SlotKind::File => {
                if ui.button("Load sector.json…").clicked() {
                    if let Some(picked) = rfd::FileDialog::new()
                        .set_title("Load sector.json")
                        .add_filter("sector json", &["json"])
                        .pick_file()
                    {
                        match Utf8PathBuf::from_path_buf(picked) {
                            Ok(path) => match load_sector_file(path) {
                                Ok(loaded) => *file = Some(loaded),
                                Err(e) => error = Some(e),
                            },
                            Err(_) => error = Some("path is not valid UTF-8".to_string()),
                        }
                    }
                }
                let label = file
                    .as_ref()
                    .map(|f| format!("{} (seed {})", f.path, f.sector.seed))
                    .unwrap_or_else(|| "(no file loaded)".to_string());
                ui.colored_label(Color32::DARK_GRAY, label);
            }
        }
    });
    error
}

fn slot_kind_label(kind: SlotKind) -> &'static str {
    match kind {
        SlotKind::Current => "Current sector",
        SlotKind::Snapshot => "Snapshot",
        SlotKind::File => "Load file…",
    }
}

fn load_sector_file(path: Utf8PathBuf) -> Result<LoadedFile, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read {path}: {e}"))?;
    let sector: GeneratedSector =
        serde_json::from_str(&text).map_err(|e| format!("parse {path}: {e}"))?;
    Ok(LoadedFile { path, sector })
}

// ── §DF2 tick inputs ───────────────────────────────────────────────────────

fn show_tick_inputs(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        ui.label("advance");
        ui.add(egui::DragValue::new(&mut state.diff.ticks).range(1..=1000));
        ui.label("conflict ticks from the current sector, then diff against it.");
    });
    ui.colored_label(
        Color32::DARK_GRAY,
        "Re-uses src/conflict.rs::advance_sector — hysteresis preserved.",
    );
}

// ── §DF4 filters ───────────────────────────────────────────────────────────

fn show_filters(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.label(RichText::new("filters (§DF4)").strong());
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.diff.skip_worlds, "skip worlds");
        ui.checkbox(&mut state.diff.skip_routes, "skip routes");
    });
    ui.horizontal(|ui| {
        ui.label("min faction Δ:");
        ui.add(
            egui::DragValue::new(&mut state.diff.min_faction_delta)
                .speed(0.1)
                .range(0.0..=1000.0),
        );
        ui.label("top deltas:");
        ui.add(egui::DragValue::new(&mut state.diff.top_faction_deltas).range(1..=200));
    });
}

// ── compute + export actions ───────────────────────────────────────────────

fn show_actions(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        if ui.button("Compute diff").clicked() {
            compute(state);
        }
        if ui
            .button("Snapshot current as 'before'")
            .on_hover_text("Take a snapshot of the live sector and point the before-slot at it.")
            .clicked()
        {
            let name = format!("diff-before-{}", state.snapshots.len());
            state.snapshot(name.clone());
            state.diff.before_kind = SlotKind::Snapshot;
            state.diff.before_snapshot = Some(name);
        }
        ui.separator();
        if ui.button("Choose export folder…").clicked() {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Choose diff export folder")
                .pick_folder()
            {
                if let Ok(path) = Utf8PathBuf::from_path_buf(folder) {
                    state.diff.export_dir = Some(path);
                }
            }
        }
        let has_report = state.diff.report.is_some();
        let has_dir = state.diff.export_dir.is_some();
        if ui
            .add_enabled(
                has_report && has_dir,
                egui::Button::new("Export diff.md + diff.json (§DF5)"),
            )
            .clicked()
        {
            export(state);
        }
    });
    let dir_label = state
        .diff
        .export_dir
        .as_ref()
        .map(|p| p.to_string())
        .unwrap_or_else(|| "(no export folder picked)".to_string());
    ui.colored_label(Color32::DARK_GRAY, dir_label);
}

fn compute(state: &mut BuilderState) {
    let cfg = state.diff.config();
    let result = match state.diff.mode {
        DiffMode::TwoSector => {
            let before = resolve_slot(
                state,
                state.diff.before_kind,
                state.diff.before_snapshot.as_deref(),
                state.diff.before_file.as_ref(),
            );
            let after = resolve_slot(
                state,
                state.diff.after_kind,
                state.diff.after_snapshot.as_deref(),
                state.diff.after_file.as_ref(),
            );
            match (before, after) {
                (Ok(b), Ok(a)) => Ok(sectorforge::diff::diff_sectors_with(&b, &a, &cfg)),
                (Err(e), _) => Err(format!("before slot: {e}")),
                (_, Err(e)) => Err(format!("after slot: {e}")),
            }
        }
        DiffMode::Ticks => {
            let before = state.sector.clone();
            let mut after = before.clone();
            for _ in 0..state.diff.ticks {
                sectorforge::conflict::advance_sector(&mut after);
            }
            Ok(sectorforge::diff::diff_sectors_with(&before, &after, &cfg))
        }
    };
    match result {
        Ok(d) => {
            state.diff.report = Some(d);
            state.diff.error = None;
        }
        Err(e) => state.diff.error = Some(e),
    }
}

fn resolve_slot(
    state: &BuilderState,
    kind: SlotKind,
    snap: Option<&str>,
    file: Option<&LoadedFile>,
) -> Result<GeneratedSector, String> {
    match kind {
        SlotKind::Current => Ok((*state.sector).clone()),
        SlotKind::Snapshot => {
            let name = snap.ok_or("no snapshot selected")?;
            state
                .snapshots
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.sector.clone())
                .ok_or_else(|| format!("snapshot '{name}' not found"))
        }
        SlotKind::File => file
            .map(|f| f.sector.clone())
            .ok_or_else(|| "no file loaded".to_string()),
    }
}

fn export(state: &mut BuilderState) {
    let Some(dir) = state.diff.export_dir.clone() else {
        return;
    };
    let Some(report) = state.diff.report.clone() else {
        return;
    };
    match sectorforge::write_diff(dir.as_path(), &report) {
        Ok(()) => {
            state.modal = Some(ModalKind::Message(format!(
                "Wrote diff.md and diff.json to {dir}"
            )));
        }
        Err(e) => {
            state.modal = Some(ModalKind::Message(format!("Diff export failed: {e}")));
        }
    }
}

// ── §DF3 result tree ───────────────────────────────────────────────────────

fn show_report(ui: &mut egui::Ui, state: &mut BuilderState) {
    let Some(d) = state.diff.report.as_ref() else {
        ui.colored_label(
            Color32::GRAY,
            "No diff yet — pick inputs above and click \"Compute diff\".",
        );
        return;
    };

    ui.label(RichText::new("result (§DF3)").strong());
    ui.horizontal(|ui| {
        ui.label(format!(
            "systems {} → {} · routes {} → {}",
            d.system_count_before, d.system_count_after, d.route_count_before, d.route_count_after,
        ));
        let (txt, col) = if d.catalog_compatible {
            ("catalog compatible", Color32::from_rgb(40, 160, 40))
        } else {
            (
                "catalog MISMATCH — best effort",
                Color32::from_rgb(200, 140, 0),
            )
        };
        ui.colored_label(col, txt);
    });
    if !d.schema_warnings.is_empty() {
        ui_kit::collapsing_section(
            ui,
            "diff_schema_warn",
            &format!("Schema warnings ({})", d.schema_warnings.len()),
            false,
            |ui| {
                for w in &d.schema_warnings {
                    ui.colored_label(Color32::from_rgb(200, 140, 0), w);
                }
            },
        );
    }

    egui::ScrollArea::vertical()
        .id_salt("diff_tree_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // §COLUMNS — the SectorDiff is one unified delta tree (each line is a
            // baked before→after change, not a naturally two-sided A|B table), so
            // rather than a side-by-side split we flow the six stratum cards
            // across a responsive 2-column board. Collapses to one column when
            // narrow via the `% n` round-robin.
            ui_kit::columns_responsive(ui, 2, 420.0, |cols| {
                let n = cols.len();
                let mut next = 0usize;
                macro_rules! col {
                    () => {{
                        let c = &mut cols[next % n];
                        next += 1;
                        c
                    }};
                }
                show_systems(col!(), d);
                show_routes(col!(), d);
                show_factions(col!(), d);
                show_regions(col!(), d);
                show_relations(col!(), d);
                show_economy(col!(), d);
                let _ = next; // final col!() bump is intentionally unread
            });
        });
}

fn show_systems(ui: &mut egui::Ui, d: &SectorDiff) {
    let header = format!(
        "Systems  (+{} −{} ~{})",
        d.systems_added.len(),
        d.systems_removed.len(),
        d.systems_changed.len()
    );
    ui_kit::collapsing_section(ui, "diff_systems", &header, false, |ui| {
        for s in &d.systems_added {
            ui.colored_label(
                Color32::from_rgb(40, 160, 40),
                format!("+ {} {}", s.id, s.name),
            );
        }
        for s in &d.systems_removed {
            ui.colored_label(
                Color32::from_rgb(190, 60, 60),
                format!("− {} {}", s.id, s.name),
            );
        }
        for c in &d.systems_changed {
            let title = if c.renamed {
                format!("~ {} {} → {}", c.id, c.name_before, c.name_after)
            } else {
                format!("~ {} {}", c.id, c.name_after)
            };
            egui::CollapsingHeader::new(title)
                .id_salt(format!("diff_sys_{}", c.id))
                .show(ui, |ui| {
                    opt_change(
                        ui,
                        "state",
                        c.state_before.map(|s| format!("{s:?}")),
                        c.state_after.map(|s| format!("{s:?}")),
                    );
                    opt_change(
                        ui,
                        "dominant",
                        c.dominant_before.as_ref().map(ToString::to_string),
                        c.dominant_after.as_ref().map(ToString::to_string),
                    );
                    opt_change(
                        ui,
                        "sovereign",
                        c.sovereign_before.as_ref().map(ToString::to_string),
                        c.sovereign_after.as_ref().map(ToString::to_string),
                    );
                    opt_change(
                        ui,
                        "occupier",
                        c.occupier_before.as_ref().map(ToString::to_string),
                        c.occupier_after.as_ref().map(ToString::to_string),
                    );
                    for f in &c.primary_factions_added {
                        ui.label(format!("  + primary faction {f}"));
                    }
                    for f in &c.primary_factions_removed {
                        ui.label(format!("  − primary faction {f}"));
                    }
                    if !c.worlds_added.is_empty()
                        || !c.worlds_removed.is_empty()
                        || !c.worlds_changed.is_empty()
                    {
                        ui.label(format!(
                            "  worlds: +{} −{} ~{}",
                            c.worlds_added.len(),
                            c.worlds_removed.len(),
                            c.worlds_changed.len()
                        ));
                        for w in &c.worlds_added {
                            ui.label(format!("    + {} {}", w.id, w.name));
                        }
                        for w in &c.worlds_removed {
                            ui.label(format!("    − {} {}", w.id, w.name));
                        }
                        for w in &c.worlds_changed {
                            let mut bits: Vec<String> = Vec::new();
                            if w.renamed {
                                bits.push(format!("renamed → {}", w.name_after));
                            }
                            if w.contested_before != w.contested_after {
                                bits.push(format!(
                                    "contested {} → {}",
                                    w.contested_before, w.contested_after
                                ));
                            }
                            if !w.claims_added.is_empty() || !w.claims_removed.is_empty() {
                                bits.push(format!(
                                    "claims +{} −{}",
                                    w.claims_added.len(),
                                    w.claims_removed.len()
                                ));
                            }
                            if !w.presences_added.is_empty() || !w.presences_removed.is_empty() {
                                bits.push(format!(
                                    "presence +{} −{}",
                                    w.presences_added.len(),
                                    w.presences_removed.len()
                                ));
                            }
                            let detail = if bits.is_empty() {
                                "control change".to_string()
                            } else {
                                bits.join(", ")
                            };
                            ui.label(format!("    ~ {} {} ({detail})", w.id, w.name_after));
                        }
                    }
                });
        }
    });
}

fn show_routes(ui: &mut egui::Ui, d: &SectorDiff) {
    let header = format!(
        "Routes  (+{} −{} ~{})",
        d.routes_added.len(),
        d.routes_removed.len(),
        d.routes_changed.len()
    );
    ui_kit::collapsing_section(ui, "diff_routes", &header, false, |ui| {
        for r in &d.routes_added {
            ui.colored_label(
                Color32::from_rgb(40, 160, 40),
                format!("+ {} {} → {}", r.id, r.from_system_id, r.to_system_id),
            );
        }
        for r in &d.routes_removed {
            ui.colored_label(
                Color32::from_rgb(190, 60, 60),
                format!("− {} {} → {}", r.id, r.from_system_id, r.to_system_id),
            );
        }
        for r in &d.routes_changed {
            ui.label(format!(
                "~ {} {} → {}: stability {:?}→{:?}, type {:?}→{:?}",
                r.id,
                r.from_system_id,
                r.to_system_id,
                r.stability_before,
                r.stability_after,
                r.route_type_before,
                r.route_type_after,
            ));
        }
    });
}

fn show_factions(ui: &mut egui::Ui, d: &SectorDiff) {
    ui_kit::collapsing_section(
        ui,
        "diff_factions",
        &format!("Factions  ({} deltas)", d.faction_deltas.len()),
        false,
        |ui| {
            for f in &d.faction_deltas {
                let col = if f.delta > 0.0 {
                    Color32::from_rgb(40, 160, 40)
                } else if f.delta < 0.0 {
                    Color32::from_rgb(190, 60, 60)
                } else {
                    Color32::GRAY
                };
                ui.colored_label(
                    col,
                    format!(
                        "{} ({}): projection {:.1} → {:.1} (Δ{:+.1}), worlds Δ{:+}, systems Δ{:+}",
                        f.name,
                        f.faction_id,
                        f.total_projection_before,
                        f.total_projection_after,
                        f.delta,
                        f.world_presence_delta,
                        f.system_presence_delta,
                    ),
                );
            }
        },
    );
}

fn show_regions(ui: &mut egui::Ui, d: &SectorDiff) {
    let header = format!(
        "Regions  (+{} −{} ~{})",
        d.regions_added.len(),
        d.regions_removed.len(),
        d.regions_changed.len()
    );
    ui_kit::collapsing_section(ui, "diff_regions", &header, false, |ui| {
        for r in &d.regions_added {
            ui.colored_label(
                Color32::from_rgb(40, 160, 40),
                format!("+ {} {:?} ({} hex)", r.id, r.kind, r.hex_count),
            );
        }
        for r in &d.regions_removed {
            ui.colored_label(
                Color32::from_rgb(190, 60, 60),
                format!("− {} {:?} ({} hex)", r.id, r.kind, r.hex_count),
            );
        }
        for r in &d.regions_changed {
            ui.label(format!(
                "~ {}: kind {:?}→{:?}, hex {}→{}",
                r.id, r.kind_before, r.kind_after, r.hex_count_before, r.hex_count_after,
            ));
        }
    });
}

fn show_relations(ui: &mut egui::Ui, d: &SectorDiff) {
    ui_kit::collapsing_section(
        ui,
        "diff_relations",
        &format!("Relations  ({} stance changes)", d.stance_changes.len()),
        false,
        |ui| {
            for s in &d.stance_changes {
                ui.label(format!("{} ↔ {}: {:?} → {:?}", s.a, s.b, s.before, s.after));
            }
        },
    );
}

fn show_economy(ui: &mut egui::Ui, d: &SectorDiff) {
    let header = format!(
        "Economy  ({} balance, +{} −{} stranded)",
        d.economy_balance_changes.len(),
        d.stranded_added.len(),
        d.stranded_removed.len()
    );
    ui_kit::collapsing_section(ui, "diff_economy", &header, false, |ui| {
        for b in &d.economy_balance_changes {
            ui.label(format!(
                "{}: {:.1} → {:.1} (Δ{:+.1})",
                b.resource, b.before, b.after, b.delta
            ));
        }
        for w in &d.stranded_added {
            ui.colored_label(Color32::from_rgb(190, 60, 60), format!("+ stranded {w}"));
        }
        for w in &d.stranded_removed {
            ui.colored_label(Color32::from_rgb(40, 160, 40), format!("− stranded {w}"));
        }
    });
}

/// Render a `label: before → after` line only when the two differ.
fn opt_change(ui: &mut egui::Ui, label: &str, before: Option<String>, after: Option<String>) {
    if before == after {
        return;
    }
    let b = before.unwrap_or_else(|| "—".to_string());
    let a = after.unwrap_or_else(|| "—".to_string());
    ui.label(format!("  {label}: {b} → {a}"));
}
