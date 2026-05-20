//! §3 NEW2.md: Mission / quest / rumour seed generator.
//!
//! Pure derivation over a finished [`GeneratedSector`]: scans contested
//! worlds, hidden masters, perilous routes, blockades, claim mismatches, and
//! per-world archetype state, and emits structured one-line mission prompts
//! suitable as session hooks. Different from [`crate::hooks`]: hooks describe
//! sector *flashpoints* (drama that is happening), missions describe
//! *commissions* (jobs that could be undertaken). Missions therefore include
//! patron, target, primary/secondary locations, public objective and a hidden
//! complication.
//!
//! Determinism: same sector ⇒ same mission list. Stage RNG is keyed off
//! `blake3("sectorforge:{seed}:missions:{anchor_id}")` so different anchors
//! draw from independent streams.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use camino::Utf8Path;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;
use crate::rng::stage_rng;
use crate::sector_model::{
    ClaimType, GeneratedRoute, GeneratedSector, GeneratedSystem, GeneratedWorld, RouteStability,
};

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MissionsConfig {
    /// Maximum missions emitted per anchor (world / route / system).
    #[serde(default = "default_per_anchor")]
    pub max_per_anchor: u32,
    /// Top-N digest size in the rendered Markdown.
    #[serde(default = "default_top")]
    pub top_n_digest: u32,
    /// When true, hide missions whose hidden_complication depends on a
    /// Hidden-tier presence (player edition).
    #[serde(default)]
    pub player_edition: bool,
}

impl Default for MissionsConfig {
    fn default() -> Self {
        Self {
            max_per_anchor: default_per_anchor(),
            top_n_digest: default_top(),
            player_edition: false,
        }
    }
}

fn default_per_anchor() -> u32 {
    2
}
fn default_top() -> u32 {
    15
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MissionsReport {
    pub sector_id: String,
    pub seed: String,
    pub missions: Vec<MissionSeed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionSeed {
    pub id: String,
    pub kind: MissionKind,
    pub title: String,
    pub patron: Option<String>,
    pub target: Option<String>,
    pub primary_location: String,
    pub secondary_location: Option<String>,
    pub route_ids: Vec<String>,
    pub public_objective: String,
    pub hidden_complication: Option<String>,
    pub reward: String,
    pub if_ignored: String,
    pub scale: MissionScale,
    pub visibility: MissionVisibility,
    pub weight: u32,
    /// True when the mission's hidden complication leaks Hidden-tier info.
    pub gm_only: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionKind {
    Investigate,
    Escort,
    Sabotage,
    Diplomacy,
    Assassination,
    Recovery,
    Defense,
    Exploration,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionScale {
    OneShot,
    ShortArc,
    CampaignArc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionVisibility {
    Public,
    Restricted,
    Secret,
    Misinformation,
}

// ── Entry point ────────────────────────────────────────────────────────────────

#[must_use]
pub fn derive(sector: &GeneratedSector) -> MissionsReport {
    derive_with(sector, &MissionsConfig::default())
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &MissionsConfig) -> MissionsReport {
    let mut out: Vec<MissionSeed> = Vec::new();
    let by_sys: BTreeMap<&str, &GeneratedSystem> =
        sector.systems.iter().map(|s| (s.id.as_str(), s)).collect();

    for sys in &sector.systems {
        for w in &sys.worlds {
            emit_world_missions(sector, sys, w, &mut out);
        }
        emit_system_missions(sector, sys, &mut out);
    }
    for r in &sector.routes {
        emit_route_missions(sector, r, &by_sys, &mut out);
    }

    cap_per_anchor(&mut out, cfg.max_per_anchor);
    out.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.id.cmp(&b.id)));
    if cfg.player_edition {
        out.retain(|m| !m.gm_only);
    }
    MissionsReport {
        sector_id: sector.id.clone(),
        seed: sector.seed.clone(),
        missions: out,
    }
}

fn anchor_key(m: &MissionSeed) -> String {
    let route_part = m
        .route_ids
        .first()
        .cloned()
        .unwrap_or_else(|| "_".to_string());
    let sec = m.secondary_location.clone().unwrap_or_default();
    format!("{}:{}:{}:{:?}", m.primary_location, sec, route_part, m.kind)
}

fn cap_per_anchor(missions: &mut Vec<MissionSeed>, cap: u32) {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    missions.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.id.cmp(&b.id)));
    missions.retain(|m| {
        let entry = counts.entry(anchor_key(m)).or_insert(0);
        if *entry < cap {
            *entry += 1;
            true
        } else {
            false
        }
    });
}

// ── World-level missions ───────────────────────────────────────────────────────

fn emit_world_missions(
    sector: &GeneratedSector,
    sys: &GeneratedSystem,
    w: &GeneratedWorld,
    out: &mut Vec<MissionSeed>,
) {
    let primary = format!("{}/{}", sys.id, w.id);
    let mut rng = stage_rng(&sector.seed, "missions", &primary);

    // Investigate hidden master.
    if let Some(hidden) = &w.control.hidden_master {
        let patron = w
            .control
            .sovereign
            .clone()
            .or_else(|| sys.primary_factions.first().cloned());
        out.push(MissionSeed {
            id: format!("mission-{}-{}-investigate", sys.id, w.id),
            kind: MissionKind::Investigate,
            title: format!("Whispers Beneath {}", w.name),
            patron: patron.clone(),
            target: Some(hidden.clone()),
            primary_location: primary.clone(),
            secondary_location: None,
            route_ids: vec![],
            public_objective: format!(
                "Map the unknown cell of `{hidden}` operating in {}.",
                w.name
            ),
            hidden_complication: Some(format!(
                "{} has compromised at least one local official close to the patron.",
                hidden
            )),
            reward: rand_choice(
                &[
                    "Inquisitorial favour",
                    "Sealed warrant",
                    "Bounty of fifty thousand thrones",
                ],
                &mut rng,
            )
            .to_string(),
            if_ignored: "The cell consolidates and the world tips toward Hidden control.".into(),
            scale: MissionScale::ShortArc,
            visibility: MissionVisibility::Restricted,
            weight: 75 + dramatic_bonus(w),
            gm_only: true,
        });
    }

    // Sabotage / Recovery on a forge or hive world with mismatched claims.
    let occupier = w
        .claims
        .iter()
        .find(|c| c.claim_type == ClaimType::MilitaryOccupation);
    let legitimate = w.claims.iter().find(|c| {
        matches!(
            c.claim_type,
            ClaimType::ImperialMandate | ClaimType::LegalSovereignty | ClaimType::DynasticRight
        )
    });
    if let (Some(occ), Some(legit)) = (occupier, legitimate) {
        if occ.faction_id != legit.faction_id {
            out.push(MissionSeed {
                id: format!("mission-{}-{}-sabotage", sys.id, w.id),
                kind: MissionKind::Sabotage,
                title: format!("Strike at {}'s Hold", occ.faction_id),
                patron: Some(legit.faction_id.clone()),
                target: Some(occ.faction_id.clone()),
                primary_location: primary.clone(),
                secondary_location: None,
                route_ids: vec![],
                public_objective: format!(
                    "Cripple {} infrastructure supporting the occupation of {}.",
                    occ.faction_id, w.name
                ),
                hidden_complication: Some(
                    "Local civilians depend on the same infrastructure.".into(),
                ),
                reward: "Charter to the liberating force".into(),
                if_ignored: "Occupation hardens; rebellion shifts underground.".into(),
                scale: MissionScale::ShortArc,
                visibility: MissionVisibility::Restricted,
                weight: 70 + dramatic_bonus(w),
                gm_only: false,
            });
        }
    }

    // Defense on contested capital-tier world.
    if w.control.contested && w.control.control_score >= 35.0 {
        let defender = w
            .control
            .dominant
            .clone()
            .or_else(|| w.control.sovereign.clone());
        let attacker = w
            .factions
            .iter()
            .map(|fp| fp.faction_id.clone())
            .find(|id| defender.as_deref() != Some(id.as_str()));
        out.push(MissionSeed {
            id: format!("mission-{}-{}-defense", sys.id, w.id),
            kind: MissionKind::Defense,
            title: format!("Hold {}", w.name),
            patron: defender.clone(),
            target: attacker,
            primary_location: primary.clone(),
            secondary_location: None,
            route_ids: vec![],
            public_objective: format!("Repel the next assault on {}.", w.name),
            hidden_complication: Some(
                "The defender's stockpile is half what manifests claim.".into(),
            ),
            reward: "Land grant or warrant of trade".into(),
            if_ignored: "The world falls; control flips to the rival.".into(),
            scale: MissionScale::OneShot,
            visibility: MissionVisibility::Public,
            weight: 60 + dramatic_bonus(w),
            gm_only: false,
        });
    }

    // Diplomacy on a multi-claim world.
    if w.claims.len() >= 2 {
        let ids: Vec<String> = w.claims.iter().map(|c| c.faction_id.clone()).collect();
        let _ = sector;
        out.push(MissionSeed {
            id: format!("mission-{}-{}-diplomacy", sys.id, w.id),
            kind: MissionKind::Diplomacy,
            title: format!("Treaty of {}", w.name),
            patron: ids.first().cloned(),
            target: ids.get(1).cloned(),
            primary_location: primary.clone(),
            secondary_location: None,
            route_ids: vec![],
            public_objective: format!(
                "Broker an arrangement between {} on {}.",
                ids.join(" and "),
                w.name
            ),
            hidden_complication: Some("One party intends to sign and immediately renege.".into()),
            reward: "Standing with the brokered faction".into(),
            if_ignored: "The dispute escalates to open conflict.".into(),
            scale: MissionScale::OneShot,
            visibility: MissionVisibility::Restricted,
            weight: 55 + dramatic_bonus(w),
            gm_only: false,
        });
    }
}

// ── System-level missions ──────────────────────────────────────────────────────

fn emit_system_missions(
    sector: &GeneratedSector,
    sys: &GeneratedSystem,
    out: &mut Vec<MissionSeed>,
) {
    let mut rng = stage_rng(&sector.seed, "missions", &sys.id);

    // Assassination of the orbital controller when it differs from sovereign.
    if let (Some(ctrl), Some(sov)) = (&sys.control.orbital_controller, &sys.control.sovereign) {
        if ctrl != sov {
            let target_world = sys
                .worlds
                .first()
                .map(|w| format!("{}/{}", sys.id, w.id))
                .unwrap_or_else(|| sys.id.clone());
            out.push(MissionSeed {
                id: format!("mission-{}-assassination", sys.id),
                kind: MissionKind::Assassination,
                title: format!("Decapitate the {} Squadron", ctrl),
                patron: Some(sov.clone()),
                target: Some(ctrl.clone()),
                primary_location: target_world,
                secondary_location: None,
                route_ids: vec![],
                public_objective: format!("Remove the commander holding {} from above.", sys.name),
                hidden_complication: Some(
                    "The patron's pretender heir is the actual buyer of the contract.".into(),
                ),
                reward: rand_choice(
                    &[
                        "A patent of authority",
                        "Pre-paid void passage out of the sector",
                        "Naval rank by acclamation",
                    ],
                    &mut rng,
                )
                .to_string(),
                if_ignored: "Orbital dominance hardens into permanent occupation.".into(),
                scale: MissionScale::ShortArc,
                visibility: MissionVisibility::Secret,
                weight: 65,
                gm_only: false,
            });
        }
    }

    // Exploration for uncharted systems.
    if matches!(
        sys.control.state,
        Some(crate::sector_model::SystemState::Uncharted)
    ) {
        out.push(MissionSeed {
            id: format!("mission-{}-explore", sys.id),
            kind: MissionKind::Exploration,
            title: format!("Survey of {}", sys.name),
            patron: None,
            target: None,
            primary_location: sys.id.clone(),
            secondary_location: None,
            route_ids: vec![],
            public_objective: format!("Chart {} and verify approach corridors.", sys.name),
            hidden_complication: Some(
                "Astropathic traffic from this system has been silent for forty years.".into(),
            ),
            reward: "Survey bond, first-claim privileges".into(),
            if_ignored: "The system stays a blind spot — for friend and foe.".into(),
            scale: MissionScale::OneShot,
            visibility: MissionVisibility::Public,
            weight: 45,
            gm_only: false,
        });
    }
}

// ── Route-level missions ───────────────────────────────────────────────────────

fn emit_route_missions(
    sector: &GeneratedSector,
    r: &GeneratedRoute,
    sys_by_id: &BTreeMap<&str, &GeneratedSystem>,
    out: &mut Vec<MissionSeed>,
) {
    let mut rng = stage_rng(&sector.seed, "missions", &r.id);
    let (Some(from), Some(to)) = (
        sys_by_id.get(r.from_system_id.as_str()).copied(),
        sys_by_id.get(r.to_system_id.as_str()).copied(),
    ) else {
        return;
    };
    let from_world = from
        .worlds
        .first()
        .map(|w| format!("{}/{}", from.id, w.id))
        .unwrap_or_else(|| from.id.clone());
    let to_world = to
        .worlds
        .first()
        .map(|w| format!("{}/{}", to.id, w.id))
        .unwrap_or_else(|| to.id.clone());

    // Escort: any non-perilous route between two inhabited worlds.
    if !matches!(r.stability, RouteStability::Perilous)
        && !from.worlds.is_empty()
        && !to.worlds.is_empty()
    {
        out.push(MissionSeed {
            id: format!("mission-{}-escort", r.id),
            kind: MissionKind::Escort,
            title: format!("Tithe-Convoy: {} → {}", from.name, to.name),
            patron: sector
                .factions
                .iter()
                .find(|f| {
                    f.kind.eq_ignore_ascii_case("imperial")
                        || f.kind.eq_ignore_ascii_case("administratum")
                })
                .map(|f| f.id.clone()),
            target: None,
            primary_location: from_world.clone(),
            secondary_location: Some(to_world.clone()),
            route_ids: vec![r.id.clone()],
            public_objective: format!(
                "Escort sealed cargo from {} to {} via route `{}`.",
                from.name, to.name, r.id
            ),
            hidden_complication: Some(
                rand_choice(
                    &[
                        "One container carries a denied prisoner.",
                        "A duplicate ledger lists half the cargo as already delivered.",
                        "An astropathic relay aboard is broadcasting under coercion.",
                    ],
                    &mut rng,
                )
                .to_string(),
            ),
            reward: "Convoy commission, signed by the receiving authority".into(),
            if_ignored: "The destination world's stockpile dips one band; rebellion ticks up."
                .into(),
            scale: MissionScale::OneShot,
            visibility: MissionVisibility::Public,
            weight: 40,
            gm_only: false,
        });
    }

    // Recovery on perilous / hidden routes.
    if matches!(
        r.stability,
        RouteStability::Hazardous | RouteStability::Perilous
    ) || r.route_type.is_hidden()
    {
        out.push(MissionSeed {
            id: format!("mission-{}-recovery", r.id),
            kind: MissionKind::Recovery,
            title: format!("Salvage: {} — {}", from.name, to.name),
            patron: None,
            target: None,
            primary_location: from_world,
            secondary_location: Some(to_world),
            route_ids: vec![r.id.clone()],
            public_objective: format!(
                "Locate the missing freight ship last logged on route `{}`.",
                r.id
            ),
            hidden_complication: Some(
                "The wreck holds an item the patron's rivals would kill for.".into(),
            ),
            reward: "Salvage rights, plus a finder's cut of the cargo".into(),
            if_ignored: "The wreck — and what it carried — falls to whoever finds it first.".into(),
            scale: MissionScale::ShortArc,
            visibility: MissionVisibility::Secret,
            weight: 50,
            gm_only: r.route_type.is_hidden(),
        });
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn dramatic_bonus(w: &GeneratedWorld) -> u32 {
    let mut b = 0u32;
    if w.control.contested {
        b += 8;
    }
    b += (w.claims.len() as u32).min(4) * 3;
    if w.control.hidden_master.is_some() {
        b += 5;
    }
    b
}

fn rand_choice<'a>(xs: &'a [&'a str], rng: &mut impl Rng) -> &'a str {
    xs.choose(rng).copied().unwrap_or("(reward unknown)")
}

// ── Markdown rendering ─────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(report: &MissionsReport, cfg: &MissionsConfig) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Mission Seeds — {}", report.sector_id);
    let _ = writeln!(
        s,
        "\nSeed: `{}`  ·  Mode: {}\nTotal: **{}**",
        report.seed,
        if cfg.player_edition { "player" } else { "GM" },
        report.missions.len()
    );
    let top = cfg.top_n_digest.min(report.missions.len() as u32) as usize;
    if top > 0 {
        let _ = writeln!(s, "\n## Top {top}");
        for m in report.missions.iter().take(top) {
            let _ = writeln!(s, "\n### {:?} — {}", m.kind, m.title);
            let _ = writeln!(s, "- Location: `{}`", m.primary_location);
            if let Some(sec) = &m.secondary_location {
                let _ = writeln!(s, "- Secondary: `{sec}`");
            }
            if let Some(p) = &m.patron {
                let _ = writeln!(s, "- Patron: `{p}`");
            }
            if let Some(t) = &m.target {
                let _ = writeln!(s, "- Target: `{t}`");
            }
            if !m.route_ids.is_empty() {
                let _ = writeln!(s, "- Routes: {}", m.route_ids.join(", "));
            }
            let _ = writeln!(s, "- Objective: {}", m.public_objective);
            if !cfg.player_edition {
                if let Some(c) = &m.hidden_complication {
                    let _ = writeln!(s, "- _Hidden:_ {c}");
                }
            }
            let _ = writeln!(s, "- Reward: {}", m.reward);
            let _ = writeln!(s, "- If ignored: {}", m.if_ignored);
            let _ = writeln!(
                s,
                "- Scale / visibility: {:?} / {:?}",
                m.scale, m.visibility
            );
        }
    }
    s
}

/// Write `missions.md` + `missions.json` into `output_dir`.
///
/// # Errors
///
/// [`SectorError::Io`] on write failure and [`SectorError::ExportFailed`] on
/// serialisation failure.
pub fn write_report(
    output_dir: &Utf8Path,
    report: &MissionsReport,
    cfg: &MissionsConfig,
) -> Result<(), SectorError> {
    crate::export::write_md_and_json(
        output_dir,
        "missions",
        &render_markdown(report, cfg),
        report,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, FactionClaim, FactionInfluence, GeneratedFaction, GeneratedStar,
        GeneratedSystem, GeneratedWorld, GenerationManifest, HexCoord, PresenceDimensions,
        SystemControlSummary, WorldControlSummary, WorldDto, WorldFactionPresence,
    };
    use std::collections::BTreeMap as Map;

    fn empty() -> GeneratedSector {
        GeneratedSector {
            id: "t".into(),
            title: "t".into(),
            seed: "s".into(),
            generator_name: "sf".into(),
            generator_version: "0".into(),
            width: 4,
            height: 4,
            systems: vec![],
            routes: vec![],
            factions: vec![],
            manifest: GenerationManifest {
                project_id: "t".into(),
                generated_at_policy: "n".into(),
                generator_name: "sf".into(),
                generator_version: "0".into(),
                seed: "s".into(),
                seed_hash: "h".into(),
                profile: None,
                input_digests: Map::new(),
                settings_digest: "d".into(),
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
    fn deterministic_missions_empty_sector() {
        let s = empty();
        let a = derive(&s);
        let b = derive(&s);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn hidden_master_yields_investigate_mission() {
        let mut s = empty();
        s.factions.push(GeneratedFaction {
            id: "cult".into(),
            name: "Cult".into(),
            kind: "chaos".into(),
            disposition: "outlaw".into(),
            system_presence: vec![],
            world_presence: vec![],
            power: Default::default(),
        });
        s.systems.push(GeneratedSystem {
            id: "sys-0001".into(),
            index: 1,
            name: "Test".into(),
            coord: HexCoord { q: 0, r: 0 },
            star: GeneratedStar {
                colour_code: "G".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            },
            worlds: vec![GeneratedWorld {
                id: "sys-0001-w1".into(),
                index: 1,
                name: "Karthax".into(),
                orbit: 1,
                source_row_index: 0,
                world: WorldDto {
                    star_colour: "Y".into(),
                    star_colour_code: "Y".into(),
                    world_type: "HiveWorld".into(),
                    atmosphere: "Breathable".into(),
                    temperature: "Temperate".into(),
                    biosphere: "Standard".into(),
                    population: "Massive".into(),
                    tech_level: "Imperial".into(),
                    government: "ImperialCommander".into(),
                    notable_features: vec![],
                },
                factions: vec![WorldFactionPresence {
                    faction_id: "cult".into(),
                    influence: FactionInfluence::Hidden,
                    relationship_to_government: "outlaw".into(),
                    dimensions: PresenceDimensions::default(),
                    dominance: DominanceState::default(),
                    intel_confidence: 5,
                }],
                tags: vec![],
                notes: vec![],
                claims: vec![FactionClaim {
                    faction_id: "imp".into(),
                    claim_type: ClaimType::LegalSovereignty,
                    strength: 80,
                }],
                control: WorldControlSummary {
                    hidden_master: Some("cult".into()),
                    ..Default::default()
                },
                stability: Default::default(),
                regions: vec![],
                conflict: Default::default(),
            }],
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
        });

        let r = derive(&s);
        assert!(r
            .missions
            .iter()
            .any(|m| m.kind == MissionKind::Investigate));
        // Player edition hides GM-only.
        let r2 = derive_with(
            &s,
            &MissionsConfig {
                player_edition: true,
                ..Default::default()
            },
        );
        assert!(r2.missions.iter().all(|m| !m.gm_only));
    }
}
