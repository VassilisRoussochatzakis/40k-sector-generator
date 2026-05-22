//! Sector diff & state-comparison tool (§10 NEW.md).
//!
//! Read-only structural comparison of two `GeneratedSector` values, returning
//! a deterministic, model-aware report rather than a raw JSON diff. Useful
//! for:
//!
//! * Comparing two seeds side-by-side ("which seed gave us the more
//!   contested sector?").
//! * Reporting the consequence of one or more `advance_sector` ticks
//!   ("between session 5 and session 6 the Eastern Reach lost three worlds
//!   to Chaos").
//!
//! The diff is a pure derivation: same inputs ⇒ same output. Entity
//! matching uses the stable IDs (`sys-NNNN`, `route-...`) that the
//! generator already assigns, so a rename is reported as a modification
//! rather than a delete + add.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use camino::Utf8Path;
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;
use crate::ids::{FactionId, RouteId, SystemId, WorldId};
use crate::sector_model::{
    ClaimType, FactionClaim, GeneratedSector, GeneratedSystem, GeneratedWorld, RouteStability,
    RouteType, SystemState, WorldFactionPresence,
};

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiffConfig {
    /// Skip world-level details when only a handful of high-level system or
    /// route changes are interesting. Default false (verbose).
    #[serde(default)]
    pub skip_worlds: bool,
    /// Skip per-route detail. Default false.
    #[serde(default)]
    pub skip_routes: bool,
    /// Minimum absolute change in faction projection power to report. Smaller
    /// movements are filtered as noise. Default 1.0.
    #[serde(default = "default_min_delta")]
    pub min_faction_delta: f32,
    /// Number of top faction deltas listed in the Markdown digest. Default 10.
    #[serde(default = "default_top_n")]
    pub top_faction_deltas: u32,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            skip_worlds: false,
            skip_routes: false,
            min_faction_delta: default_min_delta(),
            top_faction_deltas: default_top_n(),
        }
    }
}

fn default_min_delta() -> f32 {
    1.0
}
fn default_top_n() -> u32 {
    10
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SectorDiff {
    pub before_id: String,
    pub after_id: String,
    pub before_seed: String,
    pub after_seed: String,
    pub before_version: String,
    pub after_version: String,
    /// True iff the sector ids and generator versions match. When false,
    /// downstream consumers should treat the diff as best-effort.
    pub catalog_compatible: bool,
    pub schema_warnings: Vec<String>,
    pub system_count_before: u32,
    pub system_count_after: u32,
    pub route_count_before: u32,
    pub route_count_after: u32,
    pub systems_added: Vec<SystemRef>,
    pub systems_removed: Vec<SystemRef>,
    pub systems_changed: Vec<SystemDiff>,
    pub routes_added: Vec<RouteRef>,
    pub routes_removed: Vec<RouteRef>,
    pub routes_changed: Vec<RouteDiff>,
    pub faction_deltas: Vec<FactionDelta>,
    /// §4 NEW.md: pairs whose stance changed between before/after.
    #[serde(default)]
    pub stance_changes: Vec<StanceChange>,
    /// §5 NEW.md: added/removed regions by id, plus kind / hex-count deltas.
    #[serde(default)]
    pub regions_added: Vec<RegionRef>,
    #[serde(default)]
    pub regions_removed: Vec<RegionRef>,
    #[serde(default)]
    pub regions_changed: Vec<RegionDelta>,
    /// §12 NEW.md: scalar economy snapshot delta (sector totals only).
    #[serde(default)]
    pub economy_balance_changes: Vec<EconomyBalanceChange>,
    #[serde(default)]
    pub stranded_added: Vec<WorldId>,
    #[serde(default)]
    pub stranded_removed: Vec<WorldId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StanceChange {
    pub a: FactionId,
    pub b: FactionId,
    pub before: crate::relations::Stance,
    pub after: crate::relations::Stance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionRef {
    pub id: String,
    pub kind: crate::regions::RegionConditionKind,
    pub hex_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionDelta {
    pub id: String,
    pub kind_before: crate::regions::RegionConditionKind,
    pub kind_after: crate::regions::RegionConditionKind,
    pub hex_count_before: u32,
    pub hex_count_after: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomyBalanceChange {
    pub resource: String,
    pub before: f32,
    pub after: f32,
    pub delta: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemRef {
    pub id: SystemId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemDiff {
    pub id: SystemId,
    pub name_before: String,
    pub name_after: String,
    pub renamed: bool,
    pub state_before: Option<SystemState>,
    pub state_after: Option<SystemState>,
    pub dominant_before: Option<FactionId>,
    pub dominant_after: Option<FactionId>,
    pub sovereign_before: Option<FactionId>,
    pub sovereign_after: Option<FactionId>,
    pub occupier_before: Option<FactionId>,
    pub occupier_after: Option<FactionId>,
    pub primary_factions_added: Vec<FactionId>,
    pub primary_factions_removed: Vec<FactionId>,
    pub worlds_added: Vec<WorldRef>,
    pub worlds_removed: Vec<WorldRef>,
    pub worlds_changed: Vec<WorldDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldRef {
    pub id: WorldId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldDiff {
    pub id: WorldId,
    pub name_before: String,
    pub name_after: String,
    pub renamed: bool,
    pub dominant_before: Option<FactionId>,
    pub dominant_after: Option<FactionId>,
    pub sovereign_before: Option<FactionId>,
    pub sovereign_after: Option<FactionId>,
    pub occupier_before: Option<FactionId>,
    pub occupier_after: Option<FactionId>,
    pub hidden_master_before: Option<FactionId>,
    pub hidden_master_after: Option<FactionId>,
    pub contested_before: bool,
    pub contested_after: bool,
    pub claims_added: Vec<FactionClaim>,
    pub claims_removed: Vec<FactionClaim>,
    pub claim_strength_changes: Vec<ClaimStrengthDelta>,
    pub presences_added: Vec<FactionId>,
    pub presences_removed: Vec<FactionId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimStrengthDelta {
    pub faction_id: crate::ids::FactionId,
    pub claim_type: ClaimType,
    pub strength_before: u8,
    pub strength_after: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRef {
    pub id: RouteId,
    pub from_system_id: SystemId,
    pub to_system_id: SystemId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDiff {
    pub id: RouteId,
    pub from_system_id: SystemId,
    pub to_system_id: SystemId,
    pub stability_before: RouteStability,
    pub stability_after: RouteStability,
    pub route_type_before: RouteType,
    pub route_type_after: RouteType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionDelta {
    pub faction_id: crate::ids::FactionId,
    pub name: String,
    pub total_projection_before: f32,
    pub total_projection_after: f32,
    pub delta: f32,
    pub world_presence_delta: i32,
    pub system_presence_delta: i32,
}

// ── Entry points ───────────────────────────────────────────────────────────────

/// Pure derivation. Never fails.
#[must_use]
pub fn diff_sectors(before: &GeneratedSector, after: &GeneratedSector) -> SectorDiff {
    diff_sectors_with(before, after, &DiffConfig::default())
}

#[must_use]
pub fn diff_sectors_with(
    before: &GeneratedSector,
    after: &GeneratedSector,
    cfg: &DiffConfig,
) -> SectorDiff {
    let mut out = SectorDiff {
        before_id: before.id.to_string(),
        after_id: after.id.to_string(),
        before_seed: before.seed.to_string(),
        after_seed: after.seed.to_string(),
        before_version: before.generator_version.to_string(),
        after_version: after.generator_version.to_string(),
        catalog_compatible: before.id == after.id
            && before.generator_version == after.generator_version,
        schema_warnings: Vec::new(),
        system_count_before: before.systems.len() as u32,
        system_count_after: after.systems.len() as u32,
        route_count_before: before.routes.len() as u32,
        route_count_after: after.routes.len() as u32,
        ..Default::default()
    };
    if before.id != after.id {
        out.schema_warnings.push(format!(
            "sector id changed: `{}` -> `{}`; diff is best-effort across distinct sectors",
            before.id, after.id
        ));
    }
    if before.generator_version != after.generator_version {
        out.schema_warnings.push(format!(
            "generator version changed: `{}` -> `{}`; structural diffs may reflect schema drift",
            before.generator_version, after.generator_version
        ));
    }

    diff_systems(before, after, cfg, &mut out);
    if !cfg.skip_routes {
        diff_routes(before, after, &mut out);
    }
    out.faction_deltas = compute_faction_deltas(before, after, cfg);
    out.stance_changes = diff_relations(before, after);
    let (regions_added, regions_removed, regions_changed) = diff_regions(before, after);
    out.regions_added = regions_added;
    out.regions_removed = regions_removed;
    out.regions_changed = regions_changed;
    let (balance_changes, stranded_added, stranded_removed) = diff_economy(before, after, cfg);
    out.economy_balance_changes = balance_changes;
    out.stranded_added = stranded_added;
    out.stranded_removed = stranded_removed;
    out
}

fn diff_relations(before: &GeneratedSector, after: &GeneratedSector) -> Vec<StanceChange> {
    let mut by_pair: BTreeMap<(FactionId, FactionId), crate::relations::Stance> = BTreeMap::new();
    for p in &before.relations.pairs {
        by_pair.insert((p.a.clone(), p.b.clone()), p.stance);
    }
    let mut out: Vec<StanceChange> = Vec::with_capacity(after.relations.pairs.len());
    for p in &after.relations.pairs {
        let key = (p.a.clone(), p.b.clone());
        if let Some(prev) = by_pair.get(&key) {
            if *prev != p.stance {
                out.push(StanceChange {
                    a: p.a.clone(),
                    b: p.b.clone(),
                    before: *prev,
                    after: p.stance,
                });
            }
        }
    }
    out.sort_unstable_by(|x, y| x.a.cmp(&y.a).then_with(|| x.b.cmp(&y.b)));
    out
}

fn diff_regions(
    before: &GeneratedSector,
    after: &GeneratedSector,
) -> (Vec<RegionRef>, Vec<RegionRef>, Vec<RegionDelta>) {
    let b_idx: BTreeMap<&str, &crate::regions::WarpRegion> =
        before.regions.iter().map(|r| (r.id.as_str(), r)).collect();
    let a_idx: BTreeMap<&str, &crate::regions::WarpRegion> =
        after.regions.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut added: Vec<RegionRef> = Vec::new();
    let mut removed: Vec<RegionRef> = Vec::new();
    let mut changed: Vec<RegionDelta> = Vec::new();
    let all: BTreeSet<&str> = b_idx.keys().copied().chain(a_idx.keys().copied()).collect();
    for id in all {
        match (b_idx.get(id), a_idx.get(id)) {
            (None, Some(r)) => added.push(RegionRef {
                id: r.id.clone(),
                kind: r.kind,
                hex_count: r.hexes.len() as u32,
            }),
            (Some(r), None) => removed.push(RegionRef {
                id: r.id.clone(),
                kind: r.kind,
                hex_count: r.hexes.len() as u32,
            }),
            (Some(a), Some(b)) => {
                if a.kind != b.kind || a.hexes.len() != b.hexes.len() {
                    changed.push(RegionDelta {
                        id: a.id.clone(),
                        kind_before: a.kind,
                        kind_after: b.kind,
                        hex_count_before: a.hexes.len() as u32,
                        hex_count_after: b.hexes.len() as u32,
                    });
                }
            }
            (None, None) => {}
        }
    }
    (added, removed, changed)
}

fn diff_economy(
    before: &GeneratedSector,
    after: &GeneratedSector,
    cfg: &DiffConfig,
) -> (Vec<EconomyBalanceChange>, Vec<WorldId>, Vec<WorldId>) {
    let mut deltas: Vec<EconomyBalanceChange> = Vec::new();
    if before.economy.enabled || after.economy.enabled {
        for k in crate::economy::RESOURCE_KEYS {
            let pull = |s: &crate::economy::EconomyReport| match *k {
                "ore" => s.sector_balance.ore,
                "promethium" => s.sector_balance.promethium,
                "foodstuffs" => s.sector_balance.foodstuffs,
                "manufactured" => s.sector_balance.manufactured,
                "archeotech" => s.sector_balance.archeotech,
                "recruits" => s.sector_balance.recruits,
                _ => 0.0,
            };
            let b = pull(&before.economy);
            let a = pull(&after.economy);
            let d = a - b;
            if d.abs() >= cfg.min_faction_delta {
                deltas.push(EconomyBalanceChange {
                    resource: (*k).to_string(),
                    before: b,
                    after: a,
                    delta: d,
                });
            }
        }
    }
    let b_stranded: BTreeSet<&str> = before
        .economy
        .worlds
        .iter()
        .filter(|w| w.stranded)
        .map(|w| w.world_id.as_str())
        .collect();
    let a_stranded: BTreeSet<&str> = after
        .economy
        .worlds
        .iter()
        .filter(|w| w.stranded)
        .map(|w| w.world_id.as_str())
        .collect();
    let added: Vec<WorldId> = a_stranded
        .difference(&b_stranded)
        .map(|s| WorldId::new(*s))
        .collect();
    let removed: Vec<WorldId> = b_stranded
        .difference(&a_stranded)
        .map(|s| WorldId::new(*s))
        .collect();
    (deltas, added, removed)
}

// ── System diff ────────────────────────────────────────────────────────────────

fn diff_systems(
    before: &GeneratedSector,
    after: &GeneratedSector,
    cfg: &DiffConfig,
    out: &mut SectorDiff,
) {
    let before_idx: BTreeMap<&str, &GeneratedSystem> =
        before.systems.iter().map(|s| (s.id.as_str(), s)).collect();
    let after_idx: BTreeMap<&str, &GeneratedSystem> =
        after.systems.iter().map(|s| (s.id.as_str(), s)).collect();

    let mut added: Vec<SystemRef> = Vec::new();
    let mut removed: Vec<SystemRef> = Vec::new();
    let mut changed: Vec<SystemDiff> = Vec::new();

    let all_ids: BTreeSet<&str> = before_idx
        .keys()
        .copied()
        .chain(after_idx.keys().copied())
        .collect();
    for id in all_ids {
        match (before_idx.get(id), after_idx.get(id)) {
            (None, Some(b)) => added.push(SystemRef {
                id: b.id.clone(),
                name: b.name.to_string(),
            }),
            (Some(a), None) => removed.push(SystemRef {
                id: a.id.clone(),
                name: a.name.to_string(),
            }),
            (Some(a), Some(b)) => {
                if let Some(d) = system_diff(a, b, cfg) {
                    changed.push(d);
                }
            }
            (None, None) => unreachable!(),
        }
    }
    out.systems_added = added;
    out.systems_removed = removed;
    out.systems_changed = changed;
}

fn system_diff(a: &GeneratedSystem, b: &GeneratedSystem, cfg: &DiffConfig) -> Option<SystemDiff> {
    let renamed = a.name != b.name;
    let state_changed = a.control.state != b.control.state;
    let dominant_changed = a.control.dominant != b.control.dominant;
    let sovereign_changed = a.control.sovereign != b.control.sovereign;
    let occupier_changed = a.control.orbital_controller != b.control.orbital_controller;

    let a_pf: BTreeSet<&str> = a.primary_factions.iter().map(|s| s.as_ref()).collect();
    let b_pf: BTreeSet<&str> = b.primary_factions.iter().map(|s| s.as_ref()).collect();
    let pf_added: Vec<FactionId> = b_pf.difference(&a_pf).map(|s| FactionId::new(*s)).collect();
    let pf_removed: Vec<FactionId> = a_pf.difference(&b_pf).map(|s| FactionId::new(*s)).collect();

    let (worlds_added, worlds_removed, worlds_changed) = if cfg.skip_worlds {
        (Vec::new(), Vec::new(), Vec::new())
    } else {
        diff_worlds(&a.worlds, &b.worlds)
    };

    let any_change = renamed
        || state_changed
        || dominant_changed
        || sovereign_changed
        || occupier_changed
        || !pf_added.is_empty()
        || !pf_removed.is_empty()
        || !worlds_added.is_empty()
        || !worlds_removed.is_empty()
        || !worlds_changed.is_empty();
    if !any_change {
        return None;
    }

    Some(SystemDiff {
        id: a.id.clone(),
        name_before: a.name.to_string(),
        name_after: b.name.to_string(),
        renamed,
        state_before: a.control.state,
        state_after: b.control.state,
        dominant_before: a.control.dominant.clone(),
        dominant_after: b.control.dominant.clone(),
        sovereign_before: a.control.sovereign.clone(),
        sovereign_after: b.control.sovereign.clone(),
        occupier_before: a.control.orbital_controller.clone(),
        occupier_after: b.control.orbital_controller.clone(),
        primary_factions_added: pf_added,
        primary_factions_removed: pf_removed,
        worlds_added,
        worlds_removed,
        worlds_changed,
    })
}

// ── World diff ─────────────────────────────────────────────────────────────────

fn diff_worlds(
    before: &[GeneratedWorld],
    after: &[GeneratedWorld],
) -> (Vec<WorldRef>, Vec<WorldRef>, Vec<WorldDiff>) {
    let before_idx: BTreeMap<&str, &GeneratedWorld> =
        before.iter().map(|w| (w.id.as_str(), w)).collect();
    let after_idx: BTreeMap<&str, &GeneratedWorld> =
        after.iter().map(|w| (w.id.as_str(), w)).collect();

    let mut added: Vec<WorldRef> = Vec::new();
    let mut removed: Vec<WorldRef> = Vec::new();
    let mut changed: Vec<WorldDiff> = Vec::new();
    let all_ids: BTreeSet<&str> = before_idx
        .keys()
        .copied()
        .chain(after_idx.keys().copied())
        .collect();
    for id in all_ids {
        match (before_idx.get(id), after_idx.get(id)) {
            (None, Some(w)) => added.push(WorldRef {
                id: w.id.clone(),
                name: w.name.to_string(),
            }),
            (Some(w), None) => removed.push(WorldRef {
                id: w.id.clone(),
                name: w.name.to_string(),
            }),
            (Some(a), Some(b)) => {
                if let Some(d) = world_diff(a, b) {
                    changed.push(d);
                }
            }
            (None, None) => unreachable!(),
        }
    }
    (added, removed, changed)
}

fn world_diff(a: &GeneratedWorld, b: &GeneratedWorld) -> Option<WorldDiff> {
    let renamed = a.name != b.name;
    let control_changed = a.control.dominant != b.control.dominant
        || a.control.sovereign != b.control.sovereign
        || a.control.occupier != b.control.occupier
        || a.control.hidden_master != b.control.hidden_master
        || a.control.contested != b.control.contested;

    let (claims_added, claims_removed, claim_strength_changes) = diff_claims(&a.claims, &b.claims);
    let (presences_added, presences_removed) = diff_presences(&a.factions, &b.factions);

    let any = renamed
        || control_changed
        || !claims_added.is_empty()
        || !claims_removed.is_empty()
        || !claim_strength_changes.is_empty()
        || !presences_added.is_empty()
        || !presences_removed.is_empty();
    if !any {
        return None;
    }
    Some(WorldDiff {
        id: a.id.clone(),
        name_before: a.name.to_string(),
        name_after: b.name.to_string(),
        renamed,
        dominant_before: a.control.dominant.clone(),
        dominant_after: b.control.dominant.clone(),
        sovereign_before: a.control.sovereign.clone(),
        sovereign_after: b.control.sovereign.clone(),
        occupier_before: a.control.occupier.clone(),
        occupier_after: b.control.occupier.clone(),
        hidden_master_before: a.control.hidden_master.clone(),
        hidden_master_after: b.control.hidden_master.clone(),
        contested_before: a.control.contested,
        contested_after: b.control.contested,
        claims_added,
        claims_removed,
        claim_strength_changes,
        presences_added,
        presences_removed,
    })
}

fn diff_claims(
    before: &[FactionClaim],
    after: &[FactionClaim],
) -> (
    Vec<FactionClaim>,
    Vec<FactionClaim>,
    Vec<ClaimStrengthDelta>,
) {
    type Key = (FactionId, ClaimType);
    let before_map: BTreeMap<Key, u8> = before
        .iter()
        .map(|c| ((c.faction_id.clone(), c.claim_type), c.strength))
        .collect();
    let after_map: BTreeMap<Key, u8> = after
        .iter()
        .map(|c| ((c.faction_id.clone(), c.claim_type), c.strength))
        .collect();

    let mut added: Vec<FactionClaim> = Vec::new();
    let mut removed: Vec<FactionClaim> = Vec::new();
    let mut strength_changes: Vec<ClaimStrengthDelta> = Vec::new();

    for (k, s_after) in &after_map {
        match before_map.get(k) {
            None => added.push(FactionClaim {
                faction_id: k.0.clone(),
                claim_type: k.1,
                strength: *s_after,
            }),
            Some(s_before) if s_before != s_after => {
                strength_changes.push(ClaimStrengthDelta {
                    faction_id: k.0.clone(),
                    claim_type: k.1,
                    strength_before: *s_before,
                    strength_after: *s_after,
                });
            }
            Some(_) => {}
        }
    }
    for (k, s_before) in &before_map {
        if !after_map.contains_key(k) {
            removed.push(FactionClaim {
                faction_id: k.0.clone(),
                claim_type: k.1,
                strength: *s_before,
            });
        }
    }
    (added, removed, strength_changes)
}

fn diff_presences(
    before: &[WorldFactionPresence],
    after: &[WorldFactionPresence],
) -> (Vec<FactionId>, Vec<FactionId>) {
    let before_set: BTreeSet<&str> = before.iter().map(|p| p.faction_id.as_str()).collect();
    let after_set: BTreeSet<&str> = after.iter().map(|p| p.faction_id.as_str()).collect();
    let added: Vec<FactionId> = after_set
        .difference(&before_set)
        .map(|s| FactionId::new(*s))
        .collect();
    let removed: Vec<FactionId> = before_set
        .difference(&after_set)
        .map(|s| FactionId::new(*s))
        .collect();
    (added, removed)
}

// ── Route diff ─────────────────────────────────────────────────────────────────

fn diff_routes(before: &GeneratedSector, after: &GeneratedSector, out: &mut SectorDiff) {
    let before_idx: BTreeMap<&str, &crate::sector_model::GeneratedRoute> =
        before.routes.iter().map(|r| (r.id.as_str(), r)).collect();
    let after_idx: BTreeMap<&str, &crate::sector_model::GeneratedRoute> =
        after.routes.iter().map(|r| (r.id.as_str(), r)).collect();
    let all_ids: BTreeSet<&str> = before_idx
        .keys()
        .copied()
        .chain(after_idx.keys().copied())
        .collect();
    for id in all_ids {
        match (before_idx.get(id), after_idx.get(id)) {
            (None, Some(r)) => out.routes_added.push(RouteRef {
                id: r.id.clone(),
                from_system_id: r.from_system_id.clone(),
                to_system_id: r.to_system_id.clone(),
            }),
            (Some(r), None) => out.routes_removed.push(RouteRef {
                id: r.id.clone(),
                from_system_id: r.from_system_id.clone(),
                to_system_id: r.to_system_id.clone(),
            }),
            (Some(a), Some(b)) => {
                if a.stability != b.stability || a.route_type != b.route_type {
                    out.routes_changed.push(RouteDiff {
                        id: a.id.clone(),
                        from_system_id: a.from_system_id.clone(),
                        to_system_id: a.to_system_id.clone(),
                        stability_before: a.stability,
                        stability_after: b.stability,
                        route_type_before: a.route_type,
                        route_type_after: b.route_type,
                    });
                }
            }
            (None, None) => unreachable!(),
        }
    }
}

// ── Faction projection delta ───────────────────────────────────────────────────

fn compute_faction_deltas(
    before: &GeneratedSector,
    after: &GeneratedSector,
    cfg: &DiffConfig,
) -> Vec<FactionDelta> {
    let before_idx: BTreeMap<&str, &crate::sector_model::GeneratedFaction> =
        before.factions.iter().map(|f| (f.id.as_str(), f)).collect();
    let after_idx: BTreeMap<&str, &crate::sector_model::GeneratedFaction> =
        after.factions.iter().map(|f| (f.id.as_str(), f)).collect();
    let all: BTreeSet<&str> = before_idx
        .keys()
        .copied()
        .chain(after_idx.keys().copied())
        .collect();
    let mut deltas: Vec<FactionDelta> = Vec::new();
    for id in all {
        let p_before = before_idx
            .get(id)
            .map(|f| f.power.total_projection())
            .unwrap_or(0.0);
        let p_after = after_idx
            .get(id)
            .map(|f| f.power.total_projection())
            .unwrap_or(0.0);
        let delta = p_after - p_before;
        if delta.abs() < cfg.min_faction_delta {
            continue;
        }
        let name = after_idx
            .get(id)
            .or_else(|| before_idx.get(id))
            .map(|f| f.name.to_string())
            .unwrap_or_else(|| id.to_string());
        let w_before = before_idx
            .get(id)
            .map(|f| f.world_presence.len())
            .unwrap_or(0) as i32;
        let w_after = after_idx
            .get(id)
            .map(|f| f.world_presence.len())
            .unwrap_or(0) as i32;
        let s_before = before_idx
            .get(id)
            .map(|f| f.system_presence.len())
            .unwrap_or(0) as i32;
        let s_after = after_idx
            .get(id)
            .map(|f| f.system_presence.len())
            .unwrap_or(0) as i32;
        deltas.push(FactionDelta {
            faction_id: FactionId::new(id),
            name,
            total_projection_before: p_before,
            total_projection_after: p_after,
            delta,
            world_presence_delta: w_after - w_before,
            system_presence_delta: s_after - s_before,
        });
    }
    deltas.sort_by(|a, b| {
        b.delta
            .abs()
            .partial_cmp(&a.delta.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.faction_id.cmp(&b.faction_id))
    });
    deltas
}

// ── Tick-driven helper ─────────────────────────────────────────────────────────

/// Generate `before` from a project, clone it, advance `ticks` ticks, then
/// diff. Convenience for the "what changed after N sim ticks" workflow.
///
/// # Errors
///
/// Propagates any [`SectorError`] from `generate_sector`.
pub fn diff_after_ticks(
    project: crate::input::ProjectInput,
    ticks: u32,
    cfg: &DiffConfig,
) -> Result<(SectorDiff, GeneratedSector, GeneratedSector), SectorError> {
    let before = crate::generation::generate(project)?;
    let mut after = before.clone();
    for _ in 0..ticks {
        crate::conflict::advance_sector(&mut after);
    }
    let d = diff_sectors_with(&before, &after, cfg);
    Ok((d, before, after))
}

// ── Markdown render ────────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(d: &SectorDiff) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Sector Diff");
    let _ = writeln!(
        s,
        "\n- Before: sector `{}` seed `{}` v{}",
        d.before_id, d.before_seed, d.before_version
    );
    let _ = writeln!(
        s,
        "- After:  sector `{}` seed `{}` v{}",
        d.after_id, d.after_seed, d.after_version
    );
    let _ = writeln!(
        s,
        "- Catalog compatible: **{}**",
        if d.catalog_compatible { "yes" } else { "no" }
    );
    if !d.schema_warnings.is_empty() {
        let _ = writeln!(s, "\n## Schema Warnings");
        for w in &d.schema_warnings {
            let _ = writeln!(s, "- {w}");
        }
    }
    let _ = writeln!(
        s,
        "\n- Systems: {} → {}  ·  Routes: {} → {}",
        d.system_count_before, d.system_count_after, d.route_count_before, d.route_count_after
    );

    // Systems.
    let _ = writeln!(s, "\n## Systems");
    if d.systems_added.is_empty() && d.systems_removed.is_empty() && d.systems_changed.is_empty() {
        let _ = writeln!(s, "\nNo system-level changes.");
    } else {
        if !d.systems_added.is_empty() {
            let _ = writeln!(s, "\n### Added");
            for r in &d.systems_added {
                let _ = writeln!(s, "- `{}` {}", r.id, r.name);
            }
        }
        if !d.systems_removed.is_empty() {
            let _ = writeln!(s, "\n### Removed");
            for r in &d.systems_removed {
                let _ = writeln!(s, "- `{}` {}", r.id, r.name);
            }
        }
        if !d.systems_changed.is_empty() {
            let _ = writeln!(s, "\n### Changed");
            for sd in &d.systems_changed {
                let _ = writeln!(s, "\n- **`{}` {}**", sd.id, sd.name_after);
                if sd.renamed {
                    let _ = writeln!(s, "  - Renamed: `{}` → `{}`", sd.name_before, sd.name_after);
                }
                if sd.state_before != sd.state_after {
                    let _ = writeln!(s, "  - State: {:?} → {:?}", sd.state_before, sd.state_after);
                }
                if sd.dominant_before != sd.dominant_after {
                    let _ = writeln!(
                        s,
                        "  - Dominant: {} → {}",
                        opt(&sd.dominant_before),
                        opt(&sd.dominant_after)
                    );
                }
                if sd.sovereign_before != sd.sovereign_after {
                    let _ = writeln!(
                        s,
                        "  - Sovereign: {} → {}",
                        opt(&sd.sovereign_before),
                        opt(&sd.sovereign_after)
                    );
                }
                if sd.occupier_before != sd.occupier_after {
                    let _ = writeln!(
                        s,
                        "  - Orbital controller: {} → {}",
                        opt(&sd.occupier_before),
                        opt(&sd.occupier_after)
                    );
                }
                if !sd.primary_factions_added.is_empty() {
                    let _ = writeln!(
                        s,
                        "  - Primary factions added: {}",
                        join_ids(&sd.primary_factions_added)
                    );
                }
                if !sd.primary_factions_removed.is_empty() {
                    let _ = writeln!(
                        s,
                        "  - Primary factions removed: {}",
                        join_ids(&sd.primary_factions_removed)
                    );
                }
                if !sd.worlds_added.is_empty() {
                    let _ = writeln!(
                        s,
                        "  - Worlds added: {}",
                        sd.worlds_added
                            .iter()
                            .map(|w| format!("`{}` {}", w.id, w.name))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                if !sd.worlds_removed.is_empty() {
                    let _ = writeln!(
                        s,
                        "  - Worlds removed: {}",
                        sd.worlds_removed
                            .iter()
                            .map(|w| format!("`{}` {}", w.id, w.name))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                for wd in &sd.worlds_changed {
                    render_world_change(&mut s, wd);
                }
            }
        }
    }

    // Routes.
    let _ = writeln!(s, "\n## Routes");
    if d.routes_added.is_empty() && d.routes_removed.is_empty() && d.routes_changed.is_empty() {
        let _ = writeln!(s, "\nNo route-level changes.");
    } else {
        if !d.routes_added.is_empty() {
            let _ = writeln!(s, "\n### Added");
            for r in &d.routes_added {
                let _ = writeln!(
                    s,
                    "- `{}` ({} → {})",
                    r.id, r.from_system_id, r.to_system_id
                );
            }
        }
        if !d.routes_removed.is_empty() {
            let _ = writeln!(s, "\n### Removed");
            for r in &d.routes_removed {
                let _ = writeln!(
                    s,
                    "- `{}` ({} → {})",
                    r.id, r.from_system_id, r.to_system_id
                );
            }
        }
        if !d.routes_changed.is_empty() {
            let _ = writeln!(s, "\n### Changed");
            for r in &d.routes_changed {
                let _ = writeln!(
                    s,
                    "- `{}` ({} → {}): {:?} → {:?}, {:?} → {:?}",
                    r.id,
                    r.from_system_id,
                    r.to_system_id,
                    r.stability_before,
                    r.stability_after,
                    r.route_type_before,
                    r.route_type_after
                );
            }
        }
    }

    // Factions.
    let _ = writeln!(s, "\n## Faction Power Deltas");
    if d.faction_deltas.is_empty() {
        let _ = writeln!(
            s,
            "\nNo faction power changes above the configured threshold."
        );
    } else {
        let _ = writeln!(
            s,
            "\n| Faction | Δ Projection | Before | After | Worlds Δ | Systems Δ |"
        );
        let _ = writeln!(s, "|---|---:|---:|---:|---:|---:|");
        for f in &d.faction_deltas {
            let _ = writeln!(
                s,
                "| {} (`{}`) | {:+.2} | {:.2} | {:.2} | {:+} | {:+} |",
                f.name,
                f.faction_id,
                f.delta,
                f.total_projection_before,
                f.total_projection_after,
                f.world_presence_delta,
                f.system_presence_delta
            );
        }
    }

    if !d.stance_changes.is_empty() {
        let _ = writeln!(s, "\n## Diplomacy changes (§4)");
        let _ = writeln!(s, "\n| A | B | Before | After |\n|---|---|---|---|");
        for c in &d.stance_changes {
            let _ = writeln!(s, "| {} | {} | {:?} | {:?} |", c.a, c.b, c.before, c.after);
        }
    }
    if !(d.regions_added.is_empty() && d.regions_removed.is_empty() && d.regions_changed.is_empty())
    {
        let _ = writeln!(s, "\n## Warp regions (§5)");
        for r in &d.regions_added {
            let _ = writeln!(s, "- ADDED `{}` {:?} ({} hexes)", r.id, r.kind, r.hex_count);
        }
        for r in &d.regions_removed {
            let _ = writeln!(
                s,
                "- REMOVED `{}` {:?} ({} hexes)",
                r.id, r.kind, r.hex_count
            );
        }
        for r in &d.regions_changed {
            let _ = writeln!(
                s,
                "- CHANGED `{}` {:?}→{:?} hexes {}→{}",
                r.id, r.kind_before, r.kind_after, r.hex_count_before, r.hex_count_after
            );
        }
    }
    if !(d.economy_balance_changes.is_empty()
        && d.stranded_added.is_empty()
        && d.stranded_removed.is_empty())
    {
        let _ = writeln!(s, "\n## Economy (§12)");
        if !d.economy_balance_changes.is_empty() {
            let _ = writeln!(
                s,
                "\n| Resource | Before | After | Δ |\n|---|---:|---:|---:|"
            );
            for c in &d.economy_balance_changes {
                let _ = writeln!(
                    s,
                    "| {} | {:.1} | {:.1} | {:+.1} |",
                    c.resource, c.before, c.after, c.delta
                );
            }
        }
        if !d.stranded_added.is_empty() {
            let _ = writeln!(
                s,
                "\nNewly stranded worlds: {}",
                join_ids(&d.stranded_added)
            );
        }
        if !d.stranded_removed.is_empty() {
            let _ = writeln!(s, "\nNo longer stranded: {}", join_ids(&d.stranded_removed));
        }
    }
    s
}

fn render_world_change(s: &mut String, wd: &WorldDiff) {
    let _ = writeln!(s, "  - World `{}` {}", wd.id, wd.name_after);
    if wd.renamed {
        let _ = writeln!(
            s,
            "    - Renamed: `{}` → `{}`",
            wd.name_before, wd.name_after
        );
    }
    if wd.dominant_before != wd.dominant_after {
        let _ = writeln!(
            s,
            "    - Dominant: {} → {}",
            opt(&wd.dominant_before),
            opt(&wd.dominant_after)
        );
    }
    if wd.sovereign_before != wd.sovereign_after {
        let _ = writeln!(
            s,
            "    - Sovereign: {} → {}",
            opt(&wd.sovereign_before),
            opt(&wd.sovereign_after)
        );
    }
    if wd.occupier_before != wd.occupier_after {
        let _ = writeln!(
            s,
            "    - Occupier: {} → {}",
            opt(&wd.occupier_before),
            opt(&wd.occupier_after)
        );
    }
    if wd.hidden_master_before != wd.hidden_master_after {
        let _ = writeln!(
            s,
            "    - Hidden master: {} → {}",
            opt(&wd.hidden_master_before),
            opt(&wd.hidden_master_after)
        );
    }
    if wd.contested_before != wd.contested_after {
        let _ = writeln!(
            s,
            "    - Contested: {} → {}",
            wd.contested_before, wd.contested_after
        );
    }
    if !wd.claims_added.is_empty() {
        let _ = writeln!(
            s,
            "    - Claims added: {}",
            wd.claims_added
                .iter()
                .map(|c| format!(
                    "{:?} by `{}` (s={})",
                    c.claim_type, c.faction_id, c.strength
                ))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    if !wd.claims_removed.is_empty() {
        let _ = writeln!(
            s,
            "    - Claims removed: {}",
            wd.claims_removed
                .iter()
                .map(|c| format!(
                    "{:?} by `{}` (s={})",
                    c.claim_type, c.faction_id, c.strength
                ))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    if !wd.claim_strength_changes.is_empty() {
        let _ = writeln!(
            s,
            "    - Claim strength: {}",
            wd.claim_strength_changes
                .iter()
                .map(|d| format!(
                    "{:?} by `{}` {} → {}",
                    d.claim_type, d.faction_id, d.strength_before, d.strength_after
                ))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    if !wd.presences_added.is_empty() {
        let _ = writeln!(
            s,
            "    - Presences added: {}",
            join_ids(&wd.presences_added)
        );
    }
    if !wd.presences_removed.is_empty() {
        let _ = writeln!(
            s,
            "    - Presences removed: {}",
            join_ids(&wd.presences_removed)
        );
    }
}

fn opt<T: AsRef<str>>(v: &Option<T>) -> String {
    v.as_ref()
        .map(|s| s.as_ref().to_string())
        .unwrap_or_else(|| "—".to_string())
}

fn join_ids<T: AsRef<str>>(ids: &[T]) -> String {
    ids.iter()
        .map(|id| id.as_ref())
        .collect::<Vec<_>>()
        .join(", ")
}

// ── Bundle writer ──────────────────────────────────────────────────────────────

/// Write `diff.md` + `diff.json` into `output_dir`.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if either file cannot be written, and
/// [`SectorError::ExportFailed`] if the diff cannot be serialised.
pub fn write_diff(output_dir: &Utf8Path, diff: &SectorDiff) -> Result<(), SectorError> {
    crate::export::write_md_and_json(output_dir, "diff", &render_markdown(diff), diff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        GeneratedStar, GeneratedSystem, GenerationManifest, HexCoord, SystemControlSummary,
    };

    fn empty_sector(id: &str) -> GeneratedSector {
        GeneratedSector {
            id: id.into(),
            title: "t".into(),
            seed: "abc".into(),
            generator_name: "t".into(),
            generator_version: "0.1".into(),
            width: 4,
            height: 4,
            systems: vec![GeneratedSystem {
                id: "sys-0001".into(),
                index: 1,
                name: "Alpha".into(),
                coord: HexCoord { q: 0, r: 0 },
                star: GeneratedStar {
                    colour_code: "A".into(),
                    colour_name: "A".into(),
                    spectral_type: None,
                    source_row_index: None,
                },
                worlds: vec![],
                primary_factions: vec![],
                tags: vec![],
                notes: vec![],
                control: SystemControlSummary::default(),
                stability: Default::default(),
                orbital_assets: vec![],
                blockade: Default::default(),
                conflict: Default::default(),
                intel: Default::default(),
                archetype: Default::default(),
            }],
            routes: vec![],
            factions: vec![],
            manifest: GenerationManifest {
                project_id: id.into(),
                generated_at_policy: "t".into(),
                generator_name: "t".into(),
                generator_version: "0.1".into(),
                seed: "abc".into(),
                seed_hash: "t".into(),
                profile: None,
                input_digests: BTreeMap::new(),
                settings_digest: "t".into(),
                system_count: 1,
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new().into(),
            economy: Default::default(),
            chronicle: Default::default(),
        }
    }

    #[test]
    fn identical_sectors_produce_no_changes() {
        let a = empty_sector("t1");
        let b = a.clone();
        let d = diff_sectors(&a, &b);
        assert!(d.systems_added.is_empty());
        assert!(d.systems_removed.is_empty());
        assert!(d.systems_changed.is_empty());
        assert!(d.routes_added.is_empty());
        assert!(d.faction_deltas.is_empty());
        assert!(d.catalog_compatible);
    }

    #[test]
    fn rename_is_modification_not_add_remove() {
        let a = empty_sector("t1");
        let mut b = a.clone();
        b.systems[0].name = "Beta".into();
        let d = diff_sectors(&a, &b);
        assert!(d.systems_added.is_empty());
        assert!(d.systems_removed.is_empty());
        assert_eq!(d.systems_changed.len(), 1);
        assert!(d.systems_changed[0].renamed);
        assert_eq!(d.systems_changed[0].name_before, "Alpha");
        assert_eq!(d.systems_changed[0].name_after, "Beta");
    }

    #[test]
    fn sector_id_mismatch_flags_incompatible() {
        let a = empty_sector("t1");
        let b = empty_sector("t2");
        let d = diff_sectors(&a, &b);
        assert!(!d.catalog_compatible);
        assert!(!d.schema_warnings.is_empty());
    }

    #[test]
    fn diff_is_deterministic() {
        let a = empty_sector("t1");
        let mut b = a.clone();
        b.systems[0].name = "Beta".into();
        let d1 = diff_sectors(&a, &b);
        let d2 = diff_sectors(&a, &b);
        let j1 = serde_json::to_string(&d1).unwrap();
        let j2 = serde_json::to_string(&d2).unwrap();
        assert_eq!(j1, j2);
    }
}
