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

/// Tie-break helper: higher score wins; on equal scores, the lexicographically
/// smaller id wins. Used by `derive_world_control` and `derive_system_control`
/// for deterministic, direction-consistent picks.
fn score_then_id(
    a_score: f32,
    a_id: &FactionId,
    b_score: f32,
    b_id: &FactionId,
) -> std::cmp::Ordering {
    crate::analysis::cmp_f32_asc(a_score, b_score).then_with(|| b_id.cmp(a_id))
}

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
            .any(|s| s.as_str() == world.world.world_type.to_string().as_str())
        {
            base.admin += 5.0;
            base.legitimacy += 5.0;
        }
        if def
            .preferred_governments
            .iter()
            .any(|s| s.as_str() == world.world.government.to_string().as_str())
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

/// Mutable refs to every presence dimension, in declaration order — single
/// source of the field list for the per-field arithmetic helpers below (B5).
fn dimension_fields_mut(d: &mut PresenceDimensions) -> [&mut f32; 10] {
    [
        &mut d.admin,
        &mut d.military,
        &mut d.orbital,
        &mut d.economic,
        &mut d.industrial,
        &mut d.ideological,
        &mut d.covert,
        &mut d.logistics,
        &mut d.legitimacy,
        &mut d.visibility,
    ]
}

fn dimension_fields(d: &PresenceDimensions) -> [f32; 10] {
    [
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
    ]
}

fn scale_dimensions(d: &mut PresenceDimensions, k: f32) {
    // visibility floors its multiplier at 0.3 (a heavily-shrunk faction never
    // goes fully invisible); the other nine scale linearly by `k`.
    let scaled_visibility = d.visibility * k.max(0.3);
    for f in dimension_fields_mut(d) {
        *f *= k;
    }
    d.visibility = scaled_visibility;
}

fn clamp_dimensions(d: &mut PresenceDimensions) {
    for f in dimension_fields_mut(d) {
        *f = f.clamp(0.0, 100.0);
    }
}

fn add_dimensions(a: &mut PresenceDimensions, b: PresenceDimensions) {
    let bs = dimension_fields(&b);
    for (f, bf) in dimension_fields_mut(a).into_iter().zip(bs) {
        *f += bf;
    }
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

fn population_factor(pop: &crate::worlds::Population) -> f32 {
    use crate::worlds::Population;
    match pop {
        Population::Uninhabited => 0.10,
        Population::Minimal => 0.45,
        Population::SoleSettlement => 0.65,
        Population::LightlyPopulated => 0.85,
        Population::DenselyPopulated => 1.00,
        Population::ExtremelyDense => 1.05,
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
        crate::analysis::cmp_f32_desc(a.1, b.1).then_with(|| a.0.cmp(&b.0))
    });

    let pick_dim = |f: fn(&PresenceDimensions) -> f32| -> Option<FactionId> {
        scored
            .iter()
            .max_by(|a, b| score_then_id(f(&a.2), &a.0, f(&b.2), &b.0))
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
        .max_by(|a, b| score_then_id(a.2.covert, &a.0, b.2.covert, &b.0))
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
        if w.world.population != crate::worlds::Population::Uninhabited {
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
            .max_by(|a, b| score_then_id(*a.1, a.0, *b.1, b.0))
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
        .max_by(|a, b| score_then_id(*a.1, a.0, *b.1, b.0))
        .map(|(k, _)| k.clone());

    let mut top: Vec<ScoredFaction> = score_sum
        .into_iter()
        .map(|(faction_id, score)| ScoredFaction { faction_id, score })
        .collect();
    top.sort_by(|a, b| {
        crate::analysis::cmp_f32_desc(a.score, b.score)
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
    let pop: f32 = match w.world.population {
        crate::worlds::Population::Uninhabited => 0.0,
        crate::worlds::Population::Minimal => 1.0,
        crate::worlds::Population::SoleSettlement => 2.0,
        crate::worlds::Population::LightlyPopulated => 3.0,
        crate::worlds::Population::DenselyPopulated => 4.0,
        crate::worlds::Population::ExtremelyDense => 5.0,
    };
    let tech: f32 = match w.world.tech_level {
        crate::worlds::TechLevel::Primitive => 0.0,
        crate::worlds::TechLevel::Low => 1.0,
        crate::worlds::TechLevel::Standard => 2.0,
        crate::worlds::TechLevel::High => 3.0,
        crate::worlds::TechLevel::Archaeotech | crate::worlds::TechLevel::XenoHybrid => 4.0,
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
        ClaimType, DominanceState, FactionInfluence, GeneratedStar, GeneratedSystem, GeneratedWorld,
        HexCoord, SystemControlSummary, SystemKind, SystemState, WorldControlSummary, WorldDto,
        WorldFactionPresence,
    };
    use crate::worlds::Population;

    fn empty_world() -> GeneratedWorld {
        GeneratedWorld {
            id: "sys-0001-w1".into(),
            index: 1,
            name: "Test".into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: crate::worlds::StarColour::Yellow,
                world_type: crate::worlds::WorldType::HiveWorld,
                atmosphere: crate::worlds::Atmosphere::Breathable,
                temperature: crate::worlds::Temperature::Temperate,
                biosphere: crate::worlds::Biosphere::Thriving,
                population: crate::worlds::Population::DenselyPopulated,
                tech_level: crate::worlds::TechLevel::Standard,
                government: crate::worlds::Government::MilitaryGovernor,
                notable_features: vec![],
            },
            factions: vec![],
            tags: vec![],
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: Default::default(),
            regions: Vec::new(),
            conflict: Default::default(),
            intel: Default::default(),
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
        world.world.population = crate::worlds::Population::ExtremelyDense;
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

    // ── A-group shared helpers ─────────────────────────────────────────────────

    /// A presence with the given `faction_id`, `relationship_to_government`, and
    /// `dimensions`. Everything else is left at a benign default.
    fn presence(
        faction_id: &str,
        relationship: &str,
        dimensions: PresenceDimensions,
    ) -> WorldFactionPresence {
        WorldFactionPresence {
            faction_id: faction_id.into(),
            subfaction_id: None,
            subfaction_name: None,
            force_id: None,
            force_name: None,
            influence: FactionInfluence::Significant,
            relationship_to_government: relationship.into(),
            dimensions,
            dominance: DominanceState::default(),
            intel_confidence: 100,
        }
    }

    /// A presence whose only nonzero dimension is `admin`. Used to clear the
    /// `derive_world_claims` score gate (`admin = 80` ⇒ score 16 > 0) while
    /// pinning the `claim_for` result via the faction id / disposition.
    fn admin_presence(faction_id: &str, relationship: &str, admin: f32) -> WorldFactionPresence {
        presence(
            faction_id,
            relationship,
            PresenceDimensions {
                admin,
                ..Default::default()
            },
        )
    }

    /// A populated system holding `worlds`. Mirrors the economy module's `sys()`
    /// builder shape; control aggregation reads only `worlds`.
    fn system_with(id: &str, worlds: Vec<GeneratedWorld>) -> GeneratedSystem {
        GeneratedSystem {
            id: id.into(),
            index: 1,
            name: id.into(),
            kind: SystemKind::Star,
            coord: HexCoord { q: 0, r: 0 },
            star: Some(GeneratedStar {
                colour_code: "G".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            }),
            worlds,
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
        }
    }

    // ── A1: derive_world_claims — dedup / score gate / sort / clamp ─────────────

    #[test]
    fn derive_world_claims_skips_zero_score_presence() {
        let mut w = empty_world();
        // All dims zero ⇒ local_control_score 0.0 ⇒ round 0 ⇒ 0 <= 0 ⇒ skipped.
        w.factions
            .push(presence("ork", "hostile", PresenceDimensions::default()));
        assert!(derive_world_claims(&w).is_empty());
    }

    #[test]
    fn derive_world_claims_dedup_keeps_higher_strength() {
        let mut w = empty_world();
        // Two presences both mapping to HuntingGround (id contains "ork"); the
        // higher-strength one (admin 80 ⇒ score 16) is kept over admin 40 ⇒ 8.
        w.factions.push(admin_presence("ork", "hostile", 80.0));
        w.factions.push(admin_presence("ork", "hostile", 40.0));
        let claims = derive_world_claims(&w);
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].claim_type, ClaimType::HuntingGround);
        assert_eq!(claims[0].strength, 16);
    }

    #[test]
    fn derive_world_claims_sorts_by_strength_desc_then_id() {
        let mut w = empty_world();
        // merchant ⇒ CommercialCharter, admin 50 ⇒ score 10.
        // ork ⇒ HuntingGround, admin 80 ⇒ score 16.
        // Distinct ClaimTypes ⇒ two claims, ordered strength-desc: ork before
        // merchant, which is the OPPOSITE of id-ascending order.
        w.factions.push(admin_presence("merchant", "opportunistic", 50.0));
        w.factions.push(admin_presence("ork", "hostile", 80.0));
        let claims = derive_world_claims(&w);
        assert_eq!(claims.len(), 2);
        assert_eq!(claims[0].faction_id.as_str(), "ork");
        assert_eq!(claims[0].strength, 16);
        assert_eq!(claims[1].faction_id.as_str(), "merchant");
        assert_eq!(claims[1].strength, 10);
    }

    #[test]
    fn derive_world_claims_clamps_strength_at_100() {
        let mut w = empty_world();
        // Every dimension 100.0; the local-control weights sum to exactly 1.00,
        // so the score is 100.0 ⇒ round 100 ⇒ clamp(0,100) = 100.
        let all_100 = PresenceDimensions {
            admin: 100.0,
            military: 100.0,
            orbital: 100.0,
            economic: 100.0,
            industrial: 100.0,
            ideological: 100.0,
            covert: 100.0,
            logistics: 100.0,
            legitimacy: 100.0,
            visibility: 100.0,
        };
        w.factions.push(presence("ork", "hostile", all_100));
        let claims = derive_world_claims(&w);
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].strength, 100);
    }

    // ── A4: claim_for ladder (id-substring precedence + disposition fallback) ───

    #[test]
    fn claim_for_id_and_disposition_ladder() {
        // (faction_id, relationship_to_government) → expected ClaimType. Each
        // presence carries admin=80 so it clears the score gate.
        let cases: &[(&str, &str, ClaimType)] = &[
            // "inquisition" wins even though the id also contains "guard" and
            // begins like an imperial arm.
            ("imperial_inquisition_guard", "lawful", ClaimType::CovertWrit),
            ("inquisition", "lawful", ClaimType::CovertWrit),
            ("adepta_sororitas", "lawful", ClaimType::ReligiousMandate),
            // starts_with "imperial" fires before the lawful disposition fallback.
            ("imperial", "lawful", ClaimType::ImperialMandate),
            // No keyword ⇒ disposition fallback.
            ("freefolk", "lawful", ClaimType::LegalSovereignty),
            ("freefolk", "insular", ClaimType::TreatyRight),
        ];
        for (id, rel, expected) in cases {
            let mut w = empty_world();
            w.factions.push(admin_presence(id, rel, 80.0));
            let claims = derive_world_claims(&w);
            assert_eq!(claims.len(), 1, "id={id} rel={rel}");
            assert_eq!(claims[0].claim_type, *expected, "id={id} rel={rel}");
        }
    }

    // ── A3: score_then_id tie-break (lex-smaller id wins on equal score) ────────

    #[test]
    fn score_then_id_breaks_ties_toward_lex_smaller_id() {
        // Equal scores ⇒ ordering is `b_id.cmp(a_id)`. With a="aaa", b="zzz"
        // that is "zzz".cmp("aaa") = Greater, so in a `max_by` "aaa" wins.
        assert_eq!(
            score_then_id(60.0, &"aaa".into(), 60.0, &"zzz".into()),
            std::cmp::Ordering::Greater
        );
        // Higher first score dominates regardless of id (ascending compare).
        assert_eq!(
            score_then_id(70.0, &"zzz".into(), 60.0, &"aaa".into()),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn world_control_resolves_ties_to_lex_smaller_id_and_is_deterministic() {
        let mut w = empty_world();
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
        // Identical scores; ids differ only lexically.
        w.factions.push(mk("aaa", 60.0));
        w.factions.push(mk("zzz", 60.0));
        let s = derive_world_control(&w);
        assert_eq!(s.dominant.as_deref(), Some("aaa"));
        // admin+legitimacy = 120 ≥ 15, military = 60 ≥ 15 ⇒ both resolve via the
        // score_then_id max_by to the lex-smaller "aaa".
        assert_eq!(s.sovereign.as_deref(), Some("aaa"));
        assert_eq!(s.occupier.as_deref(), Some("aaa"));
        // Re-deriving the same world is byte-identical.
        let s2 = derive_world_control(&w);
        assert_eq!(format!("{s:?}"), format!("{s2:?}"));
    }

    // ── A5: aggregate_faction_power — strategic-value-weighted rollup ───────────

    /// Clone `empty_world()` and override population + tech so `strategic_value`
    /// is pinned, then attach `factions`.
    fn world_pop_tech(
        pop: Population,
        tech: crate::worlds::TechLevel,
        factions: Vec<WorldFactionPresence>,
    ) -> GeneratedWorld {
        let mut w = empty_world();
        w.world.population = pop;
        w.world.tech_level = tech;
        w.factions = factions;
        w
    }

    #[test]
    fn aggregate_faction_power_sums_presences_with_strategic_value() {
        use crate::worlds::TechLevel;
        // sv = 1 + pop(DenselyPopulated=4) + tech(Standard=2) = 7.0.
        // "a": two presences admin=60 ⇒ administrative = (60+60)*7*0.01 = 8.4.
        // "b": one presence admin=90 ⇒ administrative = 90*7*0.01 = 6.3.
        let w = world_pop_tech(
            Population::DenselyPopulated,
            TechLevel::Standard,
            vec![
                admin_presence("a", "lawful", 60.0),
                admin_presence("a", "lawful", 60.0),
                admin_presence("b", "lawful", 90.0),
            ],
        );
        let power = aggregate_faction_power(&[system_with("sys-0001", vec![w])]);
        let a = power.get("a").expect("faction a present");
        let b = power.get("b").expect("faction b present");
        assert!((a.administrative - 8.4).abs() < 1e-3, "a = {}", a.administrative);
        assert!((b.administrative - 6.3).abs() < 1e-3, "b = {}", b.administrative);
        assert!(a.administrative > b.administrative);
    }

    #[test]
    fn aggregate_faction_power_scales_by_strategic_value() {
        use crate::worlds::TechLevel;
        // Dense + Archaeotech ⇒ sv = 1 + 5 + 4 = 10 ⇒ admin 50 → 50*10*0.01 = 5.0.
        let rich = world_pop_tech(
            Population::ExtremelyDense,
            TechLevel::Archaeotech,
            vec![admin_presence("c", "lawful", 50.0)],
        );
        let power_rich = aggregate_faction_power(&[system_with("sys-rich", vec![rich])]);
        assert!(
            (power_rich.get("c").unwrap().administrative - 5.0).abs() < 1e-3,
            "rich = {}",
            power_rich.get("c").unwrap().administrative
        );
        // Minimal + Primitive ⇒ sv = 1 + 1 + 0 = 2 ⇒ admin 50 → 50*2*0.01 = 1.0.
        let poor = world_pop_tech(
            Population::Minimal,
            TechLevel::Primitive,
            vec![admin_presence("c", "lawful", 50.0)],
        );
        let power_poor = aggregate_faction_power(&[system_with("sys-poor", vec![poor])]);
        assert!(
            (power_poor.get("c").unwrap().administrative - 1.0).abs() < 1e-3,
            "poor = {}",
            power_poor.get("c").unwrap().administrative
        );
    }

    #[test]
    fn aggregate_faction_power_omits_factions_without_presence() {
        use crate::worlds::TechLevel;
        let w = world_pop_tech(
            Population::DenselyPopulated,
            TechLevel::Standard,
            vec![admin_presence("a", "lawful", 60.0)],
        );
        // The map keys solely off presences — a faction with none is absent even
        // if it exists in the catalogue elsewhere.
        let power = aggregate_faction_power(&[system_with("sys-0001", vec![w])]);
        assert!(power.contains_key("a"));
        assert!(!power.contains_key("ghost"));
    }

    // ── A2: classify_system_state — 7-way cascade ──────────────────────────────

    #[test]
    fn classify_system_state_empty_system_is_uncharted() {
        let sys = system_with("sys-0001", vec![]);
        assert_eq!(
            derive_system_control(&sys).state,
            Some(SystemState::Uncharted)
        );
    }

    #[test]
    fn classify_system_state_quarantined_beats_warzone() {
        // World tagged quarantined AND carrying warzone_signal == 2 (war_zone +
        // daemonic_corruption). Absent quarantine this would classify as Warzone;
        // quarantine is checked first ⇒ Quarantined wins.
        let mut w0 = empty_world();
        w0.tags = vec![
            "feature:quarantined".into(),
            "feature:war_zone".into(),
            "feature:daemonic_corruption".into(),
        ];
        w0.factions.push(admin_presence("imp", "lawful", 80.0));
        let sys = system_with("sys-0001", vec![w0]);
        assert_eq!(
            derive_system_control(&sys).state,
            Some(SystemState::Quarantined)
        );
    }

    #[test]
    fn classify_system_state_two_contested_worlds_is_warzone() {
        // Two populated, non-quarantined worlds each marked contested ⇒
        // contested_worlds == 2 ⇒ Warzone.
        let contested_world = |id: &str| {
            let mut w = empty_world();
            w.id = id.into();
            w.control = WorldControlSummary {
                contested: true,
                ..Default::default()
            };
            w.factions.push(admin_presence("imp", "lawful", 80.0));
            w
        };
        let sys = system_with(
            "sys-0001",
            vec![contested_world("sys-0001-w1"), contested_world("sys-0001-w2")],
        );
        assert_eq!(derive_system_control(&sys).state, Some(SystemState::Warzone));
    }

    #[test]
    fn classify_system_state_dominant_ne_orbital_is_blockaded() {
        // Construct the cascade inputs directly so dominant (faction "a") differs
        // from orbital_controller (faction "b") with no warzone/quarantine signal.
        let sys = system_with("sys-0001", vec![empty_world()]);
        let dominant: Option<FactionId> = Some("a".into());
        let orbital: Option<FactionId> = Some("b".into());
        let none: Option<FactionId> = None;
        let state = classify_system_state(SystemStateParams {
            sys: &sys,
            populated_worlds: 1,
            contested_worlds: 0,
            warzone_signal: 0,
            quarantined: false,
            dominant: &dominant,
            orbital_controller: &orbital,
            hidden_master: &none,
        });
        assert_eq!(state, SystemState::Blockaded);
    }

    #[test]
    fn classify_system_state_two_distinct_dominants_is_fragmented() {
        // Two populated worlds with distinct per-world `control.dominant`, not
        // contested, no blockade/infiltration trigger ⇒ Fragmented.
        let dominant_world = |id: &str, dom: &str| {
            let mut w = empty_world();
            w.id = id.into();
            w.control = WorldControlSummary {
                dominant: Some(dom.into()),
                ..Default::default()
            };
            w.factions.push(admin_presence(dom, "lawful", 80.0));
            w
        };
        let sys = system_with(
            "sys-0001",
            vec![
                dominant_world("sys-0001-w1", "alpha"),
                dominant_world("sys-0001-w2", "beta"),
            ],
        );
        let none: Option<FactionId> = None;
        let state = classify_system_state(SystemStateParams {
            sys: &sys,
            populated_worlds: 2,
            contested_worlds: 0,
            warzone_signal: 0,
            quarantined: false,
            dominant: &none,
            orbital_controller: &none,
            hidden_master: &none,
        });
        assert_eq!(state, SystemState::Fragmented);
    }
}
