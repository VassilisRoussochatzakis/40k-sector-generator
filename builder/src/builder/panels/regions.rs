//! REGIONS tab — Phase C §REG1..§REG7.
//!
//! §REG1 region table editor (id / name / kind / hexes / centre / remove).
//! §REG2 paint tool hint (the MAP tab's `REGION PAINT` mode brushes into the
//!       currently selected region; primary = paint, secondary = erase).
//! §REG3 grow-seeded-region command — wraps `regions::seed_region`.
//! §REG4 live route effect preview — counts routes whose endpoints fall under
//!       a region condition and offers an "apply to routes" button that calls
//!       `regions::apply_route_effects` against the live sector.
//! §REG5 `regions.toml` config editor (`DataCatalogs::regions`).
//! §REG6 inline invariant chips for `REGION_HEX_OUT_OF_BOUNDS`,
//!       `REGION_HEX_OVERLAP`, `REGION_ISOLATES_SECTOR`.
//! §REG7 ASCII glyph preview grid (`~ ^ = # *` + extended set).

use std::collections::BTreeMap;

use egui::{Color32, RichText, Ui};

use sectorforge::regions::{
    apply_route_effects, dominant_route_condition, seed_region, ConditionEntry,
    RegionConditionKind, RegionsConfig, WarpRegion,
};
use sectorforge::sector_model::HexCoord;
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit::{self, labeled};

use crate::builder::command::BuilderCommand;
use crate::builder::state::{BuilderTab, EntityRef, MapTool};
use crate::builder::BuilderState;

pub(crate) fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Warp Regions");
    ui.label(
        RichText::new(
            "Paint warp phenomena over the map — storms, calm corridors, anomalies — that bend nearby routes.",
        )
        .color(Color32::DARK_GRAY),
    );
    ui.add_space(2.0);

    // §COLUMNS — 3-pane: the region table + invariants pin to the left rail
    // (`regions_table`); the §REG5 `regions.toml` config form pins to the right
    // rail (`regions_config`); the centre `CentralPanel` carries the selected
    // region editor + paint / grow / route-effect / glyph-map surfaces (the
    // glyph map wants width). SidePanels claim their width before the central
    // pane fills the rest, so each is declared before the CentralPanel. Each
    // closure takes `&mut state` in turn (left, then right, then centre), so
    // there is never more than one live mutable borrow.
    egui::SidePanel::left("regions_table")
        .resizable(true)
        .default_width(250.0)
        .width_range(200.0..=420.0)
        .show_inside(ui, |ui| show_region_table(ui, state));

    egui::SidePanel::right("regions_config")
        .resizable(true)
        .default_width(300.0)
        .width_range(240.0..=480.0)
        .show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| show_regions_config_editor(ui, state));
        });

    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                if let Some(idx) = selected_region_index(state) {
                    show_region_inspector(ui, state, idx);
                } else {
                    ui_kit::placeholder(
                        ui,
                        "No region selected yet — pick one on the left, or use ➕ New region to add one.",
                    );
                }
                ui.separator();
                show_grow_seeded(ui, state);
                show_route_effects(ui, state);
                show_paint_hint(ui, state);
                show_glyph_preview(ui, state);
            });
    });
}

/// §COLUMNS — left-rail region table (master pane): the §REG6 invariant chips
/// on top, then the §REG1 region list with select + `+ new region`. Selection
/// is pure view state, set directly.
fn show_region_table(ui: &mut Ui, state: &mut BuilderState) {
    ui.add_space(2.0);
    show_invariants(ui, state);
    ui.separator();
    show_region_picker(ui, state);
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn selected_region_index(state: &mut BuilderState) -> Option<usize> {
    if let Some(id) = state.selected_region_id.clone() {
        if let Some(i) = state.sector.regions.iter().position(|r| r.id == id) {
            return Some(i);
        }
    }
    state.selected_region_id = state.sector.regions.first().map(|r| r.id.clone());
    state
        .selected_region_id
        .as_ref()
        .and_then(|id| state.sector.regions.iter().position(|r| &r.id == id))
}

// ── §REG6 invariants summary ────────────────────────────────────────────────

fn show_invariants(ui: &mut Ui, state: &BuilderState) {
    let Some(report) = state.invariant_report.as_ref() else {
        ui_kit::placeholder(
            ui,
            "Checks haven't run yet — edit a region to see warnings here.",
        );
        return;
    };
    let codes = [
        "REGION_HEX_OUT_OF_BOUNDS",
        "REGION_HEX_OVERLAP",
        "REGION_ISOLATES_SECTOR",
    ];
    let hits: Vec<_> = report
        .violations
        .iter()
        .filter(|v| codes.contains(&v.code.as_str()))
        .collect();
    if hits.is_empty() {
        ui.colored_label(Color32::DARK_GREEN, "✔ No region problems found.");
        return;
    }
    ui.colored_label(
        palette::danger(),
        format!("⚠ {} region problem(s):", hits.len()),
    );
    for v in hits {
        ui.label(RichText::new(format!("• {}", v.message)).color(palette::danger()))
            .on_hover_text(format!("check: {}", v.code));
    }
}

// ── §REG1 region picker + inspector ─────────────────────────────────────────

fn show_region_picker(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("Regions ({})", state.sector.regions.len())).strong());
        if ui
            .button("➕  New region")
            .on_hover_text("Add a region centred on the map and select it")
            .clicked()
        {
            let centre = default_new_centre(state);
            state.add_region("Region", RegionConditionKind::Turbulence, centre);
        }
    });
    if state.sector.regions.is_empty() {
        ui_kit::placeholder(
            ui,
            "No regions yet — use ➕ New region above, then paint its hexes on the map.",
        );
        return;
    }
    let current = state.selected_region_id.clone();
    let mut pick: Option<String> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for r in state.sector.regions.iter() {
                let glyph = r.kind.glyph();
                let line = format!(
                    "{glyph} {} — {}  ({} hex)",
                    r.name,
                    r.kind.label(),
                    r.hexes.len()
                );
                if ui
                    .selectable_label(current.as_deref() == Some(r.id.as_str()), line)
                    .on_hover_text(format!("id: {}", r.id))
                    .clicked()
                {
                    pick = Some(r.id.clone());
                }
            }
        });
    if let Some(id) = pick {
        state.selected_region_id = Some(id);
    }
}

fn default_new_centre(state: &BuilderState) -> HexCoord {
    let w = state.sector.width.max(1) as i32;
    let h = state.sector.height.max(1) as i32;
    HexCoord {
        q: (w / 2).min(w - 1).max(0),
        r: (h / 2).min(h - 1).max(0),
    }
}

fn show_region_inspector(ui: &mut Ui, state: &mut BuilderState, idx: usize) {
    let region = state.sector.regions[idx].clone();
    let id = region.id.clone();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(region.name.clone()).strong());
            ui.label(RichText::new(format!("id: {id}")).color(Color32::DARK_GRAY))
                .on_hover_text("Stable identifier (schema: id) — used by saved files and the map.");
        });
        ui.label(
            RichText::new(format!(
                "{} hex(es) · centre ({},{}) · map symbol {}",
                region.hexes.len(),
                region.centre.q,
                region.centre.r,
                region.kind.glyph(),
            ))
            .color(Color32::DARK_GRAY),
        );
        ui.add_space(2.0);

        // name (schema: name)
        let name_key = egui::Id::new(("region_name_buf", id.as_str()));
        let name_src = region.name.clone();
        labeled(
            ui,
            "Display name",
            "Friendly name shown in the region list and tooltips (schema: name).",
            |ui| {
                let (name, resp) =
                    crate::builder::panels::persistent_singleline(ui, name_key, &name_src);
                if resp.lost_focus() && name != name_src {
                    let _ = state.update_region(&id, |r| r.name = name.clone());
                    crate::builder::panels::persistent_text_clear(ui, name_key);
                }
            },
        );

        // kind (schema: kind)
        let mut current = region.kind;
        labeled(
            ui,
            "Phenomenon",
            "What kind of warp condition this region is (schema: kind). Decides how it affects nearby routes.",
            |ui| {
                ui_kit::combo(("region_kind", &id), current.label()).show_ui(ui, |ui| {
                    for k in RegionConditionKind::ALL {
                        ui.selectable_value(&mut current, *k, k.label());
                    }
                });
            },
        );
        if current != region.kind {
            let _ = state.update_region(&id, |r| r.kind = current);
        }
        ui.label(RichText::new(current.description()).color(Color32::DARK_GRAY));
        ui.add_space(2.0);

        // centre (schema: centre)
        labeled(
            ui,
            "Centre hex",
            "Where the region is anchored on the map (schema: centre). Growing seeds outward from here.",
            |ui| {
                let mut q = region.centre.q;
                let r_old = region.centre.r;
                let w = state.sector.width.saturating_sub(1) as i32;
                let h = state.sector.height.saturating_sub(1) as i32;
                ui.label("q");
                ui.add(egui::DragValue::new(&mut q).range(0..=w));
                ui.label("r");
                let mut r = r_old;
                ui.add(egui::DragValue::new(&mut r).range(0..=h));
                if (q, r) != (region.centre.q, region.centre.r) {
                    let _ = state.update_region(&id, |reg| {
                        reg.centre = HexCoord { q, r };
                    });
                }
            },
        );

        // hex list summary + clear (schema: hexes)
        labeled(
            ui,
            "Painted hexes",
            "The map cells this region covers (schema: hexes). Paint them on the map or grow them below.",
            |ui| {
                if region.hexes.is_empty() {
                    ui_kit::placeholder(ui, "none yet — paint on the map or grow below");
                } else {
                    let preview: String = region
                        .hexes
                        .iter()
                        .take(20)
                        .map(|h| format!("({},{})", h.q, h.r))
                        .collect::<Vec<_>>()
                        .join("");
                    ui.label(RichText::new(preview).monospace().color(Color32::DARK_GRAY));
                    if region.hexes.len() > 20 {
                        ui.label(format!("(+{} more)", region.hexes.len() - 20));
                    }
                }
            },
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .button("🧹  Clear hexes")
                .on_hover_text("Empty this region's painted cells (keeps the region)")
                .clicked()
            {
                let _ = state.update_region(&id, |r| r.hexes.clear());
            }
            if ui
                .button("🖌  Paint on map  →")
                .on_hover_text("Switch to the map's region brush to paint this region's cells")
                .clicked()
            {
                state.map_tool = MapTool::RegionPaint;
                state.focus_entity(EntityRef::Tab(BuilderTab::Map));
            }
            if ui
                .add(egui::Button::new(
                    RichText::new("🗑  Delete region").color(palette::danger()),
                ))
                .on_hover_text("Remove this region from the sector")
                .clicked()
            {
                let _ = state.remove_region(&id);
            }
        });
    });
}

// ── §REG3 grow seeded region ────────────────────────────────────────────────

fn show_grow_seeded(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::section(ui, "Auto-grow a region blob", |ui| {
        ui_kit::placeholder(
            ui,
            "Pick a centre and a rough size, then Grow to flood-fill a blob of hexes (avoids other regions).",
        );
        ui.add_space(4.0);
        let w = state.sector.width.saturating_sub(1) as i32;
        let h = state.sector.height.saturating_sub(1) as i32;
        let seed_state = state
            .selected_region_id
            .clone()
            .unwrap_or_else(|| "new".into());

        labeled(
            ui,
            "Seed hex",
            "Where the blob starts growing from (schema: centre on the resulting region).",
            |ui| {
                ui.label("q");
                ui.add(egui::DragValue::new(&mut state.region_grow_q).range(0..=w));
                ui.label("r");
                ui.add(egui::DragValue::new(&mut state.region_grow_r).range(0..=h));
            },
        );
        labeled(
            ui,
            "Target size",
            "Roughly how many hexes to grow (schema: mean_size). Actual size varies with the layout.",
            |ui| {
                ui.add(egui::DragValue::new(&mut state.region_grow_size).range(1..=200));
            },
        );
        labeled(
            ui,
            "Phenomenon",
            "Which warp condition the grown region will be (schema: kind).",
            |ui| {
                ui_kit::combo("region_grow_kind", state.region_grow_kind.label()).show_ui(
                    ui,
                    |ui| {
                        for k in RegionConditionKind::ALL {
                            ui.selectable_value(&mut state.region_grow_kind, *k, k.label());
                        }
                    },
                );
            },
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mode_label = if state.selected_region_id.is_some() {
                "Grow will replace the selected region's hexes"
            } else {
                "Grow will create a new region"
            };
            ui.colored_label(Color32::DARK_GRAY, mode_label);
            if ui
                .button("🌱  Grow")
                .on_hover_text("Flood-fill a blob of hexes from the seed")
                .clicked()
            {
                let centre = HexCoord {
                    q: state.region_grow_q,
                    r: state.region_grow_r,
                };
                let existing: Vec<WarpRegion> = state
                    .sector
                    .regions
                    .iter()
                    .filter(|r| Some(r.id.as_str()) != state.selected_region_id.as_deref())
                    .cloned()
                    .collect();
                let hexes = seed_region(
                    state.sector.seed.as_ref(),
                    &format!("manual:{seed_state}:{}", state.region_grow_size),
                    centre,
                    state.region_grow_size as usize,
                    state.sector.width,
                    state.sector.height,
                    &existing,
                );
                if hexes.is_empty() {
                    state.modal = Some(crate::builder::state::ModalKind::Message(
                        "Grow returned no hexes (centre blocked or sector empty).".into(),
                    ));
                    return;
                }
                if let Some(id) = state.selected_region_id.clone() {
                    let kind = state.region_grow_kind;
                    let _ = state.update_region(&id, |r| {
                        r.kind = kind;
                        r.centre = centre;
                        r.hexes = hexes;
                    });
                } else {
                    let new_id = state.add_region("Grown", state.region_grow_kind, centre);
                    let _ = state.update_region(&new_id, |r| r.hexes = hexes);
                }
            }
        });
    });
}

// ── §REG4 live route effects ────────────────────────────────────────────────

fn show_route_effects(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::section(ui, "How regions change routes", |ui| {
        let regions: Vec<WarpRegion> = state.sector.regions.iter().cloned().collect();
        let systems = &state.sector.systems;
        let coord_by_id: BTreeMap<&str, HexCoord> =
            systems.iter().map(|s| (s.id.as_str(), s.coord)).collect();

        let mut affected = 0usize;
        let mut perilous = 0usize;
        let mut degrade = 0usize;
        let mut upgrade = 0usize;
        for route in state.sector.routes.iter() {
            let (Some(&a), Some(&b)) = (
                coord_by_id.get(route.from_system_id.as_str()),
                coord_by_id.get(route.to_system_id.as_str()),
            ) else {
                continue;
            };
            if let Some(cond) = dominant_route_condition(&regions, a, b) {
                affected += 1;
                match cond {
                    RegionConditionKind::WarpStorm => perilous += 1,
                    RegionConditionKind::Turbulence | RegionConditionKind::EmpyricBleed => {
                        degrade += 1
                    }
                    RegionConditionKind::CalmCorridor | RegionConditionKind::BeaconChain => {
                        upgrade += 1
                    }
                    _ => {}
                }
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} route(s) total", state.sector.routes.len()));
            ui.label(format!("· {affected} affected"));
            ui.colored_label(palette::danger(), format!("→ perilous: {perilous}"));
            ui.colored_label(palette::warning(), format!("↓ worse: {degrade}"));
            ui.colored_label(palette::success(), format!("↑ better: {upgrade}"));
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    affected > 0,
                    egui::Button::new("⚡  Apply effects to routes"),
                )
                .on_hover_text("Bake the region effects into the affected routes (undoable)")
                .clicked()
            {
                // §R4: build the mutated routes vector against owned clones (no
                // live `state.sector` borrow held across the dispatch), then route
                // the write through the command bus so the edit is undoable.
                // `ReplaceRoutes::apply` captures `before`, so we pass an empty Vec.
                let regions_clone = regions.clone();
                let systems_clone = state.sector.systems.clone();
                let max_route_distance = state.config.generation.routes.max_route_distance;
                let mut after = state.sector.routes.clone();
                apply_route_effects(
                    &regions_clone,
                    &systems_clone,
                    &mut after,
                    max_route_distance,
                );
                if let Err(e) = state.run(BuilderCommand::ReplaceRoutes {
                    before: Vec::new(),
                    after,
                }) {
                    state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                        "Apply route effects failed: {e}"
                    )));
                }
            }
            ui.colored_label(
                Color32::DARK_GRAY,
                "Affected routes get tagged with the region that changed them, so the effect is visible later.",
            );
        });
    });
}

// ── §REG2 paint hint ────────────────────────────────────────────────────────

fn show_paint_hint(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::section(ui, "Paint regions on the map", |ui| {
        ui.horizontal_wrapped(|ui| {
            let active = state.map_tool == MapTool::RegionPaint;
            if ui
                .selectable_label(active, "🖌  Region brush")
                .on_hover_text("Turn on the map's region brush and jump to the map")
                .clicked()
            {
                state.map_tool = MapTool::RegionPaint;
                state.focus_entity(EntityRef::Tab(BuilderTab::Map));
            }
            ui.colored_label(
                Color32::DARK_GRAY,
                "Left-click paints into the selected region, right-click erases. \
                 Painting onto another region flags an overlap warning.",
            );
        });
    });
}

// ── §REG7 ASCII glyph preview ───────────────────────────────────────────────

fn show_glyph_preview(ui: &mut Ui, state: &BuilderState) {
    ui_kit::section(ui, "Text map preview", |ui| {
        let w = state.sector.width as usize;
        let h = state.sector.height as usize;
        if w == 0 || h == 0 {
            ui_kit::placeholder(ui, "Nothing to preview — the sector has no size yet.");
            return;
        }
        let mut grid: Vec<Vec<char>> = vec![vec!['.'; w]; h];
        let mut systems: std::collections::BTreeSet<(i32, i32)> = std::collections::BTreeSet::new();
        for s in &state.sector.systems {
            systems.insert((s.coord.q, s.coord.r));
        }
        for region in state.sector.regions.iter() {
            let g = region.kind.glyph();
            for hex in &region.hexes {
                if hex.q < 0 || hex.r < 0 {
                    continue;
                }
                let (q, r) = (hex.q as usize, hex.r as usize);
                if q < w && r < h && !systems.contains(&(hex.q, hex.r)) {
                    grid[r][q] = g;
                }
            }
        }
        for (q, r) in systems {
            if (q as usize) < w && (r as usize) < h {
                grid[r as usize][q as usize] = '@';
            }
        }
        let mut text = String::with_capacity((w + 1) * h);
        for row in grid {
            for c in row {
                text.push(c);
                text.push(' ');
            }
            text.push('\n');
        }
        ui.add(
            egui::TextEdit::multiline(&mut text.as_str())
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .desired_rows(h.min(40))
                .interactive(false),
        );
        ui.colored_label(
            Color32::DARK_GRAY,
            "Legend: @ system  ~ storm  ^ turbulence  = calm corridor  # blackout  * anomaly  % necropolis  + beacon  ? bleed",
        );
    });
}

// ── §REG5 regions.toml editor ───────────────────────────────────────────────

fn show_regions_config_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::section(ui, "Region generation settings", |ui| {
        if state.data_catalogs.regions.is_none() {
            ui_kit::placeholder(
                ui,
                "No generation settings yet — create defaults to control how regions are auto-generated.",
            );
            ui.add_space(4.0);
            if ui
                .button("➕  Create defaults")
                .on_hover_text("Start a region settings file with sensible defaults")
                .clicked()
            {
                state.data_catalogs.regions = Some(RegionsConfig::default());
                if state.config.inputs.regions.is_none() {
                    state.config.inputs.regions = Some("data/routes/regions.toml".into());
                }
                state.dirty = true;
            }
            return;
        }
        ui_kit::placeholder(
            ui,
            "Controls how many regions the generator places and their phenomenon mix.",
        );
        ui.add_space(4.0);
        let mut cfg = state
            .data_catalogs
            .regions
            .as_ref()
            .expect("checked above")
            .clone();
        let mut changed = false;
        let mut save_clicked = false;
        labeled(
            ui,
            "Generate regions",
            "Whether the generator places warp regions at all (schema: enabled).",
            |ui| {
                changed |= ui.checkbox(&mut cfg.enabled, "").changed();
            },
        );
        labeled(
            ui,
            "How many",
            "Number of regions to place when generating (schema: count).",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.count).range(0..=2_000u32))
                    .changed();
            },
        );
        labeled(
            ui,
            "Typical size",
            "Average hex count per generated region (schema: mean_size).",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.mean_size).range(1..=2_000u32))
                    .changed();
            },
        );
        labeled(
            ui,
            "Affect routes",
            "Apply region effects to routes during generation (schema: apply_to_routes).",
            |ui| {
                changed |= ui.checkbox(&mut cfg.apply_to_routes, "").changed();
            },
        );

        ui.add_space(4.0);
        ui.label(RichText::new("Phenomenon mix").strong());
        ui_kit::placeholder(
            ui,
            "Which phenomena can appear, and how likely each is (higher weight = more common).",
        );
        let mut remove_idx: Option<usize> = None;
        for (i, entry) in cfg.conditions.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui_kit::combo(("region_cfg_kind", i), entry.kind.label()).show_ui(ui, |ui| {
                    for k in RegionConditionKind::ALL {
                        if ui.selectable_label(entry.kind == *k, k.label()).clicked() {
                            entry.kind = *k;
                            changed = true;
                        }
                    }
                });
                ui.label("weight")
                    .on_hover_text("Relative likelihood of this phenomenon (schema: weight).");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut entry.weight)
                            .range(0.0..=100.0)
                            .speed(0.1),
                    )
                    .changed();
                ui.label("label")
                    .on_hover_text("Optional custom name for this phenomenon (schema: label).");
                let label_key = egui::Id::new(("region_cfg_label_buf", i));
                let label_src = entry.label.clone().unwrap_or_default();
                let (label_buf, resp) =
                    crate::builder::panels::persistent_singleline(ui, label_key, &label_src);
                if resp.lost_focus() {
                    let new = if label_buf.is_empty() {
                        None
                    } else {
                        Some(label_buf)
                    };
                    if new != entry.label {
                        entry.label = new;
                        changed = true;
                    }
                    crate::builder::panels::persistent_text_clear(ui, label_key);
                }
                if ui
                    .button(RichText::new("×").color(palette::danger()))
                    .clicked()
                {
                    remove_idx = Some(i);
                }
            });
        }
        if let Some(i) = remove_idx {
            cfg.conditions.remove(i);
            changed = true;
        }
        ui.horizontal(|ui| {
            if ui
                .button("➕  Add phenomenon")
                .on_hover_text("Add another phenomenon to the generation mix")
                .clicked()
            {
                cfg.conditions.push(ConditionEntry {
                    kind: RegionConditionKind::Turbulence,
                    weight: 1.0,
                    label: None,
                });
                changed = true;
            }
            if ui
                .button("💾  Save settings")
                .on_hover_text("Write these region settings back to the project")
                .clicked()
            {
                save_clicked = true;
            }
        });
        if changed {
            state.data_catalogs.regions = Some(cfg);
            state.dirty = true;
            if let Some(rel) = state.config.inputs.regions.clone() {
                state.dirty_files.insert(rel);
            }
            state.mark_validation_dirty();
        }
        if save_clicked {
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Save regions.toml failed: {e}"
                )));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::HexCoord;

    #[test]
    fn next_region_id_increments_past_existing() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let _ = state.add_region(
            "a",
            RegionConditionKind::Turbulence,
            HexCoord { q: 0, r: 0 },
        );
        let _ = state.add_region("b", RegionConditionKind::WarpStorm, HexCoord { q: 1, r: 0 });
        assert_eq!(state.next_region_id(), "reg-0003");
    }

    #[test]
    fn paint_then_erase_round_trips_hex_list() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let id = state.add_region(
            "a",
            RegionConditionKind::Turbulence,
            HexCoord { q: 2, r: 2 },
        );
        state
            .paint_region_hex(&id, HexCoord { q: 3, r: 3 })
            .unwrap();
        assert!(state.sector.regions[0]
            .hexes
            .contains(&HexCoord { q: 3, r: 3 }));
        state
            .erase_region_hex(&id, HexCoord { q: 3, r: 3 })
            .unwrap();
        assert!(!state.sector.regions[0]
            .hexes
            .contains(&HexCoord { q: 3, r: 3 }));
    }

    #[test]
    fn overlap_paint_surfaces_region_hex_overlap_invariant() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let a = state.add_region(
            "a",
            RegionConditionKind::Turbulence,
            HexCoord { q: 1, r: 1 },
        );
        let b = state.add_region("b", RegionConditionKind::WarpStorm, HexCoord { q: 5, r: 5 });
        state.paint_region_hex(&b, HexCoord { q: 1, r: 1 }).unwrap();
        let _ = a;
        let codes: Vec<&str> = state
            .invariant_report
            .as_ref()
            .unwrap()
            .violations
            .iter()
            .map(|v| v.code.as_str())
            .collect();
        assert!(codes.contains(&"REGION_HEX_OVERLAP"));
    }

    #[test]
    fn seed_region_grows_in_bounds_and_avoids_existing() {
        let centre = HexCoord { q: 2, r: 2 };
        let hexes = seed_region("seed", "disc", centre, 5, 8, 8, &[]);
        assert!(!hexes.is_empty());
        assert!(hexes
            .iter()
            .all(|h| h.q >= 0 && h.q < 8 && h.r >= 0 && h.r < 8));
        let blocker = WarpRegion {
            id: "blk".into(),
            name: "blk".into(),
            kind: RegionConditionKind::Blackout,
            hexes: hexes.clone(),
            centre,
        };
        let grown = seed_region("seed", "disc2", centre, 5, 8, 8, &[blocker]);
        for h in &grown {
            assert!(!hexes.contains(h), "seed_region must avoid existing hexes");
        }
    }

    #[test]
    fn region_condition_glyphs_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for k in RegionConditionKind::ALL {
            assert!(seen.insert(k.glyph()), "duplicate glyph for {:?}", k);
        }
    }
}
