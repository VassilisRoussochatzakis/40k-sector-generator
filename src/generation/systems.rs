//! Per-system construction: star colour roll, name selection, world list build.

use std::collections::BTreeSet;

use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::config::AppConfig;
use crate::errors::SectorError;
use crate::ids;
use crate::names::{roman_numeral, NameTables};
use crate::rng::{self, weighted_index};
use crate::sector_model::{GeneratedStar, GeneratedSystem, HexCoord, SystemControlSummary};
use crate::world_pool::WorldCandidatePool;
use crate::worlds::StarColour;

use super::world_placement::{generate_worlds_for_system, WorldGenParams};

/// Build one fully populated `GeneratedSystem` for a given index and coordinate.
/// Pure: depends only on the seed in `config.generation.seed` plus the inputs
/// passed in. Used by sector generation and by standalone single-system
/// generation.
pub fn build_system(
    config: &AppConfig,
    pool: &WorldCandidatePool,
    names: &NameTables,
    system_index: usize,
    coord: HexCoord,
    used_system_names: &mut BTreeSet<String>,
) -> Result<GeneratedSystem, SectorError> {
    build_system_with_bias(
        config,
        pool,
        names,
        system_index,
        coord,
        used_system_names,
        false,
    )
}

pub fn build_system_with_bias(
    config: &AppConfig,
    pool: &WorldCandidatePool,
    names: &NameTables,
    system_index: usize,
    coord: HexCoord,
    used_system_names: &mut BTreeSet<String>,
    anomaly_bias: bool,
) -> Result<GeneratedSystem, SectorError> {
    let sys_id = ids::system_id(system_index);
    let mut sys_rng = rng::stage_rng(&config.generation.seed, "system", &sys_id);

    let star_colour = choose_system_star_colour(pool, &mut sys_rng)?;
    let name = pick_system_name(names, system_index, &mut sys_rng, used_system_names);
    used_system_names.insert(name.clone());

    let worlds = generate_worlds_for_system(WorldGenParams {
        config,
        pool,
        names,
        system_index,
        system_name: &name,
        star_colour,
        sys_rng: &mut sys_rng,
        root_seed: &config.generation.seed,
        anomaly_bias,
    })?;

    Ok(GeneratedSystem {
        id: sys_id,
        index: system_index,
        name: name.into(),
        coord,
        kind: crate::sector_model::SystemKind::Star,
        star: Some(GeneratedStar {
            colour_code: star_colour.code().to_string().into(),
            colour_name: star_colour.short_name().to_string().into(),
            spectral_type: Some(spectral_type_fallback(star_colour).to_string().into()),
            source_row_index: worlds.first().map(|w| w.source_row_index),
        }),
        worlds,
        primary_factions: Vec::new(),
        tags: Vec::new(),
        notes: Vec::new(),
        control: SystemControlSummary::default(),
        stability: crate::stability::StabilityState::default(),
        orbital_assets: Vec::new(),
        blockade: Default::default(),
        conflict: Default::default(),
        intel: Default::default(),
        archetype: Default::default(),
    })
}

fn choose_system_star_colour(
    pool: &WorldCandidatePool,
    rng: &mut ChaCha8Rng,
) -> Result<StarColour, SectorError> {
    if pool.star_colour_weights.is_empty() {
        return Err(SectorError::NoWorldCandidates);
    }
    let idx = weighted_index(&pool.star_colour_weights, rng, "system_star_colour")?;
    Ok(pool.star_colour_weights[idx].0)
}

fn pick_system_name(
    names: &NameTables,
    index: usize,
    rng: &mut ChaCha8Rng,
    used: &BTreeSet<String>,
) -> String {
    let base = generate_base_name(names, index, rng);
    deduplicate_name(base, used)
}

fn generate_base_name(names: &NameTables, index: usize, rng: &mut ChaCha8Rng) -> String {
    let sys = &names.system_names;
    let have_singles = !sys.single_names.is_empty();
    let have_pairs = !sys.prefixes.is_empty() && !sys.suffixes.is_empty();

    if !have_singles && !have_pairs {
        return format!("System {index}");
    }
    if !have_pairs {
        let i = rng.gen_range(0..sys.single_names.len());
        return sys.single_names[i].clone();
    }
    if !have_singles {
        let pi = rng.gen_range(0..sys.prefixes.len());
        let si = rng.gen_range(0..sys.suffixes.len());
        return format!("{} {}", sys.prefixes[pi], sys.suffixes[si]);
    }
    // Both pools available — flip a coin.
    if rng.gen::<bool>() {
        let i = rng.gen_range(0..sys.single_names.len());
        sys.single_names[i].clone()
    } else {
        let pi = rng.gen_range(0..sys.prefixes.len());
        let si = rng.gen_range(0..sys.suffixes.len());
        format!("{} {}", sys.prefixes[pi], sys.suffixes[si])
    }
}

fn deduplicate_name(base: String, used: &BTreeSet<String>) -> String {
    if !used.contains(&base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{} {}", base, roman_numeral(n));
        if !used.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn spectral_type_fallback(sc: StarColour) -> &'static str {
    match sc {
        StarColour::BlueHypergiant => "O",
        StarColour::BlueWhite => "B",
        StarColour::White => "A",
        StarColour::YellowWhite => "F",
        StarColour::Yellow => "G",
        StarColour::OrangeDwarf => "K",
        StarColour::RedDwarf => "M",
    }
}
