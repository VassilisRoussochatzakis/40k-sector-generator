//! Command pattern (U1). Every structural mutation flows through one of
//! these variants so the command log can replay or revert it.
//!
//! The Phase A surface is intentionally narrow — it covers the structural
//! mutations needed by the map and entity inspectors. Overlay mutations
//! (regions, presence, intel, ...) call into `GeneratedSector` directly until
//! the matching panels (Phase C/D) wire up their commands.

use serde::{Deserialize, Serialize};

use super::data_catalogs::CatalogSnapshot;
use super::derivation_cache::DepClass;
use sectorforge::archetypes::ArchetypeState;
use sectorforge::conflict::ConflictState;
use sectorforge::history::SectorChronicle;
use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::intel::SystemIntel;
use sectorforge::orbital_assets::{BlockadeReport, OrbitalAsset};
use sectorforge::regions::{RegionConditionKind, WarpRegion};
use sectorforge::sector_model::mutation::MutationError;
use sectorforge::sector_model::{
    GeneratedFaction, GeneratedRoute, GeneratedSector, GeneratedStar, GeneratedSystem,
    GeneratedWorld, HexCoord, PowerProfile, RouteStability, RouteType,
};
use sectorforge::stability::StabilityState;
use sectorforge::surface_region::SurfaceRegion;
use std::collections::BTreeMap;
use std::sync::Arc;

/// §AR3: per-axis enable mask for `BuilderCommand::AutoAssignArchetypes`. A
/// disabled axis is reset to its default after `sectorforge::archetypes::
/// apply_all` runs, so the user can opt a faction archetype out of the
/// sector-wide derivation without forking `src/archetypes.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchetypeApplyFlags {
    pub imperial: bool,
    pub necron: bool,
    pub tyranid: bool,
    pub ork: bool,
    pub gsc: bool,
    pub tau: bool,
    pub aeldari: bool,
    pub chaos: bool,
}

impl Default for ArchetypeApplyFlags {
    fn default() -> Self {
        Self {
            imperial: true,
            necron: true,
            tyranid: true,
            ork: true,
            gsc: true,
            tau: true,
            aeldari: true,
            chaos: true,
        }
    }
}

impl ArchetypeApplyFlags {
    /// Reset every axis on `state` whose flag is `false` to the default value
    /// from `ArchetypeState::default()`. Used by
    /// `BuilderCommand::AutoAssignArchetypes::apply` after
    /// `sectorforge::archetypes::apply_all` writes its derived values.
    pub fn mask(self, state: &mut ArchetypeState) {
        let d = ArchetypeState::default();
        if !self.imperial {
            state.imperial_co_sovereigns = d.imperial_co_sovereigns.clone();
        }
        if !self.necron {
            state.necron_phase = d.necron_phase;
        }
        if !self.tyranid {
            state.tyranid_stage = d.tyranid_stage;
        }
        if !self.ork {
            state.ork_waaagh = d.ork_waaagh;
        }
        if !self.gsc {
            state.gsc_stage = d.gsc_stage;
        }
        if !self.tau {
            state.tau_sphere = d.tau_sphere;
        }
        if !self.aeldari {
            state.aeldari_activity = d.aeldari_activity;
        }
        if !self.chaos {
            state.chaos_corruption = d.chaos_corruption;
            state.daemon_manifestation = d.daemon_manifestation;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuilderCommand {
    AddSystem {
        coord: HexCoord,
        name: String,
        /// Filled by `apply` so `revert` can put the system back deterministically.
        result_id: Option<SystemId>,
    },
    RemoveSystem {
        id: SystemId,
        /// Filled by `apply` so `revert` can restore the removed entity.
        before: Option<Box<GeneratedSystem>>,
        /// Routes removed alongside the system. Used by `revert`.
        removed_routes: Vec<GeneratedRoute>,
    },
    MoveSystem {
        id: SystemId,
        from: HexCoord,
        to: HexCoord,
    },
    RenameSystem {
        id: SystemId,
        from: String,
        to: String,
    },
    /// §S6: collision-swap. Used when a drag targets an occupied hex and the
    /// user picks "Swap" in the resolver dialog. Reverts by swapping again.
    SwapSystems { a: SystemId, b: SystemId },
    /// §S5: drop in a freshly generated system at `coord`. The caller
    /// (typically [`crate::builder::state::BuilderState::generate_system_here`])
    /// builds `new_system` via `sectorforge::generate_system_standalone` and
    /// hands the payload to the command bus. `before` records any system that
    /// was replaced at the same coord so revert can restore it.
    ReplaceSystem {
        coord: HexCoord,
        new_system: Box<GeneratedSystem>,
        before: Option<Box<GeneratedSystem>>,
    },
    AddWorld {
        system: SystemId,
        name: String,
        result_id: Option<WorldId>,
    },
    RemoveWorld {
        world: WorldId,
        before: Option<Box<GeneratedWorld>>,
        parent_system: Option<SystemId>,
        parent_position: Option<usize>,
    },
    AddRoute {
        from: SystemId,
        to: SystemId,
        route_type: RouteType,
        stability: RouteStability,
        result_id: Option<RouteId>,
    },
    RemoveRoute {
        id: RouteId,
        before: Option<Box<GeneratedRoute>>,
    },
    ReplaceRoutes {
        before: Vec<GeneratedRoute>,
        after: Vec<GeneratedRoute>,
    },
    AddFaction {
        id: FactionId,
        name: String,
        kind: String,
    },
    RemoveFaction {
        id: FactionId,
        before: Option<Box<GeneratedFaction>>,
    },
    /// §AR1: pin one system's `ArchetypeState` to `after`. `before` is filled
    /// by `apply` so `revert` restores the prior value exactly.
    SetArchetype {
        system: SystemId,
        before: Option<Box<ArchetypeState>>,
        after: ArchetypeState,
    },
    /// §AR2: sector-wide auto-assign that runs `sectorforge::archetypes::
    /// apply_all` then masks per-axis using `flags`. `before` snapshots every
    /// system's prior state so `revert` is a deterministic restore.
    AutoAssignArchetypes {
        flags: ArchetypeApplyFlags,
        before: Vec<(SystemId, ArchetypeState)>,
    },
    /// §O1: overwrite a system's `orbital_assets` vector. `before` snapshots
    /// the prior list so `revert` is exact.
    SetOrbitalAssets {
        system: SystemId,
        before: Option<Vec<OrbitalAsset>>,
        after: Vec<OrbitalAsset>,
    },
    /// §O2: overwrite a system's `BlockadeReport`. `before` snapshots the
    /// prior report so `revert` is exact.
    SetBlockadeReport {
        system: SystemId,
        before: Option<Box<BlockadeReport>>,
        after: BlockadeReport,
    },
    /// §SU1: overwrite a world's `regions` vector (§32 surface regions).
    /// `before` snapshots the prior list so `revert` is exact.
    SetSurfaceRegions {
        world: WorldId,
        before: Option<Vec<SurfaceRegion>>,
        after: Vec<SurfaceRegion>,
    },
    /// §CF1: overwrite a world's `ConflictState`. `before` snapshots the prior
    /// state so `revert` is exact.
    SetWorldConflict {
        world: WorldId,
        before: Option<Box<ConflictState>>,
        after: ConflictState,
    },
    /// §CF2: overwrite a system's aggregated `ConflictState`. The "Override
    /// aggregate" toggle in the panel switches edits from `derive_system_conflict`
    /// to direct mutation through this command.
    SetSystemConflict {
        system: SystemId,
        before: Option<Box<ConflictState>>,
        after: ConflictState,
    },
    /// §CF3: overwrite a world's `StabilityState` (7 dimensions).
    SetWorldStability {
        world: WorldId,
        before: Option<Box<StabilityState>>,
        after: StabilityState,
    },
    /// §CTX1 Phase 5 (§6.4): retype an existing route. `before`/`after` are
    /// the prior + new [`RouteType`] so the bus can replay/undo without
    /// re-deriving anything else on the route.
    SetRouteType {
        id: RouteId,
        before: RouteType,
        after: RouteType,
    },
    /// §CTX1 Phase 5 (§6.4): change a route's stability tier. Pattern matches
    /// [`Self::SetRouteType`].
    SetRouteStability {
        id: RouteId,
        before: RouteStability,
        after: RouteStability,
    },
    /// §CTX1 Phase 5 (§6.5): switch a region's [`RegionConditionKind`] — the
    /// map renderer derives the region overlay colour from `kind`, so the menu
    /// label is "RECOLOR ▸" but the mutation is on `kind`. `before` snapshots
    /// the prior kind so revert is exact.
    SetRegionKind {
        region: String,
        before: RegionConditionKind,
        after: RegionConditionKind,
    },
    /// §CTX1 Phase 5 (§6.5): rename an existing region. `before`/`after` hold
    /// the prior + new `name` string so undo restores the label exactly.
    RenameRegion {
        region: String,
        before: String,
        after: String,
    },
    /// §REG / D1: add a warp region (map overlay). Routed through the bus so
    /// region creation is undoable like [`Self::SetRegionKind`] /
    /// [`Self::RenameRegion`] already are. `id` is the caller-assigned
    /// `reg-NNNN` slug; `revert` removes that region again.
    AddRegion {
        id: String,
        name: String,
        kind: RegionConditionKind,
        centre: HexCoord,
    },
    /// §REG / D1: remove a warp region. `before` snapshots the whole region
    /// (including its painted hex list) so `revert` restores it exactly.
    RemoveRegion {
        id: String,
        before: Option<Box<WarpRegion>>,
    },
    /// §REG / D1: replace a warp region wholesale — the undoable unit behind a
    /// paint/erase brush *stroke* (coalesced on drag-release), the REG1 table
    /// edits, and the REG3 seeded-grow hex-list replacement. Carrying the full
    /// [`WarpRegion`] lets one command cover name / kind / centre / hex-list
    /// edits. `before` is captured on `apply` so `revert` is exact.
    EditRegion {
        region: String,
        before: Option<Box<WarpRegion>>,
        after: Box<WarpRegion>,
    },
    /// §CTX1 Phase 6 (§6.6): overwrite a system's `star` field. `before` is
    /// captured on apply so revert restores the prior state exactly (including
    /// the no-star case). Setting `after = None` matches the `REMOVE STAR`
    /// menu row; `Some(GeneratedStar { ... })` covers `ADD STAR` / future
    /// star-replacement flows.
    SetStar {
        system: SystemId,
        before: Option<GeneratedStar>,
        after: Option<GeneratedStar>,
    },
    /// §CTX1 Phase 6 (§6.6): cycle/replace a star's `spectral_type` while
    /// leaving the rest of the [`GeneratedStar`] untouched. Errors out via
    /// `MutationError::SystemNotFound` when the system has no star — the menu
    /// hides the row in that case (`CYCLE SPECTRAL CLASS` is gated on
    /// `star.is_some()`).
    SetStarSpectral {
        system: SystemId,
        before: Option<Arc<str>>,
        after: Option<Arc<str>>,
    },
    /// §CTX1 Phase 6 (§6.7): rename a world. Mirrors [`Self::RenameSystem`].
    RenameWorld {
        world: WorldId,
        before: String,
        after: String,
    },
    /// §CTX1 Phase 6 (§6.7): move a world to a different orbit. `MOVE ORBIT ▸`
    /// dispatches one of these per orbit row (1..=max). `before` is captured
    /// on apply so revert is exact.
    SetWorldOrbit {
        world: WorldId,
        before: u8,
        after: u8,
    },
    /// §CF4: run `sectorforge::conflict::advance_sector` for `ticks` ticks.
    /// `before_world` / `before_system` snapshot per-entity conflict state so
    /// `revert` is a deterministic restore. `before_dominant` snapshots the
    /// `SystemControlSummary::dominant` field touched by hysteresis flips.
    AdvanceConflictTicks {
        ticks: u32,
        before_world: Vec<(WorldId, ConflictState)>,
        before_system: Vec<(SystemId, ConflictState)>,
        before_dominant: Vec<(SystemId, Option<FactionId>)>,
    },
    /// §R4 (XC-2): replace a single world's full payload. The deep detail
    /// editors (WORLD / CONTROL / INTEL tabs) build a mutated clone of the
    /// target world and dispatch this so every field edit — classification,
    /// environment, notable features, tags, notes, faction presences, claims,
    /// intel — lands on the undo/redo log. `before` is captured on `apply` so
    /// `revert` restores the prior world exactly. Use the narrow
    /// [`Self::RenameWorld`] / [`Self::SetWorldOrbit`] for those two fields.
    EditWorld {
        world: WorldId,
        before: Option<Box<GeneratedWorld>>,
        after: Box<GeneratedWorld>,
    },
    /// §R4 (XC-2): replace a single system's full payload for the
    /// non-structural fields the inspector edits — kind, tags, notes,
    /// `primary_factions`, control summary, intel. The panel clones the live
    /// system, mutates the clone, and dispatches this; the system's `worlds`
    /// vector rides through unchanged because only system-scope fields are
    /// touched. `before` is captured on `apply`. Structural world add/remove
    /// still goes through [`Self::AddWorld`] / [`Self::RemoveWorld`].
    EditSystem {
        system: SystemId,
        before: Option<Box<GeneratedSystem>>,
        after: Box<GeneratedSystem>,
    },
    /// §R4 (XC-2 / §H): replace the sector chronicle. The HISTORY tab clones
    /// `sector.chronicle`, applies the edit (event field change, delete,
    /// manual pin, or wizard add), and dispatches this so chronicle edits are
    /// undoable. `before` is captured on `apply`.
    EditChronicle {
        before: Option<Box<SectorChronicle>>,
        after: Box<SectorChronicle>,
    },
    /// §R4 (XC-2 / CTL-4): replace many worlds in a single undo step — the
    /// CONTROL "bulk convert claim type" action rewrites claims across every
    /// matching world. `after` carries the post-edit payload per world id;
    /// `before` is captured on `apply` from the matching live worlds, so
    /// `revert` restores them all exactly.
    BulkEditWorlds {
        before: Vec<(WorldId, GeneratedWorld)>,
        after: Vec<(WorldId, GeneratedWorld)>,
    },
    /// §R4 (XC-2 / §I3): "Generate baseline intel" — runs
    /// [`sectorforge::intel::derive_intel`] over the whole sector. `observers`
    /// is stored so `redo` replays the same derivation; `before_systems` /
    /// `before_worlds` snapshot each entity's prior intel so `revert` is exact.
    DeriveBaselineIntel {
        observers: Vec<String>,
        before_systems: Vec<(SystemId, SystemIntel)>,
        before_worlds: Vec<(WorldId, SystemIntel)>,
    },
    /// §R4 (IMPROVEMENT_REVIEW E1): "Apply to faction totals" overwrites every
    /// faction's stored `power` (`#[serde(default)]` document state) with the
    /// presence-aggregated projection. The CONTROL tab computes the target
    /// `after` map and dispatches this so the bulk overwrite is undoable;
    /// `before` snapshots each faction's prior power on `apply`.
    ApplyFactionPower {
        before: Vec<(FactionId, PowerProfile)>,
        after: BTreeMap<FactionId, PowerProfile>,
    },
    /// Audit finding #8: a coalesced edit to the catalog-edit document slice
    /// (the 13 `data/*.toml` mirrors + the §E1..§E3 economy override
    /// side-tables, both captured in [`CatalogSnapshot`]). Catalog edits used to
    /// bypass the bus and mutate `BuilderState.data_catalogs` directly, which
    /// left them un-undoable; routing them through this variant puts a whole
    /// edit *burst* (coalesced by [`crate::builder::BuilderState::
    /// begin_catalog_session`]) on the undo stack as one entry.
    ///
    /// [`Self::apply`] / [`Self::revert`] are **no-ops on the sector** — the
    /// catalog/override slice lives outside `GeneratedSector`, so the actual
    /// swap happens in `BuilderState::{run,undo,redo}` right after the sector
    /// apply/revert (it cannot borrow the sector and the catalogs at once).
    EditCatalog {
        before: Box<CatalogSnapshot>,
        after: Box<CatalogSnapshot>,
    },
}

/// D-S1/D3 dedup: collapse the "find the entity or raise `*NotFound`" chain
/// that otherwise recurs verbatim in ~12 `apply` arms. Returning `Result`
/// (not `Option`) lets each arm end in `let sys = system_mut(sector, id)?;`.
fn system_mut<'a>(
    sector: &'a mut GeneratedSector,
    id: &SystemId,
) -> Result<&'a mut GeneratedSystem, MutationError> {
    sector
        .systems
        .iter_mut()
        .find(|s| s.id == *id)
        .ok_or_else(|| MutationError::SystemNotFound(id.to_string()))
}

/// D-S1/D3 dedup: world counterpart to [`system_mut`] — flattens the
/// system→worlds walk that several `apply` arms repeat.
fn world_mut<'a>(
    sector: &'a mut GeneratedSector,
    id: &WorldId,
) -> Result<&'a mut GeneratedWorld, MutationError> {
    sector
        .systems
        .iter_mut()
        .flat_map(|s| s.worlds.iter_mut())
        .find(|w| w.id == *id)
        .ok_or_else(|| MutationError::WorldNotFound(id.to_string()))
}

impl BuilderCommand {
    /// LD2 — the §39 mutation-input classes this command touches. The command
    /// bus passes the result to [`crate::builder::DerivationLedger::invalidate`]
    /// so only the derivations downstream of the touched classes are marked
    /// stale, replacing the blanket cache flush.
    ///
    /// Most structural system/world edits map to
    /// [`DepClass::SystemsWorlds`], which §39 fans out to *every* derivation;
    /// route edits to [`DepClass::Routes`]; faction-roster edits to
    /// [`DepClass::Factions`]; warp-region edits to [`DepClass::Regions`].
    /// Catalog edits (`data/*.toml` mirrors + the economy override side-tables)
    /// now route through [`Self::EditCatalog`] (audit finding #8) — coalesced
    /// into one undoable burst — and classify to `SystemsWorlds` so every
    /// catalog-derived overlay is re-invalidated; the live `recompute_*` calls
    /// the panels make during the burst still drive the precise §39 cascade.
    ///
    /// D-S1/D3: this is one of three exhaustive `match self` blocks
    /// (`dep_classes` / [`Self::apply`] / [`Self::revert`]) maintained in
    /// parallel. None carries a `_` arm, so the compiler forces every new
    /// variant to be handled in all three — the only risk it cannot catch is
    /// returning the *wrong* (or empty) class here, which the
    /// `dep_classes_cover_all_variants` test guards.
    pub fn dep_classes(&self) -> &'static [DepClass] {
        use DepClass as D;
        match self {
            // Route-only structural edits.
            Self::AddRoute { .. }
            | Self::RemoveRoute { .. }
            | Self::ReplaceRoutes { .. }
            | Self::SetRouteType { .. }
            | Self::SetRouteStability { .. } => &[D::Routes],
            // Removing a system drops its incident routes alongside it, so the
            // route graph changes too — even though `SystemsWorlds` already
            // fans out to everything, the pair documents the real footprint.
            Self::RemoveSystem { .. } => &[D::SystemsWorlds, D::Routes],
            // Faction-roster edits.
            Self::AddFaction { .. }
            | Self::RemoveFaction { .. }
            | Self::ApplyFactionPower { .. } => &[D::Factions],
            // Warp-region edits.
            Self::SetRegionKind { .. }
            | Self::RenameRegion { .. }
            | Self::AddRegion { .. }
            | Self::RemoveRegion { .. }
            | Self::EditRegion { .. } => &[D::Regions],
            // Everything else is a system/world field edit, which §39 treats
            // as invalidating all derivations.
            Self::AddSystem { .. }
            | Self::MoveSystem { .. }
            | Self::RenameSystem { .. }
            | Self::SwapSystems { .. }
            | Self::ReplaceSystem { .. }
            | Self::AddWorld { .. }
            | Self::RemoveWorld { .. }
            | Self::SetArchetype { .. }
            | Self::AutoAssignArchetypes { .. }
            | Self::SetOrbitalAssets { .. }
            | Self::SetBlockadeReport { .. }
            | Self::SetSurfaceRegions { .. }
            | Self::SetWorldConflict { .. }
            | Self::SetSystemConflict { .. }
            | Self::SetWorldStability { .. }
            | Self::SetStar { .. }
            | Self::SetStarSpectral { .. }
            | Self::RenameWorld { .. }
            | Self::SetWorldOrbit { .. }
            | Self::AdvanceConflictTicks { .. }
            | Self::EditWorld { .. }
            | Self::EditSystem { .. }
            | Self::EditChronicle { .. }
            | Self::BulkEditWorlds { .. }
            | Self::DeriveBaselineIntel { .. } => &[D::SystemsWorlds],
            // A coalesced catalog edit can touch *any* of the 13 config mirrors
            // (economy / relations / personae / hooks / sites / missions / prose
            // / history / factions / regions / worlds / names / routes) plus the
            // economy override side-tables, so it must invalidate every
            // catalog-derived overlay. §39 makes `SystemsWorlds` fan out to
            // *all* derivations — the broadest, safe over-invalidation. Precise
            // per-catalog classing is unnecessary here (correctness > precision).
            Self::EditCatalog { .. } => &[D::SystemsWorlds],
        }
    }

    pub fn apply(&mut self, sector: &mut GeneratedSector) -> Result<(), MutationError> {
        match self {
            Self::AddSystem {
                coord,
                name,
                result_id,
            } => {
                let id = sector.add_system(*coord, name)?;
                *result_id = Some(id);
                Ok(())
            }
            Self::RemoveSystem {
                id,
                before,
                removed_routes,
            } => {
                let sys = sector
                    .systems
                    .iter()
                    .find(|s| s.id == *id)
                    .cloned()
                    .ok_or_else(|| MutationError::SystemNotFound(id.to_string()))?;
                *removed_routes = sector
                    .routes
                    .iter()
                    .filter(|r| r.from_system_id == *id || r.to_system_id == *id)
                    .cloned()
                    .collect();
                sector.remove_system(id)?;
                *before = Some(Box::new(sys));
                Ok(())
            }
            // D4: `move_system`/`swap_systems` refresh route `distance` (which is
            // coord-derived) but deliberately do *not* recompute route `controls`.
            // `derive_route_controls` reads faction presences + endpoint tags at
            // the two endpoint systems — never coordinates — so a coord change
            // cannot stale controls. `AddRoute` recomputes only because a brand-new
            // route starts with an empty `controls` vector that needs deriving.
            Self::MoveSystem { id, from: _, to } => sector.move_system(id, *to),
            Self::RenameSystem { id, from: _, to } => sector.rename_system(id, to),
            // D4: see `MoveSystem` — controls are endpoint-presence-derived, not
            // position-derived, so a swap leaves them correct.
            Self::SwapSystems { a, b } => sector.swap_systems(a, b),
            Self::ReplaceSystem {
                coord,
                new_system,
                before,
            } => {
                if let Some(pos) = sector.systems.iter().position(|s| s.coord == *coord) {
                    *before = Some(Box::new(sector.systems.remove(pos)));
                }
                sector.systems.push((**new_system).clone());
                sector.manifest.system_count = sector.systems.len();
                sector.manifest.world_count = sector.systems.iter().map(|s| s.worlds.len()).sum();
                Ok(())
            }
            Self::AddWorld {
                system,
                name,
                result_id,
            } => {
                let id = sector.add_world_to_system(system, name)?;
                *result_id = Some(id);
                Ok(())
            }
            Self::RemoveWorld {
                world,
                before,
                parent_system,
                parent_position,
            } => {
                for sys in &sector.systems {
                    if let Some(pos) = sys.worlds.iter().position(|w| w.id == *world) {
                        *before = Some(Box::new(sys.worlds[pos].clone()));
                        *parent_system = Some(sys.id.clone());
                        *parent_position = Some(pos);
                        break;
                    }
                }
                sector.remove_world(world)
            }
            Self::AddRoute {
                from,
                to,
                route_type,
                stability,
                result_id,
            } => {
                let id = sector.add_route(from, to, *route_type, *stability)?;
                let systems_by_id: std::collections::BTreeMap<&str, &GeneratedSystem> =
                    sector.systems.iter().map(|s| (s.id.as_str(), s)).collect();
                if let Some(route) = sector.routes.iter_mut().find(|r| r.id == id) {
                    route.controls = sectorforge::route_control::derive_route_controls(
                        route,
                        &systems_by_id,
                        &sector.factions,
                    );
                }
                *result_id = Some(id);
                Ok(())
            }
            Self::RemoveRoute { id, before } => {
                let r = sector
                    .routes
                    .iter()
                    .find(|r| r.id == *id)
                    .cloned()
                    .ok_or_else(|| MutationError::RouteNotFound(id.to_string()))?;
                sector.remove_route(id)?;
                *before = Some(Box::new(r));
                Ok(())
            }
            Self::ReplaceRoutes { before, after } => {
                *before = sector.routes.clone();
                sector.routes = after.clone();
                sector.manifest.route_count = sector.routes.len();
                Ok(())
            }
            Self::AddFaction { id, name, kind } => {
                sector.add_faction(id.clone(), name, kind);
                Ok(())
            }
            Self::RemoveFaction { id, before } => {
                let f = sector
                    .factions
                    .iter()
                    .find(|f| f.id == *id)
                    .cloned()
                    .ok_or_else(|| MutationError::FactionNotFound(id.to_string()))?;
                sector.remove_faction(id)?;
                *before = Some(Box::new(f));
                Ok(())
            }
            Self::SetArchetype {
                system,
                before,
                after,
            } => {
                let prev = sector
                    .systems
                    .iter()
                    .find(|s| s.id == *system)
                    .map(|s| s.archetype.clone())
                    .ok_or_else(|| MutationError::SystemNotFound(system.to_string()))?;
                sector.set_archetype(system, after.clone())?;
                *before = Some(Box::new(prev));
                Ok(())
            }
            Self::AutoAssignArchetypes { flags, before } => {
                *before = sector
                    .systems
                    .iter()
                    .map(|s| (s.id.clone(), s.archetype.clone()))
                    .collect();
                sectorforge::archetypes::apply_all(sector);
                for sys in &mut sector.systems {
                    flags.mask(&mut sys.archetype);
                }
                Ok(())
            }
            Self::SetOrbitalAssets {
                system,
                before,
                after,
            } => {
                let sys = system_mut(sector, system)?;
                *before = Some(sys.orbital_assets.clone());
                sys.orbital_assets = after.clone();
                Ok(())
            }
            Self::SetBlockadeReport {
                system,
                before,
                after,
            } => {
                let sys = system_mut(sector, system)?;
                *before = Some(Box::new(sys.blockade.clone()));
                sys.blockade = after.clone();
                Ok(())
            }
            Self::SetSurfaceRegions {
                world,
                before,
                after,
            } => {
                let w = world_mut(sector, world)?;
                *before = Some(w.regions.clone());
                w.regions = after.clone();
                Ok(())
            }
            Self::SetWorldConflict {
                world,
                before,
                after,
            } => {
                let w = world_mut(sector, world)?;
                *before = Some(Box::new(w.conflict.clone()));
                w.conflict = after.clone();
                Ok(())
            }
            Self::SetSystemConflict {
                system,
                before,
                after,
            } => {
                let sys = system_mut(sector, system)?;
                *before = Some(Box::new(sys.conflict.clone()));
                sys.conflict = after.clone();
                Ok(())
            }
            Self::SetWorldStability {
                world,
                before,
                after,
            } => {
                let w = world_mut(sector, world)?;
                *before = Some(Box::new(w.stability));
                w.stability = *after;
                Ok(())
            }
            Self::SetRouteType { id, before, after } => {
                let route = sector
                    .routes
                    .iter_mut()
                    .find(|r| r.id == *id)
                    .ok_or_else(|| MutationError::RouteNotFound(id.to_string()))?;
                *before = route.route_type;
                route.route_type = *after;
                Ok(())
            }
            Self::SetRouteStability { id, before, after } => {
                let route = sector
                    .routes
                    .iter_mut()
                    .find(|r| r.id == *id)
                    .ok_or_else(|| MutationError::RouteNotFound(id.to_string()))?;
                *before = route.stability;
                route.stability = *after;
                Ok(())
            }
            Self::SetRegionKind {
                region,
                before,
                after,
            } => {
                let mut regions = (*sector.regions).clone();
                let reg = regions
                    .iter_mut()
                    .find(|r| r.id == *region)
                    .ok_or_else(|| MutationError::RegionNotFound(region.clone()))?;
                *before = reg.kind;
                reg.kind = *after;
                sector.regions = std::sync::Arc::new(regions);
                Ok(())
            }
            Self::RenameRegion {
                region,
                before,
                after,
            } => {
                let mut regions = (*sector.regions).clone();
                let reg = regions
                    .iter_mut()
                    .find(|r| r.id == *region)
                    .ok_or_else(|| MutationError::RegionNotFound(region.clone()))?;
                *before = reg.name.clone();
                reg.name = after.clone();
                sector.regions = std::sync::Arc::new(regions);
                Ok(())
            }
            Self::AddRegion {
                id,
                name,
                kind,
                centre,
            } => {
                sector.add_region(id, name, *kind, *centre);
                Ok(())
            }
            Self::RemoveRegion { id, before } => {
                let r = sector
                    .regions
                    .iter()
                    .find(|r| r.id == *id)
                    .cloned()
                    .ok_or_else(|| MutationError::RegionNotFound(id.clone()))?;
                sector.remove_region(id)?;
                *before = Some(Box::new(r));
                Ok(())
            }
            Self::EditRegion {
                region,
                before,
                after,
            } => {
                let mut regions = (*sector.regions).clone();
                let slot = regions
                    .iter_mut()
                    .find(|r| r.id == *region)
                    .ok_or_else(|| MutationError::RegionNotFound(region.clone()))?;
                // D1: keep a caller-supplied pre-stroke snapshot. A paint stroke
                // pre-applies to the live region for feedback, then commits with
                // `before` already set, so re-capturing here would record the
                // post-stroke state and make `revert` a no-op. REG1/REG3 edits
                // dispatch with `before: None` and the live region untouched, so
                // capturing from the live `slot` is correct for them.
                if before.is_none() {
                    *before = Some(Box::new(slot.clone()));
                }
                *slot = (**after).clone();
                sector.regions = std::sync::Arc::new(regions);
                Ok(())
            }
            Self::SetStar {
                system,
                before,
                after,
            } => {
                let sys = system_mut(sector, system)?;
                *before = sys.star.clone();
                sys.star = after.clone();
                Ok(())
            }
            Self::SetStarSpectral {
                system,
                before,
                after,
            } => {
                let sys = system_mut(sector, system)?;
                let star = sys
                    .star
                    .as_mut()
                    .ok_or_else(|| MutationError::SystemNotFound(system.to_string()))?;
                *before = star.spectral_type.clone();
                star.spectral_type = after.clone();
                Ok(())
            }
            Self::RenameWorld {
                world,
                before,
                after,
            } => {
                let w = world_mut(sector, world)?;
                *before = w.name.to_string();
                w.name = Arc::from(after.as_str());
                Ok(())
            }
            Self::SetWorldOrbit {
                world,
                before,
                after,
            } => {
                let w = world_mut(sector, world)?;
                *before = w.orbit;
                w.orbit = *after;
                Ok(())
            }
            Self::AdvanceConflictTicks {
                ticks,
                before_world,
                before_system,
                before_dominant,
            } => {
                before_world.clear();
                before_system.clear();
                before_dominant.clear();
                for sys in &sector.systems {
                    before_system.push((sys.id.clone(), sys.conflict.clone()));
                    before_dominant.push((sys.id.clone(), sys.control.dominant.clone()));
                    for w in &sys.worlds {
                        before_world.push((w.id.clone(), w.conflict.clone()));
                    }
                }
                for _ in 0..*ticks {
                    sectorforge::conflict::advance_sector(sector);
                }
                Ok(())
            }
            Self::EditWorld {
                world,
                before,
                after,
            } => {
                let w = world_mut(sector, world)?;
                *before = Some(Box::new(w.clone()));
                *w = (**after).clone();
                Ok(())
            }
            Self::EditSystem {
                system,
                before,
                after,
            } => {
                let sys = system_mut(sector, system)?;
                *before = Some(Box::new(sys.clone()));
                *sys = (**after).clone();
                // The panel only edits system-scope fields, so `worlds` is
                // unchanged — but recompute the manifest defensively in case a
                // future caller diverges, keeping counts coherent.
                sector.manifest.system_count = sector.systems.len();
                sector.manifest.world_count = sector.systems.iter().map(|s| s.worlds.len()).sum();
                Ok(())
            }
            Self::EditChronicle { before, after } => {
                *before = Some(Box::new(sector.chronicle.clone()));
                sector.chronicle = (**after).clone();
                Ok(())
            }
            Self::BulkEditWorlds { before, after } => {
                before.clear();
                for (wid, new_w) in after.iter() {
                    if let Some(w) = sector
                        .systems
                        .iter_mut()
                        .flat_map(|s| s.worlds.iter_mut())
                        .find(|w| w.id == *wid)
                    {
                        before.push((wid.clone(), w.clone()));
                        *w = new_w.clone();
                    }
                }
                Ok(())
            }
            Self::DeriveBaselineIntel {
                observers,
                before_systems,
                before_worlds,
            } => {
                before_systems.clear();
                before_worlds.clear();
                for sys in &sector.systems {
                    before_systems.push((sys.id.clone(), sys.intel.clone()));
                    for w in &sys.worlds {
                        before_worlds.push((w.id.clone(), w.intel.clone()));
                    }
                }
                let refs: Vec<&str> = observers.iter().map(String::as_str).collect();
                sectorforge::intel::derive_intel(sector, &refs);
                Ok(())
            }
            Self::ApplyFactionPower { before, after } => {
                before.clear();
                for f in &sector.factions {
                    before.push((f.id.clone(), f.power));
                }
                sectorforge::control::apply_faction_power(&mut sector.factions, after);
                Ok(())
            }
            // No-op on the sector: the catalog/override slice it carries lives
            // outside `GeneratedSector` and is swapped in `BuilderState::run`
            // (and `redo`) right after this returns.
            Self::EditCatalog { .. } => Ok(()),
        }
    }

    pub fn revert(&self, sector: &mut GeneratedSector) -> Result<(), MutationError> {
        match self {
            Self::AddSystem { result_id, .. } => {
                if let Some(id) = result_id {
                    sector.remove_system(id)?;
                }
                Ok(())
            }
            Self::RemoveSystem {
                before,
                removed_routes,
                ..
            } => {
                if let Some(sys) = before {
                    sector.systems.push((**sys).clone());
                    sector.manifest.system_count = sector.systems.len();
                    sector.manifest.world_count =
                        sector.systems.iter().map(|s| s.worlds.len()).sum();
                }
                for r in removed_routes {
                    sector.routes.push(r.clone());
                }
                sector.manifest.route_count = sector.routes.len();
                Ok(())
            }
            Self::MoveSystem { id, from, .. } => sector.move_system(id, *from),
            Self::RenameSystem { id, from, .. } => sector.rename_system(id, from),
            Self::SwapSystems { a, b } => sector.swap_systems(a, b),
            Self::ReplaceSystem {
                new_system, before, ..
            } => {
                if let Some(pos) = sector.systems.iter().position(|s| s.id == new_system.id) {
                    sector.systems.remove(pos);
                }
                if let Some(prev) = before {
                    sector.systems.push((**prev).clone());
                }
                sector.manifest.system_count = sector.systems.len();
                sector.manifest.world_count = sector.systems.iter().map(|s| s.worlds.len()).sum();
                Ok(())
            }
            Self::AddWorld { result_id, .. } => {
                if let Some(id) = result_id {
                    sector.remove_world(id)?;
                }
                Ok(())
            }
            Self::RemoveWorld {
                before,
                parent_system,
                parent_position,
                ..
            } => {
                if let (Some(world), Some(sys_id), Some(pos)) =
                    (before, parent_system, parent_position)
                {
                    if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == *sys_id) {
                        let insert_at = (*pos).min(sys.worlds.len());
                        sys.worlds.insert(insert_at, (**world).clone());
                        sector.manifest.world_count =
                            sector.systems.iter().map(|s| s.worlds.len()).sum();
                    }
                }
                Ok(())
            }
            Self::AddRoute { result_id, .. } => {
                if let Some(id) = result_id {
                    sector.remove_route(id)?;
                }
                Ok(())
            }
            Self::RemoveRoute { before, .. } => {
                if let Some(r) = before {
                    sector.routes.push((**r).clone());
                    sector.manifest.route_count = sector.routes.len();
                }
                Ok(())
            }
            Self::ReplaceRoutes { before, .. } => {
                sector.routes = before.clone();
                sector.manifest.route_count = sector.routes.len();
                Ok(())
            }
            Self::AddFaction { id, .. } => sector.remove_faction(id),
            Self::RemoveFaction { before, .. } => {
                if let Some(f) = before {
                    sector.factions.push((**f).clone());
                }
                Ok(())
            }
            Self::SetArchetype { system, before, .. } => {
                if let Some(prev) = before {
                    sector.set_archetype(system, (**prev).clone())?;
                }
                Ok(())
            }
            Self::AutoAssignArchetypes { before, .. } => {
                for (id, state) in before {
                    if sector.systems.iter().any(|s| s.id == *id) {
                        sector.set_archetype(id, state.clone())?;
                    }
                }
                Ok(())
            }
            Self::SetOrbitalAssets { system, before, .. } => {
                if let Some(prev) = before {
                    if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == *system) {
                        sys.orbital_assets = prev.clone();
                    }
                }
                Ok(())
            }
            Self::SetBlockadeReport { system, before, .. } => {
                if let Some(prev) = before {
                    if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == *system) {
                        sys.blockade = (**prev).clone();
                    }
                }
                Ok(())
            }
            Self::SetSurfaceRegions { world, before, .. } => {
                if let Some(prev) = before {
                    if let Some(w) = sector
                        .systems
                        .iter_mut()
                        .flat_map(|s| s.worlds.iter_mut())
                        .find(|w| w.id == *world)
                    {
                        w.regions = prev.clone();
                    }
                }
                Ok(())
            }
            Self::SetWorldConflict { world, before, .. } => {
                if let Some(prev) = before {
                    if let Some(w) = sector
                        .systems
                        .iter_mut()
                        .flat_map(|s| s.worlds.iter_mut())
                        .find(|w| w.id == *world)
                    {
                        w.conflict = (**prev).clone();
                    }
                }
                Ok(())
            }
            Self::SetSystemConflict { system, before, .. } => {
                if let Some(prev) = before {
                    if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == *system) {
                        sys.conflict = (**prev).clone();
                    }
                }
                Ok(())
            }
            Self::SetWorldStability { world, before, .. } => {
                if let Some(prev) = before {
                    if let Some(w) = sector
                        .systems
                        .iter_mut()
                        .flat_map(|s| s.worlds.iter_mut())
                        .find(|w| w.id == *world)
                    {
                        w.stability = **prev;
                    }
                }
                Ok(())
            }
            Self::SetRouteType { id, before, .. } => {
                if let Some(r) = sector.routes.iter_mut().find(|r| r.id == *id) {
                    r.route_type = *before;
                }
                Ok(())
            }
            Self::SetRouteStability { id, before, .. } => {
                if let Some(r) = sector.routes.iter_mut().find(|r| r.id == *id) {
                    r.stability = *before;
                }
                Ok(())
            }
            Self::SetRegionKind { region, before, .. } => {
                let mut regions = (*sector.regions).clone();
                if let Some(reg) = regions.iter_mut().find(|r| r.id == *region) {
                    reg.kind = *before;
                    sector.regions = std::sync::Arc::new(regions);
                }
                Ok(())
            }
            Self::RenameRegion { region, before, .. } => {
                let mut regions = (*sector.regions).clone();
                if let Some(reg) = regions.iter_mut().find(|r| r.id == *region) {
                    reg.name = before.clone();
                    sector.regions = std::sync::Arc::new(regions);
                }
                Ok(())
            }
            // D1: `add_region` appends, so revert removes by id. Like the
            // `RemoveFaction`/`RemoveSystem` reverts, a re-add lands at the end
            // of the vector — region order isn't load-bearing for output.
            Self::AddRegion { id, .. } => sector.remove_region(id),
            Self::RemoveRegion { before, .. } => {
                if let Some(r) = before {
                    let mut regions = (*sector.regions).clone();
                    regions.push((**r).clone());
                    sector.regions = std::sync::Arc::new(regions);
                }
                Ok(())
            }
            Self::EditRegion {
                region, before, ..
            } => {
                if let Some(prev) = before {
                    let mut regions = (*sector.regions).clone();
                    if let Some(slot) = regions.iter_mut().find(|r| r.id == *region) {
                        *slot = (**prev).clone();
                        sector.regions = std::sync::Arc::new(regions);
                    }
                }
                Ok(())
            }
            Self::SetStar { system, before, .. } => {
                if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == *system) {
                    sys.star = before.clone();
                }
                Ok(())
            }
            Self::SetStarSpectral { system, before, .. } => {
                if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == *system) {
                    if let Some(star) = sys.star.as_mut() {
                        star.spectral_type = before.clone();
                    }
                }
                Ok(())
            }
            Self::RenameWorld { world, before, .. } => {
                if let Some(w) = sector
                    .systems
                    .iter_mut()
                    .flat_map(|s| s.worlds.iter_mut())
                    .find(|w| w.id == *world)
                {
                    w.name = Arc::from(before.as_str());
                }
                Ok(())
            }
            Self::SetWorldOrbit { world, before, .. } => {
                if let Some(w) = sector
                    .systems
                    .iter_mut()
                    .flat_map(|s| s.worlds.iter_mut())
                    .find(|w| w.id == *world)
                {
                    w.orbit = *before;
                }
                Ok(())
            }
            Self::AdvanceConflictTicks {
                before_world,
                before_system,
                before_dominant,
                ..
            } => {
                for (sys_id, conflict) in before_system {
                    if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == *sys_id) {
                        sys.conflict = conflict.clone();
                    }
                }
                for (sys_id, dominant) in before_dominant {
                    if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == *sys_id) {
                        sys.control.dominant = dominant.clone();
                    }
                }
                for (world_id, conflict) in before_world {
                    if let Some(w) = sector
                        .systems
                        .iter_mut()
                        .flat_map(|s| s.worlds.iter_mut())
                        .find(|w| w.id == *world_id)
                    {
                        w.conflict = conflict.clone();
                    }
                }
                Ok(())
            }
            Self::EditWorld { world, before, .. } => {
                if let Some(prev) = before {
                    if let Some(w) = sector
                        .systems
                        .iter_mut()
                        .flat_map(|s| s.worlds.iter_mut())
                        .find(|w| w.id == *world)
                    {
                        *w = (**prev).clone();
                    }
                }
                Ok(())
            }
            Self::EditSystem { system, before, .. } => {
                if let Some(prev) = before {
                    if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == *system) {
                        *sys = (**prev).clone();
                    }
                    sector.manifest.system_count = sector.systems.len();
                    sector.manifest.world_count =
                        sector.systems.iter().map(|s| s.worlds.len()).sum();
                }
                Ok(())
            }
            Self::EditChronicle { before, .. } => {
                if let Some(prev) = before {
                    sector.chronicle = (**prev).clone();
                }
                Ok(())
            }
            Self::ApplyFactionPower { before, .. } => {
                for (fid, prof) in before {
                    if let Some(f) = sector.factions.iter_mut().find(|f| f.id == *fid) {
                        f.power = *prof;
                    }
                }
                Ok(())
            }
            Self::BulkEditWorlds { before, .. } => {
                for (wid, prev) in before {
                    if let Some(w) = sector
                        .systems
                        .iter_mut()
                        .flat_map(|s| s.worlds.iter_mut())
                        .find(|w| w.id == *wid)
                    {
                        *w = prev.clone();
                    }
                }
                Ok(())
            }
            Self::DeriveBaselineIntel {
                before_systems,
                before_worlds,
                ..
            } => {
                for (sid, intel) in before_systems {
                    if let Some(sys) = sector.systems.iter_mut().find(|s| s.id == *sid) {
                        sys.intel = intel.clone();
                    }
                }
                for (wid, intel) in before_worlds {
                    if let Some(w) = sector
                        .systems
                        .iter_mut()
                        .flat_map(|s| s.worlds.iter_mut())
                        .find(|w| w.id == *wid)
                    {
                        w.intel = intel.clone();
                    }
                }
                Ok(())
            }
            // No-op on the sector — `BuilderState::undo` restores the prior
            // catalog/override snapshot (`before`) right after this returns.
            Self::EditCatalog { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> GeneratedSector {
        GeneratedSector::empty("t", "T", "seed", 8, 8)
    }

    /// D9 — bound the inline footprint of the largest `BuilderCommand`
    /// variant. The command is cloned once per dispatch into the undo ring
    /// buffer (up to the configured capacity), so a fat inline payload taxes
    /// every clone. Heap payloads ride behind `Box`/`Vec`; this catches a new
    /// variant that smuggles a large value inline. Re-measure and bump
    /// `MAX` deliberately if a variant legitimately needs more.
    #[test]
    fn builder_command_size_is_bounded() {
        const MAX: usize = 256;
        let actual = std::mem::size_of::<BuilderCommand>();
        assert!(actual <= MAX, "BuilderCommand is {actual} bytes (cap {MAX})");
    }

    /// LD2 — every command classifies into at least one dependency class, and
    /// route / faction / region commands classify to exactly the expected one.
    #[test]
    fn dep_classes_are_assigned() {
        let route = BuilderCommand::AddRoute {
            from: SystemId::from("a"),
            to: SystemId::from("b"),
            route_type: RouteType::ChartedPassage,
            stability: RouteStability::Stable,
            result_id: None,
        };
        assert_eq!(route.dep_classes(), &[DepClass::Routes]);

        let faction = BuilderCommand::AddFaction {
            id: FactionId::from("imperium"),
            name: "Imperium".into(),
            kind: "imperial".into(),
        };
        assert_eq!(faction.dep_classes(), &[DepClass::Factions]);

        let region = BuilderCommand::RenameRegion {
            region: "r".into(),
            before: "a".into(),
            after: "b".into(),
        };
        assert_eq!(region.dep_classes(), &[DepClass::Regions]);

        let world = BuilderCommand::AddWorld {
            system: SystemId::from("a"),
            name: "W".into(),
            result_id: None,
        };
        assert_eq!(world.dep_classes(), &[DepClass::SystemsWorlds]);

        // RemoveSystem also drops incident routes.
        let rm = BuilderCommand::RemoveSystem {
            id: SystemId::from("a"),
            before: None,
            removed_routes: Vec::new(),
        };
        assert_eq!(
            rm.dep_classes(),
            &[DepClass::SystemsWorlds, DepClass::Routes]
        );
    }

    /// D-S1/D3: every variant must classify into a *non-empty* dependency set.
    /// The exhaustive `match` already forces an arm per variant, but cannot
    /// catch an arm that returns `&[]` (which would silently skip derivation
    /// invalidation). Constructing one of each also documents the full command
    /// surface, and the length tripwire forces this list to grow with the enum.
    #[test]
    fn dep_classes_cover_all_variants() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        let wid = s.add_world_to_system(&a, "W").unwrap();
        s.add_faction(FactionId::from("f"), "F", "imperial");
        let rid = s
            .add_route(&a, &b, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        s.add_region(
            "reg-0001",
            "R",
            RegionConditionKind::Blackout,
            HexCoord { q: 0, r: 0 },
        );
        let world = s.get_world(&wid).unwrap().clone();
        let system = s.get_system(&a).unwrap().clone();
        let region = s.regions[0].clone();
        let star = GeneratedStar {
            colour_code: Arc::from("G"),
            colour_name: Arc::from("Yellow"),
            spectral_type: Some(Arc::from("G2V")),
            source_row_index: None,
        };

        let all = vec![
            BuilderCommand::AddSystem {
                coord: HexCoord { q: 5, r: 5 },
                name: "N".into(),
                result_id: None,
            },
            BuilderCommand::RemoveSystem {
                id: a.clone(),
                before: None,
                removed_routes: Vec::new(),
            },
            BuilderCommand::MoveSystem {
                id: a.clone(),
                from: HexCoord { q: 0, r: 0 },
                to: HexCoord { q: 2, r: 2 },
            },
            BuilderCommand::RenameSystem {
                id: a.clone(),
                from: "A".into(),
                to: "A2".into(),
            },
            BuilderCommand::SwapSystems {
                a: a.clone(),
                b: b.clone(),
            },
            BuilderCommand::ReplaceSystem {
                coord: HexCoord { q: 0, r: 0 },
                new_system: Box::new(system.clone()),
                before: None,
            },
            BuilderCommand::AddWorld {
                system: a.clone(),
                name: "W2".into(),
                result_id: None,
            },
            BuilderCommand::RemoveWorld {
                world: wid.clone(),
                before: None,
                parent_system: None,
                parent_position: None,
            },
            BuilderCommand::AddRoute {
                from: a.clone(),
                to: b.clone(),
                route_type: RouteType::ChartedPassage,
                stability: RouteStability::Stable,
                result_id: None,
            },
            BuilderCommand::RemoveRoute {
                id: rid.clone(),
                before: None,
            },
            BuilderCommand::ReplaceRoutes {
                before: Vec::new(),
                after: Vec::new(),
            },
            BuilderCommand::AddFaction {
                id: FactionId::from("g"),
                name: "G".into(),
                kind: "ork".into(),
            },
            BuilderCommand::RemoveFaction {
                id: FactionId::from("f"),
                before: None,
            },
            BuilderCommand::SetArchetype {
                system: a.clone(),
                before: None,
                after: ArchetypeState::default(),
            },
            BuilderCommand::AutoAssignArchetypes {
                flags: ArchetypeApplyFlags::default(),
                before: Vec::new(),
            },
            BuilderCommand::SetOrbitalAssets {
                system: a.clone(),
                before: None,
                after: Vec::new(),
            },
            BuilderCommand::SetBlockadeReport {
                system: a.clone(),
                before: None,
                after: BlockadeReport::default(),
            },
            BuilderCommand::SetSurfaceRegions {
                world: wid.clone(),
                before: None,
                after: Vec::new(),
            },
            BuilderCommand::SetWorldConflict {
                world: wid.clone(),
                before: None,
                after: ConflictState::default(),
            },
            BuilderCommand::SetSystemConflict {
                system: a.clone(),
                before: None,
                after: ConflictState::default(),
            },
            BuilderCommand::SetWorldStability {
                world: wid.clone(),
                before: None,
                after: StabilityState::default(),
            },
            BuilderCommand::SetRouteType {
                id: rid.clone(),
                before: RouteType::StableWarpLane,
                after: RouteType::SmugglingLane,
            },
            BuilderCommand::SetRouteStability {
                id: rid.clone(),
                before: RouteStability::Stable,
                after: RouteStability::Perilous,
            },
            BuilderCommand::SetRegionKind {
                region: "reg-0001".into(),
                before: RegionConditionKind::Blackout,
                after: RegionConditionKind::WarpStorm,
            },
            BuilderCommand::RenameRegion {
                region: "reg-0001".into(),
                before: "R".into(),
                after: "R2".into(),
            },
            BuilderCommand::AddRegion {
                id: "reg-0002".into(),
                name: "R2".into(),
                kind: RegionConditionKind::Blackout,
                centre: HexCoord { q: 0, r: 0 },
            },
            BuilderCommand::RemoveRegion {
                id: "reg-0001".into(),
                before: None,
            },
            BuilderCommand::EditRegion {
                region: "reg-0001".into(),
                before: None,
                after: Box::new(region.clone()),
            },
            BuilderCommand::SetStar {
                system: a.clone(),
                before: None,
                after: Some(star.clone()),
            },
            BuilderCommand::SetStarSpectral {
                system: a.clone(),
                before: None,
                after: Some(Arc::from("M5V")),
            },
            BuilderCommand::RenameWorld {
                world: wid.clone(),
                before: "W".into(),
                after: "W3".into(),
            },
            BuilderCommand::SetWorldOrbit {
                world: wid.clone(),
                before: 0,
                after: 3,
            },
            BuilderCommand::AdvanceConflictTicks {
                ticks: 1,
                before_world: Vec::new(),
                before_system: Vec::new(),
                before_dominant: Vec::new(),
            },
            BuilderCommand::EditWorld {
                world: wid.clone(),
                before: None,
                after: Box::new(world.clone()),
            },
            BuilderCommand::EditSystem {
                system: a.clone(),
                before: None,
                after: Box::new(system.clone()),
            },
            BuilderCommand::EditChronicle {
                before: None,
                after: Box::new(s.chronicle.clone()),
            },
            BuilderCommand::BulkEditWorlds {
                before: Vec::new(),
                after: Vec::new(),
            },
            BuilderCommand::DeriveBaselineIntel {
                observers: Vec::new(),
                before_systems: Vec::new(),
                before_worlds: Vec::new(),
            },
            BuilderCommand::ApplyFactionPower {
                before: Vec::new(),
                after: std::collections::BTreeMap::new(),
            },
            BuilderCommand::EditCatalog {
                before: Box::new(crate::builder::CatalogSnapshot::default()),
                after: Box::new(crate::builder::CatalogSnapshot::default()),
            },
        ];
        assert_eq!(
            all.len(),
            40,
            "add the new BuilderCommand variant to this exhaustive list"
        );
        for cmd in &all {
            assert!(
                !cmd.dep_classes().is_empty(),
                "{cmd:?} returned an empty dep_classes() — derivations would never invalidate"
            );
        }
    }

    #[test]
    fn add_system_round_trip() {
        let mut s = empty();
        let mut cmd = BuilderCommand::AddSystem {
            coord: HexCoord { q: 1, r: 1 },
            name: "Alpha".into(),
            result_id: None,
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(s.systems.len(), 1);
        cmd.revert(&mut s).unwrap();
        assert_eq!(s.systems.len(), 0);
    }

    #[test]
    fn remove_system_round_trip() {
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 2, r: 2 }, "Beta").unwrap();
        let mut cmd = BuilderCommand::RemoveSystem {
            id: id.clone(),
            before: None,
            removed_routes: Vec::new(),
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(s.systems.len(), 0);
        cmd.revert(&mut s).unwrap();
        assert_eq!(s.systems.len(), 1);
        assert_eq!(s.systems[0].id, id);
    }

    #[test]
    fn swap_systems_round_trip() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 4, r: 0 }, "B").unwrap();
        let mut cmd = BuilderCommand::SwapSystems {
            a: a.clone(),
            b: b.clone(),
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(
            s.systems.iter().find(|x| x.id == a).unwrap().coord,
            HexCoord { q: 4, r: 0 }
        );
        cmd.revert(&mut s).unwrap();
        assert_eq!(
            s.systems.iter().find(|x| x.id == a).unwrap().coord,
            HexCoord { q: 0, r: 0 }
        );
    }

    #[test]
    fn apply_faction_power_round_trip() {
        let mut s = empty();
        s.add_faction(FactionId::from("f"), "F", "imperial");
        s.factions[0].power.military = 7.0;
        let mut after = std::collections::BTreeMap::new();
        after.insert(
            FactionId::from("f"),
            PowerProfile {
                military: 42.0,
                ..Default::default()
            },
        );
        let mut cmd = BuilderCommand::ApplyFactionPower {
            before: Vec::new(),
            after,
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(s.factions[0].power.military, 42.0);
        // revert restores the pre-apply snapshot captured on apply
        cmd.revert(&mut s).unwrap();
        assert_eq!(s.factions[0].power.military, 7.0);
    }

    #[test]
    fn replace_system_round_trip() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 1, r: 1 }, "A").unwrap();
        let new_sys = GeneratedSystem::new_at(
            sectorforge::ids::system_id(99),
            99,
            HexCoord { q: 1, r: 1 },
            "Replacement",
        );
        let mut cmd = BuilderCommand::ReplaceSystem {
            coord: HexCoord { q: 1, r: 1 },
            new_system: Box::new(new_sys),
            before: None,
        };
        cmd.apply(&mut s).unwrap();
        assert!(s.systems.iter().any(|x| x.id != a));
        assert!(s.systems.iter().any(|x| &*x.name == "Replacement"));
        cmd.revert(&mut s).unwrap();
        assert!(s.systems.iter().any(|x| x.id == a));
        assert!(!s.systems.iter().any(|x| &*x.name == "Replacement"));
    }

    #[test]
    fn replace_routes_round_trip() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        let c = s.add_system(HexCoord { q: 2, r: 0 }, "C").unwrap();
        s.add_route(&a, &b, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        let before = s.routes.clone();
        let after = vec![GeneratedRoute {
            id: sectorforge::ids::route_id(&b, &c),
            from_system_id: b,
            to_system_id: c,
            distance: 1,
            route_type: RouteType::ChartedPassage,
            stability: RouteStability::Hazardous,
            tags: vec!["bridge".into()],
            controls: Vec::new(),
        }];
        let mut cmd = BuilderCommand::ReplaceRoutes {
            before: Vec::new(),
            after: after.clone(),
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(s.routes, after);
        cmd.revert(&mut s).unwrap();
        assert_eq!(s.routes, before);
    }

    #[test]
    fn set_archetype_round_trip() {
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 1, r: 1 }, "A").unwrap();
        let after = ArchetypeState {
            ork_waaagh: 73,
            necron_phase: sectorforge::archetypes::NecronPhase::Awakening,
            ..Default::default()
        };
        let mut cmd = BuilderCommand::SetArchetype {
            system: id.clone(),
            before: None,
            after: after.clone(),
        };
        cmd.apply(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert_eq!(sys.archetype.ork_waaagh, 73);
        assert_eq!(
            sys.archetype.necron_phase,
            sectorforge::archetypes::NecronPhase::Awakening
        );
        cmd.revert(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert!(ArchetypeState::is_default(&sys.archetype));
    }

    #[test]
    fn auto_assign_archetypes_round_trip_respects_flag_mask() {
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        // Pre-seed a non-default archetype so revert has something to restore.
        let seed = ArchetypeState {
            ork_waaagh: 11,
            ..Default::default()
        };
        s.set_archetype(&id, seed.clone()).unwrap();
        let flags = ArchetypeApplyFlags {
            ork: false,
            ..ArchetypeApplyFlags::default()
        };
        let mut cmd = BuilderCommand::AutoAssignArchetypes {
            flags,
            before: Vec::new(),
        };
        cmd.apply(&mut s).unwrap();
        // Ork axis is masked → defaults restored; pre-seeded value gone.
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert_eq!(sys.archetype.ork_waaagh, 0);
        cmd.revert(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert_eq!(sys.archetype.ork_waaagh, 11);
    }

    #[test]
    fn set_orbital_assets_round_trip() {
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let after = vec![OrbitalAsset {
            id: format!("{id}-manual-1"),
            kind: sectorforge::orbital_assets::OrbitalAssetKind::Station,
            faction_id: FactionId::new("imperium"),
            strength: 80,
            ship_inventory: Vec::new(),
        }];
        let mut cmd = BuilderCommand::SetOrbitalAssets {
            system: id.clone(),
            before: None,
            after: after.clone(),
        };
        cmd.apply(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert_eq!(sys.orbital_assets, after);
        cmd.revert(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert!(sys.orbital_assets.is_empty());
    }

    #[test]
    fn set_blockade_report_round_trip() {
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let after = BlockadeReport {
            under_blockade: true,
            blockader: Some(FactionId::new("orks")),
            besieged: Some(FactionId::new("imperium")),
            intensity: 65,
        };
        let mut cmd = BuilderCommand::SetBlockadeReport {
            system: id.clone(),
            before: None,
            after: after.clone(),
        };
        cmd.apply(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert_eq!(sys.blockade, after);
        cmd.revert(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert!(BlockadeReport::is_default(&sys.blockade));
    }

    #[test]
    fn set_surface_regions_round_trip() {
        use sectorforge::surface_region::{RegionKind, SurfaceRegion};
        let mut s = empty();
        let sys_id = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let world_id = s.add_world_to_system(&sys_id, "Capitis").unwrap();
        let after = vec![SurfaceRegion {
            name: "Hive Primus".into(),
            kind: RegionKind::Hive,
            dominant: Some(FactionId::new("imperium")),
            control_score: 75,
            population_weight: 60,
            visibility: 80,
            notes: "manual entry".into(),
        }];
        let mut cmd = BuilderCommand::SetSurfaceRegions {
            world: world_id.clone(),
            before: None,
            after: after.clone(),
        };
        cmd.apply(&mut s).unwrap();
        let w = s
            .systems
            .iter()
            .flat_map(|sys| sys.worlds.iter())
            .find(|w| w.id == world_id)
            .unwrap();
        assert_eq!(w.regions, after);
        cmd.revert(&mut s).unwrap();
        let w = s
            .systems
            .iter()
            .flat_map(|sys| sys.worlds.iter())
            .find(|w| w.id == world_id)
            .unwrap();
        assert!(w.regions.is_empty());
    }

    #[test]
    fn set_route_type_round_trip() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        let id = s
            .add_route(&a, &b, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        let mut cmd = BuilderCommand::SetRouteType {
            id: id.clone(),
            before: RouteType::StableWarpLane,
            after: RouteType::SmugglingLane,
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(
            s.routes.iter().find(|r| r.id == id).unwrap().route_type,
            RouteType::SmugglingLane
        );
        cmd.revert(&mut s).unwrap();
        assert_eq!(
            s.routes.iter().find(|r| r.id == id).unwrap().route_type,
            RouteType::StableWarpLane
        );
    }

    #[test]
    fn set_route_stability_round_trip() {
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        let id = s
            .add_route(&a, &b, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap();
        let mut cmd = BuilderCommand::SetRouteStability {
            id: id.clone(),
            before: RouteStability::Stable,
            after: RouteStability::Perilous,
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(
            s.routes.iter().find(|r| r.id == id).unwrap().stability,
            RouteStability::Perilous
        );
        cmd.revert(&mut s).unwrap();
        assert_eq!(
            s.routes.iter().find(|r| r.id == id).unwrap().stability,
            RouteStability::Stable
        );
    }

    #[test]
    fn set_region_kind_round_trip() {
        let mut s = empty();
        s.add_region(
            "reg-0001",
            "Veil",
            RegionConditionKind::Blackout,
            HexCoord { q: 1, r: 1 },
        );
        let mut cmd = BuilderCommand::SetRegionKind {
            region: "reg-0001".into(),
            before: RegionConditionKind::Blackout,
            after: RegionConditionKind::WarpStorm,
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(
            s.regions.iter().find(|r| r.id == "reg-0001").unwrap().kind,
            RegionConditionKind::WarpStorm
        );
        cmd.revert(&mut s).unwrap();
        assert_eq!(
            s.regions.iter().find(|r| r.id == "reg-0001").unwrap().kind,
            RegionConditionKind::Blackout
        );
    }

    #[test]
    fn rename_region_round_trip() {
        let mut s = empty();
        s.add_region(
            "reg-0001",
            "Veil",
            RegionConditionKind::Blackout,
            HexCoord { q: 1, r: 1 },
        );
        let mut cmd = BuilderCommand::RenameRegion {
            region: "reg-0001".into(),
            before: String::new(),
            after: "Sealed Mirror".into(),
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(
            s.regions.iter().find(|r| r.id == "reg-0001").unwrap().name,
            "Sealed Mirror"
        );
        cmd.revert(&mut s).unwrap();
        assert_eq!(
            s.regions.iter().find(|r| r.id == "reg-0001").unwrap().name,
            "Veil"
        );
    }

    #[test]
    fn add_region_round_trip() {
        let mut s = empty();
        let mut cmd = BuilderCommand::AddRegion {
            id: "reg-0001".into(),
            name: "Veil".into(),
            kind: RegionConditionKind::Blackout,
            centre: HexCoord { q: 2, r: 2 },
        };
        cmd.apply(&mut s).unwrap();
        assert!(s.regions.iter().any(|r| r.id == "reg-0001"));
        cmd.revert(&mut s).unwrap();
        assert!(!s.regions.iter().any(|r| r.id == "reg-0001"));
    }

    #[test]
    fn remove_region_round_trip() {
        let mut s = empty();
        s.add_region(
            "reg-0001",
            "Veil",
            RegionConditionKind::Blackout,
            HexCoord { q: 1, r: 1 },
        );
        let mut cmd = BuilderCommand::RemoveRegion {
            id: "reg-0001".into(),
            before: None,
        };
        cmd.apply(&mut s).unwrap();
        assert!(s.regions.iter().all(|r| r.id != "reg-0001"));
        cmd.revert(&mut s).unwrap();
        let r = s.regions.iter().find(|r| r.id == "reg-0001").unwrap();
        assert_eq!(r.name, "Veil");
    }

    #[test]
    fn edit_region_captures_before_when_none() {
        // REG1/REG3 path: live region untouched at dispatch, so apply captures
        // the live state as `before`.
        let mut s = empty();
        s.add_region(
            "reg-0001",
            "Veil",
            RegionConditionKind::Blackout,
            HexCoord { q: 1, r: 1 },
        );
        let mut after = s.regions.iter().find(|r| r.id == "reg-0001").unwrap().clone();
        after.name = "Sealed".into();
        after.hexes.push(HexCoord { q: 2, r: 1 });
        let mut cmd = BuilderCommand::EditRegion {
            region: "reg-0001".into(),
            before: None,
            after: Box::new(after),
        };
        cmd.apply(&mut s).unwrap();
        let r = s.regions.iter().find(|r| r.id == "reg-0001").unwrap();
        assert_eq!(r.name, "Sealed");
        assert_eq!(r.hexes.len(), 2);
        cmd.revert(&mut s).unwrap();
        let r = s.regions.iter().find(|r| r.id == "reg-0001").unwrap();
        assert_eq!(r.name, "Veil");
        assert_eq!(r.hexes.len(), 1);
    }

    #[test]
    fn edit_region_keeps_caller_supplied_before() {
        // D1 stroke path: the brush pre-applies to the live region for preview,
        // then commits with `before` already set. apply must NOT re-capture the
        // post-stroke state, or revert would be a no-op.
        let mut s = empty();
        s.add_region(
            "reg-0001",
            "Veil",
            RegionConditionKind::Blackout,
            HexCoord { q: 1, r: 1 },
        );
        let pre = s.regions.iter().find(|r| r.id == "reg-0001").unwrap().clone();
        // Live preview painted an extra hex before the commit.
        s.add_region_hex("reg-0001", HexCoord { q: 2, r: 1 }).unwrap();
        let after = s.regions.iter().find(|r| r.id == "reg-0001").unwrap().clone();
        let mut cmd = BuilderCommand::EditRegion {
            region: "reg-0001".into(),
            before: Some(Box::new(pre)),
            after: Box::new(after),
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(
            s.regions.iter().find(|r| r.id == "reg-0001").unwrap().hexes.len(),
            2
        );
        cmd.revert(&mut s).unwrap();
        assert_eq!(
            s.regions.iter().find(|r| r.id == "reg-0001").unwrap().hexes.len(),
            1,
            "revert must restore the pre-stroke footprint"
        );
    }

    #[test]
    fn set_route_type_unknown_route_errors() {
        let mut s = empty();
        let mut cmd = BuilderCommand::SetRouteType {
            id: sectorforge::ids::route_id(
                &sectorforge::ids::system_id(1),
                &sectorforge::ids::system_id(2),
            ),
            before: RouteType::StableWarpLane,
            after: RouteType::Webway,
        };
        assert!(matches!(
            cmd.apply(&mut s),
            Err(MutationError::RouteNotFound(_))
        ));
    }

    #[test]
    fn set_star_round_trip() {
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let new_star = GeneratedStar {
            colour_code: Arc::from("G"),
            colour_name: Arc::from("Yellow"),
            spectral_type: Some(Arc::from("G2V")),
            source_row_index: None,
        };
        let mut cmd = BuilderCommand::SetStar {
            system: id.clone(),
            before: None,
            after: Some(new_star.clone()),
        };
        cmd.apply(&mut s).unwrap();
        assert!(s
            .systems
            .iter()
            .find(|x| x.id == id)
            .unwrap()
            .star
            .as_ref()
            .map(|st| &*st.colour_code == "G")
            .unwrap_or(false));
        cmd.revert(&mut s).unwrap();
        assert!(s
            .systems
            .iter()
            .find(|x| x.id == id)
            .unwrap()
            .star
            .is_none());
    }

    #[test]
    fn remove_star_clears_spectral() {
        // §CTX1 Phase 6 — REMOVE STAR drops the entire GeneratedStar payload,
        // which by construction takes the spectral_type with it.
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        if let Some(sys) = s.systems.iter_mut().find(|x| x.id == id) {
            sys.star = Some(GeneratedStar {
                colour_code: Arc::from("G"),
                colour_name: Arc::from("Yellow"),
                spectral_type: Some(Arc::from("G2V")),
                source_row_index: None,
            });
        }
        let mut cmd = BuilderCommand::SetStar {
            system: id.clone(),
            before: None,
            after: None,
        };
        cmd.apply(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert!(sys.star.is_none());
        cmd.revert(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert_eq!(
            sys.star.as_ref().unwrap().spectral_type.as_deref(),
            Some("G2V")
        );
    }

    #[test]
    fn set_star_spectral_round_trip() {
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        if let Some(sys) = s.systems.iter_mut().find(|x| x.id == id) {
            sys.star = Some(GeneratedStar {
                colour_code: Arc::from("G"),
                colour_name: Arc::from("Yellow"),
                spectral_type: Some(Arc::from("G2V")),
                source_row_index: None,
            });
        }
        let mut cmd = BuilderCommand::SetStarSpectral {
            system: id.clone(),
            before: None,
            after: Some(Arc::from("M5V")),
        };
        cmd.apply(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert_eq!(
            sys.star.as_ref().unwrap().spectral_type.as_deref(),
            Some("M5V")
        );
        cmd.revert(&mut s).unwrap();
        let sys = s.systems.iter().find(|x| x.id == id).unwrap();
        assert_eq!(
            sys.star.as_ref().unwrap().spectral_type.as_deref(),
            Some("G2V")
        );
    }

    #[test]
    fn rename_world_round_trip() {
        let mut s = empty();
        let sys = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let wid = s.add_world_to_system(&sys, "Original").unwrap();
        let mut cmd = BuilderCommand::RenameWorld {
            world: wid.clone(),
            before: String::new(),
            after: "Patched".into(),
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(
            &*s.all_worlds().find(|w| w.id == wid).unwrap().name,
            "Patched"
        );
        cmd.revert(&mut s).unwrap();
        assert_eq!(
            &*s.all_worlds().find(|w| w.id == wid).unwrap().name,
            "Original"
        );
    }

    #[test]
    fn set_world_orbit_round_trip() {
        let mut s = empty();
        let sys = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let wid = s.add_world_to_system(&sys, "W").unwrap();
        let before = s.all_worlds().find(|w| w.id == wid).unwrap().orbit;
        let mut cmd = BuilderCommand::SetWorldOrbit {
            world: wid.clone(),
            before: 0,
            after: 7,
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(s.all_worlds().find(|w| w.id == wid).unwrap().orbit, 7);
        cmd.revert(&mut s).unwrap();
        assert_eq!(s.all_worlds().find(|w| w.id == wid).unwrap().orbit, before);
    }

    #[test]
    fn set_star_spectral_errors_when_no_star() {
        // §CTX1 Phase 6 — CYCLE SPECTRAL CLASS must hide when no star is
        // present; the command itself errors out as a defence-in-depth check.
        let mut s = empty();
        let id = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let mut cmd = BuilderCommand::SetStarSpectral {
            system: id,
            before: None,
            after: Some(Arc::from("G2V")),
        };
        assert!(matches!(
            cmd.apply(&mut s),
            Err(MutationError::SystemNotFound(_))
        ));
    }

    /// R8 determinism: a fixed command sequence applied to a blank sector
    /// must produce the same canonical-JSON BLAKE3 digest across runs.
    #[test]
    fn command_log_determinism_blake3() {
        use sectorforge::rng::digest_bytes;
        fn replay() -> String {
            let mut s = GeneratedSector::empty("t", "T", "seed", 8, 8);
            let cmds = vec![
                BuilderCommand::AddSystem {
                    coord: HexCoord { q: 1, r: 1 },
                    name: "Alpha".into(),
                    result_id: None,
                },
                BuilderCommand::AddSystem {
                    coord: HexCoord { q: 2, r: 3 },
                    name: "Beta".into(),
                    result_id: None,
                },
                BuilderCommand::AddSystem {
                    coord: HexCoord { q: 4, r: 5 },
                    name: "Gamma".into(),
                    result_id: None,
                },
            ];
            for mut c in cmds {
                c.apply(&mut s).unwrap();
            }
            digest_bytes(&serde_json::to_vec(&s).unwrap())
        }
        let a = replay();
        let b = replay();
        assert_eq!(a, b, "BuilderCommand log must be byte-stable");
    }

    #[test]
    fn edit_world_round_trip() {
        let mut s = empty();
        let sys = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let wid = s.add_world_to_system(&sys, "W").unwrap();
        let mut edited = s.get_world(&wid).unwrap().clone();
        edited.tags.push(Arc::from("hive"));
        edited.notes.push(Arc::from("manual edit"));
        let mut cmd = BuilderCommand::EditWorld {
            world: wid.clone(),
            before: None,
            after: Box::new(edited),
        };
        cmd.apply(&mut s).unwrap();
        let w = s.get_world(&wid).unwrap();
        assert!(w.tags.iter().any(|t| &**t == "hive"));
        assert!(w.notes.iter().any(|n| &**n == "manual edit"));
        cmd.revert(&mut s).unwrap();
        let w = s.get_world(&wid).unwrap();
        assert!(w.tags.is_empty());
        assert!(w.notes.is_empty());
    }

    #[test]
    fn edit_system_round_trip() {
        let mut s = empty();
        let sys = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        // A world rides along on the system; EditSystem must preserve it.
        let wid = s.add_world_to_system(&sys, "W").unwrap();
        let mut edited = s.get_system(&sys).unwrap().clone();
        edited.tags.push(Arc::from("frontier"));
        let mut cmd = BuilderCommand::EditSystem {
            system: sys.clone(),
            before: None,
            after: Box::new(edited),
        };
        cmd.apply(&mut s).unwrap();
        let sysref = s.get_system(&sys).unwrap();
        assert!(sysref.tags.iter().any(|t| &**t == "frontier"));
        assert!(sysref.worlds.iter().any(|w| w.id == wid));
        cmd.revert(&mut s).unwrap();
        let sysref = s.get_system(&sys).unwrap();
        assert!(sysref.tags.is_empty());
        assert!(sysref.worlds.iter().any(|w| w.id == wid));
    }

    #[test]
    fn edit_chronicle_round_trip() {
        use sectorforge::history::{EventKind, HistoryAnchor, HistoryEvent};
        let mut s = empty();
        assert!(s.chronicle.events.is_empty());
        let mut chron = s.chronicle.clone();
        chron.events.push(HistoryEvent {
            id: "ev-test".into(),
            date: "M41.000".into(),
            era_id: String::new(),
            era_label: String::new(),
            relative_year: 0,
            anchor: HistoryAnchor::Sector,
            kind: EventKind::Foundation,
            summary: String::new(),
            narrative: "test event".into(),
            factions: Vec::new(),
            entities: Vec::new(),
            consequences: Vec::new(),
            weight: 10,
            manual: true,
        });
        let mut cmd = BuilderCommand::EditChronicle {
            before: None,
            after: Box::new(chron),
        };
        cmd.apply(&mut s).unwrap();
        assert_eq!(s.chronicle.events.len(), 1);
        cmd.revert(&mut s).unwrap();
        assert!(s.chronicle.events.is_empty());
    }

    #[test]
    fn bulk_edit_worlds_round_trip() {
        let mut s = empty();
        let sys = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let w1 = s.add_world_to_system(&sys, "W1").unwrap();
        let w2 = s.add_world_to_system(&sys, "W2").unwrap();
        let mut e1 = s.get_world(&w1).unwrap().clone();
        e1.tags.push(Arc::from("x"));
        let mut e2 = s.get_world(&w2).unwrap().clone();
        e2.tags.push(Arc::from("y"));
        let mut cmd = BuilderCommand::BulkEditWorlds {
            before: Vec::new(),
            after: vec![(w1.clone(), e1), (w2.clone(), e2)],
        };
        cmd.apply(&mut s).unwrap();
        assert!(s.get_world(&w1).unwrap().tags.iter().any(|t| &**t == "x"));
        assert!(s.get_world(&w2).unwrap().tags.iter().any(|t| &**t == "y"));
        cmd.revert(&mut s).unwrap();
        assert!(s.get_world(&w1).unwrap().tags.is_empty());
        assert!(s.get_world(&w2).unwrap().tags.is_empty());
    }

    #[test]
    fn derive_baseline_intel_round_trip() {
        let mut s = empty();
        s.add_faction(FactionId::from("imperium"), "Imperium", "imperial");
        let sys = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let wid = s.add_world_to_system(&sys, "W").unwrap();
        let sys_intel_before_empty = s.get_system(&sys).unwrap().intel.is_empty();
        let world_intel_before_empty = s.get_world(&wid).unwrap().intel.is_empty();
        let mut cmd = BuilderCommand::DeriveBaselineIntel {
            observers: vec!["imperium".to_string()],
            before_systems: Vec::new(),
            before_worlds: Vec::new(),
        };
        cmd.apply(&mut s).unwrap();
        // Snapshots captured for every system + world.
        if let BuilderCommand::DeriveBaselineIntel {
            before_systems,
            before_worlds,
            ..
        } = &cmd
        {
            assert_eq!(before_systems.len(), 1);
            assert_eq!(before_worlds.len(), 1);
        } else {
            unreachable!();
        }
        cmd.revert(&mut s).unwrap();
        // Revert restores the pre-derive intel exactly.
        assert_eq!(
            s.get_system(&sys).unwrap().intel.is_empty(),
            sys_intel_before_empty
        );
        assert_eq!(
            s.get_world(&wid).unwrap().intel.is_empty(),
            world_intel_before_empty
        );
    }
}
