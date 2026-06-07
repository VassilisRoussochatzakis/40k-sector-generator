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
    assert!(state.feedback.validation_dirty_since.is_none());
    add_n_systems(&mut state, 1);
    assert!(state.feedback.validation_dirty_since.is_some());
}

#[test]
fn pump_validation_holds_within_debounce_window() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    state.feedback.validation_debounce = Duration::from_secs(5);
    add_n_systems(&mut state, 1);
    assert!(!state.pump_validation());
    assert!(state.feedback.validation_dirty_since.is_some());
}

#[test]
fn pump_validation_flushes_after_debounce() {
    let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
    state.feedback.validation_debounce = Duration::from_millis(0);
    add_n_systems(&mut state, 1);
    // No worlds catalog => synth returns None; debounce still clears so
    // we don't burn cycles every frame.
    assert!(state.pump_validation());
    assert!(state.feedback.validation_dirty_since.is_none());
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
    assert_eq!(state.map_view.tool, MapTool::Select);
}

#[test]
fn builder_tab_all_is_full_n1_set() {
    // §N1 lists 24 working tabs (PROJECT..EXPORT) plus the two XC-1 diagnostics
    // tabs (VALIDATION, INVARIANTS) appended at the right edge → 26 total.
    assert_eq!(BuilderTab::ALL.len(), 26);
    assert_eq!(BuilderTab::ALL[0], BuilderTab::Project);
    assert_eq!(BuilderTab::ALL[1], BuilderTab::Map);
    assert_eq!(BuilderTab::ALL[23], BuilderTab::Export);
    assert_eq!(*BuilderTab::ALL.last().unwrap(), BuilderTab::Invariants);
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
    assert_eq!(s.selection.system_id, Some(sid("sys-0001")));

    s.focus_entity(EntityRef::Faction(FactionId::new("imperium")));
    assert_eq!(s.active_tab, BuilderTab::Factions);
    assert_eq!(s.selection.faction_id, Some(FactionId::new("imperium")));

    s.focus_entity(EntityRef::Tab(BuilderTab::Map));
    assert_eq!(s.active_tab, BuilderTab::Map);

    s.focus_entity(EntityRef::Region("warp-storm-1".into()));
    assert_eq!(s.active_tab, BuilderTab::Regions);
    assert_eq!(s.selection.region_id.as_deref(), Some("warp-storm-1"));

    s.focus_entity(EntityRef::Subsector("sub-A".into()));
    assert_eq!(s.active_tab, BuilderTab::Subsectors);
    assert_eq!(s.selection.subsector_id.as_deref(), Some("sub-A"));

    s.focus_entity(EntityRef::HistoryEvent("ev-1".into()));
    assert_eq!(s.active_tab, BuilderTab::History);
    assert_eq!(s.selection.history_event.as_deref(), Some("ev-1"));

    s.focus_entity(EntityRef::Persona("p-1".into()));
    assert_eq!(s.active_tab, BuilderTab::Personae);
    assert_eq!(s.selection.persona_id.as_deref(), Some("p-1"));

    s.focus_entity(EntityRef::Hook("h-1".into()));
    assert_eq!(s.active_tab, BuilderTab::Hooks);
    assert_eq!(s.selection.hook_id.as_deref(), Some("h-1"));
}

#[test]
fn focus_entity_world_sets_both_system_and_world() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    s.focus_entity(EntityRef::World {
        system: sid("sys-0042"),
        world: WorldId::new("sys-0042-w01"),
    });
    assert_eq!(s.active_tab, BuilderTab::World);
    assert_eq!(s.selection.system_id, Some(sid("sys-0042")));
    assert_eq!(s.selection.world_id, Some(WorldId::new("sys-0042-w01")));
}

#[test]
fn focus_entity_pushes_back_stack() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    s.focus_entity(EntityRef::System(sid("sys-alpha")));
    s.focus_entity(EntityRef::Faction(FactionId::new("imperium")));
    // Stack contains the initial Tab(Project) snapshot plus System(alpha).
    assert_eq!(s.selection.nav_back_stack.len(), 2);
    s.nav_back();
    assert_eq!(s.active_tab, BuilderTab::System);
    assert_eq!(s.selection.system_id, Some(sid("sys-alpha")));
    assert_eq!(s.selection.nav_forward_stack.len(), 1);
}

#[test]
fn focus_entity_is_idempotent() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    s.focus_entity(EntityRef::Faction(FactionId::new("imperium")));
    let before = s.selection.nav_back_stack.len();
    s.focus_entity(EntityRef::Faction(FactionId::new("imperium")));
    assert_eq!(s.selection.nav_back_stack.len(), before);
}

#[test]
fn back_stack_caps_at_64() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    for i in 0..100 {
        s.focus_entity(EntityRef::Faction(FactionId::new(format!("f-{i}"))));
    }
    assert_eq!(s.selection.nav_back_stack.len(), 64);
}

#[test]
fn forward_stack_clears_on_new_focus() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 8, 8);
    s.focus_entity(EntityRef::System(sid("a")));
    s.focus_entity(EntityRef::System(sid("b")));
    s.nav_back();
    assert_eq!(s.selection.nav_forward_stack.len(), 1);
    s.focus_entity(EntityRef::System(sid("c")));
    assert!(s.selection.nav_forward_stack.is_empty());
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

// ── §39 live derivations (LD1..LD4) ─────────────────────────────────────────

use crate::builder::{DepClass, DerivationKind};

/// LD1 — the input fingerprint is stable for an unchanged sector and shifts
/// when a systems/worlds edit lands.
#[test]
fn ld1_fingerprint_tracks_sector_inputs() {
    let mut s = seeded();
    let before = s.derivation_fingerprint(DerivationKind::Hooks);
    assert_eq!(
        before,
        s.derivation_fingerprint(DerivationKind::Hooks),
        "fingerprint is deterministic for an unchanged sector"
    );
    s.run(BuilderCommand::AddSystem {
        coord: HexCoord { q: 2, r: 0 },
        name: "gamma".into(),
        result_id: None,
    })
    .unwrap();
    assert_ne!(
        before,
        s.derivation_fingerprint(DerivationKind::Hooks),
        "a systems/worlds edit changes the Hooks input fingerprint"
    );
}

/// LD2 — a command-bus mutation marks exactly the derived overlays downstream
/// of its `dep_classes` stale, and leaves unrelated ones fresh.
#[test]
fn ld2_route_edit_invalidates_precisely() {
    let mut s = seeded();
    // Make both overlays "derived" so they are eligible for staleness.
    s.recompute_hooks();
    s.recompute_personae();
    assert!(!s.derivations.is_stale(DerivationKind::Hooks));
    assert!(!s.derivations.is_stale(DerivationKind::Personae));

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

    // Routes → {analytics, economy, hooks}; personae is untouched.
    assert!(
        s.derivations.is_stale(DerivationKind::Hooks),
        "hooks read routes, so a route edit stales them"
    );
    assert!(
        !s.derivations.is_stale(DerivationKind::Personae),
        "personae do not read routes, so they stay fresh"
    );
}

/// LD2 — a world edit (SystemsWorlds) fans out to every derived overlay.
#[test]
fn ld2_world_edit_invalidates_all_derived() {
    let mut s = seeded();
    s.recompute_hooks();
    s.recompute_personae();
    s.run(BuilderCommand::RenameSystem {
        id: s.sector.systems[0].id.clone(),
        from: "alpha".into(),
        to: "alpha-prime".into(),
    })
    .unwrap();
    assert!(s.derivations.is_stale(DerivationKind::Hooks));
    assert!(s.derivations.is_stale(DerivationKind::Personae));
}

/// LD3/LD4 — `pump_derivations` re-derives the active tab's stale overlay so
/// the panel about to paint reads a live value; off-tab overlays stay stale.
#[test]
fn ld4_pump_refreshes_active_tab_only() {
    let mut s = seeded();
    s.recompute_hooks();
    s.recompute_personae();
    s.run(BuilderCommand::AddSystem {
        coord: HexCoord { q: 3, r: 0 },
        name: "delta".into(),
        result_id: None,
    })
    .unwrap();
    assert!(s.derivations.is_stale(DerivationKind::Hooks));
    assert!(s.derivations.is_stale(DerivationKind::Personae));

    s.active_tab = BuilderTab::Hooks;
    s.pump_derivations();
    assert!(
        !s.derivations.is_stale(DerivationKind::Hooks),
        "the active tab's overlay is refreshed by the pump"
    );
    assert!(
        s.derivations.is_stale(DerivationKind::Personae),
        "an off-tab overlay stays stale until visited"
    );
}

/// LD2 — `invalidate_derivations` honours a panel-supplied catalog-config
/// class (relations.toml → relations, briefing) without a command-bus mutation.
#[test]
fn ld2_catalog_class_invalidation() {
    let mut s = seeded();
    // Pretend briefing + relations were derived this session.
    s.mark_derivation_fresh(DerivationKind::Briefing);
    s.mark_derivation_fresh(DerivationKind::Relations);
    s.invalidate_derivations(&[DepClass::RelationsCfg]);
    assert!(s.derivations.is_stale(DerivationKind::Briefing));
    assert!(s.derivations.is_stale(DerivationKind::Relations));
    assert!(!s.derivations.is_stale(DerivationKind::Hooks));
}

/// LD2 — a full-sector swap (snapshot revert) marks every previously-derived
/// overlay stale.
#[test]
fn ld2_snapshot_revert_invalidates_all_derived() {
    let mut s = seeded();
    s.recompute_hooks();
    s.snapshot("base");
    s.run(BuilderCommand::AddSystem {
        coord: HexCoord { q: 4, r: 0 },
        name: "epsilon".into(),
        result_id: None,
    })
    .unwrap();
    s.mark_derivation_fresh(DerivationKind::Hooks); // re-derive after the edit
    assert!(s.revert_to_snapshot("base"));
    assert!(
        s.derivations.is_stale(DerivationKind::Hooks),
        "reverting the sector invalidates the derived overlay"
    );
}

// ── §E-S3: edit_world / edit_system clone-mutate-dispatch helpers ─────────

/// The helper clones the target, runs the closure, and commits one
/// `EditWorld` — so the edit lands on the undo log (one command) and undo
/// restores the prior payload. A stale id surfaces `WorldNotFound`, matching
/// what `run(EditWorld)` returns directly.
#[test]
fn edit_world_round_trip() {
    let mut s = seeded();
    let sys_id = s.sector.systems[0].id.clone();
    s.run(BuilderCommand::AddWorld {
        system: sys_id,
        name: "terra".into(),
        result_id: None,
    })
    .unwrap();
    let wid = s.sector.systems[0].worlds.last().unwrap().id.clone();
    let log_before = s.command_log.len();

    s.edit_world(wid.clone(), |w| w.tags = vec!["hive".into()])
        .unwrap();
    assert_eq!(
        s.command_log.len(),
        log_before + 1,
        "edit_world commits exactly one undoable command"
    );
    let (si, wi) = s.find_world_indices(&wid).unwrap();
    assert_eq!(s.sector.systems[si].worlds[wi].tags.len(), 1);

    s.undo().unwrap();
    let (si, wi) = s.find_world_indices(&wid).unwrap();
    assert!(
        s.sector.systems[si].worlds[wi].tags.is_empty(),
        "undo restores the pre-edit world"
    );

    let err = s.edit_world(WorldId::new("no-such-world"), |_| {}).unwrap_err();
    assert!(matches!(
        err,
        crate::builder::errors::BuilderError::Mutation(
            sectorforge::sector_model::mutation::MutationError::WorldNotFound(_)
        )
    ));
}

/// System counterpart: one `EditSystem` per call, undoable, stale id →
/// `SystemNotFound`.
#[test]
fn edit_system_round_trip() {
    let mut s = seeded();
    let sys_id = s.sector.systems[0].id.clone();
    let log_before = s.command_log.len();

    s.edit_system(sys_id.clone(), |sys| sys.notes = vec!["pinned".into()])
        .unwrap();
    assert_eq!(
        s.command_log.len(),
        log_before + 1,
        "edit_system commits exactly one undoable command"
    );
    let idx = s.system_index_by_id(&sys_id).unwrap();
    assert_eq!(s.sector.systems[idx].notes.len(), 1);

    s.undo().unwrap();
    let idx = s.system_index_by_id(&sys_id).unwrap();
    assert!(
        s.sector.systems[idx].notes.is_empty(),
        "undo restores the pre-edit system"
    );

    let err = s
        .edit_system(SystemId::new("no-such-system"), |_| {})
        .unwrap_err();
    assert!(matches!(
        err,
        crate::builder::errors::BuilderError::Mutation(
            sectorforge::sector_model::mutation::MutationError::SystemNotFound(_)
        )
    ));
}

// ── §TF-T-13b: command bus + live-derivation deep coverage (gaps 194-213) ────
//
// These tests drive the remaining `BuilderCommand` apply/revert paths, the
// state-level recompute / ensure_fresh / dispatch machinery, the §R4 transient
// selection ops, and the run()/undo()/redo() bookkeeping invariants. Document
// mutations go through `state.run(BuilderCommand::..)`; `#[cfg(test)]` may bypass
// the bus only to *build* fixtures (e.g. `s.sector.factions.push(..)`), never to
// mutate document state under test. Sectors stay square. GeneratedSector has no
// PartialEq (leaf f32), so state equality is compared via canonical JSON.

use super::types::TickLogScope;
use crate::builder::DerivationStatus;
use sectorforge::sector_model::{GeneratedFaction, GeneratedSector};

/// Minimal faction for the relations/personae fixtures (mirrors the helper in
/// `derivations::ld3_background_tests`). Bypasses the bus to seed `sector`.
fn gen_faction(id: &str, kind: &str, disposition: &str) -> GeneratedFaction {
    GeneratedFaction {
        id: id.into(),
        name: std::sync::Arc::from(id),
        kind: std::sync::Arc::from(kind),
        disposition: std::sync::Arc::from(disposition),
        subfactions: Vec::new(),
        system_presence: vec![],
        world_presence: vec![],
        power: Default::default(),
    }
}

/// Bounded poll of the background-job drain pump so a slow worker thread cannot
/// hang the test. Copied from `derivations::ld3_background_tests` — always run
/// to completion so worker threads are joined before the test exits.
fn drain_until_done(state: &mut BuilderState, kind: DerivationKind) -> bool {
    for _ in 0..200 {
        state.pump_derivation_jobs();
        if !state.derivations.deriving.contains(&kind)
            && !state.derivation_jobs.has_in_flight(kind)
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

/// Gap 194 (H) — `RemoveWorld` revert re-inserts at the *original* index, not
/// the tail. The middle world of three must come back at index 1, and `apply`
/// must have captured `parent_position = Some(1)`.
#[test]
fn remove_world_revert_restores_original_index() {
    let mut s = seeded();
    let sys = s.sector.systems[0].id.clone();
    for n in ["w0", "w1", "w2"] {
        s.run(BuilderCommand::AddWorld {
            system: sys.clone(),
            name: n.into(),
            result_id: None,
        })
        .unwrap();
    }
    let w1 = s.sector.systems[0].worlds[1].id.clone(); // the MIDDLE world

    s.run(BuilderCommand::RemoveWorld {
        world: w1.clone(),
        before: None,
        parent_system: None,
        parent_position: None,
    })
    .unwrap();
    assert_eq!(s.sector.systems[0].worlds.len(), 2);
    assert!(s.sector.systems[0].worlds.iter().all(|w| w.id != w1));

    // apply captured the original position.
    let BuilderCommand::RemoveWorld {
        parent_position, ..
    } = &s.command_log[s.command_cursor - 1]
    else {
        panic!("last command should be RemoveWorld");
    };
    assert_eq!(*parent_position, Some(1));

    s.undo().unwrap();
    assert_eq!(s.sector.systems[0].worlds.len(), 3);
    assert_eq!(
        s.sector.systems[0].worlds[1].id, w1,
        "undo re-inserts the removed world at its original index 1, not appended at the tail"
    );
}

/// Gap 195 (H) — `AdvanceConflictTicks` apply/revert. ticks=0 is a no-op on the
/// sector JSON (but still captures before-vecs + pushes a command); ticks=3
/// advances the world's conflict age and revert restores conflict + dominant.
#[test]
fn advance_conflict_ticks_command_apply_revert() {
    // ── ticks=0 sub-case: sector JSON unchanged ──
    {
        let mut s = seeded();
        let sys = s.sector.systems[0].id.clone();
        s.run(BuilderCommand::AddWorld {
            system: sys,
            name: "w".into(),
            result_id: None,
        })
        .unwrap();
        let before = serde_json::to_string(&s.sector).unwrap();
        s.run(BuilderCommand::AdvanceConflictTicks {
            ticks: 0,
            before_world: vec![],
            before_system: vec![],
            before_dominant: vec![],
        })
        .unwrap();
        assert_eq!(
            serde_json::to_string(&s.sector).unwrap(),
            before,
            "ticks=0 advances nothing, so the sector JSON is byte-identical"
        );
    }

    // ── ticks=3 sub-case: world age advances; undo restores ──
    {
        let mut s = seeded();
        let sys = s.sector.systems[0].id.clone();
        s.run(BuilderCommand::AddWorld {
            system: sys,
            name: "w".into(),
            result_id: None,
        })
        .unwrap();
        let before = serde_json::to_string(&s.sector).unwrap();
        s.run(BuilderCommand::AdvanceConflictTicks {
            ticks: 3,
            before_world: vec![],
            before_system: vec![],
            before_dominant: vec![],
        })
        .unwrap();
        assert_eq!(
            s.sector.systems[0].worlds[0].conflict.age, 3,
            "advance_one increments age once per tick"
        );

        // The before-vecs were captured on apply (one system, one world).
        let BuilderCommand::AdvanceConflictTicks {
            before_world,
            before_system,
            before_dominant,
            ..
        } = &s.command_log[s.command_cursor - 1]
        else {
            panic!("last command should be AdvanceConflictTicks");
        };
        // `seeded()` builds two systems (alpha + beta); apply snapshots one
        // entry per system, plus one world entry (the single world on alpha).
        assert_eq!(before_system.len(), 2);
        assert_eq!(before_world.len(), 1);
        assert_eq!(before_dominant.len(), 2);

        s.undo().unwrap();
        assert_eq!(
            serde_json::to_string(&s.sector).unwrap(),
            before,
            "revert restores per-world conflict AND per-system control.dominant"
        );
    }
}

/// Gap 196 (H) — the `advance_conflict_ticks` wrapper: ticks=0 pushes no command
/// (early return); ticks>0 pushes exactly one `AdvanceConflictTicks` command and
/// is undoable.
#[test]
fn advance_conflict_ticks_wrapper_command_count() {
    let mut s = seeded();
    let sys = s.sector.systems[0].id.clone();
    s.run(BuilderCommand::AddWorld {
        system: sys,
        name: "w".into(),
        result_id: None,
    })
    .unwrap();

    let n = s.command_log.len();
    s.advance_conflict_ticks(0).unwrap();
    assert_eq!(
        s.command_log.len(),
        n,
        "ticks=0 returns early and pushes no command"
    );

    let before = serde_json::to_string(&s.sector).unwrap();
    let n = s.command_log.len();
    s.advance_conflict_ticks(2).unwrap();
    assert_eq!(
        s.command_log.len(),
        n + 1,
        "ticks>0 pushes exactly one AdvanceConflictTicks command"
    );
    s.undo().unwrap();
    assert_eq!(
        serde_json::to_string(&s.sector).unwrap(),
        before,
        "the wrapper's command is undoable"
    );
}

/// Gap 197 (H) — `run()` error path. A failing `apply` (unknown-id RemoveSystem)
/// returns the MutationError and leaves command_log / cursor / dirty / sector
/// untouched and the redo tail intact (the `.truncate` after `apply?` never runs).
#[test]
fn run_failed_apply_leaves_state_untouched() {
    let mut s = seeded(); // 2 AddSystem on the log, cursor=2
    s.run(BuilderCommand::AddSystem {
        coord: HexCoord { q: 2, r: 0 },
        name: "gamma".into(),
        result_id: None,
    })
    .unwrap(); // log=3, cursor=3
    s.undo().unwrap(); // cursor=2, redo tail = [gamma AddSystem]

    let log_len = s.command_log.len();
    let cursor = s.command_cursor;
    let dirty = s.dirty;
    let sector_json = serde_json::to_string(&s.sector).unwrap();

    let err = s
        .run(BuilderCommand::RemoveSystem {
            id: SystemId::new("no-such-system"),
            before: None,
            removed_routes: Vec::new(),
        })
        .unwrap_err();

    assert!(matches!(
        err,
        crate::builder::errors::BuilderError::Mutation(
            sectorforge::sector_model::mutation::MutationError::SystemNotFound(_)
        )
    ));
    assert_eq!(
        s.command_log.len(),
        log_len,
        "a failing run never reaches truncate, so the redo tail survives"
    );
    assert_eq!(s.command_cursor, cursor);
    assert_eq!(s.dirty, dirty);
    assert_eq!(serde_json::to_string(&s.sector).unwrap(), sector_json);

    // The redo tail still works: gamma comes back.
    s.redo().unwrap();
    assert_eq!(s.command_cursor, 3);
    assert!(s.sector.systems.iter().any(|x| &*x.name == "gamma"));
}

/// Gap 198 (H) — §R4 transient selection ops push NO command. focus / toggle /
/// set_active_tab / focus_entity / nav_back only touch view state; the command
/// log and cursor are untouched and a prior bus mutation still undoes cleanly.
#[test]
fn selection_ops_are_transient_no_command() {
    let mut s = seeded(); // 2 AddSystem on the log, cursor=2
    let sys0 = s.sector.systems[0].id.clone();
    let sys1 = s.sector.systems[1].id.clone();
    let log_len = s.command_log.len();
    let cursor = s.command_cursor;

    s.focus_system(sys0.clone());
    s.toggle_system_selection(sys1);
    s.set_active_tab(BuilderTab::Map);
    s.focus_entity(EntityRef::Faction(FactionId::new("imperium")));
    s.nav_back();

    assert_eq!(
        s.command_log.len(),
        log_len,
        "transient selection ops push no command"
    );
    assert_eq!(s.command_cursor, cursor);

    // The bus state was never disturbed: undo still reverts the prior AddSystem.
    let n_systems = s.sector.systems.len();
    s.undo().unwrap();
    assert_eq!(s.sector.systems.len(), n_systems - 1);
}

/// Gap 199 (H) — `ensure_fresh` with a stale flag but an unchanged fingerprint
/// clears the stale flag via the early `mark_fresh` branch WITHOUT recomputing
/// (the `hooks_report` sentinel set to None stays None).
#[test]
fn ensure_fresh_stale_but_matching_fingerprint_skips_recompute() {
    let mut s = seeded();
    s.recompute_hooks(); // fingerprints[Hooks] recorded; hooks_report = Some; not stale
    assert!(s.hooks_report.is_some());

    // Flag stale WITHOUT changing any Hooks input.
    s.derivations.stale.insert(DerivationKind::Hooks);
    assert!(s.derivations.is_stale(DerivationKind::Hooks));

    // Drop the report to a sentinel so we can detect whether recompute ran.
    s.hooks_report = None;
    s.ensure_fresh(DerivationKind::Hooks);

    assert!(
        !s.derivations.is_stale(DerivationKind::Hooks),
        "the early mark_fresh branch clears the stale flag"
    );
    assert!(
        s.hooks_report.is_none(),
        "fingerprint unchanged => no recompute => the sentinel stays None"
    );
}

/// Gap 200 (H) — `recompute_economy` cascade. It always invalidates EconomyCfg
/// (which stales Hooks). With `feed_stability=false` Personae stays fresh
/// (Personae.deps() excludes EconomyCfg); with `feed_stability=true` it ALSO
/// invalidates SystemsWorlds, which stales everything including Personae.
#[test]
fn recompute_economy_feed_stability_cascade() {
    // ── feed_stability = false (default: no economy catalog) ──
    {
        let mut s = seeded();
        s.recompute_hooks(); // prime so invalidate can flag them (cold kinds are skipped)
        s.recompute_personae();
        s.recompute_economy();
        assert!(
            s.derivations.is_stale(DerivationKind::Hooks),
            "the EconomyCfg cascade always stales Hooks"
        );
        assert!(
            !s.derivations.is_stale(DerivationKind::Personae),
            "Personae does not depend on EconomyCfg, so feed_stability=false leaves it fresh"
        );
    }

    // ── feed_stability = true ──
    {
        let mut s = seeded();
        s.recompute_hooks();
        s.recompute_personae();
        s.data_catalogs.economy = Some(sectorforge::economy::EconomyConfig {
            enabled: true,
            feed_stability: true,
            ..Default::default()
        });
        s.recompute_economy();
        assert!(s.derivations.is_stale(DerivationKind::Hooks));
        assert!(
            s.derivations.is_stale(DerivationKind::Personae),
            "feed_stability=true invalidates SystemsWorlds, which stales Personae"
        );
    }
}

/// Gap 201 (H) — undo AND redo re-invalidate exactly `cmd.dep_classes()`. An
/// AddRoute's dep_classes() == [Routes] => stales Hooks but not Personae, on
/// both the undo and redo legs. Recompute AFTER the edit so the fingerprint is
/// the post-edit one and the undo/redo slice change re-stales it.
#[test]
fn undo_redo_reinvalidates_dep_classes_precisely() {
    let mut s = seeded(); // alpha @ q0r0, beta @ q1r0
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
    s.recompute_hooks();
    s.recompute_personae();

    s.undo().unwrap();
    assert!(
        s.derivations.is_stale(DerivationKind::Hooks),
        "undo of a route edit re-stales Hooks (Routes -> Hooks)"
    );
    assert!(
        !s.derivations.is_stale(DerivationKind::Personae),
        "Personae does not read routes, so a route edit leaves it untouched"
    );

    s.recompute_hooks(); // re-fresh before the redo leg
    s.redo().unwrap();
    assert!(
        s.derivations.is_stale(DerivationKind::Hooks),
        "redo of a route edit re-stales Hooks too"
    );
}

/// Gap 202 (M) — `AddRoute` apply derives route `controls` from endpoint world
/// faction presence. A world at an endpoint system carrying a
/// `WorldFactionPresence` (count alone gates emission — zero dimensions are
/// fine) yields a non-empty `controls`, the revert removes the route, and a
/// re-run reproduces byte-identical controls (BTreeMap-sorted by faction_id).
#[test]
fn add_route_derives_controls_from_endpoint_presence() {
    use sectorforge::sector_model::{
        DominanceState, FactionInfluence, PresenceDimensions, RouteStability, RouteType,
        WorldFactionPresence,
    };

    let mut s = seeded(); // alpha @ q0r0, beta @ q1r0
    let from = s.sector.systems[0].id.clone();
    let to = s.sector.systems[1].id.clone();

    // Register faction F on the roster ("imperial" => lawful-navy / economic).
    s.sector
        .factions
        .push(gen_faction("imperium", "imperial", "lawful"));

    // Add a world to the from-endpoint and attach a presence for "imperium".
    s.run(BuilderCommand::AddWorld {
        system: from.clone(),
        name: "w".into(),
        result_id: None,
    })
    .unwrap();
    s.sector.systems[0].worlds[0]
        .factions
        .push(WorldFactionPresence {
            faction_id: FactionId::new("imperium"),
            subfaction_id: None,
            subfaction_name: None,
            force_id: None,
            force_name: None,
            influence: FactionInfluence::Significant,
            relationship_to_government: "n".into(),
            dimensions: PresenceDimensions::default(),
            dominance: DominanceState::default(),
            intel_confidence: 100,
        });

    s.run(BuilderCommand::AddRoute {
        from,
        to,
        route_type: RouteType::StableWarpLane,
        stability: RouteStability::Stable,
        result_id: None,
    })
    .unwrap();

    let r = &s.sector.routes[0];
    assert!(!r.controls.is_empty(), "endpoint presence yields controls");
    assert!(
        r.controls
            .iter()
            .any(|c| c.faction_id == FactionId::new("imperium")),
        "the derived control references the endpoint faction"
    );
    let controls_json = serde_json::to_string(&r.controls).unwrap();

    s.undo().unwrap();
    assert!(s.sector.routes.is_empty(), "revert removes the route");

    s.redo().unwrap();
    assert_eq!(
        serde_json::to_string(&s.sector.routes[0].controls).unwrap(),
        controls_json,
        "re-run reproduces byte-identical, deterministically-sorted controls"
    );
}

/// Gap 203 (M) — `SetWorldConflict` / `SetSystemConflict` / `SetWorldStability`
/// round-trip (apply -> undo -> redo, byte-identical) plus the NotFound error
/// arm for an unknown world id (apply uses `world_mut(..)?`).
#[test]
fn set_conflict_and_stability_round_trip_and_not_found() {
    // SetWorldConflict round-trip.
    {
        let mut s = seeded();
        let sys = s.sector.systems[0].id.clone();
        s.run(BuilderCommand::AddWorld {
            system: sys,
            name: "w".into(),
            result_id: None,
        })
        .unwrap();
        let wid = s.sector.systems[0].worlds[0].id.clone();
        assert_round_trip(
            s,
            BuilderCommand::SetWorldConflict {
                world: wid,
                before: None,
                after: sectorforge::conflict::ConflictState {
                    intensity: 55,
                    ..Default::default()
                },
            },
        );
    }

    // SetSystemConflict round-trip (no world needed).
    {
        let s = seeded();
        let sys = s.sector.systems[0].id.clone();
        assert_round_trip(
            s,
            BuilderCommand::SetSystemConflict {
                system: sys,
                before: None,
                after: sectorforge::conflict::ConflictState {
                    intensity: 40,
                    ..Default::default()
                },
            },
        );
    }

    // SetWorldStability round-trip.
    {
        let mut s = seeded();
        let sys = s.sector.systems[0].id.clone();
        s.run(BuilderCommand::AddWorld {
            system: sys,
            name: "w".into(),
            result_id: None,
        })
        .unwrap();
        let wid = s.sector.systems[0].worlds[0].id.clone();
        assert_round_trip(
            s,
            BuilderCommand::SetWorldStability {
                world: wid,
                before: None,
                after: sectorforge::stability::StabilityState {
                    public_order: 42.0,
                    ..Default::default()
                },
            },
        );
    }

    // NotFound arm: an unknown world id surfaces WorldNotFound via world_mut.
    {
        let mut s = seeded();
        let err = s
            .run(BuilderCommand::SetWorldConflict {
                world: WorldId::new("no-such-world"),
                before: None,
                after: sectorforge::conflict::ConflictState::default(),
            })
            .unwrap_err();
        assert!(matches!(
            err,
            crate::builder::errors::BuilderError::Mutation(
                sectorforge::sector_model::mutation::MutationError::WorldNotFound(_)
            )
        ));
    }
}

/// Gap 204 (M) — `run()` truncates the redo tail on the next mutation after an
/// undo. After undoing back to cursor 1, a fresh AddSystem drops the redo tail
/// and appends the branch; redo is then a no-op and the orphaned systems are gone.
#[test]
fn run_truncates_redo_tail_on_new_branch() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 16, 16);
    add_n_systems(&mut s, 3); // sys-0, sys-1, sys-2; log len 3, cursor 3
    s.undo().unwrap(); // cursor 2
    s.undo().unwrap(); // cursor 1 (redo tail = [sys-1, sys-2])

    s.run(BuilderCommand::AddSystem {
        coord: HexCoord { q: 5, r: 5 },
        name: "branch".into(),
        result_id: None,
    })
    .unwrap();
    assert_eq!(
        s.command_log.len(),
        2,
        "the redo tail is dropped and the branch appended"
    );
    assert_eq!(s.command_cursor, 2);

    // redo is now a no-op.
    let cur = s.command_cursor;
    s.redo().unwrap();
    assert_eq!(s.command_cursor, cur);

    // Survivors: sys-0 + branch; sys-1 / sys-2 are gone.
    assert_eq!(s.sector.systems.len(), 2);
    assert!(s.sector.systems.iter().any(|x| &*x.name == "sys-0"));
    assert!(s.sector.systems.iter().any(|x| &*x.name == "branch"));
}

/// Gap 205 (M) — `recompute_economy` per-system rollup. Pinning per-world
/// vectors via `world_economy_overrides` sums them into the system row and the
/// sector balance, with surplus/shortage keyed at >= 20 / <= -20.
#[test]
fn recompute_economy_rollup_surplus_shortage() {
    use sectorforge::economy::ResourceVector;

    let mut s = BuilderState::new_blank("t", "T", "seed", 16, 16);
    s.run(BuilderCommand::AddSystem {
        coord: HexCoord { q: 0, r: 0 },
        name: "alpha".into(),
        result_id: None,
    })
    .unwrap();
    let sys = s.sector.systems[0].id.clone();
    s.run(BuilderCommand::AddWorld {
        system: sys.clone(),
        name: "wA".into(),
        result_id: None,
    })
    .unwrap();
    s.run(BuilderCommand::AddWorld {
        system: sys.clone(),
        name: "wB".into(),
        result_id: None,
    })
    .unwrap();
    let wa = s.sector.systems[0].worlds[0].id.clone();
    let wb = s.sector.systems[0].worlds[1].id.clone();

    s.world_economy_overrides.insert(
        wa,
        ResourceVector {
            ore: 15.0,
            foodstuffs: -15.0,
            ..Default::default()
        },
    );
    s.world_economy_overrides.insert(
        wb,
        ResourceVector {
            ore: 10.0,
            foodstuffs: -10.0,
            ..Default::default()
        },
    );
    s.recompute_economy();

    let report = s.sector.economy.as_ref();
    let sysrow = report
        .systems
        .iter()
        .find(|r| r.system_id == sys)
        .expect("the system has an economy row");
    assert_eq!(sysrow.vector.ore, 25.0); // 15 + 10
    assert_eq!(sysrow.vector.foodstuffs, -25.0); // -15 + -10
    assert!(
        sysrow.surplus_resources.iter().any(|k| k == "ore"),
        "25 >= 20 => ore is a surplus"
    );
    assert!(
        sysrow.shortage_resources.iter().any(|k| k == "foodstuffs"),
        "-25 <= -20 => foodstuffs is a shortage"
    );
    assert_eq!(report.sector_balance.ore, 25.0);
    assert_eq!(report.sector_balance.foodstuffs, -25.0);
}

/// Gap 206 (M) — `recompute_chronicle_undoable` routes through the command bus
/// as one `EditChronicle`, preserves manual events, and is undoable.
#[test]
fn recompute_chronicle_undoable_is_one_command_and_preserves_manual() {
    let mut s = seeded();
    let manual = sectorforge::history::HistoryEvent {
        id: "manual-1".into(),
        date: "M41.001".into(),
        era_id: String::new(),
        era_label: String::new(),
        relative_year: 0,
        anchor: sectorforge::history::HistoryAnchor::Sector,
        kind: sectorforge::history::EventKind::Foundation,
        summary: String::new(),
        narrative: "hand-written".into(),
        factions: vec![],
        entities: vec![],
        consequences: vec![],
        weight: 50,
        manual: true,
    };
    s.sector.chronicle.events.push(manual);
    let before = serde_json::to_string(&s.sector.chronicle).unwrap();
    let log_before = s.command_log.len();

    s.recompute_chronicle_undoable().unwrap();

    assert_eq!(
        s.command_log.len(),
        log_before + 1,
        "the recompute lands as exactly one EditChronicle command"
    );
    assert!(
        s.sector
            .chronicle
            .events
            .iter()
            .any(|e| e.id == "manual-1" && e.manual),
        "the manually-authored event survives the recompute"
    );

    s.undo().unwrap();
    assert_eq!(
        serde_json::to_string(&s.sector.chronicle).unwrap(),
        before,
        "undo restores the prior chronicle"
    );
    s.redo().unwrap();
    assert!(
        s.sector
            .chronicle
            .events
            .iter()
            .any(|e| e.id == "manual-1" && e.manual),
        "redo re-applies, manual event present again"
    );
}

/// Gap 207 (M) — `dispatch_background_derivations` + `pump_derivation_jobs`
/// drain two eligible kinds (Relations + Personae) concurrently to completion.
/// Both end Fresh with their outputs installed.
#[test]
fn pump_derivation_jobs_drains_multiple_kinds() {
    let ctx = egui::Context::default();
    let mut s = BuilderState::new_blank("t", "T", "seed", 16, 16);
    // Bypass-bus fixture: a 2-faction sector so each recompute has observable output.
    s.sector = GeneratedSector::empty("t", "T", "seed", 16, 16).into();
    s.sector
        .factions
        .push(gen_faction("imp", "imperial", "lawful"));
    s.sector
        .factions
        .push(gen_faction("chaos", "chaos_space_marine", "hostile"));

    // Prime both as previously-derived so invalidate can flag them.
    s.mark_derivation_fresh(DerivationKind::Relations);
    s.mark_derivation_fresh(DerivationKind::Personae);
    // A Factions-class edit stales BOTH (both list Factions in deps()).
    s.derivations.invalidate(&[DepClass::Factions]);
    assert!(s.derivations.is_stale(DerivationKind::Relations));
    assert!(s.derivations.is_stale(DerivationKind::Personae));

    s.dispatch_background_derivations(&ctx);
    assert_eq!(
        s.derivation_jobs.in_flight.len(),
        2,
        "two eligible stale kinds spawn two in-flight jobs"
    );

    assert!(drain_until_done(&mut s, DerivationKind::Relations));
    assert!(drain_until_done(&mut s, DerivationKind::Personae));
    assert!(!s.derivations.is_stale(DerivationKind::Relations));
    assert!(!s.derivations.is_stale(DerivationKind::Personae));
    assert!(
        s.personae_report.is_some(),
        "the Personae output is installed"
    );
    assert_eq!(
        s.derivation_status(DerivationKind::Relations),
        DerivationStatus::Fresh
    );
}

/// Gap 208 (M) — Economy is excluded from background dispatch (its install
/// mutates the sector), but a sibling eligible kind (Relations) IS dispatched.
#[test]
fn dispatch_excludes_economy_but_dispatches_sibling() {
    let ctx = egui::Context::default();
    let mut s = BuilderState::new_blank("t", "T", "seed", 16, 16);
    s.sector = GeneratedSector::empty("t", "T", "seed", 16, 16).into();
    s.sector
        .factions
        .push(gen_faction("imp", "imperial", "lawful"));
    s.sector
        .factions
        .push(gen_faction("chaos", "chaos_space_marine", "hostile"));

    // Prime both so invalidate can flag them stale.
    s.mark_derivation_fresh(DerivationKind::Economy);
    s.mark_derivation_fresh(DerivationKind::Relations);
    // SystemsWorlds stales BOTH (both depend on SystemsWorlds).
    s.derivations.invalidate(&[DepClass::SystemsWorlds]);
    assert!(s.derivations.is_stale(DerivationKind::Economy));
    assert!(s.derivations.is_stale(DerivationKind::Relations));

    s.dispatch_background_derivations(&ctx);
    assert!(
        !s.derivation_jobs.has_in_flight(DerivationKind::Economy),
        "Economy is never dispatched to a worker (it mutates the sector)"
    );
    assert!(
        s.derivation_jobs.has_in_flight(DerivationKind::Relations),
        "an eligible sibling kind IS dispatched"
    );
    assert!(!s.derivations.deriving.contains(&DerivationKind::Economy));

    // Join the worker before the test exits.
    assert!(drain_until_done(&mut s, DerivationKind::Relations));
}

/// Gap 209 (M) — `revalidate_now` D10 skip reason. Without a worlds catalog the
/// validation report is skipped (with a recorded reason) but the invariant
/// report is always set; with a worlds catalog the report is set and the skip
/// reason cleared.
#[test]
fn revalidate_now_skip_reason_without_worlds_catalog() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 16, 16);
    s.revalidate_now();
    assert_eq!(
        s.feedback.last_validation_skip_reason.as_deref(),
        Some("no worlds catalog loaded")
    );
    assert!(
        s.invariant_report.is_some(),
        "the invariant report is always set, catalog or not"
    );

    let mut s2 = BuilderState::new_blank("t", "T", "seed", 16, 16);
    s2.data_catalogs.worlds = Some(sectorforge::worlds_toml::WorldsConfig::default());
    s2.revalidate_now();
    assert!(
        s2.feedback.last_validation_skip_reason.is_none(),
        "a worlds catalog clears the skip reason"
    );
    assert!(
        s2.validation_report.is_some(),
        "the validation report is set when worlds are present"
    );
}

/// Gap 210 (M) — `export_block_reason` refuse branch. An out-of-bounds region
/// hex trips exactly one `check_sector` invariant violation (REGION_HEX_OUT_OF_BOUNDS),
/// and the strict-warning path (an injected warning, no worlds catalog so it
/// survives revalidate_now) blocks only under strict mode.
#[test]
fn export_block_reason_refuses_on_violation_and_strict_warning() {
    use sectorforge::regions::{RegionConditionKind, WarpRegion};

    // ── invariant-violation branch ──
    {
        let mut s = BuilderState::new_blank("t", "T", "seed", 16, 16); // no worlds catalog
        s.sector.regions = std::sync::Arc::new(vec![WarpRegion {
            id: "reg-0001".into(),
            name: "out".into(),
            kind: RegionConditionKind::WarpStorm,
            hexes: vec![HexCoord { q: 99, r: 0 }], // out of bounds on a 16x16 grid
            centre: HexCoord { q: 99, r: 0 },
        }]);
        let reason = s
            .export_block_reason()
            .expect("an out-of-bounds region hex must block export");
        assert!(reason.starts_with("Export refused — "));
        assert!(
            reason.contains("1 invariant violation(s)"),
            "exactly one REGION_HEX_OUT_OF_BOUNDS violation: {reason}"
        );
    }

    // ── strict-warning branch ──
    {
        let mut s = BuilderState::new_blank("t", "T", "seed", 16, 16); // no worlds catalog
        // No worlds catalog => revalidate_now's None-branch leaves validation_report
        // untouched, so a pre-injected warning survives the recompute.
        s.validation_report = Some(warn_validation());
        s.validation_strict = true;
        let reason = s
            .export_block_reason()
            .expect("a strict-mode warning must block export");
        assert!(reason.starts_with("Export refused — "));
        assert!(
            reason.contains("1 validation warning(s) (strict mode)"),
            "strict mode promotes the warning: {reason}"
        );

        // Non-strict: the same warning does not block.
        s.validation_strict = false;
        assert_eq!(s.export_block_reason(), None);
    }
}

/// Gap 211 (L) — `ReplaceSystem` onto an EMPTY hex leaves `before` None; revert
/// removes the new system and resurrects nothing.
#[test]
fn replace_system_on_empty_hex_captures_no_prior() {
    let mut s = BuilderState::new_blank("t", "T", "seed", 16, 16); // empty sector
    let new_sys = sectorforge::sector_model::GeneratedSystem::new_at(
        sectorforge::ids::system_id(99),
        99,
        HexCoord { q: 3, r: 3 },
        "Dropped",
    );
    s.run(BuilderCommand::ReplaceSystem {
        coord: HexCoord { q: 3, r: 3 },
        new_system: Box::new(new_sys),
        before: None,
    })
    .unwrap();
    assert_eq!(s.sector.systems.len(), 1);

    let BuilderCommand::ReplaceSystem { before, .. } = &s.command_log[s.command_cursor - 1] else {
        panic!("last command should be ReplaceSystem");
    };
    assert!(
        before.is_none(),
        "dropping onto an empty hex captures no prior system"
    );

    s.undo().unwrap();
    assert_eq!(
        s.sector.systems.len(),
        0,
        "revert removes the new system and resurrects nothing"
    );
}

/// Gap 212 (L) — `advance_conflict_ticks` zero-guard pushes no command and logs
/// no tick entry; a real tick records a change-only row for the world whose
/// conflict age advanced.
#[test]
fn advance_conflict_ticks_zero_guard_and_change_only_log() {
    // ── zero-guard ──
    {
        let mut s = seeded();
        let sys = s.sector.systems[0].id.clone();
        s.run(BuilderCommand::AddWorld {
            system: sys,
            name: "w".into(),
            result_id: None,
        })
        .unwrap();
        let log_before = s.command_log.len();
        let tick_before = s.conflict_panel.tick_log.len();
        s.advance_conflict_ticks(0).unwrap();
        assert_eq!(
            s.command_log.len(),
            log_before,
            "ticks=0 pushes no command"
        );
        assert_eq!(
            s.conflict_panel.tick_log.len(),
            tick_before,
            "ticks=0 logs no tick entry"
        );
    }

    // ── change-only: the world (age 0 -> 1) is logged ──
    {
        let mut s = seeded();
        let sys = s.sector.systems[0].id.clone();
        s.run(BuilderCommand::AddWorld {
            system: sys,
            name: "w".into(),
            result_id: None,
        })
        .unwrap();
        let wid = s.sector.systems[0].worlds[0].id.clone();
        s.advance_conflict_ticks(1).unwrap();
        assert!(
            s.conflict_panel.tick_log.iter().any(|e| matches!(
                &e.scope,
                TickLogScope::World { world, .. } if *world == wid
            )),
            "the world whose conflict changed lands a tick-log row"
        );
    }
}

/// Gap 213 (M) — each `recompute_*` marks its own kind Fresh, arms validation
/// (`validation_dirty_since`), and produces a report. Covers the report-backed
/// overlays plus the three that install into `sector.*`.
#[test]
fn recompute_methods_mark_fresh_and_arm_validation() {
    // Report-backed overlays: own-kind Fresh + validation armed + report present.
    macro_rules! check_report_overlay {
        ($method:ident, $kind:expr, $report:ident) => {{
            let mut s = seeded();
            s.feedback.validation_dirty_since = None; // clear the arm from seeded()'s AddSystem
            s.$method();
            assert!(
                !s.derivations.is_stale($kind),
                concat!(stringify!($method), " marks its own kind Fresh")
            );
            assert_eq!(s.derivation_status($kind), DerivationStatus::Fresh);
            assert!(
                s.feedback.validation_dirty_since.is_some(),
                concat!(stringify!($method), " arms validation")
            );
            assert!(
                s.$report.is_some(),
                concat!(stringify!($method), " installs its report")
            );
        }};
    }
    check_report_overlay!(recompute_personae, DerivationKind::Personae, personae_report);
    check_report_overlay!(recompute_hooks, DerivationKind::Hooks, hooks_report);
    check_report_overlay!(recompute_sites, DerivationKind::Sites, sites_report);
    check_report_overlay!(recompute_missions, DerivationKind::Missions, missions_report);
    check_report_overlay!(recompute_prose, DerivationKind::Prose, prose_report);

    // sector.*-installing overlays: own-kind Fresh + validation armed.
    {
        let mut s = seeded();
        s.feedback.validation_dirty_since = None;
        s.recompute_relations();
        assert_eq!(
            s.derivation_status(DerivationKind::Relations),
            DerivationStatus::Fresh
        );
        assert!(s.feedback.validation_dirty_since.is_some());
    }
    {
        let mut s = seeded();
        s.feedback.validation_dirty_since = None;
        s.recompute_chronicle();
        assert_eq!(
            s.derivation_status(DerivationKind::History),
            DerivationStatus::Fresh
        );
        assert!(s.feedback.validation_dirty_since.is_some());
    }
    {
        // recompute_economy ALSO invalidates EconomyCfg/Hooks and sets
        // invariant_report, so only assert Economy Fresh + validation armed.
        let mut s = seeded();
        s.feedback.validation_dirty_since = None;
        s.recompute_economy();
        assert_eq!(
            s.derivation_status(DerivationKind::Economy),
            DerivationStatus::Fresh
        );
        assert!(s.feedback.validation_dirty_since.is_some());
    }
}
