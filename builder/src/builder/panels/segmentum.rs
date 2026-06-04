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
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;

use crate::builder::project_io;
use crate::builder::segmentum_run::{progress_label, SegmentumState};
use crate::builder::state::{ConfirmAction, ModalKind};
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

    ui.heading("Segmentum");
    ui.label(
        RichText::new(
            "Compose several independently-generated child sectors into one super-grid, stitched together with inter-sector warp links.",
        )
        .color(Color32::DARK_GRAY),
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
            ui_kit::placeholder(
                ui,
                "No segmentum open yet — click “New segmentum…” to start one, or load an existing segmentum.toml / segmentum.json.",
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
        if ui
            .button("➕  New segmentum…")
            .on_hover_text("Start a fresh segmentum based on this project's id, title, and seed")
            .clicked()
        {
            let id = format!("{}-segmentum", state.config.project.id);
            let title = format!("{} Segmentum", state.config.project.title);
            let seed = format!("{}-stitch", state.config.generation.seed);
            state.segmentum.file = Some(SegmentumState::blank_file(&id, &title, &seed));
            state.segmentum.file_path = None;
            state.segmentum.composed = None;
            state.segmentum.error = None;
        }

        if ui
            .button("📂  Open segmentum.toml…")
            .on_hover_text("Load a saved segmentum layout to edit and compose")
            .clicked()
        {
            if let Some(picked) = rfd::FileDialog::new()
                .add_filter("segmentum", &["toml"])
                .set_title("Open segmentum.toml")
                .pick_file()
            {
                load_toml(state, picked);
            }
        }

        if ui
            .button("📂  Open composed segmentum.json…")
            .on_hover_text("Load an already-composed segmentum to view or re-export")
            .clicked()
        {
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
            .add_enabled(can_save, egui::Button::new("💾  Save segmentum.toml"))
            .on_hover_text("Write the current segmentum layout back to disk")
            .clicked()
        {
            save_toml(state);
        }
    });

    if let Some(path) = &state.segmentum.file_path {
        ui.label(
            RichText::new(format!("Open document: {path}"))
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

/// Aligned label-left / control-right row with a hover tooltip. The visible
/// label reads in human terms ("Columns", "Faction handling") while the tooltip
/// names the underlying TOML field plus a plain-language note, so power users
/// keep the schema mapping. Friendlier replacement for the old bare `egui::Grid`
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

fn config_editor(ui: &mut Ui, state: &mut BuilderState) {
    // §FRIENDLY_PANEL_PASS transform #7: `sg_children_section` runs with only a
    // `&mut SegmentumFile`, so a child 🗑 records the child id here; the confirm
    // modal is opened below, once the `file` borrow on `state` has ended.
    let mut delete_child: Option<String> = None;
    {
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
                sg_children_section(right, file, &mut delete_child);
            }
        });

        // §SG1: a quick visual of the super-grid slot occupancy. This is a 2D grid
        // render that wants the full width, so it stays full-width below the config
        // columns rather than flowing into one of them.
        grid_preview(ui, file);
    }

    if let Some(id) = delete_child {
        state.modal = Some(ModalKind::ConfirmDestructive {
            title: "Remove child sector?".into(),
            body: format!("Drop child “{id}” from the segmentum grid."),
            action: ConfirmAction::DeleteSegmentumChild(id),
        });
    }
}

fn sg_config_section(ui: &mut Ui, file: &mut sectorforge::segmentum::SegmentumFile) {
    ui_kit::collapsing_section(ui, "sg_config", "Segmentum setup (§SG1)", true, |ui| {
        labeled(
            ui,
            "ID",
            "Unique identifier for this segmentum (schema: id). Used in the composed output and file names.",
            |ui| {
                ui.text_edit_singleline(&mut file.segmentum.id);
            },
        );
        labeled(
            ui,
            "Title",
            "Display name shown on the composed super-manifest (schema: title).",
            |ui| {
                ui.text_edit_singleline(&mut file.segmentum.title);
            },
        );
        labeled(
            ui,
            "Stitch seed",
            "Seed that drives where inter-sector warp links are placed (schema: stitch_seed). Same seed + same children = same stitching.",
            |ui| {
                ui.text_edit_singleline(&mut file.segmentum.stitch_seed);
            },
        );
        labeled(
            ui,
            "Columns",
            "Number of columns in the super-grid (schema: columns). Children are placed by column/row.",
            |ui| {
                ui.add(egui::DragValue::new(&mut file.segmentum.columns).range(1..=64));
            },
        );
        labeled(
            ui,
            "Rows",
            "Number of rows in the super-grid (schema: rows). Children are placed by column/row.",
            |ui| {
                ui.add(egui::DragValue::new(&mut file.segmentum.rows).range(1..=64));
            },
        );
        labeled(
            ui,
            "Faction handling",
            "Whether children share one faction roster or keep their own (schema: faction_mode).",
            |ui| faction_mode_combo(ui, &mut file.segmentum.faction_mode),
        );
    });
}

fn sg_stitch_policy_section(ui: &mut Ui, file: &mut sectorforge::segmentum::SegmentumFile) {
    ui_kit::collapsing_section(ui, "sg_stitch_policy", "Stitching rules (§SG1)", false, |ui| {
        labeled(
            ui,
            "Max links per pair",
            "Most warp links to create between any two neighbouring child sectors (schema: max_links_per_pair).",
            |ui| {
                ui.add(egui::DragValue::new(&mut file.stitch.max_links_per_pair).range(0..=16));
            },
        );
        labeled(
            ui,
            "Border depth",
            "How far in from the shared edge a system can sit and still be linked (schema: border_depth). Larger = more candidate systems.",
            |ui| {
                ui.add(egui::DragValue::new(&mut file.stitch.border_depth).range(1..=16));
            },
        );
        labeled(
            ui,
            "Default route type",
            "Route type stamped on each generated inter-sector link (schema: default_route_type).",
            |ui| {
                route_type_combo(
                    ui,
                    "seg_stitch_route_type",
                    &mut file.stitch.default_route_type,
                )
            },
        );
        labeled(
            ui,
            "Default stability",
            "How safe each generated inter-sector link is by default (schema: default_stability).",
            |ui| {
                stability_combo(
                    ui,
                    "seg_stitch_stability",
                    &mut file.stitch.default_stability,
                )
            },
        );
    });
}

fn sg_children_section(
    ui: &mut Ui,
    file: &mut sectorforge::segmentum::SegmentumFile,
    delete_request: &mut Option<String>,
) {
    ui_kit::collapsing_section(
        ui,
        "sg_children",
        &format!("Child sectors ({})", file.children.len()),
        true,
        |ui| {
            if file.children.is_empty() {
                ui_kit::placeholder(
                    ui,
                    "No child sectors yet — click “Add child” to point at a generated project.",
                );
            }
            let cols = file.segmentum.columns;
            let rows = file.segmentum.rows;
            let mut remove: Option<usize> = None;
            for (i, child) in file.children.iter_mut().enumerate() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.strong(format!("Child {}", i + 1));
                        if ui
                            .small_button("🗑  Remove")
                            .on_hover_text("Remove this child sector from the segmentum")
                            .clicked()
                        {
                            remove = Some(i);
                        }
                    });
                    labeled(
                        ui,
                        "ID",
                        "Name for this child within the segmentum (schema: id). Inter-sector links refer to children by this id.",
                        |ui| {
                            ui.text_edit_singleline(&mut child.id);
                        },
                    );
                    labeled(
                        ui,
                        "Project",
                        "Folder of the generated child sector to place here (schema: project).",
                        |ui| {
                            let mut proj = child.project.to_string();
                            if ui.text_edit_singleline(&mut proj).changed() {
                                child.project = Utf8PathBuf::from(proj);
                            }
                            if ui
                                .button("📂  Pick…")
                                .on_hover_text("Browse for the child project folder")
                                .clicked()
                            {
                                if let Some(p) = rfd::FileDialog::new()
                                    .set_title("Pick child project folder")
                                    .pick_folder()
                                {
                                    if let Ok(u) = Utf8PathBuf::from_path_buf(p) {
                                        child.project = u;
                                    }
                                }
                            }
                        },
                    );
                    labeled(
                        ui,
                        "Column",
                        "Column this child occupies in the super-grid, starting at 0 (schema: column).",
                        |ui| {
                            ui.add(
                                egui::DragValue::new(&mut child.column)
                                    .range(0..=cols.saturating_sub(1)),
                            );
                        },
                    );
                    labeled(
                        ui,
                        "Row",
                        "Row this child occupies in the super-grid, starting at 0 (schema: row).",
                        |ui| {
                            ui.add(
                                egui::DragValue::new(&mut child.row)
                                    .range(0..=rows.saturating_sub(1)),
                            );
                        },
                    );
                    labeled(
                        ui,
                        "Seed override",
                        "Optional seed to regenerate this child with, instead of its own (schema: seed).",
                        |ui| optional_text(ui, &mut child.seed, "(use child's own seed)"),
                    );
                    labeled(
                        ui,
                        "Title override",
                        "Optional title to show for this child, instead of its own sector title (schema: title).",
                        |ui| optional_text(ui, &mut child.title, "(use child's sector title)"),
                    );
                });
            }
            // §FRIENDLY_PANEL_PASS transform #7: record the child id for the
            // caller to confirm, rather than dropping the child inline.
            if let Some(i) = remove {
                if let Some(child) = file.children.get(i) {
                    *delete_request = Some(child.id.clone());
                }
            }
            if ui
                .button("➕  Add child")
                .on_hover_text("Add a slot for another child sector")
                .clicked()
            {
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
        "Super-grid layout (§SG1)",
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
    ui.heading("Compose (§SG2)");

    if ui
        .button("📂  Choose output folder…")
        .on_hover_text("Where the composed segmentum and its reports will be written")
        .clicked()
    {
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
            ui.label(
                RichText::new(format!("Output folder: {dir}"))
                    .weak()
                    .monospace(),
            );
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
            .add_enabled(can_compose, egui::Button::new("▶  Compose"))
            .on_hover_text("Load, generate, validate, and stitch every child sector into one segmentum")
            .clicked()
        {
            compose_clicked = true;
        }
        if state.segmentum.is_running()
            && ui
                .button("■  Cancel")
                .on_hover_text("Stop the compose in progress")
                .clicked()
        {
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
        ui.heading("Composed summary (§SG3)");
        ui.label(
            RichText::new(format!(
                "{} — {}  ·  {}×{}  ·  faction handling: {}",
                seg.id,
                seg.title,
                seg.columns,
                seg.rows,
                faction_mode_label(seg.faction_mode)
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
            &format!("Composed children ({})", seg.children.len()),
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
    // §FRIENDLY_PANEL_PASS transform #7: a 🗑 inside the closure records the link
    // index here; the confirm modal is opened after the `seg` borrow ends.
    let mut remove: Option<usize> = None;
    ui_kit::collapsing_section(
        ui,
        "sg_links",
        &format!("Inter-sector warp links ({link_count})"),
        true,
        |ui| {
            if let Some(seg) = state.segmentum.composed.as_mut() {
                if seg.inter_sector_links.is_empty() {
                    ui_kit::placeholder(
                        ui,
                        "No links between child sectors yet — add one below to connect them.",
                    );
                }
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
                            if ui
                                .small_button("🗑")
                                .on_hover_text("Remove this link")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                        labeled(
                            ui,
                            "From system",
                            "System id in the source child where this link starts (schema: from_system_id).",
                            |ui| edit_system_id(ui, &mut link.from_system_id),
                        );
                        labeled(
                            ui,
                            "To system",
                            "System id in the destination child where this link ends (schema: to_system_id).",
                            |ui| edit_system_id(ui, &mut link.to_system_id),
                        );
                        labeled(
                            ui,
                            "Distance",
                            "Travel distance along the link, in map units (schema: distance_units).",
                            |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut link.distance_units).range(1..=999),
                                );
                            },
                        );
                        labeled(
                            ui,
                            "Route type",
                            "What kind of route this link is (schema: route_type).",
                            |ui| {
                                route_type_combo(
                                    ui,
                                    &format!("seg_link_rt_{i}"),
                                    &mut link.route_type,
                                )
                            },
                        );
                        labeled(
                            ui,
                            "Stability",
                            "How safe this link is to travel (schema: stability).",
                            |ui| {
                                stability_combo(
                                    ui,
                                    &format!("seg_link_st_{i}"),
                                    &mut link.stability,
                                )
                            },
                        );
                    });
                }
            }

            ui.separator();
            ui.strong("Add a link");
            add_link_form(ui, state);
        },
    );

    // §FRIENDLY_PANEL_PASS transform #7: inter-sector links bypass the undo bus,
    // so a 🗑 click opens a confirm rather than deleting inline.
    if let Some(idx) = remove {
        state.modal = Some(ModalKind::ConfirmDestructive {
            title: "Remove warp link?".into(),
            body: "Remove this inter-sector warp link.".into(),
            action: ConfirmAction::DeleteSegmentumLink(idx),
        });
    }
}

/// §FRIENDLY_PANEL_PASS transform #7: remove the child sector with id `id`
/// (confirmed payload of [`ModalKind::ConfirmDestructive`]) from the segmentum
/// document. The segmentum file bypasses the undo bus.
pub(crate) fn delete_child(state: &mut BuilderState, id: &str) {
    if let Some(file) = state.segmentum.file.as_mut() {
        if let Some(i) = file.children.iter().position(|c| c.id == id) {
            file.children.remove(i);
        }
    }
}

/// §FRIENDLY_PANEL_PASS transform #7: remove the inter-sector warp link at `idx`
/// (confirmed payload of [`ModalKind::ConfirmDestructive`]) from the composed
/// segmentum and refresh the manifest count.
pub(crate) fn delete_link(state: &mut BuilderState, idx: usize) {
    if let Some(seg) = state.segmentum.composed.as_mut() {
        if idx < seg.inter_sector_links.len() {
            seg.inter_sector_links.remove(idx);
            seg.manifest.inter_sector_link_count = seg.inter_sector_links.len();
        }
    }
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
        labeled(
            ui,
            "From child",
            "Source child sector this link starts in (schema: from_child_id).",
            |ui| {
                child_combo(
                    ui,
                    "seg_add_from_child",
                    &mut draft.from_child_id,
                    &child_ids,
                )
            },
        );
        labeled(
            ui,
            "To child",
            "Destination child sector this link ends in (schema: to_child_id).",
            |ui| child_combo(ui, "seg_add_to_child", &mut draft.to_child_id, &child_ids),
        );
        labeled(
            ui,
            "From system",
            "System id within the source child where the link starts (schema: from_system_id).",
            |ui| {
                ui.text_edit_singleline(&mut draft.from_system_id);
            },
        );
        labeled(
            ui,
            "To system",
            "System id within the destination child where the link ends (schema: to_system_id).",
            |ui| {
                ui.text_edit_singleline(&mut draft.to_system_id);
            },
        );
        labeled(
            ui,
            "Route type",
            "What kind of route the new link is (schema: route_type).",
            |ui| route_type_combo(ui, "seg_add_rt", &mut draft.route_type),
        );
        labeled(
            ui,
            "Stability",
            "How safe the new link is to travel (schema: stability).",
            |ui| stability_combo(ui, "seg_add_st", &mut draft.stability),
        );
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
        .add_enabled(valid, egui::Button::new("➕  Add link"))
        .on_hover_text("Create a warp link between the two chosen child sectors")
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
    ui.heading("Export composed segmentum (§SG5)");
    let has_dir = state.segmentum.output_dir.is_some();
    if !has_dir {
        ui.colored_label(WARN, "Pick an output folder (above) to re-export.");
    }
    if ui
        .add_enabled(
            has_dir && state.segmentum.composed.is_some(),
            egui::Button::new("💾  Export segmentum.json + .md"),
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

/// Friendly, capitalised label for a faction-handling mode. The combo shows this
/// while the raw `faction_mode` key stays reachable on hover.
fn faction_mode_label(mode: FactionMode) -> &'static str {
    match mode.as_slug() {
        "shared" => "Shared roster",
        "independent" => "Independent rosters",
        other => other,
    }
}

/// Friendly, capitalised label for a route stability. The combo shows this while
/// the raw `stability` key stays reachable on hover.
fn stability_label(stability: RouteStability) -> &'static str {
    match stability.as_slug() {
        "stable" => "Stable",
        "unstable" => "Unstable",
        "hazardous" => "Hazardous",
        "perilous" => "Perilous",
        other => other,
    }
}

fn faction_mode_combo(ui: &mut Ui, value: &mut FactionMode) {
    ui_kit::combo("seg_faction_mode", faction_mode_label(*value)).show_ui(ui, |ui| {
        for mode in FACTION_MODES {
            ui.selectable_value(value, mode, faction_mode_label(mode))
                .on_hover_text(mode.as_slug());
        }
    });
}

fn route_type_combo(ui: &mut Ui, id: &str, value: &mut RouteType) {
    ui_kit::combo(id, value.editor_label()).show_ui(ui, |ui| {
        for option in RouteType::ALL {
            ui.selectable_value(value, option, option.editor_label())
                .on_hover_text(option.as_slug());
        }
    });
}

fn stability_combo(ui: &mut Ui, id: &str, value: &mut RouteStability) {
    ui_kit::combo(id, stability_label(*value)).show_ui(ui, |ui| {
        for option in STABILITIES {
            ui.selectable_value(value, option, stability_label(option))
                .on_hover_text(option.as_slug());
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
