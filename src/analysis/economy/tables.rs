//! Built-in production / strategic-output tables: the default per-`world_type`
//! resource and strategic vectors, tech/population multipliers, the per-feature
//! rules, and the helpers that fold those rules into a `StrategicOutput`. Users
//! override these via `economy.toml`.

use super::config::{ResourceVector, StrategicOutput, StrategicOutputRule};
use crate::worlds::{Population, TechLevel};

// ── Built-in vectors (positive = surplus, negative = deficit) ──────────────────

pub(super) fn default_world_type_vector(world_type: &str) -> ResourceVector {
    match world_type {
        "HiveWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: -40.0,
            manufactured: 40.0,
            archeotech: 0.0,
            recruits: 60.0,
        },
        "ForgeWorld" => ResourceVector {
            ore: -20.0,
            promethium: 0.0,
            foodstuffs: -20.0,
            manufactured: 80.0,
            archeotech: 10.0,
            recruits: 5.0,
        },
        "AgriWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 80.0,
            manufactured: -10.0,
            archeotech: 0.0,
            recruits: 10.0,
        },
        "DeathWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 0.0,
            manufactured: 0.0,
            archeotech: 5.0,
            recruits: 40.0,
        },
        "FeudalWorld" | "FeralWorld" => ResourceVector {
            ore: 5.0,
            promethium: 0.0,
            foodstuffs: 20.0,
            manufactured: -20.0,
            archeotech: 0.0,
            recruits: 25.0,
        },
        "ShrineWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: -10.0,
            manufactured: 0.0,
            archeotech: 0.0,
            recruits: 20.0,
        },
        "PleasureWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: -10.0,
            manufactured: -10.0,
            archeotech: 0.0,
            recruits: 5.0,
        },
        "DeadWorld" | "WarpLostWorld" => ResourceVector::default(),
        _ => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 0.0,
            manufactured: 0.0,
            archeotech: 0.0,
            recruits: 0.0,
        },
    }
}

pub(super) fn default_tech_multiplier(tech: &str) -> f32 {
    match tech {
        "Primitive" => 0.2,
        _ => 1.0,
    }
}

pub(super) fn default_population_multiplier(pop_tag: &str) -> f32 {
    // tag form: "population:massive" etc.
    match pop_tag {
        "population:massive" => 1.5,
        "population:large" | "population:huge" => 1.3,
        "population:standard" => 1.0,
        "population:sole_settlement" | "population:lightly_populated" => 0.5,
        "population:minimal" => 0.25,
        "population:uninhabited" => 0.0,
        _ => 1.0,
    }
}

pub(super) fn default_strategic_world_type(world_type: &str) -> StrategicOutput {
    match world_type {
        "AgriWorld" => StrategicOutput {
            food: 90.0,
            manpower: 20.0,
            manufacturing: 5.0,
            ..Default::default()
        },
        "ForgeWorld" => StrategicOutput {
            ore: 5.0,
            manufacturing: 85.0,
            arms: 75.0,
            ships: 45.0,
            knowledge: 35.0,
            manpower: 15.0,
            ..Default::default()
        },
        "IndustrialWorld" => StrategicOutput {
            manufacturing: 70.0,
            arms: 45.0,
            ships: 20.0,
            manpower: 35.0,
            ..Default::default()
        },
        "ExtractiveColony" | "Asteroid" => StrategicOutput {
            ore: 80.0,
            manufacturing: 10.0,
            manpower: 10.0,
            ..Default::default()
        },
        "HiveWorld" => StrategicOutput {
            food: 5.0,
            manufacturing: 45.0,
            arms: 25.0,
            manpower: 85.0,
            psyker_tithe: 30.0,
            knowledge: 20.0,
            ..Default::default()
        },
        "ShrineWorld" => StrategicOutput {
            food: 10.0,
            pilgrimage: 85.0,
            manpower: 25.0,
            knowledge: 20.0,
            ..Default::default()
        },
        "ResearchStation" => StrategicOutput {
            knowledge: 80.0,
            xenos_value: 20.0,
            manufacturing: 10.0,
            ..Default::default()
        },
        "BastionWorld" => StrategicOutput {
            arms: 55.0,
            ships: 25.0,
            manpower: 60.0,
            manufacturing: 25.0,
            ..Default::default()
        },
        "PenalWorld" => StrategicOutput {
            manpower: 45.0,
            ore: 25.0,
            ..Default::default()
        },
        "DeathWorld" | "FeralWorld" | "FeudalWorld" => StrategicOutput {
            food: 25.0,
            manpower: 45.0,
            ore: 10.0,
            ..Default::default()
        },
        "PleasureWorld" => StrategicOutput {
            pilgrimage: 45.0,
            knowledge: 10.0,
            ..Default::default()
        },
        "TombWorld" => StrategicOutput {
            knowledge: 25.0,
            xenos_value: 90.0,
            ore: 20.0,
            ..Default::default()
        },
        "XenosWorld" | "Worldship" => StrategicOutput {
            xenos_value: 75.0,
            knowledge: 30.0,
            ships: 20.0,
            ..Default::default()
        },
        "DeadWorld" | "WarpLostWorld" | "PlanetaryDump" => StrategicOutput::default(),
        _ => StrategicOutput {
            food: 20.0,
            ore: 15.0,
            manufacturing: 15.0,
            manpower: 20.0,
            knowledge: 10.0,
            ..Default::default()
        },
    }
}

pub(super) fn strategic_tech_multiplier(tech: &TechLevel) -> f32 {
    match tech {
        TechLevel::Primitive => 0.35,
        TechLevel::Low => 0.60,
        TechLevel::Standard => 1.00,
        TechLevel::High => 1.20,
        TechLevel::XenoHybrid => 1.10,
        TechLevel::Archaeotech => 1.45,
    }
}

pub(super) fn strategic_population_multiplier(pop: &Population) -> f32 {
    match pop {
        Population::Uninhabited => 0.0,
        Population::Minimal => 0.25,
        Population::LightlyPopulated => 0.50,
        Population::SoleSettlement => 0.55,
        Population::DenselyPopulated => 1.00,
        Population::ExtremelyDense => 1.35,
    }
}

pub(super) fn default_feature_rule(feature: &str) -> Option<StrategicOutputRule> {
    let mut r = StrategicOutputRule::default();
    match feature {
        "HeavyMining" | "FreakGeology" | "GoldRush" => r.ore = Some(25.0),
        "HeavyIndustry" | "GreatWork" | "LocalTech" => r.manufacturing = Some(20.0),
        "MajorSpaceyard" => {
            r.ships = Some(35.0);
            r.manufacturing = Some(15.0);
            r.supply_resilience = Some(10.0);
        }
        "VastFortresses" | "MartialLaw" | "ImperialKnights" => {
            r.arms = Some(20.0);
            r.manpower = Some(15.0);
        }
        "ImportantShrine" | "PilgrimageSite" | "Missionaries" | "SororitasConvent" => {
            r.pilgrimage = Some(25.0);
        }
        "PsykerAcademy" | "PsykerCult" => r.psyker_tithe = Some(30.0),
        "AncientArchive" | "ArchaeotechRuins" | "ForbiddenTech" => {
            r.knowledge = Some(25.0);
            r.xenos_value = Some(10.0);
        }
        "XenoRuins" | "AncientTombs" | "SealedMenace" => r.xenos_value = Some(35.0),
        "TradeHub" | "Freeport" | "AdministrativeHub" | "SubsectorHegemon" => {
            r.trade_multiplier = Some(1.25);
            r.supply_resilience = Some(15.0);
        }
        "VerdantEcology" | "OceanWorld" | "JungleWorld" => r.food = Some(20.0),
        "WarZone" | "NavalBlockade" | "CivilWar" | "Pandemic" | "Quarantined" => {
            r.supply_resilience = Some(-15.0);
        }
        _ => return None,
    }
    Some(r)
}

pub(super) fn apply_world_type_rule(
    mut base: StrategicOutput,
    rule: &StrategicOutputRule,
) -> StrategicOutput {
    for (f, v) in base.fields_mut().into_iter().zip(rule.scores()) {
        if let Some(v) = v {
            *f = v;
        }
    }
    base
}

pub(super) fn apply_feature_rule(out: &mut StrategicOutput, rule: &StrategicOutputRule) {
    for (f, v) in out.fields_mut().into_iter().zip(rule.scores()) {
        *f += v.unwrap_or(0.0);
    }
}
