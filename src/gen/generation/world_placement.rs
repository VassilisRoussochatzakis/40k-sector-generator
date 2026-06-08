//! Per-world generation: candidate selection, feature picking, naming, tags.
//!
//! Named `world_placement` to avoid path-confusion with top-level
//! [`crate::worlds`]. Despite the name, this submodule covers the full payload
//! pipeline for an individual world — picking a row, attaching features, naming
//! it, and deriving its tag set.

use std::collections::BTreeSet;
use std::sync::Arc;

use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::config::{AppConfig, WorldSelectionConfig};
use crate::errors::SectorError;
use crate::ids;
use crate::names::{roman_numeral, NameTables};
use crate::rng::{self, weighted_index};
use crate::sector_model::{GeneratedWorld, WorldControlSummary, WorldDto};
use crate::taxonomy;
use crate::world_pool::{WorldCandidate, WorldCandidatePool};
use crate::worlds::{NotableFeature, StarColour};

pub(super) struct WorldGenParams<'a> {
    pub config: &'a AppConfig,
    pub pool: &'a WorldCandidatePool,
    pub names: &'a NameTables,
    pub system_index: usize,
    pub system_name: &'a str,
    pub star_colour: StarColour,
    pub sys_rng: &'a mut ChaCha8Rng,
    pub root_seed: &'a str,
    pub anomaly_bias: bool,
    /// Re-roll suffix folded into the per-world `("world",<world_id>)` RNG
    /// discriminator. Empty (the default) reproduces the legacy key
    /// byte-for-byte; `":r{n}"` yields a deterministic distinct world payload.
    /// Worlds are folded under `Stage::Systems` for v1, so this carries the
    /// `Stage::Systems` suffix.
    pub reroll_suffix: &'a str,
}

pub(super) fn generate_worlds_for_system(
    params: WorldGenParams,
) -> Result<Vec<GeneratedWorld>, SectorError> {
    let WorldGenParams {
        config,
        pool,
        names,
        system_index,
        system_name,
        star_colour,
        sys_rng,
        root_seed,
        anomaly_bias,
        reroll_suffix,
    } = params;
    let min_w = config.generation.min_worlds_per_system;
    let max_w = config.generation.max_worlds_per_system;
    let world_count = if min_w == max_w {
        min_w
    } else {
        sys_rng.gen_range(min_w..=max_w)
    };

    let mut used_world_types: BTreeSet<crate::worlds::WorldType> = BTreeSet::new();
    let mut worlds: Vec<GeneratedWorld> = Vec::with_capacity(world_count);

    for w_idx in 1..=world_count {
        let world_id = ids::world_id(system_index, w_idx);
        let mut w_rng = rng::stage_rng(root_seed, "world", &format!("{world_id}{reroll_suffix}"));

        let cand = choose_world_candidate(
            pool,
            star_colour,
            &config.generation.world_selection,
            &used_world_types,
            &mut w_rng,
            anomaly_bias,
        )?;

        if config
            .generation
            .world_selection
            .avoid_duplicate_world_type_in_system
        {
            used_world_types.insert(cand.world_type.clone());
        }

        let features = pick_features(
            cand,
            pool,
            config.generation.world_feature_count,
            star_colour,
            &mut w_rng,
        );
        let world = cand.to_world(features.clone());
        let world_dto = WorldDto::from(&world);
        let tags = tags_for_world(&world);

        let name = pick_world_name(names, system_name, w_idx, &mut w_rng);

        worlds.push(GeneratedWorld {
            id: world_id,
            index: w_idx,
            name: name.into(),
            orbit: w_idx as u8,
            source_row_index: cand.row_index,
            world: world_dto,
            factions: Vec::new(),
            tags,
            notes: Vec::new(),
            claims: Vec::new(),
            control: WorldControlSummary::default(),
            stability: crate::stability::StabilityState::default(),
            regions: Vec::new(),
            conflict: Default::default(),
            intel: Default::default(),
        });
    }
    // Sort by orbit for stable output.
    worlds.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(worlds)
}

fn choose_world_candidate<'a>(
    pool: &'a WorldCandidatePool,
    star_colour: StarColour,
    cfg: &WorldSelectionConfig,
    used_world_types: &BTreeSet<crate::worlds::WorldType>,
    rng: &mut ChaCha8Rng,
    anomaly_bias: bool,
) -> Result<&'a WorldCandidate, SectorError> {
    let collect = |skip_dup: bool| -> Vec<(&'a WorldCandidate, f64)> {
        pool.candidates
            .iter()
            .filter_map(|c| {
                if cfg.strict_same_star_colour && c.star_colour != star_colour {
                    return None;
                }
                if skip_dup
                    && cfg.avoid_duplicate_world_type_in_system
                    && used_world_types.contains(&c.world_type)
                {
                    return None;
                }
                let mut w = c.weight;
                if c.star_colour == star_colour {
                    w *= cfg.same_star_colour_bias.max(0.0);
                }
                if anomaly_bias && is_anomaly_friendly(c.primary_feature.as_ref()) {
                    w *= 3.0;
                }
                (w.is_finite() && w > 0.0).then_some((c, w))
            })
            .collect()
    };

    let mut weighted = collect(true);
    if weighted.is_empty() {
        // Fallback: ignore avoid_duplicate constraint to keep generation moving.
        weighted = collect(false);
    }
    if weighted.is_empty() {
        return Err(SectorError::WeightedSelectionFailed {
            context: "world_candidate".to_string(),
        });
    }
    let idx = weighted_index(&weighted, rng, "world_candidate")?;
    Ok(weighted[idx].0)
}

/// §5 NEW.md: features that the `Anomaly` region condition should bias
/// candidates toward (warp phenomena, ancient ruins, daemonic taint).
fn is_anomaly_friendly(feature: Option<&NotableFeature>) -> bool {
    matches!(
        feature,
        Some(
            NotableFeature::WarpPhenomena
                | NotableFeature::DaemonicCorruption
                | NotableFeature::CelestialPhenomena
                | NotableFeature::ArchaeotechRuins
                | NotableFeature::AncientArchive
                | NotableFeature::AncientTombs
                | NotableFeature::XenoRuins
                | NotableFeature::SealedMenace
        )
    )
}

fn pick_features(
    cand: &WorldCandidate,
    pool: &WorldCandidatePool,
    target_count: usize,
    star_colour: StarColour,
    rng: &mut ChaCha8Rng,
) -> Vec<NotableFeature> {
    let mut chosen: Vec<NotableFeature> = Vec::with_capacity(target_count);
    let mut seen: BTreeSet<NotableFeature> = BTreeSet::new();

    if let Some(f) = &cand.primary_feature {
        chosen.push(f.clone());
        seen.insert(f.clone());
    }

    let tiers: [&[crate::world_pool::WeightedFeature]; 3] = [
        pool.feature_pool
            .by_world_type
            .get(&cand.world_type)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        pool.feature_pool
            .by_star_colour
            .get(&star_colour)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        pool.feature_pool.global.as_slice(),
    ];

    for tier in tiers {
        if chosen.len() >= target_count {
            break;
        }
        let mut filtered: Vec<(NotableFeature, f64)> = tier
            .iter()
            .filter(|wf| !seen.contains(&wf.feature))
            .map(|wf| (wf.feature.clone(), wf.weight))
            .collect();
        while chosen.len() < target_count && !filtered.is_empty() {
            let idx = match weighted_index(&filtered, rng, "feature") {
                Ok(i) => i,
                Err(_) => break,
            };
            let (feature, _) = filtered.remove(idx);
            if seen.insert(feature.clone()) {
                chosen.push(feature);
            }
        }
    }

    if chosen.len() < target_count {
        let key_tier: Vec<(NotableFeature, f64)> = pool
            .feature_pool
            .key_table_features
            .iter()
            .filter(|f| !seen.contains(*f))
            .map(|f| (f.clone(), 1.0))
            .collect();
        let mut filtered = key_tier;
        while chosen.len() < target_count && !filtered.is_empty() {
            let idx = match weighted_index(&filtered, rng, "feature") {
                Ok(i) => i,
                Err(_) => break,
            };
            let (feature, _) = filtered.remove(idx);
            if seen.insert(feature.clone()) {
                chosen.push(feature);
            }
        }
    }

    chosen
}

fn pick_world_name(
    names: &NameTables,
    system_name: &str,
    world_index: usize,
    rng: &mut ChaCha8Rng,
) -> String {
    let pool = &names.world_names;
    let have_roots = !pool.roots.is_empty();

    if !have_roots {
        let pattern = &names.location_names.fallback_pattern;
        return pattern
            .replace("{system_name}", system_name)
            .replace("{roman}", &roman_numeral(world_index));
    }

    let root = pool.roots[rng.gen_range(0..pool.roots.len())].clone();
    let prefix = if !pool.prefixes.is_empty() && rng.gen::<f64>() < 0.4 {
        Some(pool.prefixes[rng.gen_range(0..pool.prefixes.len())].clone())
    } else {
        None
    };
    let suffix = if !pool.suffixes.is_empty() && rng.gen::<f64>() < 0.35 {
        Some(pool.suffixes[rng.gen_range(0..pool.suffixes.len())].clone())
    } else {
        None
    };

    match (prefix, suffix) {
        (Some(p), Some(s)) => format!("{p} {root} {s}"),
        (Some(p), None) => format!("{p} {root}"),
        (None, Some(s)) => format!("{root} {s}"),
        (None, None) => root,
    }
}

fn tags_for_world(world: &crate::worlds::World) -> Vec<Arc<str>> {
    let snake = |s: &str| taxonomy::to_snake_case(s);
    let mut tags: Vec<Arc<str>> = vec![
        format!("world_type:{}", snake(&world.world_type.to_string())).into(),
        format!("atmosphere:{}", snake(&world.atmosphere.to_string())).into(),
        format!("temperature:{}", snake(&world.temperature.to_string())).into(),
        format!("biosphere:{}", snake(&world.biosphere.to_string())).into(),
        format!("population:{}", snake(&world.population.to_string())).into(),
        format!("tech:{}", snake(&world.tech_level.to_string())).into(),
        format!("gov:{}", snake(&world.government.to_string())).into(),
        format!(
            "star:{}",
            snake(taxonomy::star_colour_variant_name(world.star_colour))
        )
        .into(),
    ];
    for f in &world.notable_features {
        tags.push(format!("feature:{}", snake(f.as_ref())).into());
    }
    tags.sort();
    tags
}

/// §W4: Re-roll a single world's payload (`WorldDto`, source row index, tags)
/// against the supplied candidate pool. Pure: depends only on the seed plus the
/// inputs. `seed_discriminator` lets the caller bump the per-world stream so
/// repeated clicks yield distinct outcomes without disturbing the rest of the
/// sector. Star colour is taken from the parent system.
pub fn regenerate_world_payload(
    config: &AppConfig,
    pool: &WorldCandidatePool,
    star_colour: StarColour,
    world_id: &str,
    seed_discriminator: u64,
) -> Result<(WorldDto, usize, Vec<Arc<str>>), SectorError> {
    let discriminator = format!("reroll:{world_id}:{seed_discriminator}");
    let mut rng = rng::stage_rng(&config.generation.seed, "world", &discriminator);
    let used_world_types: BTreeSet<crate::worlds::WorldType> = BTreeSet::new();
    let cand = choose_world_candidate(
        pool,
        star_colour,
        &config.generation.world_selection,
        &used_world_types,
        &mut rng,
        false,
    )?;
    let features = pick_features(
        cand,
        pool,
        config.generation.world_feature_count,
        star_colour,
        &mut rng,
    );
    let world = cand.to_world(features);
    let dto = WorldDto::from(&world);
    let tags = tags_for_world(&world);
    Ok((dto, cand.row_index, tags))
}

#[cfg(test)]
mod tests {
    use super::*;

    // GAP 173: `tags_for_world` emits one namespaced tag per the 8 component
    // prefixes plus one `feature:<snake>` per notable feature, sorted. Every
    // component enum's Display is `{self:?}` (the variant name), then
    // `taxonomy::to_snake_case` converts CamelCase → snake_case. World has no
    // Default, so each field is constructed by name (in-crate, so the enums'
    // `#[non_exhaustive]` does not block construction here).
    #[test]
    fn tags_for_world_sorted_namespaced() {
        use crate::worlds::*;
        let world = World {
            star_colour: StarColour::White,
            world_type: WorldType::HiveWorld,
            atmosphere: Atmosphere::Breathable,
            temperature: Temperature::Temperate,
            biosphere: Biosphere::Thriving,
            population: Population::Uninhabited,
            tech_level: TechLevel::Standard,
            government: Government::None,
            notable_features: vec![NotableFeature::AncientArchive, NotableFeature::XenoRuins],
        };
        let tags: Vec<String> = tags_for_world(&world)
            .iter()
            .map(|t| t.to_string())
            .collect();

        let expected = vec![
            "atmosphere:breathable".to_string(),
            "biosphere:thriving".to_string(),
            "feature:ancient_archive".to_string(),
            "feature:xeno_ruins".to_string(),
            "gov:none".to_string(),
            "population:uninhabited".to_string(),
            "star:white".to_string(),
            "tech:standard".to_string(),
            "temperature:temperate".to_string(),
            "world_type:hive_world".to_string(),
        ];
        assert_eq!(tags, expected);

        // The sorted invariant the gap calls out, explicitly.
        let mut sorted = tags.clone();
        sorted.sort();
        assert_eq!(tags, sorted, "tags must be sorted");

        // 8 base prefixes + 2 feature tags.
        assert_eq!(tags.len(), 10, "8 base prefixes + 2 features");
        assert_eq!(
            tags.iter().filter(|t| t.starts_with("feature:")).count(),
            2
        );

        // Each of the 8 base namespaces is present exactly once.
        for ns in [
            "world_type:",
            "atmosphere:",
            "temperature:",
            "biosphere:",
            "population:",
            "tech:",
            "gov:",
            "star:",
        ] {
            assert_eq!(
                tags.iter().filter(|t| t.starts_with(ns)).count(),
                1,
                "missing/dup namespace {ns}"
            );
        }
    }
}
