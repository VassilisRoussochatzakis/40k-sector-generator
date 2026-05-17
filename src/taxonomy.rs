//! Maps variant-name strings (used in user configs and tags) to worlds.rs enums.
//!
//! worlds.rs FromStr already accepts the *display* names (e.g. "Hive World").
//! Configs in this app use *variant* names (e.g. "HiveWorld"). Helpers here
//! bridge both directions and provide stable snake_case tag forms.

use crate::worlds::{
    Atmosphere, Biosphere, Government, NotableFeature, Population, StarColour, TechLevel,
    Temperature, WorldType,
};

/// Convert a CamelCase variant name like "HiveWorld" to "hive_world".
pub fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            let prev = name.chars().nth(i - 1).unwrap();
            if !prev.is_uppercase() {
                out.push('_');
            }
        }
        out.extend(ch.to_lowercase());
    }
    out
}

pub fn star_colour_variant_name(sc: StarColour) -> &'static str {
    match sc {
        StarColour::BlueHypergiant => "BlueHypergiant",
        StarColour::BlueWhite => "BlueWhite",
        StarColour::White => "White",
        StarColour::YellowWhite => "YellowWhite",
        StarColour::Yellow => "Yellow",
        StarColour::OrangeDwarf => "OrangeDwarf",
        StarColour::RedDwarf => "RedDwarf",
    }
}

pub fn parse_star_colour_variant(s: &str) -> Option<StarColour> {
    match s {
        "BlueHypergiant" => Some(StarColour::BlueHypergiant),
        "BlueWhite" => Some(StarColour::BlueWhite),
        "White" => Some(StarColour::White),
        "YellowWhite" => Some(StarColour::YellowWhite),
        "Yellow" => Some(StarColour::Yellow),
        "OrangeDwarf" => Some(StarColour::OrangeDwarf),
        "RedDwarf" => Some(StarColour::RedDwarf),
        _ => None,
    }
}

pub fn all_star_colours() -> &'static [StarColour] {
    &[
        StarColour::BlueHypergiant,
        StarColour::BlueWhite,
        StarColour::White,
        StarColour::YellowWhite,
        StarColour::Yellow,
        StarColour::OrangeDwarf,
        StarColour::RedDwarf,
    ]
}

pub fn parse_world_type_variant(s: &str) -> Option<WorldType> {
    Some(match s {
        "AgriWorld" => WorldType::AgriWorld,
        "Asteroid" => WorldType::Asteroid,
        "BastionWorld" => WorldType::BastionWorld,
        "DeathWorld" => WorldType::DeathWorld,
        "DeadWorld" => WorldType::DeadWorld,
        "ExtractiveColony" => WorldType::ExtractiveColony,
        "FeralWorld" => WorldType::FeralWorld,
        "FeudalWorld" => WorldType::FeudalWorld,
        "ForgeWorld" => WorldType::ForgeWorld,
        "FrontierWorld" => WorldType::FrontierWorld,
        "HiveWorld" => WorldType::HiveWorld,
        "IndustrialWorld" => WorldType::IndustrialWorld,
        "Orbital" => WorldType::Orbital,
        "PenalWorld" => WorldType::PenalWorld,
        "PlanetaryDump" => WorldType::PlanetaryDump,
        "PlanetaryMonument" => WorldType::PlanetaryMonument,
        "PleasureWorld" => WorldType::PleasureWorld,
        "ResearchStation" => WorldType::ResearchStation,
        "ShrineWorld" => WorldType::ShrineWorld,
        "TombWorld" => WorldType::TombWorld,
        "WarpLostWorld" => WorldType::WarpLostWorld,
        "Worldship" => WorldType::Worldship,
        "XenosWorld" => WorldType::XenosWorld,
        _ => return None,
    })
}

pub fn parse_government_variant(s: &str) -> Option<Government> {
    Some(match s {
        "BalkanizedLocalFactions" => Government::BalkanizedLocalFactions,
        "ChaosCult" => Government::ChaosCult,
        "ClansTribes" => Government::ClansTribes,
        "Communards" => Government::Communards,
        "CorruptAristocrats" => Government::CorruptAristocrats,
        "Demagogue" => Government::Demagogue,
        "EcclesiarchicalAppointee" => Government::EcclesiarchicalAppointee,
        "ElitistTyrant" => Government::ElitistTyrant,
        "ExploratorAuthority" => Government::ExploratorAuthority,
        "GuildsCombine" => Government::GuildsCombine,
        "Hereteks" => Government::Hereteks,
        "HereticalImperialCult" => Government::HereticalImperialCult,
        "InfractionistGang" => Government::InfractionistGang,
        "LocalReligiousAuthorities" => Government::LocalReligiousAuthorities,
        "LoyalistMassMovement" => Government::LoyalistMassMovement,
        "MagistrateCouncil" => Government::MagistrateCouncil,
        "MechanicusForgeLord" => Government::MechanicusForgeLord,
        "Megacorporations" => Government::Megacorporations,
        "MilitaryGovernor" => Government::MilitaryGovernor,
        "None" => Government::None,
        "PopulistTyrant" => Government::PopulistTyrant,
        "PuppetGovernment" => Government::PuppetGovernment,
        "RevolutionaryJunta" => Government::RevolutionaryJunta,
        "RogueTraderDynasty" => Government::RogueTraderDynasty,
        "ShadowyPsykerCabal" => Government::ShadowyPsykerCabal,
        "TraditionalOligarchy" => Government::TraditionalOligarchy,
        "TraditionalistAristocracy" => Government::TraditionalistAristocracy,
        "Warlords" => Government::Warlords,
        "WarriorAristocracy" => Government::WarriorAristocracy,
        "XenosOverlords" => Government::XenosOverlords,
        _ => return None,
    })
}

pub fn parse_notable_feature_variant(s: &str) -> Option<NotableFeature> {
    Some(match s {
        "Abhumans" => NotableFeature::Abhumans,
        "AlteredHumans" => NotableFeature::AlteredHumans,
        "AncientArchive" => NotableFeature::AncientArchive,
        "AncientTombs" => NotableFeature::AncientTombs,
        "ArchaeotechRuins" => NotableFeature::ArchaeotechRuins,
        "BlindingMists" => NotableFeature::BlindingMists,
        "CelestialPhenomena" => NotableFeature::CelestialPhenomena,
        "ChaosCultists" => NotableFeature::ChaosCultists,
        "CivilWar" => NotableFeature::CivilWar,
        "ColdWar" => NotableFeature::ColdWar,
        "CrumblingArcologies" => NotableFeature::CrumblingArcologies,
        "DaemonicCorruption" => NotableFeature::DaemonicCorruption,
        "DangerousWildlife" => NotableFeature::DangerousWildlife,
        "DesertWorld" => NotableFeature::DesertWorld,
        "DeviantReligion" => NotableFeature::DeviantReligion,
        "EugenicCult" => NotableFeature::EugenicCult,
        "ExtremeEnvironment" => NotableFeature::ExtremeEnvironment,
        "FactionalFragmentation" => NotableFeature::FactionalFragmentation,
        "FailedParadise" => NotableFeature::FailedParadise,
        "FlyingCities" => NotableFeature::FlyingCities,
        "ForbiddenTech" => NotableFeature::ForbiddenTech,
        "ForeignControl" => NotableFeature::ForeignControl,
        "FreakGeology" => NotableFeature::FreakGeology,
        "FreakWeather" => NotableFeature::FreakWeather,
        "Freeport" => NotableFeature::Freeport,
        "FriendlyXenos" => NotableFeature::FriendlyXenos,
        "FrozenWorld" => NotableFeature::FrozenWorld,
        "GoldRush" => NotableFeature::GoldRush,
        "GreatWork" => NotableFeature::GreatWork,
        "HeavyIndustry" => NotableFeature::HeavyIndustry,
        "HeavyMining" => NotableFeature::HeavyMining,
        "Hereteks" => NotableFeature::Hereteks,
        "HolyWar" => NotableFeature::HolyWar,
        "HostileBiosphere" => NotableFeature::HostileBiosphere,
        "HostileXenos" => NotableFeature::HostileXenos,
        "ImpendingDoom" => NotableFeature::ImpendingDoom,
        "ImperialKnights" => NotableFeature::ImperialKnights,
        "ImportantShrine" => NotableFeature::ImportantShrine,
        "InquisitionOutpost" => NotableFeature::InquisitionOutpost,
        "JungleWorld" => NotableFeature::JungleWorld,
        "Libertines" => NotableFeature::Libertines,
        "LocalSpecialty" => NotableFeature::LocalSpecialty,
        "LocalTech" => NotableFeature::LocalTech,
        "MajorSpaceyard" => NotableFeature::MajorSpaceyard,
        "MartialLaw" => NotableFeature::MartialLaw,
        "MassPanic" => NotableFeature::MassPanic,
        "MinimalContact" => NotableFeature::MinimalContact,
        "Missionaries" => NotableFeature::Missionaries,
        "MutantHordes" => NotableFeature::MutantHordes,
        "NavalBlockade" => NotableFeature::NavalBlockade,
        "NavalOutpost" => NotableFeature::NavalOutpost,
        "NavigatorHouse" => NotableFeature::NavigatorHouse,
        "NomadicCities" => NotableFeature::NomadicCities,
        "NotableLocal" => NotableFeature::NotableLocal,
        "OceanWorld" => NotableFeature::OceanWorld,
        "OutOfContact" => NotableFeature::OutOfContact,
        "Pandemic" => NotableFeature::Pandemic,
        "PilgrimageSite" => NotableFeature::PilgrimageSite,
        "PocketEmpire" => NotableFeature::PocketEmpire,
        "PoliceState" => NotableFeature::PoliceState,
        "PopularUprising" => NotableFeature::PopularUprising,
        "PowerfulCriminals" => NotableFeature::PowerfulCriminals,
        "PowerfulNobles" => NotableFeature::PowerfulNobles,
        "PrimitiveXenos" => NotableFeature::PrimitiveXenos,
        "Prosperous" => NotableFeature::Prosperous,
        "PsykerAcademy" => NotableFeature::PsykerAcademy,
        "PsykerCult" => NotableFeature::PsykerCult,
        "Quarantined" => NotableFeature::Quarantined,
        "Radioactive" => NotableFeature::Radioactive,
        "RecentlyRediscovered" => NotableFeature::RecentlyRediscovered,
        "ScholaProgenium" => NotableFeature::ScholaProgenium,
        "SeagoingCities" => NotableFeature::SeagoingCities,
        "SealedMenace" => NotableFeature::SealedMenace,
        "SecretMasters" => NotableFeature::SecretMasters,
        "Sectarians" => NotableFeature::Sectarians,
        "SeismicInstability" => NotableFeature::SeismicInstability,
        "Separatists" => NotableFeature::Separatists,
        "SilicaAnimus" => NotableFeature::SilicaAnimus,
        "SoleSuppliers" => NotableFeature::SoleSuppliers,
        "SororitasConvent" => NotableFeature::SororitasConvent,
        "SpaceHulks" => NotableFeature::SpaceHulks,
        "StrangeCustoms" => NotableFeature::StrangeCustoms,
        "StrangeHatred" => NotableFeature::StrangeHatred,
        "SubsectorHegemon" => NotableFeature::SubsectorHegemon,
        "TechPriestCult" => NotableFeature::TechPriestCult,
        "TestSite" => NotableFeature::TestSite,
        "TheSilentTrade" => NotableFeature::TheSilentTrade,
        "TradeHub" => NotableFeature::TradeHub,
        "AdministrativeHub" => NotableFeature::AdministrativeHub,
        "UnmappedWastes" => NotableFeature::UnmappedWastes,
        "VastFortresses" => NotableFeature::VastFortresses,
        "VerdantEcology" => NotableFeature::VerdantEcology,
        "WarZone" => NotableFeature::WarZone,
        "WarpPhenomena" => NotableFeature::WarpPhenomena,
        "WitchHunt" => NotableFeature::WitchHunt,
        "XenoRuins" => NotableFeature::XenoRuins,
        "Xenophiles" => NotableFeature::Xenophiles,
        "Xenophobes" => NotableFeature::Xenophobes,
        "XenosInfiltrators" => NotableFeature::XenosInfiltrators,
        "Zombies" => NotableFeature::Zombies,
        _ => return None,
    })
}

/// Display impls in worlds.rs use the Debug variant name (e.g. "HiveWorld"),
/// so format!("{}", x) works for all enums *except* StarColour. These helpers
/// provide a unified accessor.
pub fn world_type_name(w: &WorldType) -> String {
    format!("{w}")
}

pub fn atmosphere_name(a: &Atmosphere) -> String {
    format!("{a}")
}

pub fn temperature_name(t: &Temperature) -> String {
    format!("{t}")
}

pub fn biosphere_name(b: &Biosphere) -> String {
    format!("{b}")
}

pub fn population_name(p: &Population) -> String {
    format!("{p}")
}

pub fn tech_level_name(t: &TechLevel) -> String {
    format!("{t}")
}

pub fn government_name(g: &Government) -> String {
    format!("{g}")
}

pub fn notable_feature_name(f: &NotableFeature) -> String {
    format!("{f}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_handles_camel() {
        assert_eq!(to_snake_case("HiveWorld"), "hive_world");
        assert_eq!(to_snake_case("ExtremelyDense"), "extremely_dense");
        assert_eq!(to_snake_case("TradeHub"), "trade_hub");
        assert_eq!(to_snake_case("None"), "none");
    }

    #[test]
    fn variant_name_round_trip_for_world_type() {
        let v = WorldType::HiveWorld;
        let name = world_type_name(&v);
        let parsed = parse_world_type_variant(&name).unwrap();
        assert_eq!(parsed, v);
    }
}
