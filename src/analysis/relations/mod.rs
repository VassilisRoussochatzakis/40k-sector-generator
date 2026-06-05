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
    DirectionalRelation, DispositionRule, FactionRelation, KindRule, PairOverride, RelationAttitude,
    RelationMetrics, RelationOverride, RelationsConfig, RelationsFile, RelationsMatrix,
    RelationsReport, Stance, TreatyStatus,
};
pub use derive::{derive, derive_with, derive_with_threshold, load_relations_file};
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
}
