//! Deterministic sector generation pipeline.

use std::collections::{BTreeMap, BTreeSet};

use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::config::{AppConfig, WorldSelectionConfig};
use crate::control;
use crate::errors::SectorError;
use crate::factions::FactionDef;
use crate::ids;
use crate::input::ProjectInput;
use crate::names::{roman_numeral, NameTables};
use crate::rng::{self, weighted_index};
use crate::routes::RouteRules;
use crate::sector_model::{
    hex_distance, DominanceState, FactionInfluence, GeneratedFaction, GeneratedRoute,
    GeneratedSector, GeneratedStar, GeneratedSystem, GeneratedWorld, GenerationManifest, HexCoord,
    RouteStability, RouteType, SystemControlSummary, WorldControlSummary, WorldDto,
    WorldFactionPresence,
};
use crate::taxonomy;
use crate::world_pool::{self, WorldCandidate, WorldCandidatePool};
use crate::worlds::{NotableFeature, StarColour};

pub fn generate(project: ProjectInput) -> Result<GeneratedSector, SectorError> {
    let ProjectInput {
        config,
        world_tables,
        world_rows,
        names,
        factions,
        route_rules,
        relations: relations_cfg,
        regions: regions_cfg,
        economy: economy_cfg,
        input_digests,
        ..
    } = project;

    let pool = world_pool::build_pool(
        &world_rows,
        &world_tables,
        &config.generation.world_selection,
    );
    if pool.candidates.is_empty() {
        return Err(SectorError::NoWorldCandidates);
    }

    let placements = place_systems(&config)?;
    let mut systems: Vec<GeneratedSystem> = Vec::with_capacity(placements.len());
    let mut used_names: BTreeSet<String> = BTreeSet::new();

    // §5 NEW.md: regions stage runs BEFORE world generation so the `Anomaly`
    // condition can reweight the per-system candidate pool toward
    // warp-phenomena / ancient-ruins candidates without changing other stages.
    let warp_regions = crate::regions::build_regions(
        &config.generation.seed,
        config.generation.sector_width,
        config.generation.sector_height,
        &regions_cfg,
    );
    let anomaly_hexes: BTreeSet<(i32, i32)> = warp_regions
        .iter()
        .filter(|r| matches!(r.kind, crate::regions::RegionConditionKind::Anomaly))
        .flat_map(|r| r.hexes.iter().map(|h| (h.q, h.r)))
        .collect();

    for (idx, coord) in placements.iter().enumerate() {
        let system_index = idx + 1;
        let anomaly_bias = anomaly_hexes.contains(&(coord.q, coord.r));
        let system = build_system_with_bias(
            &config,
            &pool,
            &names,
            system_index,
            *coord,
            &mut used_names,
            anomaly_bias,
        )?;
        systems.push(system);
    }

    // ── Factions ────────────────────────────────────────────────────────────
    if !factions.is_empty() {
        let mut faction_rng = rng::stage_rng(&config.generation.seed, "factions", "sector");
        assign_factions(&mut systems, &factions, &mut faction_rng);
    }

    let generated_factions = aggregate_factions(&systems, &factions);

    // ── Routes ──────────────────────────────────────────────────────────────
    let mut routes = if config.generation.routes.enabled {
        let mut route_rng = rng::stage_rng(&config.generation.seed, "routes", "sector");
        generate_routes(&config, &route_rules, &systems, &mut route_rng)
    } else {
        Vec::new()
    };

    // §5 NEW.md: apply region effects to routes (storm → perilous, turbulence
    // → one tier worse, calm corridor → one tier better up to the perilous
    // ceiling). Idempotent.
    if regions_cfg.apply_to_routes && !warp_regions.is_empty() {
        crate::regions::apply_route_effects(&warp_regions, &systems, &mut routes);
    }

    // §3 NEXT: append hidden route layers (webway / black-ship / smuggling)
    // before per-route control derivation so they receive the same control
    // treatment as public lanes.
    let mut generated_factions = generated_factions;
    if !generated_factions.is_empty() {
        crate::hidden_routes::append_hidden_routes_with_regions(
            &systems,
            &generated_factions,
            &warp_regions,
            &mut routes,
        );
    }

    // §3 per-route per-faction control. Derived after routes are built and
    // factions assigned, so endpoint presence reflects final state.
    if !routes.is_empty() && !generated_factions.is_empty() {
        let by_id: BTreeMap<&str, &GeneratedSystem> =
            systems.iter().map(|s| (s.id.as_str(), s)).collect();
        for r in &mut routes {
            r.controls =
                crate::route_control::derive_route_controls(r, &by_id, &generated_factions);
        }
    }

    // §1 NEXT: per-world surface regions.
    // §2 NEXT: per-system orbital assets + blockade detection.
    // §5 NEXT: per-world + per-system initial conflict state.
    // §7 NEXT: per-system fog-of-war intel records, observed by every
    // faction with at least one system-presence in the sector.
    let observer_ids: Vec<String> = generated_factions
        .iter()
        .filter(|f| !f.system_presence.is_empty())
        .map(|f| f.id.clone())
        .collect();
    for sys in systems.iter_mut() {
        for w in sys.worlds.iter_mut() {
            w.regions = crate::surface_region::derive_regions(w);
            w.conflict = crate::conflict::derive_world_conflict(w);
        }
        let (assets, blockade) = crate::orbital_assets::derive_orbital_assets(sys);
        sys.orbital_assets = assets;
        sys.blockade = blockade;
        sys.conflict = crate::conflict::derive_system_conflict(sys);
        let obs_refs: Vec<&str> = observer_ids.iter().map(|s| s.as_str()).collect();
        sys.intel = crate::intel::derive_system_intel(sys, &obs_refs);
    }

    // Sort everything for stable serialization.
    let mut sorted_systems = systems;
    sorted_systems.sort_by(|a, b| a.id.cmp(&b.id));
    let mut sorted_routes = routes;
    sorted_routes.sort_by(|a, b| a.id.cmp(&b.id));
    generated_factions.sort_by(|a, b| a.id.cmp(&b.id));

    let manifest = build_manifest(&config, &input_digests, &sorted_systems, &sorted_routes);

    let mut sector = GeneratedSector {
        id: config.project.id.clone(),
        title: config.project.title.clone(),
        seed: config.generation.seed.clone(),
        generator_name: crate::GENERATOR_NAME.to_string(),
        generator_version: crate::GENERATOR_VERSION.to_string(),
        width: config.generation.sector_width,
        height: config.generation.sector_height,
        systems: sorted_systems,
        routes: sorted_routes,
        factions: generated_factions,
        manifest,
        influence_field: Default::default(),
        power_projection: Default::default(),
        relations: Default::default(),
        regions: warp_regions,
        economy: Default::default(),
    };

    // §11 NEXT: archetype rules.
    crate::archetypes::apply_all(&mut sector);
    // §4 NEXT: power projection over routes (decays + doctrine).
    sector.power_projection = crate::power_projection::project_sector(&sector);
    crate::power_projection::apply_to_factions(&sector.power_projection, &mut sector.factions);
    // §9 NEXT: continuous area layers.
    sector.influence_field = crate::influence_field::build(&sector);

    // §4 NEW.md: derive inter-faction relationship matrix once factions are
    // finalised. Pure derivation, no extra RNG draws affect prior stages.
    sector.relations = crate::relations::derive_with(&sector, &relations_cfg);

    // §12 NEW.md: derive the economy snapshot last so it can read final
    // route stability + control records. Optional `feed_stability` nudge
    // applies after the snapshot is built.
    sector.economy = crate::economy::derive_with(&sector, &economy_cfg);
    if economy_cfg.feed_stability && sector.economy.enabled {
        let snap = sector.economy.clone();
        crate::economy::apply_stability_nudge(&snap, &mut sector);
    }

    Ok(sector)
}

// ── Placement ───────────────────────────────────────────────────────────────

fn place_systems(config: &AppConfig) -> Result<Vec<HexCoord>, SectorError> {
    let g = &config.generation;
    let width = g.sector_width as i32;
    let height = g.sector_height as i32;
    let total_cells = (width * height) as usize;

    let target = g.system_count.min(total_cells);
    if target == 0 {
        return Ok(Vec::new());
    }
    if target > total_cells {
        return Err(SectorError::InvalidConfig(format!(
            "system_count {} > grid cells {}",
            g.system_count, total_cells
        )));
    }

    let mut all: Vec<HexCoord> = Vec::with_capacity(total_cells);
    for r in 0..height {
        for q in 0..width {
            all.push(HexCoord { q, r });
        }
    }

    let mut rng = rng::stage_rng(&g.seed, "placement", "sector");
    // Fisher-Yates with rng.gen_range — deterministic given seed.
    for i in (1..all.len()).rev() {
        let j = rng.gen_range(0..=i);
        all.swap(i, j);
    }

    let mut placed: Vec<HexCoord> = Vec::with_capacity(target);
    let mut leftover: Vec<HexCoord> = Vec::new();
    let min_dist = g.placement.minimum_system_distance;
    for c in all {
        if placed.len() >= target {
            break;
        }
        if min_dist <= 1 || placed.iter().all(|p| hex_distance(*p, c) >= min_dist) {
            placed.push(c);
        } else {
            leftover.push(c);
        }
    }

    if placed.len() < target {
        // Couldn't satisfy minimum distance — relax constraint by progressively
        // shrinking it, still consuming the shuffled leftover pool so fill stays
        // spatially scattered rather than packed in grid order.
        let mut relaxed = min_dist;
        while placed.len() < target && relaxed > 1 {
            relaxed -= 1;
            let mut still_blocked: Vec<HexCoord> = Vec::new();
            for c in leftover.drain(..) {
                if placed.len() >= target {
                    still_blocked.push(c);
                    continue;
                }
                if relaxed <= 1 || placed.iter().all(|p| hex_distance(*p, c) >= relaxed) {
                    placed.push(c);
                } else {
                    still_blocked.push(c);
                }
            }
            leftover = still_blocked;
        }
        // Final fallback: any remaining shuffled cells.
        for c in leftover {
            if placed.len() >= target {
                break;
            }
            placed.push(c);
        }
    }

    // Sort so output ordering is deterministic regardless of shuffle order.
    placed.sort();
    Ok(placed)
}

// ── Per-system builder (used by sector and standalone APIs) ────────────────────

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

    let worlds = generate_worlds_for_system(
        config,
        pool,
        names,
        system_index,
        &name,
        star_colour,
        &mut sys_rng,
        &config.generation.seed,
        anomaly_bias,
    )?;

    Ok(GeneratedSystem {
        id: sys_id,
        index: system_index,
        name,
        coord,
        star: GeneratedStar {
            colour_code: star_colour.code().to_string(),
            colour_name: star_colour.short_name().to_string(),
            spectral_type: Some(spectral_type_fallback(star_colour).to_string()),
            source_row_index: worlds.first().map(|w| w.source_row_index),
        },
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

/// Apply faction assignment to one or more systems. Public so the standalone
/// system generator can reuse the same logic the sector generator does.
pub fn assign_factions_for_systems(
    systems: &mut [GeneratedSystem],
    factions: &[FactionDef],
    seed: &str,
    discriminator: &str,
) {
    if factions.is_empty() {
        return;
    }
    let mut rng = rng::stage_rng(seed, "factions", discriminator);
    assign_factions(systems, factions, &mut rng);
}

// ── System star colour ────────────────────────────────────────────────────────

fn choose_system_star_colour(
    pool: &WorldCandidatePool,
    rng: &mut ChaCha8Rng,
) -> Result<StarColour, SectorError> {
    let mut totals: BTreeMap<String, f64> = BTreeMap::new();
    for c in &pool.candidates {
        *totals
            .entry(taxonomy::star_colour_variant_name(c.star_colour).to_string())
            .or_insert(0.0) += c.weight;
    }
    let weighted: Vec<(StarColour, f64)> = totals
        .into_iter()
        .filter_map(|(name, w)| taxonomy::parse_star_colour_variant(&name).map(|sc| (sc, w)))
        .collect();
    if weighted.is_empty() {
        return Err(SectorError::NoWorldCandidates);
    }
    let idx = weighted_index(&weighted, rng, "system_star_colour")?;
    Ok(weighted[idx].0)
}

// ── System naming ─────────────────────────────────────────────────────────────

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

// ── World generation ──────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn generate_worlds_for_system(
    config: &AppConfig,
    pool: &WorldCandidatePool,
    names: &NameTables,
    system_index: usize,
    system_name: &str,
    star_colour: StarColour,
    sys_rng: &mut ChaCha8Rng,
    root_seed: &str,
    anomaly_bias: bool,
) -> Result<Vec<GeneratedWorld>, SectorError> {
    let min_w = config.generation.min_worlds_per_system;
    let max_w = config.generation.max_worlds_per_system;
    let world_count = if min_w == max_w {
        min_w
    } else {
        sys_rng.gen_range(min_w..=max_w)
    };

    let mut used_world_types: BTreeSet<String> = BTreeSet::new();
    let mut worlds: Vec<GeneratedWorld> = Vec::with_capacity(world_count);

    for w_idx in 1..=world_count {
        let world_id = ids::world_id(system_index, w_idx);
        let mut w_rng = rng::stage_rng(root_seed, "world", &world_id);

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
            used_world_types.insert(cand.world_type.to_string());
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
            name,
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
    used_world_types: &BTreeSet<String>,
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
                    && used_world_types.contains(&c.world_type.to_string())
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
    let mut seen: BTreeSet<String> = BTreeSet::new();

    if let Some(f) = &cand.primary_feature {
        chosen.push(f.clone());
        seen.insert(f.to_string());
    }

    let by_wt_key = cand.world_type.to_string();
    let by_sc_key = taxonomy::star_colour_variant_name(star_colour).to_string();

    let tiers: Vec<Vec<crate::world_pool::WeightedFeature>> = vec![
        pool.feature_pool
            .by_world_type
            .get(&by_wt_key)
            .cloned()
            .unwrap_or_default(),
        pool.feature_pool
            .by_star_colour
            .get(&by_sc_key)
            .cloned()
            .unwrap_or_default(),
        pool.feature_pool.global.clone(),
        pool.feature_pool
            .key_table_features
            .iter()
            .cloned()
            .map(|f| crate::world_pool::WeightedFeature {
                feature: f,
                weight: 1.0,
            })
            .collect(),
    ];

    for tier in tiers {
        if chosen.len() >= target_count {
            break;
        }
        let mut filtered: Vec<(NotableFeature, f64)> = tier
            .into_iter()
            .filter(|wf| !seen.contains(&wf.feature.to_string()))
            .map(|wf| (wf.feature, wf.weight))
            .collect();
        while chosen.len() < target_count && !filtered.is_empty() {
            let idx = match weighted_index(&filtered, rng, "feature") {
                Ok(i) => i,
                Err(_) => break,
            };
            let (feature, _) = filtered.remove(idx);
            if seen.insert(feature.to_string()) {
                chosen.push(feature);
            }
        }
    }

    chosen
}

// ── World naming ──────────────────────────────────────────────────────────────

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

// ── Tags ──────────────────────────────────────────────────────────────────────

fn tags_for_world(world: &crate::worlds::World) -> Vec<String> {
    let snake = |s: &str| taxonomy::to_snake_case(s);
    let mut tags: Vec<String> = vec![
        format!("world_type:{}", snake(&world.world_type.to_string())),
        format!("atmosphere:{}", snake(&world.atmosphere.to_string())),
        format!("temperature:{}", snake(&world.temperature.to_string())),
        format!("biosphere:{}", snake(&world.biosphere.to_string())),
        format!("population:{}", snake(&world.population.to_string())),
        format!("tech:{}", snake(&world.tech_level.to_string())),
        format!("gov:{}", snake(&world.government.to_string())),
        format!(
            "star:{}",
            snake(taxonomy::star_colour_variant_name(world.star_colour))
        ),
    ];
    for f in &world.notable_features {
        tags.push(format!("feature:{}", snake(&f.to_string())));
    }
    tags.sort();
    tags
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

// ── Factions ──────────────────────────────────────────────────────────────────

/// Spec §10.9: at most this many primary factions per system.
const PRIMARY_FACTION_LIMIT: usize = 3;

fn assign_factions(systems: &mut [GeneratedSystem], factions: &[FactionDef], rng: &mut ChaCha8Rng) {
    if factions.is_empty() {
        return;
    }
    assign_factions_inner(systems, factions, rng);
    // Post-pass: derive per-world claims + multi-winner snapshots, then roll up
    // to system-level state classification. Pure, deterministic.
    // Build a temporary catalog of GeneratedFactions for stability derivation —
    // stability only needs id→kind, so use the def list directly.
    let stability_factions: Vec<crate::sector_model::GeneratedFaction> = factions
        .iter()
        .map(|f| crate::sector_model::GeneratedFaction {
            id: f.id.clone(),
            name: f.name.clone(),
            kind: f.kind.clone(),
            disposition: f.default_disposition.clone(),
            system_presence: vec![],
            world_presence: vec![],
            power: crate::sector_model::PowerProfile::default(),
        })
        .collect();
    for sys in systems.iter_mut() {
        for world in &mut sys.worlds {
            world.claims = control::derive_world_claims(world);
            world.control = control::derive_world_control(world);
            world.stability = crate::stability::derive_world_stability(world, &stability_factions);
        }
        sys.control = control::derive_system_control(sys);
        sys.stability = crate::stability::derive_system_stability(sys);
    }
}

fn assign_factions_inner(
    systems: &mut [GeneratedSystem],
    factions: &[FactionDef],
    rng: &mut ChaCha8Rng,
) {
    if factions.is_empty() {
        return;
    }
    // Stable catalog order: ID-sorted index for deterministic tie-breaking.
    let catalog_order: BTreeMap<String, usize> = factions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id.clone(), i))
        .collect();

    for sys in systems.iter_mut() {
        // Per-system accumulator: faction_id -> (score, world_appearances)
        let mut scores: BTreeMap<String, (f64, usize)> = BTreeMap::new();
        for world in &mut sys.worlds {
            let pop_tag = world
                .tags
                .iter()
                .find(|t| t.starts_with("population:"))
                .cloned()
                .unwrap_or_default();
            let max_factions: usize = match pop_tag.as_str() {
                "population:uninhabited" => 0,
                "population:minimal" | "population:lightly_populated" => 1,
                "population:sole_settlement" => 2,
                _ => 3,
            };
            if max_factions == 0 {
                continue;
            }

            let mut weighted: Vec<(&FactionDef, f64)> = factions
                .iter()
                .map(|f| {
                    let mut w = f.weight;
                    if f.preferred_world_types
                        .iter()
                        .any(|s| s == &world.world.world_type)
                    {
                        w *= 1.5;
                    }
                    if f.preferred_governments
                        .iter()
                        .any(|s| s == &world.world.government)
                    {
                        w *= 1.4;
                    }
                    let feat_hits = f
                        .preferred_notable_features
                        .iter()
                        .filter(|s| world.world.notable_features.contains(s))
                        .count();
                    if feat_hits > 0 {
                        w *= 1.3_f64.powi(feat_hits as i32);
                    }
                    (f, w)
                })
                .collect();

            let mut chosen: BTreeSet<String> = BTreeSet::new();
            let influences = [
                FactionInfluence::Dominant,
                FactionInfluence::Significant,
                FactionInfluence::Minor,
            ];

            for inf in influences.iter().take(max_factions) {
                if weighted.is_empty() {
                    break;
                }
                let pairs: Vec<(&FactionDef, f64)> =
                    weighted.iter().map(|(f, w)| (*f, *w)).collect();
                let idx = match weighted_index(&pairs, rng, "faction") {
                    Ok(i) => i,
                    Err(_) => break,
                };
                let f = weighted[idx].0;
                if chosen.insert(f.id.clone()) {
                    let dims = control::presence_dimensions(
                        &f.kind,
                        &f.default_disposition,
                        *inf,
                        Some(f),
                        world,
                    );
                    let dominance = DominanceState::from_score(dims.local_control_score());
                    let intel_confidence = dims.visibility.round().clamp(0.0, 100.0) as u8;
                    world.factions.push(WorldFactionPresence {
                        faction_id: f.id.clone(),
                        influence: *inf,
                        relationship_to_government: f.default_disposition.clone(),
                        dimensions: dims,
                        dominance,
                        intel_confidence,
                    });
                    let entry = scores.entry(f.id.clone()).or_insert((0.0, 0));
                    entry.0 += inf.weight();
                    entry.1 += 1;
                }
                weighted.remove(idx);
            }
            // Sort world.factions deterministically: by influence rank then catalog order.
            world.factions.sort_by(|a, b| {
                influence_rank(b.influence)
                    .cmp(&influence_rank(a.influence))
                    .then_with(|| {
                        catalog_order
                            .get(&a.faction_id)
                            .copied()
                            .unwrap_or(usize::MAX)
                            .cmp(
                                &catalog_order
                                    .get(&b.faction_id)
                                    .copied()
                                    .unwrap_or(usize::MAX),
                            )
                    })
                    .then_with(|| a.faction_id.cmp(&b.faction_id))
            });
        }
        // Spec §10.9: primary factions = top by score, ties broken by world
        // appearances, then catalog order, then faction id.
        let mut entries: Vec<(String, f64, usize)> =
            scores.into_iter().map(|(id, (s, n))| (id, s, n)).collect();
        entries.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| {
                    catalog_order
                        .get(&a.0)
                        .copied()
                        .unwrap_or(usize::MAX)
                        .cmp(&catalog_order.get(&b.0).copied().unwrap_or(usize::MAX))
                })
                .then_with(|| a.0.cmp(&b.0))
        });
        entries.truncate(PRIMARY_FACTION_LIMIT);
        sys.primary_factions = entries.into_iter().map(|(id, _, _)| id).collect();
    }
}

fn influence_rank(i: FactionInfluence) -> u8 {
    match i {
        FactionInfluence::Dominant => 3,
        FactionInfluence::Significant => 2,
        FactionInfluence::Minor => 1,
        FactionInfluence::Hidden => 0,
    }
}

fn aggregate_factions(
    systems: &[GeneratedSystem],
    factions: &[FactionDef],
) -> Vec<GeneratedFaction> {
    if factions.is_empty() {
        return Vec::new();
    }
    let mut by_id: BTreeMap<String, GeneratedFaction> = BTreeMap::new();
    for f in factions {
        by_id.insert(
            f.id.clone(),
            GeneratedFaction {
                id: f.id.clone(),
                name: f.name.clone(),
                kind: f.kind.clone(),
                disposition: f.default_disposition.clone(),
                system_presence: Vec::new(),
                world_presence: Vec::new(),
                power: Default::default(),
            },
        );
    }
    for sys in systems {
        for world in &sys.worlds {
            for p in &world.factions {
                if let Some(gf) = by_id.get_mut(&p.faction_id) {
                    gf.world_presence.push(world.id.clone());
                    if !gf.system_presence.contains(&sys.id) {
                        gf.system_presence.push(sys.id.clone());
                    }
                }
            }
        }
    }
    let mut v: Vec<GeneratedFaction> = by_id.into_values().collect();
    for f in &mut v {
        f.system_presence.sort();
        f.world_presence.sort();
    }
    let power = control::aggregate_faction_power(systems);
    control::apply_faction_power(&mut v, &power);
    v
}

// ── Routes ────────────────────────────────────────────────────────────────────

fn generate_routes(
    config: &AppConfig,
    rules: &RouteRules,
    systems: &[GeneratedSystem],
    rng: &mut ChaCha8Rng,
) -> Vec<GeneratedRoute> {
    if systems.len() < 2 {
        return Vec::new();
    }
    let max_distance = config
        .generation
        .routes
        .max_route_distance
        .max(rules.max_distance);
    let density = config.generation.routes.route_density.clamp(0.0, 1.0);

    let mut candidates: Vec<(usize, usize, f64, u32)> = Vec::new();
    for i in 0..systems.len() {
        for j in (i + 1)..systems.len() {
            let dist = hex_distance(systems[i].coord, systems[j].coord);
            if dist == 0 || dist > max_distance {
                continue;
            }
            let mut w = rules.default_weight;
            // Distance falloff.
            w *= 1.0 / f64::from(dist);

            let combined_tags: Vec<&String> = systems[i]
                .worlds
                .iter()
                .chain(systems[j].worlds.iter())
                .flat_map(|wd| wd.tags.iter())
                .collect();

            if combined_tags.iter().any(|t| {
                t.as_str() == "feature:trade_hub"
                    || t.as_str() == "feature:freeport"
                    || t.as_str() == "feature:major_spaceyard"
                    || t.as_str() == "feature:administrative_hub"
                    || t.as_str() == "feature:subsector_hegemon"
            }) {
                w *= 2.0;
            }
            if combined_tags.iter().any(|t| {
                t.as_str() == "feature:warp_phenomena"
                    || t.as_str() == "feature:quarantined"
                    || t.as_str() == "feature:war_zone"
                    || t.as_str() == "feature:daemonic_corruption"
            }) {
                w *= 0.25;
            }

            // Apply config modifiers.
            for m in &rules.modifiers {
                if let Some(s) = &m.when.notable_feature {
                    let tag = format!("feature:{}", taxonomy::to_snake_case(s));
                    if combined_tags.iter().any(|t| **t == tag) {
                        w *= m.multiplier;
                    }
                }
                if let Some(s) = &m.when.world_type {
                    let tag = format!("world_type:{}", taxonomy::to_snake_case(s));
                    if combined_tags.iter().any(|t| **t == tag) {
                        w *= m.multiplier;
                    }
                }
            }

            if w.is_finite() && w > 0.0 {
                candidates.push((i, j, w, dist));
            }
        }
    }

    // Sort by descending weight for deterministic top selection.
    candidates.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
            .then(a.1.cmp(&b.1))
    });

    let total_pairs = candidates.len();
    let target_count = ((total_pairs as f64) * density).round() as usize;
    let target_count = target_count.max(systems.len().saturating_sub(1));

    let mut chosen: Vec<(usize, usize, u32, Vec<String>)> = Vec::new();
    let mut chosen_set: BTreeSet<(usize, usize)> = BTreeSet::new();

    // Top-weight portion (deterministic).
    for (i, j, _, dist) in candidates.iter().take(target_count) {
        if chosen_set.insert((*i, *j)) {
            chosen.push((*i, *j, *dist, Vec::new()));
        }
    }

    // Connect isolated components if requested.
    if config.generation.routes.ensure_connected_graph {
        let mut parent: Vec<usize> = (0..systems.len()).collect();
        for (i, j, _, _) in &chosen {
            union(&mut parent, *i, *j);
        }
        let _ = rng; // RNG reserved for future stochastic edges
        for i in 0..systems.len() {
            for j in (i + 1)..systems.len() {
                if find(&mut parent, i) == find(&mut parent, j) {
                    continue;
                }
                let dist = hex_distance(systems[i].coord, systems[j].coord);
                if dist == 0 || dist > max_distance {
                    continue;
                }
                if chosen_set.insert((i, j)) {
                    chosen.push((i, j, dist, vec!["bridge".to_string()]));
                    union(&mut parent, i, j);
                }
            }
        }
    }

    let mut routes: Vec<GeneratedRoute> = chosen
        .into_iter()
        .map(|(i, j, dist, tags)| {
            let a = systems[i].id.clone();
            let b = systems[j].id.clone();
            let (from_id, to_id) = if a <= b { (a, b) } else { (b, a) };
            let (rt, stab) = classify_route(&systems[i], &systems[j], dist, max_distance);
            GeneratedRoute {
                id: ids::route_id(&from_id, &to_id),
                from_system_id: from_id,
                to_system_id: to_id,
                distance: dist,
                route_type: rt,
                stability: stab,
                tags,
                controls: Vec::new(),
            }
        })
        .collect();

    // Cap perilous routes at 10% of total. Excess downgraded to Hazardous.
    let perilous_limit = ((routes.len() as f64) * 0.10).round() as usize;
    if routes
        .iter()
        .filter(|r| r.stability == RouteStability::Perilous)
        .count()
        > perilous_limit
    {
        let remaining = std::cell::Cell::new(
            routes
                .iter()
                .filter(|r| r.stability == RouteStability::Perilous)
                .count()
                .saturating_sub(perilous_limit),
        );
        for r in &mut routes {
            if r.stability == RouteStability::Perilous && remaining.get() > 0 {
                r.stability = RouteStability::Hazardous;
                remaining.set(remaining.get() - 1);
            }
        }
    }

    routes.sort_by(|a, b| a.id.cmp(&b.id));
    routes
}

fn classify_route(
    a: &GeneratedSystem,
    b: &GeneratedSystem,
    dist: u32,
    max_dist: u32,
) -> (RouteType, RouteStability) {
    let tags: Vec<&String> = a
        .worlds
        .iter()
        .chain(b.worlds.iter())
        .flat_map(|w| w.tags.iter())
        .collect();
    let has = |tag: &str| tags.iter().any(|t| t.as_str() == tag);
    if has("feature:warp_phenomena") || has("feature:daemonic_corruption") {
        if dist >= max_dist - 2 && dist < max_dist {
            return (RouteType::DangerousPassage, RouteStability::Perilous);
        }
        return (RouteType::DangerousPassage, RouteStability::Hazardous);
    }
    if has("feature:war_zone") {
        return (RouteType::DangerousPassage, RouteStability::Perilous);
    }
    if dist >= max_dist {
        return (RouteType::ChartedPassage, RouteStability::Unstable);
    }
    if has("feature:trade_hub") || has("feature:administrative_hub") {
        return (RouteType::StableWarpLane, RouteStability::Stable);
    }
    (RouteType::ChartedPassage, RouteStability::Stable)
}

fn find(parent: &mut [usize], i: usize) -> usize {
    if parent[i] == i {
        return i;
    }
    let root = find(parent, parent[i]);
    parent[i] = root;
    root
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let ra = find(parent, a);
    let rb = find(parent, b);
    if ra != rb {
        parent[ra] = rb;
    }
}

// ── Manifest ──────────────────────────────────────────────────────────────────

fn build_manifest(
    config: &AppConfig,
    input_digests: &BTreeMap<String, String>,
    systems: &[GeneratedSystem],
    routes: &[GeneratedRoute],
) -> GenerationManifest {
    let settings_repr = format!(
        "{}-{}-{}-{}-{}-{}-{}",
        config.generation.seed,
        config.generation.sector_width,
        config.generation.sector_height,
        config.generation.system_count,
        config.generation.min_worlds_per_system,
        config.generation.max_worlds_per_system,
        config.generation.world_feature_count,
    );
    let settings_digest = format!(
        "blake3:{}",
        rng::hex(blake3::hash(settings_repr.as_bytes()).as_bytes())
    );
    let seed_hash = format!(
        "blake3:{}",
        rng::hex(&rng::hash_root_seed(&config.generation.seed))
    );
    let world_count: usize = systems.iter().map(|s| s.worlds.len()).sum();

    GenerationManifest {
        project_id: config.project.id.clone(),
        generated_at_policy: "not recorded by default".to_string(),
        generator_name: crate::GENERATOR_NAME.to_string(),
        generator_version: crate::GENERATOR_VERSION.to_string(),
        seed: config.generation.seed.clone(),
        seed_hash,
        profile: None,
        input_digests: input_digests.clone(),
        settings_digest,
        system_count: systems.len(),
        world_count,
        route_count: routes.len(),
    }
}
