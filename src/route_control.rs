//! Per-route per-faction control derivation (§3 from
//! `faction_sector_control_and_power_design.md`, NEXT.md §3).
//!
//! Static snapshot only — no convoy / interdiction simulation. Each route
//! gets a `Vec<RouteControl>`, one entry per faction with meaningful
//! presence on at least one endpoint system. Fields are 0.0..=100.0 and
//! reflect projected behaviour:
//!
//! * `patrol` — declared military presence along the route (Navy / Guard /
//!   Astartes / Tau / Mechanicus and similar lawful kinds with high
//!   `military` on endpoints).
//! * `toll` — economic gatekeeping (Merchant / Mechanicus / Tau /
//!   sovereign Imperial with high `economic`).
//! * `interdiction` — Inquisition / Deathwatch / Necron blockade strength,
//!   amplified by `feature:quarantined` on the endpoints.
//! * `piracy` — Drukhari / Ork / criminal / rebel raid pressure
//!   (covert + military average).
//! * `secrecy` — `100 - avg(visibility)` for the faction across endpoint
//!   presences. Hidden / Aeldari / Harlequin kinds also get a flat bump.
//! * `confidence` — `avg(visibility)` for the faction across the
//!   endpoints; what observers actually know about the faction's traffic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::sector_model::{GeneratedFaction, GeneratedRoute, GeneratedSystem};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RouteControl {
    pub faction_id: crate::ids::FactionId,
    pub patrol: f32,
    pub toll: f32,
    pub interdiction: f32,
    pub piracy: f32,
    pub secrecy: f32,
    pub confidence: f32,
}

fn clamp(v: f32) -> f32 {
    v.clamp(0.0, 100.0)
}

#[derive(Default, Clone, Copy)]
struct Aggregate {
    military: f32,
    economic: f32,
    covert: f32,
    visibility: f32,
    count: f32,
}

fn endpoint_aggregates(sys: &GeneratedSystem) -> BTreeMap<&str, Aggregate> {
    let mut m: BTreeMap<&str, Aggregate> = BTreeMap::new();
    for w in &sys.worlds {
        for p in &w.factions {
            let e = m.entry(p.faction_id.as_ref()).or_default();
            let d = &p.dimensions;
            e.military += d.military;
            e.economic += d.economic;
            e.covert += d.covert;
            e.visibility += d.visibility;
            e.count += 1.0;
        }
    }
    for v in m.values_mut() {
        if v.count > 0.0 {
            v.military /= v.count;
            v.economic /= v.count;
            v.covert /= v.count;
            v.visibility /= v.count;
        }
    }
    m
}

fn merge_endpoints(a: Aggregate, b: Aggregate) -> Aggregate {
    let mut out = Aggregate::default();
    let mut n = 0.0;
    if a.count > 0.0 {
        out.military += a.military;
        out.economic += a.economic;
        out.covert += a.covert;
        out.visibility += a.visibility;
        n += 1.0;
    }
    if b.count > 0.0 {
        out.military += b.military;
        out.economic += b.economic;
        out.covert += b.covert;
        out.visibility += b.visibility;
        n += 1.0;
    }
    if n > 0.0 {
        out.military /= n;
        out.economic /= n;
        out.covert /= n;
        out.visibility /= n;
    }
    out.count = n;
    out
}

fn endpoint_has_tag(sys: &GeneratedSystem, needle: &str) -> bool {
    sys.worlds.iter().any(|w| {
        w.tags.iter().any(|t| t.contains(needle))
            || w.world.notable_features.iter().any(|f| f.contains(needle))
    })
}

fn derive_one(
    faction_id: &str,
    kind: &str,
    agg: Aggregate,
    quarantined: bool,
    war_zone: bool,
) -> Option<RouteControl> {
    if agg.count == 0.0 {
        return None;
    }
    let lawful_navy = matches!(
        kind,
        "imperial"
            | "imperial_guard"
            | "adeptus_astartes"
            | "deathwatch"
            | "grey_knights"
            | "talons_of_the_emperor"
            | "tau"
            | "mechanicus"
            | "leagues_of_votann"
            | "imperial_knight"
    );
    let economic = matches!(
        kind,
        "merchant" | "mechanicus" | "tau" | "imperial" | "leagues_of_votann"
    );
    let interdictor = matches!(
        kind,
        "inquisition" | "deathwatch" | "grey_knights" | "necron"
    );
    let pirate = matches!(kind, "chaos" | "criminal" | "drukhari" | "ork" | "rebel");
    let stealth = matches!(
        kind,
        "aeldari" | "harlequin" | "drukhari" | "cult" | "genestealer_cult"
    );

    let mut patrol = 0.0;
    if lawful_navy {
        patrol = agg.military;
        if war_zone {
            patrol = clamp(patrol + 10.0);
        }
    }

    let mut toll = 0.0;
    if economic {
        toll = agg.economic * 0.8;
    }

    let mut interdiction = 0.0;
    if interdictor {
        interdiction = (agg.military + agg.covert) * 0.5;
        if quarantined {
            interdiction = clamp(interdiction + 25.0);
        }
    }

    let mut piracy = 0.0;
    if pirate {
        piracy = (agg.military + agg.covert) * 0.5;
    }

    let mut secrecy = clamp(100.0 - agg.visibility);
    if stealth {
        secrecy = clamp(secrecy + 15.0);
    }

    let confidence = clamp(agg.visibility);

    let any = patrol + toll + interdiction + piracy > 5.0 || secrecy >= 50.0;
    if !any {
        return None;
    }

    Some(RouteControl {
        faction_id: crate::ids::FactionId::new(faction_id),
        patrol: clamp(patrol),
        toll: clamp(toll),
        interdiction: clamp(interdiction),
        piracy: clamp(piracy),
        secrecy,
        confidence,
    })
}

/// Build the per-faction `RouteControl` list for one route.
#[must_use]
pub fn derive_route_controls(
    route: &GeneratedRoute,
    systems_by_id: &BTreeMap<&str, &GeneratedSystem>,
    factions: &[GeneratedFaction],
) -> Vec<RouteControl> {
    let Some(a) = systems_by_id.get(route.from_system_id.as_ref()).copied() else {
        return Vec::new();
    };
    let Some(b) = systems_by_id.get(route.to_system_id.as_ref()).copied() else {
        return Vec::new();
    };
    let agg_a = endpoint_aggregates(a);
    let agg_b = endpoint_aggregates(b);
    let quarantined = endpoint_has_tag(a, "quarantined") || endpoint_has_tag(b, "quarantined");
    let war_zone = endpoint_has_tag(a, "war_zone") || endpoint_has_tag(b, "war_zone");

    let kind_of: BTreeMap<&str, &str> = factions
        .iter()
        .map(|f| (f.id.as_ref(), f.kind.as_ref()))
        .collect();

    let mut ids: BTreeMap<&str, ()> = BTreeMap::new();
    for k in agg_a.keys() {
        ids.insert(k, ());
    }
    for k in agg_b.keys() {
        ids.insert(k, ());
    }

    let mut out: Vec<RouteControl> = Vec::with_capacity(ids.len());
    for (id, _) in ids {
        let ea = agg_a.get(id).copied().unwrap_or_default();
        let eb = agg_b.get(id).copied().unwrap_or_default();
        let merged = merge_endpoints(ea, eb);
        let kind = kind_of.get(id).copied().unwrap_or("");
        if let Some(rc) = derive_one(id, kind, merged, quarantined, war_zone) {
            out.push(rc);
        }
    }
    out.sort_unstable_by(|a, b| a.faction_id.cmp(&b.faction_id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, FactionInfluence, GeneratedStar, GeneratedWorld, HexCoord, PowerProfile,
        PresenceDimensions, RouteStability, RouteType, SystemControlSummary, WorldControlSummary,
        WorldDto, WorldFactionPresence,
    };

    fn sys(
        id: &str,
        presences: Vec<(&str, PresenceDimensions)>,
        tags: Vec<&str>,
    ) -> GeneratedSystem {
        let world = GeneratedWorld {
            id: crate::ids::WorldId::new(format!("{id}-w1")),
            index: 1,
            name: "W".into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "amber".into(),
                star_colour_code: "A".into(),
                world_type: "AgriWorld".into(),
                atmosphere: "Breathable".into(),
                temperature: "Temperate".into(),
                biosphere: "Thriving".into(),
                population: "DenselyPopulated".into(),
                tech_level: "High".into(),
                government: "MagistrateCouncil".into(),
                notable_features: vec![],
            },
            factions: presences
                .into_iter()
                .map(|(fid, dims)| WorldFactionPresence {
                    faction_id: fid.into(),
                    subfaction_id: None,
                    subfaction_name: None,
                    force_id: None,
                    force_name: None,
                    influence: FactionInfluence::Significant,
                    relationship_to_government: "n".into(),
                    dimensions: dims,
                    dominance: DominanceState::default(),
                    intel_confidence: 100,
                })
                .collect(),
            tags: tags.into_iter().map(|t| t.into()).collect(),
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: Default::default(),
            regions: Vec::new(),
            conflict: Default::default(),
        };
        GeneratedSystem {
            id: id.into(),
            index: 1,
            name: id.into(),
            coord: HexCoord { q: 0, r: 0 },
            kind: crate::sector_model::SystemKind::Star,
            star: Some(GeneratedStar {
                colour_code: "A".into(),
                colour_name: "A".into(),
                spectral_type: None,
                source_row_index: None,
            }),
            worlds: vec![world],
            primary_factions: vec![],
            tags: vec![],
            notes: vec![],
            control: SystemControlSummary::default(),
            stability: Default::default(),
            orbital_assets: Vec::new(),
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        }
    }

    fn dims(mil: f32, econ: f32, cov: f32, vis: f32) -> PresenceDimensions {
        PresenceDimensions {
            military: mil,
            economic: econ,
            covert: cov,
            visibility: vis,
            ..PresenceDimensions::default()
        }
    }

    fn fac(id: &str, kind: &str) -> GeneratedFaction {
        GeneratedFaction {
            id: id.into(),
            name: id.into(),
            kind: kind.into(),
            disposition: "lawful".into(),
            subfactions: Vec::new(),
            system_presence: vec![],
            world_presence: vec![],
            power: PowerProfile::default(),
        }
    }

    fn route(a: &str, b: &str) -> GeneratedRoute {
        GeneratedRoute {
            id: crate::ids::RouteId::new(format!("{a}-{b}")),
            from_system_id: a.into(),
            to_system_id: b.into(),
            distance: 1,
            route_type: RouteType::StableWarpLane,
            stability: RouteStability::Stable,
            tags: vec![],
            controls: vec![],
        }
    }

    #[test]
    fn navy_endpoint_produces_patrol() {
        let a = sys("a", vec![("nav", dims(80.0, 30.0, 5.0, 90.0))], vec![]);
        let b = sys("b", vec![("nav", dims(60.0, 20.0, 5.0, 90.0))], vec![]);
        let factions = vec![fac("nav", "imperial_guard")];
        let mut by_id: BTreeMap<&str, &GeneratedSystem> = BTreeMap::new();
        by_id.insert("a", &a);
        by_id.insert("b", &b);
        let r = route("a", "b");
        let rcs = derive_route_controls(&r, &by_id, &factions);
        assert_eq!(rcs.len(), 1);
        assert!(rcs[0].patrol >= 60.0, "{}", rcs[0].patrol);
        assert!(rcs[0].confidence >= 80.0);
        assert_eq!(rcs[0].piracy, 0.0);
    }

    #[test]
    fn drukhari_endpoint_drives_piracy_and_secrecy() {
        let a = sys("a", vec![("drk", dims(40.0, 0.0, 60.0, 10.0))], vec![]);
        let b = sys("b", vec![("drk", dims(20.0, 0.0, 50.0, 10.0))], vec![]);
        let factions = vec![fac("drk", "drukhari")];
        let mut by_id: BTreeMap<&str, &GeneratedSystem> = BTreeMap::new();
        by_id.insert("a", &a);
        by_id.insert("b", &b);
        let r = route("a", "b");
        let rcs = derive_route_controls(&r, &by_id, &factions);
        assert_eq!(rcs.len(), 1);
        assert!(rcs[0].piracy >= 30.0);
        assert!(rcs[0].secrecy >= 90.0);
    }

    #[test]
    fn quarantine_amplifies_inquisitorial_interdiction() {
        let a = sys(
            "a",
            vec![("inq", dims(40.0, 0.0, 60.0, 50.0))],
            vec!["feature:quarantined"],
        );
        let b = sys("b", vec![("inq", dims(40.0, 0.0, 40.0, 50.0))], vec![]);
        let factions = vec![fac("inq", "inquisition")];
        let mut by_id: BTreeMap<&str, &GeneratedSystem> = BTreeMap::new();
        by_id.insert("a", &a);
        by_id.insert("b", &b);
        let r = route("a", "b");
        let rcs = derive_route_controls(&r, &by_id, &factions);
        assert_eq!(rcs.len(), 1);
        assert!(rcs[0].interdiction >= 60.0, "{}", rcs[0].interdiction);
    }
}
