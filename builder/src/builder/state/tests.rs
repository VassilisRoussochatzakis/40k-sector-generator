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
    // §N1 working tabs (PROJECT..EXPORT) — now including the
    // ITERATIVE_GENERATION.md Phase P IterativeGen tab slotted just before EXPORT
    // — plus the two XC-1 diagnostics tabs (VALIDATION, INVARIANTS) appended at
    // the right edge → 27 total.
    assert_eq!(BuilderTab::ALL.len(), 27);
    assert_eq!(BuilderTab::ALL[0], BuilderTab::Project);
    assert_eq!(BuilderTab::ALL[1], BuilderTab::Map);
    assert_eq!(BuilderTab::ALL[23], BuilderTab::IterativeGen);
    assert_eq!(BuilderTab::ALL[24], BuilderTab::Export);
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

    let err = s
        .edit_world(WorldId::new("no-such-world"), |_| {})
        .unwrap_err();
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
        if !state.derivations.deriving.contains(&kind) && !state.derivation_jobs.has_in_flight(kind)
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
        assert_eq!(s.command_log.len(), log_before, "ticks=0 pushes no command");
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
    check_report_overlay!(
        recompute_personae,
        DerivationKind::Personae,
        personae_report
    );
    check_report_overlay!(recompute_hooks, DerivationKind::Hooks, hooks_report);
    check_report_overlay!(recompute_sites, DerivationKind::Sites, sites_report);
    check_report_overlay!(
        recompute_missions,
        DerivationKind::Missions,
        missions_report
    );
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

// ── #35: apply ∘ revert == identity for EVERY BuilderCommand variant ──────────
//
// Parametric coverage layered on top of the bespoke per-command round-trip
// tests above. The single source of truth for "one of every variant" is
// `command::tests::all_variants()` (also reused by
// `dep_classes_cover_all_variants`), so a new variant is automatically picked up
// here the moment it is added there.
//
// For each variant: deep-clone the prepared fixture sector, `apply` the command
// to the clone, then `revert` it, and assert the resulting sector matches the
// pre-apply clone. This exercises each command's own mutation contract directly
// (`BuilderCommand::apply`/`revert`) — the same surface the round-trip tests
// above drive, just exhaustively. (`#[cfg(test)]` may build the fixture sector
// off the bus, which is exactly what `all_variants()` does.)
//
// Two facts shape the comparison:
//   * GeneratedSector has no PartialEq (leaf f32), so equality is over JSON.
//   * Several reverts re-insert a removed/replaced entity at the *tail* of its
//     vector rather than its original index (documented in command.rs — vec
//     order is not load-bearing for output: "a re-add lands at the end of the
//     vector — region order isn't load-bearing"). Equality is therefore
//     compared with id-bearing arrays recursively sorted by id, i.e. "same
//     entities, same field values, order-insensitive".
//
// `EditCatalog::{apply,revert}` are documented no-ops on the sector (the catalog
// slice it carries lives outside `GeneratedSector` and is swapped in
// `BuilderState::{run,undo}`), so identity on the sector holds trivially and
// correctly for it; its catalog-slice round-trip is covered by the command-bus
// tests above (e.g. the `EditCatalog` handling in `run`/`undo`/`redo`).
#[cfg(test)]
mod apply_revert_identity {
    use crate::builder::command::tests::all_variants;
    use sectorforge::sector_model::GeneratedSector;

    /// Recursively sort every JSON array whose elements are all objects with a
    /// string `"id"` field, keyed by that id. Canonicalises the order-insensitive
    /// document collections (systems / routes / factions / regions / worlds) so
    /// a tail-reinserting revert compares equal to the original.
    fn canonicalize(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Array(items) => {
                for item in items.iter_mut() {
                    canonicalize(item);
                }
                let all_have_id = !items.is_empty()
                    && items
                        .iter()
                        .all(|i| i.get("id").and_then(|x| x.as_str()).is_some());
                if all_have_id {
                    items.sort_by(|a, b| {
                        let ak = a.get("id").and_then(|x| x.as_str()).unwrap_or("");
                        let bk = b.get("id").and_then(|x| x.as_str()).unwrap_or("");
                        ak.cmp(bk)
                    });
                }
            }
            serde_json::Value::Object(map) => {
                for (_k, val) in map.iter_mut() {
                    canonicalize(val);
                }
            }
            _ => {}
        }
    }

    /// Canonical (id-sorted) JSON snapshot of a sector.
    fn canonical_sector(sector: &GeneratedSector) -> serde_json::Value {
        let mut v = serde_json::to_value(sector).expect("serialize sector");
        canonicalize(&mut v);
        v
    }

    #[test]
    fn every_variant_round_trips_to_identity() {
        let (fixture, variants) = all_variants();
        // Sanity tripwire: the reused list is the full command surface (mirrors
        // the length assertion in `dep_classes_cover_all_variants`). If this
        // drifts, the brief's "EVERY variant" guarantee has silently shrunk.
        assert_eq!(
            variants.len(),
            40,
            "all_variants() must cover every BuilderCommand variant"
        );

        for mut cmd in variants {
            let label = format!("{cmd:?}");
            let label = label
                .split_whitespace()
                .next()
                .unwrap_or(&label)
                .to_string();

            // Fresh deep clone of the prepared document state per variant.
            let mut sector = fixture.clone();
            let before = canonical_sector(&sector);

            cmd.apply(&mut sector)
                .unwrap_or_else(|e| panic!("{label}: apply failed: {e:?}"));
            cmd.revert(&mut sector)
                .unwrap_or_else(|e| panic!("{label}: revert failed: {e:?}"));

            let after = canonical_sector(&sector);
            assert_eq!(
                before, after,
                "{label}: apply ∘ revert must restore the sector exactly \
                 (compared as id-canonicalised JSON)"
            );
        }
    }
}

/// Audit findings #30 / #22 / #46: auto-save debounce + close-flush, undo/redo
/// re-establishing persisted-derived state, and the gated structural index
/// rebuild. These exercise the off-bus derived/persistence seam — all document
/// mutations still flow through the bus; the tests bypass it only to install
/// fixtures and to corrupt the index as a rebuild tripwire (the `#[cfg(test)]`
/// carve-out).
mod off_bus_seam {
    use super::*;
    use camino::Utf8PathBuf;
    use sectorforge::sector_model::GeneratedSector;

    fn auto_save_state(dir: &tempfile::TempDir) -> (BuilderState, Utf8PathBuf) {
        let path = Utf8PathBuf::from_path_buf(dir.path().join("sector.json")).unwrap();
        let mut s = seeded(); // alpha @ q0r0, beta @ q1r0
        s.auto_save_path = Some(path.clone());
        // The seeding ran before a path was set, so nothing is on disk yet and
        // no write is owed.
        s.auto_save_pending = false;
        s.last_auto_save = None;
        (s, path)
    }

    fn read_saved(path: &Utf8PathBuf) -> GeneratedSector {
        let text = std::fs::read_to_string(path.as_std_path()).expect("auto-save file exists");
        serde_json::from_str(&text).expect("auto-save JSON parses")
    }

    /// #30 — the FIRST command after a path is set writes immediately (no prior
    /// write to throttle against), so the file lands and is up to date.
    #[test]
    fn first_command_writes_immediately() {
        let dir = tempfile::TempDir::new().unwrap();
        let (mut s, path) = auto_save_state(&dir);
        assert!(
            !path.as_std_path().exists(),
            "no write before the first command"
        );

        s.run(BuilderCommand::RenameSystem {
            id: s.sector.systems[0].id.clone(),
            from: String::new(),
            to: "alpha-1".into(),
        })
        .unwrap();

        assert!(
            !s.auto_save_pending,
            "the immediate write clears the pending flag"
        );
        let saved = read_saved(&path);
        assert_eq!(saved.systems[0].name.as_ref(), "alpha-1");
    }

    /// #30 — a rapid SECOND command within the debounce interval does NOT write;
    /// it only marks a pending flag, so the on-disk file still shows the first
    /// edit. `flush_auto_save` then forces the trailing state out (the guaranteed
    /// close-flush path), and the pending flag clears.
    #[test]
    fn rapid_second_command_debounced_then_flushed() {
        let dir = tempfile::TempDir::new().unwrap();
        let (mut s, path) = auto_save_state(&dir);

        s.run(BuilderCommand::RenameSystem {
            id: s.sector.systems[0].id.clone(),
            from: String::new(),
            to: "first".into(),
        })
        .unwrap();
        // Immediately rename again — well inside AUTO_SAVE_MIN_INTERVAL.
        s.run(BuilderCommand::RenameSystem {
            id: s.sector.systems[0].id.clone(),
            from: "first".into(),
            to: "second".into(),
        })
        .unwrap();

        assert!(
            s.auto_save_pending,
            "the second rapid command is debounced — a write is owed, not done"
        );
        assert_eq!(
            read_saved(&path).systems[0].name.as_ref(),
            "first",
            "on-disk file still reflects the first (throttled) write, not the burst tail"
        );

        s.flush_auto_save();
        assert!(!s.auto_save_pending, "flush clears the pending flag");
        assert_eq!(
            read_saved(&path).systems[0].name.as_ref(),
            "second",
            "the guaranteed flush writes the trailing edit"
        );
    }

    /// #30 — `flush_auto_save` / `tick_auto_save` are no-ops when no path is set
    /// (debounce must never panic or spuriously mark state on an unconfigured
    /// session) and when nothing is pending.
    #[test]
    fn flush_and_tick_are_noops_without_path_or_pending() {
        let mut s = seeded();
        assert!(s.auto_save_path.is_none());
        s.run(BuilderCommand::RenameSystem {
            id: s.sector.systems[0].id.clone(),
            from: String::new(),
            to: "x".into(),
        })
        .unwrap();
        assert!(!s.auto_save_pending, "no path ⇒ no pending write");
        s.flush_auto_save();
        assert_eq!(s.tick_auto_save(), None);
    }

    /// #22 — after an undo that stales the in-sector `economy` derivation, the
    /// undo path re-establishes it so `sector.economy` describes the REVERTED
    /// graph (status Fresh), never the pre-undo one. Economy is primed first so
    /// the §39 ledger actually tracks it (cold kinds are intentionally skipped).
    #[test]
    fn undo_reestablishes_persisted_economy() {
        let mut s = seeded();
        s.recompute_economy(); // prime: Economy now fresh + fingerprinted
        assert_eq!(
            s.derivation_status(DerivationKind::Economy),
            DerivationStatus::Fresh
        );

        // A systems/worlds structural edit stales Economy (SystemsWorlds → all).
        s.run(BuilderCommand::AddSystem {
            coord: HexCoord { q: 2, r: 0 },
            name: "gamma".into(),
            result_id: None,
        })
        .unwrap();

        // Undo it. The fix recomputes Economy against the reverted (2-system)
        // graph BEFORE returning, so the status is Fresh, not Stale.
        s.undo().unwrap();
        assert_eq!(
            s.derivation_status(DerivationKind::Economy),
            DerivationStatus::Fresh,
            "#22: undo must re-establish the in-sector economy so the persisted \
             pair is self-consistent with the reverted graph"
        );

        // The installed report describes the reverted (2-system) graph, not the
        // pre-undo 3-system one.
        assert_eq!(
            s.sector.economy.systems.len(),
            2,
            "the re-established economy matches the reverted graph's system count"
        );
    }

    /// #22 + #30 together — with auto-save on, an undo flushes a sector whose
    /// economy matches its graph. We force a flush and re-derive economy from the
    /// persisted sector to confirm the serialized artifact is self-consistent.
    #[test]
    fn undo_then_flush_persists_consistent_economy() {
        let dir = tempfile::TempDir::new().unwrap();
        let (mut s, path) = auto_save_state(&dir);
        s.recompute_economy();

        s.run(BuilderCommand::AddSystem {
            coord: HexCoord { q: 2, r: 0 },
            name: "gamma".into(),
            result_id: None,
        })
        .unwrap();
        s.undo().unwrap();
        s.flush_auto_save();

        // Reload the persisted sector and re-derive economy from it; a
        // self-consistent artifact reproduces its own stored economy footprint.
        let saved = read_saved(&path);
        assert_eq!(saved.systems.len(), 2, "reverted graph persisted");
        let cfg = sectorforge::economy::EconomyConfig {
            enabled: true,
            ..Default::default()
        };
        let fresh = sectorforge::economy::derive_with(&saved, &cfg);
        // A stale (3-system) economy paired with a 2-system graph would differ.
        assert_eq!(
            saved.economy.systems.len(),
            fresh.systems.len(),
            "#22/#30: the persisted economy describes the persisted graph"
        );
    }

    /// #46 — a region-only command (`dep_classes == [Regions]`) must NOT rebuild
    /// the structural index. We corrupt the index with a sentinel beforehand; if
    /// the gate works the sentinel survives (no rebuild), and a subsequent
    /// structural command wipes it (rebuild fires).
    #[test]
    fn index_rebuild_gated_on_structural_dep_classes() {
        let mut s = seeded();
        let sentinel = SystemId::new("sentinel-not-in-sector");

        // Region edit: index must be left untouched.
        s.index.systems.insert(sentinel.clone(), 999);
        s.run(BuilderCommand::AddRegion {
            id: "reg-0001".into(),
            name: "Rift".into(),
            kind: sectorforge::regions::RegionConditionKind::WarpStorm,
            centre: HexCoord { q: 0, r: 0 },
        })
        .unwrap();
        assert!(
            s.index.systems.contains_key(&sentinel),
            "#46: a Regions-only command must skip the index rebuild"
        );

        // Structural edit: the rebuild fires and wipes the sentinel.
        s.run(BuilderCommand::AddSystem {
            coord: HexCoord { q: 3, r: 0 },
            name: "delta".into(),
            result_id: None,
        })
        .unwrap();
        assert!(
            !s.index.systems.contains_key(&sentinel),
            "a SystemsWorlds command must rebuild the index"
        );
    }

    /// #46 — the gate also covers `undo`: undoing a region edit leaves the index
    /// untouched (no rebuild), proven again via the sentinel.
    #[test]
    fn index_rebuild_gated_on_undo_of_region_edit() {
        let mut s = seeded();
        s.run(BuilderCommand::AddRegion {
            id: "reg-0001".into(),
            name: "Rift".into(),
            kind: sectorforge::regions::RegionConditionKind::WarpStorm,
            centre: HexCoord { q: 0, r: 0 },
        })
        .unwrap();
        let sentinel = SystemId::new("sentinel-not-in-sector");
        s.index.systems.insert(sentinel.clone(), 999);
        s.undo().unwrap();
        assert!(
            s.index.systems.contains_key(&sentinel),
            "#46: undo of a Regions-only command must skip the index rebuild"
        );
    }
}

/// ITERATIVE_GENERATION.md Phase S: `GenStep` ↔ `Stage` mapping, the square-grid
/// invariant on session construction (#4), and the DAG step/invalidation logic
/// (§2.3). These are pure-logic checks — no egui context, no engine run.
mod iterative_gen_session {
    use super::*;
    use crate::builder::state::iterative_gen::{
        precheck_generatable, GenStep, IterativeGenSession, MAX_GEN_ROUTE_PAIRS,
    };
    use sectorforge::random_sector::{SectorSize, MAX_CUSTOM_DIM};
    use sectorforge::Stage;

    #[test]
    fn genstep_stage_matches_section_2_3_table() {
        assert_eq!(GenStep::Size.stage(), None);
        assert_eq!(GenStep::Placement.stage(), Some(Stage::Placement));
        assert_eq!(GenStep::Regions.stage(), Some(Stage::Regions));
        assert_eq!(GenStep::Systems.stage(), Some(Stage::Systems));
        assert_eq!(GenStep::Factions.stage(), Some(Stage::Factions));
        // Routes covers Routes..RouteControls — cutoff is RouteControls.
        assert_eq!(GenStep::Routes.stage(), Some(Stage::RouteControls));
        // Finalize runs the whole pipeline.
        assert_eq!(GenStep::Finalize.stage(), Some(Stage::Chronicle));
        assert_eq!(GenStep::Finalize.stage(), Some(Stage::LAST));
    }

    #[test]
    fn early_steps_preview_through_systems_so_systems_render() {
        // The documented v1 nuance: Placement/Regions render through Systems so
        // placed systems are visible, even though their *own* re-roll stage is
        // earlier.
        assert_eq!(GenStep::Size.preview_through(), None);
        assert_eq!(GenStep::Placement.preview_through(), Some(Stage::Systems));
        assert_eq!(GenStep::Regions.preview_through(), Some(Stage::Systems));
        // From Systems onward, preview cutoff == owned stage.
        assert_eq!(GenStep::Systems.preview_through(), GenStep::Systems.stage());
        assert_eq!(GenStep::Routes.preview_through(), GenStep::Routes.stage());
        assert_eq!(
            GenStep::Finalize.preview_through(),
            GenStep::Finalize.stage()
        );
    }

    #[test]
    fn genstep_navigation_is_a_total_order_over_all() {
        assert_eq!(GenStep::ALL.len(), 7);
        assert_eq!(GenStep::Size.prev(), None);
        assert_eq!(GenStep::Finalize.next(), None);
        // Round-trip every adjacent pair.
        for w in GenStep::ALL.windows(2) {
            assert_eq!(w[0].next(), Some(w[1]));
            assert_eq!(w[1].prev(), Some(w[0]));
            assert!(w[0] < w[1], "ALL must be in ascending Ord order");
        }
    }

    fn new_session(dim: u32) -> IterativeGenSession {
        let baseline = BuilderState::new_blank("t", "T", "ignored", dim, dim);
        IterativeGenSession::new(
            baseline.config.clone(),
            "iter-seed".to_string(),
            SectorSize::Custom { dim },
        )
    }

    #[test]
    fn new_session_is_square_and_seeded_from_roller() {
        // Invariant #4: width == height == the picked dimension.
        let s = new_session(24);
        assert_eq!(s.config.generation.sector_width, 24);
        assert_eq!(s.config.generation.sector_height, 24);
        assert_eq!(
            s.config.generation.sector_width,
            s.config.generation.sector_height
        );
        // The session adopts the caller's seed and starts with no re-rolls
        // (nonce-0 ⇒ byte-identical to one-shot, invariant #2) on the Size step.
        assert_eq!(s.root_seed, "iter-seed");
        assert_eq!(s.config.generation.seed, "iter-seed");
        assert_eq!(s.current_step, GenStep::Size);
        assert_eq!(s.accepted_through, None);
        assert!(s.preview.is_none());
        assert!(s.dest.is_none());
        assert!(!s.is_running());
        // Generation knobs match the byte-deterministic roller for (seed, size).
        let rolled = sectorforge::random_sector::build_random_config(
            "iter-seed",
            SectorSize::Custom { dim: 24 },
        );
        assert_eq!(
            s.config.generation.system_count,
            rolled.generation.system_count
        );
    }

    #[test]
    fn new_session_default_nonces_are_byte_identical_suffix() {
        // RerollNonces::default() ⇒ "" for every stage (invariant #2).
        let s = new_session(16);
        for step in GenStep::ALL {
            if let Some(stage) = step.stage() {
                assert_eq!(
                    s.nonces.suffix(stage),
                    "",
                    "fresh session must produce the legacy (empty) suffix for {stage:?}"
                );
            }
        }
    }

    // ── Phase C1 commit guards ──────────────────────────────────────────────
    //
    // The full happy-path (commit == one-shot, byte-for-byte) is covered by the
    // Phase T equivalence + golden suites; here we pin the deterministic error
    // paths of `commit_new_project`, each of which must leave the wizard intact
    // so the user can fix the problem and retry (no half-applied session
    // boundary).

    #[test]
    fn commit_without_session_is_an_error() {
        // No active wizard ⇒ nothing to commit; reported as an error, no panic.
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        assert!(state.iterative_gen.is_none());
        let err = state.commit_new_project().unwrap_err();
        assert!(
            err.contains("No iterative session"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn commit_without_dest_is_an_error_and_keeps_session() {
        // A session with no chosen folder cannot write anything; the wizard must
        // survive so the panel can prompt for a destination and retry.
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.iterative_gen = Some(new_session(8));
        assert!(state.iterative_gen.as_ref().unwrap().dest.is_none());
        let err = state.commit_new_project().unwrap_err();
        assert!(err.contains("destination"), "unexpected message: {err}");
        assert!(
            state.iterative_gen.is_some(),
            "commit must not clear the session when it errors"
        );
    }

    #[test]
    fn commit_without_worlds_catalog_is_an_error_and_keeps_session() {
        // `new_blank` has no worlds catalog, so `synthesize_project_input`
        // returns None and the full run cannot start. With a dest set we get
        // past the dest guard and hit the missing-catalog guard; the session
        // (and the chosen dest) survive for a retry.
        let dir = tempfile::TempDir::new().unwrap();
        let dest = camino::Utf8PathBuf::from_path_buf(dir.path().join("iterative-seed")).unwrap();
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let mut session = new_session(8);
        session.dest = Some(dest.clone());
        state.iterative_gen = Some(session);

        let err = state.commit_new_project().unwrap_err();
        assert!(err.contains("worlds catalog"), "unexpected message: {err}");
        assert!(state.iterative_gen.is_some(), "session survives the error");
        assert_eq!(
            state.iterative_gen.as_ref().unwrap().dest.as_ref(),
            Some(&dest),
            "the chosen destination is preserved for a retry"
        );
        // Nothing was written to disk on the failed run.
        assert!(
            !dest.as_std_path().exists(),
            "no project tree on a failed commit"
        );
    }

    #[test]
    fn blank_builder_commit_seeded_from_base_writes_full_project() {
        // Regression (the user's report): launching the iterative wizard with NO
        // project open used to dead-end — `data_catalogs.worlds` is `None`, so the
        // preview never ran, "re-roll this step" had no visible effect, and commit
        // failed with "No worlds catalog is loaded". The panel entry now seeds the
        // session's `catalogs_override` (+ matching `[inputs]`) from the bundled
        // `_base` preset, exactly like the one-shot random generator scaffolds it.
        // This pins the end-to-end happy path of that seeding: a blank builder
        // commits a complete, reopenable project with a non-empty world pool and
        // roster — the panels the user found empty are now populated.
        let base = camino::Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets/_base");
        let (catalogs, inputs) = crate::builder::project_io::load_base_catalogs_from(&base)
            .expect("the bundled _base preset must load");
        assert!(catalogs.worlds.is_some(), "_base supplies a world pool");

        let mut state = BuilderState::new_blank("blank", "Blank", "seed-x", 12, 12);
        assert!(
            state.data_catalogs.worlds.is_none(),
            "a blank builder starts with no worlds catalog (the bug's precondition)"
        );

        // Mirror the panel entry (project.rs): seed the override + inputs from
        // `_base` because the live builder has no project open.
        let mut session = new_session(12);
        session.config.inputs = inputs;
        session.catalogs_override = Some(catalogs);
        let dir = tempfile::TempDir::new().unwrap();
        let dest = camino::Utf8PathBuf::from_path_buf(dir.path().join("iterative-blank")).unwrap();
        session.dest = Some(dest.clone());
        state.iterative_gen = Some(session);

        state
            .commit_new_project()
            .expect("commit must succeed once the wizard is seeded from _base");

        // The commit is a session boundary: the wizard is cleared and the freshly
        // written project is reopened in place.
        assert!(
            state.iterative_gen.is_none(),
            "wizard cleared at the commit boundary"
        );
        assert_eq!(
            state.project_path.as_ref(),
            Some(&dest),
            "the reopened project points at the committed destination"
        );

        // The reopened project carries the seeded catalogs (the fix), so the
        // builder's world editor and faction roster are populated — not the empty
        // panels the user hit.
        assert!(
            state.data_catalogs.worlds.is_some(),
            "the reopened project has a worlds catalog"
        );
        let factions = state
            .data_catalogs
            .factions
            .as_ref()
            .expect("the reopened project has a factions catalog");
        assert!(
            !factions.factions.is_empty(),
            "the reopened roster is non-empty"
        );

        // A real sector was generated and persisted (preview/commit produce
        // systems from the seeded world pool, not the blank starting grid).
        assert!(
            !state.sector.systems.is_empty(),
            "the committed sector has systems"
        );

        // The catalog + sector files actually landed on disk so the reopen above
        // found a real project tree.
        let out_rel = state.config.outputs.directory.trim_end_matches('/');
        assert!(
            dest.join(out_rel)
                .join("sector.json")
                .as_std_path()
                .exists(),
            "sector.json was written under the outputs directory"
        );
        let worlds_dir = state.config.inputs.world_data_dir.trim_end_matches('/');
        assert!(
            dest.join(worlds_dir)
                .join("worlds.toml")
                .as_std_path()
                .exists(),
            "worlds.toml was written so the reopen finds a world pool"
        );
    }

    // ── §5 transient regions-override seam ───────────────────────────────────
    //
    // The override is applied *only* when assembling the `ProjectInput` for a
    // run (`synthesize_project_input_with`), never written into `data_catalogs`
    // (invariant #5). `None` must reproduce the verbatim catalog assembly
    // byte-for-byte (invariants #2/#6); `Some` must substitute the patch.

    #[test]
    fn regions_override_substitutes_only_in_assembled_input() {
        use sectorforge::regions::RegionsConfig;

        // A `BuilderState` with a worlds catalog (so `synthesize_*` returns Some)
        // and a *disabled* regions catalog standing in for the project's file.
        // (`#[cfg(test)]` may write catalogs directly to build the fixture.)
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.data_catalogs.worlds = Some(sectorforge::worlds_toml::WorldsConfig::default());
        state.data_catalogs.regions = Some(RegionsConfig {
            enabled: false,
            count: 2,
            ..RegionsConfig::default()
        });

        // No override ⇒ the assembled input carries the catalog verbatim, and is
        // byte-identical to the plain `synthesize_project_input` (the legacy
        // path) — invariants #2/#6.
        let base = state
            .synthesize_project_input()
            .expect("worlds catalog present");
        let none = state
            .synthesize_project_input_with(None)
            .expect("worlds catalog present");
        assert_eq!(
            serde_json::to_string(&base.catalogs.regions).unwrap(),
            serde_json::to_string(&none.catalogs.regions).unwrap(),
            "override=None must assemble the regions catalog verbatim (byte-identical)",
        );
        assert!(
            !none.catalogs.regions.enabled && none.catalogs.regions.count == 2,
            "override=None must use the project catalog's regions config",
        );

        // Some(patch) ⇒ the assembled input carries the patch instead of the
        // catalog, but `data_catalogs.regions` is NOT mutated (invariant #5:
        // preview substitution only, never a document write).
        let patch = RegionsConfig {
            enabled: true,
            count: 7,
            mean_size: 9,
            apply_to_routes: false,
            ..RegionsConfig::default()
        };
        let patched = state
            .synthesize_project_input_with(Some(&patch))
            .expect("worlds catalog present");
        assert!(
            patched.catalogs.regions.enabled
                && patched.catalogs.regions.count == 7
                && patched.catalogs.regions.mean_size == 9
                && !patched.catalogs.regions.apply_to_routes,
            "override=Some must substitute the patch into the assembled input",
        );
        assert_eq!(
            state
                .data_catalogs
                .regions
                .as_ref()
                .map(|r| (r.enabled, r.count)),
            Some((false, 2)),
            "applying the override must NOT mutate data_catalogs (transient, preview-only)",
        );
    }

    // ── Phase S pre-flight generation guards (size cap + density cost) ────────
    //
    // `precheck_generatable` must reject a session config that is oversized,
    // structurally invalid, or so dense that a synchronous (uncancellable) commit
    // would hang / OOM on the routes stage's O(n²) allocation — BEFORE any
    // generation runs — while never false-rejecting a legitimate config. The
    // wizard's Size field additionally clamps to `MAX_CUSTOM_DIM` via
    // `set_grid_dim`. These tests pin both layers and a spread of unusual inputs.

    /// Build a wizard config with explicit size/density knobs for the guard tests
    /// (starts from a valid rolled baseline, then overrides the fields under test).
    fn precheck_cfg(dim: u32, system_count: usize) -> sectorforge::config::AppConfig {
        let mut c = new_session(16).config;
        c.generation.sector_width = dim;
        c.generation.sector_height = dim;
        c.generation.system_count = system_count;
        c
    }

    #[test]
    fn precheck_accepts_normal_configs() {
        // No false positives: the default rolled session and a mid-range config.
        assert!(precheck_generatable(&new_session(16).config).is_ok());
        assert!(precheck_generatable(&precheck_cfg(32, 300)).is_ok());
    }

    #[test]
    fn precheck_accepts_the_densest_shipped_preset() {
        // Huge = 80×80 rolls up to ~2560 systems (~3.3M route pairs); the guard
        // must admit it, or it would reject a shipped preset size.
        assert!(
            precheck_generatable(&precheck_cfg(MAX_CUSTOM_DIM, 2560)).is_ok(),
            "the densest shipped preset (80×80, 2560 systems) must stay generatable"
        );
    }

    #[test]
    fn precheck_rejects_oversized_dimension() {
        // One past the cap, and the user's far-oversized ">100" case.
        let err = precheck_generatable(&precheck_cfg(MAX_CUSTOM_DIM + 1, 100)).unwrap_err();
        assert!(err.contains("exceeds the maximum"), "unexpected: {err}");
        assert!(precheck_generatable(&precheck_cfg(200, 100)).is_err());
    }

    #[test]
    fn precheck_dimension_boundary_is_inclusive() {
        assert!(precheck_generatable(&precheck_cfg(MAX_CUSTOM_DIM, 100)).is_ok());
        assert!(precheck_generatable(&precheck_cfg(MAX_CUSTOM_DIM + 1, 100)).is_err());
    }

    #[test]
    fn precheck_rejects_runaway_density() {
        // The reported case: 80×80 at ~1.0 density ⇒ 6400 systems ⇒ ~20.5M pairs.
        let dense = (MAX_CUSTOM_DIM * MAX_CUSTOM_DIM) as usize;
        let err = precheck_generatable(&precheck_cfg(MAX_CUSTOM_DIM, dense)).unwrap_err();
        assert!(err.contains("too dense"), "unexpected: {err}");
    }

    #[test]
    fn precheck_density_uses_effective_system_count() {
        // A huge `system_count` on a TINY grid must NOT be rejected — placement
        // caps to the hex capacity, so the real route cost is small. dim=8 ⇒ 64
        // cells ⇒ ≤64 systems ⇒ ~2k pairs regardless of the absurd request.
        assert!(
            precheck_generatable(&precheck_cfg(8, 1_000_000)).is_ok(),
            "system_count far above the hex capacity must clamp, not reject"
        );
    }

    #[test]
    fn precheck_rejects_inverted_world_range() {
        // min > max would make the systems stage sample an empty range and panic.
        let mut c = precheck_cfg(16, 100);
        c.generation.min_worlds_per_system = 6;
        c.generation.max_worlds_per_system = 2;
        let err = precheck_generatable(&c).unwrap_err();
        assert!(err.contains("inverted"), "unexpected: {err}");
    }

    #[test]
    fn precheck_route_pair_budget_boundary() {
        // Pin the exact O(n²) cost boundary. Largest n with n(n-1)/2 ≤ cap:
        let cap = MAX_GEN_ROUTE_PAIRS;
        let max_n = ((1.0 + (1.0 + 8.0 * cap as f64).sqrt()) / 2.0).floor() as u64;
        // Grid large enough to actually hold them (effective count == requested).
        let dim = MAX_CUSTOM_DIM;
        assert!(
            u64::from(dim) * u64::from(dim) > max_n,
            "grid must hold max_n+1"
        );
        assert!(
            precheck_generatable(&precheck_cfg(dim, max_n as usize)).is_ok(),
            "n at the budget must pass (pairs ≤ cap)"
        );
        assert!(
            precheck_generatable(&precheck_cfg(dim, (max_n + 1) as usize)).is_err(),
            "n one past the budget must fail (pairs > cap)"
        );
    }

    #[test]
    fn precheck_allows_degenerate_small_configs() {
        // 1×1 single system and an empty (0-system) sector are valid, not runaway.
        assert!(precheck_generatable(&precheck_cfg(1, 1)).is_ok());
        assert!(precheck_generatable(&precheck_cfg(16, 0)).is_ok());
    }

    #[test]
    fn set_grid_dim_clamps_above_max_custom_dim() {
        // The Size field cannot push the wizard past the supported grid.
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.iterative_gen = Some(new_session(16));
        let ctx = egui::Context::default();
        state.set_grid_dim(&ctx, 200);
        let g = &state.iterative_gen.as_ref().unwrap().config.generation;
        assert_eq!(g.sector_width, MAX_CUSTOM_DIM);
        assert_eq!(
            g.sector_height, MAX_CUSTOM_DIM,
            "clamp keeps the square invariant"
        );
    }

    #[test]
    fn set_grid_dim_clamps_zero_to_one() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.iterative_gen = Some(new_session(16));
        let ctx = egui::Context::default();
        state.set_grid_dim(&ctx, 0);
        let g = &state.iterative_gen.as_ref().unwrap().config.generation;
        assert_eq!((g.sector_width, g.sector_height), (1, 1));
    }

    #[test]
    fn set_grid_dim_keeps_in_range_value_and_exact_cap() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.iterative_gen = Some(new_session(16));
        let ctx = egui::Context::default();
        state.set_grid_dim(&ctx, 50);
        {
            let g = &state.iterative_gen.as_ref().unwrap().config.generation;
            assert_eq!((g.sector_width, g.sector_height), (50, 50));
        }
        // Exactly at the cap is preserved, not clamped down.
        state.set_grid_dim(&ctx, MAX_CUSTOM_DIM);
        let g = &state.iterative_gen.as_ref().unwrap().config.generation;
        assert_eq!(g.sector_width, MAX_CUSTOM_DIM);
    }

    #[test]
    fn commit_rejects_oversized_dimension_and_writes_nothing() {
        // An out-of-band oversized dim (set_grid_dim would have clamped it) is
        // refused by the commit guard before any disk write.
        let dir = tempfile::TempDir::new().unwrap();
        let dest = camino::Utf8PathBuf::from_path_buf(dir.path().join("too-big")).unwrap();
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let mut session = new_session(16);
        session.config.generation.sector_width = MAX_CUSTOM_DIM + 40;
        session.config.generation.sector_height = MAX_CUSTOM_DIM + 40;
        session.dest = Some(dest.clone());
        state.iterative_gen = Some(session);

        let err = state.commit_new_project().unwrap_err();
        assert!(err.contains("exceeds the maximum"), "unexpected: {err}");
        assert!(
            state.iterative_gen.is_some(),
            "session survives the rejection"
        );
        assert!(
            !dest.as_std_path().exists(),
            "no project tree written on rejection"
        );
    }

    #[test]
    fn commit_rejects_runaway_density_without_hanging() {
        // THE reported pathological case: 80×80 at ~1.0 density (every hex a
        // system). The guard rejects it instantly — the test returning at all is
        // the proof that the O(n²) routes allocation / uncancellable hang was
        // never reached.
        let dir = tempfile::TempDir::new().unwrap();
        let dest = camino::Utf8PathBuf::from_path_buf(dir.path().join("too-dense")).unwrap();
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let mut session = new_session(16);
        session.config.generation.sector_width = MAX_CUSTOM_DIM;
        session.config.generation.sector_height = MAX_CUSTOM_DIM;
        session.config.generation.system_count = (MAX_CUSTOM_DIM * MAX_CUSTOM_DIM) as usize;
        session.dest = Some(dest.clone());
        state.iterative_gen = Some(session);

        let err = state.commit_new_project().unwrap_err();
        assert!(err.contains("too dense"), "unexpected: {err}");
        assert!(state.iterative_gen.is_some(), "session survives");
        assert!(!dest.as_std_path().exists(), "nothing written");
    }

    #[test]
    fn commit_rejects_inverted_world_range() {
        let dir = tempfile::TempDir::new().unwrap();
        let dest = camino::Utf8PathBuf::from_path_buf(dir.path().join("inverted")).unwrap();
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let mut session = new_session(16);
        session.config.generation.min_worlds_per_system = 9;
        session.config.generation.max_worlds_per_system = 3;
        session.dest = Some(dest.clone());
        state.iterative_gen = Some(session);

        let err = state.commit_new_project().unwrap_err();
        assert!(err.contains("inverted"), "unexpected: {err}");
        assert!(!dest.as_std_path().exists());
    }

    #[test]
    fn preview_rejects_runaway_density_and_sets_feedback() {
        // The preview path must also refuse a pathological config: no job spawned,
        // a clear feedback message, preview stays empty (no spinner that never
        // resolves).
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let mut session = new_session(16);
        session.config.generation.sector_width = MAX_CUSTOM_DIM;
        session.config.generation.sector_height = MAX_CUSTOM_DIM;
        session.config.generation.system_count = (MAX_CUSTOM_DIM * MAX_CUSTOM_DIM) as usize;
        state.iterative_gen = Some(session);

        let ctx = egui::Context::default();
        state.spawn_prefix(&ctx, sectorforge::Stage::Systems);

        let s = state.iterative_gen.as_ref().unwrap();
        assert!(
            s.job.is_none(),
            "no background job dispatched for a rejected config"
        );
        assert!(s.preview.is_none(), "preview stays empty");
        let err = state
            .feedback
            .last_command_error
            .as_ref()
            .expect("a feedback reason is surfaced");
        assert!(err.contains("too dense"), "unexpected: {err}");
    }

    #[test]
    fn preview_rejects_oversized_dimension_and_sets_feedback() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let mut session = new_session(16);
        session.config.generation.sector_width = 150;
        session.config.generation.sector_height = 150;
        state.iterative_gen = Some(session);

        let ctx = egui::Context::default();
        state.spawn_prefix(&ctx, sectorforge::Stage::Systems);

        assert!(state.iterative_gen.as_ref().unwrap().job.is_none());
        let err = state
            .feedback
            .last_command_error
            .as_ref()
            .expect("a feedback reason is surfaced");
        assert!(err.contains("exceeds the maximum"), "unexpected: {err}");
    }
}
