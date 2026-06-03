//! SEGMENTUM tab (§N1 / §N2) — multi-sector composition editor.
//!
//! Implements `docs/BUILDER_REQS.txt §SG1..§SG5`:
//!   * §SG1 the `segmentum.toml` editor: super-grid layout (columns × rows),
//!     per-child `{ id, project, column, row, seed, title }` entries,
//!     `faction_mode`, `stitch_seed`, and the `[stitch]` policy block.
//!   * §SG2 an off-thread "Compose" run (loads + generates + stitches every
//!     child) with a live per-child progress bar, backed by
//!     [`crate::builder::segmentum_run::SegmentumState`].
//!   * §SG3 the super-manifest preview after compose (digests + per-child
//!     roll-up).
//!   * §SG4 the per-link editor over the composed inter-sector warp links
//!     (add / remove / edit endpoints, route type, stability, distance).
//!   * §SG5 the composed segmentum opens in this same SEGMENTUM tab — freshly
//!     composed, or loaded from a prior `segmentum.json` — and can be
//!     re-exported.

use camino::Utf8PathBuf;
use egui::{Color32, RichText, Ui};

use sectorforge::ids::SystemId;
use sectorforge::sector_model::{RouteStability, RouteType};
use sectorforge::segmentum::{BorderOrientation, ChildEntry, FactionMode, InterSectorLink};
use sectorforge_gui_core::ui_kit;

use crate::builder::project_io;
use crate::builder::segmentum_run::{progress_label, SegmentumState};
use crate::builder::BuilderState;

const STABILITIES: [RouteStability; 4] = [
    RouteStability::Stable,
    RouteStability::Unstable,
    RouteStability::Hazardous,
    RouteStability::Perilous,
];

const FACTION_MODES: [FactionMode; 2] = [FactionMode::Shared, FactionMode::Independent];

const WARN: Color32 = Color32::from_rgb(235, 180, 50);
const ERR: Color32 = Color32::from_rgb(220, 80, 80);
const OK: Color32 = Color32::from_rgb(120, 200, 120);

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    // §SG2: drain a completed compose worker first so the result renders the
    // same frame it lands.
    if state.segmentum.pump() {
        ui.ctx().request_repaint();
    }

    ui.heading("Segmentum (§SG1..§SG5)");
    ui.label(
        RichText::new(
            "Compose several independently-generated child sectors into one super-grid, stitched with inter-sector warp links.",
        )
        .weak(),
    );
    ui.separator();

    document_controls(ui, state);

    if state.segmentum.file.is_none() {
        ui.separator();
        if state.segmentum.composed.is_some() {
            // §SG5: a composed / loaded segmentum can be viewed even without an
            // editable document open.
            composed_section(ui, state);
        } else {
            ui.colored_label(
                WARN,
                "No segmentum document open. Click “New segmentum…”, or load an existing segmentum.toml / segmentum.json.",
            );
        }
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            // §COLUMNS — §SG2 compose controls (the off-thread pump + live
            // progress) stay full-width on top as a control bar; the §SG1 config
            // sections flow across responsive columns below it; the §SG3/§SG4/§SG5
            // composed view (which contains its own grid/table renders) stays
            // full-width underneath.
            compose_controls(ui, state); // §SG2
            ui.separator();
            config_editor(ui, state); // §SG1
            if state.segmentum.composed.is_some() {
                ui.separator();
                composed_section(ui, state); // §SG3 + §SG4 + §SG5
            }
        });
}

// ── Document controls ─────────────────────────────────────────────────────────

fn document_controls(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("New segmentum…").clicked() {
            let id = format!("{}-segmentum", state.config.project.id);
            let title = format!("{} Segmentum", state.config.project.title);
            let seed = format!("{}-stitch", state.config.generation.seed);
            state.segmentum.file = Some(SegmentumState::blank_file(&id, &title, &seed));
            state.segmentum.file_path = None;
            state.segmentum.composed = None;
            state.segmentum.error = None;
        }

        if ui.button("Load segmentum.toml…").clicked() {
            if let Some(picked) = rfd::FileDialog::new()
                .add_filter("segmentum", &["toml"])
                .set_title("Open segmentum.toml")
                .pick_file()
            {
                load_toml(state, picked);
            }
        }

        if ui.button("Load composed segmentum.json…").clicked() {
            if let Some(picked) = rfd::FileDialog::new()
                .add_filter("segmentum", &["json"])
                .set_title("Open composed segmentum.json")
                .pick_file()
            {
                load_composed_json(state, picked);
            }
        }

        let can_save = state.segmentum.file.is_some() && !state.segmentum.is_running();
        if ui
            .add_enabled(can_save, egui::Button::new("Save segmentum.toml"))
            .clicked()
        {
            save_toml(state);
        }
    });

    if let Some(path) = &state.segmentum.file_path {
        ui.label(
            RichText::new(format!("document: {path}"))
                .weak()
                .monospace(),
        );
    }
    if let Some(err) = state.segmentum.error.clone() {
        let colour = if err.starts_with("saved") || err.starts_with("loaded") {
            OK
        } else {
            ERR
        };
        ui.colored_label(colour, err);
    }
}

fn load_toml(state: &mut BuilderState, picked: std::path::PathBuf) {
    let Ok(path) = Utf8PathBuf::from_path_buf(picked) else {
        state.segmentum.error = Some("path is not valid UTF-8".to_string());
        return;
    };
    match sectorforge::load_segmentum_file(&path) {
        Ok(file) => {
            state.segmentum.file = Some(file);
            state.segmentum.file_path = Some(path.clone());
            state.segmentum.composed = None;
            state.segmentum.error = Some(format!("loaded {path}"));
        }
        Err(e) => state.segmentum.error = Some(format!("load failed: {e}")),
    }
}

fn load_composed_json(state: &mut BuilderState, picked: std::path::PathBuf) {
    let Ok(path) = Utf8PathBuf::from_path_buf(picked) else {
        state.segmentum.error = Some("path is not valid UTF-8".to_string());
        return;
    };
    match sectorforge::load_segmentum_json(&path) {
        Ok(seg) => {
            state.segmentum.composed = Some(seg);
            state.segmentum.error = Some(format!("loaded composed {path}"));
        }
        Err(e) => state.segmentum.error = Some(format!("load failed: {e}")),
    }
}

fn save_toml(state: &mut BuilderState) {
    let Some(file) = state.segmentum.file.clone() else {
        return;
    };
    // Resolve a target path: reuse the loaded path, else prompt.
    let target = match state.segmentum.file_path.clone() {
        Some(p) => Some(p),
        None => rfd::FileDialog::new()
            .add_filter("segmentum", &["toml"])
            .set_file_name("segmentum.toml")
            .set_title("Save segmentum.toml")
            .save_file()
            .and_then(|p| Utf8PathBuf::from_path_buf(p).ok()),
    };
    let Some(target) = target else {
        return;
    };
    match toml::to_string_pretty(&file) {
        Ok(text) => match project_io::atomic_write(&target, text.as_bytes()) {
            Ok(()) => {
                state.segmentum.file_path = Some(target.clone());
                state.segmentum.error = Some(format!("saved {target}"));
            }
            Err(e) => state.segmentum.error = Some(format!("save failed: {e}")),
        },
        Err(e) => state.segmentum.error = Some(format!("serialise failed: {e}")),
    }
}

// ── §SG1 config editor ──────────────────────────────────────────────────────

fn config_editor(ui: &mut Ui, state: &mut BuilderState) {
    let Some(file) = state.segmentum.file.as_mut() else {
        return;
    };

    // §COLUMNS — RC-2 over the config sections: grid/seed config + [stitch]
    // policy share the first column (both are short 2-col field grids), the
    // taller per-child editor takes the second. Hand-assigned so the policy
    // pair stays grouped; collapse-safe via the `if n > 1` guard. The single
    // `&mut file` borrow is reused sequentially across the columns, so no two
    // closures hold it at once.
    ui_kit::columns_responsive(ui, 2, 360.0, |cols| {
        let n = cols.len();
        {
            let left = &mut cols[0];
            sg_config_section(left, file);
            left.add_space(4.0);
            sg_stitch_policy_section(left, file);
        }
        {
            let right = &mut cols[if n > 1 { 1 } else { 0 }];
            if n == 1 {
                right.add_space(4.0);
            }
            sg_children_section(right, file);
        }
    });

    // §SG1: a quick visual of the super-grid slot occupancy. This is a 2D grid
    // render that wants the full width, so it stays full-width below the config
    // columns rather than flowing into one of them.
    grid_preview(ui, file);
}

fn sg_config_section(ui: &mut Ui, file: &mut sectorforge::segmentum::SegmentumFile) {
    ui_kit::collapsing_section(ui, "sg_config", "§SG1 — Segmentum config", true, |ui| {
        egui::Grid::new("seg_cfg_grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("id");
                ui.text_edit_singleline(&mut file.segmentum.id);
                ui.end_row();
                ui.label("title");
                ui.text_edit_singleline(&mut file.segmentum.title);
                ui.end_row();
                ui.label("stitch_seed");
                ui.text_edit_singleline(&mut file.segmentum.stitch_seed);
                ui.end_row();
                ui.label("columns");
                ui.add(egui::DragValue::new(&mut file.segmentum.columns).range(1..=64));
                ui.end_row();
                ui.label("rows");
                ui.add(egui::DragValue::new(&mut file.segmentum.rows).range(1..=64));
                ui.end_row();
                ui.label("faction_mode");
                faction_mode_combo(ui, &mut file.segmentum.faction_mode);
                ui.end_row();
            });
    });
}

fn sg_stitch_policy_section(ui: &mut Ui, file: &mut sectorforge::segmentum::SegmentumFile) {
    ui_kit::collapsing_section(
        ui,
        "sg_stitch_policy",
        "§SG1 — [stitch] policy",
        false,
        |ui| {
            egui::Grid::new("seg_stitch_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("max_links_per_pair");
                    ui.add(egui::DragValue::new(&mut file.stitch.max_links_per_pair).range(0..=16));
                    ui.end_row();
                    ui.label("border_depth");
                    ui.add(egui::DragValue::new(&mut file.stitch.border_depth).range(1..=16));
                    ui.end_row();
                    ui.label("default_route_type");
                    route_type_combo(
                        ui,
                        "seg_stitch_route_type",
                        &mut file.stitch.default_route_type,
                    );
                    ui.end_row();
                    ui.label("default_stability");
                    stability_combo(
                        ui,
                        "seg_stitch_stability",
                        &mut file.stitch.default_stability,
                    );
                    ui.end_row();
                });
        },
    );
}

fn sg_children_section(ui: &mut Ui, file: &mut sectorforge::segmentum::SegmentumFile) {
    ui_kit::collapsing_section(
        ui,
        "sg_children",
        &format!("§SG1 — children ({})", file.children.len()),
        true,
        |ui| {
            let cols = file.segmentum.columns;
            let rows = file.segmentum.rows;
            let mut remove: Option<usize> = None;
            for (i, child) in file.children.iter_mut().enumerate() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.strong(format!("child #{i}"));
                        if ui.small_button("✖ remove").clicked() {
                            remove = Some(i);
                        }
                    });
                    egui::Grid::new(("seg_child", i))
                        .num_columns(2)
                        .spacing([12.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("id");
                            ui.text_edit_singleline(&mut child.id);
                            ui.end_row();

                            ui.label("project");
                            ui.horizontal(|ui| {
                                let mut proj = child.project.to_string();
                                if ui.text_edit_singleline(&mut proj).changed() {
                                    child.project = Utf8PathBuf::from(proj);
                                }
                                if ui.button("pick…").clicked() {
                                    if let Some(p) = rfd::FileDialog::new()
                                        .set_title("Pick child project folder")
                                        .pick_folder()
                                    {
                                        if let Ok(u) = Utf8PathBuf::from_path_buf(p) {
                                            child.project = u;
                                        }
                                    }
                                }
                            });
                            ui.end_row();

                            ui.label("column");
                            ui.add(
                                egui::DragValue::new(&mut child.column)
                                    .range(0..=cols.saturating_sub(1)),
                            );
                            ui.end_row();
                            ui.label("row");
                            ui.add(
                                egui::DragValue::new(&mut child.row)
                                    .range(0..=rows.saturating_sub(1)),
                            );
                            ui.end_row();

                            ui.label("seed override");
                            optional_text(ui, &mut child.seed, "(use child's own seed)");
                            ui.end_row();
                            ui.label("title override");
                            optional_text(ui, &mut child.title, "(use child's sector title)");
                            ui.end_row();
                        });
                });
            }
            if let Some(i) = remove {
                file.children.remove(i);
            }
            if ui.button("+ Add child").clicked() {
                let n = file.children.len() + 1;
                file.children.push(ChildEntry {
                    id: format!("child-{n}"),
                    project: Utf8PathBuf::new(),
                    column: 0,
                    row: 0,
                    seed: None,
                    title: None,
                });
            }
        },
    );
}

/// A small read-only super-grid map: one cell per `(column, row)`, naming the
/// child that occupies it (or `·` when empty). Surfaces duplicate slots in red.
fn grid_preview(ui: &mut Ui, file: &sectorforge::segmentum::SegmentumFile) {
    ui_kit::collapsing_section(
        ui,
        "sg_grid_preview",
        "§SG1 — super-grid preview",
        true,
        |ui| {
            let cols = file.segmentum.columns.max(1);
            let rows = file.segmentum.rows.max(1);
            egui::Grid::new("seg_grid_preview")
                .num_columns(cols as usize)
                .spacing([6.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    for r in 0..rows {
                        for c in 0..cols {
                            let occupants: Vec<&str> = file
                                .children
                                .iter()
                                .filter(|ch| ch.column == c && ch.row == r)
                                .map(|ch| ch.id.as_str())
                                .collect();
                            match occupants.as_slice() {
                                [] => {
                                    ui.label(RichText::new("·").weak());
                                }
                                [one] => {
                                    ui.label(*one);
                                }
                                many => {
                                    ui.colored_label(ERR, format!("⚠ {}", many.join(", ")));
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
        },
    );
}

// ── §SG2 compose controls ──────────────────────────────────────────────────

fn compose_controls(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("§SG2 — Compose");

    if ui.button("Choose output folder…").clicked() {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Choose segmentum output folder")
            .pick_folder()
        {
            if let Ok(path) = Utf8PathBuf::from_path_buf(folder) {
                state.segmentum.output_dir = Some(path);
            }
        }
    }
    match &state.segmentum.output_dir {
        Some(dir) => {
            ui.label(RichText::new(format!("output: {dir}")).weak().monospace());
        }
        None => {
            ui.colored_label(WARN, "Pick an output folder before composing.");
        }
    }

    let child_count = state
        .segmentum
        .file
        .as_ref()
        .map_or(0, |f| f.children.len());
    let can_compose =
        child_count > 0 && state.segmentum.output_dir.is_some() && !state.segmentum.is_running();

    let mut compose_clicked = false;
    let mut cancel_clicked = false;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(can_compose, egui::Button::new("▶ Compose"))
            .on_hover_text("Loads, generates, validates and stitches every child sector off-thread")
            .clicked()
        {
            compose_clicked = true;
        }
        if state.segmentum.is_running() && ui.button("■ Cancel").clicked() {
            cancel_clicked = true;
        }
    });
    if child_count == 0 {
        ui.colored_label(WARN, "Add at least one child before composing.");
    }

    // §SG2: live per-child progress while a worker is in flight.
    if state.segmentum.is_running() {
        let frac = state.segmentum.job.as_ref().map_or(0.0, |j| j.progress());
        ui.add(egui::ProgressBar::new(frac).show_percentage());
        let label = state
            .segmentum
            .progress_snapshot()
            .map(|p| progress_label(&p))
            .unwrap_or_else(|| "starting…".to_string());
        ui.label(RichText::new(label).monospace());
    }

    if cancel_clicked {
        state.segmentum.cancel();
    }

    if compose_clicked {
        if let (Some(file), Some(output_dir)) = (
            state.segmentum.file.clone(),
            state.segmentum.output_dir.clone(),
        ) {
            let project_parent = state
                .project_path
                .as_ref()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()));
            let base_dir = state.segmentum.base_dir(project_parent.as_ref());
            let ctx = ui.ctx().clone();
            state.segmentum.spawn(&ctx, file, base_dir, output_dir);
        }
    }
}

// ── §SG3 / §SG4 / §SG5 composed view ────────────────────────────────────────

fn composed_section(ui: &mut Ui, state: &mut BuilderState) {
    // §SG3 super-manifest preview — read-only over an immutable borrow.
    {
        let Some(seg) = state.segmentum.composed.as_ref() else {
            return;
        };
        ui.heading("§SG3 — Super-manifest");
        ui.label(
            RichText::new(format!(
                "{} — {}  ·  {}×{}  ·  faction_mode: {}",
                seg.id, seg.title, seg.columns, seg.rows, seg.faction_mode
            ))
            .strong(),
        );
        let m = &seg.manifest;
        ui.label(
            RichText::new(format!(
                "generator {} v{}\nstitch_seed `{}`  (hash {})\nsettings_digest {}\nsystems {} · worlds {} · routes {} · inter-sector links {}",
                seg.generator_name,
                seg.generator_version,
                m.stitch_seed,
                short_hash(&m.stitch_seed_hash),
                short_hash(&m.settings_digest),
                m.system_count,
                m.world_count,
                m.route_count,
                m.inter_sector_link_count,
            ))
            .monospace(),
        );

        ui_kit::collapsing_section(
            ui,
            "sg_manifest_children",
            &format!("§SG3 — children ({})", seg.children.len()),
            true,
            |ui| {
                egui::Grid::new("seg_manifest_children")
                    .num_columns(6)
                    .striped(true)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        for h in ["id", "slot", "title", "sys/wld/rte", "seed", "digest"] {
                            ui.label(RichText::new(h).strong());
                        }
                        ui.end_row();
                        for c in &seg.children {
                            ui.label(c.id.as_str());
                            ui.label(format!("({}, {})", c.column, c.row));
                            ui.label(c.title.as_str());
                            ui.label(format!(
                                "{}/{}/{}",
                                c.system_count, c.world_count, c.route_count
                            ));
                            ui.label(RichText::new(c.seed.as_str()).monospace());
                            ui.label(RichText::new(short_hash(&c.sector_digest)).monospace());
                            ui.end_row();
                        }
                    });
            },
        );
    }

    // §SG4 link editor + §SG5 re-export — mutable borrow.
    link_editor(ui, state);
    reexport_controls(ui, state);
}

fn link_editor(ui: &mut Ui, state: &mut BuilderState) {
    let link_count = state
        .segmentum
        .composed
        .as_ref()
        .map_or(0, |s| s.inter_sector_links.len());
    ui_kit::collapsing_section(
        ui,
        "sg_links",
        &format!("§SG4 — inter-sector links ({link_count})"),
        true,
        |ui| {
            if let Some(seg) = state.segmentum.composed.as_mut() {
                let mut remove: Option<usize> = None;
                for (i, link) in seg.inter_sector_links.iter_mut().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(link.id.as_str());
                            ui.label(
                                RichText::new(format!(
                                    "{} → {}  ({})",
                                    link.from_child_id, link.to_child_id, link.orientation
                                ))
                                .weak(),
                            );
                            if ui.small_button("✖").clicked() {
                                remove = Some(i);
                            }
                        });
                        egui::Grid::new(("seg_link", i))
                            .num_columns(2)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("from system");
                                edit_system_id(ui, &mut link.from_system_id);
                                ui.end_row();
                                ui.label("to system");
                                edit_system_id(ui, &mut link.to_system_id);
                                ui.end_row();
                                ui.label("distance_units");
                                ui.add(
                                    egui::DragValue::new(&mut link.distance_units).range(1..=999),
                                );
                                ui.end_row();
                                ui.label("route_type");
                                route_type_combo(
                                    ui,
                                    &format!("seg_link_rt_{i}"),
                                    &mut link.route_type,
                                );
                                ui.end_row();
                                ui.label("stability");
                                stability_combo(
                                    ui,
                                    &format!("seg_link_st_{i}"),
                                    &mut link.stability,
                                );
                                ui.end_row();
                            });
                    });
                }
                if let Some(i) = remove {
                    seg.inter_sector_links.remove(i);
                    seg.manifest.inter_sector_link_count = seg.inter_sector_links.len();
                }
            }

            ui.separator();
            ui.strong("Add link");
            add_link_form(ui, state);
        },
    );
}

fn add_link_form(ui: &mut Ui, state: &mut BuilderState) {
    // Child id list for the endpoint combos.
    let child_ids: Vec<String> = state
        .segmentum
        .composed
        .as_ref()
        .map(|s| s.children.iter().map(|c| c.id.clone()).collect())
        .unwrap_or_default();

    {
        let draft = &mut state.segmentum.new_link;
        egui::Grid::new("seg_add_link")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("from child");
                child_combo(
                    ui,
                    "seg_add_from_child",
                    &mut draft.from_child_id,
                    &child_ids,
                );
                ui.end_row();
                ui.label("to child");
                child_combo(ui, "seg_add_to_child", &mut draft.to_child_id, &child_ids);
                ui.end_row();
                ui.label("from system id");
                ui.text_edit_singleline(&mut draft.from_system_id);
                ui.end_row();
                ui.label("to system id");
                ui.text_edit_singleline(&mut draft.to_system_id);
                ui.end_row();
                ui.label("route_type");
                route_type_combo(ui, "seg_add_rt", &mut draft.route_type);
                ui.end_row();
                ui.label("stability");
                stability_combo(ui, "seg_add_st", &mut draft.stability);
                ui.end_row();
            });
    }

    let draft = &state.segmentum.new_link;
    let valid = !draft.from_child_id.is_empty()
        && !draft.to_child_id.is_empty()
        && !draft.from_system_id.trim().is_empty()
        && !draft.to_system_id.trim().is_empty();
    if !valid {
        ui.colored_label(WARN, "Pick both child sectors and enter both system ids.");
    }

    if ui
        .add_enabled(valid, egui::Button::new("+ Add link"))
        .clicked()
    {
        if let Some(seg) = state.segmentum.composed.as_mut() {
            let draft = &state.segmentum.new_link;
            let orientation = infer_orientation(seg, &draft.from_child_id, &draft.to_child_id);
            // Manual links are namespaced so they never collide with generated
            // `sl-NNNN` ids.
            let id = next_manual_link_id(seg);
            seg.inter_sector_links.push(InterSectorLink {
                id,
                from_child_id: draft.from_child_id.clone(),
                to_child_id: draft.to_child_id.clone(),
                from_system_id: SystemId::new(draft.from_system_id.trim()),
                to_system_id: SystemId::new(draft.to_system_id.trim()),
                orientation,
                distance_units: 1,
                route_type: draft.route_type,
                stability: draft.stability,
            });
            seg.manifest.inter_sector_link_count = seg.inter_sector_links.len();
        }
    }
}

/// Infer the border orientation from the two children's super-grid slots:
/// different rows ⇒ north/south, otherwise east/west.
fn infer_orientation(
    seg: &sectorforge::segmentum::Segmentum,
    from_id: &str,
    to_id: &str,
) -> BorderOrientation {
    let row_of = |id: &str| seg.children.iter().find(|c| c.id == id).map(|c| c.row);
    match (row_of(from_id), row_of(to_id)) {
        (Some(a), Some(b)) if a != b => BorderOrientation::NorthSouth,
        _ => BorderOrientation::EastWest,
    }
}

/// First unused `sl-manual-NNNN` id for a manually-added link.
fn next_manual_link_id(seg: &sectorforge::segmentum::Segmentum) -> String {
    let mut n = seg.inter_sector_links.len() + 1;
    loop {
        let candidate = format!("sl-manual-{n:04}");
        if !seg.inter_sector_links.iter().any(|l| l.id == candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn reexport_controls(ui: &mut Ui, state: &mut BuilderState) {
    // §SG5: write the (possibly hand-edited) composed segmentum back out as
    // segmentum.md + segmentum.json + super_manifest.json.
    ui.separator();
    ui.heading("§SG5 — Export composed segmentum");
    let has_dir = state.segmentum.output_dir.is_some();
    if !has_dir {
        ui.colored_label(WARN, "Pick an output folder (above) to re-export.");
    }
    if ui
        .add_enabled(
            has_dir && state.segmentum.composed.is_some(),
            egui::Button::new("Export segmentum.json + .md"),
        )
        .on_hover_text(
            "Writes segmentum.json, segmentum.md and super_manifest.json into the output folder",
        )
        .clicked()
    {
        if let (Some(seg), Some(dir)) = (
            state.segmentum.composed.as_ref(),
            state.segmentum.output_dir.clone(),
        ) {
            match sectorforge::write_segmentum(&dir, seg) {
                Ok(()) => state.segmentum.error = Some(format!("saved reports to {dir}")),
                Err(e) => state.segmentum.error = Some(format!("export failed: {e}")),
            }
        }
    }
}

// ── widgets ─────────────────────────────────────────────────────────────────

fn edit_system_id(ui: &mut Ui, value: &mut SystemId) {
    let mut buf = value.as_str().to_string();
    if ui.text_edit_singleline(&mut buf).changed() {
        *value = SystemId::new(buf.trim());
    }
}

fn optional_text(ui: &mut Ui, value: &mut Option<String>, hint: &str) {
    let mut buf = value.clone().unwrap_or_default();
    let resp = ui.add(egui::TextEdit::singleline(&mut buf).hint_text(hint));
    if resp.changed() {
        *value = (!buf.trim().is_empty()).then(|| buf.trim().to_string());
    }
}

fn faction_mode_combo(ui: &mut Ui, value: &mut FactionMode) {
    ui_kit::combo("seg_faction_mode", value.as_slug()).show_ui(ui, |ui| {
        for mode in FACTION_MODES {
            ui.selectable_value(value, mode, mode.as_slug());
        }
    });
}

fn route_type_combo(ui: &mut Ui, id: &str, value: &mut RouteType) {
    ui_kit::combo(id, value.editor_label()).show_ui(ui, |ui| {
        for option in RouteType::ALL {
            ui.selectable_value(value, option, option.editor_label());
        }
    });
}

fn stability_combo(ui: &mut Ui, id: &str, value: &mut RouteStability) {
    ui_kit::combo(id, value.as_slug()).show_ui(ui, |ui| {
        for option in STABILITIES {
            ui.selectable_value(value, option, option.as_slug());
        }
    });
}

fn child_combo(ui: &mut Ui, id: &str, value: &mut String, child_ids: &[String]) {
    let selected = if value.is_empty() {
        "(pick)".to_string()
    } else {
        value.clone()
    };
    ui_kit::combo(id, selected).show_ui(ui, |ui| {
        for cid in child_ids {
            ui.selectable_value(value, cid.clone(), cid);
        }
    });
}

/// Render a `blake3:<hex>` digest as the algo tag plus the first 8 hex chars.
fn short_hash(h: &str) -> &str {
    match h.split_once(':') {
        Some((algo, hex)) => {
            let prefix_len = (algo.len() + 1 + hex.len().min(8)).min(h.len());
            &h[..prefix_len]
        }
        None => &h[..h.len().min(12)],
    }
}
