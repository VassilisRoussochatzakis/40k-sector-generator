//! Core derivation pipeline + the `relations.toml` loader: the public entry
//! points (`derive`/`derive_with`/`derive_with_threshold`), the per-pair
//! computation (`compute_pair`/`build_relation` + override application +
//! directional views + metrics), the canonical-pair helper and numeric
//! normalizers, and `load_relations_file`.

use std::collections::BTreeMap;
use std::fs;

use camino::Utf8Path;
use rand::Rng;

use super::config::{
    DirectionalRelation, DispositionRule, FactionRelation, KindRule, RelationAttitude,
    RelationMetrics, RelationOverride, RelationsConfig, RelationsMatrix, Stance, TreatyStatus,
};
use super::tables::{
    cross_kinds, default_disposition_delta, default_kind_stance, ideological_distance,
    is_hidden_kind, is_merchant_kind, IMPERIAL_KINDS, MERCHANT_KINDS,
};
use super::tension::{build_cooccurrence, cooccur_stats, tension_of, CooccurStats};
use crate::errors::SectorError;
use crate::rng::stage_rng;
use crate::sector_model::{GeneratedFaction, GeneratedSector};
use crate::FxMap;

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
    derive_with_threshold_reroll(sector, cfg, min_world_presence, "")
}

/// Like [`derive_with_threshold`] but folds a re-roll suffix into the per-pair
/// `("relations","<a>:<b>")` RNG discriminator. An empty suffix reproduces the
/// legacy key byte-for-byte (golden-safe, invariant #2); a `":r{n}"` suffix
/// yields a deterministically different perturbation set. Used by the iterative
/// generation seam ([`crate::generation::generate_prefix`]).
#[must_use]
pub fn derive_with_threshold_reroll(
    sector: &GeneratedSector,
    cfg: &RelationsConfig,
    min_world_presence: usize,
    reroll_suffix: &str,
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
    // B6: faction-id → index map, so the hot co-occurrence path keys on
    // `(u32, u32)` instead of allocating two `String`s per pair-event/lookup.
    // The map is lookup-only (never iterated for output), so the integer keys
    // do not affect emission order.
    let idx: FxMap<&str, u32> = all_facs
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id.as_str(), i as u32))
        .collect();

    // Build co-occurrence weights for tension.
    let cooccur = build_cooccurrence(sector, &idx);
    // B3: index the user kind/disposition rules once, off the O(F²) pair loop.
    let rules = RuleIndex::build(cfg);

    let mut pairs: Vec<FactionRelation> = Vec::with_capacity(facs.len() * (facs.len() - 1) / 2);
    for i in 0..facs.len() {
        for j in (i + 1)..facs.len() {
            let a = facs[i];
            let b = facs[j];
            let (lo_id, _hi_id) = canonical_pair(&a.id, &b.id);
            let (lo, hi) = if lo_id == a.id { (a, b) } else { (b, a) };

            let rel = compute_pair(
                &sector.seed,
                lo,
                hi,
                cfg,
                &rules,
                &cooccur,
                &idx,
                reroll_suffix,
            );
            pairs.push(rel);
        }
    }
    pairs.sort_by(|x, y| {
        crate::analysis::cmp_f32_desc(x.tension, y.tension)
            .then_with(|| x.a.cmp(&y.a))
            .then_with(|| x.b.cmp(&y.b))
    });
    RelationsMatrix {
        pairs,
        feed_conflict: cfg.feed_conflict,
    }
}

/// Assign `s` the next sequential id if it is not already interned.
fn intern<'c>(map: &mut FxMap<&'c str, u32>, s: &'c str) {
    let n = map.len() as u32;
    map.entry(s).or_insert(n);
}

/// Pre-indexed user rules (B3). The O(F²) pair loop previously rescanned the
/// full `kind_rules` / `disposition_rules` slices per pair. Here each rule set
/// is indexed once on the canonical (unordered) pair of interned kind /
/// disposition strings, so a pair does two map lookups instead of two linear
/// scans. Strings absent from every rule are never interned — a kind/disposition
/// that matches no rule can never satisfy a match, so a missing index entry is
/// exactly the linear scan's "no match".
struct RuleIndex<'c> {
    kind_ids: FxMap<&'c str, u32>,
    /// First rule per canonical kind pair (matches the scan's first-match-wins).
    kind_rules: BTreeMap<(u32, u32), &'c KindRule>,
    disp_ids: FxMap<&'c str, u32>,
    /// All rules per canonical disposition pair, in config order (the scan sums
    /// every match and concatenates causes in order).
    disp_rules: BTreeMap<(u32, u32), Vec<&'c DispositionRule>>,
}

impl<'c> RuleIndex<'c> {
    fn build(cfg: &'c RelationsConfig) -> Self {
        let mut kind_ids: FxMap<&str, u32> = FxMap::default();
        for r in &cfg.kind_rules {
            intern(&mut kind_ids, r.a.as_str());
            intern(&mut kind_ids, r.b.as_str());
        }
        let mut kind_rules: BTreeMap<(u32, u32), &KindRule> = BTreeMap::new();
        for r in &cfg.kind_rules {
            let key = canonical_pair_idx(kind_ids[r.a.as_str()], kind_ids[r.b.as_str()]);
            kind_rules.entry(key).or_insert(r);
        }

        let mut disp_ids: FxMap<&str, u32> = FxMap::default();
        for r in &cfg.disposition_rules {
            intern(&mut disp_ids, r.a.as_str());
            intern(&mut disp_ids, r.b.as_str());
        }
        let mut disp_rules: BTreeMap<(u32, u32), Vec<&DispositionRule>> = BTreeMap::new();
        for r in &cfg.disposition_rules {
            let key = canonical_pair_idx(disp_ids[r.a.as_str()], disp_ids[r.b.as_str()]);
            disp_rules.entry(key).or_default().push(r);
        }

        Self {
            kind_ids,
            kind_rules,
            disp_ids,
            disp_rules,
        }
    }

    /// The winning kind rule for an (unordered) kind pair, if any.
    fn kind_rule(&self, a: &str, b: &str) -> Option<&'c KindRule> {
        let ia = *self.kind_ids.get(a)?;
        let ib = *self.kind_ids.get(b)?;
        self.kind_rules.get(&canonical_pair_idx(ia, ib)).copied()
    }

    /// Every disposition rule matching an (unordered) disposition pair, in
    /// config order.
    fn disposition_rules(&self, a: &str, b: &str) -> &[&'c DispositionRule] {
        let (Some(&ia), Some(&ib)) = (self.disp_ids.get(a), self.disp_ids.get(b)) else {
            return &[];
        };
        self.disp_rules
            .get(&canonical_pair_idx(ia, ib))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_pair(
    seed: &str,
    a: &GeneratedFaction,
    b: &GeneratedFaction,
    cfg: &RelationsConfig,
    rules: &RuleIndex,
    cooccur: &BTreeMap<(u32, u32), CooccurStats>,
    idx: &FxMap<&str, u32>,
    reroll_suffix: &str,
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
                idx,
                rel_override,
            );
        }
    }

    // 2) User kind_rules (first symmetric match wins).
    let base = rules.kind_rule(&a.kind, &b.kind).map(|r| {
        (
            r.stance,
            r.cause.clone().unwrap_or_else(|| match_cause(a, b)),
        )
    });
    let (base_stance, mut cause) = base.unwrap_or_else(|| {
        let (s, c) = default_kind_stance(&a.kind, &b.kind);
        (s, c.to_string())
    });

    // 3) Disposition delta: sum user rules and the built-in fallback.
    let mut delta = 0i32;
    let disp_rules = rules.disposition_rules(&a.disposition, &b.disposition);
    for r in disp_rules {
        delta += r.delta;
        if let Some(c) = &r.cause {
            cause.push_str("; ");
            cause.push_str(c);
        }
    }
    if disp_rules.is_empty() {
        delta += default_disposition_delta(&a.disposition, &b.disposition);
    }

    // 4) Deterministic perturbation derived from the pair.
    let discriminator = format!("{}:{}{reroll_suffix}", a.id, b.id);
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
    build_relation(a, b, stance, cause, cooccur, idx, rel_override)
}

fn build_relation(
    a: &GeneratedFaction,
    b: &GeneratedFaction,
    base_stance: Stance,
    mut cause: String,
    cooccur: &BTreeMap<(u32, u32), CooccurStats>,
    idx: &FxMap<&str, u32>,
    rel_override: Option<&RelationOverride>,
) -> FactionRelation {
    let stats = cooccur_stats(cooccur, idx, &a.id, &b.id);
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
        tension: tension_of(a, b, secret_stance, cooccur, idx),
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

pub(super) fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Allocation-free canonical key for the co-occurrence map, keyed on faction
/// indices instead of id strings (B6). Order-independent, like `canonical_pair`.
pub(super) fn canonical_pair_idx(a: u32, b: u32) -> (u32, u32) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn normalized_power(v: f32) -> f32 {
    (v.max(0.0).ln_1p() * 12.0).clamp(0.0, 100.0)
}

fn clamp_score(v: f32) -> u8 {
    v.round().clamp(0.0, 100.0) as u8
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
    let parsed: super::config::RelationsFile = toml::from_str(&text)
        .map_err(|e| SectorError::config_parse(path.as_str(), e.to_string()))?;
    Ok(parsed.relations)
}
