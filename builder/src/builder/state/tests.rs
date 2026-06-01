//! Determinism + undo/redo + ring-buffer + debounce + nav-default tests for
//! [`super::BuilderState`].

use super::nav::EntityRef;
use super::types::{BuilderTab, HealthLevel, MapTool, DEFAULT_COMMAND_LOG_CAPACITY};
use super::BuilderState;
use crate::builder::command::BuilderCommand;
use sectorforge::ids::{FactionId, SystemId, WorldId};
use sectorforge::sector_model::HexCoord;
use std::time::Duration;

fn add_n_systems(state: &mut BuilderState, n: u32) {
    let base = state.sector.systems.len() as u32;
    for k in 0..n {
        let i = base + k;
        state
            .run(BuilderCommand::AddSystem {
                coord: HexCoord {
                    q: (i % 8) as i32,
                    r: (i / 8) as i32,
                },
                name: format!("sys-{i}"),
                result_id: None,
            })
            .unwrap();
    }
}

#[test]
fn ring_buffer_caps_command_log() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 64, 64);
    state.command_log_capacity = 5;
    add_n_systems(&mut state, 12);
    assert_eq!(state.command_log.len(), 5);
    assert_eq!(state.command_cursor, 5);
    assert_eq!(state.sector.systems.len(), 12);
}

#[test]
fn ring_buffer_shifts_snapshot_positions() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 64, 64);
    state.command_log_capacity = 4;
    add_n_systems(&mut state, 2);
    state.snapshot("after-2");
    let snap_pos_before = state.snapshots[0].command_log_position;
    assert_eq!(snap_pos_before, 2);
    add_n_systems(&mut state, 6);
    assert_eq!(state.command_log.len(), 4);
    assert_eq!(state.snapshots[0].command_log_position, 0);
}

#[test]
fn unbounded_capacity_zero_keeps_all_commands() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 64, 64);
    state.command_log_capacity = 0;
    add_n_systems(&mut state, 50);
    assert_eq!(state.command_log.len(), 50);
}

#[test]
fn default_capacity_is_200() {
    let state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    assert_eq!(state.command_log_capacity, DEFAULT_COMMAND_LOG_CAPACITY);
    assert_eq!(DEFAULT_COMMAND_LOG_CAPACITY, 200);
}

#[test]
fn undo_redo_basic_round_trip() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    add_n_systems(&mut state, 3);
    assert_eq!(state.sector.systems.len(), 3);
    state.undo().unwrap();
    assert_eq!(state.sector.systems.len(), 2);
    assert_eq!(state.command_cursor, 2);
    state.redo().unwrap();
    assert_eq!(state.sector.systems.len(), 3);
    assert_eq!(state.command_cursor, 3);
}

#[test]
fn undo_clamps_at_zero() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    state.undo().unwrap();
    assert_eq!(state.command_cursor, 0);
}

// ── §V3 ──────────────────────────────────────────────────────────────

#[test]
fn mutation_arms_validation_debounce() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    assert!(state.validation_dirty_since.is_none());
    add_n_systems(&mut state, 1);
    assert!(state.validation_dirty_since.is_some());
}

#[test]
fn pump_validation_holds_within_debounce_window() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    state.validation_debounce = Duration::from_secs(5);
    add_n_systems(&mut state, 1);
    assert!(!state.pump_validation());
    assert!(state.validation_dirty_since.is_some());
}

#[test]
fn pump_validation_flushes_after_debounce() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    state.validation_debounce = Duration::from_millis(0);
    add_n_systems(&mut state, 1);
    // No worlds catalog => synth returns None; debounce still clears so
    // we don't burn cycles every frame.
    assert!(state.pump_validation());
    assert!(state.validation_dirty_since.is_none());
}

#[test]
fn revalidate_now_populates_report_when_worlds_present() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    state.data_catalogs.worlds = Some(sectorforge::worlds_toml::WorldsConfig::default());
    state.revalidate_now();
    assert!(state.validation_report.is_some(), "report should be set");
}

#[test]
fn health_level_red_on_invariant_violation() {
    use sectorforge::invariants::{InvariantReport, InvariantViolation};
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    state.invariant_report = Some(InvariantReport {
        ok: false,
        violations: vec![InvariantViolation {
            code: "X".into(),
            message: "m".into(),
            path: None,
        }],
    });
    assert_eq!(state.health_level(), HealthLevel::Red);
}

#[test]
fn health_level_yellow_when_reports_missing() {
    let state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    assert_eq!(state.health_level(), HealthLevel::Yellow);
}

// ── §N1 / §N3 ────────────────────────────────────────────────────────

#[test]
fn default_tab_is_project() {
    let state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    assert_eq!(state.active_tab, BuilderTab::Project);
}

#[test]
fn default_map_tool_is_select() {
    let state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    assert_eq!(state.map_tool, MapTool::Select);
}

#[test]
fn builder_tab_all_is_full_n1_set() {
    // §N1 lists 24 tabs (PROJECT..EXPORT).
    assert_eq!(BuilderTab::ALL.len(), 24);
    assert_eq!(BuilderTab::ALL[0], BuilderTab::Project);
    assert_eq!(BuilderTab::ALL[1], BuilderTab::Map);
    assert_eq!(*BuilderTab::ALL.last().unwrap(), BuilderTab::Export);
}

#[test]
fn builder_tab_labels_are_uppercase_words() {
    for tab in BuilderTab::ALL {
        let label = tab.label();
        assert!(!label.is_empty());
        assert_eq!(label, label.to_uppercase());
    }
}

#[test]
fn health_level_green_when_both_clean() {
    use sectorforge::invariants::InvariantReport;
    use sectorforge::validation::{ValidationReport, WorldWorkbookValidation};
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    state.invariant_report = Some(InvariantReport {
        ok: true,
        violations: Vec::new(),
    });
    state.validation_report = Some(ValidationReport {
        ok: true,
        errors: Vec::new(),
        warnings: Vec::new(),
        world_workbook: WorldWorkbookValidation {
            row_count: 0,
            usable_candidate_count: 0,
            excluded_row_count: 0,
            exclusion_reasons: Default::default(),
            key_table_counts: Default::default(),
        },
    });
    assert_eq!(state.health_level(), HealthLevel::Green);
}

fn warn_validation() -> sectorforge::validation::ValidationReport {
    use sectorforge::validation::{
        Severity, ValidationIssue, ValidationReport, WorldWorkbookValidation,
    };
    ValidationReport {
        // errors empty ⇒ `ok` stays true even with a warning present.
        ok: true,
        errors: Vec::new(),
        warnings: vec![ValidationIssue {
            code: "W".into(),
            message: "w".into(),
            path: None,
            row: None,
            severity: Severity::Warning,
        }],
        world_workbook: WorldWorkbookValidation {
            row_count: 0,
            usable_candidate_count: 0,
            excluded_row_count: 0,
            exclusion_reasons: Default::default(),
            key_table_counts: Default::default(),
        },
    }
}

fn clean_invariant() -> sectorforge::invariants::InvariantReport {
    sectorforge::invariants::InvariantReport {
        ok: true,
        violations: Vec::new(),
    }
}

#[test]
fn health_level_yellow_on_warnings_without_strict() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    state.validation_report = Some(warn_validation());
    state.invariant_report = Some(clean_invariant());
    state.validation_strict = false;
    assert_eq!(state.health_level(), HealthLevel::Yellow);
}

#[test]
fn health_level_red_when_strict_and_warnings() {
    // §V4: strict mode promotes validation warnings to errors → Red pip.
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    state.validation_report = Some(warn_validation());
    state.invariant_report = Some(clean_invariant());
    state.validation_strict = true;
    assert_eq!(state.health_level(), HealthLevel::Red);
}

#[test]
fn export_block_reason_none_on_clean_blank_sector() {
    // §V6: a blank, valid sector must be exportable (gate returns None).
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    assert_eq!(state.export_block_reason(), None);
}

// ── §LINK navigation tests ────────────────────────────────────────────────

fn sid(s: &str) -> SystemId {
    SystemId::new(s)
}

#[test]
fn focus_entity_sets_tab_and_selection_per_variant() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);

    s.focus_entity(EntityRef::System(sid("sys-0001")));
    assert_eq!(s.active_tab, BuilderTab::System);
    assert_eq!(s.selected_system_id, Some(sid("sys-0001")));

    s.focus_entity(EntityRef::Faction(FactionId::new("imperium")));
    assert_eq!(s.active_tab, BuilderTab::Factions);
    assert_eq!(s.selected_faction_id, Some(FactionId::new("imperium")));

    s.focus_entity(EntityRef::Tab(BuilderTab::Map));
    assert_eq!(s.active_tab, BuilderTab::Map);

    s.focus_entity(EntityRef::Region("warp-storm-1".into()));
    assert_eq!(s.active_tab, BuilderTab::Regions);
    assert_eq!(s.selected_region_id.as_deref(), Some("warp-storm-1"));

    s.focus_entity(EntityRef::Subsector("sub-A".into()));
    assert_eq!(s.active_tab, BuilderTab::Subsectors);
    assert_eq!(s.selected_subsector_id.as_deref(), Some("sub-A"));

    s.focus_entity(EntityRef::HistoryEvent("ev-1".into()));
    assert_eq!(s.active_tab, BuilderTab::History);
    assert_eq!(s.selected_history_event.as_deref(), Some("ev-1"));

    s.focus_entity(EntityRef::Persona("p-1".into()));
    assert_eq!(s.active_tab, BuilderTab::Personae);
    assert_eq!(s.selected_persona_id.as_deref(), Some("p-1"));

    s.focus_entity(EntityRef::Hook("h-1".into()));
    assert_eq!(s.active_tab, BuilderTab::Hooks);
    assert_eq!(s.selected_hook_id.as_deref(), Some("h-1"));
}

#[test]
fn focus_entity_world_sets_both_system_and_world() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    s.focus_entity(EntityRef::World {
        system: sid("sys-0042"),
        world: WorldId::new("sys-0042-w01"),
    });
    assert_eq!(s.active_tab, BuilderTab::World);
    assert_eq!(s.selected_system_id, Some(sid("sys-0042")));
    assert_eq!(s.selected_world_id, Some(WorldId::new("sys-0042-w01")));
}

#[test]
fn focus_entity_pushes_back_stack() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    s.focus_entity(EntityRef::System(sid("sys-alpha")));
    s.focus_entity(EntityRef::Faction(FactionId::new("imperium")));
    // Stack contains the initial Tab(Project) snapshot plus System(alpha).
    assert_eq!(s.nav_back_stack.len(), 2);
    s.nav_back();
    assert_eq!(s.active_tab, BuilderTab::System);
    assert_eq!(s.selected_system_id, Some(sid("sys-alpha")));
    assert_eq!(s.nav_forward_stack.len(), 1);
}

#[test]
fn focus_entity_is_idempotent() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    s.focus_entity(EntityRef::Faction(FactionId::new("imperium")));
    let before = s.nav_back_stack.len();
    s.focus_entity(EntityRef::Faction(FactionId::new("imperium")));
    assert_eq!(s.nav_back_stack.len(), before);
}

#[test]
fn back_stack_caps_at_64() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    for i in 0..100 {
        s.focus_entity(EntityRef::Faction(FactionId::new(format!("f-{i}"))));
    }
    assert_eq!(s.nav_back_stack.len(), 64);
}

#[test]
fn forward_stack_clears_on_new_focus() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    s.focus_entity(EntityRef::System(sid("a")));
    s.focus_entity(EntityRef::System(sid("b")));
    s.nav_back();
    assert_eq!(s.nav_forward_stack.len(), 1);
    s.focus_entity(EntityRef::System(sid("c")));
    assert!(s.nav_forward_stack.is_empty());
}

#[test]
fn nav_back_on_empty_stack_is_noop() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    let tab = s.active_tab;
    s.nav_back();
    assert_eq!(s.active_tab, tab);
}

// ── TF-T-13: BuilderCommand round-trip coverage ──────────────────────────

/// Build a tiny seeded sector with one system, one world on it, and a
/// neighbouring system + route — enough for most command variants to operate
/// against without each test reinventing the fixture.
fn seeded() -> BuilderState {
    let mut s = BuilderState::new_blank("t", "T", "seed", 16, 16);
    s.run(BuilderCommand::AddSystem {
        coord: HexCoord { q: 0, r: 0 },
        name: "alpha".into(),
        result_id: None,
    })
    .unwrap();
    s.run(BuilderCommand::AddSystem {
        coord: HexCoord { q: 1, r: 0 },
        name: "beta".into(),
        result_id: None,
    })
    .unwrap();
    s
}

fn assert_round_trip(mut state: BuilderState, cmd: BuilderCommand) {
    // GeneratedSector intentionally has no PartialEq derive (its leaf types
    // include f32). Compare via canonical JSON instead — same byte-stability
    // guarantee the golden tests rely on.
    let snapshot = |s: &BuilderState| serde_json::to_string(&s.sector).unwrap();
    let before = snapshot(&state);
    state.run(cmd).unwrap();
    let after = snapshot(&state);
    assert_ne!(before, after, "command should produce a state change");
    state.undo().unwrap();
    assert_eq!(snapshot(&state), before, "undo should restore prior sector");
    state.redo().unwrap();
    assert_eq!(
        snapshot(&state),
        after,
        "redo should reapply the same change"
    );
}

#[test]
fn round_trip_add_system() {
    let s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    assert_round_trip(
        s,
        BuilderCommand::AddSystem {
            coord: HexCoord { q: 2, r: 2 },
            name: "gamma".into(),
            result_id: None,
        },
    );
}

#[test]
fn round_trip_remove_system() {
    let mut s = seeded();
    let id = s.sector.systems[0].id.clone();
    let count = s.sector.systems.len();
    s.run(BuilderCommand::RemoveSystem {
        id: id.clone(),
        before: None,
        removed_routes: Vec::new(),
    })
    .unwrap();
    assert_eq!(s.sector.systems.len(), count - 1);
    assert!(s.sector.systems.iter().all(|x| x.id != id));
    s.undo().unwrap();
    // Restored set contains the removed id again (order may differ — undo
    // re-inserts at the tail, not the original index).
    assert_eq!(s.sector.systems.len(), count);
    assert!(s.sector.systems.iter().any(|x| x.id == id));
    s.redo().unwrap();
    assert_eq!(s.sector.systems.len(), count - 1);
    assert!(s.sector.systems.iter().all(|x| x.id != id));
}

#[test]
fn round_trip_move_system() {
    let s = seeded();
    let id = s.sector.systems[0].id.clone();
    let from_coord = s.sector.systems[0].coord;
    assert_round_trip(
        s,
        BuilderCommand::MoveSystem {
            id,
            from: from_coord,
            to: HexCoord { q: 4, r: 4 },
        },
    );
}

#[test]
fn round_trip_rename_system() {
    let s = seeded();
    let id = s.sector.systems[0].id.clone();
    let from = s.sector.systems[0].name.as_ref().to_string();
    assert_round_trip(
        s,
        BuilderCommand::RenameSystem {
            id,
            from,
            to: "alpha-renamed".into(),
        },
    );
}

#[test]
fn round_trip_swap_systems() {
    let s = seeded();
    let a = s.sector.systems[0].id.clone();
    let b = s.sector.systems[1].id.clone();
    assert_round_trip(s, BuilderCommand::SwapSystems { a, b });
}

#[test]
fn round_trip_add_route() {
    let s = seeded();
    let from = s.sector.systems[0].id.clone();
    let to = s.sector.systems[1].id.clone();
    assert_round_trip(
        s,
        BuilderCommand::AddRoute {
            from,
            to,
            route_type: sectorforge::sector_model::RouteType::StableWarpLane,
            stability: sectorforge::sector_model::RouteStability::Stable,
            result_id: None,
        },
    );
}

#[test]
fn round_trip_remove_route() {
    let mut s = seeded();
    let from = s.sector.systems[0].id.clone();
    let to = s.sector.systems[1].id.clone();
    s.run(BuilderCommand::AddRoute {
        from,
        to,
        route_type: sectorforge::sector_model::RouteType::StableWarpLane,
        stability: sectorforge::sector_model::RouteStability::Stable,
        result_id: None,
    })
    .unwrap();
    let route_id = s.sector.routes[0].id.clone();
    assert_round_trip(
        s,
        BuilderCommand::RemoveRoute {
            id: route_id,
            before: None,
        },
    );
}

#[test]
fn round_trip_add_faction() {
    let s = seeded();
    assert_round_trip(
        s,
        BuilderCommand::AddFaction {
            id: FactionId::new("imperium"),
            name: "Imperium".into(),
            kind: "imperial".into(),
        },
    );
}

#[test]
fn round_trip_remove_faction() {
    let mut s = seeded();
    s.run(BuilderCommand::AddFaction {
        id: FactionId::new("imperium"),
        name: "Imperium".into(),
        kind: "imperial".into(),
    })
    .unwrap();
    assert_round_trip(
        s,
        BuilderCommand::RemoveFaction {
            id: FactionId::new("imperium"),
            before: None,
        },
    );
}

#[test]
fn round_trip_set_route_type() {
    let mut s = seeded();
    let from = s.sector.systems[0].id.clone();
    let to = s.sector.systems[1].id.clone();
    s.run(BuilderCommand::AddRoute {
        from,
        to,
        route_type: sectorforge::sector_model::RouteType::StableWarpLane,
        stability: sectorforge::sector_model::RouteStability::Stable,
        result_id: None,
    })
    .unwrap();
    let id = s.sector.routes[0].id.clone();
    let before = s.sector.routes[0].route_type;
    assert_round_trip(
        s,
        BuilderCommand::SetRouteType {
            id,
            before,
            after: sectorforge::sector_model::RouteType::ChartedPassage,
        },
    );
}

#[test]
fn round_trip_set_route_stability() {
    let mut s = seeded();
    let from = s.sector.systems[0].id.clone();
    let to = s.sector.systems[1].id.clone();
    s.run(BuilderCommand::AddRoute {
        from,
        to,
        route_type: sectorforge::sector_model::RouteType::StableWarpLane,
        stability: sectorforge::sector_model::RouteStability::Stable,
        result_id: None,
    })
    .unwrap();
    let id = s.sector.routes[0].id.clone();
    let before = s.sector.routes[0].stability;
    assert_round_trip(
        s,
        BuilderCommand::SetRouteStability {
            id,
            before,
            after: sectorforge::sector_model::RouteStability::Hazardous,
        },
    );
}
