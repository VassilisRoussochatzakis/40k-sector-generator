//! Inter-faction diplomacy / relationship layer (§4 NEW.md).
//!
//! For every unordered pair of factions present in the sector this derives a
//! single canonical stance (Allied … At War) plus a short cause-text. Base
//! stance is computed from `kind × kind` and `disposition × disposition`
//! rules that ship as built-in defaults; users may extend or override them in
//! `relations.toml` (catalogued under `inputs.relations` in
//! `sectorforge.toml`). A small deterministic perturbation derived from
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Stance {
    Allied,
    Aligned,
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
    fn label(self) -> &'static str {
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
    pub a: String,
    pub b: String,
    pub stance: Stance,
    pub cause: String,
    /// 0..=100 derived from how often the pair co-occurs on contested worlds /
    /// active warzones. Pure read-only derivation.
    pub tension: f32,
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
            return FactionRelation {
                a: a.id.clone(),
                b: b.id.clone(),
                stance: ov.stance,
                cause: ov
                    .cause
                    .clone()
                    .unwrap_or_else(|| format!("Override: {}", ov.stance.label())),
                tension: tension_of(a, b, ov.stance, cooccur),
            };
        }
    }

    // 2) User kind_rules (first symmetric match wins).
    let mut base = None;
    for r in &cfg.kind_rules {
        if (r.a == a.kind && r.b == b.kind) || (r.a == b.kind && r.b == a.kind) {
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
        if (r.a == a.disposition && r.b == b.disposition)
            || (r.a == b.disposition && r.b == a.disposition)
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

    FactionRelation {
        a: a.id.clone(),
        b: b.id.clone(),
        stance,
        cause,
        tension: tension_of(a, b, stance, cooccur),
    }
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

// ── Tension scalar ─────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Copy)]
struct CooccurStats {
    contested_worlds: u32,
    same_system_worlds: u32,
    active_warzones: u32,
}

fn build_cooccurrence(sector: &GeneratedSector) -> BTreeMap<(String, String), CooccurStats> {
    let mut out: BTreeMap<(String, String), CooccurStats> = BTreeMap::new();
    let bump = |out: &mut BTreeMap<(String, String), CooccurStats>,
                a: &str,
                b: &str,
                f: fn(&mut CooccurStats)| {
        if a == b {
            return;
        }
        let key = canonical_pair(a, b);
        let entry = out.entry(key).or_default();
        f(entry);
    };

    for sys in &sector.systems {
        for world in &sys.worlds {
            let ids: Vec<&str> = world
                .factions
                .iter()
                .map(|p| p.faction_id.as_str())
                .collect();
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    bump(&mut out, ids[i], ids[j], |s| s.same_system_worlds += 1);
                    if world.control.contested {
                        bump(&mut out, ids[i], ids[j], |s| s.contested_worlds += 1);
                    }
                }
            }
        }
        // Active warzone at the system level adds heat between every co-located
        // pair in the system.
        if let Some(crate::sector_model::SystemState::Warzone) = sys.control.state {
            let mut sys_ids: BTreeSet<&str> = BTreeSet::new();
            for w in &sys.worlds {
                for p in &w.factions {
                    sys_ids.insert(p.faction_id.as_str());
                }
            }
            let ids: Vec<&str> = sys_ids.into_iter().collect();
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    bump(&mut out, ids[i], ids[j], |s| s.active_warzones += 1);
                }
            }
        }
    }
    out
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
    let raw = stance_bonus
        + stats.contested_worlds as f32 * 8.0
        + stats.active_warzones as f32 * 10.0
        + stats.same_system_worlds as f32 * 1.5;
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
                "- **{} ↔ {}** — {} (tension {:.0})",
                r.a, r.b, r.cause, r.tension
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
                "- {} ↔ {} — {} (tension {:.0})",
                r.a, r.b, r.cause, r.tension
            );
        }
    }

    let _ = writeln!(s, "\n## Full matrix");
    let _ = writeln!(s, "\n| A | B | Stance | Tension | Cause |");
    let _ = writeln!(s, "|---|---|--------|---------|-------|");
    for r in &report.matrix.pairs {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {:.0} | {} |",
            r.a,
            r.b,
            r.stance.label(),
            r.tension,
            r.cause
        );
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
                profile: None,
                input_digests: Map::new(),
                settings_digest: "d".into(),
                system_count: 0,
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: RelationsMatrix::default(),
            regions: vec![],
            economy: Default::default(),
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
