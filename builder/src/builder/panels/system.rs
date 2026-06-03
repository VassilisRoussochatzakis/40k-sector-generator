//! SYSTEM tab (§N1 / §N2) — Phase B §S2..§S6 inspector.
//!
//! Covers every `GeneratedSystem` field via a per-section inspector, the
//! §S3 pinned toggle (driven by [`BuilderState::pinned_systems`]), the §S4
//! bulk-ops block over [`BuilderState::selected_systems`], the §S5
//! single-system regenerate (`sectorforge::generate_system_standalone`), and
//! the §S6 coord-validity check on inline coord edits. Fields managed by
//! sibling panels (worlds §8, primary factions §10, control §11, orbital
//! assets §31, conflict §28, intel §29, archetype §30) are shown read-only
//! with deep-link buttons.

use std::collections::BTreeSet;
use std::sync::Arc;

use egui::{Color32, RichText, Ui};

use sectorforge::ids::SystemId;
use sectorforge::sector_model::{HexCoord, SystemKind, SystemState};
use sectorforge::system_map::{render_system, SystemRenderOptions};
use sectorforge_gui_core::system_view::{SystemClick, SystemLayout, SystemSelection, SystemView};
use sectorforge_gui_core::ui_kit;

use crate::builder::command::BuilderCommand;
use crate::builder::state::{BuilderTab, EntityRef, ModalKind, SystemBitmapPreview};
use crate::builder::BuilderState;

/// §CTX0 — scroll-anchor id used by [`show_star_section`] when
/// [`BuilderState::scroll_target`] points at the Star header. Mirrors the
/// literal passed to the inner `egui::Grid::new` so both sides stay in sync.
///
/// §CTX1 Phase 6 — `panels/system_map.rs` mirrors this constant so the
/// in-system right-click menu's `FOCUS STAR DETAILS` row arms the same anchor.
const SYS_STAR_GRID_ANCHOR: &str = "sys_star_grid";

/// Slider clamp for the SYSTEM-tab embedded `SystemView` size.
const SYSTEM_VIEW_SIDE_MIN: f32 = 400.0;
const SYSTEM_VIEW_SIDE_MAX: f32 = 2400.0;

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("System");
    ui.add_space(4.0);

    let count = state.sector.systems.len();
    if count == 0 {
        ui_kit::placeholder(
            ui,
            "No systems in this sector — use the MAP tab's ADD SYSTEM tool.",
        );
        return;
    }

    show_system_picker(ui, state);
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let selected = state.selected_system_id.clone();
            let Some(sys_id) = selected else {
                ui_kit::placeholder(ui, "Select a system from the picker or the MAP tab.");
                show_bulk_ops(ui, state);
                return;
            };

            let Some(sys_idx) = state.sector.systems.iter().position(|s| s.id == sys_id) else {
                state.selected_system_id = None;
                return;
            };

            show_header(ui, state, sys_idx);
            ui.separator();
            show_system_map_section(ui, state, sys_idx);
            ui.add_space(4.0);
            show_bitmap_preview_section(ui, state, sys_idx);
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            ui.columns(2, |cols| {
                // Left column: identity + read-only state.
                let left = &mut cols[0];
                show_identity_section(left, state, sys_idx);
                left.add_space(4.0);
                let star_resp = show_star_section(left, state, sys_idx);
                if state.scroll_target == Some(SYS_STAR_GRID_ANCHOR) {
                    star_resp
                        .header_response
                        .scroll_to_me(Some(egui::Align::TOP));
                    state.scroll_target = None;
                }
                left.add_space(4.0);
                show_tags_notes_section(left, state, sys_idx);
                left.add_space(4.0);
                show_worlds_link(left, state, sys_idx);
                left.add_space(4.0);
                show_routes_section(left, state, sys_idx);
                left.add_space(4.0);
                show_factions_section(left, state, sys_idx);
                left.add_space(4.0);
                show_control_section(left, state, sys_idx);

                // Right column: overlays, archetypes, sibling-panel sections.
                let right = &mut cols[1];
                show_overlays_section(right, state, sys_idx);
                right.add_space(4.0);
                show_archetype_section(right, state, sys_idx);
                right.add_space(4.0);
                show_archetype_auto_assign(right, state);
                right.add_space(4.0);
                show_archetype_rules(right, state);
                right.add_space(4.0);
                crate::builder::panels::orbital::show_orbital_section(right, state, sys_idx);
                right.add_space(4.0);
                crate::builder::panels::conflict::show_system_conflict_section(
                    right, state, sys_idx,
                );
                right.add_space(4.0);
                crate::builder::panels::intel::show_system_intel_section(right, state, sys_idx);
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            show_regen_section(ui, state, sys_idx);
            ui.add_space(8.0);
            show_bulk_ops(ui, state);
        });
}

// ── picker / header ─────────────────────────────────────────────────────────

fn show_system_picker(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        ui.label("system:");
        let current = state.selected_system_id.clone();
        let label = current
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "(none)".into());
        ui_kit::combo("system_picker", label).show_ui(ui, |ui| {
            for sys in &state.sector.systems {
                let sel = current.as_ref() == Some(&sys.id);
                let label = format!("{} — {}", sys.id, sys.name);
                if ui.selectable_label(sel, label).clicked() {
                    state.selected_system_id = Some(sys.id.clone());
                    state.selected_systems.clear();
                    state.selected_systems.insert(sys.id.clone());
                }
            }
        });
    });
}

fn show_header(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    let sys = &state.sector.systems[sys_idx];
    let id = sys.id.clone();
    let pinned = state.pinned_systems.contains(&id);
    ui.horizontal_wrapped(|ui| {
        ui.heading(sys.name.to_string());
        ui.label(
            RichText::new(sys.id.to_string())
                .color(Color32::GRAY)
                .monospace(),
        );
        if pinned {
            ui.colored_label(Color32::from_rgb(255, 160, 100), "PINNED");
        }
    });
}

// ── §CTX0 in-system map (Phase 0 of docs/CONTEXT_MENU.txt) ─────────────────

/// Embeds the shared [`SystemView`] widget under the SYSTEM tab so the in-system
/// map has a host before the context-menu work in Phase 6 lands. Click on a
/// planet → updates [`BuilderState::selected_world_id`]; click on the central
/// star → arms [`BuilderState::scroll_target`] so the Star section scrolls
/// into view on the same frame.
fn show_system_map_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_in_map", "In-system map", true, |ui| {
        let panel_width = ui.available_width();
        ui.horizontal(|ui| {
            ui.label("Layout:");
            let mut horiz = matches!(state.system_layout, SystemLayout::Horizontal);
            if ui
                .selectable_label(horiz, "Horizontal")
                .on_hover_text("Star left, planets arrayed right in orbit order")
                .clicked()
            {
                horiz = true;
            }
            if ui
                .selectable_label(!horiz, "Orbital")
                .on_hover_text("Concentric orbit rings")
                .clicked()
            {
                horiz = false;
            }
            state.system_layout = if horiz {
                SystemLayout::Horizontal
            } else {
                SystemLayout::Orbital
            };
            ui.separator();
            ui.label("Size:");
            ui.add(
                egui::Slider::new(
                    &mut state.system_view_side,
                    SYSTEM_VIEW_SIDE_MIN..=SYSTEM_VIEW_SIDE_MAX,
                )
                .show_value(false),
            );
            if ui
                .button("Fit width")
                .on_hover_text("Resize preview to fill the panel width")
                .clicked()
            {
                state.system_view_side =
                    panel_width.clamp(SYSTEM_VIEW_SIDE_MIN, SYSTEM_VIEW_SIDE_MAX);
            }
        });
        let layout = state.system_layout;
        let side = state
            .system_view_side
            .clamp(SYSTEM_VIEW_SIDE_MIN, SYSTEM_VIEW_SIDE_MAX);
        // 3:1 aspect — preview spans the panel width but only a third as
        // tall so the in-system map doesn't dominate the SYSTEM tab.
        let height = (side / 3.0).max(SYSTEM_VIEW_SIDE_MIN / 3.0);
        let sys = &state.sector.systems[sys_idx];
        // §CTX1 Phase 7 — while the in-system right-click menu is open,
        // override the `selected` rendering so the SystemView highlights
        // the menu's contextual entity (star / world). Reuses the existing
        // SELECTION ring so we don't need to allocate a separate painter
        // overlay. Falls back to `selected_world_id` when no menu is open.
        let selected = crate::builder::panels::system_map::menu_selection_override(state, sys_idx)
            .unwrap_or_else(|| match state.selected_world_id.as_ref() {
                Some(wid) => sys
                    .worlds
                    .iter()
                    .find(|w| &w.id == wid)
                    .map(|w| SystemSelection::World(w.index))
                    .unwrap_or(SystemSelection::None),
                None => SystemSelection::None,
            });
        let (resp, click) = SystemView {
            system: sys,
            selected,
            side,
            height,
            layout,
        }
        .show(ui);
        if let Some(c) = click {
            handle_system_view_click(state, sys_idx, c);
        }
        // §CTX1 Phase 6 — secondary-click → open in-system menu. Resolver
        // + render live in `panels/system_map.rs`.
        if resp.secondary_clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                crate::builder::panels::system_map::arm_system_context_menu(
                    state,
                    sys_idx,
                    side,
                    height,
                    pos,
                    resp.rect.min,
                );
            }
        }
    });
    crate::builder::panels::system_map::show_system_context_menu(ui.ctx(), state);
    crate::builder::panels::system_map::show_world_rename_dialog(ui.ctx(), state);
}

/// Side-effect-free routing of a [`SystemClick`] to the corresponding builder
/// state mutation. Extracted so unit tests can exercise the wiring without
/// spinning up an egui context.
fn handle_system_view_click(state: &mut BuilderState, sys_idx: usize, click: SystemClick) {
    match click {
        SystemClick::Star => {
            state.scroll_target = Some(SYS_STAR_GRID_ANCHOR);
        }
        SystemClick::World(idx) => {
            let sys = &state.sector.systems[sys_idx];
            if let Some(w) = sys.worlds.iter().find(|w| w.index == idx) {
                state.selected_world_id = Some(w.id.clone());
            }
        }
        _ => {}
    }
}

// ── identity (S2 + S6) ──────────────────────────────────────────────────────

/// §35 T5 — per-system bitmap preview. Renders the focused system through the
/// same `system_map` PNG renderer the exporter uses (honouring the project's
/// §T1/§T2 map theme and the §EX3 `faction_fill` flag), uploads it once as an
/// egui texture, and caches it until the system / theme / faction_fill changes.
fn show_bitmap_preview_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    /// Preview render resolution. Scale 1 ≈ 1080×720 px — large enough to read,
    /// cheap enough to re-upload on demand. The exporter's `system_scale`
    /// controls the on-disk resolution separately.
    const PREVIEW_SCALE: u32 = 1;

    ui_kit::collapsing_section(
        ui,
        "sys_bitmap_preview",
        "Bitmap preview (§T5)",
        false,
        |ui| {
            let faction_fill = state.config.outputs.bitmap.faction_fill;
            let theme =
                sectorforge::map_theme::resolve_map_theme(&state.config.outputs.bitmap.theme)
                    .unwrap_or_else(|_| sectorforge::map_theme::MapTheme::gm_dark());
            let theme_name = theme.name.clone();

            let key = {
                let sys = &state.sector.systems[sys_idx];
                format!(
                    "{}|ff{}|{}|w{}|s{}",
                    sys.id,
                    u8::from(faction_fill),
                    theme_name,
                    sys.worlds.len(),
                    PREVIEW_SCALE
                )
            };

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("PNG render via the per-system exporter")
                        .small()
                        .color(Color32::GRAY),
                );
                if ui.button("Refresh").clicked() {
                    state.system_bitmap_preview = None;
                }
            });

            let stale = state
                .system_bitmap_preview
                .as_ref()
                .is_none_or(|p| p.key != key);
            if stale {
                let opts = SystemRenderOptions {
                    faction_fill,
                    theme,
                };
                let img = {
                    let sys = &state.sector.systems[sys_idx];
                    render_system(sys, &state.sector.factions, PREVIEW_SCALE, opts)
                };
                let size = [img.width() as usize, img.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                let texture = ui.ctx().load_texture(
                    "sys_bitmap_preview",
                    color,
                    egui::TextureOptions::LINEAR,
                );
                state.system_bitmap_preview = Some(SystemBitmapPreview { key, texture, size });
            }

            if let Some(p) = &state.system_bitmap_preview {
                let avail = ui.available_width().min(p.size[0] as f32);
                let sized = egui::load::SizedTexture::new(
                    p.texture.id(),
                    egui::vec2(p.size[0] as f32, p.size[1] as f32),
                );
                ui.add(egui::Image::new(sized).max_width(avail));
                ui.label(
                    RichText::new(format!(
                        "{}×{} px · theme {theme_name} · faction_fill {faction_fill}",
                        p.size[0], p.size[1]
                    ))
                    .small()
                    .color(Color32::DARK_GRAY),
                );
            }
        },
    );
}

fn show_identity_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_identity", "Identity", true, |ui| {
        let sys = &state.sector.systems[sys_idx];
        let id = sys.id.clone();
        let coord = sys.coord;
        let kind = sys.kind;
        let name_buf_key = egui::Id::new(("sys_identity_name_buf", id.as_str()));
        let kind_choice_key = egui::Id::new(("sys_identity_kind_choice", id.as_str()));
        let coord_q_key = egui::Id::new(("sys_identity_coord_q", id.as_str()));
        let coord_r_key = egui::Id::new(("sys_identity_coord_r", id.as_str()));
        let source_name = sys.name.to_string();
        // Persist q/r across frames so DragValue edits survive until the
        // user clicks "Apply coord". Without this the locals reseed from
        // `coord` next frame and the in-flight value is lost.
        let mut q = ui
            .data_mut(|d| d.get_temp::<i32>(coord_q_key))
            .unwrap_or(coord.q);
        let mut r = ui
            .data_mut(|d| d.get_temp::<i32>(coord_r_key))
            .unwrap_or(coord.r);
        // Persist kind_choice across frames so the "Apply kind" button
        // remains visible after the user picks a new option in the combo.
        // Without this the local reseeds from `kind` next frame and the
        // pending selection is lost before the user can confirm it.
        let mut kind_choice = ui
            .data_mut(|d| d.get_temp::<SystemKind>(kind_choice_key))
            .unwrap_or(kind);

        let mut name_buf = String::new();
        let mut name_changed = false;
        egui::Grid::new("sys_identity_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("id");
                ui.monospace(id.to_string());
                ui.end_row();
                ui.label("name");
                let (buf, resp) =
                    crate::builder::panels::persistent_singleline(ui, name_buf_key, &source_name);
                name_buf = buf;
                name_changed = resp.lost_focus();
                ui.end_row();
                ui.label("coord");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut q)
                            .range(0..=state.sector.width as i32 - 1)
                            .prefix("q"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut r)
                            .range(0..=state.sector.height as i32 - 1)
                            .prefix("r"),
                    );
                });
                ui.data_mut(|d| {
                    d.insert_temp(coord_q_key, q);
                    d.insert_temp(coord_r_key, r);
                });
                ui.end_row();
                ui.label("kind");
                ui_kit::combo("sys_kind", format!("{}", kind_choice)).show_ui(ui, |ui| {
                    for k in [
                        SystemKind::Star,
                        SystemKind::SpecialLocation,
                        SystemKind::BlackHole,
                        SystemKind::WarpAnomaly,
                        SystemKind::SpaceStation,
                    ] {
                        ui.selectable_value(&mut kind_choice, k, format!("{}", k));
                    }
                });
                ui.data_mut(|d| d.insert_temp(kind_choice_key, kind_choice));
                ui.end_row();
                ui.label("pinned");
                let mut pinned = state.pinned_systems.contains(&id);
                if ui.checkbox(&mut pinned, "(pin from generator)").changed() {
                    if pinned {
                        state.pinned_systems.insert(id.clone());
                    } else {
                        state.pinned_systems.remove(&id);
                    }
                }
                ui.end_row();
            });

        ui.horizontal(|ui| {
            if (ui.button("Apply name").clicked() || name_changed)
                && name_buf != *state.sector.systems[sys_idx].name
            {
                let from = state.sector.systems[sys_idx].name.to_string();
                let cmd = BuilderCommand::RenameSystem {
                    id: id.clone(),
                    from,
                    to: name_buf.clone(),
                };
                if let Err(e) = state.run(cmd) {
                    state.modal = Some(ModalKind::Message(format!("Rename failed: {e}")));
                } else {
                    crate::builder::panels::persistent_text_clear(ui, name_buf_key);
                }
            }
            if ui.button("Apply coord").clicked() {
                let new_coord = HexCoord { q, r };
                if new_coord != coord {
                    apply_coord_move(state, id.clone(), coord, new_coord);
                }
                ui.data_mut(|d| {
                    d.remove::<i32>(coord_q_key);
                    d.remove::<i32>(coord_r_key);
                });
            }
            if kind_choice != kind && ui.button("Apply kind").clicked() {
                // §R4: route the kind change through EditSystem so undo/redo
                // and the validation pump pick it up (was a direct field
                // write). `worlds` rides through the system clone unchanged.
                let sys_id = state.sector.systems[sys_idx].id.clone();
                let mut draft = state.sector.systems[sys_idx].clone();
                draft.kind = kind_choice;
                let cmd = BuilderCommand::EditSystem {
                    system: sys_id,
                    before: None,
                    after: Box::new(draft),
                };
                if let Err(e) = state.run(cmd) {
                    state.modal = Some(ModalKind::Message(format!("System edit failed: {e}")));
                } else {
                    ui.data_mut(|d| d.remove::<SystemKind>(kind_choice_key));
                }
            }
        });
    });
}

fn apply_coord_move(state: &mut BuilderState, id: SystemId, from: HexCoord, to: HexCoord) {
    if to.q < 0
        || to.r < 0
        || (to.q as u32) >= state.sector.width
        || (to.r as u32) >= state.sector.height
    {
        state.modal = Some(ModalKind::Message(format!(
            "Coord ({},{}) out of bounds {}x{}.",
            to.q, to.r, state.sector.width, state.sector.height
        )));
        return;
    }
    let occupant = state
        .sector
        .systems
        .iter()
        .find(|s| s.coord == to && s.id != id)
        .map(|s| s.id.clone());
    if let Some(occupant) = occupant {
        state.pending_collision = Some(crate::builder::state::PendingCollision {
            dragging: id,
            target: to,
            occupant,
        });
        return;
    }
    let cmd = BuilderCommand::MoveSystem { id, from, to };
    if let Err(e) = state.run(cmd) {
        state.modal = Some(ModalKind::Message(format!("Move failed: {e}")));
    }
}

// ── star ────────────────────────────────────────────────────────────────────

fn show_star_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
) -> egui::CollapsingResponse<()> {
    // §UO P3a: framed to match `ui_kit::collapsing_section`, but kept as a raw
    // `Frame::group` + `CollapsingHeader` because the caller consumes
    // `header_response.scroll_to_me` (§S star-grid anchor) — which the helper's
    // `Option<R>` return does not expose. Margins mirror the helper exactly.
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(8.0))
        .rounding(egui::Rounding::same(6.0))
        .show(ui, |ui| {
            egui::CollapsingHeader::new("Star")
                .default_open(false)
                .show(ui, |ui| {
                    let sys_id_key = state.sector.systems[sys_idx].id.as_str().to_string();
                    let mut has_star = state.sector.systems[sys_idx].star.is_some();
                    let mut toggle_star = false;
                    if ui.checkbox(&mut has_star, "present").changed() {
                        toggle_star = true;
                    }
                    let mut star_buf = state.sector.systems[sys_idx].star.clone();
                    let mut field_changed = false;
                    let (code_key, name_key, spectral_key) = (
                        egui::Id::new(("sys_star_code_buf", sys_id_key.as_str())),
                        egui::Id::new(("sys_star_name_buf", sys_id_key.as_str())),
                        egui::Id::new(("sys_star_spectral_buf", sys_id_key.as_str())),
                    );
                    let mut new_code = String::new();
                    let mut new_name = String::new();
                    let mut new_spectral = String::new();
                    if let Some(star) = star_buf.as_mut() {
                        let code_src = star.colour_code.to_string();
                        let name_src = star.colour_name.to_string();
                        let spectral_src = star
                            .spectral_type
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_default();
                        egui::Grid::new("sys_star_grid")
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("colour_code");
                                let (buf, resp) = crate::builder::panels::persistent_singleline(
                                    ui, code_key, &code_src,
                                );
                                new_code = buf;
                                field_changed |= resp.lost_focus();
                                ui.end_row();
                                ui.label("colour_name");
                                let (buf, resp) = crate::builder::panels::persistent_singleline(
                                    ui, name_key, &name_src,
                                );
                                new_name = buf;
                                field_changed |= resp.lost_focus();
                                ui.end_row();
                                ui.label("spectral_type");
                                let (buf, resp) = crate::builder::panels::persistent_singleline(
                                    ui,
                                    spectral_key,
                                    &spectral_src,
                                );
                                new_spectral = buf;
                                field_changed |= resp.lost_focus();
                                ui.end_row();
                            });
                        star.colour_code = Arc::from(new_code.as_str());
                        star.colour_name = Arc::from(new_name.as_str());
                        star.spectral_type = if new_spectral.trim().is_empty() {
                            None
                        } else {
                            Some(Arc::from(new_spectral.as_str()))
                        };
                    }

                    // §R4: both the present-toggle and the colour/spectral field edits
                    // now funnel through SetStar (was a direct `sector_mut().star`
                    // write). `before: None` lets `apply` snapshot the prior star so
                    // revert is exact. Mirrors the SetStar call shape used by the
                    // in-system right-click menu in `panels/system_map.rs`.
                    let current_star = state.sector.systems[sys_idx].star.clone();
                    if toggle_star {
                        let after = if has_star && current_star.is_none() {
                            Some(sectorforge::sector_model::GeneratedStar {
                                colour_code: Arc::from("G"),
                                colour_name: Arc::from("Yellow"),
                                spectral_type: None,
                                source_row_index: None,
                            })
                        } else if !has_star {
                            None
                        } else {
                            current_star.clone()
                        };
                        // Only the present/absent toggle is meaningful here; the no-op
                        // re-check (star already present, still present) leaves `after`
                        // == `current_star` and must not push a command. `GeneratedStar`
                        // has no `PartialEq`, so compare presence rather than value.
                        if after.is_some() != current_star.is_some() {
                            let cmd = BuilderCommand::SetStar {
                                system: state.sector.systems[sys_idx].id.clone(),
                                before: None,
                                after,
                            };
                            if let Err(e) = state.run(cmd) {
                                state.modal =
                                    Some(ModalKind::Message(format!("Star update failed: {e}")));
                            }
                        }
                    } else if field_changed {
                        let cmd = BuilderCommand::SetStar {
                            system: state.sector.systems[sys_idx].id.clone(),
                            before: None,
                            after: star_buf,
                        };
                        if let Err(e) = state.run(cmd) {
                            state.modal =
                                Some(ModalKind::Message(format!("Star update failed: {e}")));
                        } else {
                            crate::builder::panels::persistent_text_clear(ui, code_key);
                            crate::builder::panels::persistent_text_clear(ui, name_key);
                            crate::builder::panels::persistent_text_clear(ui, spectral_key);
                        }
                    }
                })
        })
        .inner
}

// ── tags + notes ────────────────────────────────────────────────────────────

fn show_tags_notes_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_tags_notes", "Tags + Notes", false, |ui| {
        let sys_id_key = state.sector.systems[sys_idx].id.as_str().to_string();
        let tags_key = egui::Id::new(("sys_tags_buf", sys_id_key.as_str()));
        let notes_key = egui::Id::new(("sys_notes_buf", sys_id_key.as_str()));
        let tags_src = state.sector.systems[sys_idx]
            .tags
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let notes_src = state.sector.systems[sys_idx]
            .notes
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        ui.label("tags (comma-separated)");
        let (tags_buf, tags_resp) =
            crate::builder::panels::persistent_singleline(ui, tags_key, &tags_src);
        let tags_changed = tags_resp.lost_focus();
        ui.label("notes (one per line)");
        let (notes_buf, notes_resp) =
            crate::builder::panels::persistent_multiline(ui, notes_key, &notes_src);
        let notes_changed = notes_resp.lost_focus();
        if tags_changed {
            // §R4: tags edit rides an EditSystem clone (was a direct
            // `systems[i].tags` write) so it lands on the undo log.
            let sys_id = state.sector.systems[sys_idx].id.clone();
            let mut draft = state.sector.systems[sys_idx].clone();
            draft.tags = tags_buf
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(Arc::from)
                .collect();
            let cmd = BuilderCommand::EditSystem {
                system: sys_id,
                before: None,
                after: Box::new(draft),
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("System edit failed: {e}")));
            } else {
                crate::builder::panels::persistent_text_clear(ui, tags_key);
            }
        }
        if notes_changed {
            // §R4: notes edit rides an EditSystem clone (was a direct
            // `systems[i].notes` write) so it lands on the undo log.
            let sys_id = state.sector.systems[sys_idx].id.clone();
            let mut draft = state.sector.systems[sys_idx].clone();
            draft.notes = notes_buf
                .lines()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(Arc::from)
                .collect();
            let cmd = BuilderCommand::EditSystem {
                system: sys_id,
                before: None,
                after: Box::new(draft),
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("System edit failed: {e}")));
            } else {
                crate::builder::panels::persistent_text_clear(ui, notes_key);
            }
        }
    });
}

// ── deep-links ──────────────────────────────────────────────────────────────

fn show_worlds_link(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_worlds", "Worlds", false, |ui| {
        let (sys_id, sys_name, world_ids, world_count, next_orbit, next_index) = {
            let sys = &state.sector.systems[sys_idx];
            let ids: Vec<_> = sys
                .worlds
                .iter()
                .map(|w| (w.id.clone(), w.name.to_string()))
                .collect();
            let max_orbit = sys.worlds.iter().map(|w| w.orbit).max().unwrap_or(0);
            let max_index = sys.worlds.iter().map(|w| w.index).max().unwrap_or(0);
            (
                sys.id.clone(),
                sys.name.to_string(),
                ids,
                sys.worlds.len(),
                max_orbit.saturating_add(1),
                max_index + 1,
            )
        };
        ui.horizontal(|ui| {
            ui.label(format!("{world_count} world(s)"));
            if ui
                .button("+ Add world")
                .on_hover_text("Append a blank world to this system")
                .clicked()
            {
                let name = format!(
                    "{sys_name} {}",
                    sectorforge::names::roman_numeral(next_orbit as usize)
                );
                let cmd = BuilderCommand::AddWorld {
                    system: sys_id.clone(),
                    name,
                    result_id: None,
                };
                match state.run(cmd) {
                    Err(e) => {
                        state.modal = Some(ModalKind::Message(format!("Add world failed: {e}")));
                    }
                    Ok(()) => {
                        // §R4: pin the new world's orbit through SetWorldOrbit
                        // (was a direct `w.orbit` write). The freshly added
                        // world is the one carrying `next_index`. `before: 0`
                        // per the command convention — SetWorldOrbit::apply
                        // re-captures the world's real prior orbit, so revert
                        // is exact regardless of the placeholder.
                        let new_world = state
                            .sector
                            .systems
                            .iter()
                            .find(|s| s.id == sys_id)
                            .and_then(|s| s.worlds.iter().find(|w| w.index == next_index))
                            .map(|w| w.id.clone());
                        if let Some(world) = new_world {
                            let cmd = BuilderCommand::SetWorldOrbit {
                                world,
                                before: 0,
                                after: next_orbit,
                            };
                            if let Err(e) = state.run(cmd) {
                                state.modal =
                                    Some(ModalKind::Message(format!("Set orbit failed: {e}")));
                            }
                        }
                    }
                }
            }
        });
        for (wid, name) in world_ids {
            ui.horizontal(|ui| {
                let clicked = sectorforge_gui_core::entity_link(ui, name, true).clicked();
                ui.label(
                    RichText::new(wid.to_string())
                        .color(Color32::GRAY)
                        .monospace()
                        .small(),
                );
                if clicked {
                    state.focus_entity(EntityRef::World {
                        system: sys_id.clone(),
                        world: wid,
                    });
                }
            });
        }
    });
}

fn show_routes_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_routes", "Routes (read-only here)", false, |ui| {
        let id = state.sector.systems[sys_idx].id.clone();
        let touching: Vec<_> = state
            .sector
            .routes
            .iter()
            .filter(|r| r.from_system_id == id || r.to_system_id == id)
            .map(|r| {
                (
                    r.id.clone(),
                    r.from_system_id.clone(),
                    r.to_system_id.clone(),
                    r.distance,
                )
            })
            .collect();
        ui.label(format!("{} route(s) touching", touching.len()));
        for (rid, from, to, dist) in touching {
            if sectorforge_gui_core::entity_link(
                ui,
                format!("{rid}  {from} → {to}  d={dist}"),
                true,
            )
            .clicked()
            {
                state.focus_entity(EntityRef::Route(rid));
            }
        }
    });
}

fn show_factions_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_factions", "Primary factions", false, |ui| {
        let primary: Vec<_> = state.sector.systems[sys_idx].primary_factions.to_vec();
        for fid in &primary {
            if sectorforge_gui_core::entity_link(ui, fid.to_string(), true).clicked() {
                state.focus_entity(EntityRef::Faction(fid.clone()));
            }
        }
        if primary.is_empty() {
            ui.colored_label(Color32::GRAY, "no primary factions");
        }
    });
}

fn show_control_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_control", "Control", false, |ui| {
        let id = state.sector.systems[sys_idx].id.clone();
        let mut current = state.sector.systems[sys_idx].control.state;
        let summary = state.sector.systems[sys_idx].control.clone();
        ui.label("control.state");
        ui_kit::combo(
            "sys_control_state",
            match current {
                None => "(none)".to_string(),
                Some(s) => format!("{s}"),
            },
        )
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut current, None, "(none)");
            for s in [
                SystemState::Pacified,
                SystemState::Fragmented,
                SystemState::Blockaded,
                SystemState::Warzone,
                SystemState::Infiltrated,
                SystemState::Quarantined,
                SystemState::Uncharted,
            ] {
                ui.selectable_value(&mut current, Some(s), format!("{s}"));
            }
        });
        if current != state.sector.systems[sys_idx].control.state {
            // §R4: route the control-state flip through EditSystem so it lands
            // on the undo/redo log (was a direct `set_system_control_state` over
            // `sector` that bypassed the command bus). EditSystem explicitly
            // covers the control summary; the setter is a plain field write with
            // no cascade, so the clone-mutate-dispatch shape is exact.
            let mut draft = state.sector.systems[sys_idx].clone();
            draft.control.state = current;
            let cmd = BuilderCommand::EditSystem {
                system: id,
                before: None,
                after: Box::new(draft),
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Control update failed: {e}")));
            }
        }
        ui.label(format!("dominant: {:?}", summary.dominant));
        ui.label(format!("sovereign: {:?}", summary.sovereign));
        ui.label(format!(
            "orbital_controller: {:?}",
            summary.orbital_controller
        ));
        ui.label(format!("economic_hegemon: {:?}", summary.economic_hegemon));
        ui.label(format!("hidden_master: {:?}", summary.hidden_master));
    });
}

fn show_overlays_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(
        ui,
        "sys_overlays",
        "Overlays (managed elsewhere)",
        false,
        |ui| {
            let sys = &state.sector.systems[sys_idx];
            ui.label(format!(
                "orbital_assets: {} (edit below)",
                sys.orbital_assets.len()
            ));
            ui.label(format!(
                "blockade present: {} (edit below)",
                !sectorforge::orbital_assets::BlockadeReport::is_default(&sys.blockade)
            ));
            ui.label(format!(
                "conflict default: {}",
                sectorforge::conflict::ConflictState::is_default(&sys.conflict)
            ));
            ui.label(format!(
                "intel observers: {} (empty? {})",
                sys.intel.by_observer.len(),
                sectorforge::intel::SystemIntel::is_empty(&sys.intel)
            ));
            ui.label(format!(
                "archetype default: {} (see Archetypes section)",
                sectorforge::archetypes::ArchetypeState::is_default(&sys.archetype)
            ));
            ui.horizontal(|ui| {
                if ui.button("Open REGIONS").clicked() {
                    state.focus_entity(EntityRef::Tab(BuilderTab::Regions));
                }
            });
        },
    );
}

// ── AR1 / AR2 / AR3 — Archetypes (§30) ─────────────────────────────────────

fn show_archetype_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    use sectorforge::archetypes::{
        ArchetypeState, GscStage, NecronPhase, TauSphereBand, TyranidStage,
    };

    let sys_id = state.sector.systems[sys_idx].id.clone();
    let mut working = state.sector.systems[sys_idx].archetype.clone();
    let original = working.clone();

    ui_kit::collapsing_section(ui, "sys_archetypes", "Archetypes", false, |ui| {
        ui.colored_label(
            Color32::GRAY,
            "per-axis progression markers. flavour notes live in the Tags / Notes section.",
        );
        ui.add_space(4.0);

        egui::Grid::new("archetype_axes")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("imperial co-sovereigns");
                ui.vertical(|ui| {
                    let mut remove_at: Option<usize> = None;
                    for (i, fid) in working.imperial_co_sovereigns.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.monospace(fid.to_string());
                            if ui.small_button("×").clicked() {
                                remove_at = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove_at {
                        working.imperial_co_sovereigns.remove(i);
                    }
                    ui.horizontal(|ui| {
                        let mut to_add: Option<sectorforge::ids::FactionId> = None;
                        ui_kit::combo("arch_imp_add", "+ add").show_ui(ui, |ui| {
                            for f in &state.sector.factions {
                                if working.imperial_co_sovereigns.contains(&f.id) {
                                    continue;
                                }
                                if ui.button(format!("{} ({})", f.id, f.name)).clicked() {
                                    to_add = Some(f.id.clone());
                                }
                            }
                        });
                        if let Some(fid) = to_add {
                            working.imperial_co_sovereigns.push(fid);
                        }
                    });
                });
                ui.end_row();

                ui.label("necron phase");
                ui_kit::combo("arch_necron", format!("{}", working.necron_phase)).show_ui(
                    ui,
                    |ui| {
                        for v in [
                            NecronPhase::None,
                            NecronPhase::Dormant,
                            NecronPhase::Awakening,
                            NecronPhase::Awake,
                        ] {
                            ui.selectable_value(&mut working.necron_phase, v, format!("{v}"));
                        }
                    },
                );
                ui.end_row();

                ui.label("tyranid stage");
                ui_kit::combo("arch_tyranid", format!("{}", working.tyranid_stage)).show_ui(
                    ui,
                    |ui| {
                        for v in [
                            TyranidStage::None,
                            TyranidStage::Inhabited,
                            TyranidStage::Besieged,
                            TyranidStage::Consumed,
                        ] {
                            ui.selectable_value(&mut working.tyranid_stage, v, format!("{v}"));
                        }
                    },
                );
                ui.end_row();

                ui.label("ork waaagh!");
                ui.add(egui::Slider::new(&mut working.ork_waaagh, 0..=100).text("/100"));
                ui.end_row();

                ui.label("genestealer stage");
                ui_kit::combo("arch_gsc", format!("{}", working.gsc_stage)).show_ui(ui, |ui| {
                    for v in [
                        GscStage::None,
                        GscStage::Rumor,
                        GscStage::HiddenCell,
                        GscStage::DistrictControl,
                        GscStage::ParallelGovernment,
                        GscStage::Uprising,
                        GscStage::PlanetarySeizure,
                    ] {
                        ui.selectable_value(&mut working.gsc_stage, v, format!("{v}"));
                    }
                });
                ui.end_row();

                ui.label("tau sphere");
                ui_kit::combo("arch_tau", format!("{}", working.tau_sphere)).show_ui(ui, |ui| {
                    for v in [
                        TauSphereBand::None,
                        TauSphereBand::Contact,
                        TauSphereBand::Fringe,
                        TauSphereBand::Client,
                        TauSphereBand::Core,
                    ] {
                        ui.selectable_value(&mut working.tau_sphere, v, format!("{v}"));
                    }
                });
                ui.end_row();

                ui.label("aeldari activity");
                ui.add(egui::Slider::new(&mut working.aeldari_activity, 0..=100).text("/100"));
                ui.end_row();

                ui.label("chaos corruption");
                ui.add(egui::Slider::new(&mut working.chaos_corruption, 0..=100).text("/100"));
                ui.end_row();

                ui.label("daemon manifestation");
                ui.add(egui::Slider::new(&mut working.daemon_manifestation, 0..=100).text("/100"));
                ui.end_row();
            });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Reset to default").clicked() {
                working = ArchetypeState::default();
            }
            if ui
                .button("Auto-assign from sector data (this system only)")
                .on_hover_text(
                    "Runs the §AR2 derivation over the full sector and keeps only \
                         this system's freshly derived archetype.",
                )
                .clicked()
            {
                let mut scratch = state.sector.clone();
                sectorforge::archetypes::apply_all(&mut scratch);
                if let Some(s) = scratch.systems.iter().find(|s| s.id == sys_id) {
                    working = s.archetype.clone();
                    state.archetype_flags.mask(&mut working);
                }
            }
        });
    });

    if working != original {
        let cmd = BuilderCommand::SetArchetype {
            system: sys_id,
            before: None,
            after: working,
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("Archetype update failed: {e}")));
        }
    }
}

fn show_archetype_auto_assign(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "sys_archetype_auto",
        "Auto-assign archetypes (sector-wide)",
        false,
        |ui| {
            ui.colored_label(
                Color32::GRAY,
                "runs `sectorforge::archetypes::apply_all` over the whole sector, \
                 masked by the §AR3 enable flags below. Undoable.",
            );
            if ui.button("Run apply_all now").clicked() {
                let flags = state.archetype_flags;
                let cmd = BuilderCommand::AutoAssignArchetypes {
                    flags,
                    before: Vec::new(),
                };
                if let Err(e) = state.run(cmd) {
                    state.modal = Some(ModalKind::Message(format!("Auto-assign failed: {e}")));
                }
            }
        },
    );
}

fn show_archetype_rules(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "sys_archetype_rules",
        "Archetype rules (builder-only defaults)",
        false,
        |ui| {
            ui.colored_label(
                Color32::GRAY,
                "`src/archetypes.rs` ships no TOML config layer, so these flags \
                 live on `BuilderState` only and are not serialised into \
                 `sector.json`. Disabled axes are reset to defaults after §AR2.",
            );
            let flags = &mut state.archetype_flags;
            ui.checkbox(&mut flags.imperial, "imperial governance stack");
            ui.checkbox(&mut flags.necron, "necron phase");
            ui.checkbox(&mut flags.tyranid, "tyranid front");
            ui.checkbox(&mut flags.ork, "ork waaagh!");
            ui.checkbox(&mut flags.gsc, "genestealer stages");
            ui.checkbox(&mut flags.tau, "tau sphere");
            ui.checkbox(&mut flags.aeldari, "aeldari intermittent");
            ui.checkbox(&mut flags.chaos, "chaos corruption + daemon");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Enable all").clicked() {
                    *flags = crate::builder::command::ArchetypeApplyFlags::default();
                }
                if ui.button("Disable all").clicked() {
                    *flags = crate::builder::command::ArchetypeApplyFlags {
                        imperial: false,
                        necron: false,
                        tyranid: false,
                        ork: false,
                        gsc: false,
                        tau: false,
                        aeldari: false,
                        chaos: false,
                    };
                }
            });
        },
    );
}

// ── S5 regen ────────────────────────────────────────────────────────────────

fn show_regen_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_regen", "Generate one system here", false, |ui| {
        let sys = &state.sector.systems[sys_idx];
        let original_coord = sys.coord;
        let sys_id = sys.id.clone();
        let id = sys.id.clone();
        let regen_q_key = egui::Id::new(("sys_regen_coord_q", id.as_str()));
        let regen_r_key = egui::Id::new(("sys_regen_coord_r", id.as_str()));
        let regen_index_key = egui::Id::new(("sys_regen_index", id.as_str()));
        // Persist q/r/index across frames so DragValue edits survive until
        // the user clicks a Regenerate button.
        let mut q = ui
            .data_mut(|d| d.get_temp::<i32>(regen_q_key))
            .unwrap_or(sys.coord.q);
        let mut r = ui
            .data_mut(|d| d.get_temp::<i32>(regen_r_key))
            .unwrap_or(sys.coord.r);
        let mut index = ui
            .data_mut(|d| d.get_temp::<usize>(regen_index_key))
            .unwrap_or(sys.index);
        let seed_src = state.config.generation.seed.clone();
        let seed_key = egui::Id::new(("sys_regen_seed_buf", id.as_str()));
        let mut seed = seed_src.clone();

        egui::Grid::new("sys_regen_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("coord");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut q)
                            .range(0..=state.sector.width as i32 - 1)
                            .prefix("q"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut r)
                            .range(0..=state.sector.height as i32 - 1)
                            .prefix("r"),
                    );
                });
                ui.end_row();
                ui.label("index");
                ui.add(egui::DragValue::new(&mut index).range(1..=usize::MAX));
                ui.data_mut(|d| {
                    d.insert_temp(regen_q_key, q);
                    d.insert_temp(regen_r_key, r);
                    d.insert_temp(regen_index_key, index);
                });
                ui.end_row();
                ui.label("seed");
                let (buf, _) =
                    crate::builder::panels::persistent_singleline(ui, seed_key, &seed_src);
                seed = buf;
                ui.end_row();
            });

        ui.horizontal(|ui| {
                if ui.button("Regenerate this system").clicked() {
                    run_regen(state, original_coord, index, &seed);
                    ui.data_mut(|d| {
                        d.remove::<i32>(regen_q_key);
                        d.remove::<i32>(regen_r_key);
                        d.remove::<usize>(regen_index_key);
                    });
                }
                if (q, r) != (original_coord.q, original_coord.r)
                    && ui.button("Regenerate at coord (replace)").clicked()
                {
                    let new_coord = HexCoord { q, r };
                    let occupant = state
                        .sector
                        .systems
                        .iter()
                        .find(|s| s.coord == new_coord && s.id != sys_id)
                        .map(|s| s.id.clone());
                    if let Some(occupant) = occupant {
                        state.modal = Some(ModalKind::Message(format!(
                            "Hex ({},{}) is held by {occupant}. Move or delete it before regenerating here.",
                            new_coord.q, new_coord.r
                        )));
                    } else {
                        run_regen(state, new_coord, index, &seed);
                        ui.data_mut(|d| {
                            d.remove::<i32>(regen_q_key);
                            d.remove::<i32>(regen_r_key);
                            d.remove::<usize>(regen_index_key);
                        });
                    }
                }
            });
        ui.colored_label(
            Color32::GRAY,
            format!("(current id: {id} — pinned systems refuse regen)"),
        );
    });
}

fn run_regen(state: &mut BuilderState, coord: HexCoord, index: usize, seed: &str) {
    let seed_override = if seed == state.config.generation.seed {
        None
    } else {
        Some(seed)
    };
    match state.generate_system_here(coord, index, seed_override) {
        Ok(id) => {
            state.focus_system(id);
        }
        Err(e) => {
            state.modal = Some(ModalKind::Message(format!("Regen failed: {e}")));
        }
    }
}

// ── S4 bulk ops ─────────────────────────────────────────────────────────────

fn show_bulk_ops(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "sys_bulk_ops", "Bulk operations", false, |ui| {
        let n = state.selected_systems.len();
        ui.label(format!("{n} system(s) selected"));
        if n == 0 {
            ui.colored_label(
                Color32::GRAY,
                "Shift-click systems or drag a rect on the MAP tab.",
            );
            return;
        }

        ui.horizontal(|ui| {
            if ui.button("Clear selection").clicked() {
                state.selected_systems.clear();
            }
            if ui.button("Pin all").clicked() {
                for id in state.selected_systems.iter().cloned().collect::<Vec<_>>() {
                    state.pinned_systems.insert(id);
                }
            }
            if ui.button("Unpin all").clicked() {
                for id in state.selected_systems.iter().cloned().collect::<Vec<_>>() {
                    state.pinned_systems.remove(&id);
                }
            }
        });

        ui.separator();
        ui.label("Rename pattern — `{n}` = sequence, `{id}` = system id, `{name}` = current name");
        let pattern = ui.data_mut(|d| {
            d.get_temp_mut_or::<String>(egui::Id::new("bulk_rename_pat"), "Sys-{n}".into())
                .clone()
        });
        let mut pattern_buf = pattern;
        if ui.text_edit_singleline(&mut pattern_buf).changed() {
            ui.data_mut(|d| {
                d.insert_temp(egui::Id::new("bulk_rename_pat"), pattern_buf.clone());
            });
        }
        if ui.button("Apply rename pattern").clicked() {
            apply_bulk_rename(state, &pattern_buf);
        }

        ui.separator();
        ui.label("Reassign primary faction");
        let factions: Vec<_> = state
            .sector
            .factions
            .iter()
            .map(|f| (f.id.clone(), f.name.to_string()))
            .collect();
        ui.horizontal_wrapped(|ui| {
            for (fid, name) in &factions {
                if ui.button(format!("→ {name} ({fid})")).clicked() {
                    apply_bulk_primary_faction(state, fid.clone());
                }
                if sectorforge_gui_core::entity_link(ui, fid.to_string(), true).clicked() {
                    state.focus_entity(EntityRef::Faction(fid.clone()));
                }
            }
        });
        if ui.button("Clear primary factions").clicked() {
            apply_bulk_clear_factions(state);
        }

        ui.separator();
        ui.label("Flip control state");
        ui.horizontal_wrapped(|ui| {
            for s in [
                None,
                Some(SystemState::Pacified),
                Some(SystemState::Fragmented),
                Some(SystemState::Blockaded),
                Some(SystemState::Warzone),
                Some(SystemState::Infiltrated),
                Some(SystemState::Quarantined),
                Some(SystemState::Uncharted),
            ] {
                let label = match s {
                    None => "(none)".to_string(),
                    Some(v) => format!("{v}"),
                };
                if ui.button(label).clicked() {
                    apply_bulk_control_state(state, s);
                }
            }
        });

        ui.separator();
        ui.label("Reseed worlds (drops + re-runs)");
        if ui.button("Reseed worlds for selection").clicked() {
            apply_bulk_reseed(state);
        }
    });
}

/// §CTX1 Phase 3 — promoted to `pub(crate)` so the MAP tab right-click
/// multi-selection menu can dispatch the same bulk-rename helper. Pattern
/// tokens (`{n}`/`{id}`/`{name}`) match the §S4 bulk-ops dialog.
pub(crate) fn apply_bulk_rename(state: &mut BuilderState, pattern: &str) {
    let selection: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
    for (n, id) in selection.into_iter().enumerate() {
        let from = match state.sector.systems.iter().find(|s| s.id == id) {
            Some(s) => s.name.to_string(),
            None => continue,
        };
        let to = pattern
            .replace("{n}", &(n + 1).to_string())
            .replace("{id}", id.as_ref())
            .replace("{name}", &from);
        if to == from {
            continue;
        }
        let cmd = BuilderCommand::RenameSystem {
            id: id.clone(),
            from,
            to,
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("Bulk rename failed: {e}")));
            return;
        }
    }
}

/// §CTX1 Phase 3 — promoted to `pub(crate)` so the MAP tab right-click
/// multi-selection menu can dispatch the same primary-faction assignment.
pub(crate) fn apply_bulk_primary_faction(
    state: &mut BuilderState,
    fid: sectorforge::ids::FactionId,
) {
    // §R4: each affected system rides its own EditSystem (was an in-place
    // `primary_factions.push`) so the bulk assignment is undoable. One undo
    // entry per system mutated; systems already carrying `fid` are skipped so
    // they don't emit no-op commands.
    let ids: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
    for id in ids {
        let draft = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .filter(|s| !s.primary_factions.contains(&fid))
            .cloned();
        let Some(mut draft) = draft else {
            continue;
        };
        draft.primary_factions.push(fid.clone());
        let cmd = BuilderCommand::EditSystem {
            system: id,
            before: None,
            after: Box::new(draft),
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("System edit failed: {e}")));
            return;
        }
    }
}

/// §CTX1 Phase 3 — promoted to `pub(crate)` for the MAP tab right-click
/// multi-selection menu.
pub(crate) fn apply_bulk_clear_factions(state: &mut BuilderState) {
    // §R4: clear each affected system's primary factions through EditSystem
    // (was an in-place `primary_factions.clear()` over `sector_mut()`).
    // Systems already empty are skipped so they don't emit no-op commands.
    let ids: BTreeSet<SystemId> = state.selected_systems.clone();
    let targets: Vec<SystemId> = state
        .sector
        .systems
        .iter()
        .filter(|s| ids.contains(&s.id) && !s.primary_factions.is_empty())
        .map(|s| s.id.clone())
        .collect();
    for id in targets {
        let Some(mut draft) = state.sector.systems.iter().find(|s| s.id == id).cloned() else {
            continue;
        };
        draft.primary_factions.clear();
        let cmd = BuilderCommand::EditSystem {
            system: id,
            before: None,
            after: Box::new(draft),
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("System edit failed: {e}")));
            return;
        }
    }
}

/// §CTX1 Phase 3 — promoted to `pub(crate)` for the MAP tab right-click
/// multi-selection menu. `value = None` clears the control flag.
pub(crate) fn apply_bulk_control_state(state: &mut BuilderState, value: Option<SystemState>) {
    // §R4: flip each selected system's control state through EditSystem (was an
    // in-place `set_system_control_state` over `sector` that bypassed the bus,
    // matching the sibling `apply_bulk_clear_factions`). Systems already at
    // `value` are skipped so they don't emit no-op commands.
    let ids: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
    for id in ids {
        let Some(mut draft) = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id && s.control.state != value)
            .cloned()
        else {
            continue;
        };
        draft.control.state = value;
        let cmd = BuilderCommand::EditSystem {
            system: id,
            before: None,
            after: Box::new(draft),
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("Control flip failed: {e}")));
            return;
        }
    }
}

/// §CTX1 Phase 3 — promoted to `pub(crate)` for the MAP tab right-click
/// multi-selection menu. Pinned systems are skipped (§S3).
pub(crate) fn apply_bulk_reseed(state: &mut BuilderState) {
    let targets: Vec<(SystemId, HexCoord, usize)> = state
        .selected_systems
        .iter()
        .filter_map(|id| {
            let sys = state.sector.systems.iter().find(|s| s.id == *id)?;
            if state.pinned_systems.contains(id) {
                return None;
            }
            Some((id.clone(), sys.coord, sys.index))
        })
        .collect();
    for (_id, coord, index) in targets {
        if let Err(e) = state.generate_system_here(coord, index, None) {
            state.modal = Some(ModalKind::Message(format!("Reseed failed: {e}")));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> BuilderState {
        BuilderState::new_blank("t", "T", "seed", 8, 8)
    }

    #[test]
    fn bulk_rename_applies_pattern() {
        let mut state = blank();
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 1, r: 0 }, "B")
            .unwrap();
        state.selected_systems.insert(a.clone());
        state.selected_systems.insert(b.clone());
        apply_bulk_rename(&mut state, "Bulk-{n}");
        let names: Vec<_> = state
            .sector
            .systems
            .iter()
            .map(|s| s.name.to_string())
            .collect();
        assert!(names.contains(&"Bulk-1".to_string()));
        assert!(names.contains(&"Bulk-2".to_string()));
    }

    #[test]
    fn bulk_control_state_flips_selection() {
        let mut state = blank();
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        state.selected_systems.insert(a.clone());
        apply_bulk_control_state(&mut state, Some(SystemState::Warzone));
        assert_eq!(
            state.sector.systems[0].control.state,
            Some(SystemState::Warzone)
        );
    }

    #[test]
    fn bulk_pin_unpin_round_trip() {
        let mut state = blank();
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        state.selected_systems.insert(a.clone());
        state.pinned_systems.insert(a.clone());
        assert!(state.pinned_systems.contains(&a));
        state.pinned_systems.remove(&a);
        assert!(!state.pinned_systems.contains(&a));
    }

    #[test]
    fn system_view_renders_when_no_worlds() {
        // §CTX0 Phase 0: an empty system must not panic when SystemView is
        // mounted under the SYSTEM tab.
        let mut state = blank();
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        state.selected_system_id = Some(a);
        let ctx = egui::Context::default();
        let raw = egui::RawInput::default();
        let _ = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let sys_idx = state
                    .sector
                    .systems
                    .iter()
                    .position(|s| Some(&s.id) == state.selected_system_id.as_ref())
                    .unwrap();
                show_system_map_section(ui, &mut state, sys_idx);
            });
        });
        assert!(state.selected_world_id.is_none());
        assert!(state.scroll_target.is_none());
    }

    #[test]
    fn world_click_updates_selected_world_id() {
        // §CTX0 Phase 0: SystemClick::World must route to the matching
        // GeneratedWorld id; SystemClick::Star must arm scroll_target.
        let mut state = blank();
        let sys = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let world = state.sector.add_world_to_system(&sys, "W").unwrap();
        let sys_idx = 0;
        let world_idx = state.sector.systems[sys_idx]
            .worlds
            .iter()
            .find(|w| w.id == world)
            .unwrap()
            .index;
        handle_system_view_click(&mut state, sys_idx, SystemClick::World(world_idx));
        assert_eq!(state.selected_world_id.as_ref(), Some(&world));
        assert!(state.scroll_target.is_none());

        handle_system_view_click(&mut state, sys_idx, SystemClick::Star);
        assert_eq!(state.scroll_target, Some(SYS_STAR_GRID_ANCHOR));
    }

    #[test]
    fn apply_coord_move_rejects_out_of_bounds() {
        let mut state = blank();
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        apply_coord_move(
            &mut state,
            a.clone(),
            HexCoord { q: 1, r: 1 },
            HexCoord { q: 99, r: 99 },
        );
        assert!(matches!(state.modal, Some(ModalKind::Message(_))));
    }
}
