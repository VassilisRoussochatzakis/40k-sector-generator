//! SYSTEM tab (§N1 / §N2) — Phase B §S2..§S6 inspector.
//!
//! Covers every `GeneratedSystem` field via a per-section inspector, the
//! §S3 pinned toggle (driven by [`BuilderState::pinned_systems`]), the §S4
//! bulk-ops block over `BuilderState::selection.systems`, the §S5
//! single-system regenerate (`sectorforge::generate_system_standalone`), and
//! the §S6 coord-validity check on inline coord edits. Fields managed by
//! sibling panels (worlds §8, primary factions §10, control §11, orbital
//! assets §31, conflict §28, intel §29, archetype §30) are shown read-only
//! with deep-link buttons.

use egui::{Color32, RichText, Ui};

use sectorforge::sector_model::{SystemKind, SystemState};
use sectorforge_gui_core::{card, palette, ui_kit::{self, labeled}};

use crate::builder::command::BuilderCommand;
use crate::builder::state::{BuilderTab, EntityRef, ModalKind};
use crate::builder::BuilderState;

mod archetype;
mod bulk_ops;
mod identity;
mod preview;
mod regen;

use archetype::{show_archetype_auto_assign, show_archetype_rules, show_archetype_section};
use bulk_ops::show_bulk_ops;
use identity::{show_identity_section, show_star_section, show_tags_notes_section};
use preview::show_bitmap_preview_section;
use regen::show_regen_section;

pub(crate) use bulk_ops::{
    apply_bulk_control_state, apply_bulk_primary_faction, apply_bulk_rename, apply_bulk_reseed,
};
pub(crate) use preview::show_system_map_section;

/// §CTX0 — scroll-anchor id used by [`show_star_section`] when
/// [`BuilderState::scroll_target`] points at the Star header. Mirrors the
/// literal passed to the inner `egui::Grid::new` so both sides stay in sync.
///
/// §CTX1 Phase 6 — `panels/system_map.rs` mirrors this constant so the
/// in-system right-click menu's `FOCUS STAR DETAILS` row arms the same anchor.
pub(super) const SYS_STAR_GRID_ANCHOR: &str = "sys_star_grid";

/// Slider clamp for the SYSTEM-tab embedded `SystemView` size.
pub(super) const SYSTEM_VIEW_SIDE_MIN: f32 = 400.0;
pub(super) const SYSTEM_VIEW_SIDE_MAX: f32 = 2400.0;

/// Human-friendly label for a [`SystemKind`]. The raw `kind` slug (the value
/// serialised to disk) stays reachable via the combo's per-row hover tooltip.
pub(super) fn system_kind_label(kind: SystemKind) -> &'static str {
    match kind {
        SystemKind::Star => "Star system",
        SystemKind::SpecialLocation => "Special location",
        SystemKind::BlackHole => "Black hole",
        SystemKind::WarpAnomaly => "Warp anomaly",
        SystemKind::SpaceStation => "Space station",
        // `SystemKind` is `#[non_exhaustive]` (defined in `sectorforge`); fall
        // back to the raw slug for any future variant.
        _ => kind.as_slug(),
    }
}

/// Turn a lower_snake slug into a Title Case label for display, e.g.
/// `hidden_cell` → `Hidden cell`. Used to humanise the archetype-axis dropdowns
/// whose enum `Display` emits the raw serialisation slug; the slug itself stays
/// reachable via each row's hover tooltip.
pub(super) fn pretty_slug(slug: &str) -> String {
    let mut out = String::with_capacity(slug.len());
    for (i, word) in slug.split('_').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

/// Human-friendly label for a [`SystemState`] control flag. The raw slug stays
/// reachable via the combo's per-row hover tooltip.
pub(super) fn system_state_label(state: SystemState) -> &'static str {
    match state {
        SystemState::Pacified => "Pacified",
        SystemState::Fragmented => "Fragmented",
        SystemState::Blockaded => "Blockaded",
        SystemState::Warzone => "Warzone",
        SystemState::Infiltrated => "Infiltrated",
        SystemState::Quarantined => "Quarantined",
        SystemState::Uncharted => "Uncharted",
        // `SystemState` is `#[non_exhaustive]` (defined in `sectorforge`); fall
        // back to the raw slug for any future variant.
        _ => state.as_slug(),
    }
}

pub(crate) fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("System");
    ui.label(
        RichText::new(
            "Inspect and edit one system — its star, worlds, factions, and storyline markers.",
        )
        .color(Color32::DARK_GRAY),
    );
    ui.add_space(4.0);

    let count = state.sector.systems.len();
    if count == 0 {
        ui_kit::placeholder(
            ui,
            "No systems in this sector — use the MAP tab's ADD SYSTEM tool.",
        );
        return;
    }

    // §COLUMNS — master-detail: a system roster on the left rail, the inspector
    // (in-system map + RC-2 section grid) filling the rest. The hard 2-column
    // split is promoted to `columns_responsive` so a narrow window collapses to
    // one column instead of crushing both.
    egui::SidePanel::left("system_roster")
        .resizable(true)
        .default_width(220.0)
        .width_range(160.0..=400.0)
        .show_inside(ui, |ui| show_system_roster(ui, state));

    egui::CentralPanel::default().show_inside(ui, |ui| show_system_inspector(ui, state));
}

/// §COLUMNS — left-rail system roster (master pane). Selecting mirrors the old
/// picker: it sets the single selection and resets the §S4 bulk-ops
/// multi-selection to just this system. Pure view state, so written directly.
fn show_system_roster(ui: &mut Ui, state: &mut BuilderState) {
    ui.add_space(2.0);
    let current = state.selection.system_id.clone();
    let mut pick = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for sys in &state.sector.systems {
                let sel = current.as_ref() == Some(&sys.id);
                // §BEAUTY: animated selectable plate (card::selectable_plate).
                let (resp, _) = card::selectable_plate(ui, ("system_row", &sys.id), sel, |ui| {
                    ui.label(
                        RichText::new(sys.name.to_string())
                            .color(palette::chrome_text())
                            .strong(),
                    );
                    ui.label(
                        RichText::new(format!("({})", sys.id))
                            .color(palette::chrome_text_dim())
                            .small(),
                    );
                });
                if resp.clicked() {
                    pick = Some(sys.id.clone());
                }
            }
        });
    if let Some(id) = pick {
        state.selection.system_id = Some(id.clone());
        state.selection.systems.clear();
        state.selection.systems.insert(id);
    }
}

fn show_system_inspector(ui: &mut Ui, state: &mut BuilderState) {
    let selected = state.selection.system_id.clone();
    let Some(sys_id) = selected else {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui_kit::placeholder(ui, "Select a system from the roster or the MAP tab.");
                show_bulk_ops(ui, state);
            });
        return;
    };
    let Some(sys_idx) = state.sector.systems.iter().position(|s| s.id == sys_id) else {
        state.selection.system_id = None;
        return;
    };

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_header(ui, state, sys_idx);
            ui.separator();
            show_system_map_section(ui, state, sys_idx);
            ui.add_space(4.0);
            show_bitmap_preview_section(ui, state, sys_idx);
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            ui_kit::columns_responsive(ui, 2, 460.0, |cols| {
                let n = cols.len();
                {
                    // Left column: identity + read-only state.
                    let left = &mut cols[0];
                    show_identity_section(left, state, sys_idx);
                    left.add_space(4.0);
                    let star_resp = show_star_section(left, state, sys_idx);
                    if state.selection.scroll_target == Some(SYS_STAR_GRID_ANCHOR) {
                        star_resp
                            .header_response
                            .scroll_to_me(Some(egui::Align::TOP));
                        state.selection.scroll_target = None;
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
                }
                {
                    // Right column — or the same single column when collapsed to
                    // one: overlays, archetypes, sibling-panel sections.
                    let right = &mut cols[if n > 1 { 1 } else { 0 }];
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
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            show_regen_section(ui, state, sys_idx);
            ui.add_space(8.0);
            show_bulk_ops(ui, state);
        });
}

// ── header ──────────────────────────────────────────────────────────────────

fn show_header(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    let sys = &state.sector.systems[sys_idx];
    let id = sys.id.clone();
    let pinned = state.pinned_systems.contains(&id);
    ui.horizontal_wrapped(|ui| {
        ui.heading(sys.name.to_string());
        ui.label(
            RichText::new(sys.id.to_string())
                .color(palette::chrome_text_dim())
                .monospace(),
        )
        .on_hover_text(
            "Unique system id (schema: id) — used by routes, presence, and saved files.",
        );
        if pinned {
            ui.colored_label(palette::warning(), "📌 Pinned")
                .on_hover_text("Pinned systems are protected from regeneration and reseeding.");
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
                .button("➕ Add world")
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
                        state.feedback.modal = Some(ModalKind::Message(format!("Add world failed: {e}")));
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
                                state.feedback.modal =
                                    Some(ModalKind::Message(format!("Set orbit failed: {e}")));
                            }
                        }
                    }
                }
            }
        });
        if world_count == 0 {
            ui_kit::placeholder(ui, "No worlds yet — use Add world above.");
        }
        for (wid, name) in world_ids {
            ui.horizontal(|ui| {
                let clicked = sectorforge_gui_core::entity_link(ui, name, true).clicked();
                ui.label(
                    RichText::new(wid.to_string())
                        .color(palette::chrome_text_dim())
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
    ui_kit::collapsing_section(ui, "sys_routes", "Routes (view only)", false, |ui| {
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
        ui.label(format!("{} route(s) touching this system", touching.len()));
        if touching.is_empty() {
            ui_kit::placeholder(ui, "No routes reach this system — add them on the MAP tab.");
        }
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
            ui_kit::placeholder(
                ui,
                "No primary factions — assign them on the CONTROL tab or via Bulk operations.",
            );
        }
    });
}

fn show_control_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_control", "Control", false, |ui| {
        let id = state.sector.systems[sys_idx].id.clone();
        let mut current = state.sector.systems[sys_idx].control.state;
        let summary = state.sector.systems[sys_idx].control.clone();
        labeled(
            ui,
            "Control status",
            "Overall political state of the system (schema: control.state). '(none)' leaves it unset.",
            |ui| {
                ui_kit::combo(
                    "sys_control_state",
                    match current {
                        None => "(none)",
                        Some(s) => system_state_label(s),
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
                        ui.selectable_value(&mut current, Some(s), system_state_label(s))
                            .on_hover_text(format!("schema: {}", s.as_slug()));
                    }
                });
            },
        );
        if current != state.sector.systems[sys_idx].control.state {
            // §R4: route the control-state flip through EditSystem so it lands
            // on the undo/redo log (was a direct `set_system_control_state` over
            // `sector` that bypassed the command bus). EditSystem explicitly
            // covers the control summary; the setter is a plain field write with
            // no cascade, so the clone-mutate-dispatch shape is exact.
            if let Err(e) = state.edit_system(id, |sys| sys.control.state = current) {
                state.feedback.modal = Some(ModalKind::Message(format!("Control update failed: {e}")));
            }
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new("Who holds power here (derived — assign on the CONTROL tab):")
                .small()
                .color(palette::chrome_text_dim()),
        );
        let role = |id: &Option<sectorforge::ids::FactionId>| {
            id.as_ref()
                .map(|f| f.to_string())
                .unwrap_or_else(|| "(none)".to_string())
        };
        labeled(
            ui,
            "Dominant",
            "Faction with the strongest overall presence (schema: control.dominant).",
            |ui| {
                ui.monospace(role(&summary.dominant));
            },
        );
        labeled(
            ui,
            "Sovereign",
            "Recognised ruling authority (schema: control.sovereign).",
            |ui| {
                ui.monospace(role(&summary.sovereign));
            },
        );
        labeled(
            ui,
            "Orbital controller",
            "Faction holding the orbital space (schema: control.orbital_controller).",
            |ui| {
                ui.monospace(role(&summary.orbital_controller));
            },
        );
        labeled(
            ui,
            "Economic hegemon",
            "Faction dominating trade and industry (schema: control.economic_hegemon).",
            |ui| {
                ui.monospace(role(&summary.economic_hegemon));
            },
        );
        labeled(
            ui,
            "Hidden master",
            "Concealed power behind the scenes (schema: control.hidden_master).",
            |ui| {
                ui.monospace(role(&summary.hidden_master));
            },
        );
    });
}

fn show_overlays_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_overlays", "Overlays at a glance", false, |ui| {
        ui.label(
            RichText::new(
                "Quick read of extra layers on this system — edit each in its own section or tab.",
            )
            .small()
            .color(palette::chrome_text_dim()),
        );
        ui.add_space(2.0);
        let sys = &state.sector.systems[sys_idx];
        let has_blockade = !sectorforge::orbital_assets::BlockadeReport::is_default(&sys.blockade);
        let has_conflict = !sectorforge::conflict::ConflictState::is_default(&sys.conflict);
        let has_archetype = !sectorforge::archetypes::ArchetypeState::is_default(&sys.archetype);
        ui.label(format!("Orbital assets: {}", sys.orbital_assets.len()));
        ui.label(format!(
            "Blockade present: {}",
            if has_blockade { "yes" } else { "no" }
        ));
        ui.label(format!(
            "Active conflict: {}",
            if has_conflict { "yes" } else { "no" }
        ));
        ui.label(format!("Intel observers: {}", sys.intel.by_observer.len()));
        ui.label(format!(
            "Archetype set: {}",
            if has_archetype { "yes" } else { "no" }
        ));
        ui.horizontal(|ui| {
            if ui
                .button("Open REGIONS tab  →")
                .on_hover_text("Jump to the REGIONS tab to manage map overlays")
                .clicked()
            {
                state.focus_entity(EntityRef::Tab(BuilderTab::Regions));
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use identity::apply_coord_move;
    use preview::handle_system_view_click;
    use sectorforge::sector_model::HexCoord;
    use sectorforge_gui_core::system_view::SystemClick;

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
        state.selection.systems.insert(a.clone());
        state.selection.systems.insert(b.clone());
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
        state.selection.systems.insert(a.clone());
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
        state.selection.systems.insert(a.clone());
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
        state.selection.system_id = Some(a);
        let ctx = egui::Context::default();
        let raw = egui::RawInput::default();
        let _ = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let sys_idx = state
                    .sector
                    .systems
                    .iter()
                    .position(|s| Some(&s.id) == state.selection.system_id.as_ref())
                    .unwrap();
                show_system_map_section(ui, &mut state, sys_idx);
            });
        });
        assert!(state.selection.world_id.is_none());
        assert!(state.selection.scroll_target.is_none());
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
        assert_eq!(state.selection.world_id.as_ref(), Some(&world));
        assert!(state.selection.scroll_target.is_none());

        handle_system_view_click(&mut state, sys_idx, SystemClick::Star);
        assert_eq!(state.selection.scroll_target, Some(SYS_STAR_GRID_ANCHOR));
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
        assert!(matches!(state.feedback.modal, Some(ModalKind::Message(_))));
    }
}
