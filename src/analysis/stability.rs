//! Static stability snapshot (§11.1, design doc §6 deferred item).
//!
//! No sim ticks — every field is derived from tags, world type, factions
//! present, and the existing multi-winner control summary. Values are
//! deterministic given a finalised sector.

use crate::sector_model::GeneratedSystem;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::sector_model::{GeneratedFaction, GeneratedWorld, SystemState};

/// All fields are 0.0..=100.0. Higher = more of the named thing.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct StabilityState {
    pub public_order: f32,
    pub corruption: f32,
    pub fear: f32,
    pub rebellion_risk: f32,
    pub xenos_threat: f32,
    pub warp_instability: f32,
    pub famine_or_resource_stress: f32,
}

impl StabilityState {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn clamp(v: f32) -> f32 {
    v.clamp(0.0, 100.0)
}

fn kind_map(factions: &[GeneratedFaction]) -> BTreeMap<&str, &str> {
    factions
        .iter()
        .map(|f| (f.id.as_str(), f.kind.as_slug()))
        .collect()
}

fn world_has_tag(w: &GeneratedWorld, needle: &str) -> bool {
    w.tags.iter().any(|t| t.contains(needle))
        || w.world
            .notable_features
            .iter()
            .any(|f| f.as_ref().contains(needle))
}

#[must_use]
pub fn derive_world_stability(w: &GeneratedWorld, factions: &[GeneratedFaction]) -> StabilityState {
    let kinds = kind_map(factions);

    let any_kind = |needles: &[&str]| {
        w.factions.iter().any(|p| {
            let k = p
                .subfaction_id
                .as_deref()
                .or_else(|| kinds.get(p.faction_id.as_str()).copied())
                .unwrap_or("");
            needles.contains(&k)
        })
    };
    let any_kind_starts = |prefixes: &[&str]| {
        w.factions.iter().any(|p| {
            let k = p
                .subfaction_id
                .as_deref()
                .or_else(|| kinds.get(p.faction_id.as_str()).copied())
                .unwrap_or("");
            prefixes.iter().any(|pre| k.starts_with(*pre))
        })
    };

    let war_zone = world_has_tag(w, "war_zone");
    let daemonic = world_has_tag(w, "daemonic_corruption");
    let warp_phen = world_has_tag(w, "warp_phenomena");
    let quarantined = world_has_tag(w, "quarantined");
    let famine = world_has_tag(w, "famine");
    let warp_lost_world = w.world.world_type == crate::worlds::WorldType::WarpLostWorld;

    let chaos = any_kind_starts(&["chaos_", "traitor_", "dark_mechanicum"])
        || any_kind(&["daemon", "cult"]);
    let xenos_tyranid = any_kind(&["tyranid"]);
    let xenos_ork = any_kind(&["ork"]);
    let xenos_necron = any_kind(&["necron"]);
    let xenos_steal = any_kind(&["genestealer_cult"]);
    let xenos_other = any_kind(&["xenos", "minor_xenos"]);
    let rebel_present = any_kind(&["rebel", "cult", "genestealer_cult"]);

    let contested = w.control.contested;
    let dom_score = w.control.control_score;

    // public_order: high baseline, eroded by violence / unrest signals.
    let mut public_order: f32 = 70.0;
    if contested {
        public_order -= 25.0;
    }
    if war_zone {
        public_order -= 20.0;
    }
    if daemonic {
        public_order -= 15.0;
    }
    if rebel_present {
        public_order -= 10.0;
    }
    if dom_score >= 70.0 {
        public_order += 10.0;
    }
    if quarantined {
        public_order -= 15.0;
    }

    // corruption: chaos / dark mechanicus footprint + warp taint.
    let mut corruption: f32 = 0.0;
    if chaos {
        corruption += 35.0;
    }
    if daemonic {
        corruption += 35.0;
    }
    if warp_phen {
        corruption += 15.0;
    }
    if w.factions.iter().any(|p| {
        let k = p
            .subfaction_id
            .as_deref()
            .or_else(|| kinds.get(p.faction_id.as_str()).copied())
            .unwrap_or("");
        matches!(k, "cult" | "genestealer_cult")
    }) {
        corruption += 15.0;
    }

    // fear: violence / dread signals.
    let mut fear: f32 = 0.0;
    if war_zone {
        fear += 25.0;
    }
    if daemonic {
        fear += 25.0;
    }
    if xenos_tyranid {
        fear += 25.0;
    }
    if any_kind(&["daemon"]) {
        fear += 20.0;
    }
    if contested {
        fear += 15.0;
    }
    if quarantined {
        fear += 15.0;
    }

    // rebellion_risk: contested + low loyalty signals.
    let mut rebellion_risk: f32 = 0.0;
    if contested {
        rebellion_risk += 30.0;
    }
    if rebel_present {
        rebellion_risk += 30.0;
    }
    if dom_score < 35.0 {
        rebellion_risk += 20.0;
    }
    if corruption >= 50.0 {
        rebellion_risk += 10.0;
    }

    // xenos_threat: presence-weighted by hostility.
    let mut xenos_threat: f32 = 0.0;
    if xenos_tyranid {
        xenos_threat += 40.0;
    }
    if xenos_necron {
        xenos_threat += 30.0;
    }
    if xenos_ork {
        xenos_threat += 25.0;
    }
    if xenos_steal {
        xenos_threat += 20.0;
    }
    if xenos_other {
        xenos_threat += 10.0;
    }

    // warp_instability: warp-tag + warp-lost type + daemonic presence.
    let mut warp_instability: f32 = 0.0;
    if warp_phen {
        warp_instability += 40.0;
    }
    if daemonic {
        warp_instability += 30.0;
    }
    if any_kind(&["daemon"]) {
        warp_instability += 25.0;
    }
    if warp_lost_world {
        warp_instability += 50.0;
    }

    // famine_or_resource_stress: war / blockade / explicit famine tag.
    let mut famine_or_resource_stress: f32 = 0.0;
    if famine {
        famine_or_resource_stress += 50.0;
    }
    if war_zone {
        famine_or_resource_stress += 25.0;
    }
    if quarantined {
        famine_or_resource_stress += 25.0;
    }

    StabilityState {
        public_order: clamp(public_order),
        corruption: clamp(corruption),
        fear: clamp(fear),
        rebellion_risk: clamp(rebellion_risk),
        xenos_threat: clamp(xenos_threat),
        warp_instability: clamp(warp_instability),
        famine_or_resource_stress: clamp(famine_or_resource_stress),
    }
}

/// Average each field across the system's worlds, then add a flat bonus driven
/// by the system-level state classification so a Warzone / Blockaded system
/// reads as more unstable than the per-world average alone.
#[must_use]
pub fn derive_system_stability(sys: &GeneratedSystem) -> StabilityState {
    let worlds = &sys.worlds;
    if worlds.is_empty() {
        return StabilityState::default();
    }
    let n = worlds.len() as f32;
    let mut acc = StabilityState::default();
    for w in worlds {
        acc.public_order += w.stability.public_order;
        acc.corruption += w.stability.corruption;
        acc.fear += w.stability.fear;
        acc.rebellion_risk += w.stability.rebellion_risk;
        acc.xenos_threat += w.stability.xenos_threat;
        acc.warp_instability += w.stability.warp_instability;
        acc.famine_or_resource_stress += w.stability.famine_or_resource_stress;
    }
    acc.public_order /= n;
    acc.corruption /= n;
    acc.fear /= n;
    acc.rebellion_risk /= n;
    acc.xenos_threat /= n;
    acc.warp_instability /= n;
    acc.famine_or_resource_stress /= n;

    match sys.control.state {
        Some(SystemState::Warzone) => {
            acc.public_order = clamp(acc.public_order - 15.0);
            acc.fear = clamp(acc.fear + 15.0);
            acc.famine_or_resource_stress = clamp(acc.famine_or_resource_stress + 10.0);
        }
        Some(SystemState::Blockaded) => {
            acc.public_order = clamp(acc.public_order - 5.0);
            acc.famine_or_resource_stress = clamp(acc.famine_or_resource_stress + 15.0);
        }
        Some(SystemState::Infiltrated) => {
            acc.corruption = clamp(acc.corruption + 10.0);
            acc.rebellion_risk = clamp(acc.rebellion_risk + 10.0);
        }
        Some(SystemState::Quarantined) => {
            acc.public_order = clamp(acc.public_order - 10.0);
            acc.fear = clamp(acc.fear + 15.0);
            acc.famine_or_resource_stress = clamp(acc.famine_or_resource_stress + 10.0);
        }
        _ => {}
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, FactionInfluence, PowerProfile, PresenceDimensions, WorldControlSummary,
        WorldDto, WorldFactionPresence,
    };

    fn faction(id: &str, kind: &str) -> GeneratedFaction {
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

    fn world_with(
        tags: Vec<&str>,
        ftype: crate::worlds::WorldType,
        factions: Vec<&str>,
    ) -> GeneratedWorld {
        GeneratedWorld {
            id: "w".into(),
            index: 1,
            name: "W".into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: crate::worlds::StarColour::White,
                world_type: ftype,
                atmosphere: crate::worlds::Atmosphere::Breathable,
                temperature: crate::worlds::Temperature::Temperate,
                biosphere: crate::worlds::Biosphere::Thriving,
                population: crate::worlds::Population::DenselyPopulated,
                tech_level: crate::worlds::TechLevel::High,
                government: crate::worlds::Government::MagistrateCouncil,
                notable_features: vec![],
            },
            factions: factions
                .into_iter()
                .map(|id| WorldFactionPresence {
                    faction_id: id.into(),
                    subfaction_id: None,
                    subfaction_name: None,
                    force_id: None,
                    force_name: None,
                    influence: FactionInfluence::Significant,
                    relationship_to_government: "neutral".into(),
                    dimensions: PresenceDimensions::default(),
                    dominance: DominanceState::default(),
                    intel_confidence: 100,
                })
                .collect(),
            tags: tags.into_iter().map(|t| t.into()).collect(),
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: StabilityState::default(),
            regions: Vec::new(),
            conflict: Default::default(),
            intel: Default::default(),
        }
    }

    #[test]
    fn chaos_corrupted_world_scores_high_corruption_and_warp() {
        let factions = vec![faction("daemon1", "daemon")];
        let w = world_with(
            vec!["feature:daemonic_corruption"],
            crate::worlds::WorldType::DeathWorld,
            vec!["daemon1"],
        );
        let s = derive_world_stability(&w, &factions);
        assert!(s.corruption >= 50.0, "{}", s.corruption);
        assert!(s.warp_instability >= 50.0, "{}", s.warp_instability);
        assert!(s.fear >= 40.0, "{}", s.fear);
    }

    #[test]
    fn tyranid_presence_drives_xenos_threat_and_fear() {
        let factions = vec![faction("hive1", "tyranid")];
        let w = world_with(vec![], crate::worlds::WorldType::AgriWorld, vec!["hive1"]);
        let s = derive_world_stability(&w, &factions);
        assert!(s.xenos_threat >= 40.0);
        assert!(s.fear >= 20.0);
    }

    #[test]
    fn quiet_imperial_world_keeps_high_public_order() {
        let factions = vec![faction("imp1", "imperial")];
        let mut w = world_with(vec![], crate::worlds::WorldType::AgriWorld, vec!["imp1"]);
        w.control.control_score = 80.0;
        let s = derive_world_stability(&w, &factions);
        assert!(s.public_order >= 70.0, "{}", s.public_order);
        assert_eq!(s.xenos_threat, 0.0);
        assert_eq!(s.warp_instability, 0.0);
    }
}
