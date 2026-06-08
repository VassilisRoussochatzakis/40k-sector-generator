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
//!
//! Split into submodules (§B11): [`config`] holds the `Stance` enum, the
//! `relations.toml` schema, and the serialized output DTOs; [`tables`] the
//! built-in kind/ideology data + classifiers; [`derive`] the entry points, the
//! per-pair derivation pipeline, and the loader; [`tension`] the co-occurrence
//! walk + tension scalar; and [`render`] the markdown + report writer. The
//! public surface is re-exported flat here so the `relations::` path is
//! unchanged.

mod config;
mod derive;
mod render;
mod tables;
mod tension;

pub use config::{
    DirectionalRelation, DispositionRule, FactionRelation, KindRule, PairOverride,
    RelationAttitude, RelationMetrics, RelationOverride, RelationsConfig, RelationsFile,
    RelationsMatrix, RelationsReport, Stance, TreatyStatus,
};
pub use derive::{
    derive, derive_with, derive_with_threshold, derive_with_threshold_reroll, load_relations_file,
};
pub use render::{render_markdown, write_report};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        GeneratedFaction, GeneratedSector, GenerationManifest, PowerProfile,
    };
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

    /// B3: the rule-index must preserve the linear scan's semantics —
    /// `kind_rules` is first-match-wins, `disposition_rules` sums every match
    /// and concatenates causes in config order. The `cause` string observes all
    /// three independently of the seed-derived stance perturbation.
    #[test]
    fn user_rules_index_preserves_first_match_and_sum_order() {
        let mut cfg = RelationsConfig::default();
        // Two kind rules match the (imperial, imperial) pair — the FIRST wins.
        cfg.kind_rules.push(KindRule {
            a: "imperial".into(),
            b: "imperial".into(),
            stance: Stance::Rival,
            cause: Some("KFIRST".into()),
        });
        cfg.kind_rules.push(KindRule {
            a: "imperial".into(),
            b: "imperial".into(),
            stance: Stance::AtWar,
            cause: Some("KSECOND".into()),
        });
        // Two disposition rules match the (lawful, lawful) pair — both apply, in
        // order; delta 0 keeps the stance (and thus the test) perturbation-proof.
        cfg.disposition_rules.push(DispositionRule {
            a: "lawful".into(),
            b: "lawful".into(),
            delta: 0,
            cause: Some("D1".into()),
        });
        cfg.disposition_rules.push(DispositionRule {
            a: "lawful".into(),
            b: "lawful".into(),
            delta: 0,
            cause: Some("D2".into()),
        });
        let m = derive_with(
            &sector_with(vec![
                faction("a", "imperial", "lawful"),
                faction("b", "imperial", "lawful"),
            ]),
            &cfg,
        );
        assert_eq!(m.pairs[0].cause, "KFIRST; D1; D2");
    }

    /// A `GeneratedFaction` with a non-default `PowerProfile`. The `faction()`
    /// builder hardcodes `PowerProfile::default()`; C4 needs asymmetric power so
    /// directional metrics diverge.
    fn faction_with_power(
        id: &str,
        kind: &str,
        disposition: &str,
        power: PowerProfile,
    ) -> GeneratedFaction {
        GeneratedFaction {
            id: id.into(),
            name: id.into(),
            kind: kind.into(),
            disposition: disposition.into(),
            subfactions: Vec::new(),
            system_presence: vec![],
            world_presence: vec![],
            power,
        }
    }

    fn pair_of<'a>(m: &'a RelationsMatrix, a: &str, b: &str) -> &'a FactionRelation {
        m.pairs
            .iter()
            .find(|p| p.a == a && p.b == b)
            .unwrap_or_else(|| panic!("pair {a}/{b} missing"))
    }

    // ── C1: pair_override + rich override on the SAME pair ─────────────────────

    /// A `pair_override` pins the base stance, but a matching rich `override`
    /// still overlays attitudes/metrics/cause on top.
    #[test]
    fn pair_override_and_rich_override_compose_on_same_pair() {
        let mut cfg = RelationsConfig::default();
        cfg.pair_overrides.push(PairOverride {
            a: "imp".into(),
            b: "trader".into(),
            stance: Stance::Allied,
            cause: Some("pinned".into()),
        });
        cfg.overrides.push(RelationOverride {
            a: "imp".into(),
            b: "trader".into(),
            secret_attitude: Some(RelationAttitude::Hostile),
            trust: Some(5),
            reason: Some("rich wins".into()),
            ..RelationOverride::default()
        });
        let m = derive_with(
            &sector_with(vec![
                faction("imp", "imperial", "lawful"),
                faction("trader", "merchant", "opportunistic"),
            ]),
            &cfg,
        );
        let rel = pair_of(&m, "imp", "trader");
        // Rich override pins secret attitude (symmetric ⇒ both views Hostile).
        assert_eq!(rel.secret_attitude, RelationAttitude::Hostile);
        assert_eq!(rel.stance, Stance::Hostile);
        // trust set to 5 on both views ⇒ pair mean (5+5)/2 = 5.
        assert_eq!(rel.metrics.trust, 5);
        // ov.reason overrides the pair_override's cause.
        assert_eq!(rel.cause, "rich wins");
        assert_eq!(rel.a, "imp");
        assert_eq!(rel.b, "trader");
    }

    // ── C2: apply_relation_override IF-branch (config a == canonical-lo) ────────

    /// When the override's `a` equals the canonical-LO id, directional attitudes
    /// map straight through (`a_* → a_to_b`, `b_* → b_to_a`) — the opposite
    /// wiring from the existing ELSE-branch test.
    #[test]
    fn directional_override_if_branch_maps_a_to_a_to_b() {
        let mut cfg = RelationsConfig::default();
        cfg.overrides.push(RelationOverride {
            a: "alpha".into(),
            b: "zeta".into(),
            a_secret_attitude: Some(RelationAttitude::Hostile),
            b_secret_attitude: Some(RelationAttitude::Suspicious),
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

    // ── C3: apply_relation_override metric clamp to 100 ────────────────────────

    #[test]
    fn rich_override_clamps_metric_at_100_on_both_views_and_pair() {
        let mut cfg = RelationsConfig::default();
        cfg.overrides.push(RelationOverride {
            a: "imp".into(),
            b: "trader".into(),
            trust: Some(250),
            ..RelationOverride::default()
        });
        let m = derive_with(
            &sector_with(vec![
                faction("imp", "imperial", "lawful"),
                faction("trader", "merchant", "opportunistic"),
            ]),
            &cfg,
        );
        let rel = pair_of(&m, "imp", "trader");
        assert_eq!(rel.a_to_b.metrics.trust, 100);
        assert_eq!(rel.b_to_a.metrics.trust, 100);
        // pair-level trust is the mean of the two clamped views ⇒ 100.
        assert_eq!(rel.metrics.trust, 100);
    }

    // ── C4: combine_metrics asymmetric mean-vs-max rule ────────────────────────

    /// `combine_metrics` averages `trust` but takes the MAX of every other
    /// metric. Asymmetric faction power makes the directional `military_pressure`
    /// values diverge, so the two rules are observably different.
    #[test]
    fn combine_metrics_uses_mean_for_trust_and_max_for_the_rest() {
        let strong = faction_with_power(
            "strong",
            "imperial",
            "lawful",
            PowerProfile {
                military: 900.0,
                ..Default::default()
            },
        );
        let weak = faction_with_power("weak", "imperial", "lawful", PowerProfile::default());
        let m = derive_with(
            &sector_with(vec![strong, weak]),
            &RelationsConfig::default(),
        );
        let rel = pair_of(&m, "strong", "weak");
        // military_pressure depends on `to.power`, so the two directions diverge
        // (perturbation only shifts the stance/attitude, not this metric).
        assert_ne!(
            rel.a_to_b.metrics.military_pressure,
            rel.b_to_a.metrics.military_pressure
        );
        // The pair-level value is the MAX of the two directional values.
        assert_eq!(
            rel.metrics.military_pressure,
            rel.a_to_b
                .metrics
                .military_pressure
                .max(rel.b_to_a.metrics.military_pressure)
        );
        // Trust is the truncated MEAN of the two directional values.
        let expected_trust =
            ((u16::from(rel.a_to_b.metrics.trust) + u16::from(rel.b_to_a.metrics.trust)) / 2) as u8;
        assert_eq!(rel.metrics.trust, expected_trust);
    }

    // ── C5: treaty_status_of derived branches (pinned stance, no rich override) ─

    /// Pin a base stance with a `pair_override` (no rich override) so the derived
    /// `treaty_status` is observable and perturbation cannot flip the branch.
    fn derived_treaty(kind_a: &str, kind_b: &str, stance: Stance) -> TreatyStatus {
        let mut cfg = RelationsConfig::default();
        cfg.pair_overrides.push(PairOverride {
            a: "fa".into(),
            b: "fb".into(),
            stance,
            cause: None,
        });
        let m = derive_with(
            &sector_with(vec![
                faction("fa", kind_a, "lawful"),
                faction("fb", kind_b, "lawful"),
            ]),
            &cfg,
        );
        m.pairs[0].treaty_status
    }

    #[test]
    fn treaty_status_imperial_merchant_aligned_is_charter() {
        assert_eq!(
            derived_treaty("imperial", "merchant", Stance::Aligned),
            TreatyStatus::Charter
        );
    }

    #[test]
    fn treaty_status_imperial_mechanicus_aligned_is_pact() {
        // IMPERIAL_KINDS includes "mechanicus", so the MERCHANT cross is false and
        // the [mechanicus] cross fires ⇒ Pact.
        assert_eq!(
            derived_treaty("imperial", "mechanicus", Stance::Aligned),
            TreatyStatus::Pact
        );
    }

    #[test]
    fn treaty_status_atwar_is_vendetta() {
        assert_eq!(
            derived_treaty("imperial", "chaos_space_marine", Stance::AtWar),
            TreatyStatus::Vendetta
        );
    }

    #[test]
    fn treaty_status_same_kind_hostile_no_warzone_is_nonaggression() {
        // Two imperial factions pinned Hostile; zero co-occurrence ⇒
        // active_warzones == 0 ⇒ Nonaggression (checked before the cross_kinds
        // Charter/Pact branches).
        assert_eq!(
            derived_treaty("imperial", "imperial", Stance::Hostile),
            TreatyStatus::Nonaggression
        );
    }

    // ── C6: Stance::shift saturation ───────────────────────────────────────────

    #[test]
    fn stance_shift_saturates_at_bounds() {
        // level -2 + -5 = -7 → clamp(-2,3) = -2 → Allied.
        assert_eq!(Stance::Allied.shift(-5), Stance::Allied);
        // level 3 + 10 = 13 → clamp = 3 → AtWar.
        assert_eq!(Stance::AtWar.shift(10), Stance::AtWar);
        // level 0 + 2 = 2 → Hostile.
        assert_eq!(Stance::Neutral.shift(2), Stance::Hostile);
        // boundary sanity.
        assert_eq!(Stance::Neutral.shift(0), Stance::Neutral);
        assert_eq!(Stance::Allied.shift(1), Stance::Aligned);
    }

    // ── derive_with_threshold: empty-presence fallback to all factions ─────────

    /// Synthetic factions have empty world+system presence, so any threshold's
    /// filter empties the set and the derivation falls back to ALL factions ⇒
    /// exactly C(n, 2) pairs.
    #[test]
    fn derive_with_threshold_empty_presence_falls_back_to_all_pairs() {
        let sector = sector_with(vec![
            faction("a", "imperial", "lawful"),
            faction("b", "mechanicus", "insular"),
            faction("c", "chaos_space_marine", "hostile"),
        ]);
        let m = derive_with_threshold(&sector, &RelationsConfig::default(), 2);
        assert_eq!(m.pairs.len(), 3);
    }
}
