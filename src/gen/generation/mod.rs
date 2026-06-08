//! Deterministic sector generation pipeline.
//!
//! Public surface lives here; the actual stages are split across the
//! `placement`, `systems`, `world_placement`, `factions`, and `routes`
//! submodules. The orchestrator below threads the `SectorProgress` callback
//! through each stage in turn so callers can observe progress, cooperative
//! cancellation, and per-stage timings without touching the internals.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use crate::config::AppConfig;
use crate::errors::SectorError;
use crate::input::ProjectInput;
use crate::rng;
use crate::sector_model::{GeneratedRoute, GeneratedSector, GeneratedSystem, GenerationManifest};

mod factions;
mod placement;
mod reroll;
mod routes;
mod systems;
mod world_placement;

pub use factions::assign_factions_for_systems;
pub use reroll::{RerollNonces, Stage};
pub(crate) use routes::{distance_base_level, stability_from_level, stability_level};
pub use systems::{build_system, build_system_with_bias};
pub use world_placement::regenerate_world_payload;

/// Progress events emitted by sector generation when a caller opts in through
/// [`generate_with_progress`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    StageStarted {
        name: &'static str,
    },
    RoutesGenerated {
        routes: usize,
    },
    RegionEffectsApplied {
        regions: usize,
        affected_routes: usize,
        changed_routes: usize,
        bridge_checks: usize,
        bridges_preserved: usize,
        stable: usize,
        unstable: usize,
        hazardous: usize,
        perilous: usize,
    },
    RegionEffectsStarted {
        regions: usize,
        systems: usize,
        routes: usize,
    },
    RegionEffectsProgress {
        current: usize,
        total: usize,
        affected_routes: usize,
        changed_routes: usize,
        bridge_checks: usize,
        bridges_preserved: usize,
    },
    RegionEffectsBridgeCheckStarted {
        check: usize,
        route_index: usize,
        total_routes: usize,
        route_id: String,
    },
    HiddenRouteLayerStarted {
        layer: &'static str,
        endpoints: usize,
    },
    HiddenRouteLayerProgress {
        layer: &'static str,
        current: usize,
        total: usize,
        pairs: usize,
    },
    HiddenRouteLayerEmitProgress {
        layer: &'static str,
        current: usize,
        total: usize,
        added: usize,
    },
    HiddenRouteLayerCompleted {
        layer: &'static str,
        added: usize,
        routes: usize,
    },
    HiddenRoutesApplied {
        added: usize,
        routes: usize,
    },
    RouteControlsProgress {
        current: usize,
        total: usize,
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
    InfluenceFieldStarted {
        systems: usize,
        anchors: usize,
        cells: usize,
        radius: u32,
    },
    InfluenceFieldAnchorsProjected {
        current: usize,
        total: usize,
        touched_cells: usize,
    },
    InfluenceFieldCellsResolved {
        current: usize,
        total: usize,
        claimed_cells: usize,
    },
    InfluenceFieldBandsBuilt {
        bands: usize,
        claimed_cells: usize,
    },
    InfluenceFieldComplete {
        cells: usize,
        bands: usize,
    },
    ChronicleStarted {
        systems: usize,
        worlds: usize,
        routes: usize,
        max_subsector_events: u32,
    },
    ChronicleSubsectorEventsStarted {
        exact_cluster_count: usize,
        emitted_cap: u32,
        sampled: bool,
    },
    ChronicleSubsectorEventsDone {
        events: usize,
    },
    ChronicleSystemsScanned {
        current: usize,
        total: usize,
        events: usize,
    },
    ChronicleRoutesScanned {
        current: usize,
        total: usize,
        events: usize,
    },
    ChronicleEventRulesApplied {
        events: usize,
    },
    ChronicleSortingStarted {
        events: usize,
    },
    ChronicleComplete {
        events: usize,
    },
    Complete {
        systems: usize,
        worlds: usize,
        routes: usize,
    },
    /// docs/OPTIMIZE.txt G7 (rust_sectorforge_existing_app_optimization_prompt_v4
    /// §5A): wall-clock duration of a major pipeline stage. Emitted after a
    /// stage's structural completion event so CLI / GUI listeners can build a
    /// stage-time histogram without instrumenting their own timers.
    ///
    /// `stage` is a stable `&'static str` matching the stage names used by
    /// `StageStarted` / `OverlayDerived`. `millis` is rounded down to whole
    /// milliseconds; sub-millisecond stages report `0`.
    StageElapsed {
        stage: &'static str,
        millis: u64,
    },
}

pub fn generate(project: ProjectInput) -> Result<GeneratedSector, SectorError> {
    generate_with_progress_and_cancel(project, |_| {}, || false)
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
    progress: F,
) -> Result<GeneratedSector, SectorError>
where
    F: FnMut(SectorProgress),
{
    generate_with_progress_and_cancel(project, progress, || false)
}

/// Deterministic top-level sector generation with progress callbacks and
/// cooperative cancellation.
///
/// `should_cancel` is checked before and after major progress events plus
/// inside the main per-system loops. A cancellation returns
/// [`SectorError::GenerationCancelled`] without producing a partial sector.
///
/// # Errors
///
/// Same as [`generate`], plus [`SectorError::GenerationCancelled`] when
/// `should_cancel` returns `true`.
pub fn generate_with_progress_and_cancel<F, C>(
    project: ProjectInput,
    progress: F,
    should_cancel: C,
) -> Result<GeneratedSector, SectorError>
where
    F: FnMut(SectorProgress),
    C: FnMut() -> bool,
{
    generate_prefix(
        project,
        Stage::LAST,
        &RerollNonces::default(),
        progress,
        should_cancel,
    )
}

/// Deterministic **prefix** generation: run the pipeline up to and including
/// `through`, returning the partial [`GeneratedSector`] populated as far as the
/// cutoff reached (overlays past the cutoff are left at their `Default`/empty
/// values). This is the engine seam behind the builder's iterative
/// (step-by-step) generation mode (`ITERATIVE_GENERATION.md` Phase E).
///
/// `nonces` folds a per-[`Stage`] re-roll counter into that stage's RNG
/// discriminator (see [`RerollNonces`]). With the default (empty) `nonces`,
/// `generate_prefix(project, Stage::LAST, ..)` is **byte-identical** to the
/// legacy one-shot orchestrator — which is exactly what
/// [`generate_with_progress_and_cancel`] now is.
///
/// # Determinism guarantee (the linchpin — `ITERATIVE_GENERATION.md` §2.2)
///
/// Every stage seeds a *fresh* `ChaCha8Rng` from
/// `blake3("sectorforge:{root_seed}:{stage}:{disc}")` (`src/model/rng.rs`).
/// Stages share **no** RNG stream: the entropy a stage draws is a pure function
/// of `(root_seed, stage, discriminator)` and is independent of which other
/// stages ran before it. Therefore, for the same `(config, seed, nonces)`, the
/// stages `<= through` of a prefix run are **byte-for-byte** the same as the
/// corresponding stages of a full run — the cutoff cannot perturb earlier
/// stages because nothing downstream feeds entropy upstream. A nonce of `0`
/// (the default) leaves every discriminator unchanged, so the golden suite
/// stays green and "step straight through == one-shot" holds.
///
/// # Partial assembly
///
/// Stages 1–11 only read scratch (pool, placements, anomaly hexes); they do not
/// mutate the sector. The sort + manifest + struct construction (stage
/// [`Stage::Manifest`]) is pure and RNG-free, so it always runs to produce a
/// *valid* [`GeneratedSector`] from whatever was built before the cutoff
/// (empty `systems`/`routes`/`factions`/`regions` if those stages were skipped).
/// Overlay stages 13–18 are each gated on `through`. An early cutoff therefore
/// yields a renderable partial sector, never an empty/garbage one.
///
/// # Errors
///
/// Same as [`generate_with_progress_and_cancel`], plus
/// [`SectorError::GenerationCancelled`] when `should_cancel` returns `true`.
/// [`SectorError::NoWorldCandidates`] is only raised when the [`Stage::WorldPool`]
/// stage actually runs (i.e. `Stage::WorldPool <= through`).
pub fn generate_prefix<F, C>(
    project: ProjectInput,
    through: Stage,
    nonces: &RerollNonces,
    mut progress: F,
    mut should_cancel: C,
) -> Result<GeneratedSector, SectorError>
where
    F: FnMut(SectorProgress),
    C: FnMut() -> bool,
{
    macro_rules! check_cancelled {
        () => {
            if should_cancel() {
                return Err(SectorError::GenerationCancelled);
            }
        };
    }

    macro_rules! emit {
        ($event:expr) => {{
            check_cancelled!();
            progress($event);
            check_cancelled!();
        }};
    }

    let ProjectInput {
        config,
        catalogs,
        input_digests,
        root_dir: _,
    } = project;
    // §TF-P-1: catalogs live behind an `Arc`. Pull the immutable fields out by
    // ref so we don't deep-clone the workbook just to read it. `try_unwrap`
    // would consume the Arc when it's unique, but the seed-search loop holds
    // its own clones — borrowing keeps both paths cheap.
    let crate::loading::input::ProjectCatalogs {
        world_tables,
        world_rows,
        authored_features,
        names,
        factions,
        route_rules,
        relations: relations_cfg,
        regions: regions_cfg,
        economy: economy_cfg,
        history: history_cfg,
        ..
    } = &*catalogs;

    // docs/OPTIMIZE.txt G7: stage timings. Each `Instant::now()` is paired with a
    // `StageElapsed` emit; macro avoids the boilerplate.
    macro_rules! time_stage {
        ($name:literal, $start:expr) => {
            emit!(SectorProgress::StageElapsed {
                stage: $name,
                millis: $start.elapsed().as_millis() as u64,
            });
        };
    }

    // ── Stage 1: WorldPool ───────────────────────────────────────────────────
    // Scratch shared with later stages. `pool` is consumed by Systems (stage 4).
    let mut pool = crate::world_pool::WorldCandidatePool::default();
    if Stage::WorldPool <= through {
        let source_rows = world_rows.len();
        let t_pool = Instant::now();
        pool = crate::world_pool::build_pool(
            world_rows,
            world_tables,
            &config.generation.world_selection,
        );
        if let Some(features) = &authored_features {
            crate::world_pool::apply_authored_features(&mut pool, features);
        }
        emit!(SectorProgress::WorldPoolBuilt {
            rows: source_rows,
            candidates: pool.candidates.len(),
            excluded: pool.excluded_rows.len(),
        });
        time_stage!("world_pool", t_pool);
        if pool.candidates.is_empty() {
            return Err(SectorError::NoWorldCandidates);
        }
    }

    // ── Stage 2: Placement ─────────────────────────────────────────────────────
    let mut placements: Vec<crate::sector_model::HexCoord> = Vec::new();
    if Stage::Placement <= through {
        let t_place = Instant::now();
        placements = placement::place_systems_reroll(&config, &nonces.suffix(Stage::Placement))?;
        emit!(SectorProgress::SystemsPlaced {
            total: placements.len(),
            width: config.generation.sector_width,
            height: config.generation.sector_height,
        });
        time_stage!("placements", t_place);
    }
    let mut systems: Vec<GeneratedSystem> = Vec::with_capacity(placements.len());
    let mut used_names: BTreeSet<String> = BTreeSet::new();

    // ── Stage 3: Regions ───────────────────────────────────────────────────────
    // §5 NEW.md: regions stage runs BEFORE world generation so the `Anomaly`
    // condition can reweight the per-system candidate pool toward
    // warp-phenomena / ancient-ruins candidates without changing other stages.
    let mut warp_regions: Vec<crate::regions::WarpRegion> = Vec::new();
    let mut anomaly_hexes: BTreeSet<(i32, i32)> = BTreeSet::new();
    if Stage::Regions <= through {
        let t_regions = Instant::now();
        warp_regions = crate::regions::build_regions_reroll(
            &config.generation.seed,
            config.generation.sector_width,
            config.generation.sector_height,
            regions_cfg,
            &nonces.suffix(Stage::Regions),
        );
        anomaly_hexes = warp_regions
            .iter()
            .filter(|r| matches!(r.kind, crate::regions::RegionConditionKind::Anomaly))
            .flat_map(|r| r.hexes.iter().map(|h| (h.q, h.r)))
            .collect();
        emit!(SectorProgress::RegionsBuilt {
            count: warp_regions.len(),
        });
        time_stage!("regions", t_regions);
    }

    // ── Stage 4: Systems (+ Worlds sub-stage) ──────────────────────────────────
    if Stage::Systems <= through {
        let t_systems = Instant::now();
        let systems_suffix = nonces.suffix(Stage::Systems);
        for (idx, coord) in placements.iter().enumerate() {
            check_cancelled!();
            let system_index = idx + 1;
            let anomaly_bias = anomaly_hexes.contains(&(coord.q, coord.r));
            let system = systems::build_system_with_bias_reroll(
                &config,
                &pool,
                names,
                system_index,
                *coord,
                &mut used_names,
                anomaly_bias,
                &systems_suffix,
            )?;
            let worlds = system.worlds.len();
            systems.push(system);
            emit!(SectorProgress::SystemBuilt {
                current: system_index,
                total: placements.len(),
                worlds,
            });
        }
        time_stage!("systems_build", t_systems);
    }

    // ── Stage 5: Factions ──────────────────────────────────────────────────────
    let mut generated_factions: Vec<crate::sector_model::GeneratedFaction> = Vec::new();
    if Stage::Factions <= through {
        let t_factions = Instant::now();
        if !factions.is_empty() {
            let mut faction_rng = rng::stage_rng(
                &config.generation.seed,
                "factions",
                &format!("sector{}", nonces.suffix(Stage::Factions)),
            );
            factions::assign_factions(&mut systems, factions, &mut faction_rng);
            emit!(SectorProgress::FactionsAssigned {
                catalog_rows: factions.len(),
            });
        }

        generated_factions = factions::aggregate_factions(&systems, factions);
        emit!(SectorProgress::FactionsAggregated {
            factions: generated_factions.len(),
        });
        time_stage!("factions", t_factions);
    }

    // ── Stage 6: Routes (public) ───────────────────────────────────────────────
    // RNG key is reserved/unused today (`routes.rs` `let _ = rng;`); the stage is
    // gated so step 6 can stop here, but its discriminator is left UNCHANGED (no
    // nonce suffix) per `ITERATIVE_GENERATION.md` §3 / §9.
    let mut routes: Vec<GeneratedRoute> = Vec::new();
    if Stage::Routes <= through {
        let t_routes = Instant::now();
        routes = if config.generation.routes.enabled {
            emit!(SectorProgress::StageStarted {
                name: "public routes",
            });
            let mut route_rng = rng::stage_rng(&config.generation.seed, "routes", "sector");
            routes::generate_routes(&config, route_rules, &systems, &mut route_rng)
        } else {
            Vec::new()
        };
        emit!(SectorProgress::RoutesGenerated {
            routes: routes.len(),
        });
        time_stage!("public_routes", t_routes);
    }

    // ── Stage 7: RegionRouteEffects ────────────────────────────────────────────
    // §5 NEW.md: apply region effects to routes (storm → perilous, turbulence
    // → one tier worse, calm corridor → one tier better up to the perilous
    // ceiling). Idempotent. No RNG.
    if Stage::RegionRouteEffects <= through && regions_cfg.apply_to_routes && !warp_regions.is_empty()
    {
        emit!(SectorProgress::StageStarted {
            name: "region route effects",
        });
        let summary = crate::regions::apply_route_effects_with_progress(
            &warp_regions,
            &systems,
            &mut routes,
            config.generation.routes.max_route_distance,
            |event| match event {
                crate::regions::RegionRouteEffectsProgress::Started {
                    regions,
                    systems,
                    routes,
                } => progress(SectorProgress::RegionEffectsStarted {
                    regions,
                    systems,
                    routes,
                }),
                crate::regions::RegionRouteEffectsProgress::RouteScanned {
                    current,
                    total,
                    affected_routes,
                    changed_routes,
                    bridge_checks,
                    bridges_preserved,
                } => progress(SectorProgress::RegionEffectsProgress {
                    current,
                    total,
                    affected_routes,
                    changed_routes,
                    bridge_checks,
                    bridges_preserved,
                }),
                crate::regions::RegionRouteEffectsProgress::BridgeCheckStarted {
                    check,
                    route_index,
                    total_routes,
                    route_id,
                } => progress(SectorProgress::RegionEffectsBridgeCheckStarted {
                    check,
                    route_index,
                    total_routes,
                    route_id,
                }),
                crate::regions::RegionRouteEffectsProgress::Completed { .. } => {}
            },
        );
        emit!(SectorProgress::RegionEffectsApplied {
            regions: warp_regions.len(),
            affected_routes: summary.affected_routes,
            changed_routes: summary.changed_routes,
            bridge_checks: summary.bridge_checks,
            bridges_preserved: summary.bridges_preserved,
            stable: summary.stable,
            unstable: summary.unstable,
            hazardous: summary.hazardous,
            perilous: summary.perilous,
        });
    }

    // ── Stage 8: HiddenRoutes ──────────────────────────────────────────────────
    // §3 NEXT: append hidden route layers (webway / black-ship / smuggling)
    // before per-route control derivation so they receive the same control
    // treatment as public lanes. No RNG.
    if Stage::HiddenRoutes <= through && !generated_factions.is_empty() {
        emit!(SectorProgress::StageStarted {
            name: "hidden routes",
        });
        let before = routes.len();
        crate::hidden_routes::append_hidden_routes_with_regions_and_progress(
            &systems,
            &generated_factions,
            &warp_regions,
            &mut routes,
            |event| match event {
                crate::hidden_routes::HiddenRoutesProgress::LayerStarted { layer, endpoints } => {
                    progress(SectorProgress::HiddenRouteLayerStarted { layer, endpoints })
                }
                crate::hidden_routes::HiddenRoutesProgress::LayerProgress {
                    layer,
                    current,
                    total,
                    pairs,
                } => progress(SectorProgress::HiddenRouteLayerProgress {
                    layer,
                    current,
                    total,
                    pairs,
                }),
                crate::hidden_routes::HiddenRoutesProgress::LayerEmitProgress {
                    layer,
                    current,
                    total,
                    added,
                } => progress(SectorProgress::HiddenRouteLayerEmitProgress {
                    layer,
                    current,
                    total,
                    added,
                }),
                crate::hidden_routes::HiddenRoutesProgress::LayerCompleted {
                    layer,
                    added,
                    routes,
                } => progress(SectorProgress::HiddenRouteLayerCompleted {
                    layer,
                    added,
                    routes,
                }),
            },
        );
        emit!(SectorProgress::HiddenRoutesApplied {
            added: routes.len().saturating_sub(before),
            routes: routes.len(),
        });
    }

    // ── Stage 9: StabilityRebalance ────────────────────────────────────────────
    // §route-rebalance: with `stability_targets` configured, re-bucket public
    // route stabilities to the target mix now that every layer is in place
    // (public generation, region effects, hidden lanes). This replaces the
    // legacy early perilous-cap — skipped in `generate_routes` when targets are
    // set — so a storm-heavy sector still keeps a guaranteed Stable backbone.
    // No RNG.
    if Stage::StabilityRebalance <= through {
        if let Some(targets) = config.generation.routes.stability_targets {
            routes::rebalance_public_stability(&mut routes, targets);
        }
    }

    // ── Stage 10: RouteControls ────────────────────────────────────────────────
    // §3 per-route per-faction control. Derived after routes are built and
    // factions assigned, so endpoint presence reflects final state. No RNG.
    let t_route_ctl = Instant::now();
    if Stage::RouteControls <= through && !routes.is_empty() && !generated_factions.is_empty() {
        emit!(SectorProgress::StageStarted {
            name: "route controls",
        });
        let by_id: BTreeMap<&str, &GeneratedSystem> =
            systems.iter().map(|s| (s.id.as_str(), s)).collect();
        let route_total = routes.len();
        for (idx, r) in routes.iter_mut().enumerate() {
            check_cancelled!();
            r.controls =
                crate::route_control::derive_route_controls(r, &by_id, &generated_factions);
            emit!(SectorProgress::RouteControlsProgress {
                current: idx + 1,
                total: route_total,
            });
        }
        emit!(SectorProgress::RouteControlsDerived {
            routes: routes.len(),
        });
        time_stage!("route_controls", t_route_ctl);
    }

    // ── Stage 11: SystemState ──────────────────────────────────────────────────
    // §1 NEXT: per-world surface regions.
    // §2 NEXT: per-system orbital assets + blockade detection.
    // §5 NEXT: per-world + per-system initial conflict state.
    // §7 NEXT: per-system fog-of-war intel records. Observers are scoped to
    // factions with at least one presence IN THIS SYSTEM. The rumor-based
    // view for distant observers is reconstructible on demand from the raw
    // system state, so persisting it everywhere would O(F·S) and bloat
    // sector.json by tens of MB on large sectors with many factions. No RNG.
    if Stage::SystemState <= through {
        let t_sys_state = Instant::now();
        let system_total = systems.len();
        for (idx, sys) in systems.iter_mut().enumerate() {
            check_cancelled!();
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
            emit!(SectorProgress::SystemStateDerived {
                current: idx + 1,
                total: system_total,
            });
        }
        time_stage!("system_state", t_sys_state);
    }

    // ── Stage 12: Manifest + GeneratedSector construction ───────────────────────
    // PARTIAL ASSEMBLY (`ITERATIVE_GENERATION.md` partial-assembly requirement):
    // this stage is pure and RNG-free, so it ALWAYS runs — even for an early
    // cutoff — to produce a *valid*, renderable `GeneratedSector` from whatever
    // earlier stages built (empty `systems`/`routes`/`factions`/`regions` if
    // those stages were skipped). Overlays past the cutoff stay at `Default`.
    // The deterministic sort below mirrors a full run, so the prefix sector is
    // byte-identical to the full run's state at the same cutoff.
    // Sort everything for stable serialization.
    let mut sorted_systems = systems;
    sorted_systems.sort_by(|a, b| a.id.cmp(&b.id));
    let mut sorted_routes = routes;
    sorted_routes.sort_by(|a, b| a.id.cmp(&b.id));
    generated_factions.sort_by(|a, b| a.id.cmp(&b.id));

    let manifest = build_manifest(&config, &input_digests, &sorted_systems, &sorted_routes);
    emit!(SectorProgress::ManifestBuilt {
        systems: sorted_systems.len(),
        worlds: manifest.world_count,
        routes: sorted_routes.len(),
    });

    let mut sector = GeneratedSector {
        id: config.project.id.clone().into(),
        title: config.project.title.clone().into(),
        seed: config.generation.seed.clone().into(),
        generator_name: std::sync::Arc::from(crate::GENERATOR_NAME),
        generator_version: std::sync::Arc::from(crate::GENERATOR_VERSION),
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
        id_history: Default::default(),
    };

    // ── Stage 13: Archetypes ───────────────────────────────────────────────────
    // §11 NEXT: archetype rules. No RNG.
    if Stage::Archetypes <= through {
        let t_arch = Instant::now();
        emit!(SectorProgress::StageStarted {
            name: "archetypes overlay",
        });
        crate::archetypes::apply_all(&mut sector);
        emit!(SectorProgress::OverlayDerived { name: "archetypes" });
        time_stage!("archetypes", t_arch);
    }
    // ── Stage 14: PowerProjection ──────────────────────────────────────────────
    // §4 NEXT: power projection over routes (decays + doctrine). No RNG.
    if Stage::PowerProjection <= through {
        let t_pp = Instant::now();
        emit!(SectorProgress::StageStarted {
            name: "power projection overlay",
        });
        sector.power_projection = crate::power_projection::project_sector(&sector).into();
        crate::power_projection::apply_to_factions(&sector.power_projection, &mut sector.factions);
        emit!(SectorProgress::OverlayDerived {
            name: "power_projection",
        });
        time_stage!("power_projection", t_pp);
    }
    // ── Stage 15: InfluenceField ───────────────────────────────────────────────
    // §9 NEXT: continuous area layers. No RNG.
    if Stage::InfluenceField <= through {
        let t_if = Instant::now();
        emit!(SectorProgress::StageStarted {
            name: "influence field overlay",
        });
        sector.influence_field =
        crate::influence_field::build_with_progress(&sector, |event| match event {
            crate::influence_field::InfluenceFieldProgress::Started {
                systems,
                anchors,
                cells,
                radius,
            } => progress(SectorProgress::InfluenceFieldStarted {
                systems,
                anchors,
                cells,
                radius,
            }),
            crate::influence_field::InfluenceFieldProgress::AnchorsProjected {
                current,
                total,
                touched_cells,
            } => progress(SectorProgress::InfluenceFieldAnchorsProjected {
                current,
                total,
                touched_cells,
            }),
            crate::influence_field::InfluenceFieldProgress::CellsResolved {
                current,
                total,
                claimed_cells,
            } => progress(SectorProgress::InfluenceFieldCellsResolved {
                current,
                total,
                claimed_cells,
            }),
            crate::influence_field::InfluenceFieldProgress::BandsBuilt {
                bands,
                claimed_cells,
            } => progress(SectorProgress::InfluenceFieldBandsBuilt {
                bands,
                claimed_cells,
            }),
            crate::influence_field::InfluenceFieldProgress::Complete { cells, bands } => {
                progress(SectorProgress::InfluenceFieldComplete { cells, bands });
            }
        })
        .into();
        emit!(SectorProgress::OverlayDerived {
            name: "influence_field",
        });
        time_stage!("influence_field", t_if);
    }

    // ── Stage 16: Relations ────────────────────────────────────────────────────
    // §5 NEW2.md: derive inter-faction relationship matrix once factions are
    // finalised. Pure derivation, no extra RNG draws affect prior stages.
    // `[generation.relations].min_world_presence` controls how aggressively
    // the canonical faction list is filtered before the C(n,2) loop. The per-pair
    // RNG discriminator gets the `Stage::Relations` nonce suffix (empty by
    // default → unchanged legacy discriminator → byte-identical).
    if Stage::Relations <= through {
        let t_rel = Instant::now();
        emit!(SectorProgress::StageStarted {
            name: "relations overlay",
        });
        sector.relations = crate::relations::derive_with_threshold_reroll(
            &sector,
            relations_cfg,
            config.generation.relations.min_world_presence,
            &nonces.suffix(Stage::Relations),
        )
        .into();
        emit!(SectorProgress::OverlayDerived { name: "relations" });
        time_stage!("relations", t_rel);
    }

    // ── Stage 17: Economy ──────────────────────────────────────────────────────
    // §12 NEW.md: derive the economy snapshot last so it can read final
    // route stability + control records. Optional `feed_stability` nudge
    // applies after the snapshot is built. No RNG.
    if Stage::Economy <= through {
        let t_econ = Instant::now();
        emit!(SectorProgress::StageStarted {
            name: "economy overlay",
        });
        sector.economy = crate::economy::derive_with(&sector, economy_cfg).into();
        if economy_cfg.feed_stability && sector.economy.enabled {
            let snap = sector.economy.clone();
            crate::economy::apply_stability_nudge(&snap, &mut sector);
        }
        emit!(SectorProgress::OverlayDerived { name: "economy" });
        time_stage!("economy", t_econ);
    }

    // ── Stage 18: Chronicle ────────────────────────────────────────────────────
    // §1 NEW2.md: timeline / chronicle derives after all structural and
    // overlay state is final so events can reference routes, regions,
    // subsectors, claims, control, and present conflicts. The per-event RNG
    // discriminator gets the `Stage::Chronicle` nonce suffix (empty by default →
    // unchanged legacy discriminator → byte-identical).
    if Stage::Chronicle <= through {
        let t_chron = Instant::now();
        emit!(SectorProgress::StageStarted {
            name: "chronicle overlay",
        });
        sector.chronicle = crate::history::derive_with_progress_reroll(
            &sector,
            history_cfg,
            &nonces.suffix(Stage::Chronicle),
            |event| match event {
            crate::history::HistoryProgress::Started {
                systems,
                worlds,
                routes,
                max_subsector_events,
            } => progress(SectorProgress::ChronicleStarted {
                systems,
                worlds,
                routes,
                max_subsector_events,
            }),
            crate::history::HistoryProgress::SubsectorEventsStarted {
                exact_cluster_count,
                emitted_cap,
                sampled,
            } => progress(SectorProgress::ChronicleSubsectorEventsStarted {
                exact_cluster_count,
                emitted_cap,
                sampled,
            }),
            crate::history::HistoryProgress::SubsectorEventsDone { events } => {
                progress(SectorProgress::ChronicleSubsectorEventsDone { events });
            }
            crate::history::HistoryProgress::SystemsScanned {
                current,
                total,
                events,
            } => progress(SectorProgress::ChronicleSystemsScanned {
                current,
                total,
                events,
            }),
            crate::history::HistoryProgress::RoutesScanned {
                current,
                total,
                events,
            } => progress(SectorProgress::ChronicleRoutesScanned {
                current,
                total,
                events,
            }),
            crate::history::HistoryProgress::EventRulesApplied { events } => {
                progress(SectorProgress::ChronicleEventRulesApplied { events });
            }
            crate::history::HistoryProgress::SortingStarted { events } => {
                progress(SectorProgress::ChronicleSortingStarted { events });
            }
            crate::history::HistoryProgress::Complete { events } => {
                progress(SectorProgress::ChronicleComplete { events });
            }
            },
        );
        emit!(SectorProgress::OverlayDerived { name: "chronicle" });
        time_stage!("chronicle", t_chron);
    }

    emit!(SectorProgress::Complete {
        systems: sector.manifest.system_count,
        worlds: sector.manifest.world_count,
        routes: sector.manifest.route_count,
    });

    Ok(sector)
}

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
        generated_at_policy: std::sync::Arc::from("not recorded by default"),
        generator_name: std::sync::Arc::from(crate::GENERATOR_NAME),
        generator_version: std::sync::Arc::from(crate::GENERATOR_VERSION),
        seed: config.generation.seed.clone().into(),
        seed_hash: seed_hash.into(),
        base_seed: config
            .generation
            .search_base_seed
            .as_ref()
            .map(|s| s.as_str().into()),
        candidate_index: config.generation.search_candidate_index,
        constraints_digest: config
            .generation
            .search_constraints_digest
            .as_ref()
            .map(|s| s.as_str().into()),
        profile: None,
        input_digests: input_digests.clone(),
        settings_digest: settings_digest.into(),
        system_count: systems.len(),
        world_count,
        route_count: routes.len(),
    }
}
