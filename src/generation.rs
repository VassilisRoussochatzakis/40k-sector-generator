//! Deterministic sector generation pipeline.

use std::collections::{BTreeMap, BTreeSet};

use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::config::{AppConfig, WorldSelectionConfig};
use crate::errors::SectorError;
use crate::factions::FactionDef;
use crate::ids;
use crate::input::ProjectInput;
use crate::names::{roman_numeral, NameTables};
use crate::rng::{self, weighted_index};
use crate::routes::RouteRules;
use crate::sector_model::{
    hex_distance, FactionInfluence, GeneratedFaction, GeneratedRoute, GeneratedSector,
    GeneratedStar, GeneratedSystem, GeneratedWorld, GenerationManifest, HexCoord, RouteStability,
    RouteType, WorldDto, WorldFactionPresence,
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

    for (idx, coord) in placements.iter().enumerate() {
        let system_index = idx + 1;
        let sys_id = ids::system_id(system_index);
        let mut sys_rng = rng::stage_rng(&config.generation.seed, "system", &sys_id);

        let star_colour = choose_system_star_colour(&pool, &mut sys_rng)?;
        let name = pick_system_name(&names, system_index, &mut sys_rng, &used_names);
        used_names.insert(name.clone());

        let worlds = generate_worlds_for_system(
            &config,
            &pool,
            &names,
            system_index,
            &sys_id,
            &name,
            star_colour,
            &mut sys_rng,
            &config.generation.seed,
        )?;

        let system = GeneratedSystem {
            id: sys_id.clone(),
            index: system_index,
            name,
            coord: *coord,
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
        };
        systems.push(system);
    }

    // ── Factions ────────────────────────────────────────────────────────────
    if !factions.is_empty() {
        let mut faction_rng = rng::stage_rng(&config.generation.seed, "factions", "sector");
        assign_factions(&mut systems, &factions, &mut faction_rng);
    }

    let generated_factions = aggregate_factions(&systems, &factions);

    // ── Routes ──────────────────────────────────────────────────────────────
    let routes = if config.generation.routes.enabled {
        let mut route_rng = rng::stage_rng(&config.generation.seed, "routes", "sector");
        generate_routes(&config, &route_rules, &systems, &mut route_rng)
    } else {
        Vec::new()
    };

    // Sort everything for stable serialization.
    let mut sorted_systems = systems;
    sorted_systems.sort_by(|a, b| a.id.cmp(&b.id));
    let mut sorted_routes = routes;
    sorted_routes.sort_by(|a, b| a.id.cmp(&b.id));
    let mut sorted_factions = generated_factions;
    sorted_factions.sort_by(|a, b| a.id.cmp(&b.id));

    let manifest = build_manifest(&config, &input_digests, &sorted_systems, &sorted_routes);

    Ok(GeneratedSector {
        id: config.project.id.clone(),
        title: config.project.title.clone(),
        seed: config.generation.seed.clone(),
        generator_name: crate::GENERATOR_NAME.to_string(),
        generator_version: crate::GENERATOR_VERSION.to_string(),
        width: config.generation.sector_width,
        height: config.generation.sector_height,
        systems: sorted_systems,
        routes: sorted_routes,
        factions: sorted_factions,
        manifest,
    })
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
    let min_dist = g.placement.minimum_system_distance;
    for c in all {
        if placed.len() >= target {
            break;
        }
        if min_dist <= 1 || placed.iter().all(|p| hex_distance(*p, c) >= min_dist) {
            placed.push(c);
        }
    }

    if placed.len() < target {
        // Couldn't satisfy minimum distance — fall back to filling without constraint.
        for r in 0..height {
            for q in 0..width {
                let c = HexCoord { q, r };
                if !placed.contains(&c) {
                    placed.push(c);
                    if placed.len() >= target {
                        break;
                    }
                }
            }
            if placed.len() >= target {
                break;
            }
        }
    }

    // Sort so output ordering is deterministic regardless of shuffle order.
    placed.sort();
    Ok(placed)
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
    sys_id: &str,
    system_name: &str,
    star_colour: StarColour,
    sys_rng: &mut ChaCha8Rng,
    root_seed: &str,
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
        )?;

        if config
            .generation
            .world_selection
            .avoid_duplicate_world_type_in_system
        {
            used_world_types.insert(taxonomy::world_type_name(&cand.world_type));
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
        });
    }
    // Sort by orbit for stable output.
    worlds.sort_by(|a, b| a.id.cmp(&b.id));
    // Mark suppress-unused warning for sys_id - actually used:
    let _ = sys_id;
    Ok(worlds)
}

fn choose_world_candidate<'a>(
    pool: &'a WorldCandidatePool,
    star_colour: StarColour,
    cfg: &WorldSelectionConfig,
    used_world_types: &BTreeSet<String>,
    rng: &mut ChaCha8Rng,
) -> Result<&'a WorldCandidate, SectorError> {
    let mut weighted: Vec<(&WorldCandidate, f64)> = Vec::new();

    for c in &pool.candidates {
        if cfg.strict_same_star_colour && c.star_colour != star_colour {
            continue;
        }
        if cfg.avoid_duplicate_world_type_in_system
            && used_world_types.contains(&taxonomy::world_type_name(&c.world_type))
        {
            continue;
        }
        let mut w = c.weight;
        if c.star_colour == star_colour {
            w *= cfg.same_star_colour_bias.max(0.0);
        }
        if w.is_finite() && w > 0.0 {
            weighted.push((c, w));
        }
    }
    if weighted.is_empty() {
        // Fallback: ignore avoid_duplicate constraint to keep generation moving.
        for c in &pool.candidates {
            if cfg.strict_same_star_colour && c.star_colour != star_colour {
                continue;
            }
            let mut w = c.weight;
            if c.star_colour == star_colour {
                w *= cfg.same_star_colour_bias.max(0.0);
            }
            if w.is_finite() && w > 0.0 {
                weighted.push((c, w));
            }
        }
    }
    if weighted.is_empty() {
        return Err(SectorError::WeightedSelectionFailed {
            context: "world_candidate".to_string(),
        });
    }
    let idx = weighted_index(&weighted, rng, "world_candidate")?;
    Ok(weighted[idx].0)
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
        seen.insert(taxonomy::notable_feature_name(f));
    }

    let by_wt_key = taxonomy::world_type_name(&cand.world_type);
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
            .filter(|wf| !seen.contains(&taxonomy::notable_feature_name(&wf.feature)))
            .map(|wf| (wf.feature, wf.weight))
            .collect();
        while chosen.len() < target_count && !filtered.is_empty() {
            let pool_slice: Vec<(&NotableFeature, f64)> =
                filtered.iter().map(|(f, w)| (f, *w)).collect();
            let pool_pairs: Vec<(NotableFeature, f64)> = pool_slice
                .into_iter()
                .map(|(f, w)| (f.clone(), w))
                .collect();
            let idx = match weighted_index(&pool_pairs, rng, "feature") {
                Ok(i) => i,
                Err(_) => break,
            };
            let (feature, _) = filtered.remove(idx);
            let key = taxonomy::notable_feature_name(&feature);
            if seen.insert(key) {
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
        (Some(p), Some(s)) => format!("{} {} {}", p, root, s),
        (Some(p), None) => format!("{} {}", p, root),
        (None, Some(s)) => format!("{} {}", root, s),
        (None, None) => root,
    }
}

// ── Tags ──────────────────────────────────────────────────────────────────────

fn tags_for_world(world: &crate::worlds::World) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    tags.push(format!(
        "world_type:{}",
        taxonomy::to_snake_case(&taxonomy::world_type_name(&world.world_type))
    ));
    tags.push(format!(
        "atmosphere:{}",
        taxonomy::to_snake_case(&taxonomy::atmosphere_name(&world.atmosphere))
    ));
    tags.push(format!(
        "temperature:{}",
        taxonomy::to_snake_case(&taxonomy::temperature_name(&world.temperature))
    ));
    tags.push(format!(
        "biosphere:{}",
        taxonomy::to_snake_case(&taxonomy::biosphere_name(&world.biosphere))
    ));
    tags.push(format!(
        "population:{}",
        taxonomy::to_snake_case(&taxonomy::population_name(&world.population))
    ));
    tags.push(format!(
        "tech:{}",
        taxonomy::to_snake_case(&taxonomy::tech_level_name(&world.tech_level))
    ));
    tags.push(format!(
        "gov:{}",
        taxonomy::to_snake_case(&taxonomy::government_name(&world.government))
    ));
    tags.push(format!(
        "star:{}",
        taxonomy::to_snake_case(taxonomy::star_colour_variant_name(world.star_colour))
    ));
    for f in &world.notable_features {
        tags.push(format!(
            "feature:{}",
            taxonomy::to_snake_case(&taxonomy::notable_feature_name(f))
        ));
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

fn assign_factions(systems: &mut [GeneratedSystem], factions: &[FactionDef], rng: &mut ChaCha8Rng) {
    if factions.is_empty() {
        return;
    }
    for sys in systems.iter_mut() {
        let mut sys_faction_counts: BTreeMap<String, usize> = BTreeMap::new();
        for world in sys.worlds.iter_mut() {
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
                    world.factions.push(WorldFactionPresence {
                        faction_id: f.id.clone(),
                        influence: *inf,
                        relationship_to_government: f.default_disposition.clone(),
                    });
                    *sys_faction_counts.entry(f.id.clone()).or_insert(0) += 1;
                }
                weighted.remove(idx);
            }
        }
        // Promote faction IDs that appear on >= 2 worlds in the system to primary.
        let mut primary: Vec<String> = sys_faction_counts
            .into_iter()
            .filter(|(_, n)| *n >= 2)
            .map(|(id, _)| id)
            .collect();
        primary.sort();
        sys.primary_factions = primary;
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
            w *= 1.0 / (dist as f64);

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
            let from = systems[i].id.clone();
            let to = systems[j].id.clone();
            let (rt, stab) = classify_route(&systems[i], &systems[j], dist, max_distance);
            GeneratedRoute {
                id: ids::route_id(&from, &to),
                from_system_id: if from <= to { from.clone() } else { to.clone() },
                to_system_id: if from <= to { to } else { from },
                distance: dist,
                route_type: rt,
                stability: stab,
                tags,
            }
        })
        .collect();

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
        return (RouteType::DangerousPassage, RouteStability::Hazardous);
    }
    if has("feature:war_zone") {
        return (RouteType::DangerousPassage, RouteStability::Unstable);
    }
    if has("feature:trade_hub") || has("feature:administrative_hub") {
        return (RouteType::StableWarpLane, RouteStability::Stable);
    }
    if dist >= max_dist {
        return (RouteType::ChartedPassage, RouteStability::Unstable);
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
