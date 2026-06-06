//! The core derivation: load `economy.toml`, walk the sector to build per-world
//! / per-system resource + strategic vectors, derive routes, dependency edges,
//! and stranded/supply/tithe status, then assemble the `EconomyReport`. Also the
//! `economy.toml` loader and the optional famine-stress stability nudge.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use camino::Utf8Path;

use crate::errors::SectorError;
use crate::sector_model::{GeneratedRoute, GeneratedSector, GeneratedWorld, RouteStability};

use super::config::{
    DependencyEdge, EconomyConfig, EconomyFile, EconomyReport, ResourceVector, RouteEconomy,
    StrategicOutput, StrategicPriority, SupplyRisk, SystemEconomy, TitheStatus, RESOURCE_KEYS,
    ROUTE_INTERDICTION_DIVISOR, ROUTE_INTERDICTION_MAX_MALUS, ROUTE_PATROL_DIVISOR,
    ROUTE_PATROL_MAX_BONUS, ROUTE_PIRACY_DIVISOR, ROUTE_PIRACY_MAX_MALUS, SELF_SUFFICIENCY_OUTPUT,
    STRATEGIC_RESOURCE_KEYS, WorldEconomy,
};
use super::risk::{
    import_risk, system_supply_risk, system_tithe_status, world_supply_risk, world_tithe_status,
};
use super::tables::{
    apply_feature_rule, apply_world_type_rule, default_feature_rule, default_population_multiplier,
    default_strategic_world_type, default_tech_multiplier, default_world_type_vector,
    strategic_population_multiplier, strategic_tech_multiplier,
};

// ── Loader ─────────────────────────────────────────────────────────────────────

/// Load `economy.toml`. Missing file → defaults (disabled).
///
/// # Errors
///
/// Returns [`SectorError::ConfigParse`] on malformed TOML and
/// [`SectorError::Io`] on read failure.
pub fn load_economy_file(path: &Utf8Path) -> Result<EconomyConfig, SectorError> {
    if !path.exists() {
        return Ok(EconomyConfig::default());
    }
    let text = fs::read_to_string(path).map_err(|e| SectorError::io(path.as_str(), e))?;
    let parsed: EconomyFile = toml::from_str(&text)
        .map_err(|e| SectorError::config_parse(path.as_str(), e.to_string()))?;
    Ok(parsed.into_config())
}

// ── Derivation ────────────────────────────────────────────────────────────────

#[must_use]
pub fn derive(sector: &GeneratedSector) -> EconomyReport {
    derive_with(
        sector,
        &EconomyConfig {
            enabled: true,
            ..Default::default()
        },
    )
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &EconomyConfig) -> EconomyReport {
    if !cfg.enabled {
        return EconomyReport::default();
    }
    let world_upper: usize = sector.systems.iter().map(|s| s.worlds.len()).sum();
    let mut worlds: Vec<WorldEconomy> = Vec::with_capacity(world_upper);
    let mut systems: Vec<SystemEconomy> = Vec::with_capacity(sector.systems.len());

    for sys in &sector.systems {
        let mut sys_vec = ResourceVector::default();
        let mut sys_strategic = StrategicOutput::default();
        for w in &sys.worlds {
            let pop_tag = w
                .tags
                .iter()
                .find(|t| t.starts_with("population:"))
                .cloned()
                .unwrap_or_default();
            let base = cfg
                .by_world_type
                .get(w.world.world_type.to_string().as_str())
                .cloned()
                .unwrap_or_else(|| default_world_type_vector(&w.world.world_type.to_string()));
            let tech = cfg
                .by_tech_level
                .get(w.world.tech_level.to_string().as_str())
                .copied()
                .unwrap_or_else(|| default_tech_multiplier(&w.world.tech_level.to_string()));
            let pop = cfg
                .by_population
                .get(pop_tag.as_ref())
                .copied()
                .unwrap_or_else(|| default_population_multiplier(&pop_tag));
            let vector = base.scale(tech * pop);
            sys_vec = add(&sys_vec, &vector);
            let (strategic_output, supply_resilience) =
                derive_world_strategic_output(w, cfg, &pop_tag);
            sys_strategic.add_assign(&strategic_output);

            let shortages: Vec<String> = RESOURCE_KEYS
                .iter()
                .filter(|k| vector.get(k) <= -20.0)
                .map(|k| (*k).to_string())
                .collect();

            worlds.push(WorldEconomy {
                system_id: sys.id.clone(),
                world_id: w.id.clone(),
                vector,
                strategic_output,
                tithe_status: TitheStatus::Adequate,
                supply_risk: SupplyRisk::Stable,
                strategic_priority: priority_for(&strategic_output),
                supply_resilience,
                stranded: false, // computed below once routes are processed
                shortages,
            });
        }
        let surplus_resources: Vec<String> = RESOURCE_KEYS
            .iter()
            .filter(|k| sys_vec.get(k) >= 20.0)
            .map(|k| (*k).to_string())
            .collect();
        let shortage_resources: Vec<String> = RESOURCE_KEYS
            .iter()
            .filter(|k| sys_vec.get(k) <= -20.0)
            .map(|k| (*k).to_string())
            .collect();
        systems.push(SystemEconomy {
            system_id: sys.id.clone(),
            vector: sys_vec,
            strategic_output: sys_strategic,
            tithe_status: TitheStatus::Adequate,
            supply_risk: SupplyRisk::Stable,
            strategic_priority: priority_for(&sys_strategic),
            surplus_resources,
            shortage_resources,
        });
    }

    // Routes: trade volume = endpoint gradient × friction.
    let by_sys: BTreeMap<&str, &SystemEconomy> =
        systems.iter().map(|s| (s.system_id.as_str(), s)).collect();
    let routes: Vec<RouteEconomy> = sector
        .routes
        .iter()
        .map(|r| route_economy(r, &by_sys))
        .collect();

    // TF-P-6: hoisted once and shared with both `derive_dependency_edges` and
    // the stranded check below. Previously each rebuilt their own copy.
    let mut valid_routes_by_sys: BTreeMap<&str, Vec<&crate::sector_model::GeneratedRoute>> =
        BTreeMap::new();
    for r in &sector.routes {
        if r.stability != RouteStability::Perilous {
            valid_routes_by_sys
                .entry(r.from_system_id.as_str())
                .or_default()
                .push(r);
            valid_routes_by_sys
                .entry(r.to_system_id.as_str())
                .or_default()
                .push(r);
        }
    }
    let dependency_edges =
        derive_dependency_edges(sector, &systems, &routes, &by_sys, &valid_routes_by_sys);

    // Stranded check: a world is stranded if it has any deficit ≥ 20 and the
    // system also nets a deficit there *and* no inbound route from a surplus
    // system on that resource exists.
    let mut stranded_world_idx: Vec<usize> = Vec::with_capacity(worlds.len());
    for (idx, we) in worlds.iter().enumerate() {
        let Some(sys) = by_sys.get(we.system_id.as_str()).copied() else {
            continue;
        };
        // Resources where the system itself is in deficit.
        let resource_deficits: Vec<&str> = RESOURCE_KEYS
            .iter()
            .copied()
            .filter(|k| sys.vector.get(k) <= -20.0 && we.vector.get(k) <= -20.0)
            .collect();
        if resource_deficits.is_empty() {
            continue;
        }
        let mut fix = false;
        if let Some(sys_routes) = valid_routes_by_sys.get(sys.system_id.as_str()) {
            for &r in sys_routes {
                let other = if r.from_system_id == sys.system_id {
                    r.to_system_id.as_str()
                } else {
                    r.from_system_id.as_str()
                };
                if let Some(other_sys) = by_sys.get(other) {
                    if resource_deficits
                        .iter()
                        .any(|k| other_sys.vector.get(k) >= 20.0)
                    {
                        fix = true;
                        break;
                    }
                }
            }
        }
        if !fix {
            stranded_world_idx.push(idx);
        }
    }
    for i in stranded_world_idx {
        worlds[i].stranded = true;
    }

    let world_refs: BTreeMap<&str, &GeneratedWorld> = sector
        .systems
        .iter()
        .flat_map(|s| s.worlds.iter().map(|w| (w.id.as_str(), w)))
        .collect();
    let world_counts: BTreeMap<&str, usize> = sector
        .systems
        .iter()
        .map(|s| (s.id.as_str(), s.worlds.len().max(1)))
        .collect();
    let system_refs: BTreeMap<&str, &crate::sector_model::GeneratedSystem> =
        sector.systems.iter().map(|s| (s.id.as_str(), s)).collect();

    // B1: pre-bucket dependency edges by (target system, resource) once, so the
    // per-system supply-risk classifier does an O(1) map lookup instead of
    // rescanning the full edge slice for every (world, resource) pair.
    let mut incoming_by_target: BTreeMap<(&str, &str), Vec<&DependencyEdge>> = BTreeMap::new();
    for e in &dependency_edges {
        incoming_by_target
            .entry((e.to_system_id.as_str(), e.resource.as_str()))
            .or_default()
            .push(e);
    }
    for sy in systems.iter_mut() {
        let count = *world_counts.get(sy.system_id.as_str()).unwrap_or(&1);
        let sys_ref = system_refs.get(sy.system_id.as_str()).copied();
        sy.supply_risk = system_supply_risk(sy, sys_ref, &incoming_by_target);
        sy.strategic_priority = priority_for(&sy.strategic_output);
        sy.tithe_status = system_tithe_status(sy, count, sys_ref);
    }
    let sys_risk: BTreeMap<&str, SupplyRisk> = systems
        .iter()
        .map(|s| (s.system_id.as_str(), s.supply_risk))
        .collect();
    for we in worlds.iter_mut() {
        let system_risk = sys_risk
            .get(we.system_id.as_str())
            .copied()
            .unwrap_or(SupplyRisk::Stable);
        we.supply_risk = world_supply_risk(we, system_risk);
        we.strategic_priority = priority_for(&we.strategic_output);
        if let Some(w) = world_refs.get(we.world_id.as_str()).copied() {
            we.tithe_status = world_tithe_status(w, we);
        }
    }

    // Sector totals.
    let mut sector_balance = ResourceVector::default();
    let mut strategic_output = StrategicOutput::default();
    for sy in &systems {
        sector_balance = add(&sector_balance, &sy.vector);
        strategic_output.add_assign(&sy.strategic_output);
    }

    EconomyReport {
        enabled: true,
        worlds,
        systems,
        routes,
        sector_balance,
        strategic_output,
        dependency_edges,
    }
}

fn derive_world_strategic_output(
    w: &GeneratedWorld,
    cfg: &EconomyConfig,
    pop_tag: &str,
) -> (StrategicOutput, f32) {
    let mut out = default_strategic_world_type(&w.world.world_type.to_string());
    if let Some(rule) = cfg
        .resources
        .world_type
        .get(w.world.world_type.to_string().as_str())
    {
        out = apply_world_type_rule(out, rule);
    }

    let mut multiplier = 1.0_f32;
    let mut resilience = base_resilience(&w.world.world_type.to_string());
    for feature in &w.world.notable_features {
        if let Some(rule) = default_feature_rule(feature.as_ref()) {
            apply_feature_rule(&mut out, &rule);
            multiplier *= rule.trade_multiplier.unwrap_or(1.0);
            resilience += rule.supply_resilience.unwrap_or(0.0);
        }
        if let Some(rule) = cfg.resources.notable_feature.get(feature.as_ref()) {
            apply_feature_rule(&mut out, rule);
            multiplier *= rule.trade_multiplier.unwrap_or(1.0);
            resilience += rule.supply_resilience.unwrap_or(0.0);
        }
    }

    let tech = strategic_tech_multiplier(&w.world.tech_level.to_string());
    let pop = strategic_population_multiplier(pop_tag, &w.world.population.to_string());
    let instability = w
        .stability
        .famine_or_resource_stress
        .max(w.stability.rebellion_risk * 0.75)
        .max(w.stability.corruption * 0.50)
        .max(w.stability.warp_instability * 0.40)
        .max(w.conflict.intensity as f32 * 0.80);
    let stability_factor = (1.0 - instability / 180.0).clamp(0.35, 1.0);
    let control_factor = if w.control.contested {
        0.82
    } else {
        (0.75 + w.control.control_score / 400.0).clamp(0.75, 1.0)
    };

    let output = out
        .scale(multiplier * tech * pop * stability_factor * control_factor)
        .clamp_scores();
    (output, resilience.clamp(0.0, 100.0))
}

fn base_resilience(world_type: &str) -> f32 {
    match world_type {
        "HiveWorld" | "ForgeWorld" | "IndustrialWorld" => 10.0,
        "AgriWorld" | "ExtractiveColony" => 15.0,
        "BastionWorld" => 25.0,
        "FrontierWorld" | "DeathWorld" => 5.0,
        _ => 0.0,
    }
}

fn priority_for(output: &StrategicOutput) -> StrategicPriority {
    match output.weighted_priority_score() {
        s if s < 35.0 => StrategicPriority::Low,
        s if s < 80.0 => StrategicPriority::Local,
        s if s < 150.0 => StrategicPriority::Subsector,
        s if s < 240.0 => StrategicPriority::Sector,
        _ => StrategicPriority::CrusadeLevel,
    }
}

pub(super) fn strategic_needs_for_world(world_type: &str) -> &'static [&'static str] {
    match world_type {
        "HiveWorld" => &["food"],
        "ForgeWorld" => &["food", "ore", "manpower"],
        "IndustrialWorld" => &["food", "ore", "manpower"],
        "BastionWorld" => &["food", "arms", "manpower"],
        "ShrineWorld" | "PleasureWorld" => &["food"],
        "ResearchStation" => &["food", "manufacturing"],
        "Orbital" | "Worldship" => &["food", "ore"],
        _ => &[],
    }
}

fn derive_dependency_edges<'a>(
    sector: &'a GeneratedSector,
    systems: &'a [SystemEconomy],
    routes: &'a [RouteEconomy],
    by_sys: &BTreeMap<&'a str, &'a SystemEconomy>,
    valid_routes_by_sys: &BTreeMap<&'a str, Vec<&'a crate::sector_model::GeneratedRoute>>,
) -> Vec<DependencyEdge> {
    let by_route: BTreeMap<&str, &RouteEconomy> =
        routes.iter().map(|r| (r.route_id.as_str(), r)).collect();

    let system_refs: BTreeMap<&str, &crate::sector_model::GeneratedSystem> =
        sector.systems.iter().map(|s| (s.id.as_str(), s)).collect();

    let mut out = Vec::with_capacity(systems.len() * 4);

    for consumer in systems {
        let Some(consumer_sys_ref) = system_refs.get(consumer.system_id.as_str()) else {
            continue;
        };
        let needs: BTreeSet<&str> = consumer_sys_ref
            .worlds
            .iter()
            .flat_map(|w| {
                strategic_needs_for_world(&w.world.world_type.to_string())
                    .iter()
                    .copied()
            })
            .collect();

        for resource in needs {
            if consumer.strategic_output.get(resource) >= SELF_SUFFICIENCY_OUTPUT {
                continue;
            }
            let mut best: Option<DependencyEdge> = None;
            if let Some(consumer_routes) = valid_routes_by_sys.get(consumer.system_id.as_str()) {
                for &r in consumer_routes {
                    let supplier_id = if r.from_system_id == consumer.system_id {
                        r.to_system_id.as_str()
                    } else {
                        r.from_system_id.as_str()
                    };
                    let Some(supplier) = by_sys.get(supplier_id).copied() else {
                        continue;
                    };
                    let supply = supplier.strategic_output.get(resource);
                    if supply < 35.0 {
                        continue;
                    }
                    let friction = by_route
                        .get(r.id.as_str())
                        .map(|e| e.friction)
                        .unwrap_or_else(|| friction_for(r));
                    let score = supply * friction / (r.distance.max(1) as f32).sqrt();
                    let edge = DependencyEdge {
                        from_system_id: supplier.system_id.clone(),
                        to_system_id: consumer.system_id.clone(),
                        resource: resource.to_string(),
                        route_id: Some(r.id.clone()),
                        score,
                        risk: import_risk(r, friction, score),
                    };
                    if best.as_ref().map(|b| edge.score > b.score).unwrap_or(true) {
                        best = Some(edge);
                    }
                }
            }
            if let Some(edge) = best {
                out.push(edge);
            }
        }
    }
    out.sort_by(|a, b| {
        a.to_system_id
            .cmp(&b.to_system_id)
            .then(a.resource.cmp(&b.resource))
            .then(a.from_system_id.cmp(&b.from_system_id))
    });
    out
}

fn route_economy(r: &GeneratedRoute, by_sys: &BTreeMap<&str, &SystemEconomy>) -> RouteEconomy {
    let a = by_sys.get(r.from_system_id.as_str()).copied();
    let b = by_sys.get(r.to_system_id.as_str()).copied();
    let gradient: f32 = match (a, b) {
        (Some(a), Some(b)) => {
            let legacy = RESOURCE_KEYS
                .iter()
                .map(|k| (a.vector.get(k) - b.vector.get(k)).abs())
                .sum::<f32>()
                / RESOURCE_KEYS.len() as f32;
            let strategic = STRATEGIC_RESOURCE_KEYS
                .iter()
                .map(|k| (a.strategic_output.get(k) - b.strategic_output.get(k)).abs())
                .sum::<f32>()
                / STRATEGIC_RESOURCE_KEYS.len() as f32;
            strategic.mul_add(0.5, legacy)
        }
        _ => 0.0,
    };
    let friction = friction_for(r);
    let distance_falloff = 1.0 / (r.distance.max(1) as f32);
    RouteEconomy {
        route_id: r.id.clone(),
        from_system_id: r.from_system_id.clone(),
        to_system_id: r.to_system_id.clone(),
        volume: (gradient * friction * distance_falloff).max(0.0),
        friction,
    }
}

fn friction_for(r: &GeneratedRoute) -> f32 {
    let mut f = match r.stability {
        RouteStability::Stable => 1.0,
        RouteStability::Unstable => 0.75,
        RouteStability::Hazardous => 0.45,
        RouteStability::Perilous => 0.10,
    };
    let max_piracy: f32 = r.controls.iter().map(|c| c.piracy).fold(0.0_f32, f32::max);
    let max_interdiction: f32 = r
        .controls
        .iter()
        .map(|c| c.interdiction)
        .fold(0.0_f32, f32::max);
    let max_patrol: f32 = r.controls.iter().map(|c| c.patrol).fold(0.0_f32, f32::max);
    f *= 1.0 - (max_piracy / ROUTE_PIRACY_DIVISOR).clamp(0.0, ROUTE_PIRACY_MAX_MALUS);
    f *= 1.0 - (max_interdiction / ROUTE_INTERDICTION_DIVISOR).clamp(0.0, ROUTE_INTERDICTION_MAX_MALUS);
    f *= 1.0 + (max_patrol / ROUTE_PATROL_DIVISOR).clamp(0.0, ROUTE_PATROL_MAX_BONUS);
    f.clamp(0.0, 1.5)
}

fn add(a: &ResourceVector, b: &ResourceVector) -> ResourceVector {
    let mut out = a.clone();
    let bs = b.fields();
    for (o, bf) in out.fields_mut().into_iter().zip(bs) {
        *o += bf;
    }
    out
}

/// §12 stability nudge: increase `famine_or_resource_stress` on every world
/// that is stranded on foodstuffs. Read-only and bounded; no other stability
/// fields are touched, so the conflict tick does not oscillate.
pub fn apply_stability_nudge(report: &EconomyReport, sector: &mut GeneratedSector) {
    if !report.enabled {
        return;
    }
    let stranded: BTreeMap<&str, bool> = report
        .worlds
        .iter()
        .filter(|w| w.stranded)
        .map(|w| (w.world_id.as_str(), true))
        .collect();
    for sys in sector.systems.iter_mut() {
        for w in sys.worlds.iter_mut() {
            if stranded.contains_key(w.id.as_str()) {
                let nudged = (w.stability.famine_or_resource_stress + 20.0).clamp(0.0, 100.0);
                w.stability.famine_or_resource_stress = nudged;
            }
        }
    }
}
