//! Special-faction archetype rules (§11 NEXT.md, design §16).
//!
//! Each `apply_*` function takes a finalised sector and mutates it in
//! place to layer faction-specific narrative state on top of the generic
//! presence model. All rules are deterministic; no RNG. Applied as a
//! post-pass after presences, claims, control summaries, stability and
//! orbital assets are computed but before render. The order of
//! application matters because later rules read fields populated by
//! earlier ones (e.g. tyranid biomass front consumes the
//! `xenos_threat` stability field).
//!
//! Sub-rules covered (design §16.1–§16.12):
//! 1. Imperial governance stack (§16.1) — overlapping sovereignty between
//!    Administratum, Ecclesiarchy, Mechanicus, Navy, Knights.
//! 2. Necron dormant/awakening (§16.9).
//! 3. Tyranid biomass front + consumption gradient (§16.8).
//! 4. Ork Waaagh! momentum (§16.7).
//! 5. Genestealer staged uprising (§16.6).
//! 6. Tau sphere of influence (§16.11).
//! 7. Aeldari/Drukhari/Harlequin intermittent appearance (§16.10).
//! 8. Chaos corruption + metaphysical layer (§16.12).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ids::{FactionId, SystemId};
use crate::sector_model::GeneratedSector;

/// Per-system archetype-derived state. Lives on `GeneratedSystem.archetype`
/// (added below as a default field).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ArchetypeState {
    /// §16.1 Imperial governance overlap — list of co-sovereign faction
    /// ids that share Imperial authority over this system.
    #[serde(default)]
    pub imperial_co_sovereigns: Vec<FactionId>,
    /// §16.9 Necron phase: dormant / awakening / awake.
    #[serde(default)]
    pub necron_phase: NecronPhase,
    /// §16.8 Tyranid front stage.
    #[serde(default)]
    pub tyranid_stage: TyranidStage,
    /// §16.7 Ork Waaagh! intensity 0..=100.
    #[serde(default)]
    pub ork_waaagh: u8,
    /// §16.6 Genestealer uprising stage.
    #[serde(default)]
    pub gsc_stage: GscStage,
    /// §16.11 Tau sphere band: core / client / fringe / contact / none.
    #[serde(default)]
    pub tau_sphere: TauSphereBand,
    /// §16.10 Aeldari intermittent activity 0..=100.
    #[serde(default)]
    pub aeldari_activity: u8,
    /// §16.12 Chaos corruption depth 0..=100.
    #[serde(default)]
    pub chaos_corruption: u8,
    /// §16.12 Daemonic manifestation probability 0..=100.
    #[serde(default)]
    pub daemon_manifestation: u8,
}

impl ArchetypeState {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NecronPhase {
    #[default]
    None,
    Dormant,
    Awakening,
    Awake,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TyranidStage {
    #[default]
    None,
    Inhabited,
    Besieged,
    Consumed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GscStage {
    #[default]
    None,
    Rumor,
    HiddenCell,
    DistrictControl,
    ParallelGovernment,
    Uprising,
    PlanetarySeizure,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TauSphereBand {
    #[default]
    None,
    Contact,
    Fringe,
    Client,
    Core,
}

/// Apply all archetype rules. Mutates `sector` in place.
pub fn apply_all(sector: &mut GeneratedSector) {
    let kinds: BTreeMap<FactionId, String> = sector
        .factions
        .iter()
        .map(|f| (f.id.clone(), f.kind.clone()))
        .collect();

    apply_imperial_governance(sector, &kinds);
    apply_necron_phase(sector, &kinds);
    apply_tyranid_front(sector, &kinds);
    apply_ork_waaagh(sector, &kinds);
    apply_genestealer_stages(sector, &kinds);
    apply_tau_sphere(sector, &kinds);
    apply_aeldari_intermittent(sector, &kinds);
    apply_chaos_corruption(sector, &kinds);
}

// ── §16.1 Imperial governance stack ────────────────────────────────────────────

fn apply_imperial_governance(sector: &mut GeneratedSector, kinds: &BTreeMap<FactionId, String>) {
    let imperial_stack: &[&str] = &[
        "imperial",
        "adepta_sororitas",
        "inquisition",
        "adeptus_astartes",
        "imperial_guard",
        "imperial_knight",
        "collegia_titanica",
        "mechanicus",
    ];
    for sys in sector.systems.iter_mut() {
        let mut co_sovereigns: BTreeSet<FactionId> = BTreeSet::new();
        for w in &sys.worlds {
            for p in &w.factions {
                let kind = kinds.get(&p.faction_id).map(|s| s.as_str()).unwrap_or("");
                if imperial_stack.contains(&kind) {
                    // Threshold on the *governance-bearing* dimensions so a
                    // mere Inquisition covert cell doesn't count as a
                    // co-sovereign, but a Mechanicus magos with
                    // industrial/admin presence does.
                    let weight = p.dimensions.admin
                        + p.dimensions.legitimacy
                        + p.dimensions.industrial * 0.5
                        + p.dimensions.ideological * 0.5;
                    if weight >= 30.0 {
                        co_sovereigns.insert(p.faction_id.clone());
                    }
                }
            }
        }
        if co_sovereigns.len() >= 2 {
            sys.archetype.imperial_co_sovereigns = co_sovereigns.into_iter().collect();
        }
    }
}

// ── §16.9 Necron dormant / awakening ───────────────────────────────────────────

fn apply_necron_phase(sector: &mut GeneratedSector, kinds: &BTreeMap<FactionId, String>) {
    for sys in sector.systems.iter_mut() {
        let mut max_score: f32 = 0.0;
        let mut max_visibility: f32 = 0.0;
        let mut tomb_tag = false;
        for w in &sys.worlds {
            if w.world.notable_features.iter().any(|f| f == "TombWorld") {
                tomb_tag = true;
            }
            if w.world.world_type == "TombWorld" {
                tomb_tag = true;
            }
            for p in &w.factions {
                let kind = kinds.get(&p.faction_id).map(|s| s.as_str()).unwrap_or("");
                if kind == "necron" {
                    max_score = max_score.max(p.dimensions.local_control_score());
                    max_visibility = max_visibility.max(p.dimensions.visibility);
                }
            }
        }
        sys.archetype.necron_phase = if max_score <= 0.0 && !tomb_tag {
            NecronPhase::None
        } else if max_visibility < 20.0 && max_score < 35.0 {
            NecronPhase::Dormant
        } else if max_visibility < 60.0 || max_score < 60.0 {
            NecronPhase::Awakening
        } else {
            NecronPhase::Awake
        };
    }
}

// ── §16.8 Tyranid biomass front ────────────────────────────────────────────────

fn apply_tyranid_front(sector: &mut GeneratedSector, kinds: &BTreeMap<FactionId, String>) {
    for sys in sector.systems.iter_mut() {
        let mut max_score: f32 = 0.0;
        let mut consumed = false;
        for w in &sys.worlds {
            for p in &w.factions {
                let kind = kinds.get(&p.faction_id).map(|s| s.as_str()).unwrap_or("");
                if kind == "tyranid" {
                    max_score = max_score.max(p.dimensions.local_control_score());
                    if w.world.population == "Uninhabited"
                        && p.dimensions.local_control_score() >= 60.0
                    {
                        consumed = true;
                    }
                }
            }
        }
        sys.archetype.tyranid_stage = if max_score <= 0.0 {
            TyranidStage::None
        } else if consumed {
            // Worlds near a high-Tyranid system lose communication
            // (suppressed comms / Shadow of the Warp).
            TyranidStage::Consumed
        } else if max_score >= 35.0 {
            TyranidStage::Besieged
        } else {
            TyranidStage::Inhabited
        };
        if matches!(
            sys.archetype.tyranid_stage,
            TyranidStage::Consumed | TyranidStage::Besieged
        ) {
            // Tag every world: shadow_of_the_warp.
            for w in sys.worlds.iter_mut() {
                let tag = "feature:shadow_of_the_warp".to_string();
                if !w.tags.iter().any(|t| t == &tag) {
                    w.tags.push(tag);
                    w.tags.sort();
                }
            }
        }
    }
}

// ── §16.7 Ork Waaagh! momentum ─────────────────────────────────────────────────

fn apply_ork_waaagh(sector: &mut GeneratedSector, kinds: &BTreeMap<FactionId, String>) {
    for sys in sector.systems.iter_mut() {
        let mut score: f32 = 0.0;
        let mut count = 0;
        for w in &sys.worlds {
            for p in &w.factions {
                let kind = kinds.get(&p.faction_id).map(|s| s.as_str()).unwrap_or("");
                if kind == "ork" {
                    score += p.dimensions.military;
                    count += 1;
                }
            }
        }
        let v = if count == 0 {
            0.0
        } else {
            (score / count as f32).min(100.0)
        };
        sys.archetype.ork_waaagh = v.round().clamp(0.0, 100.0) as u8;
    }
}

// ── §16.6 Genestealer staged uprising ─────────────────────────────────────────

fn apply_genestealer_stages(sector: &mut GeneratedSector, kinds: &BTreeMap<FactionId, String>) {
    for sys in sector.systems.iter_mut() {
        let mut max_score: f32 = 0.0;
        let mut min_visibility: f32 = 100.0;
        let mut has_gsc = false;
        for w in &sys.worlds {
            for p in &w.factions {
                let kind = kinds.get(&p.faction_id).map(|s| s.as_str()).unwrap_or("");
                if kind == "genestealer_cult" {
                    has_gsc = true;
                    max_score = max_score.max(p.dimensions.local_control_score());
                    min_visibility = min_visibility.min(p.dimensions.visibility);
                }
            }
        }
        sys.archetype.gsc_stage = if !has_gsc {
            GscStage::None
        } else if max_score < 10.0 {
            GscStage::Rumor
        } else if min_visibility < 20.0 && max_score < 25.0 {
            GscStage::HiddenCell
        } else if max_score < 40.0 {
            GscStage::DistrictControl
        } else if max_score < 55.0 {
            GscStage::ParallelGovernment
        } else if max_score < 75.0 {
            GscStage::Uprising
        } else {
            GscStage::PlanetarySeizure
        };
    }
}

// ── §16.11 Tau sphere of influence ────────────────────────────────────────────

fn apply_tau_sphere(sector: &mut GeneratedSector, kinds: &BTreeMap<FactionId, String>) {
    // Tau home-system aggregate (= max tau presence anywhere in the sector).
    let max_tau_at: BTreeMap<SystemId, f32> = sector
        .systems
        .iter()
        .map(|sys| {
            let s = sys
                .worlds
                .iter()
                .flat_map(|w| w.factions.iter())
                .filter(|p| kinds.get(&p.faction_id).map(|s| s.as_str()).unwrap_or("") == "tau")
                .map(|p| p.dimensions.local_control_score())
                .fold(0.0_f32, f32::max);
            (sys.id.clone(), s)
        })
        .collect();
    for sys in sector.systems.iter_mut() {
        let local = max_tau_at.get(&sys.id).copied().unwrap_or(0.0);
        sys.archetype.tau_sphere = match local {
            s if s >= 70.0 => TauSphereBand::Core,
            s if s >= 45.0 => TauSphereBand::Client,
            s if s >= 20.0 => TauSphereBand::Fringe,
            s if s > 0.0 => TauSphereBand::Contact,
            _ => TauSphereBand::None,
        };
    }
}

// ── §16.10 Aeldari intermittent ───────────────────────────────────────────────

fn apply_aeldari_intermittent(sector: &mut GeneratedSector, kinds: &BTreeMap<FactionId, String>) {
    for sys in sector.systems.iter_mut() {
        let mut max_covert: f32 = 0.0;
        for w in &sys.worlds {
            for p in &w.factions {
                let kind = kinds.get(&p.faction_id).map(|s| s.as_str()).unwrap_or("");
                if matches!(kind, "aeldari" | "harlequin" | "drukhari") {
                    max_covert = max_covert.max(p.dimensions.covert);
                }
            }
        }
        sys.archetype.aeldari_activity = max_covert.round().clamp(0.0, 100.0) as u8;
    }
}

// ── §16.12 Chaos corruption + metaphysical layer ──────────────────────────────

fn apply_chaos_corruption(sector: &mut GeneratedSector, kinds: &BTreeMap<FactionId, String>) {
    for sys in sector.systems.iter_mut() {
        let mut traitor_military: f32 = 0.0;
        let mut daemonic_presence: f32 = 0.0;
        let mut warp_phen_count: u32 = 0;
        for w in &sys.worlds {
            if w.tags.iter().any(|t| t.contains("warp_phenomena"))
                || w.tags.iter().any(|t| t.contains("daemonic_corruption"))
            {
                warp_phen_count += 1;
            }
            for p in &w.factions {
                let kind = kinds.get(&p.faction_id).map(|s| s.as_str()).unwrap_or("");
                match kind {
                    "chaos_space_marine"
                    | "chaos_knight"
                    | "traitor_guard"
                    | "traitor_titan_legion"
                    | "dark_mechanicum" => {
                        traitor_military += p.dimensions.military;
                    }
                    "daemon" => {
                        daemonic_presence += p.dimensions.ideological;
                    }
                    "cult" => {
                        traitor_military += p.dimensions.ideological * 0.5;
                    }
                    _ => {}
                }
            }
        }
        let corruption =
            (traitor_military / 4.0 + daemonic_presence / 2.0 + warp_phen_count as f32 * 8.0)
                .clamp(0.0, 100.0);
        let manifestation = (daemonic_presence + warp_phen_count as f32 * 12.0).clamp(0.0, 100.0);
        sys.archetype.chaos_corruption = corruption.round() as u8;
        sys.archetype.daemon_manifestation = manifestation.round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, FactionInfluence, GeneratedFaction, GeneratedStar, GeneratedSystem,
        GeneratedWorld, GenerationManifest, HexCoord, PowerProfile, PresenceDimensions,
        SystemControlSummary, WorldControlSummary, WorldDto, WorldFactionPresence,
    };

    fn world_with(
        world_type: &str,
        presences: Vec<(&str, PresenceDimensions)>,
        pop: &str,
    ) -> GeneratedWorld {
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
                population: pop.into(),
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

    fn sector_of(worlds: Vec<GeneratedWorld>, factions: Vec<(&str, &str)>) -> GeneratedSector {
        let system = GeneratedSystem {
            id: "sys-0001".into(),
            index: 1,
            name: "T".into(),
            coord: HexCoord { q: 0, r: 0 },
            star: GeneratedStar {
                colour_code: "A".into(),
                colour_name: "A".into(),
                spectral_type: None,
                source_row_index: None,
            },
            worlds,
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
        };
        GeneratedSector {
            id: "t".into(),
            title: "t".into(),
            seed: "t".into(),
            generator_name: "t".into(),
            generator_version: "t".into(),
            width: 10,
            height: 10,
            systems: vec![system],
            routes: vec![],
            factions: factions
                .into_iter()
                .map(|(id, kind)| GeneratedFaction {
                    id: id.into(),
                    name: id.into(),
                    kind: kind.into(),
                    disposition: "lawful".into(),
                    subfactions: Vec::new(),
                    system_presence: vec!["sys-0001".into()],
                    world_presence: vec![],
                    power: PowerProfile::default(),
                })
                .collect(),
            manifest: GenerationManifest {
                project_id: "t".into(),
                generated_at_policy: "t".into(),
                generator_name: "t".into(),
                generator_version: "t".into(),
                seed: "t".into(),
                seed_hash: "t".into(),
                profile: None,
                input_digests: BTreeMap::new(),
                settings_digest: "t".into(),
                system_count: 0,
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new(),
            economy: Default::default(),
        }
    }

    #[test]
    fn dormant_necron_with_low_visibility() {
        let w = world_with(
            "TombWorld",
            vec![(
                "tomb",
                PresenceDimensions {
                    admin: 5.0,
                    visibility: 10.0,
                    ..Default::default()
                },
            )],
            "Uninhabited",
        );
        let mut s = sector_of(vec![w], vec![("tomb", "necron")]);
        apply_all(&mut s);
        assert_eq!(s.systems[0].archetype.necron_phase, NecronPhase::Dormant);
    }

    #[test]
    fn imperial_stack_detects_two_co_sovereigns() {
        let w = world_with(
            "ForgeWorld",
            vec![
                (
                    "imp",
                    PresenceDimensions {
                        admin: 60.0,
                        legitimacy: 60.0,
                        visibility: 90.0,
                        ..Default::default()
                    },
                ),
                (
                    "mech",
                    PresenceDimensions {
                        admin: 40.0,
                        industrial: 70.0,
                        visibility: 90.0,
                        ..Default::default()
                    },
                ),
            ],
            "DenselyPopulated",
        );
        let mut s = sector_of(vec![w], vec![("imp", "imperial"), ("mech", "mechanicus")]);
        apply_all(&mut s);
        assert!(s.systems[0].archetype.imperial_co_sovereigns.len() >= 2);
    }
}
