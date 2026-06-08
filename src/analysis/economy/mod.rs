//! Trade, tithe, and strategic resource layer (§12 NEW.md + §4 NEW2.md).
//!
//! Pure read-only derivation over the finished sector: each world declares a
//! production/consumption vector (keyed by `world_type` × `tech_level` ×
//! population scale, mapped from the existing `tags`), routes carry derived
//! trade volume = function of endpoint surplus/deficit gradient × distance
//! falloff × `RouteStability` × per-faction `RouteControl` interference.
//! No new RNG draws — same sector ⇒ same numbers.
//!
//! Default production tables ship in this module; users may override or
//! extend them in `economy.toml` (referenced by `inputs.economy` in
//! `sectorforge.toml`).
//!
//! Split into submodules (§B11): [`config`] holds the schema/DTO types and
//! tunables, [`tables`] the built-in production data, [`derive`] the core
//! walk + dependency-edge derivation + loader, [`risk`] the supply/tithe
//! classifiers, and [`render`] the markdown + report writer. The public surface
//! is re-exported flat here so the `economy::` path is unchanged.

mod config;
mod derive;
mod render;
mod risk;
mod tables;

pub use config::{
    DependencyEdge, EconomyConfig, EconomyFile, EconomyReport, ResourceModelConfig, ResourceVector,
    RouteEconomy, StrategicOutput, StrategicOutputRule, StrategicPriority, SupplyRisk,
    SystemEconomy, TitheStatus, WorldEconomy, RESOURCE_KEYS, STRATEGIC_RESOURCE_KEYS,
};
pub use derive::{apply_stability_nudge, derive, derive_with, load_economy_file};
pub use render::{render_markdown, write_report};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_control::RouteControl;
    use crate::sector_model::{
        GeneratedRoute, GeneratedSector, GeneratedStar, GeneratedSystem, GeneratedWorld,
        GenerationManifest, HexCoord, RouteStability, RouteType, SystemControlSummary,
        WorldControlSummary, WorldDto,
    };
    use crate::stability::StabilityState;
    use crate::worlds::{
        Atmosphere, Biosphere, Government, Population, StarColour, TechLevel, Temperature,
        WorldType,
    };
    use std::collections::BTreeMap as Map;

    fn world(id: &str, world_type: WorldType, tech: TechLevel, pop_tag: &str) -> GeneratedWorld {
        GeneratedWorld {
            id: id.into(),
            index: 1,
            name: id.into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: StarColour::Yellow,
                world_type,
                atmosphere: Atmosphere::Breathable,
                temperature: Temperature::Temperate,
                biosphere: Biosphere::Thriving,
                population: Population::DenselyPopulated,
                tech_level: tech,
                government: Government::MilitaryGovernor,
                notable_features: vec![],
            },
            factions: vec![],
            tags: vec![pop_tag.into()],
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: Default::default(),
            regions: vec![],
            conflict: Default::default(),
            intel: Default::default(),
        }
    }

    fn sys(id: &str, worlds: Vec<GeneratedWorld>) -> GeneratedSystem {
        GeneratedSystem {
            id: id.into(),
            index: 1,
            name: id.into(),
            kind: crate::sector_model::SystemKind::Star,
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

    fn sector(systems: Vec<GeneratedSystem>) -> GeneratedSector {
        GeneratedSector {
            id: "econ-test".into(),
            title: "Econ".into(),
            seed: "seed".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0".into(),
            width: 4,
            height: 4,
            systems,
            routes: vec![],
            factions: vec![],
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
            relations: Default::default(),
            regions: vec![].into(),
            economy: Default::default(),
            chronicle: Default::default(),
            id_history: Default::default(),
        }
    }

    fn route(id: &str, from: &str, to: &str) -> GeneratedRoute {
        GeneratedRoute {
            id: id.into(),
            from_system_id: from.into(),
            to_system_id: to.into(),
            distance: 1,
            route_type: RouteType::StableWarpLane,
            stability: RouteStability::Stable,
            tags: vec![],
            controls: vec![],
        }
    }

    #[test]
    fn disabled_yields_empty() {
        let s = sector(vec![sys(
            "sys-0001",
            vec![world(
                "wrld-0001-1",
                WorldType::HiveWorld,
                TechLevel::Standard,
                "population:massive",
            )],
        )]);
        let r = derive_with(&s, &EconomyConfig::default());
        assert!(!r.enabled);
        assert!(r.worlds.is_empty());
    }

    #[test]
    fn hive_world_food_deficit() {
        let s = sector(vec![sys(
            "sys-0001",
            vec![world(
                "wrld-0001-1",
                WorldType::HiveWorld,
                TechLevel::Standard,
                "population:massive",
            )],
        )]);
        let r = derive(&s);
        assert!(r.enabled);
        assert!(r.worlds[0].vector.foodstuffs < 0.0);
    }

    #[test]
    fn deterministic() {
        let s = sector(vec![
            sys(
                "sys-0001",
                vec![world(
                    "a",
                    WorldType::HiveWorld,
                    TechLevel::Standard,
                    "population:massive",
                )],
            ),
            sys(
                "sys-0002",
                vec![world(
                    "b",
                    WorldType::AgriWorld,
                    TechLevel::Standard,
                    "population:standard",
                )],
            ),
        ]);
        let a = derive(&s);
        let b = derive(&s);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn strategic_dependency_edges_link_food_supplier_to_hive() {
        let mut s = sector(vec![
            sys(
                "sys-0001",
                vec![world(
                    "agri",
                    WorldType::AgriWorld,
                    TechLevel::Standard,
                    "population:standard",
                )],
            ),
            sys(
                "sys-0002",
                vec![world(
                    "hive",
                    WorldType::HiveWorld,
                    TechLevel::Standard,
                    "population:massive",
                )],
            ),
        ]);
        s.routes = vec![route("route-1", "sys-0001", "sys-0002")];
        let r = derive(&s);
        assert!(r.strategic_output.food > 0.0);
        assert!(r.dependency_edges.iter().any(|e| {
            e.from_system_id == "sys-0001" && e.to_system_id == "sys-0002" && e.resource == "food"
        }));
        let hive = r.worlds.iter().find(|w| w.world_id == "hive").unwrap();
        assert!(hive.strategic_output.manpower > 0.0);
    }

    #[test]
    fn top_level_resources_block_merges_into_config() {
        let parsed: EconomyFile = toml::from_str(
            r#"
            [economy]
            enabled = true

            [resources.world_type.AgriWorld]
            food = 42
            manpower = 7
            "#,
        )
        .unwrap();
        let cfg = parsed.into_config();
        let rule = cfg.resources.world_type.get("AgriWorld").unwrap();
        assert_eq!(rule.food, Some(42.0));
        assert_eq!(rule.manpower, Some(7.0));
    }

    // ── E-group shared helpers ─────────────────────────────────────────────────

    /// `route()` with explicit `distance`, `stability`, and `controls`.
    fn route_full(
        id: &str,
        from: &str,
        to: &str,
        distance: u32,
        stability: RouteStability,
        controls: Vec<RouteControl>,
    ) -> GeneratedRoute {
        GeneratedRoute {
            id: id.into(),
            from_system_id: from.into(),
            to_system_id: to.into(),
            distance,
            route_type: RouteType::StableWarpLane,
            stability,
            tags: vec![],
            controls,
        }
    }

    // ── E1: apply_stability_nudge ──────────────────────────────────────────────

    #[test]
    fn apply_stability_nudge_bumps_stranded_world_famine() {
        // Lone HiveWorld: built-in foodstuffs -40 × pop massive 1.5 × tech 1.0 =
        // -60 ≤ -20, no routes ⇒ stranded.
        let mut s = sector(vec![sys(
            "sys-0001",
            vec![world(
                "wrld-0001-1",
                WorldType::HiveWorld,
                TechLevel::Standard,
                "population:massive",
            )],
        )]);
        let report = derive(&s);
        assert!(report.worlds[0].stranded, "precondition: world is stranded");
        apply_stability_nudge(&report, &mut s);
        assert!(
            (s.systems[0].worlds[0].stability.famine_or_resource_stress - 20.0).abs() < 1e-3,
            "famine = {}",
            s.systems[0].worlds[0].stability.famine_or_resource_stress
        );
    }

    #[test]
    fn apply_stability_nudge_clamps_famine_at_100() {
        let mut w = world(
            "wrld-0001-1",
            WorldType::HiveWorld,
            TechLevel::Standard,
            "population:massive",
        );
        w.stability.famine_or_resource_stress = 95.0;
        let mut s = sector(vec![sys("sys-0001", vec![w])]);
        let report = derive(&s);
        assert!(report.worlds[0].stranded);
        apply_stability_nudge(&report, &mut s);
        // 95 + 20 = 115 → clamp(0,100) = 100.
        assert_eq!(
            s.systems[0].worlds[0].stability.famine_or_resource_stress,
            100.0
        );
    }

    #[test]
    fn apply_stability_nudge_leaves_non_stranded_world_untouched() {
        // Lone AgriWorld: foodstuffs +80 (surplus); no resource ≤ -20 ⇒ not
        // stranded ⇒ famine stays 0.
        let mut s = sector(vec![sys(
            "sys-0001",
            vec![world(
                "wrld-0001-1",
                WorldType::AgriWorld,
                TechLevel::Standard,
                "population:massive",
            )],
        )]);
        let report = derive(&s);
        assert!(
            !report.worlds[0].stranded,
            "AgriWorld should not be stranded"
        );
        apply_stability_nudge(&report, &mut s);
        assert_eq!(
            s.systems[0].worlds[0].stability.famine_or_resource_stress,
            0.0
        );
    }

    #[test]
    fn apply_stability_nudge_noop_when_report_disabled() {
        let mut s = sector(vec![sys(
            "sys-0001",
            vec![world(
                "wrld-0001-1",
                WorldType::HiveWorld,
                TechLevel::Standard,
                "population:massive",
            )],
        )]);
        let before = serde_json::to_string(&s).unwrap();
        // EconomyReport::default() has enabled=false ⇒ early return, no mutation.
        let empty = EconomyReport::default();
        apply_stability_nudge(&empty, &mut s);
        let after = serde_json::to_string(&s).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn apply_stability_nudge_touches_only_famine_field() {
        let mut s = sector(vec![sys(
            "sys-0001",
            vec![world(
                "wrld-0001-1",
                WorldType::HiveWorld,
                TechLevel::Standard,
                "population:massive",
            )],
        )]);
        let report = derive(&s);
        apply_stability_nudge(&report, &mut s);
        let st = s.systems[0].worlds[0].stability;
        // Only famine_or_resource_stress is nudged; the other fields stay default.
        assert_eq!(
            StabilityState {
                famine_or_resource_stress: 0.0,
                ..st
            },
            StabilityState::default()
        );
    }

    // ── E2: friction_for (surfaced verbatim as RouteEconomy.friction) ──────────

    /// Build a two-system sector with one route of the given `stability` /
    /// `controls`, and read back the route's friction.
    fn friction_of(stability: RouteStability, controls: Vec<RouteControl>) -> f32 {
        let mut s = sector(vec![
            sys(
                "sys-0001",
                vec![world(
                    "wrld-0001-1",
                    WorldType::HiveWorld,
                    TechLevel::Standard,
                    "population:massive",
                )],
            ),
            sys(
                "sys-0002",
                vec![world(
                    "wrld-0002-1",
                    WorldType::AgriWorld,
                    TechLevel::Standard,
                    "population:standard",
                )],
            ),
        ]);
        s.routes = vec![route_full(
            "r1", "sys-0001", "sys-0002", 1, stability, controls,
        )];
        derive(&s).routes[0].friction
    }

    #[test]
    fn friction_for_stable_no_controls_is_one() {
        assert!((friction_of(RouteStability::Stable, vec![]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn friction_for_perilous_no_controls_is_point_one() {
        assert!((friction_of(RouteStability::Perilous, vec![]) - 0.10).abs() < 1e-6);
    }

    #[test]
    fn friction_for_patrol_exceeds_baseline() {
        // 1.0 × (1 + (100/400).clamp(0,0.25)) = 1.0 × 1.25 = 1.25.
        let f = friction_of(
            RouteStability::Stable,
            vec![RouteControl {
                patrol: 100.0,
                ..Default::default()
            }],
        );
        assert!((f - 1.25).abs() < 1e-6, "f = {f}");
    }

    #[test]
    fn friction_for_piracy_applies_malus() {
        // 1.0 × (1 - (100/200).clamp(0,0.5)) = 1.0 × 0.5 = 0.5.
        let f = friction_of(
            RouteStability::Stable,
            vec![RouteControl {
                piracy: 100.0,
                ..Default::default()
            }],
        );
        assert!((f - 0.5).abs() < 1e-6, "f = {f}");
    }

    #[test]
    fn friction_for_uses_max_across_controls() {
        // max_patrol over {100, 0} = 100 ⇒ identical to the single patrol=100 case.
        let two = friction_of(
            RouteStability::Stable,
            vec![
                RouteControl {
                    patrol: 100.0,
                    ..Default::default()
                },
                RouteControl {
                    patrol: 0.0,
                    ..Default::default()
                },
            ],
        );
        let one = friction_of(
            RouteStability::Stable,
            vec![RouteControl {
                patrol: 100.0,
                ..Default::default()
            }],
        );
        assert!((two - one).abs() < 1e-6);
        assert!((two - 1.25).abs() < 1e-6);
    }

    // ── E3: derive_dependency_edges (surfaced as EconomyReport.dependency_edges) ─

    #[test]
    fn dependency_edges_pick_higher_score_supplier() {
        // Consumer HiveWorld needs food; two AgriWorld suppliers with identical
        // output — A one jump away (friction 1.0, score = supply/1), B four jumps
        // away (score = supply/2) ⇒ A wins.
        let mut s = sector(vec![
            sys(
                "sys-a",
                vec![world(
                    "agri-a",
                    WorldType::AgriWorld,
                    TechLevel::Standard,
                    "population:standard",
                )],
            ),
            sys(
                "sys-b",
                vec![world(
                    "agri-b",
                    WorldType::AgriWorld,
                    TechLevel::Standard,
                    "population:standard",
                )],
            ),
            sys(
                "sys-cons",
                vec![world(
                    "hive",
                    WorldType::HiveWorld,
                    TechLevel::Standard,
                    "population:massive",
                )],
            ),
        ]);
        s.routes = vec![
            route_full(
                "r-a",
                "sys-cons",
                "sys-a",
                1,
                RouteStability::Stable,
                vec![],
            ),
            route_full(
                "r-b",
                "sys-cons",
                "sys-b",
                4,
                RouteStability::Stable,
                vec![],
            ),
        ];
        let r = derive(&s);
        let food_edges: Vec<&DependencyEdge> = r
            .dependency_edges
            .iter()
            .filter(|e| e.to_system_id == "sys-cons" && e.resource == "food")
            .collect();
        assert_eq!(
            food_edges.len(),
            1,
            "expected exactly one food edge to consumer"
        );
        assert_eq!(food_edges[0].from_system_id, "sys-a");
    }

    #[test]
    fn dependency_edges_skip_self_sufficient_or_needless_consumer() {
        // A lone AgriWorld system: AgriWorld has no strategic needs (and is food
        // self-sufficient anyway) ⇒ no inbound food edge.
        let mut s = sector(vec![
            sys(
                "sys-agri",
                vec![world(
                    "agri",
                    WorldType::AgriWorld,
                    TechLevel::Standard,
                    "population:standard",
                )],
            ),
            sys(
                "sys-other",
                vec![world(
                    "agri2",
                    WorldType::AgriWorld,
                    TechLevel::Standard,
                    "population:standard",
                )],
            ),
        ]);
        s.routes = vec![route_full(
            "r1",
            "sys-agri",
            "sys-other",
            1,
            RouteStability::Stable,
            vec![],
        )];
        let r = derive(&s);
        assert!(
            !r.dependency_edges
                .iter()
                .any(|e| e.to_system_id == "sys-agri" && e.resource == "food"),
            "AgriWorld consumer should have no inbound food edge"
        );
    }

    #[test]
    fn dependency_edges_exclude_perilous_only_route() {
        // HiveWorld consumer + AgriWorld supplier joined by a single Perilous
        // route: Perilous routes are dropped from the valid-route index ⇒ no
        // candidate ⇒ no edges at all.
        let mut s = sector(vec![
            sys(
                "sys-cons",
                vec![world(
                    "hive",
                    WorldType::HiveWorld,
                    TechLevel::Standard,
                    "population:massive",
                )],
            ),
            sys(
                "sys-sup",
                vec![world(
                    "agri",
                    WorldType::AgriWorld,
                    TechLevel::Standard,
                    "population:standard",
                )],
            ),
        ]);
        s.routes = vec![route_full(
            "r1",
            "sys-cons",
            "sys-sup",
            1,
            RouteStability::Perilous,
            vec![],
        )];
        let r = derive(&s);
        assert!(r.dependency_edges.is_empty());
    }

    #[test]
    fn dependency_edges_skip_supplier_below_threshold() {
        // ShrineWorld food output ≈ 10 < 35 ⇒ even routed to a hungry HiveWorld,
        // it cannot anchor a food dependency edge.
        let mut s = sector(vec![
            sys(
                "sys-cons",
                vec![world(
                    "hive",
                    WorldType::HiveWorld,
                    TechLevel::Standard,
                    "population:massive",
                )],
            ),
            sys(
                "sys-shrine",
                vec![world(
                    "shrine",
                    WorldType::ShrineWorld,
                    TechLevel::Standard,
                    "population:standard",
                )],
            ),
        ]);
        s.routes = vec![route_full(
            "r1",
            "sys-cons",
            "sys-shrine",
            1,
            RouteStability::Stable,
            vec![],
        )];
        let r = derive(&s);
        assert!(
            !r.dependency_edges
                .iter()
                .any(|e| e.from_system_id == "sys-shrine" && e.resource == "food"),
            "below-threshold shrine should not supply a food edge"
        );
    }

    #[test]
    fn dependency_edges_are_sorted_by_to_resource_from() {
        // Two HiveWorld consumers, each fed by its own AgriWorld supplier ⇒ two
        // food edges with distinct `to_system_id` ⇒ assert the (to, resource,
        // from) sort holds across the whole edge list.
        let mut s = sector(vec![
            sys(
                "sys-cons-b",
                vec![world(
                    "hive-b",
                    WorldType::HiveWorld,
                    TechLevel::Standard,
                    "population:massive",
                )],
            ),
            sys(
                "sys-cons-a",
                vec![world(
                    "hive-a",
                    WorldType::HiveWorld,
                    TechLevel::Standard,
                    "population:massive",
                )],
            ),
            sys(
                "sys-sup-a",
                vec![world(
                    "agri-a",
                    WorldType::AgriWorld,
                    TechLevel::Standard,
                    "population:standard",
                )],
            ),
            sys(
                "sys-sup-b",
                vec![world(
                    "agri-b",
                    WorldType::AgriWorld,
                    TechLevel::Standard,
                    "population:standard",
                )],
            ),
        ]);
        s.routes = vec![
            route_full(
                "r-a",
                "sys-cons-a",
                "sys-sup-a",
                1,
                RouteStability::Stable,
                vec![],
            ),
            route_full(
                "r-b",
                "sys-cons-b",
                "sys-sup-b",
                1,
                RouteStability::Stable,
                vec![],
            ),
        ];
        let r = derive(&s);
        assert!(r.dependency_edges.len() >= 2, "expected at least two edges");
        let keys: Vec<(String, String, String)> = r
            .dependency_edges
            .iter()
            .map(|e| {
                (
                    e.to_system_id.to_string(),
                    e.resource.clone(),
                    e.from_system_id.to_string(),
                )
            })
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(
            keys, sorted,
            "dependency edges not sorted by (to, resource, from)"
        );
    }
}
