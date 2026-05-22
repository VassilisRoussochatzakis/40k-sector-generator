//! Deterministic sector generation pipeline.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

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
    hex_distance, DominanceState, FactionInfluence, GeneratedFaction, GeneratedForce,
    GeneratedRoute, GeneratedSector, GeneratedStar, GeneratedSubfaction, GeneratedSystem,
    GeneratedWorld, GenerationManifest, HexCoord, RouteStability, RouteType, SystemControlSummary,
    WorldControlSummary, WorldDto, WorldFactionPresence,
};
use crate::taxonomy;
use crate::world_pool::{self, WorldCandidate, WorldCandidatePool};
use crate::worlds::{NotableFeature, StarColour};

/// Progress events emitted by sector generation when a caller opts in through
/// [`generate_with_progress`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectorProgress {
    WorldPoolBuilt {
        rows: usize,
        candidates: usize,
        excluded: usize,
    },
    SystemsPlaced {
        total: usize,
        width: u32,
        height: u32,
    },
    RegionsBuilt {
        count: usize,
    },
    SystemBuilt {
        current: usize,
        total: usize,
        worlds: usize,
    },
    FactionsAssigned {
        catalog_rows: usize,
    },
    FactionsAggregated {
        factions: usize,
    },
    RoutesGenerated {
        routes: usize,
    },
    RegionEffectsApplied {
        regions: usize,
    },
    HiddenRoutesApplied {
        added: usize,
        routes: usize,
    },
    RouteControlsDerived {
        routes: usize,
    },
    SystemStateDerived {
        current: usize,
        total: usize,
    },
    ManifestBuilt {
        systems: usize,
        worlds: usize,
        routes: usize,
    },
    OverlayDerived {
        name: &'static str,
    },
    Complete {
        systems: usize,
        worlds: usize,
        routes: usize,
    },
}

pub fn generate(project: ProjectInput) -> Result<GeneratedSector, SectorError> {
    generate_with_progress(project, |_| {})
}

/// Deterministic top-level sector generation with progress callbacks.
///
/// The callback is synchronous and receives major pipeline milestones plus
/// per-system counters. The default [`generate`] wrapper uses a no-op callback.
///
/// # Errors
///
/// Same as [`generate`].
pub fn generate_with_progress<F>(
    project: ProjectInput,
    mut progress: F,
) -> Result<GeneratedSector, SectorError>
where
    F: FnMut(SectorProgress),
{
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
        history: history_cfg,
        input_digests,
        ..
    } = project;

    let source_rows = world_rows.len();
    let pool = world_pool::build_pool(
        &world_rows,
        &world_tables,
        &config.generation.world_selection,
    );
    progress(SectorProgress::WorldPoolBuilt {
        rows: source_rows,
        candidates: pool.candidates.len(),
        excluded: pool.excluded_rows.len(),
    });
    if pool.candidates.is_empty() {
        return Err(SectorError::NoWorldCandidates);
    }

    let placements = place_systems(&config)?;
    progress(SectorProgress::SystemsPlaced {
        total: placements.len(),
        width: config.generation.sector_width,
        height: config.generation.sector_height,
    });
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
    progress(SectorProgress::RegionsBuilt {
        count: warp_regions.len(),
    });

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
        let worlds = system.worlds.len();
        systems.push(system);
        progress(SectorProgress::SystemBuilt {
            current: system_index,
            total: placements.len(),
            worlds,
        });
    }

    // ── Factions ────────────────────────────────────────────────────────────
    if !factions.is_empty() {
        let mut faction_rng = rng::stage_rng(&config.generation.seed, "factions", "sector");
        assign_factions(&mut systems, &factions, &mut faction_rng);
        progress(SectorProgress::FactionsAssigned {
            catalog_rows: factions.len(),
        });
    }

    let generated_factions = aggregate_factions(&systems, &factions);
    progress(SectorProgress::FactionsAggregated {
        factions: generated_factions.len(),
    });

    // ── Routes ──────────────────────────────────────────────────────────────
    let mut routes = if config.generation.routes.enabled {
        let mut route_rng = rng::stage_rng(&config.generation.seed, "routes", "sector");
        generate_routes(&config, &route_rules, &systems, &mut route_rng)
    } else {
        Vec::new()
    };
    progress(SectorProgress::RoutesGenerated {
        routes: routes.len(),
    });

    // §5 NEW.md: apply region effects to routes (storm → perilous, turbulence
    // → one tier worse, calm corridor → one tier better up to the perilous
    // ceiling). Idempotent.
    if regions_cfg.apply_to_routes && !warp_regions.is_empty() {
        crate::regions::apply_route_effects(&warp_regions, &systems, &mut routes);
        progress(SectorProgress::RegionEffectsApplied {
            regions: warp_regions.len(),
        });
    }

    // §3 NEXT: append hidden route layers (webway / black-ship / smuggling)
    // before per-route control derivation so they receive the same control
    // treatment as public lanes.
    let mut generated_factions = generated_factions;
    if !generated_factions.is_empty() {
        let before = routes.len();
        crate::hidden_routes::append_hidden_routes_with_regions(
            &systems,
            &generated_factions,
            &warp_regions,
            &mut routes,
        );
        progress(SectorProgress::HiddenRoutesApplied {
            added: routes.len().saturating_sub(before),
            routes: routes.len(),
        });
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
        progress(SectorProgress::RouteControlsDerived {
            routes: routes.len(),
        });
    }

    // §1 NEXT: per-world surface regions.
    // §2 NEXT: per-system orbital assets + blockade detection.
    // §5 NEXT: per-world + per-system initial conflict state.
    // §7 NEXT: per-system fog-of-war intel records. Observers are scoped to
    // factions with at least one presence IN THIS SYSTEM. The rumor-based
    // view for distant observers is reconstructible on demand from the raw
    // system state, so persisting it everywhere would O(F·S) and bloat
    // sector.json by tens of MB on large sectors with many factions.
    let system_total = systems.len();
    for (idx, sys) in systems.iter_mut().enumerate() {
        for w in sys.worlds.iter_mut() {
            w.regions = crate::surface_region::derive_regions(w);
            w.conflict = crate::conflict::derive_world_conflict(w);
        }
        let (assets, blockade) = crate::orbital_assets::derive_orbital_assets(sys);
        sys.orbital_assets = assets;
        sys.blockade = blockade;
        sys.conflict = crate::conflict::derive_system_conflict(sys);
        let mut per_sys: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for w in &sys.worlds {
            for p in &w.factions {
                per_sys.insert(p.faction_id.as_str());
            }
        }
        let obs_refs: Vec<&str> = per_sys.into_iter().collect();
        sys.intel = crate::intel::derive_system_intel(sys, &obs_refs);
        progress(SectorProgress::SystemStateDerived {
            current: idx + 1,
            total: system_total,
        });
    }

    // Sort everything for stable serialization.
    let mut sorted_systems = systems;
    sorted_systems.sort_by(|a, b| a.id.cmp(&b.id));
    let mut sorted_routes = routes;
    sorted_routes.sort_by(|a, b| a.id.cmp(&b.id));
    generated_factions.sort_by(|a, b| a.id.cmp(&b.id));

    let manifest = build_manifest(&config, &input_digests, &sorted_systems, &sorted_routes);
    progress(SectorProgress::ManifestBuilt {
        systems: sorted_systems.len(),
        worlds: manifest.world_count,
        routes: sorted_routes.len(),
    });

    let mut sector = GeneratedSector {
        id: config.project.id.clone().into(),
        title: config.project.title.clone().into(),
        seed: config.generation.seed.clone().into(),
        generator_name: crate::GENERATOR_NAME.to_string().into(),
        generator_version: crate::GENERATOR_VERSION.to_string().into(),
        width: config.generation.sector_width,
        height: config.generation.sector_height,
        systems: sorted_systems,
        routes: sorted_routes,
        factions: generated_factions,
        manifest,
        influence_field: Default::default(),
        power_projection: Default::default(),
        relations: Default::default(),
        regions: warp_regions.into(),
        economy: Default::default(),
        chronicle: Default::default(),
    };

    // §11 NEXT: archetype rules.
    crate::archetypes::apply_all(&mut sector);
    progress(SectorProgress::OverlayDerived { name: "archetypes" });
    // §4 NEXT: power projection over routes (decays + doctrine).
    sector.power_projection = crate::power_projection::project_sector(&sector).into();
    crate::power_projection::apply_to_factions(&sector.power_projection, &mut sector.factions);
    progress(SectorProgress::OverlayDerived {
        name: "power_projection",
    });
    // §9 NEXT: continuous area layers.
    sector.influence_field = crate::influence_field::build(&sector).into();
    progress(SectorProgress::OverlayDerived {
        name: "influence_field",
    });

    // §5 NEW2.md: derive inter-faction relationship matrix once factions are
    // finalised. Pure derivation, no extra RNG draws affect prior stages.
    // `[generation.relations].min_world_presence` controls how aggressively
    // the canonical faction list is filtered before the C(n,2) loop.
    sector.relations = crate::relations::derive_with_threshold(
        &sector,
        &relations_cfg,
        config.generation.relations.min_world_presence,
    ).into();
    progress(SectorProgress::OverlayDerived { name: "relations" });

    // §12 NEW.md: derive the economy snapshot last so it can read final
    // route stability + control records. Optional `feed_stability` nudge
    // applies after the snapshot is built.
    sector.economy = crate::economy::derive_with(&sector, &economy_cfg).into();
    if economy_cfg.feed_stability && sector.economy.enabled {
        let snap = sector.economy.clone();
        crate::economy::apply_stability_nudge(&snap, &mut sector);
    }
    progress(SectorProgress::OverlayDerived { name: "economy" });

    // §1 NEW2.md: timeline / chronicle derives after all structural and
    // overlay state is final so events can reference routes, regions,
    // subsectors, claims, control, and present conflicts.
    sector.chronicle = crate::history::derive_with(&sector, &history_cfg);
    progress(SectorProgress::OverlayDerived { name: "chronicle" });
    progress(SectorProgress::Complete {
        systems: sector.manifest.system_count,
        worlds: sector.manifest.world_count,
        routes: sector.manifest.route_count,
    });

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
    let mut leftover: Vec<HexCoord> = Vec::with_capacity(all.len().saturating_sub(target));
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
            let mut still_blocked: Vec<HexCoord> = Vec::with_capacity(leftover.len());
            for c in std::mem::take(&mut leftover) {
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
        star: GeneratedStar {
            colour_code: star_colour.code().to_string().into(),
            colour_name: star_colour.short_name().to_string().into(),
            spectral_type: Some(spectral_type_fallback(star_colour).to_string().into()),
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
    if pool.star_colour_weights.is_empty() {
        return Err(SectorError::NoWorldCandidates);
    }
    let idx = weighted_index(&pool.star_colour_weights, rng, "system_star_colour")?;
    Ok(pool.star_colour_weights[idx].0)
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

struct WorldGenParams<'a> {
    config: &'a AppConfig,
    pool: &'a WorldCandidatePool,
    names: &'a NameTables,
    system_index: usize,
    system_name: &'a str,
    star_colour: StarColour,
    sys_rng: &'a mut ChaCha8Rng,
    root_seed: &'a str,
    anomaly_bias: bool,
}

fn generate_worlds_for_system(
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
        ).into(),
    ];
    for f in &world.notable_features {
        tags.push(format!("feature:{}", snake(f.as_ref())).into());
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
    // Build a temporary top-faction catalog for stability derivation.
    let stability_factions: Vec<crate::sector_model::GeneratedFaction> =
        build_faction_groups(factions)
            .iter()
            .map(|g| crate::sector_model::GeneratedFaction {
                id: g.id.clone(),
                name: g.name.clone().into(),
                kind: g.kind.clone().into(),
                disposition: g.disposition.clone().into(),
                subfactions: Vec::new(),
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
    let groups = build_faction_groups(factions);
    let subgroups = build_subfaction_groups(&groups);
    // Stable faction/sub-faction order from first appearance in the source catalog.
    let catalog_order: BTreeMap<crate::ids::FactionId, usize> =
        groups.iter().map(|g| (g.id.clone(), g.order)).collect();
    let sub_catalog_order: BTreeMap<crate::ids::FactionId, usize> =
        subgroups.iter().map(|g| (g.id.clone(), g.order)).collect();

    for sys in systems.iter_mut() {
        // Per-system accumulator: overall faction_id -> (score, world_appearances)
        let mut scores: BTreeMap<crate::ids::FactionId, (f64, usize)> = BTreeMap::new();
        for world in &mut sys.worlds {
            let pop_tag = world
                .tags
                .iter()
                .find(|t| t.starts_with("population:"))
                .cloned()
                .unwrap_or_default();
            let max_factions: usize = match pop_tag.as_ref() {
                "population:uninhabited" => 0,
                "population:minimal" | "population:lightly_populated" => 1,
                "population:sole_settlement" => 2,
                _ => 3,
            };
            if max_factions == 0 {
                continue;
            }

            let mut weighted: Vec<(&SubfactionGroup<'_>, f64)> = subgroups
                .iter()
                .map(|g| {
                    let g = *g;
                    let w = g
                        .members
                        .iter()
                        .map(|f| faction_weight_for_world(f, world))
                        .fold(0.0_f64, f64::max);
                    (g, w)
                })
                .collect();

            let mut chosen: BTreeSet<crate::ids::FactionId> = BTreeSet::new();
            let influences = [
                FactionInfluence::Dominant,
                FactionInfluence::Significant,
                FactionInfluence::Minor,
            ];

            for inf in influences.iter().take(max_factions) {
                if weighted.is_empty() {
                    break;
                }
                let pairs: Vec<(&SubfactionGroup<'_>, f64)> =
                    weighted.iter().map(|(g, w)| (*g, *w)).collect();
                let idx = match weighted_index(&pairs, rng, "faction") {
                    Ok(i) => i,
                    Err(_) => break,
                };
                let g = weighted[idx].0;
                if chosen.insert(g.id.clone()) {
                    let f = match choose_force(g, world, rng) {
                        Some(f) => f,
                        None => {
                            weighted.remove(idx);
                            continue;
                        }
                    };
                    let dims = control::presence_dimensions(
                        &g.kind,
                        &f.default_disposition,
                        *inf,
                        Some(f),
                        world,
                    );
                    let dominance = DominanceState::from_score(dims.local_control_score());
                    let intel_confidence = dims.visibility.round().clamp(0.0, 100.0) as u8;
                    world.factions.push(WorldFactionPresence {
                        faction_id: g.faction_id.clone(),
                        subfaction_id: Some(g.id.clone()),
                        subfaction_name: Some(g.name.clone().into()),
                        force_id: Some(f.id.clone()),
                        force_name: Some(f.name.clone().into()),
                        influence: *inf,
                        relationship_to_government: f.default_disposition.clone().into(),
                        dimensions: dims,
                        dominance,
                        intel_confidence,
                    });
                    let entry = scores.entry(g.faction_id.clone()).or_insert((0.0, 0));
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
                    .then_with(|| {
                        sub_catalog_order
                            .get(a.subfaction_id.as_ref().unwrap_or(&a.faction_id))
                            .copied()
                            .unwrap_or(usize::MAX)
                            .cmp(
                                &sub_catalog_order
                                    .get(b.subfaction_id.as_ref().unwrap_or(&b.faction_id))
                                    .copied()
                                    .unwrap_or(usize::MAX),
                            )
                    })
                    .then_with(|| a.subfaction_id.cmp(&b.subfaction_id))
                    .then_with(|| a.force_id.cmp(&b.force_id))
            });
        }
        // Spec §10.9: primary factions = top by score, ties broken by world
        // appearances, then catalog order, then faction id.
        let mut entries: Vec<(crate::ids::FactionId, f64, usize)> =
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

#[derive(Debug)]
struct FactionGroup<'a> {
    id: crate::ids::FactionId,
    name: String,
    kind: String,
    disposition: String,
    order: usize,
    subfactions: Vec<SubfactionGroup<'a>>,
}

#[derive(Debug)]
struct SubfactionGroup<'a> {
    faction_id: crate::ids::FactionId,
    id: crate::ids::FactionId,
    name: String,
    kind: String,
    disposition: String,
    order: usize,
    members: Vec<&'a FactionDef>,
}

fn build_faction_groups(factions: &[FactionDef]) -> Vec<FactionGroup<'_>> {
    let mut members: BTreeMap<crate::ids::FactionId, Vec<&FactionDef>> = BTreeMap::new();
    let mut order: BTreeMap<crate::ids::FactionId, usize> = BTreeMap::new();
    for (idx, f) in factions.iter().enumerate() {
        let top_id = f.top_faction_id();
        order.entry(top_id.clone()).or_insert(idx);
        members.entry(top_id).or_default().push(f);
    }

    let mut groups: Vec<FactionGroup<'_>> = members
        .into_iter()
        .map(|(top_id, group_members)| {
            let disposition = representative_disposition(&group_members);
            let name = group_members
                .first()
                .map(|f| f.top_faction_name())
                .unwrap_or_else(|| crate::factions::display_name_from_id(top_id.as_str()).into_owned());
            FactionGroup {
                id: top_id.clone(),
                name,
                kind: top_id.to_string(),
                disposition,
                order: order.get(&top_id).copied().unwrap_or(usize::MAX),
                subfactions: build_subfactions_for_group(&top_id, group_members),
            }
        })
        .collect();
    groups.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    groups
}

fn build_subfactions_for_group<'a>(
    top_id: &crate::ids::FactionId,
    members: Vec<&'a FactionDef>,
) -> Vec<SubfactionGroup<'a>> {
    let mut grouped: BTreeMap<crate::ids::FactionId, Vec<&FactionDef>> = BTreeMap::new();
    let mut order: BTreeMap<crate::ids::FactionId, usize> = BTreeMap::new();
    for (idx, f) in members.into_iter().enumerate() {
        let sub_id = f.subfaction_id();
        order.entry(sub_id.clone()).or_insert(idx);
        grouped.entry(sub_id).or_default().push(f);
    }

    let mut subfactions: Vec<SubfactionGroup<'a>> = grouped
        .into_iter()
        .map(|(sub_id, mut group_members)| {
            group_members.sort_by(|a, b| a.id.cmp(&b.id));
            let first = group_members.first().copied();
            SubfactionGroup {
                faction_id: top_id.clone(),
                id: sub_id.clone(),
                name: first
                    .map(FactionDef::subfaction_name)
                    .unwrap_or_else(|| crate::factions::display_name_from_id(sub_id.as_str()).into_owned()),
                kind: first
                    .map(|f| f.kind.clone())
                    .unwrap_or_else(|| sub_id.to_string()),
                disposition: representative_disposition(&group_members),
                order: order.get(&sub_id).copied().unwrap_or(usize::MAX),
                members: group_members,
            }
        })
        .collect();
    subfactions.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    subfactions
}

fn build_subfaction_groups<'a>(groups: &'a [FactionGroup<'a>]) -> Vec<&'a SubfactionGroup<'a>> {
    groups.iter().flat_map(|g| g.subfactions.iter()).collect()
}

fn representative_disposition(members: &[&FactionDef]) -> String {
    let mut totals: BTreeMap<&str, f64> = BTreeMap::new();
    for f in members {
        *totals.entry(f.default_disposition.as_str()).or_default() += f.weight.max(0.0);
    }
    totals
        .into_iter()
        .max_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.0.cmp(a.0))
        })
        .map(|(d, _)| d.to_string())
        .unwrap_or_default()
}

fn faction_weight_for_world(f: &FactionDef, world: &GeneratedWorld) -> f64 {
    let mut w = f.weight;
    if f.preferred_world_types
        .iter()
        .any(|s| s == world.world.world_type.as_ref())
    {
        w *= 1.5;
    }
    if f.preferred_governments
        .iter()
        .any(|s| s == world.world.government.as_ref())
    {
        w *= 1.4;
    }
    let feat_hits = f
        .preferred_notable_features
        .iter()
        .filter(|s| world.world.notable_features.iter().any(|nf| nf.as_ref() == *s))
        .count();
    if feat_hits > 0 {
        w *= 1.3_f64.powi(feat_hits as i32);
    }
    w
}

fn choose_force<'a>(
    group: &SubfactionGroup<'a>,
    world: &GeneratedWorld,
    rng: &mut ChaCha8Rng,
) -> Option<&'a FactionDef> {
    let weighted: Vec<(&FactionDef, f64)> = group
        .members
        .iter()
        .map(|f| (*f, faction_weight_for_world(f, world)))
        .collect();
    let idx = weighted_index(&weighted, rng, "force").ok()?;
    Some(weighted[idx].0)
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
    let mut by_id: BTreeMap<crate::ids::FactionId, GeneratedFaction> = BTreeMap::new();
    for g in build_faction_groups(factions) {
        by_id.insert(
            g.id.clone(),
            GeneratedFaction {
                id: g.id.clone(),
                name: g.name.into(),
                kind: g.kind.into(),
                disposition: g.disposition.into(),
                subfactions: g
                    .subfactions
                    .iter()
                    .map(|sf| GeneratedSubfaction {
                        id: sf.id.clone(),
                        name: sf.name.clone().into(),
                        disposition: sf.disposition.clone().into(),
                        forces: sf
                            .members
                            .iter()
                            .map(|f| GeneratedForce {
                                id: f.id.clone(),
                                name: f.name.clone().into(),
                                disposition: f.default_disposition.clone().into(),
                                system_presence: Vec::new(),
                                world_presence: Vec::new(),
                            })
                            .collect(),
                        system_presence: Vec::new(),
                        world_presence: Vec::new(),
                    })
                    .collect(),
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
                    if let Some(sub_id) = &p.subfaction_id {
                        if let Some(sf) = gf.subfactions.iter_mut().find(|sf| sf.id == *sub_id) {
                            sf.world_presence.push(world.id.clone());
                            if !sf.system_presence.contains(&sys.id) {
                                sf.system_presence.push(sys.id.clone());
                            }
                            if let Some(force_id) = &p.force_id {
                                if let Some(force) =
                                    sf.forces.iter_mut().find(|force| force.id == *force_id)
                                {
                                    force.world_presence.push(world.id.clone());
                                    if !force.system_presence.contains(&sys.id) {
                                        force.system_presence.push(sys.id.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let mut v: Vec<GeneratedFaction> = by_id.into_values().collect();
    for f in &mut v {
        f.system_presence.sort();
        f.system_presence.dedup();
        f.world_presence.sort();
        f.world_presence.dedup();
        for sf in &mut f.subfactions {
            sf.system_presence.sort();
            sf.system_presence.dedup();
            sf.world_presence.sort();
            sf.world_presence.dedup();
            for force in &mut sf.forces {
                force.system_presence.sort();
                force.system_presence.dedup();
                force.world_presence.sort();
                force.world_presence.dedup();
            }
        }
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

    let pair_upper = systems.len().saturating_sub(1) * systems.len() / 2;
    let mut candidates: Vec<(usize, usize, f64, u32)> = Vec::with_capacity(pair_upper);
    for i in 0..systems.len() {
        for j in (i + 1)..systems.len() {
            let dist = hex_distance(systems[i].coord, systems[j].coord);
            if dist == 0 || dist > max_distance {
                continue;
            }
            let mut w = rules.default_weight;
            // Distance falloff.
            w *= 1.0 / f64::from(dist);

            let combined_tags: Vec<&Arc<str>> = systems[i]
                .worlds
                .iter()
                .chain(systems[j].worlds.iter())
                .flat_map(|wd| wd.tags.iter())
                .collect();

            if combined_tags.iter().any(|t| {
                let s = t.as_ref();
                s == "feature:trade_hub"
                    || s == "feature:freeport"
                    || s == "feature:major_spaceyard"
                    || s == "feature:administrative_hub"
                    || s == "feature:subsector_hegemon"
            }) {
                w *= 2.0;
            }
            if combined_tags.iter().any(|t| {
                let s = t.as_ref();
                s == "feature:warp_phenomena"
                    || s == "feature:quarantined"
                    || s == "feature:war_zone"
                    || s == "feature:daemonic_corruption"
            }) {
                w *= 0.25;
            }

            // Apply config modifiers.
            for m in &rules.modifiers {
                if let Some(s) = &m.when.notable_feature {
                    let tag = format!("feature:{}", taxonomy::to_snake_case(s));
                    if combined_tags.iter().any(|t| t.as_ref() == tag) {
                        w *= m.multiplier;
                    }
                }
                if let Some(s) = &m.when.world_type {
                    let tag = format!("world_type:{}", taxonomy::to_snake_case(s));
                    if combined_tags.iter().any(|t| t.as_ref() == tag) {
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

    let mut chosen: Vec<(usize, usize, u32, Vec<Arc<str>>)> = Vec::with_capacity(target_count);
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
                    chosen.push((i, j, dist, vec!["bridge".into()]));
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
    let tags: Vec<&Arc<str>> = a
        .worlds
        .iter()
        .chain(b.worlds.iter())
        .flat_map(|w| w.tags.iter())
        .collect();
    let has = |tag: &str| tags.iter().any(|t| t.as_ref() == tag);
    if has("feature:warp_phenomena") || has("feature:daemonic_corruption") {
        if dist >= max_dist - 2 && dist < max_dist {
            return (RouteType::ChartedPassage, RouteStability::Perilous);
        }
        return (RouteType::ChartedPassage, RouteStability::Hazardous);
    }
    if has("feature:war_zone") {
        return (RouteType::ChartedPassage, RouteStability::Perilous);
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
        project_id: config.project.id.clone().into(),
        generated_at_policy: "not recorded by default".to_string().into(),
        generator_name: crate::GENERATOR_NAME.to_string().into(),
        generator_version: crate::GENERATOR_VERSION.to_string().into(),
        seed: config.generation.seed.clone().into(),
        seed_hash: seed_hash.into(),
        profile: None,
        input_digests: input_digests.clone(),
        settings_digest: settings_digest.into(),
        system_count: systems.len(),
        world_count,
        route_count: routes.len(),
    }
}
