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
    use crate::sector_model::{
        GeneratedRoute, GeneratedSector, GeneratedStar, GeneratedSystem, GeneratedWorld,
        GenerationManifest, HexCoord, RouteStability, RouteType, SystemControlSummary,
        WorldControlSummary, WorldDto,
    };
    use crate::worlds::{
        Atmosphere, Biosphere, Government, Population, StarColour, Temperature, TechLevel, WorldType,
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
}
