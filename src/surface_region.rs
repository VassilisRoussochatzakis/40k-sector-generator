//! Surface-region modelling (§1 NEXT.md, design §6.1).
//!
//! Each `GeneratedWorld` whose type/population implies a meaningful
//! geography (Hive, Civilised, Agri, Forge, Shrine, Tomb, Cardinal, Knight,
//! Dead/Tomb necron) gets a list of named `SurfaceRegion`s. Per-region
//! `dominant` is computed from the world-level faction presences plus a
//! kind-specific bias: an Imperial admin presence claims the capital, a
//! covert presence claims the underhive, a hostile xenos presence claims
//! contested wilderness, etc. The world-level `control.dominant` is the
//! weighted average across regions — but per-region detail is preserved
//! so a "planet zoom" GUI can render contested districts.

use serde::{Deserialize, Serialize};

use crate::sector_model::GeneratedWorld;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SurfaceRegion {
    pub name: String,
    pub kind: RegionKind,
    /// Faction id dominating this region. May differ from the world's
    /// `control.dominant` when an Imperial capital sits over a Genestealer
    /// underhive.
    pub dominant: Option<crate::ids::FactionId>,
    /// 0..=100 — the dominant faction's local control score in this region.
    pub control_score: u8,
    /// Population weight: contribution to the world's weighted average
    /// (§6.1). Sums to ≤100 across regions.
    pub population_weight: u8,
    /// 0..=100 visibility — covert regions (underhive, tomb interior) read
    /// low even when surface visibility is high.
    pub visibility: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    Capital,
    Hive,
    Underhive,
    ForgeComplex,
    ShrineContinent,
    AgriBelt,
    CardinalSpire,
    KnightHousehold,
    Wilderness,
    TombComplex,
    Hideout,
    Other,
}

/// Build the per-world region list. Returns an empty Vec when the world
/// type/population doesn't warrant a region split (e.g. uninhabited barren
/// worlds).
#[must_use]
pub fn derive_regions(w: &GeneratedWorld) -> Vec<SurfaceRegion> {
    let wt = w.world.world_type.as_str();
    let pop = w.world.population.as_str();
    if pop == "Uninhabited" && !matches!(wt, "TombWorld" | "DeadWorld" | "WarpLostWorld") {
        return Vec::new();
    }

    let kinds: Vec<(RegionKind, u8, &'static str)> = match wt {
        "HiveWorld" => vec![
            (RegionKind::Capital, 25, "Sector Capital"),
            (RegionKind::Hive, 45, "Primary Hive"),
            (RegionKind::Underhive, 25, "Underhive"),
            (RegionKind::Wilderness, 5, "Outer Wastes"),
        ],
        "CivilisedWorld" | "Civilised" => vec![
            (RegionKind::Capital, 30, "Capital Conurbation"),
            (RegionKind::AgriBelt, 30, "Agri Belt"),
            (RegionKind::Hideout, 10, "Outback Settlements"),
            (RegionKind::Wilderness, 30, "Wild Continents"),
        ],
        "ForgeWorld" => vec![
            (RegionKind::ForgeComplex, 50, "Primary Forge"),
            (RegionKind::Capital, 15, "Magos Spire"),
            (RegionKind::Underhive, 15, "Servitor Underworks"),
            (RegionKind::Wilderness, 20, "Slag Plains"),
        ],
        "AgriWorld" => vec![
            (RegionKind::AgriBelt, 70, "Agri Belt"),
            (RegionKind::Capital, 10, "Provincial Capital"),
            (RegionKind::Wilderness, 20, "Wild Districts"),
        ],
        "ShrineWorld" => vec![
            (RegionKind::ShrineContinent, 55, "Pilgrim Continent"),
            (RegionKind::CardinalSpire, 25, "Cardinal Spire"),
            (RegionKind::Hideout, 5, "Heretic Cells"),
            (RegionKind::Wilderness, 15, "Outer Cloisters"),
        ],
        "CardinalWorld" => vec![
            (RegionKind::CardinalSpire, 60, "Cardinal Palace"),
            (RegionKind::Capital, 20, "Sector Adjudication"),
            (RegionKind::Underhive, 20, "Penitent Quarter"),
        ],
        "KnightWorld" => vec![
            (RegionKind::KnightHousehold, 50, "Knight Household"),
            (RegionKind::AgriBelt, 30, "Tithe Lands"),
            (RegionKind::Wilderness, 20, "Wilds"),
        ],
        "TombWorld" => vec![
            (RegionKind::TombComplex, 60, "Inner Tomb"),
            (RegionKind::Wilderness, 40, "Surface Wastes"),
        ],
        "DeathWorld" => vec![
            (RegionKind::Capital, 15, "Garrison"),
            (RegionKind::Wilderness, 75, "Death-World Biome"),
            (RegionKind::Hideout, 10, "Cult Cells"),
        ],
        "FortressWorld" => vec![
            (RegionKind::Capital, 35, "Fortress Garrison"),
            (RegionKind::ForgeComplex, 15, "Armoury"),
            (RegionKind::Wilderness, 50, "Outer Ranges"),
        ],
        _ => vec![
            (RegionKind::Capital, 30, "Capital"),
            (RegionKind::Wilderness, 70, "Hinterland"),
        ],
    };

    let mut out: Vec<SurfaceRegion> = Vec::with_capacity(kinds.len());
    for (kind, weight, name) in kinds {
        let (dominant, score, visibility) = pick_region_dominant(w, kind);
        out.push(SurfaceRegion {
            name: name.to_string(),
            kind,
            dominant,
            control_score: score,
            population_weight: weight,
            visibility,
        });
    }
    out
}

fn pick_region_dominant(
    w: &GeneratedWorld,
    kind: RegionKind,
) -> (Option<crate::ids::FactionId>, u8, u8) {
    if w.factions.is_empty() {
        return (None, 0, 0);
    }
    let mut best: Option<(&str, f32, f32)> = None;
    for p in &w.factions {
        let bias = region_bias(kind, &p.faction_id, &p.dimensions);
        if bias <= 0.0 {
            continue;
        }
        let score = p.dimensions.local_control_score() * bias;
        match best {
            Some((_, bs, _)) if bs >= score => {}
            _ => best = Some((p.faction_id.as_str(), score, p.dimensions.visibility)),
        }
    }
    match best {
        Some((id, s, v)) => (
            Some(crate::ids::FactionId::new(id)),
            s.round().clamp(0.0, 100.0) as u8,
            v.round().clamp(0.0, 100.0) as u8,
        ),
        None => (None, 0, 0),
    }
}

fn region_bias(kind: RegionKind, fid: &str, d: &crate::sector_model::PresenceDimensions) -> f32 {
    // Use only the presence dimension shape — kind hints are encoded by
    // weighting different dimensions per region kind. Faction-id substrings
    // bias narrative ownership where the dimensions alone would tie.
    let id_has = |needle: &str| fid.contains(needle);
    match kind {
        RegionKind::Capital => 0.6 + d.admin * 0.005 + d.legitimacy * 0.004,
        RegionKind::Hive => 0.6 + d.economic * 0.004 + d.industrial * 0.004 + d.admin * 0.002,
        RegionKind::Underhive => {
            let mut b = 0.4 + d.covert * 0.01 + (100.0 - d.visibility) * 0.003;
            if id_has("genestealer") || id_has("cult") || id_has("criminal") {
                b += 0.4;
            }
            b
        }
        RegionKind::ForgeComplex => {
            let mut b = 0.4 + d.industrial * 0.008;
            if id_has("mechanicus") {
                b += 0.4;
            }
            b
        }
        RegionKind::ShrineContinent => {
            let mut b = 0.4 + d.ideological * 0.008;
            if id_has("ecclesiarch") || id_has("sororitas") || id_has("shrine") {
                b += 0.4;
            }
            b
        }
        RegionKind::AgriBelt => 0.5 + d.economic * 0.005 + d.legitimacy * 0.002,
        RegionKind::CardinalSpire => {
            let mut b = 0.4 + d.ideological * 0.006 + d.legitimacy * 0.003;
            if id_has("ecclesiarch") {
                b += 0.5;
            }
            b
        }
        RegionKind::KnightHousehold => {
            let mut b = 0.3 + d.military * 0.003;
            if id_has("knight") {
                b += 0.6;
            }
            b
        }
        RegionKind::Wilderness => {
            let mut b = 0.3 + d.military * 0.002;
            if id_has("tyranid") || id_has("ork") || id_has("daemon") {
                b += 0.4;
            }
            b
        }
        RegionKind::TombComplex => {
            let mut b = 0.1;
            if id_has("necron") || id_has("tomb") {
                b += 0.9;
            }
            b
        }
        RegionKind::Hideout => 0.3 + d.covert * 0.01,
        RegionKind::Other => 0.5 + d.admin * 0.003,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, FactionInfluence, GeneratedWorld, PresenceDimensions, WorldControlSummary,
        WorldDto, WorldFactionPresence,
    };

    fn mk_world(world_type: &str, presences: Vec<(&str, PresenceDimensions)>) -> GeneratedWorld {
        GeneratedWorld {
            id: "sys-0001-w1".into(),
            index: 1,
            name: "W".into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "amber".into(),
                star_colour_code: "A".into(),
                world_type: world_type.into(),
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
                    influence: FactionInfluence::Significant,
                    relationship_to_government: "lawful".into(),
                    dimensions: dims,
                    dominance: DominanceState::default(),
                    intel_confidence: 100,
                })
                .collect(),
            tags: vec![],
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: Default::default(),
            regions: Vec::new(),
            conflict: Default::default(),
        }
    }

    #[test]
    fn hive_world_splits_into_four_regions() {
        let w = mk_world(
            "HiveWorld",
            vec![(
                "imperial",
                PresenceDimensions {
                    admin: 80.0,
                    economic: 60.0,
                    visibility: 90.0,
                    ..Default::default()
                },
            )],
        );
        let r = derive_regions(&w);
        assert_eq!(r.len(), 4);
        assert!(r.iter().any(|x| x.kind == RegionKind::Underhive));
    }

    #[test]
    fn genestealer_cult_dominates_underhive_even_when_imperial_owns_capital() {
        let w = mk_world(
            "HiveWorld",
            vec![
                (
                    "imperial",
                    PresenceDimensions {
                        admin: 80.0,
                        legitimacy: 70.0,
                        visibility: 90.0,
                        ..Default::default()
                    },
                ),
                (
                    "genestealer_cult_alpha",
                    PresenceDimensions {
                        covert: 80.0,
                        visibility: 10.0,
                        ..Default::default()
                    },
                ),
            ],
        );
        let r = derive_regions(&w);
        let underhive = r.iter().find(|x| x.kind == RegionKind::Underhive).unwrap();
        assert_eq!(
            underhive.dominant.as_deref(),
            Some("genestealer_cult_alpha")
        );
        let capital = r.iter().find(|x| x.kind == RegionKind::Capital).unwrap();
        assert_eq!(capital.dominant.as_deref(), Some("imperial"));
    }
}
