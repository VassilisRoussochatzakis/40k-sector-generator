//! §CTX1 Phase 6 — in-system right-click menu (§6.6 — §6.9 of
//! `docs/CONTEXT_MENU.txt`). Embedded under the SYSTEM tab by
//! [`super::system::show_system_map_section`]; this module owns the
//! resolver, the menu action enum, and the floating `egui::Area`.
//!
//! All mutations funnel through existing or Phase-6-new `BuilderCommand`
//! variants so undo/redo and the validation pump keep working.

use std::sync::Arc;

use egui::{Pos2, Vec2};

use sectorforge::ids::{SystemId, WorldId};
use sectorforge::sector_model::GeneratedStar;

use sectorforge_gui_core::system_view::{pick_world, SystemPick};

use crate::builder::command::BuilderCommand;
use crate::builder::state::{
    EntityRef, ModalKind, PendingWorldRename, SystemContextMenu, SystemMenuTarget,
};
use crate::builder::BuilderState;

/// Default spectral cycle order used by `CYCLE SPECTRAL CLASS`. Keeps the
/// menu out of the worlds.toml catalog so the cycle is deterministic and
/// surface-light.
const SPECTRAL_CYCLE: &[&str] = &["O", "B", "A", "F", "G", "K", "M"];

/// §CTX1 Phase 6 — per-item action types. Each variant maps 1:1 to a menu
/// row in the §6.6 — §6.9 schemas. Splitting the actions from the render
/// path lets unit tests assert state mutations without standing up an egui
/// context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SystemMenuAction {
    // ── §6.6 star ─────────────────────────────────────────────────────────
    FocusStarDetails,
    CycleSpectralClass { system: SystemId },
    RemoveStar { system: SystemId },
    AddStar { system: SystemId },
    // ── §6.7 world ────────────────────────────────────────────────────────
    FocusWorld { system: SystemId, world: WorldId },
    RenameWorldOpen { system: SystemId, world: WorldId },
    SetWorldOrbit { world: WorldId, orbit: u8 },
    DuplicateWorld { system: SystemId, world: WorldId },
    RemoveWorld { world: WorldId },
    OpenWorldTab { system: SystemId, world: WorldId },
    // ── §6.8 empty orbit ──────────────────────────────────────────────────
    AddWorldAtOrbit { system: SystemId, orbit: u8 },
    // ── §6.9 background ───────────────────────────────────────────────────
    AddWorldDefault { system: SystemId },
    RegenerateSystem { system: SystemId },
    DeriveOrbitalAssets { system: SystemId },
}

/// Anchor id consumed by [`super::system::show`] after the right-click
/// `FOCUS STAR DETAILS` row arms `scroll_target`. Must match the
/// `SYS_STAR_GRID_ANCHOR` constant in [`super::system`].
const SYS_STAR_GRID_ANCHOR: &str = "sys_star_grid";

/// Map a [`SystemPick`] (returned by [`pick_world`]) onto a
/// [`SystemMenuTarget`]. Returns `None` when the system index is stale.
pub(super) fn resolve_system_context(
    state: &BuilderState,
    sys_idx: usize,
    pick: SystemPick,
) -> Option<SystemMenuTarget> {
    let sys = state.sector.systems.get(sys_idx)?;
    let system = sys.id.clone();
    match pick {
        SystemPick::Star => Some(SystemMenuTarget::Star { system }),
        SystemPick::World(idx) => {
            let w = sys.worlds.iter().find(|w| w.index == idx)?;
            Some(SystemMenuTarget::World {
                system,
                world: w.id.clone(),
                orbit: w.orbit,
            })
        }
        SystemPick::EmptyOrbit(orbit) => Some(SystemMenuTarget::EmptyOrbit { system, orbit }),
        SystemPick::Background => Some(SystemMenuTarget::Background { system }),
    }
}

/// Run one menu item. Returns `true` so callers can dismiss the menu.
pub(super) fn apply_system_menu_action(state: &mut BuilderState, action: SystemMenuAction) -> bool {
    match action {
        SystemMenuAction::FocusStarDetails => {
            state.scroll_target = Some(SYS_STAR_GRID_ANCHOR);
        }
        SystemMenuAction::CycleSpectralClass { system } => {
            cycle_spectral_class(state, &system);
        }
        SystemMenuAction::RemoveStar { system } => {
            let cmd = BuilderCommand::SetStar {
                system,
                before: None,
                after: None,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Remove star failed: {e}")));
            }
        }
        SystemMenuAction::AddStar { system } => {
            let after = Some(GeneratedStar {
                colour_code: Arc::from("G"),
                colour_name: Arc::from("Yellow"),
                spectral_type: Some(Arc::from("G2V")),
                source_row_index: None,
            });
            let cmd = BuilderCommand::SetStar {
                system,
                before: None,
                after,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Add star failed: {e}")));
            }
        }
        SystemMenuAction::FocusWorld { system, world } => {
            state.focus_entity(EntityRef::World { system, world });
        }
        SystemMenuAction::RenameWorldOpen { system, world } => {
            let text = state
                .sector
                .all_worlds()
                .find(|w| w.id == world)
                .map(|w| w.name.to_string())
                .unwrap_or_default();
            state.pending_world_rename = Some(PendingWorldRename {
                system,
                world,
                text,
            });
        }
        SystemMenuAction::SetWorldOrbit { world, orbit } => {
            let before = state
                .sector
                .all_worlds()
                .find(|w| w.id == world)
                .map(|w| w.orbit);
            let Some(before) = before else { return true };
            if before == orbit {
                return true;
            }
            let cmd = BuilderCommand::SetWorldOrbit {
                world,
                before,
                after: orbit,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Set orbit failed: {e}")));
            }
        }
        SystemMenuAction::DuplicateWorld { system, world } => {
            duplicate_world(state, &system, &world);
        }
        SystemMenuAction::RemoveWorld { world } => {
            let cmd = BuilderCommand::RemoveWorld {
                world,
                before: None,
                parent_system: None,
                parent_position: None,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Remove world failed: {e}")));
            }
        }
        SystemMenuAction::OpenWorldTab { system, world } => {
            state.focus_entity(EntityRef::World { system, world });
        }
        SystemMenuAction::AddWorldAtOrbit { system, orbit } => {
            add_world_at_orbit(state, system, orbit);
        }
        SystemMenuAction::AddWorldDefault { system } => {
            let next_orbit = state
                .sector
                .systems
                .iter()
                .find(|s| s.id == system)
                .map(|s| s.worlds.iter().map(|w| w.orbit).max().unwrap_or(0) + 1)
                .unwrap_or(1);
            add_world_at_orbit(state, system, next_orbit);
        }
        SystemMenuAction::RegenerateSystem { system } => {
            let entry = state
                .sector
                .systems
                .iter()
                .find(|s| s.id == system)
                .map(|s| (s.coord, s.index));
            let Some((coord, index)) = entry else {
                return true;
            };
            if state.pinned_systems.contains(&system) {
                return true;
            }
            match state.generate_system_here(coord, index, None) {
                Ok(new_id) => state.focus_system(new_id),
                Err(e) => {
                    state.modal = Some(ModalKind::Message(format!("Regen failed: {e}")));
                }
            }
        }
        SystemMenuAction::DeriveOrbitalAssets { system } => {
            super::orbital::derive_and_apply_orbital_assets(state, &system);
        }
    }
    true
}

/// Cycle `star.spectral_type` through [`SPECTRAL_CYCLE`]. The first letter of
/// the current spectral string is used as the cycle key so values like
/// `"G2V"` map back to the `"G"` slot before the +1 step is applied.
fn cycle_spectral_class(state: &mut BuilderState, system: &SystemId) {
    let before = state
        .sector
        .systems
        .iter()
        .find(|s| s.id == *system)
        .and_then(|s| s.star.as_ref().map(|st| st.spectral_type.clone()));
    let Some(before) = before else {
        return;
    };
    let key = before
        .as_ref()
        .and_then(|s| s.chars().next())
        .map(|c| c.to_ascii_uppercase().to_string());
    let next_idx = match key.as_deref() {
        Some(k) => SPECTRAL_CYCLE
            .iter()
            .position(|s| *s == k)
            .map(|p| (p + 1) % SPECTRAL_CYCLE.len())
            .unwrap_or(0),
        None => 0,
    };
    let after = Some(Arc::from(SPECTRAL_CYCLE[next_idx]));
    let cmd = BuilderCommand::SetStarSpectral {
        system: system.clone(),
        before: None,
        after,
    };
    if let Err(e) = state.run(cmd) {
        state.modal = Some(ModalKind::Message(format!(
            "Cycle spectral class failed: {e}"
        )));
    }
}

/// §6.7 DUPLICATE WORLD — dispatches [`BuilderCommand::AddWorld`] then patches
/// the newly added world's payload to mirror `source`. The patch step bypasses
/// the bus (one-shot mutation + dirty flag) because there is no equivalent
/// `DuplicateWorld` command yet; the AddWorld itself is undoable so re-undo
/// removes the duplicate cleanly even if the patch is lost.
fn duplicate_world(state: &mut BuilderState, system: &SystemId, source: &WorldId) {
    let source_payload = state.sector.all_worlds().find(|w| w.id == *source).cloned();
    let Some(source_payload) = source_payload else {
        return;
    };
    let next_index = state
        .sector
        .systems
        .iter()
        .find(|s| s.id == *system)
        .map(|s| s.worlds.iter().map(|w| w.index).max().unwrap_or(0) + 1)
        .unwrap_or(1);
    let cmd = BuilderCommand::AddWorld {
        system: system.clone(),
        name: format!("{}-copy", source_payload.name),
        result_id: None,
    };
    if let Err(e) = state.run(cmd) {
        state.modal = Some(ModalKind::Message(format!("Duplicate world failed: {e}")));
        return;
    }
    if let Some(sys) = state.sector.systems.iter_mut().find(|s| s.id == *system) {
        if let Some(w) = sys.worlds.iter_mut().find(|w| w.index == next_index) {
            let new_id = w.id.clone();
            let new_idx = w.index;
            let new_name = w.name.clone();
            *w = source_payload.clone();
            w.id = new_id;
            w.index = new_idx;
            w.name = new_name;
            // Mirror the source's orbit so the duplicate lands on the same ring.
            w.orbit = source_payload.orbit;
        }
    }
    state.dirty = true;
    state.mark_validation_dirty();
}

/// §6.8/§6.9 ADD WORLD — dispatches [`BuilderCommand::AddWorld`] and then
/// pins the resulting world's orbit. The orbit pin bypasses undo for the same
/// reason as [`duplicate_world`]: the AddWorld bus entry is the single source
/// of truth.
fn add_world_at_orbit(state: &mut BuilderState, system: SystemId, orbit: u8) {
    let next_index = state
        .sector
        .systems
        .iter()
        .find(|s| s.id == system)
        .map(|s| s.worlds.iter().map(|w| w.index).max().unwrap_or(0) + 1)
        .unwrap_or(1);
    let cmd = BuilderCommand::AddWorld {
        system: system.clone(),
        name: format!("World-{next_index}"),
        result_id: None,
    };
    if let Err(e) = state.run(cmd) {
        state.modal = Some(ModalKind::Message(format!("Add world failed: {e}")));
        return;
    }
    if let Some(sys) = state.sector.systems.iter_mut().find(|s| s.id == system) {
        if let Some(w) = sys.worlds.iter_mut().find(|w| w.index == next_index) {
            w.orbit = orbit;
        }
    }
    state.dirty = true;
    state.mark_validation_dirty();
}

/// §CTX1 Phase 6 — secondary-click entry point. Reads the screen position and
/// the widget rect's origin, computes the [`SystemPick`] via [`pick_world`],
/// resolves the [`SystemMenuTarget`], and arms `system_context_menu`. Pure
/// `state` mutation so unit tests can drive it without an egui frame.
pub(super) fn arm_system_context_menu(
    state: &mut BuilderState,
    sys_idx: usize,
    side: f32,
    screen_pos: Pos2,
    rect_origin: Pos2,
) {
    let local = Pos2::new(screen_pos.x - rect_origin.x, screen_pos.y - rect_origin.y);
    let sys = match state.sector.systems.get(sys_idx) {
        Some(s) => s,
        None => return,
    };
    let pick = pick_world(side, sys, local);
    if let Some(target) = resolve_system_context(state, sys_idx, pick) {
        state.system_context_menu = Some(SystemContextMenu { screen_pos, target });
    }
}

/// §CTX1 Phase 6 — render the floating in-system menu. Mirrors the dismiss
/// rules used by [`super::map::show_sector_context_menu`]: Escape, focus loss,
/// outside primary click, or item activation closes the menu.
pub(super) fn show_system_context_menu(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(menu) = state.system_context_menu.as_ref() else {
        return;
    };
    let screen_pos = menu.screen_pos;
    let target = menu.target.clone();
    let mut close = false;
    let area_resp = egui::Area::new(egui::Id::new("system_context_menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen_pos)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);
                match &target {
                    SystemMenuTarget::Star { system } => {
                        close |= render_star_menu(ui, state, system.clone());
                    }
                    SystemMenuTarget::World {
                        system,
                        world,
                        orbit,
                    } => {
                        close |=
                            render_world_menu(ui, state, system.clone(), world.clone(), *orbit);
                    }
                    SystemMenuTarget::EmptyOrbit { system, orbit } => {
                        close |= render_empty_orbit_menu(ui, state, system.clone(), *orbit);
                    }
                    SystemMenuTarget::Background { system } => {
                        close |= render_background_menu(ui, state, system.clone());
                    }
                }
            });
        });

    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let focused = ctx.input(|i| i.focused);
    let area_rect = area_resp.response.rect;
    let primary_click_outside = ctx.input(|i| {
        i.pointer.primary_clicked()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !area_rect.contains(p))
    });

    if close || esc || !focused || primary_click_outside {
        state.system_context_menu = None;
    }
}

fn render_star_menu(ui: &mut egui::Ui, state: &mut BuilderState, system: SystemId) -> bool {
    let mut close = false;
    let has_star = state
        .sector
        .systems
        .iter()
        .find(|s| s.id == system)
        .map(|s| s.star.is_some())
        .unwrap_or(false);

    if ui.selectable_label(false, "FOCUS STAR DETAILS").clicked() {
        close |= apply_system_menu_action(state, SystemMenuAction::FocusStarDetails);
    }

    let cycle_resp = ui.add_enabled(
        has_star,
        egui::SelectableLabel::new(false, "CYCLE SPECTRAL CLASS"),
    );
    if cycle_resp.clicked() {
        close |= apply_system_menu_action(
            state,
            SystemMenuAction::CycleSpectralClass {
                system: system.clone(),
            },
        );
    }
    if !has_star {
        cycle_resp.on_hover_text("System has no star — add one first.");
    }

    if has_star {
        if ui.selectable_label(false, "✕ REMOVE STAR").clicked() {
            close |= apply_system_menu_action(state, SystemMenuAction::RemoveStar { system });
        }
    } else if ui.selectable_label(false, "ADD STAR").clicked() {
        close |= apply_system_menu_action(state, SystemMenuAction::AddStar { system });
    }

    close
}

fn render_world_menu(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    system: SystemId,
    world: WorldId,
    orbit: u8,
) -> bool {
    let mut close = false;
    let summary = state
        .sector
        .all_worlds()
        .find(|w| w.id == world)
        .map(|w| (w.name.to_string(), w.orbit));
    let Some((name, cur_orbit)) = summary else {
        if ui.selectable_label(false, "CLOSE").clicked() {
            close = true;
        }
        return close;
    };
    let _ = orbit; // surfaced through `cur_orbit` after we re-read the model
    ui.label(egui::RichText::new(format!("WORLD {world} — {name}")).italics());
    ui.separator();

    if ui.selectable_label(false, "FOCUS WORLD").clicked() {
        close |= apply_system_menu_action(
            state,
            SystemMenuAction::FocusWorld {
                system: system.clone(),
                world: world.clone(),
            },
        );
    }
    if ui.selectable_label(false, "RENAME…").clicked() {
        close |= apply_system_menu_action(
            state,
            SystemMenuAction::RenameWorldOpen {
                system: system.clone(),
                world: world.clone(),
            },
        );
    }

    let max_offer = cur_orbit.saturating_add(4).max(8);
    ui.menu_button("MOVE ORBIT ▸", |ui| {
        for o in 1_u8..=max_offer {
            let label = if o == cur_orbit {
                format!("• orbit {o}")
            } else {
                format!("  orbit {o}")
            };
            if ui.selectable_label(false, label).clicked() {
                close |= apply_system_menu_action(
                    state,
                    SystemMenuAction::SetWorldOrbit {
                        world: world.clone(),
                        orbit: o,
                    },
                );
                ui.close_menu();
            }
        }
    });

    if ui.selectable_label(false, "DUPLICATE WORLD").clicked() {
        close |= apply_system_menu_action(
            state,
            SystemMenuAction::DuplicateWorld {
                system: system.clone(),
                world: world.clone(),
            },
        );
    }
    if ui.selectable_label(false, "✕ REMOVE WORLD").clicked() {
        close |= apply_system_menu_action(
            state,
            SystemMenuAction::RemoveWorld {
                world: world.clone(),
            },
        );
    }

    ui.separator();

    if ui.selectable_label(false, "Open in WORLD tab").clicked() {
        close |= apply_system_menu_action(
            state,
            SystemMenuAction::OpenWorldTab {
                system,
                world: world.clone(),
            },
        );
    }
    if ui
        .selectable_label(false, format!("COPY ID ({world})"))
        .clicked()
    {
        ui.output_mut(|o| o.copied_text = world.to_string());
        close = true;
    }

    close
}

fn render_empty_orbit_menu(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    system: SystemId,
    orbit: u8,
) -> bool {
    let mut close = false;
    ui.label(egui::RichText::new(format!("ORBIT {orbit}")).italics());
    ui.separator();
    if ui
        .selectable_label(false, format!("ADD WORLD AT ORBIT {orbit}"))
        .clicked()
    {
        close |=
            apply_system_menu_action(state, SystemMenuAction::AddWorldAtOrbit { system, orbit });
    }
    close
}

fn render_background_menu(ui: &mut egui::Ui, state: &mut BuilderState, system: SystemId) -> bool {
    let mut close = false;
    let pinned = state.pinned_systems.contains(&system);

    if ui.selectable_label(false, "ADD WORLD").clicked() {
        close |= apply_system_menu_action(
            state,
            SystemMenuAction::AddWorldDefault {
                system: system.clone(),
            },
        );
    }

    let regen_resp = ui.add_enabled(
        !pinned,
        egui::SelectableLabel::new(false, "REGENERATE SYSTEM"),
    );
    if regen_resp.clicked() {
        close |= apply_system_menu_action(
            state,
            SystemMenuAction::RegenerateSystem {
                system: system.clone(),
            },
        );
    }
    if pinned {
        regen_resp.on_hover_text("Unpin first (§S3).");
    }

    if ui
        .selectable_label(false, "DERIVE ORBITAL ASSETS")
        .clicked()
    {
        close |= apply_system_menu_action(state, SystemMenuAction::DeriveOrbitalAssets { system });
    }

    close
}

/// §CTX1 Phase 6 — modal rename dialog for the §6.7 "RENAME…" entry. Commits
/// through [`BuilderCommand::RenameWorld`] so the change is undoable.
pub(super) fn show_world_rename_dialog(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(pending) = state.pending_world_rename.clone() else {
        return;
    };
    let before = state
        .sector
        .all_worlds()
        .find(|w| w.id == pending.world)
        .map(|w| w.name.to_string())
        .unwrap_or_default();
    let mut text = pending.text.clone();
    let mut commit = false;
    let mut close = false;
    egui::Window::new("Rename world")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(format!("world: {} — current: {}", pending.world, before));
            ui.text_edit_singleline(&mut text);
            ui.horizontal(|ui| {
                let enabled = !text.trim().is_empty() && text != before;
                if ui
                    .add_enabled(enabled, egui::Button::new("Rename"))
                    .clicked()
                {
                    commit = true;
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if commit {
        let cmd = BuilderCommand::RenameWorld {
            world: pending.world.clone(),
            before: before.clone(),
            after: text.clone(),
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("Rename world failed: {e}")));
        }
    }
    if close {
        state.pending_world_rename = None;
    } else {
        state.pending_world_rename = Some(PendingWorldRename {
            system: pending.system,
            world: pending.world,
            text,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::HexCoord;

    fn blank() -> BuilderState {
        BuilderState::new_blank("t", "T", "seed", 8, 8)
    }

    fn add_sys_world(state: &mut BuilderState) -> (SystemId, WorldId) {
        let sys = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let w = state.sector.add_world_to_system(&sys, "Capitis").unwrap();
        (sys, w)
    }

    #[test]
    fn resolve_star_pick_returns_star_target() {
        let mut state = blank();
        let (sys, _) = add_sys_world(&mut state);
        let target = resolve_system_context(&state, 0, SystemPick::Star).unwrap();
        assert_eq!(target, SystemMenuTarget::Star { system: sys });
    }

    #[test]
    fn resolve_world_pick_uses_world_index() {
        let mut state = blank();
        let (sys, wid) = add_sys_world(&mut state);
        let idx = state.sector.systems[0].worlds[0].index;
        let target = resolve_system_context(&state, 0, SystemPick::World(idx)).unwrap();
        assert_eq!(
            target,
            SystemMenuTarget::World {
                system: sys,
                world: wid,
                orbit: 1
            }
        );
    }

    #[test]
    fn add_star_then_remove_round_trips_through_bus() {
        let mut state = blank();
        let sys = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        assert!(state.sector.systems[0].star.is_none());
        apply_system_menu_action(
            &mut state,
            SystemMenuAction::AddStar {
                system: sys.clone(),
            },
        );
        assert!(state.sector.systems[0].star.is_some());
        apply_system_menu_action(
            &mut state,
            SystemMenuAction::RemoveStar {
                system: sys.clone(),
            },
        );
        assert!(state.sector.systems[0].star.is_none());
    }

    #[test]
    fn cycle_spectral_class_wraps_to_first_when_unknown() {
        let mut state = blank();
        let sys = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        if let Some(s) = state.sector.systems.iter_mut().find(|s| s.id == sys) {
            s.star = Some(GeneratedStar {
                colour_code: Arc::from("G"),
                colour_name: Arc::from("Yellow"),
                spectral_type: None,
                source_row_index: None,
            });
        }
        cycle_spectral_class(&mut state, &sys);
        let s = state.sector.systems.iter().find(|s| s.id == sys).unwrap();
        assert_eq!(s.star.as_ref().unwrap().spectral_type.as_deref(), Some("O"));
    }

    #[test]
    fn set_world_orbit_dispatches_command_when_changed() {
        let mut state = blank();
        let (_, wid) = add_sys_world(&mut state);
        apply_system_menu_action(
            &mut state,
            SystemMenuAction::SetWorldOrbit {
                world: wid.clone(),
                orbit: 5,
            },
        );
        assert_eq!(state.sector.systems[0].worlds[0].orbit, 5);
    }

    #[test]
    fn set_world_orbit_noop_when_unchanged() {
        let mut state = blank();
        let (_, wid) = add_sys_world(&mut state);
        let before_log = state.command_log.len();
        apply_system_menu_action(
            &mut state,
            SystemMenuAction::SetWorldOrbit {
                world: wid,
                orbit: 1, // already 1
            },
        );
        assert_eq!(state.command_log.len(), before_log);
    }

    #[test]
    fn duplicate_world_produces_unique_id() {
        let mut state = blank();
        let (sys, wid) = add_sys_world(&mut state);
        let before_count = state.sector.systems[0].worlds.len();
        apply_system_menu_action(
            &mut state,
            SystemMenuAction::DuplicateWorld {
                system: sys,
                world: wid.clone(),
            },
        );
        let worlds = &state.sector.systems[0].worlds;
        assert_eq!(worlds.len(), before_count + 1);
        let ids: std::collections::HashSet<_> = worlds.iter().map(|w| w.id.clone()).collect();
        assert_eq!(ids.len(), worlds.len());
        assert!(worlds.iter().any(|w| w.id == wid));
    }

    #[test]
    fn add_world_at_orbit_sets_orbit() {
        let mut state = blank();
        let (sys, _) = add_sys_world(&mut state);
        apply_system_menu_action(
            &mut state,
            SystemMenuAction::AddWorldAtOrbit {
                system: sys,
                orbit: 9,
            },
        );
        let last = state.sector.systems[0].worlds.last().unwrap();
        assert_eq!(last.orbit, 9);
    }

    #[test]
    fn remove_world_drops_world() {
        let mut state = blank();
        let (_, wid) = add_sys_world(&mut state);
        apply_system_menu_action(&mut state, SystemMenuAction::RemoveWorld { world: wid });
        assert!(state.sector.systems[0].worlds.is_empty());
    }

    #[test]
    fn rename_world_open_arms_dialog() {
        let mut state = blank();
        let (sys, wid) = add_sys_world(&mut state);
        apply_system_menu_action(
            &mut state,
            SystemMenuAction::RenameWorldOpen {
                system: sys,
                world: wid.clone(),
            },
        );
        let pending = state.pending_world_rename.as_ref().unwrap();
        assert_eq!(pending.world, wid);
        assert_eq!(pending.text, "Capitis");
    }

    #[test]
    fn focus_star_details_arms_scroll_target() {
        let mut state = blank();
        let _ = add_sys_world(&mut state);
        apply_system_menu_action(&mut state, SystemMenuAction::FocusStarDetails);
        assert_eq!(state.scroll_target, Some(SYS_STAR_GRID_ANCHOR));
    }
}
