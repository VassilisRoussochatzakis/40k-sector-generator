//! Post-generation sector invariants (spec §11.11).
//!
//! Operates on a fully generated `GeneratedSector` and returns a list of
//! violations. Empty list ⇒ sector passes invariants. Pure; no I/O.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::sector_model::{hex_distance, GeneratedSector};

#[derive(Debug, Clone, Serialize)]
pub struct InvariantViolation {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvariantReport {
    pub ok: bool,
    pub violations: Vec<InvariantViolation>,
}

/// §V5 — authoritative catalogue of every invariant code [`check_sector`] may
/// emit, each paired with a one-line human description, grouped in the same
/// stratum order the builder buckets violations by (systems → worlds → routes
/// → factions → regions → economy → manifest).
///
/// This is the single source of truth for the `sectorforge-builder` read-only
/// "what is checked" panel (§V5): the panel renders this list directly so it
/// can never drift from the codes the checker actually raises. Keep it in
/// lockstep with the `violation("CODE", …)` call sites in this module; the
/// `invariant_catalogue_is_consistent` test guards uniqueness, casing, and the
/// count.
///
/// Note: `GEN_SECTOR_NOT_SQUARE` is a *pre-generation* validation rule (see
/// [`crate::validation`]), not a post-generation invariant, so it is not listed
/// here.
pub const INVARIANT_CODES: &[(&str, &str)] = &[
    // systems
    ("DUPLICATE_SYSTEM_ID", "Two systems share the same id."),
    ("SYSTEM_INDEX_ZERO", "A system's 1-based index is zero."),
    (
        "COORD_OUT_OF_BOUNDS",
        "A system coordinate lies outside the sector grid.",
    ),
    (
        "DUPLICATE_COORDINATE",
        "Two systems occupy the same hex coordinate.",
    ),
    (
        "SYSTEM_CONTROL_UNKNOWN_FACTION",
        "A system control entry names a faction absent from the roster.",
    ),
    (
        "SYSTEM_CONTROL_TOP_UNKNOWN",
        "A system's top-control faction is absent from the roster.",
    ),
    (
        "PRIMARY_FACTION_MISSING_SUMMARY",
        "A system primary faction has no matching sector summary.",
    ),
    // worlds
    (
        "WORLD_ID_PREFIX",
        "A world id does not start with its parent system id.",
    ),
    (
        "DUPLICATE_WORLD_ID_IN_SYSTEM",
        "Two worlds within one system share the same id.",
    ),
    (
        "DUPLICATE_WORLD_ID_GLOBAL",
        "A world id is reused across systems.",
    ),
    (
        "WORLD_INDEX_OR_ORBIT_ZERO",
        "A world's index or orbit is zero.",
    ),
    (
        "WORLD_TAG_NAMESPACE_MISSING",
        "A world tag lacks its required `namespace:` prefix.",
    ),
    ("WORLD_TAG_DUPLICATE", "A world carries the same tag twice."),
    (
        "PRESENCE_DIMENSION_OUT_OF_RANGE",
        "A faction-presence dimension is outside 0..=100.",
    ),
    (
        "WORLD_FACTION_MISSING_SUMMARY",
        "A world faction presence has no matching sector summary.",
    ),
    (
        "WORLD_CLAIM_UNKNOWN_FACTION",
        "A world claim names a faction absent from the roster.",
    ),
    (
        "WORLD_CLAIM_STRENGTH_OUT_OF_RANGE",
        "A world claim strength is outside 0..=100.",
    ),
    (
        "WORLD_CONTROL_UNKNOWN_FACTION",
        "A world control entry names a faction absent from the roster.",
    ),
    // routes
    (
        "ROUTE_SELF_REFERENCE",
        "A route connects a system to itself.",
    ),
    (
        "ROUTE_UNKNOWN_FROM",
        "A route's source system id does not exist.",
    ),
    (
        "ROUTE_UNKNOWN_TO",
        "A route's destination system id does not exist.",
    ),
    (
        "ROUTE_DUPLICATE_UNDIRECTED",
        "Two routes describe the same undirected edge.",
    ),
    (
        "ROUTE_DISTANCE_MISMATCH",
        "A route distance disagrees with its endpoints' hex distance.",
    ),
    // factions
    (
        "FACTION_SYSTEM_PRESENCE_UNKNOWN",
        "A faction's system-presence list names a missing system.",
    ),
    (
        "FACTION_WORLD_PRESENCE_UNKNOWN",
        "A faction's world-presence list names a missing world.",
    ),
    // regions
    (
        "REGION_HEX_OUT_OF_BOUNDS",
        "A region hex lies outside the sector grid.",
    ),
    ("REGION_HEX_OVERLAP", "Two regions claim the same hex."),
    (
        "REGION_ISOLATES_SECTOR",
        "Region effects split the navigable route graph.",
    ),
    // economy
    (
        "ECONOMY_ENABLED_NO_WORLDS",
        "The economy layer is enabled but the sector has no worlds.",
    ),
    // manifest
    (
        "MANIFEST_SYSTEM_COUNT_MISMATCH",
        "The manifest system count disagrees with the sector.",
    ),
    (
        "MANIFEST_WORLD_COUNT_MISMATCH",
        "The manifest world count disagrees with the sector.",
    ),
    (
        "MANIFEST_ROUTE_COUNT_MISMATCH",
        "The manifest route count disagrees with the sector.",
    ),
];

pub fn check_sector(sector: &GeneratedSector) -> InvariantReport {
    let mut v: Vec<InvariantViolation> = Vec::new();

    check_counts(sector, &mut v);
    let (sys_ids, all_world_ids) = check_systems(sector, &mut v);
    check_routes(sector, &sys_ids, &mut v);
    check_factions(sector, &sys_ids, &all_world_ids, &mut v);
    check_regions(sector, &mut v);
    check_region_connectivity(sector, &mut v);
    check_economy(sector, &mut v);

    InvariantReport {
        ok: v.is_empty(),
        violations: v,
    }
}

/// §5 NEW.md invariant: region hex coordinates must fall inside the grid and
/// must not overlap each other.
fn check_regions(s: &GeneratedSector, v: &mut Vec<InvariantViolation>) {
    let mut seen: BTreeMap<(i32, i32), String> = BTreeMap::new();
    for reg in s.regions.iter() {
        for h in &reg.hexes {
            if h.q < 0 || h.r < 0 || h.q >= s.width as i32 || h.r >= s.height as i32 {
                v.push(violation(
                    "REGION_HEX_OUT_OF_BOUNDS",
                    &format!(
                        "region '{}' hex (q={}, r={}) outside sector {}x{}",
                        reg.id, h.q, h.r, s.width, s.height
                    ),
                    Some(&format!("regions.{}.hexes", reg.id)),
                ));
            }
            if let Some(other) = seen.insert((h.q, h.r), reg.id.clone()) {
                v.push(violation(
                    "REGION_HEX_OVERLAP",
                    &format!(
                        "regions '{}' and '{}' both cover hex (q={}, r={})",
                        other, reg.id, h.q, h.r
                    ),
                    Some(&format!("regions.{}.hexes", reg.id)),
                ));
            }
        }
    }
}

/// §5 NEW.md: if region effects (storm/turbulence) increase route-graph
/// disconnectedness when restricted to navigable lanes (non-Perilous), surface
/// a violation so the user can adjust region density or relax effects.
fn check_region_connectivity(s: &GeneratedSector, v: &mut Vec<InvariantViolation>) {
    if s.regions.is_empty() || s.systems.len() < 2 {
        return;
    }
    let region_perilous = s.routes.iter().any(|r| {
        r.tags
            .iter()
            .any(|t| t.as_ref() == "region:perilous_applied")
    });
    if !region_perilous {
        return;
    }
    let actual = navigable_component_count(s, false);
    let before_region_perilous = navigable_component_count(s, true);
    if actual > before_region_perilous {
        v.push(violation(
            "REGION_ISOLATES_SECTOR",
            &format!(
                "region effects split the navigable route graph ({} -> {} components)",
                before_region_perilous, actual
            ),
            Some("regions"),
        ));
    }
}

fn navigable_component_count(s: &GeneratedSector, restore_region_perilous: bool) -> usize {
    use crate::sector_model::RouteStability;
    let idx: BTreeMap<&str, usize> = s
        .systems
        .iter()
        .enumerate()
        .map(|(i, sys)| (sys.id.as_str(), i))
        .collect();
    let mut parent: Vec<usize> = (0..s.systems.len()).collect();
    for r in &s.routes {
        let restored = restore_region_perilous
            && r.tags
                .iter()
                .any(|t| t.as_ref() == "region:perilous_applied");
        if matches!(r.stability, RouteStability::Perilous) && !restored {
            continue;
        }
        let (Some(&a), Some(&b)) = (
            idx.get(r.from_system_id.as_str()),
            idx.get(r.to_system_id.as_str()),
        ) else {
            continue;
        };
        let ra = find_root(&mut parent, a);
        let rb = find_root(&mut parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    }
    let mut roots: BTreeSet<usize> = BTreeSet::new();
    for i in 0..parent.len() {
        roots.insert(find_root(&mut parent, i));
    }
    roots.len()
}

fn find_root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// §12 NEW.md invariant: when the economy derivation ran (`enabled == true`)
/// there must be at least one world covered — an empty world list means
/// derivation was misconfigured.
fn check_economy(s: &GeneratedSector, v: &mut Vec<InvariantViolation>) {
    if s.economy.enabled
        && s.economy.worlds.is_empty()
        && s.systems.iter().any(|sys| !sys.worlds.is_empty())
    {
        v.push(violation(
            "ECONOMY_ENABLED_NO_WORLDS",
            "economy.enabled is true but no per-world entries were derived",
            Some("economy.worlds"),
        ));
    }
}

fn check_counts(s: &GeneratedSector, v: &mut Vec<InvariantViolation>) {
    if s.manifest.system_count != s.systems.len() {
        v.push(violation(
            "MANIFEST_SYSTEM_COUNT_MISMATCH",
            &format!(
                "manifest.system_count={} but systems.len={}",
                s.manifest.system_count,
                s.systems.len()
            ),
            Some("manifest.system_count"),
        ));
    }
    let total_worlds: usize = s.systems.iter().map(|x| x.worlds.len()).sum();
    if s.manifest.world_count != total_worlds {
        v.push(violation(
            "MANIFEST_WORLD_COUNT_MISMATCH",
            &format!(
                "manifest.world_count={} but actual={}",
                s.manifest.world_count, total_worlds
            ),
            Some("manifest.world_count"),
        ));
    }
    if s.manifest.route_count != s.routes.len() {
        v.push(violation(
            "MANIFEST_ROUTE_COUNT_MISMATCH",
            &format!(
                "manifest.route_count={} but routes.len={}",
                s.manifest.route_count,
                s.routes.len()
            ),
            Some("manifest.route_count"),
        ));
    }
}

fn check_systems(
    s: &GeneratedSector,
    v: &mut Vec<InvariantViolation>,
) -> (
    BTreeSet<crate::ids::SystemId>,
    BTreeSet<crate::ids::WorldId>,
) {
    let mut sys_ids: BTreeSet<crate::ids::SystemId> = BTreeSet::new();
    let mut all_world_ids: BTreeSet<crate::ids::WorldId> = BTreeSet::new();
    let mut coords: BTreeMap<(i32, i32), crate::ids::SystemId> = BTreeMap::new();

    for sys in s.systems.iter() {
        if !sys_ids.insert(sys.id.clone()) {
            v.push(violation(
                "DUPLICATE_SYSTEM_ID",
                &format!("duplicate system id '{}'", sys.id),
                Some(&format!("systems[{}]", sys.id)),
            ));
        }
        if sys.index == 0 {
            v.push(violation(
                "SYSTEM_INDEX_ZERO",
                &format!("system '{}' has index 0; must be 1-based", sys.id),
                Some(&format!("systems.{}.index", sys.id)),
            ));
        }
        let (q, r) = (sys.coord.q, sys.coord.r);
        if q < 0 || q >= s.width as i32 || r < 0 || r >= s.height as i32 {
            v.push(violation(
                "COORD_OUT_OF_BOUNDS",
                &format!(
                    "system '{}' coord (q={}, r={}) outside sector {}x{}",
                    sys.id, q, r, s.width, s.height
                ),
                Some(&format!("systems.{}.coord", sys.id)),
            ));
        }
        if let Some(prev) = coords.insert((q, r), sys.id.clone()) {
            v.push(violation(
                "DUPLICATE_COORDINATE",
                &format!(
                    "systems '{}' and '{}' both occupy (q={}, r={})",
                    prev, sys.id, q, r
                ),
                Some(&format!("systems.{}.coord", sys.id)),
            ));
        }

        // Worlds
        let mut local_world_ids: BTreeSet<crate::ids::WorldId> = BTreeSet::new();
        for w in sys.worlds.iter() {
            if !w.id.as_str().starts_with(sys.id.as_str()) {
                v.push(violation(
                    "WORLD_ID_PREFIX",
                    &format!(
                        "world '{}' id does not begin with parent system id '{}'",
                        w.id, sys.id
                    ),
                    Some(&format!("systems.{}.worlds.{}.id", sys.id, w.id)),
                ));
            }
            if !local_world_ids.insert(w.id.clone()) {
                v.push(violation(
                    "DUPLICATE_WORLD_ID_IN_SYSTEM",
                    &format!("duplicate world id '{}' in system '{}'", w.id, sys.id),
                    Some(&format!("systems.{}.worlds", sys.id)),
                ));
            }
            if !all_world_ids.insert(w.id.clone()) {
                v.push(violation(
                    "DUPLICATE_WORLD_ID_GLOBAL",
                    &format!("world id '{}' is not unique across the sector", w.id),
                    Some(&format!("systems.{}.worlds.{}.id", sys.id, w.id)),
                ));
            }
            if w.index == 0 || w.orbit == 0 {
                v.push(violation(
                    "WORLD_INDEX_OR_ORBIT_ZERO",
                    &format!("world '{}' index/orbit must be 1-based", w.id),
                    Some(&format!("systems.{}.worlds.{}", sys.id, w.id)),
                ));
            }

            // Spec §10.10: every scalar world-profile field must produce
            // exactly one tag in the relevant namespace.
            for prefix in [
                "atmosphere:",
                "biosphere:",
                "gov:",
                "population:",
                "star:",
                "tech:",
                "temperature:",
                "world_type:",
            ] {
                if !w.tags.iter().any(|t| t.starts_with(prefix)) {
                    v.push(violation(
                        "WORLD_TAG_NAMESPACE_MISSING",
                        &format!("world '{}' missing tag in namespace '{}'", w.id, prefix),
                        Some(&format!("systems.{}.worlds.{}.tags", sys.id, w.id)),
                    ));
                }
            }
            // Deduplicated tags
            let mut tag_set: BTreeSet<&str> = BTreeSet::new();
            for t in &w.tags {
                if !tag_set.insert(t.as_ref()) {
                    v.push(violation(
                        "WORLD_TAG_DUPLICATE",
                        &format!("world '{}' has duplicate tag '{}'", w.id, t),
                        Some(&format!("systems.{}.worlds.{}.tags", sys.id, w.id)),
                    ));
                }
            }
        }
    }
    (sys_ids, all_world_ids)
}

fn check_routes(
    s: &GeneratedSector,
    sys_ids: &BTreeSet<crate::ids::SystemId>,
    v: &mut Vec<InvariantViolation>,
) {
    let coord_by_id: BTreeMap<&str, crate::sector_model::HexCoord> =
        s.systems.iter().map(|x| (x.id.as_str(), x.coord)).collect();

    let mut undirected_keys: BTreeSet<(crate::ids::SystemId, crate::ids::SystemId)> =
        BTreeSet::new();

    for r in &s.routes {
        if r.from_system_id == r.to_system_id {
            v.push(violation(
                "ROUTE_SELF_REFERENCE",
                &format!("route '{}' references the same system twice", r.id),
                Some(&format!("routes.{}", r.id)),
            ));
        }
        if !sys_ids.contains(&r.from_system_id) {
            v.push(violation(
                "ROUTE_UNKNOWN_FROM",
                &format!(
                    "route '{}' from_system_id '{}' is not a generated system",
                    r.id, r.from_system_id
                ),
                Some(&format!("routes.{}.from_system_id", r.id)),
            ));
        }
        if !sys_ids.contains(&r.to_system_id) {
            v.push(violation(
                "ROUTE_UNKNOWN_TO",
                &format!(
                    "route '{}' to_system_id '{}' is not a generated system",
                    r.id, r.to_system_id
                ),
                Some(&format!("routes.{}.to_system_id", r.id)),
            ));
        }
        let (a, b) = if r.from_system_id <= r.to_system_id {
            (r.from_system_id.clone(), r.to_system_id.clone())
        } else {
            (r.to_system_id.clone(), r.from_system_id.clone())
        };
        if !undirected_keys.insert((a, b)) {
            v.push(violation(
                "ROUTE_DUPLICATE_UNDIRECTED",
                &format!("route '{}' duplicates an existing undirected edge", r.id),
                Some(&format!("routes.{}", r.id)),
            ));
        }
        if let (Some(fc), Some(tc)) = (
            coord_by_id.get(r.from_system_id.as_str()),
            coord_by_id.get(r.to_system_id.as_str()),
        ) {
            let actual = hex_distance(*fc, *tc);
            if actual != r.distance {
                v.push(violation(
                    "ROUTE_DISTANCE_MISMATCH",
                    &format!(
                        "route '{}' distance={} but hex_distance={}",
                        r.id, r.distance, actual
                    ),
                    Some(&format!("routes.{}.distance", r.id)),
                ));
            }
        }
    }
}

fn check_factions(
    s: &GeneratedSector,
    sys_ids: &BTreeSet<crate::ids::SystemId>,
    world_ids: &BTreeSet<crate::ids::WorldId>,
    v: &mut Vec<InvariantViolation>,
) {
    let summary_ids: BTreeSet<&str> = s.factions.iter().map(|f| f.id.as_str()).collect();

    for f in &s.factions {
        for sid in &f.system_presence {
            if !sys_ids.contains(sid) {
                v.push(violation(
                    "FACTION_SYSTEM_PRESENCE_UNKNOWN",
                    &format!(
                        "faction '{}' lists system '{}' which is not generated",
                        f.id, sid
                    ),
                    Some(&format!("factions.{}.system_presence", f.id)),
                ));
            }
        }
        for wid in &f.world_presence {
            if !world_ids.contains(wid) {
                v.push(violation(
                    "FACTION_WORLD_PRESENCE_UNKNOWN",
                    &format!(
                        "faction '{}' lists world '{}' which is not generated",
                        f.id, wid
                    ),
                    Some(&format!("factions.{}.world_presence", f.id)),
                ));
            }
        }
    }

    for sys in s.systems.iter() {
        for w in sys.worlds.iter() {
            for fp in &w.factions {
                if !summary_ids.contains(fp.faction_id.as_str()) {
                    v.push(violation(
                        "WORLD_FACTION_MISSING_SUMMARY",
                        &format!(
                            "world '{}' references faction '{}' that has no sector summary",
                            w.id, fp.faction_id
                        ),
                        Some(&format!("systems.{}.worlds.{}.factions", sys.id, w.id)),
                    ));
                }
                check_presence_dimensions(sys, w, fp, v);
            }
            for c in &w.claims {
                if !summary_ids.contains(c.faction_id.as_str()) {
                    v.push(violation(
                        "WORLD_CLAIM_UNKNOWN_FACTION",
                        &format!(
                            "world '{}' claim references faction '{}' that has no sector summary",
                            w.id, c.faction_id
                        ),
                        Some(&format!("systems.{}.worlds.{}.claims", sys.id, w.id)),
                    ));
                }
                if c.strength > 100 {
                    v.push(violation(
                        "WORLD_CLAIM_STRENGTH_OUT_OF_RANGE",
                        &format!("world '{}' claim strength={} > 100", w.id, c.strength),
                        Some(&format!("systems.{}.worlds.{}.claims", sys.id, w.id)),
                    ));
                }
            }
            check_world_control(sys, w, &summary_ids, v);
        }
        for pf in &sys.primary_factions {
            if !summary_ids.contains(pf.as_str()) {
                v.push(violation(
                    "PRIMARY_FACTION_MISSING_SUMMARY",
                    &format!(
                        "system '{}' primary faction '{}' has no sector summary",
                        sys.id, pf
                    ),
                    Some(&format!("systems.{}.primary_factions", sys.id)),
                ));
            }
        }
        check_system_control(sys, &summary_ids, v);
    }
}

fn check_presence_dimensions(
    sys: &crate::sector_model::GeneratedSystem,
    w: &crate::sector_model::GeneratedWorld,
    p: &crate::sector_model::WorldFactionPresence,
    v: &mut Vec<InvariantViolation>,
) {
    let d = &p.dimensions;
    let fields = [
        ("admin", d.admin),
        ("military", d.military),
        ("orbital", d.orbital),
        ("economic", d.economic),
        ("industrial", d.industrial),
        ("ideological", d.ideological),
        ("covert", d.covert),
        ("logistics", d.logistics),
        ("legitimacy", d.legitimacy),
        ("visibility", d.visibility),
    ];
    for (name, value) in fields {
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            v.push(violation(
                "PRESENCE_DIMENSION_OUT_OF_RANGE",
                &format!(
                    "world '{}' faction '{}' dimension '{}' = {} not in 0..=100",
                    w.id, p.faction_id, name, value
                ),
                Some(&format!(
                    "systems.{}.worlds.{}.factions.{}.dimensions.{}",
                    sys.id, w.id, p.faction_id, name
                )),
            ));
        }
    }
}

fn check_world_control(
    sys: &crate::sector_model::GeneratedSystem,
    w: &crate::sector_model::GeneratedWorld,
    summary_ids: &BTreeSet<&str>,
    v: &mut Vec<InvariantViolation>,
) {
    let ctl = &w.control;
    let check =
        |slot: &str, id: Option<&crate::ids::FactionId>, v: &mut Vec<InvariantViolation>| {
            if let Some(s) = id {
                if !summary_ids.contains(s.as_str()) {
                    v.push(violation(
                        "WORLD_CONTROL_UNKNOWN_FACTION",
                        &format!(
                            "world '{}' control.{} references unknown '{}'",
                            w.id, slot, s
                        ),
                        Some(&format!(
                            "systems.{}.worlds.{}.control.{}",
                            sys.id, w.id, slot
                        )),
                    ));
                }
            }
        };
    check("dominant", ctl.dominant.as_ref(), v);
    check("sovereign", ctl.sovereign.as_ref(), v);
    check("occupier", ctl.occupier.as_ref(), v);
    check("economic_hegemon", ctl.economic_hegemon.as_ref(), v);
    check("popular_authority", ctl.popular_authority.as_ref(), v);
    check("hidden_master", ctl.hidden_master.as_ref(), v);
}

fn check_system_control(
    sys: &crate::sector_model::GeneratedSystem,
    summary_ids: &BTreeSet<&str>,
    v: &mut Vec<InvariantViolation>,
) {
    let c = &sys.control;
    let check =
        |slot: &str, id: Option<&crate::ids::FactionId>, v: &mut Vec<InvariantViolation>| {
            if let Some(s) = id {
                if !summary_ids.contains(s.as_str()) {
                    v.push(violation(
                        "SYSTEM_CONTROL_UNKNOWN_FACTION",
                        &format!(
                            "system '{}' control.{} references unknown '{}'",
                            sys.id, slot, s
                        ),
                        Some(&format!("systems.{}.control.{}", sys.id, slot)),
                    ));
                }
            }
        };
    check("dominant", c.dominant.as_ref(), v);
    check("sovereign", c.sovereign.as_ref(), v);
    check("orbital_controller", c.orbital_controller.as_ref(), v);
    check("economic_hegemon", c.economic_hegemon.as_ref(), v);
    check("hidden_master", c.hidden_master.as_ref(), v);
    for sf in &c.top_factions {
        if !summary_ids.contains(sf.faction_id.as_str()) {
            v.push(violation(
                "SYSTEM_CONTROL_TOP_UNKNOWN",
                &format!(
                    "system '{}' control.top_factions references unknown '{}'",
                    sys.id, sf.faction_id
                ),
                Some(&format!("systems.{}.control.top_factions", sys.id)),
            ));
        }
    }
}

fn violation(code: &str, message: &str, path: Option<&str>) -> InvariantViolation {
    InvariantViolation {
        code: code.to_string(),
        message: message.to_string(),
        path: path.map(|s| s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::INVARIANT_CODES;
    use std::collections::BTreeSet;

    #[test]
    fn invariant_catalogue_is_consistent() {
        // Codes are unique, SCREAMING_SNAKE_CASE, and each has a description.
        let mut seen = BTreeSet::new();
        for (code, desc) in INVARIANT_CODES {
            assert!(seen.insert(*code), "duplicate catalogue code: {code}");
            assert!(!desc.is_empty(), "empty description for {code}");
            assert_eq!(
                *code,
                code.to_ascii_uppercase(),
                "catalogue codes must be SCREAMING_SNAKE_CASE: {code}"
            );
        }
        // The crate cannot enumerate live `violation(…)` call sites at compile
        // time, so guard the total: bump this deliberately whenever a code is
        // added/removed so the §V5 catalogue is never silently left stale.
        assert_eq!(
            INVARIANT_CODES.len(),
            32,
            "INVARIANT_CODES count changed — update the catalogue and this guard together"
        );
    }
}
