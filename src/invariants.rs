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

pub fn check_sector(sector: &GeneratedSector) -> InvariantReport {
    let mut v: Vec<InvariantViolation> = Vec::new();

    check_counts(sector, &mut v);
    let (sys_ids, all_world_ids) = check_systems(sector, &mut v);
    check_routes(sector, &sys_ids, &mut v);
    check_factions(sector, &sys_ids, &all_world_ids, &mut v);

    InvariantReport {
        ok: v.is_empty(),
        violations: v,
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
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut sys_ids: BTreeSet<String> = BTreeSet::new();
    let mut all_world_ids: BTreeSet<String> = BTreeSet::new();
    let mut coords: BTreeMap<(i32, i32), String> = BTreeMap::new();

    for sys in &s.systems {
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
        let mut local_world_ids: BTreeSet<String> = BTreeSet::new();
        for w in &sys.worlds {
            if !w.id.starts_with(&sys.id) {
                v.push(violation(
                    "WORLD_ID_PREFIX",
                    &format!("world '{}' id does not begin with parent system id '{}'", w.id, sys.id),
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
                if !tag_set.insert(t.as_str()) {
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
    sys_ids: &BTreeSet<String>,
    v: &mut Vec<InvariantViolation>,
) {
    let coord_by_id: BTreeMap<&str, crate::sector_model::HexCoord> = s
        .systems
        .iter()
        .map(|x| (x.id.as_str(), x.coord))
        .collect();

    let mut undirected_keys: BTreeSet<(String, String)> = BTreeSet::new();

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
    sys_ids: &BTreeSet<String>,
    world_ids: &BTreeSet<String>,
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

    for sys in &s.systems {
        for w in &sys.worlds {
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
