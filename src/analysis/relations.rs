//! Inter-faction diplomacy / relationship layer (§4 NEW.md, §5 NEW2.md).
//!
//! For every unordered pair of factions present in the sector this derives a
//! canonical public / secret attitude, directional views from each side, a
//! treaty status, numeric trust/fear/rivalry/economic/military/covert
//! dimensions, and the legacy `stance` field used by older callers. Base stance
//! is computed from `kind × kind` and `disposition × disposition` rules that
//! ship as built-in defaults; users may extend or override them in
//! `relations.toml` (catalogued under `inputs.relations` in `sectorforge.toml`).
//! A small deterministic perturbation derived from
//! `blake3("sectorforge:{seed}:relations:{a}:{b}")` breaks ties so two pairs
//! with identical kind/disposition do not always pick the same direction.
//!
//! The matrix is emitted on [`crate::GeneratedSector::relations`] (empty by
//! default for back-compat). A derived `tension` scalar per pair is computed
//! from the worlds and systems where both factions co-occur and feeds the
//! "Factions at war" digest plus the Tension heatmap.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;

use camino::Utf8Path;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;
use crate::rng::stage_rng;
use crate::sector_model::{GeneratedFaction, GeneratedSector};

// ── Stance enum ────────────────────────────────────────────────────────────────

#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Stance {
    Allied,
    Aligned,
    #[default]
    Neutral,
    Rival,
    Hostile,
    AtWar,
}

impl Stance {
    fn level(self) -> i32 {
        match self {
            Stance::Allied => -2,
            Stance::Aligned => -1,
            Stance::Neutral => 0,
            Stance::Rival => 1,
            Stance::Hostile => 2,
            Stance::AtWar => 3,
        }
    }
    fn from_level(l: i32) -> Stance {
        match l {
            i if i <= -2 => Stance::Allied,
            -1 => Stance::Aligned,
            0 => Stance::Neutral,
            1 => Stance::Rival,
            2 => Stance::Hostile,
            _ => Stance::AtWar,
        }
    }
    fn shift(self, delta: i32) -> Stance {
        Stance::from_level((self.level() + delta).clamp(-2, 3))
    }
    /// True for Hostile / At War. Used by the tension heatmap and the
    /// "Factions at war" digest.
    #[must_use]
    pub fn is_hot(self) -> bool {
        matches!(self, Stance::Hostile | Stance::AtWar)
    }
    /// Stable human-readable label. Used by the markdown digest and the
    /// builder relations panel.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Stance::Allied => "Allied",
            Stance::Aligned => "Aligned",
            Stance::Neutral => "Neutral",
            Stance::Rival => "Rival",
            Stance::Hostile => "Hostile",
            Stance::AtWar => "At War",
        }
    }
}

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RelationsFile {
    #[serde(default)]
    pub relations: RelationsConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RelationsConfig {
    /// Kind-pair base stance rules. Match is symmetric; the first match wins.
    /// Built-in defaults always apply when the file is silent on a pair.
    #[serde(default)]
    pub kind_rules: Vec<KindRule>,
    /// Disposition adjustments. Sum is applied to the kind-pair base stance.
    #[serde(default)]
    pub disposition_rules: Vec<DispositionRule>,
    /// Explicit `(faction_id, faction_id)` pin. Bypasses the kind/disposition
    /// pipeline entirely.
    #[serde(default)]
    pub pair_overrides: Vec<PairOverride>,
    /// §5 NEW2.md richer overrides. These may pin public/secret attitudes,
    /// treaty status, and selected numeric dimensions while leaving other
    /// values derived.
    #[serde(default)]
    pub overrides: Vec<RelationOverride>,
    /// Whether the derived stance should bias the conflict tick (advisory).
    #[serde(default)]
    pub feed_conflict: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KindRule {
    pub a: String,
    pub b: String,
    pub stance: Stance,
    #[serde(default)]
    pub cause: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DispositionRule {
    pub a: String,
    pub b: String,
    /// Stance level delta. Positive = more hostile, negative = warmer.
    pub delta: i32,
    #[serde(default)]
    pub cause: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PairOverride {
    pub a: String,
    pub b: String,
    pub stance: Stance,
    #[serde(default)]
    pub cause: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RelationOverride {
    pub a: String,
    pub b: String,
    /// Symmetric public attitude override.
    #[serde(default)]
    pub public_attitude: Option<RelationAttitude>,
    /// Symmetric secret attitude override.
    #[serde(default)]
    pub secret_attitude: Option<RelationAttitude>,
    /// Directional override for `a → b`.
    #[serde(default)]
    pub a_public_attitude: Option<RelationAttitude>,
    /// Directional override for `b → a`.
    #[serde(default)]
    pub b_public_attitude: Option<RelationAttitude>,
    /// Directional override for `a → b`.
    #[serde(default)]
    pub a_secret_attitude: Option<RelationAttitude>,
    /// Directional override for `b → a`.
    #[serde(default)]
    pub b_secret_attitude: Option<RelationAttitude>,
    #[serde(default)]
    pub treaty_status: Option<TreatyStatus>,
    #[serde(default)]
    pub trust: Option<u8>,
    #[serde(default)]
    pub fear: Option<u8>,
    #[serde(default)]
    pub rivalry: Option<u8>,
    #[serde(default)]
    pub ideological_distance: Option<u8>,
    #[serde(default)]
    pub economic_dependency: Option<u8>,
    #[serde(default)]
    pub military_pressure: Option<u8>,
    #[serde(default)]
    pub covert_activity: Option<u8>,
    #[serde(default)]
    pub reason: Option<String>,
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelationsMatrix {
    pub pairs: Vec<FactionRelation>,
    /// Mirror of [`RelationsConfig::feed_conflict`] copied onto the derived
    /// matrix so [`crate::conflict::advance_sector`] knows whether to apply
    /// stance-based momentum bias on each tick.
    #[serde(default)]
    pub feed_conflict: bool,
}

impl RelationsMatrix {
    /// Lookup the stance between two faction ids (order-independent).
    #[must_use]
    pub fn stance_between(&self, a: &str, b: &str) -> Option<Stance> {
        let (lo, hi) = canonical_pair(a, b);
        self.pairs
            .iter()
            .find(|p| p.a == lo && p.b == hi)
            .map(|p| p.stance)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionRelation {
    pub a: crate::ids::FactionId,
    pub b: crate::ids::FactionId,
    /// Legacy canonical stance. In the NEW2 model this mirrors
    /// `secret_stance`, because GM-facing / mechanical consumers need the
    /// actual relationship rather than the public mask.
    pub stance: Stance,
    #[serde(default)]
    pub public_stance: Stance,
    #[serde(default)]
    pub secret_stance: Stance,
    #[serde(default)]
    pub public_attitude: RelationAttitude,
    #[serde(default)]
    pub secret_attitude: RelationAttitude,
    #[serde(default)]
    pub treaty_status: TreatyStatus,
    #[serde(default)]
    pub metrics: RelationMetrics,
    #[serde(default)]
    pub a_to_b: DirectionalRelation,
    #[serde(default)]
    pub b_to_a: DirectionalRelation,
    pub cause: String,
    /// 0..=100 derived from how often the pair co-occurs on contested worlds /
    /// active warzones. Pure read-only derivation.
    pub tension: f32,
}

#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RelationAttitude {
    Allied,
    Friendly,
    #[default]
    Transactional,
    Suspicious,
    Hostile,
    ExistentialEnemy,
}

impl RelationAttitude {
    fn level(self) -> i32 {
        match self {
            Self::Allied => -2,
            Self::Friendly => -1,
            Self::Transactional => 0,
            Self::Suspicious => 1,
            Self::Hostile => 2,
            Self::ExistentialEnemy => 3,
        }
    }

    fn from_stance(s: Stance) -> Self {
        match s {
            Stance::Allied => Self::Allied,
            Stance::Aligned => Self::Friendly,
            Stance::Neutral => Self::Transactional,
            Stance::Rival => Self::Suspicious,
            Stance::Hostile => Self::Hostile,
            Stance::AtWar => Self::ExistentialEnemy,
        }
    }

    fn to_stance(self) -> Stance {
        match self {
            Self::Allied => Stance::Allied,
            Self::Friendly => Stance::Aligned,
            Self::Transactional => Stance::Neutral,
            Self::Suspicious => Stance::Rival,
            Self::Hostile => Stance::Hostile,
            Self::ExistentialEnemy => Stance::AtWar,
        }
    }

    /// Stable human-readable label. Used by the markdown digest and the
    /// builder relations panel.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Allied => "Allied",
            Self::Friendly => "Friendly",
            Self::Transactional => "Transactional",
            Self::Suspicious => "Suspicious",
            Self::Hostile => "Hostile",
            Self::ExistentialEnemy => "Existential Enemy",
        }
    }
}

#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TreatyStatus {
    #[default]
    None,
    Pact,
    Truce,
    Vassalage,
    Charter,
    Nonaggression,
    Vendetta,
}

impl TreatyStatus {
    /// Stable human-readable label. Used by the markdown digest and the
    /// builder relations panel.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Pact => "Pact",
            Self::Truce => "Truce",
            Self::Vassalage => "Vassalage",
            Self::Charter => "Charter",
            Self::Nonaggression => "Nonaggression",
            Self::Vendetta => "Vendetta",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationMetrics {
    pub trust: u8,
    pub fear: u8,
    pub rivalry: u8,
    pub ideological_distance: u8,
    pub economic_dependency: u8,
    pub military_pressure: u8,
    pub covert_activity: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectionalRelation {
    pub from: crate::ids::FactionId,
    pub to: crate::ids::FactionId,
    #[serde(default)]
    pub public_attitude: RelationAttitude,
    #[serde(default)]
    pub secret_attitude: RelationAttitude,
    #[serde(default)]
    pub public_stance: Stance,
    #[serde(default)]
    pub secret_stance: Stance,
    #[serde(default)]
    pub metrics: RelationMetrics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelationsReport {
    pub sector_id: String,
    pub seed: String,
    pub matrix: RelationsMatrix,
}

// ── Built-in stance rules ──────────────────────────────────────────────────────

const IMPERIAL_KINDS: &[&str] = &[
    "imperial",
    "imperial_guard",
    "imperial_knight",
    "adepta_sororitas",
    "adeptus_astartes",
    "deathwatch",
    "grey_knights",
    "ecclesiarchy",
    "inquisition",
    "mechanicus",
    "talons_of_the_emperor",
    "collegia_titanica",
];

const CHAOS_KINDS: &[&str] = &[
    "chaos",
    "chaos_space_marine",
    "chaos_knight",
    "traitor_guard",
    "traitor_titan_legion",
    "dark_mechanicum",
    "daemon",
    "cult",
];

const TYRANID_KINDS: &[&str] = &["tyranid"];
const NECRON_KINDS: &[&str] = &["necron"];
const ORK_KINDS: &[&str] = &["ork"];
const AELDARI_KINDS: &[&str] = &["aeldari", "harlequin"];
const DRUKHARI_KINDS: &[&str] = &["drukhari"];
const TAU_KINDS: &[&str] = &["tau"];
const GSC_KINDS: &[&str] = &["genestealer_cult"];
const VOTANN_KINDS: &[&str] = &["leagues_of_votann"];
const MISC_XENOS_KINDS: &[&str] = &["xenos", "minor_xenos"];
const CRIMINAL_KINDS: &[&str] = &["criminal"];
const MERCHANT_KINDS: &[&str] = &["merchant"];
const REBEL_KINDS: &[&str] = &["rebel"];

fn in_group(kind: &str, group: &[&str]) -> bool {
    group.contains(&kind)
}

/// Default base stance between two kinds. Returned only when no user kind_rule
/// matches the pair.
fn default_kind_stance(a: &str, b: &str) -> (Stance, &'static str) {
    let both = |group: &[&str]| in_group(a, group) && in_group(b, group);
    let cross = |g1: &[&str], g2: &[&str]| {
        (in_group(a, g1) && in_group(b, g2)) || (in_group(a, g2) && in_group(b, g1))
    };

    // Same-group same-faction-family: usually warm.
    if both(IMPERIAL_KINDS) {
        return (Stance::Aligned, "Shared Imperial allegiance");
    }
    if both(CHAOS_KINDS) {
        return (Stance::Rival, "Chaos warbands eternally squabble");
    }
    if both(AELDARI_KINDS) {
        return (Stance::Aligned, "Shared Aeldari heritage");
    }
    if both(ORK_KINDS) {
        return (Stance::Rival, "Greenskin rivalry — until a Waaagh!");
    }

    // Tyranid + Necron eat / annihilate everything.
    if in_group(a, TYRANID_KINDS) || in_group(b, TYRANID_KINDS) {
        return (Stance::AtWar, "Tyranid swarm consumes all biomass");
    }
    if in_group(a, NECRON_KINDS) || in_group(b, NECRON_KINDS) {
        return (Stance::AtWar, "Necron protocols reclaim the galaxy");
    }

    // Imperial vs. Chaos / xenos / heresy.
    if cross(IMPERIAL_KINDS, CHAOS_KINDS) {
        return (Stance::AtWar, "Imperium and Chaos are eternal enemies");
    }
    if cross(IMPERIAL_KINDS, GSC_KINDS) {
        return (
            Stance::AtWar,
            "Genestealer infestation answers only to purge",
        );
    }
    if cross(IMPERIAL_KINDS, REBEL_KINDS) {
        return (Stance::AtWar, "Rebellion is heresy");
    }
    if cross(IMPERIAL_KINDS, ORK_KINDS) {
        return (Stance::AtWar, "Greenskin invasion underway");
    }
    if cross(IMPERIAL_KINDS, DRUKHARI_KINDS) {
        return (Stance::AtWar, "Drukhari slave-raids invite extermination");
    }
    if cross(IMPERIAL_KINDS, AELDARI_KINDS) {
        return (Stance::Hostile, "Aeldari motives are never trusted");
    }
    if cross(IMPERIAL_KINDS, TAU_KINDS) {
        return (
            Stance::Hostile,
            "T'au Empire expansion borders Imperial space",
        );
    }
    if cross(IMPERIAL_KINDS, MISC_XENOS_KINDS) {
        return (Stance::Hostile, "Xenos suspicion as standing policy");
    }
    if cross(IMPERIAL_KINDS, VOTANN_KINDS) {
        return (Stance::Rival, "Votann territorial claims under dispute");
    }

    // Chaos vs. the rest.
    if cross(CHAOS_KINDS, AELDARI_KINDS) {
        return (Stance::AtWar, "Aeldari oppose the Dark Gods with prophecy");
    }
    if cross(CHAOS_KINDS, ORK_KINDS) {
        return (Stance::Hostile, "Orks fight whoever is loudest");
    }
    if cross(CHAOS_KINDS, TAU_KINDS) {
        return (Stance::Hostile, "T'au reject the warp outright");
    }
    if cross(CHAOS_KINDS, MISC_XENOS_KINDS) {
        return (Stance::Hostile, "Mutual revulsion of warp and flesh");
    }
    if cross(CHAOS_KINDS, GSC_KINDS) {
        return (Stance::Rival, "Both prey on the underbelly of worlds");
    }
    if cross(CHAOS_KINDS, REBEL_KINDS) {
        return (
            Stance::Aligned,
            "Rebels make the easiest converts to the Pantheon",
        );
    }

    // Criminal / merchant pragmatic ties.
    if cross(CRIMINAL_KINDS, MERCHANT_KINDS) {
        return (Stance::Rival, "Crime and commerce share customers");
    }
    if both(CRIMINAL_KINDS) {
        return (Stance::Rival, "Rival syndicates fighting for turf");
    }
    if cross(IMPERIAL_KINDS, CRIMINAL_KINDS) {
        return (Stance::Hostile, "Imperial enforcement vs. underworld trade");
    }
    if cross(IMPERIAL_KINDS, MERCHANT_KINDS) {
        return (Stance::Aligned, "Free Traders operate under Imperial writ");
    }
    if cross(REBEL_KINDS, MERCHANT_KINDS) {
        return (
            Stance::Aligned,
            "Rebellion bankrolled by black-market trade",
        );
    }

    // Aeldari / Drukhari hate-bond.
    if cross(AELDARI_KINDS, DRUKHARI_KINDS) {
        return (Stance::Hostile, "Estranged Aeldari kin — old wounds");
    }
    // T'au + Aeldari pragmatic warmth.
    if cross(TAU_KINDS, AELDARI_KINDS) {
        return (Stance::Neutral, "Aeldari tolerate the upstart Empire");
    }
    // GSC vs. anyone else: hidden hostility but rarely openly at war.
    if in_group(a, GSC_KINDS) || in_group(b, GSC_KINDS) {
        return (Stance::Rival, "Cult infiltration plays a long game");
    }

    (Stance::Neutral, "No standing accord")
}

const DISPOSITION_DELTAS: &[(&str, &str, i32)] = &[
    ("hostile", "hostile", 2),
    ("hostile", "lawful", 1),
    ("hostile", "zealous", 2),
    ("zealous", "zealous", 1),
    ("zealous", "secretive", 1),
    ("opportunistic", "opportunistic", -1),
    ("opportunistic", "lawful", 0),
    ("lawful", "lawful", -1),
    ("insular", "insular", -1),
    ("insular", "zealous", 1),
    ("secretive", "secretive", 0),
];

fn default_disposition_delta(a: &str, b: &str) -> i32 {
    for (x, y, d) in DISPOSITION_DELTAS {
        if (*x == a && *y == b) || (*x == b && *y == a) {
            return *d;
        }
    }
    0
}

// ── Entry point ────────────────────────────────────────────────────────────────

/// Pure derivation: build the relations matrix for a generated sector.
#[must_use]
pub fn derive(sector: &GeneratedSector) -> RelationsMatrix {
    derive_with(sector, &RelationsConfig::default())
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &RelationsConfig) -> RelationsMatrix {
    derive_with_threshold(sector, cfg, 1)
}

/// Like [`derive_with`] but also takes a presence threshold. A faction is
/// included in the matrix only if its `world_presence.len() >=
/// min_world_presence`. Threshold `1` matches the historical behaviour
/// (every faction with any world presence anywhere). Higher thresholds drop
/// incidental single-world cameos and shrink the matrix quadratically.
#[must_use]
pub fn derive_with_threshold(
    sector: &GeneratedSector,
    cfg: &RelationsConfig,
    min_world_presence: usize,
) -> RelationsMatrix {
    let all_facs: &[GeneratedFaction] = &sector.factions;
    if all_facs.len() < 2 {
        return RelationsMatrix::default();
    }
    // Only emit pairs for factions that meaningfully appear in the sector.
    // The full catalogue can hold ~1000 factions (C(1000,2) ≈ 500k pairs,
    // ~70 MB JSON), which blows up load + render. The threshold controls how
    // many worlds a faction must occupy. Fall back to all factions if the
    // filter would empty the matrix — back-compat for unit tests + minimal
    // synthetic sectors where presence is omitted entirely.
    let threshold = min_world_presence.max(1);
    let present: Vec<&GeneratedFaction> = all_facs
        .iter()
        .filter(|f| {
            f.world_presence.len() >= threshold || (threshold == 1 && !f.system_presence.is_empty())
        })
        .collect();
    let facs: Vec<&GeneratedFaction> = if present.is_empty() {
        all_facs.iter().collect()
    } else {
        present
    };
    if facs.len() < 2 {
        return RelationsMatrix::default();
    }
    // Build co-occurrence weights for tension.
    let cooccur = build_cooccurrence(sector);

    let mut pairs: Vec<FactionRelation> = Vec::with_capacity(facs.len() * (facs.len() - 1) / 2);
    for i in 0..facs.len() {
        for j in (i + 1)..facs.len() {
            let a = facs[i];
            let b = facs[j];
            let (lo_id, _hi_id) = canonical_pair(&a.id, &b.id);
            let (lo, hi) = if lo_id == a.id { (a, b) } else { (b, a) };

            let rel = compute_pair(&sector.seed, lo, hi, cfg, &cooccur);
            pairs.push(rel);
        }
    }
    pairs.sort_by(|x, y| {
        y.tension
            .partial_cmp(&x.tension)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| x.a.cmp(&y.a))
            .then_with(|| x.b.cmp(&y.b))
    });
    RelationsMatrix {
        pairs,
        feed_conflict: cfg.feed_conflict,
    }
}

fn compute_pair(
    seed: &str,
    a: &GeneratedFaction,
    b: &GeneratedFaction,
    cfg: &RelationsConfig,
    cooccur: &BTreeMap<(String, String), CooccurStats>,
) -> FactionRelation {
    // 1) Explicit pair override (id-based) wins outright.
    for ov in &cfg.pair_overrides {
        let (lo, hi) = canonical_pair(&ov.a, &ov.b);
        if lo == a.id && hi == b.id {
            let rel_override = matching_relation_override(cfg, a, b);
            return build_relation(
                a,
                b,
                ov.stance,
                ov.cause
                    .clone()
                    .unwrap_or_else(|| format!("Override: {}", ov.stance.label())),
                cooccur,
                rel_override,
            );
        }
    }

    // 2) User kind_rules (first symmetric match wins).
    let mut base = None;
    for r in &cfg.kind_rules {
        if (r.a == *a.kind && r.b == *b.kind) || (r.a == *b.kind && r.b == *a.kind) {
            base = Some((
                r.stance,
                r.cause.clone().unwrap_or_else(|| match_cause(a, b)),
            ));
            break;
        }
    }
    let (base_stance, mut cause) = base.unwrap_or_else(|| {
        let (s, c) = default_kind_stance(&a.kind, &b.kind);
        (s, c.to_string())
    });

    // 3) Disposition delta: sum user rules and the built-in fallback.
    let mut delta = 0i32;
    let mut user_disp_hit = false;
    for r in &cfg.disposition_rules {
        if (r.a == *a.disposition && r.b == *b.disposition)
            || (r.a == *b.disposition && r.b == *a.disposition)
        {
            delta += r.delta;
            if let Some(c) = &r.cause {
                cause.push_str("; ");
                cause.push_str(c);
            }
            user_disp_hit = true;
        }
    }
    if !user_disp_hit {
        delta += default_disposition_delta(&a.disposition, &b.disposition);
    }

    // 4) Deterministic perturbation derived from the pair.
    let discriminator = format!("{}:{}", a.id, b.id);
    let mut rng = stage_rng(seed, "relations", &discriminator);
    // 25% chance to shift by ±1 — breaks symmetric ties so two same-kind/
    // same-disposition pairs are not always identical.
    let roll: f64 = rng.gen();
    let perturb = if roll < 0.125 {
        -1
    } else if roll < 0.25 {
        1
    } else {
        0
    };

    let stance = base_stance.shift(delta + perturb);

    let rel_override = matching_relation_override(cfg, a, b);
    build_relation(a, b, stance, cause, cooccur, rel_override)
}

fn build_relation(
    a: &GeneratedFaction,
    b: &GeneratedFaction,
    base_stance: Stance,
    mut cause: String,
    cooccur: &BTreeMap<(String, String), CooccurStats>,
    rel_override: Option<&RelationOverride>,
) -> FactionRelation {
    let stats = cooccur
        .get(&canonical_pair(&a.id, &b.id))
        .copied()
        .unwrap_or_default();
    let mut a_to_b = directional_view(a, b, base_stance, stats);
    let mut b_to_a = directional_view(b, a, base_stance, stats);
    let mut treaty_status = treaty_status_of(a, b, base_stance, stats);

    if let Some(ov) = rel_override {
        apply_relation_override(ov, &mut a_to_b, &mut b_to_a, &mut treaty_status);
        if let Some(reason) = &ov.reason {
            cause = reason.clone();
        }
    }

    let public_attitude = max_attitude(a_to_b.public_attitude, b_to_a.public_attitude);
    let secret_attitude = max_attitude(a_to_b.secret_attitude, b_to_a.secret_attitude);
    let public_stance = public_attitude.to_stance();
    let secret_stance = secret_attitude.to_stance();
    let metrics = combine_metrics(a_to_b.metrics, b_to_a.metrics);

    FactionRelation {
        a: a.id.clone(),
        b: b.id.clone(),
        stance: secret_stance,
        public_stance,
        secret_stance,
        public_attitude,
        secret_attitude,
        treaty_status,
        metrics,
        a_to_b,
        b_to_a,
        cause,
        tension: tension_of(a, b, secret_stance, cooccur),
    }
}

fn matching_relation_override<'a>(
    cfg: &'a RelationsConfig,
    a: &GeneratedFaction,
    b: &GeneratedFaction,
) -> Option<&'a RelationOverride> {
    cfg.overrides.iter().find(|ov| {
        let (lo, hi) = canonical_pair(&ov.a, &ov.b);
        lo == a.id && hi == b.id
    })
}

fn apply_relation_override(
    ov: &RelationOverride,
    a_to_b: &mut DirectionalRelation,
    b_to_a: &mut DirectionalRelation,
    treaty_status: &mut TreatyStatus,
) {
    if let Some(v) = ov.public_attitude {
        a_to_b.public_attitude = v;
        b_to_a.public_attitude = v;
    }
    if let Some(v) = ov.secret_attitude {
        a_to_b.secret_attitude = v;
        b_to_a.secret_attitude = v;
    }
    let config_a_is_first = ov.a == a_to_b.from.as_ref();
    if config_a_is_first {
        if let Some(v) = ov.a_public_attitude {
            a_to_b.public_attitude = v;
        }
        if let Some(v) = ov.b_public_attitude {
            b_to_a.public_attitude = v;
        }
        if let Some(v) = ov.a_secret_attitude {
            a_to_b.secret_attitude = v;
        }
        if let Some(v) = ov.b_secret_attitude {
            b_to_a.secret_attitude = v;
        }
    } else {
        if let Some(v) = ov.a_public_attitude {
            b_to_a.public_attitude = v;
        }
        if let Some(v) = ov.b_public_attitude {
            a_to_b.public_attitude = v;
        }
        if let Some(v) = ov.a_secret_attitude {
            b_to_a.secret_attitude = v;
        }
        if let Some(v) = ov.b_secret_attitude {
            a_to_b.secret_attitude = v;
        }
    }
    if let Some(v) = ov.treaty_status {
        *treaty_status = v;
    }

    for view in [a_to_b, b_to_a] {
        view.public_stance = view.public_attitude.to_stance();
        view.secret_stance = view.secret_attitude.to_stance();
        if let Some(v) = ov.trust {
            view.metrics.trust = v.min(100);
        }
        if let Some(v) = ov.fear {
            view.metrics.fear = v.min(100);
        }
        if let Some(v) = ov.rivalry {
            view.metrics.rivalry = v.min(100);
        }
        if let Some(v) = ov.ideological_distance {
            view.metrics.ideological_distance = v.min(100);
        }
        if let Some(v) = ov.economic_dependency {
            view.metrics.economic_dependency = v.min(100);
        }
        if let Some(v) = ov.military_pressure {
            view.metrics.military_pressure = v.min(100);
        }
        if let Some(v) = ov.covert_activity {
            view.metrics.covert_activity = v.min(100);
        }
    }
}

fn directional_view(
    from: &GeneratedFaction,
    to: &GeneratedFaction,
    stance: Stance,
    stats: CooccurStats,
) -> DirectionalRelation {
    let secret_attitude = RelationAttitude::from_stance(stance);
    let public_attitude = public_attitude_for(from, to, secret_attitude, stats);
    let public_stance = public_attitude.to_stance();
    let secret_stance = secret_attitude.to_stance();
    DirectionalRelation {
        from: from.id.clone(),
        to: to.id.clone(),
        public_attitude,
        secret_attitude,
        public_stance,
        secret_stance,
        metrics: directional_metrics(from, to, secret_attitude, stats),
    }
}

fn public_attitude_for(
    from: &GeneratedFaction,
    to: &GeneratedFaction,
    secret: RelationAttitude,
    stats: CooccurStats,
) -> RelationAttitude {
    if secret.level() < RelationAttitude::Hostile.level() {
        return secret;
    }
    if is_hidden_kind(&from.kind) || from.disposition.as_ref() == "secretive" {
        return RelationAttitude::Suspicious;
    }
    if is_hidden_kind(&to.kind) && stats.hidden_overlap > 0 {
        return RelationAttitude::Suspicious;
    }
    secret
}

fn directional_metrics(
    from: &GeneratedFaction,
    to: &GeneratedFaction,
    secret: RelationAttitude,
    stats: CooccurStats,
) -> RelationMetrics {
    let ideology = ideological_distance(&from.kind, &to.kind, secret.to_stance());
    let to_force = normalized_power(
        to.power
            .logistical
            .mul_add(0.4, to.power.military + to.power.naval),
    );
    let from_force = normalized_power(
        from.power
            .logistical
            .mul_add(0.4, from.power.military + from.power.naval),
    );
    let to_econ = normalized_power(to.power.economic + to.power.industrial + to.power.logistical);
    let from_econ =
        normalized_power(from.power.economic + from.power.industrial + from.power.logistical);
    let to_covert = normalized_power(to.power.covert);

    let rivalry_base = match secret {
        RelationAttitude::Allied => 0.0,
        RelationAttitude::Friendly => 8.0,
        RelationAttitude::Transactional => 18.0,
        RelationAttitude::Suspicious => 42.0,
        RelationAttitude::Hostile => 68.0,
        RelationAttitude::ExistentialEnemy => 90.0,
    };
    let trust_base = match secret {
        RelationAttitude::Allied => 88.0,
        RelationAttitude::Friendly => 68.0,
        RelationAttitude::Transactional => 45.0,
        RelationAttitude::Suspicious => 26.0,
        RelationAttitude::Hostile => 9.0,
        RelationAttitude::ExistentialEnemy => 0.0,
    };

    let rivalry = clamp_score(stats.route_competition.mul_add(
        0.45,
        (stats.active_warzones as f32).mul_add(
            10.0,
            (stats.claim_conflicts as f32).mul_add(
                10.0,
                (stats.contested_worlds as f32).mul_add(7.0, rivalry_base),
            ),
        ),
    ));
    let military_pressure = clamp_score(
        (stats.active_warzones as f32)
            .mul_add(14.0, to_force * 0.65 + stats.military_pressure * 0.45),
    );
    let covert_activity = clamp_score(
        to_covert.mul_add(0.75, stats.covert_activity * 0.5)
            + if is_hidden_kind(&to.kind) { 18.0 } else { 0.0 },
    );
    let economic_dependency = clamp_score(
        from_econ
            .min(to_econ)
            .mul_add(0.55, stats.economic_dependency * 0.55)
            + if is_merchant_kind(&from.kind) || is_merchant_kind(&to.kind) {
                8.0
            } else {
                0.0
            },
    );
    let fear = clamp_score((to_force - from_force).max(0.0).mul_add(
        0.5,
        (covert_activity as f32).mul_add(
            0.12,
            (military_pressure as f32).mul_add(
                0.35,
                match secret {
                    RelationAttitude::Allied => 5.0,
                    RelationAttitude::Friendly => 10.0,
                    RelationAttitude::Transactional => 20.0,
                    RelationAttitude::Suspicious => 35.0,
                    RelationAttitude::Hostile => 55.0,
                    RelationAttitude::ExistentialEnemy => 78.0,
                },
            ),
        ),
    ));
    let trust = clamp_score((covert_activity as f32).mul_add(
        -0.08,
        (ideology as f32).mul_add(
            -0.18,
            (rivalry as f32).mul_add(
                -0.24,
                (economic_dependency as f32).mul_add(0.12, trust_base),
            ),
        ),
    ));

    RelationMetrics {
        trust,
        fear,
        rivalry,
        ideological_distance: ideology,
        economic_dependency,
        military_pressure,
        covert_activity,
    }
}

fn combine_metrics(a: RelationMetrics, b: RelationMetrics) -> RelationMetrics {
    RelationMetrics {
        trust: ((u16::from(a.trust) + u16::from(b.trust)) / 2) as u8,
        fear: a.fear.max(b.fear),
        rivalry: a.rivalry.max(b.rivalry),
        ideological_distance: a.ideological_distance.max(b.ideological_distance),
        economic_dependency: a.economic_dependency.max(b.economic_dependency),
        military_pressure: a.military_pressure.max(b.military_pressure),
        covert_activity: a.covert_activity.max(b.covert_activity),
    }
}

fn max_attitude(a: RelationAttitude, b: RelationAttitude) -> RelationAttitude {
    if a.level() >= b.level() {
        a
    } else {
        b
    }
}

fn treaty_status_of(
    a: &GeneratedFaction,
    b: &GeneratedFaction,
    stance: Stance,
    stats: CooccurStats,
) -> TreatyStatus {
    if stats.claim_conflicts > 0 && matches!(stance, Stance::Hostile | Stance::AtWar) {
        return TreatyStatus::Vendetta;
    }
    if matches!(stance, Stance::AtWar) {
        return TreatyStatus::Vendetta;
    }
    if matches!(stance, Stance::Hostile) && stats.active_warzones == 0 {
        return TreatyStatus::Nonaggression;
    }
    if cross_kinds(&a.kind, &b.kind, IMPERIAL_KINDS, MERCHANT_KINDS) {
        return TreatyStatus::Charter;
    }
    if cross_kinds(&a.kind, &b.kind, IMPERIAL_KINDS, &["mechanicus"]) {
        return TreatyStatus::Pact;
    }
    if matches!(stance, Stance::Allied | Stance::Aligned) && stats.economic_dependency > 35.0 {
        return TreatyStatus::Pact;
    }
    if matches!(stance, Stance::Rival) && stats.contested_worlds > 0 {
        return TreatyStatus::Truce;
    }
    TreatyStatus::None
}

fn match_cause(a: &GeneratedFaction, b: &GeneratedFaction) -> String {
    format!("Kind rule: {} ↔ {}", a.kind, b.kind)
}

fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

fn cross_kinds(a: &str, b: &str, g1: &[&str], g2: &[&str]) -> bool {
    (in_group(a, g1) && in_group(b, g2)) || (in_group(a, g2) && in_group(b, g1))
}

fn is_hidden_kind(kind: &str) -> bool {
    in_group(kind, GSC_KINDS)
        || in_group(kind, CRIMINAL_KINDS)
        || in_group(kind, DRUKHARI_KINDS)
        || in_group(kind, AELDARI_KINDS)
        || kind == "cult"
        || kind == "harlequin"
}

fn is_merchant_kind(kind: &str) -> bool {
    in_group(kind, MERCHANT_KINDS) || kind == "rogue_trader"
}

fn ideology_group(kind: &str) -> &'static str {
    if in_group(kind, IMPERIAL_KINDS) {
        "imperial"
    } else if in_group(kind, CHAOS_KINDS) {
        "chaos"
    } else if in_group(kind, AELDARI_KINDS) || in_group(kind, DRUKHARI_KINDS) {
        "aeldari"
    } else if in_group(kind, TAU_KINDS) {
        "tau"
    } else if in_group(kind, ORK_KINDS) {
        "ork"
    } else if in_group(kind, NECRON_KINDS) {
        "necron"
    } else if in_group(kind, TYRANID_KINDS) || in_group(kind, GSC_KINDS) {
        "hive"
    } else if in_group(kind, VOTANN_KINDS) {
        "votann"
    } else if in_group(kind, MERCHANT_KINDS) {
        "merchant"
    } else if in_group(kind, CRIMINAL_KINDS) {
        "criminal"
    } else if in_group(kind, REBEL_KINDS) {
        "rebel"
    } else {
        "misc"
    }
}

fn ideological_distance(a: &str, b: &str, stance: Stance) -> u8 {
    if a == b {
        return 5;
    }
    let ga = ideology_group(a);
    let gb = ideology_group(b);
    if ga == gb {
        return match ga {
            "chaos" | "ork" | "criminal" => 45,
            "imperial" | "aeldari" | "merchant" => 22,
            _ => 30,
        };
    }
    if matches!(
        (ga, gb),
        ("imperial", "chaos")
            | ("chaos", "imperial")
            | ("imperial", "hive")
            | ("hive", "imperial")
            | ("chaos", "aeldari")
            | ("aeldari", "chaos")
    ) {
        return 100;
    }
    if ga == "necron" || gb == "necron" || ga == "hive" || gb == "hive" {
        return 90;
    }
    match stance {
        Stance::Allied => 20,
        Stance::Aligned => 30,
        Stance::Neutral => 45,
        Stance::Rival => 65,
        Stance::Hostile => 82,
        Stance::AtWar => 96,
    }
}

fn normalized_power(v: f32) -> f32 {
    (v.max(0.0).ln_1p() * 12.0).clamp(0.0, 100.0)
}

fn clamp_score(v: f32) -> u8 {
    v.round().clamp(0.0, 100.0) as u8
}

// ── Tension scalar ─────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Copy)]
struct CooccurStats {
    contested_worlds: u32,
    same_system_worlds: u32,
    active_warzones: u32,
    claim_conflicts: u32,
    hidden_overlap: u32,
    route_competition: f32,
    economic_dependency: f32,
    military_pressure: f32,
    covert_activity: f32,
}

fn build_cooccurrence(sector: &GeneratedSector) -> BTreeMap<(String, String), CooccurStats> {
    let mut out: BTreeMap<(String, String), CooccurStats> = BTreeMap::new();
    for sys in &sector.systems {
        for world in sector.get_worlds_for_system(sys) {
            for i in 0..world.factions.len() {
                for j in (i + 1)..world.factions.len() {
                    let pa = &world.factions[i];
                    let pb = &world.factions[j];
                    let a = pa.faction_id.as_str();
                    let b = pb.faction_id.as_str();
                    bump_cooccur(&mut out, a, b, |s| {
                        s.same_system_worlds += 1;
                        s.economic_dependency +=
                            pa.dimensions.economic.min(pb.dimensions.economic).mul_add(
                                0.06,
                                pa.dimensions.logistics.min(pb.dimensions.logistics) * 0.04,
                            );
                        s.military_pressure += (pa.dimensions.military + pa.dimensions.orbital)
                            .max(pb.dimensions.military + pb.dimensions.orbital)
                            * 0.04;
                        s.covert_activity += pa.dimensions.covert.max(pb.dimensions.covert) * 0.05;
                        if matches!(pa.influence, crate::sector_model::FactionInfluence::Hidden)
                            || matches!(pb.influence, crate::sector_model::FactionInfluence::Hidden)
                            || pa.dimensions.visibility < 25.0
                            || pb.dimensions.visibility < 25.0
                        {
                            s.hidden_overlap += 1;
                        }
                    });
                    if world.control.contested {
                        bump_cooccur(&mut out, a, b, |s| s.contested_worlds += 1);
                    }
                }
            }
            for i in 0..world.claims.len() {
                for j in (i + 1)..world.claims.len() {
                    bump_cooccur(
                        &mut out,
                        world.claims[i].faction_id.as_str(),
                        world.claims[j].faction_id.as_str(),
                        |s| s.claim_conflicts += 1,
                    );
                }
            }
        }
        // Active warzone at the system level adds heat between every co-located
        // pair in the system.
        if let Some(crate::sector_model::SystemState::Warzone) = sys.control.state {
            let mut sys_ids: BTreeSet<&str> = BTreeSet::new();
            for w in sector.get_worlds_for_system(sys) {
                for p in &w.factions {
                    sys_ids.insert(p.faction_id.as_str());
                }
            }
            let ids: Vec<&str> = sys_ids.into_iter().collect();
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    bump_cooccur(&mut out, ids[i], ids[j], |s| s.active_warzones += 1);
                }
            }
        }
    }
    for route in &sector.routes {
        for i in 0..route.controls.len() {
            for j in (i + 1)..route.controls.len() {
                let a = &route.controls[i];
                let b = &route.controls[j];
                bump_cooccur(
                    &mut out,
                    a.faction_id.as_str(),
                    b.faction_id.as_str(),
                    |s| {
                        s.route_competition += a.piracy.min(b.piracy).mul_add(
                            0.25,
                            a.interdiction.min(b.interdiction).mul_add(
                                0.25,
                                a.patrol
                                    .min(b.patrol)
                                    .mul_add(0.15, a.toll.min(b.toll) * 0.20),
                            ),
                        );
                        s.economic_dependency += a.toll.min(b.toll) * 0.20;
                        s.military_pressure += (a.patrol + a.interdiction + a.piracy)
                            .max(b.patrol + b.interdiction + b.piracy)
                            * 0.10;
                        s.covert_activity += a
                            .secrecy
                            .max(b.secrecy)
                            .mul_add(0.10, a.piracy.max(b.piracy) * 0.10);
                        if a.secrecy.max(b.secrecy) >= 65.0 {
                            s.hidden_overlap += 1;
                        }
                    },
                );
            }
        }
    }
    out
}

fn bump_cooccur<F>(out: &mut BTreeMap<(String, String), CooccurStats>, a: &str, b: &str, f: F)
where
    F: FnOnce(&mut CooccurStats),
{
    if a == b {
        return;
    }
    let key = canonical_pair(a, b);
    let entry = out.entry(key).or_default();
    f(entry);
}

fn tension_of(
    a: &GeneratedFaction,
    b: &GeneratedFaction,
    stance: Stance,
    cooccur: &BTreeMap<(String, String), CooccurStats>,
) -> f32 {
    let key = canonical_pair(&a.id, &b.id);
    let stats = cooccur.get(&key).copied().unwrap_or_default();
    let stance_bonus = match stance {
        Stance::AtWar => 40.0,
        Stance::Hostile => 25.0,
        Stance::Rival => 12.0,
        Stance::Neutral => 0.0,
        Stance::Aligned => -5.0,
        Stance::Allied => -10.0,
    };
    let raw = stats.covert_activity.mul_add(
        0.1,
        stats.military_pressure.mul_add(
            0.2,
            stats.route_competition.mul_add(
                0.4,
                (stats.claim_conflicts as f32).mul_add(
                    6.0,
                    (stats.same_system_worlds as f32).mul_add(
                        1.5,
                        (stats.active_warzones as f32).mul_add(
                            10.0,
                            (stats.contested_worlds as f32).mul_add(8.0, stance_bonus),
                        ),
                    ),
                ),
            ),
        ),
    );
    raw.clamp(0.0, 100.0)
}

// ── Loader for `relations.toml` ────────────────────────────────────────────────

/// Load `relations.toml` from disk. Missing file returns the default config.
///
/// # Errors
///
/// Returns [`SectorError::ConfigParse`] if the file is malformed or
/// [`SectorError::Io`] if it cannot be read.
pub fn load_relations_file(path: &Utf8Path) -> Result<RelationsConfig, SectorError> {
    if !path.exists() {
        return Ok(RelationsConfig::default());
    }
    let text = fs::read_to_string(path).map_err(|e| SectorError::io(path.as_str(), e))?;
    let parsed: RelationsFile = toml::from_str(&text)
        .map_err(|e| SectorError::config_parse(path.as_str(), e.to_string()))?;
    Ok(parsed.relations)
}

// ── Markdown render ────────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(report: &RelationsReport) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Diplomacy — {}", report.sector_id);
    let _ = writeln!(s, "\nSeed: `{}`", report.seed);
    let _ = writeln!(s, "\nTotal pairs: **{}**", report.matrix.pairs.len());

    // Factions at war digest first — the headline.
    let at_war: Vec<&FactionRelation> = report
        .matrix
        .pairs
        .iter()
        .filter(|p| p.stance == Stance::AtWar)
        .collect();
    if !at_war.is_empty() {
        let _ = writeln!(s, "\n## Factions at war");
        for r in at_war {
            let _ = writeln!(
                s,
                "- **{} ↔ {}** — {} / {} (tension {:.0}; {})",
                r.a,
                r.b,
                r.public_attitude.label(),
                r.secret_attitude.label(),
                r.tension,
                r.cause
            );
        }
    }
    let hot: Vec<&FactionRelation> = report
        .matrix
        .pairs
        .iter()
        .filter(|p| p.stance == Stance::Hostile)
        .collect();
    if !hot.is_empty() {
        let _ = writeln!(s, "\n## Hostile pairs");
        for r in hot {
            let _ = writeln!(
                s,
                "- {} ↔ {} — {} / {} (tension {:.0}; {})",
                r.a,
                r.b,
                r.public_attitude.label(),
                r.secret_attitude.label(),
                r.tension,
                r.cause
            );
        }
    }

    let _ = writeln!(s, "\n## Full matrix");
    let _ = writeln!(
        s,
        "\n| A | B | Public | Secret | Treaty | Trust | Fear | Rivalry | Ideology | Econ | Military | Covert | Tension | Cause |"
    );
    let _ = writeln!(
        s,
        "|---|---|--------|--------|--------|------:|-----:|--------:|---------:|-----:|---------:|-------:|--------:|-------|"
    );
    for r in &report.matrix.pairs {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.0} | {} |",
            r.a,
            r.b,
            r.public_attitude.label(),
            r.secret_attitude.label(),
            r.treaty_status.label(),
            r.metrics.trust,
            r.metrics.fear,
            r.metrics.rivalry,
            r.metrics.ideological_distance,
            r.metrics.economic_dependency,
            r.metrics.military_pressure,
            r.metrics.covert_activity,
            r.tension,
            r.cause
        );
    }
    let _ = writeln!(s, "\n## Faction dossiers");
    let mut by_faction: BTreeMap<&str, Vec<&FactionRelation>> = BTreeMap::new();
    for r in &report.matrix.pairs {
        by_faction.entry(r.a.as_str()).or_default().push(r);
        by_faction.entry(r.b.as_str()).or_default().push(r);
    }
    for (fid, rels) in by_faction {
        let _ = writeln!(s, "\n### {fid}");
        for r in rels.into_iter().take(8) {
            let other = if r.a == fid { &r.b } else { &r.a };
            let view = if r.a == fid { &r.a_to_b } else { &r.b_to_a };
            let _ = writeln!(
                s,
                "- {}: public {}, secret {}, trust {}, fear {}, rivalry {}",
                other,
                view.public_attitude.label(),
                view.secret_attitude.label(),
                view.metrics.trust,
                view.metrics.fear,
                view.metrics.rivalry
            );
        }
    }
    s
}

/// Write `relations.md` + `relations.json` into a directory.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_report(output_dir: &Utf8Path, report: &RelationsReport) -> Result<(), SectorError> {
    crate::export::write_md_and_json(output_dir, "relations", &render_markdown(report), report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{GeneratedFaction, GenerationManifest, PowerProfile};
    use std::collections::BTreeMap as Map;

    fn faction(id: &str, kind: &str, disposition: &str) -> GeneratedFaction {
        GeneratedFaction {
            id: id.into(),
            name: id.into(),
            kind: kind.into(),
            disposition: disposition.into(),
            subfactions: Vec::new(),
            system_presence: vec![],
            world_presence: vec![],
            power: PowerProfile::default(),
        }
    }

    fn sector_with(factions: Vec<GeneratedFaction>) -> GeneratedSector {
        GeneratedSector {
            id: "rel-test".into(),
            title: "Rel Test".into(),
            seed: "rel-seed".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0".into(),
            width: 2,
            height: 2,
            systems: vec![],
            routes: vec![],
            factions,
            manifest: GenerationManifest {
                project_id: "t".into(),
                generated_at_policy: "n".into(),
                generator_name: "sf".into(),
                generator_version: "0".into(),
                seed: "s".into(),
                seed_hash: "h".into(),
                base_seed: None,
                candidate_index: None,
                constraints_digest: None,
                profile: None,
                input_digests: Map::new(),
                settings_digest: "d".into(),
                system_count: 0,
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: RelationsMatrix::default().into(),
            regions: vec![].into(),
            economy: Default::default(),
            chronicle: Default::default(),
            ..Default::default()
        }
    }

    #[test]
    fn imperial_vs_chaos_is_war() {
        let m = derive(&sector_with(vec![
            faction("imp", "imperial", "lawful"),
            faction("chaos", "chaos_space_marine", "hostile"),
        ]));
        let s = m.stance_between("imp", "chaos").unwrap();
        assert_eq!(s, Stance::AtWar);
    }

    #[test]
    fn imperial_aligned_kinds_are_warm() {
        let m = derive(&sector_with(vec![
            faction("a", "imperial", "lawful"),
            faction("b", "mechanicus", "insular"),
        ]));
        let s = m.stance_between("a", "b").unwrap();
        // Aligned base, no dispositional escalation expected from these two.
        assert!(matches!(
            s,
            Stance::Aligned | Stance::Allied | Stance::Neutral
        ));
    }

    #[test]
    fn pair_overrides_win() {
        let mut cfg = RelationsConfig::default();
        cfg.pair_overrides.push(PairOverride {
            a: "imp".into(),
            b: "chaos".into(),
            stance: Stance::Allied,
            cause: Some("test override".into()),
        });
        let m = derive_with(
            &sector_with(vec![
                faction("imp", "imperial", "lawful"),
                faction("chaos", "chaos_space_marine", "hostile"),
            ]),
            &cfg,
        );
        assert_eq!(m.stance_between("imp", "chaos"), Some(Stance::Allied));
    }

    #[test]
    fn rich_override_sets_public_secret_attitudes() {
        let mut cfg = RelationsConfig::default();
        cfg.overrides.push(RelationOverride {
            a: "imp".into(),
            b: "trader".into(),
            public_attitude: Some(RelationAttitude::Friendly),
            secret_attitude: Some(RelationAttitude::Hostile),
            treaty_status: Some(TreatyStatus::Charter),
            trust: Some(35),
            rivalry: Some(70),
            reason: Some("disputed charter".into()),
            ..RelationOverride::default()
        });
        let m = derive_with(
            &sector_with(vec![
                faction("imp", "imperial", "lawful"),
                faction("trader", "merchant", "opportunistic"),
            ]),
            &cfg,
        );
        let rel = m
            .pairs
            .iter()
            .find(|p| p.a == "imp" && p.b == "trader")
            .unwrap();
        assert_eq!(rel.public_attitude, RelationAttitude::Friendly);
        assert_eq!(rel.secret_attitude, RelationAttitude::Hostile);
        assert_eq!(rel.stance, Stance::Hostile);
        assert_eq!(rel.treaty_status, TreatyStatus::Charter);
        assert_eq!(rel.metrics.trust, 35);
        assert_eq!(rel.metrics.rivalry, 70);
        assert_eq!(rel.cause, "disputed charter");
    }

    #[test]
    fn directional_override_follows_config_order() {
        let mut cfg = RelationsConfig::default();
        cfg.overrides.push(RelationOverride {
            a: "zeta".into(),
            b: "alpha".into(),
            a_secret_attitude: Some(RelationAttitude::Suspicious),
            b_secret_attitude: Some(RelationAttitude::Hostile),
            ..RelationOverride::default()
        });
        let m = derive_with(
            &sector_with(vec![
                faction("alpha", "imperial", "lawful"),
                faction("zeta", "merchant", "opportunistic"),
            ]),
            &cfg,
        );
        let rel = &m.pairs[0];
        assert_eq!(rel.a, "alpha");
        assert_eq!(rel.b, "zeta");
        assert_eq!(rel.a_to_b.secret_attitude, RelationAttitude::Hostile);
        assert_eq!(rel.b_to_a.secret_attitude, RelationAttitude::Suspicious);
    }

    #[test]
    fn deterministic() {
        let s = sector_with(vec![
            faction("a", "imperial", "lawful"),
            faction("b", "mechanicus", "insular"),
            faction("c", "chaos_space_marine", "hostile"),
            faction("d", "tyranid", "hostile"),
        ]);
        let a = derive(&s);
        let b = derive(&s);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }
}
