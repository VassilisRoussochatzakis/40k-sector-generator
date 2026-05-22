//! Faction presence → control → claim → power derivation.
//!
//! Implements the model from `faction_sector_control_and_power_design.md`:
//! per-presence multi-dimensional scores, per-world claims, multi-winner
//! summaries, system-level state classification, and per-faction power
//! aggregation. All functions are pure and deterministic given their inputs.

use std::collections::BTreeMap;

use crate::factions::FactionDef;
use crate::ids::FactionId;
use crate::sector_model::{
    ClaimType, FactionClaim, FactionInfluence, GeneratedFaction, GeneratedSystem, GeneratedWorld,
    PowerProfile, PresenceDimensions, ScoredFaction, SystemControlSummary, SystemState,
    WorldControlSummary,
};

/// Derive multi-dimensional presence scores for a single (kind, disposition,
/// influence, world) combination. All output components are clamped to 0..=100.
#[must_use]
pub fn presence_dimensions(
    kind: &str,
    disposition: &str,
    influence: FactionInfluence,
    faction_def: Option<&FactionDef>,
    world: &GeneratedWorld,
) -> PresenceDimensions {
    let mut base = kind_profile(kind);
    apply_disposition(&mut base, disposition);
    let scale = influence_scale(influence);
    scale_dimensions(&mut base, scale);

    // Hidden cells suppress visibility further regardless of base.
    if matches!(influence, FactionInfluence::Hidden) {
        base.visibility = (base.visibility * 0.25).min(20.0);
    }

    // World tag bonuses — preferred world type / government / feature hits.
    if let Some(def) = faction_def {
        if def
            .preferred_world_types
            .iter()
            .any(|s| s.as_str() == world.world.world_type.as_ref())
        {
            base.admin += 5.0;
            base.legitimacy += 5.0;
        }
        if def
            .preferred_governments
            .iter()
            .any(|s| s.as_str() == world.world.government.as_ref())
        {
            base.admin += 5.0;
            base.legitimacy += 5.0;
        }
        let hits = def
            .preferred_notable_features
            .iter()
            .filter(|s| {
                world
                    .world
                    .notable_features
                    .iter()
                    .any(|f| f.as_ref() == s.as_str())
            })
            .count() as f32;
        if hits > 0.0 {
            base.ideological += 4.0 * hits;
            base.covert += 2.0 * hits;
        }
    }

    // Population scales civil/economic; uninhabited worlds shouldn't claim
    // strong administration regardless of kind profile.
    let pop_factor = population_factor(&world.world.population);
    base.admin *= pop_factor;
    base.economic *= pop_factor;
    base.industrial *= pop_factor;
    base.legitimacy *= pop_factor;

    clamp_dimensions(&mut base);
    base
}

/// Spec §4.5 — relative strength per influence bucket.
fn influence_scale(i: FactionInfluence) -> f32 {
    match i {
        FactionInfluence::Dominant => 1.0,
        FactionInfluence::Significant => 0.65,
        FactionInfluence::Minor => 0.35,
        FactionInfluence::Hidden => 0.18,
    }
}

fn scale_dimensions(d: &mut PresenceDimensions, k: f32) {
    d.admin *= k;
    d.military *= k;
    d.orbital *= k;
    d.economic *= k;
    d.industrial *= k;
    d.ideological *= k;
    d.covert *= k;
    d.logistics *= k;
    d.legitimacy *= k;
    d.visibility *= k.max(0.3);
}

fn clamp_dimensions(d: &mut PresenceDimensions) {
    let c = |x: &mut f32| *x = x.clamp(0.0, 100.0);
    c(&mut d.admin);
    c(&mut d.military);
    c(&mut d.orbital);
    c(&mut d.economic);
    c(&mut d.industrial);
    c(&mut d.ideological);
    c(&mut d.covert);
    c(&mut d.logistics);
    c(&mut d.legitimacy);
    c(&mut d.visibility);
}

fn add_dimensions(a: &mut PresenceDimensions, b: PresenceDimensions) {
    a.admin += b.admin;
    a.military += b.military;
    a.orbital += b.orbital;
    a.economic += b.economic;
    a.industrial += b.industrial;
    a.ideological += b.ideological;
    a.covert += b.covert;
    a.logistics += b.logistics;
    a.legitimacy += b.legitimacy;
    a.visibility += b.visibility;
}

fn apply_disposition(d: &mut PresenceDimensions, disposition: &str) {
    match disposition {
        "lawful" => {
            d.legitimacy += 12.0;
            d.admin += 6.0;
            d.covert -= 6.0;
        }
        "hostile" => {
            d.military += 10.0;
            d.legitimacy -= 18.0;
            d.visibility += 4.0;
        }
        "secretive" => {
            d.covert += 18.0;
            d.visibility -= 25.0;
        }
        "zealous" => {
            d.ideological += 18.0;
            d.legitimacy += 4.0;
        }
        "insular" => {
            d.admin += 5.0;
            d.logistics -= 6.0;
            d.visibility -= 8.0;
        }
        "opportunistic" => {
            d.economic += 10.0;
            d.logistics += 6.0;
        }
        _ => {}
    }
}

fn population_factor(pop: &str) -> f32 {
    match pop {
        "Uninhabited" => 0.10,
        "Minimal" => 0.45,
        "SoleSettlement" => 0.65,
        "LightlyPopulated" => 0.85,
        "DenselyPopulated" => 1.00,
        "ExtremelyDense" => 1.05,
        _ => 0.80,
    }
}

/// Spec §2 / archetype tables — base dimension profile per faction kind. Values
/// are pre-scaling references; influence + disposition + world bonuses apply on
/// top of these.
fn kind_profile(kind: &str) -> PresenceDimensions {
    let p = |admin,
             military,
             orbital,
             economic,
             industrial,
             ideological,
             covert,
             logistics,
             legitimacy,
             visibility| {
        PresenceDimensions {
            admin,
            military,
            orbital,
            economic,
            industrial,
            ideological,
            covert,
            logistics,
            legitimacy,
            visibility,
        }
    };
    match kind {
        // Imperial civil authority.
        "imperial" => p(80.0, 30.0, 20.0, 50.0, 35.0, 45.0, 15.0, 50.0, 75.0, 90.0),
        "adepta_sororitas" => p(30.0, 65.0, 25.0, 25.0, 10.0, 90.0, 25.0, 35.0, 70.0, 80.0),
        "inquisition" => p(20.0, 35.0, 15.0, 10.0, 10.0, 30.0, 90.0, 25.0, 60.0, 20.0),
        // Imperial military / elite.
        "adeptus_astartes" => p(15.0, 90.0, 70.0, 10.0, 30.0, 40.0, 30.0, 55.0, 70.0, 75.0),
        "imperial_guard" => p(30.0, 80.0, 25.0, 20.0, 55.0, 35.0, 15.0, 60.0, 55.0, 90.0),
        "imperial_knight" => p(25.0, 75.0, 15.0, 25.0, 30.0, 50.0, 15.0, 35.0, 65.0, 75.0),
        "collegia_titanica" => p(25.0, 85.0, 30.0, 25.0, 65.0, 40.0, 15.0, 55.0, 65.0, 75.0),
        "deathwatch" | "grey_knights" | "talons_of_the_emperor" => {
            p(10.0, 80.0, 60.0, 5.0, 15.0, 25.0, 80.0, 40.0, 50.0, 25.0)
        }
        // Mechanicus.
        "mechanicus" => p(50.0, 35.0, 50.0, 70.0, 95.0, 50.0, 25.0, 65.0, 60.0, 80.0),
        "dark_mechanicum" => p(25.0, 50.0, 45.0, 55.0, 85.0, 40.0, 65.0, 50.0, 5.0, 40.0),
        // Chaos / traitor / warp.
        "chaos_space_marine" => p(5.0, 85.0, 60.0, 10.0, 25.0, 60.0, 50.0, 45.0, 5.0, 60.0),
        "chaos_knight" => p(5.0, 75.0, 15.0, 10.0, 25.0, 45.0, 35.0, 25.0, 5.0, 50.0),
        "traitor_guard" => p(15.0, 70.0, 15.0, 15.0, 40.0, 40.0, 40.0, 45.0, 5.0, 65.0),
        "traitor_titan_legion" => p(10.0, 90.0, 30.0, 15.0, 55.0, 40.0, 25.0, 45.0, 5.0, 60.0),
        "daemon" => p(0.0, 75.0, 30.0, 5.0, 0.0, 80.0, 70.0, 10.0, 0.0, 50.0),
        "cult" => p(5.0, 25.0, 5.0, 15.0, 10.0, 75.0, 80.0, 15.0, 5.0, 20.0),
        // Xenos polities.
        "tau" => p(55.0, 55.0, 50.0, 55.0, 75.0, 60.0, 35.0, 55.0, 40.0, 70.0),
        "aeldari" => p(20.0, 60.0, 40.0, 25.0, 30.0, 50.0, 80.0, 45.0, 30.0, 25.0),
        "drukhari" => p(10.0, 70.0, 35.0, 25.0, 20.0, 25.0, 75.0, 30.0, 5.0, 30.0),
        "harlequin" => p(5.0, 50.0, 30.0, 10.0, 10.0, 60.0, 90.0, 30.0, 20.0, 10.0),
        "leagues_of_votann" => p(60.0, 55.0, 40.0, 70.0, 85.0, 35.0, 30.0, 60.0, 50.0, 70.0),
        // Hostile xenos.
        "ork" => p(5.0, 80.0, 30.0, 15.0, 35.0, 40.0, 20.0, 30.0, 10.0, 80.0),
        "tyranid" => p(0.0, 90.0, 60.0, 0.0, 0.0, 0.0, 30.0, 40.0, 0.0, 60.0),
        "necron" => p(30.0, 80.0, 50.0, 20.0, 50.0, 35.0, 55.0, 40.0, 25.0, 35.0),
        "minor_xenos" | "xenos" => p(25.0, 40.0, 20.0, 25.0, 20.0, 30.0, 35.0, 25.0, 25.0, 50.0),
        // Commercial / criminal.
        "merchant" => p(30.0, 15.0, 35.0, 85.0, 40.0, 20.0, 30.0, 75.0, 45.0, 70.0),
        "criminal" => p(5.0, 20.0, 20.0, 60.0, 30.0, 15.0, 80.0, 55.0, 5.0, 25.0),
        // Rebel insurgency.
        "rebel" => p(15.0, 50.0, 10.0, 25.0, 20.0, 55.0, 50.0, 25.0, 25.0, 55.0),
        "genestealer_cult" => p(10.0, 35.0, 5.0, 25.0, 25.0, 50.0, 90.0, 20.0, 5.0, 15.0),
        _ => p(25.0, 25.0, 15.0, 25.0, 25.0, 25.0, 25.0, 25.0, 25.0, 50.0),
    }
}

// ── Per-world claim derivation ─────────────────────────────────────────────────

/// Build the set of `FactionClaim`s implied by a world's presences (§3.3).
/// Each present faction contributes at most one claim; strongest-presence wins
/// when the same `ClaimType` would be produced more than once.
#[must_use]
pub fn derive_world_claims(world: &GeneratedWorld) -> Vec<FactionClaim> {
    let mut by_kind: BTreeMap<ClaimType, FactionClaim> = BTreeMap::new();
    for p in &world.factions {
        let strength = p.dimensions.local_control_score().round() as i32;
        if strength <= 0 {
            continue;
        }
        let claim_type = claim_for(p.faction_id.as_str(), p);
        let claim = FactionClaim {
            faction_id: p.faction_id.clone(),
            claim_type,
            strength: strength.clamp(0, 100) as u8,
        };
        by_kind
            .entry(claim_type)
            .and_modify(|existing| {
                if claim.strength > existing.strength {
                    *existing = claim.clone();
                }
            })
            .or_insert(claim);
    }
    let mut out: Vec<FactionClaim> = by_kind.into_values().collect();
    out.sort_by(|a, b| {
        b.strength
            .cmp(&a.strength)
            .then(a.faction_id.cmp(&b.faction_id))
    });
    out
}

fn claim_for(faction_id: &str, p: &crate::sector_model::WorldFactionPresence) -> ClaimType {
    // Disposition / id heuristics keyed off relationship_to_government, which
    // generation seeds from the faction's default disposition.
    let disposition = p.relationship_to_government.as_ref();
    let mut id = faction_id.to_string();
    if let Some(sub_id) = &p.subfaction_id {
        id.push(' ');
        id.push_str(sub_id.as_ref());
    }
    if let Some(force_id) = &p.force_id {
        id.push(' ');
        id.push_str(force_id.as_ref());
    }
    let id = id.as_str();
    if id.contains("inquisition") {
        return ClaimType::CovertWrit;
    }
    if id.contains("ecclesiarch") || id.contains("sororitas") || id.contains("shrine") {
        return ClaimType::ReligiousMandate;
    }
    if id.contains("knight") || id.contains("rogue_trader") || id.contains("dynasty") {
        return ClaimType::DynasticRight;
    }
    if id.contains("rebel") || id.contains("genestealer") || id.contains("cult") {
        return ClaimType::Rebellion;
    }
    if id.contains("necron") || id.contains("tomb") {
        return ClaimType::AncientDomain;
    }
    if id.contains("ork") || id.contains("drukhari") || id.contains("tyranid") {
        return ClaimType::HuntingGround;
    }
    if id.contains("merchant") || id.contains("trader") || id.contains("criminal") {
        return ClaimType::CommercialCharter;
    }
    if id.contains("traitor") || id.contains("chaos") || id.contains("daemon") {
        return ClaimType::MilitaryOccupation;
    }
    if id.starts_with("imperial") || id.contains("administratum") || id.contains("guard") {
        if disposition == "lawful" {
            return ClaimType::ImperialMandate;
        }
        return ClaimType::ImperialMandate;
    }
    match disposition {
        "lawful" => ClaimType::LegalSovereignty,
        "hostile" => ClaimType::MilitaryOccupation,
        "secretive" => ClaimType::CovertWrit,
        "zealous" => ClaimType::ReligiousMandate,
        "insular" => ClaimType::TreatyRight,
        "opportunistic" => ClaimType::CommercialCharter,
        _ => ClaimType::TreatyRight,
    }
}

// ── World multi-winner summary ─────────────────────────────────────────────────

/// Compute the multi-winner snapshot for a world (§5.3). Requires that every
/// presence already has its `dimensions` populated.
#[must_use]
pub fn derive_world_control(world: &GeneratedWorld) -> WorldControlSummary {
    if world.factions.is_empty() {
        return WorldControlSummary::default();
    }

    // Score each top-level faction once. Multiple sub-factions/forces from the
    // same faction can coexist on a world, so their dimensions roll up here.
    let mut aggregate: BTreeMap<FactionId, (PresenceDimensions, u8)> = BTreeMap::new();
    for p in &world.factions {
        let entry = aggregate.entry(p.faction_id.clone()).or_default();
        add_dimensions(&mut entry.0, p.dimensions);
        entry.1 = entry.1.max(p.intel_confidence);
    }
    let mut scored: Vec<(FactionId, f32, PresenceDimensions, u8)> = aggregate
        .into_iter()
        .map(|(id, (mut dimensions, confidence))| {
            clamp_dimensions(&mut dimensions);
            (id, dimensions.local_control_score(), dimensions, confidence)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let pick_dim = |f: fn(&PresenceDimensions) -> f32| -> Option<FactionId> {
        scored
            .iter()
            .max_by(|a, b| {
                f(&a.2)
                    .partial_cmp(&f(&b.2))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            })
            .filter(|x| f(&x.2) >= 15.0)
            .map(|x| x.0.clone())
    };

    let dominant = scored.first().map(|x| x.0.clone());
    let sovereign = pick_dim(|d| d.admin + d.legitimacy);
    let occupier = pick_dim(|d| d.military);
    let economic_hegemon = pick_dim(|d| d.economic);
    let popular_authority = pick_dim(|d| d.ideological);
    let hidden_master = scored
        .iter()
        .filter(|x| x.2.covert >= 30.0 && x.2.visibility <= 35.0)
        .max_by(|a, b| {
            a.2.covert
                .partial_cmp(&b.2.covert)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        })
        .map(|x| x.0.clone());

    let control_score = scored.first().map(|x| x.1).unwrap_or(0.0);
    let contested = match (scored.first(), scored.get(1)) {
        (Some(a), Some(b)) => a.1 - b.1 < 15.0 && a.1 >= 35.0,
        _ => false,
    };

    WorldControlSummary {
        dominant,
        sovereign,
        occupier,
        economic_hegemon,
        popular_authority,
        hidden_master,
        contested,
        control_score,
    }
}

// ── System-level summary ───────────────────────────────────────────────────────

/// Aggregate per-world control into a system snapshot (§6.4). The state field
/// is heuristic: it inspects dominant/contested distributions and world tags.
#[must_use]
pub fn derive_system_control(sys: &GeneratedSystem) -> SystemControlSummary {
    if sys.worlds.is_empty() {
        return SystemControlSummary {
            state: Some(SystemState::Uncharted),
            ..Default::default()
        };
    }

    let mut score_sum: BTreeMap<FactionId, f32> = BTreeMap::new();
    let mut admin_sum: BTreeMap<FactionId, f32> = BTreeMap::new();
    let mut orbital_sum: BTreeMap<FactionId, f32> = BTreeMap::new();
    let mut economic_sum: BTreeMap<FactionId, f32> = BTreeMap::new();
    let mut covert_sum: BTreeMap<FactionId, f32> = BTreeMap::new();
    let mut visibility_sum: BTreeMap<FactionId, f32> = BTreeMap::new();

    let mut populated_worlds = 0u32;
    let mut contested_worlds = 0u32;
    let mut warzone_signal = 0u32;
    let mut quarantined = false;

    for w in &sys.worlds {
        if w.world.population.as_ref() != "Uninhabited" {
            populated_worlds += 1;
        }
        for tag in &w.tags {
            let t = tag.as_ref();
            if t.ends_with(":quarantined") {
                quarantined = true;
            }
            if t.ends_with(":war_zone") || t.ends_with(":daemonic_corruption") {
                warzone_signal += 1;
            }
        }
        if w.control.contested {
            contested_worlds += 1;
        }
        for p in &w.factions {
            let id = p.faction_id.clone();
            *score_sum.entry(id.clone()).or_insert(0.0) += p.dimensions.local_control_score();
            *admin_sum.entry(id.clone()).or_insert(0.0) += p.dimensions.admin;
            *orbital_sum.entry(id.clone()).or_insert(0.0) += p.dimensions.orbital;
            *economic_sum.entry(id.clone()).or_insert(0.0) += p.dimensions.economic;
            *covert_sum.entry(id.clone()).or_insert(0.0) += p.dimensions.covert;
            *visibility_sum.entry(id).or_insert(0.0) += p.dimensions.visibility;
        }
    }

    let pick = |m: &BTreeMap<FactionId, f32>, threshold: f32| -> Option<FactionId> {
        m.iter()
            .filter(|(_, v)| **v >= threshold)
            .max_by(|a, b| {
                a.1.partial_cmp(b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(b.0.cmp(a.0))
            })
            .map(|(k, _)| k.clone())
    };

    let dominant = pick(&score_sum, 1.0);
    let sovereign = pick(&admin_sum, 5.0);
    let orbital_controller = pick(&orbital_sum, 5.0);
    let economic_hegemon = pick(&economic_sum, 5.0);
    let hidden_master = covert_sum
        .iter()
        .filter(|(id, cov)| {
            **cov >= 25.0 && visibility_sum.get(*id).copied().unwrap_or(0.0) < **cov * 0.7
        })
        .max_by(|a, b| {
            a.1.partial_cmp(b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.0.cmp(a.0))
        })
        .map(|(k, _)| k.clone());

    let mut top: Vec<ScoredFaction> = score_sum
        .into_iter()
        .map(|(faction_id, score)| ScoredFaction { faction_id, score })
        .collect();
    top.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.faction_id.cmp(&b.faction_id))
    });
    top.truncate(5);

    let state = classify_system_state(SystemStateParams {
        sys,
        populated_worlds,
        contested_worlds,
        warzone_signal,
        quarantined,
        dominant: &dominant,
        orbital_controller: &orbital_controller,
        hidden_master: &hidden_master,
    });

    SystemControlSummary {
        state: Some(state),
        dominant,
        sovereign,
        orbital_controller,
        economic_hegemon,
        hidden_master,
        top_factions: top,
    }
}

struct SystemStateParams<'a> {
    sys: &'a GeneratedSystem,
    populated_worlds: u32,
    contested_worlds: u32,
    warzone_signal: u32,
    quarantined: bool,
    dominant: &'a Option<FactionId>,
    orbital_controller: &'a Option<FactionId>,
    hidden_master: &'a Option<FactionId>,
}

fn classify_system_state(params: SystemStateParams) -> SystemState {
    let SystemStateParams {
        sys,
        populated_worlds,
        contested_worlds,
        warzone_signal,
        quarantined,
        dominant,
        orbital_controller,
        hidden_master,
    } = params;
    if quarantined {
        return SystemState::Quarantined;
    }
    if warzone_signal >= 2 || contested_worlds >= 2 {
        return SystemState::Warzone;
    }
    if populated_worlds == 0 {
        return SystemState::Uncharted;
    }
    if let (Some(d), Some(o)) = (dominant.as_deref(), orbital_controller.as_deref()) {
        if d != o {
            return SystemState::Blockaded;
        }
    }
    if hidden_master.is_some() && dominant.is_none() {
        return SystemState::Infiltrated;
    }
    if contested_worlds > 0
        || sys
            .worlds
            .iter()
            .filter(|w| w.control.dominant.is_some())
            .count()
            > 1
            && unique_dominant_count(sys) > 1
    {
        return SystemState::Fragmented;
    }
    SystemState::Pacified
}

fn unique_dominant_count(sys: &GeneratedSystem) -> usize {
    let mut set: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for w in &sys.worlds {
        if let Some(d) = &w.control.dominant {
            set.insert(d.as_ref());
        }
    }
    set.len()
}

// ── Faction power aggregation ──────────────────────────────────────────────────

/// Aggregate per-faction `PowerProfile`s across the sector. Each world
/// presence contributes its dimension scores weighted by strategic value
/// (population + tech).
#[must_use]
pub fn aggregate_faction_power(systems: &[GeneratedSystem]) -> BTreeMap<FactionId, PowerProfile> {
    let mut acc: BTreeMap<FactionId, PowerProfile> = BTreeMap::new();
    for sys in systems {
        for w in &sys.worlds {
            let sv = strategic_value(w);
            for p in &w.factions {
                let entry = acc.entry(p.faction_id.clone()).or_default();
                let d = &p.dimensions;
                entry.administrative += d.admin * sv;
                entry.military += d.military * sv;
                entry.naval += d.orbital * sv;
                entry.economic += d.economic * sv;
                entry.industrial += d.industrial * sv;
                entry.ideological += d.ideological * sv;
                entry.covert += d.covert * sv;
                entry.logistical += d.logistics * sv;
                entry.legitimacy += d.legitimacy * sv;
            }
        }
    }
    // Scale down so totals stay in a readable range (roughly 0..=several hundred).
    for v in acc.values_mut() {
        let s = 0.01;
        v.administrative *= s;
        v.military *= s;
        v.naval *= s;
        v.economic *= s;
        v.industrial *= s;
        v.ideological *= s;
        v.covert *= s;
        v.logistical *= s;
        v.legitimacy *= s;
    }
    acc
}

fn strategic_value(w: &GeneratedWorld) -> f32 {
    let pop: f32 = match w.world.population.as_ref() {
        "Uninhabited" => 0.0,
        "Minimal" => 1.0,
        "SoleSettlement" => 2.0,
        "LightlyPopulated" => 3.0,
        "DenselyPopulated" => 4.0,
        "ExtremelyDense" => 5.0,
        _ => 1.0,
    };
    let tech: f32 = match w.world.tech_level.as_ref() {
        "Primitive" => 0.0,
        "Low" => 1.0,
        "Standard" => 2.0,
        "High" => 3.0,
        "Archaeotech" | "XenoHybrid" => 4.0,
        _ => 1.0,
    };
    (1.0_f32 + pop + tech).max(0.5)
}

/// Apply per-faction `PowerProfile`s onto the sector's `GeneratedFaction`
/// rollups in place. Factions not mentioned are left at default.
pub fn apply_faction_power(
    factions: &mut [GeneratedFaction],
    power: &BTreeMap<FactionId, PowerProfile>,
) {
    for f in factions {
        if let Some(p) = power.get(&f.id) {
            f.power = *p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, FactionInfluence, GeneratedWorld, WorldDto, WorldFactionPresence,
    };

    fn empty_world() -> GeneratedWorld {
        GeneratedWorld {
            id: "sys-0001-w1".into(),
            index: 1,
            name: "Test".into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "yellow".into(),
                star_colour_code: "G".into(),
                world_type: "HiveWorld".into(),
                atmosphere: "Breathable".into(),
                temperature: "Temperate".into(),
                biosphere: "Thriving".into(),
                population: "DenselyPopulated".into(),
                tech_level: "Standard".into(),
                government: "Imperial".into(),
                notable_features: vec![],
            },
            factions: vec![],
            tags: vec![],
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: Default::default(),
            regions: Vec::new().into(),
            conflict: Default::default(),
        }
    }

    #[test]
    fn dominance_buckets_monotone() {
        assert_eq!(DominanceState::from_score(0.0), DominanceState::Rumored);
        assert_eq!(DominanceState::from_score(15.0), DominanceState::Presence);
        assert_eq!(DominanceState::from_score(35.0), DominanceState::Influence);
        assert_eq!(DominanceState::from_score(50.0), DominanceState::Contested);
        assert_eq!(DominanceState::from_score(70.0), DominanceState::Controlled);
        assert_eq!(DominanceState::from_score(95.0), DominanceState::Stronghold);
    }

    #[test]
    fn imperial_lawful_dominant_yields_high_admin() {
        let world = empty_world();
        let d = presence_dimensions(
            "imperial",
            "lawful",
            FactionInfluence::Dominant,
            None,
            &world,
        );
        assert!(d.admin > 60.0, "admin too low: {}", d.admin);
        assert!(d.legitimacy > 60.0, "legitimacy too low: {}", d.legitimacy);
    }

    #[test]
    fn inquisition_secretive_hidden_is_invisible_but_covert() {
        let world = empty_world();
        let d = presence_dimensions(
            "inquisition",
            "secretive",
            FactionInfluence::Hidden,
            None,
            &world,
        );
        assert!(d.covert > 10.0);
        assert!(d.visibility < 25.0, "visibility = {}", d.visibility);
    }

    #[test]
    fn dimensions_clamp_to_unit_range() {
        let mut world = empty_world();
        world.world.population = "ExtremelyDense".into();
        let d = presence_dimensions(
            "tyranid",
            "hostile",
            FactionInfluence::Dominant,
            None,
            &world,
        );
        let fields = [
            d.admin,
            d.military,
            d.orbital,
            d.economic,
            d.industrial,
            d.ideological,
            d.covert,
            d.logistics,
            d.legitimacy,
            d.visibility,
        ];
        for f in fields {
            assert!((0.0..=100.0).contains(&f), "{f} out of range");
        }
    }

    #[test]
    fn world_control_detects_contestation() {
        let mut world = empty_world();
        let mk = |id: &str, score: f32| WorldFactionPresence {
            faction_id: id.into(),
            subfaction_id: None,
            subfaction_name: None,
            force_id: None,
            force_name: None,
            influence: FactionInfluence::Significant,
            relationship_to_government: "lawful".into(),
            dimensions: PresenceDimensions {
                admin: score,
                military: score,
                orbital: score * 0.3,
                economic: score * 0.6,
                industrial: score * 0.5,
                ideological: score * 0.5,
                covert: 5.0,
                logistics: score * 0.4,
                legitimacy: score,
                visibility: 80.0,
            },
            dominance: DominanceState::default(),
            intel_confidence: 100,
        };
        world.factions.push(mk("a", 90.0));
        world.factions.push(mk("b", 85.0));
        let summary = derive_world_control(&world);
        assert!(summary.contested);
        assert_eq!(summary.dominant.as_deref(), Some("a"));
    }
}
