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
    build_system_with_bias_reroll(
        config,
        pool,
        names,
        system_index,
        coord,
        used_system_names,
        anomaly_bias,
        "",
    )
}

/// Like [`build_system_with_bias`] but folds a re-roll suffix into both the
/// per-system `("system",<sys_id>)` and per-world `("world",<world_id>)` RNG
/// discriminators (worlds are folded under `Stage::Systems` for v1). An empty
/// suffix reproduces the legacy keys byte-for-byte; `":r{n}"` yields a
/// deterministic distinct system + world payload.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_system_with_bias_reroll(
    config: &AppConfig,
    pool: &WorldCandidatePool,
    names: &NameTables,
    system_index: usize,
    coord: HexCoord,
    used_system_names: &mut BTreeSet<String>,
    anomaly_bias: bool,
    reroll_suffix: &str,
) -> Result<GeneratedSystem, SectorError> {
    let sys_id = ids::system_id(system_index);
    let mut sys_rng = rng::stage_rng(
        &config.generation.seed,
        "system",
        &format!("{sys_id}{reroll_suffix}"),
    );

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
        reroll_suffix,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic, determinism-invariant RNG for the empty-/single-pool
    /// name tests. Always built via `crate::rng::stage_rng` (stage-keyed via
    /// blake3) — never `thread_rng`/`seed_from_u64`.
    fn test_rng() -> ChaCha8Rng {
        crate::rng::stage_rng("gen-base-name-test", "system", "test")
    }

    // GAP 169: `deduplicate_name` Roman-suffix behaviour.
    //   - empty used set → base returned unchanged (no suffix)
    //   - base already used → first free suffix is "II" (n starts at 2)
    //   - base + "Cadia II" used → next free is "III"
    #[test]
    fn deduplicate_name_cases() {
        // Empty used set → base returned unchanged.
        let empty: BTreeSet<String> = BTreeSet::new();
        assert_eq!(deduplicate_name("Cadia".to_string(), &empty), "Cadia");

        // Base already used → first free Roman suffix is II (n starts at 2).
        let used: BTreeSet<String> = ["Cadia".to_string()].into_iter().collect();
        assert_eq!(deduplicate_name("Cadia".to_string(), &used), "Cadia II");

        // Base and "Cadia II" used → next free is III.
        let used2: BTreeSet<String> = ["Cadia".to_string(), "Cadia II".to_string()]
            .into_iter()
            .collect();
        assert_eq!(deduplicate_name("Cadia".to_string(), &used2), "Cadia III");
    }

    // GAP 170: `generate_base_name` branch coverage. Single-element pools make
    // each branch deterministic regardless of the exact RNG draw:
    // `rng.gen_range(0..1)` is always 0, so the picked element is fixed.

    #[test]
    fn generate_base_name_empty_pools_fallback() {
        // Both pools empty ⇒ `format!("System {index}")`.
        let names = NameTables::default();
        let mut rng = test_rng();
        assert_eq!(generate_base_name(&names, 7, &mut rng), "System 7");
    }

    #[test]
    fn generate_base_name_singles_only() {
        let mut names = NameTables::default();
        names.system_names.single_names = vec!["Solaris".to_string()];
        let mut rng = test_rng();
        // have_singles=true, have_pairs=false ⇒ pick single_names[gen_range(0..1)]
        // = single_names[0] = "Solaris", deterministic for any RNG state.
        assert_eq!(generate_base_name(&names, 3, &mut rng), "Solaris");
    }

    #[test]
    fn generate_base_name_pairs_only() {
        let mut names = NameTables::default();
        names.system_names.prefixes = vec!["Alpha".to_string()];
        names.system_names.suffixes = vec!["Prime".to_string()];
        let mut rng = test_rng();
        // have_singles=false, have_pairs=true ⇒ format!("{} {}", prefix[0], suffix[0]).
        assert_eq!(generate_base_name(&names, 5, &mut rng), "Alpha Prime");
    }
}
