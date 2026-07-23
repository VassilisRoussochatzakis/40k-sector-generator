//! ITERATIVE_GENERATION.md Phase S — transient session state + off-thread
//! prefix-runner + re-roll / edit ops for the stage-by-stage random generator.
//!
//! # What lives here (and why it is *not* document state)
//!
//! [`IterativeGenSession`] is the in-progress wizard: the working [`AppConfig`]
//! knobs, the `root_seed`, the per-stage [`RerollNonces`], the user's current
//! step, the cached preview [`GeneratedSector`], the destination folder, and the
//! background prefix-run job handle. Per the §V2 transient carve-out
//! ([CLAUDE.md] command-bus invariant; ITERATIVE_GENERATION.md §2.4 + invariant
//! #5) **none of this is document state** — it never lands in `sector.json` and
//! has nothing to undo, so it is written **directly** to
//! [`super::BuilderState::iterative_gen`], *not* through the command bus.
//! Re-rolling a stage just bumps a nonce and re-runs the preview; the only
//! document mutation is the final commit (Phase C), which for a brand-new
//! project is a session boundary (`open_project`) and needs no command — exactly
//! like `random_run.rs` / `generate_random.rs` `apply_result` today.
//!
//! # The prefix-run architecture (ITERATIVE_GENERATION.md §2.1 / §2.2)
//!
//! To show the sector *as of step K* we re-run the engine pipeline from stage 1
//! through the engine cutoff of step K via [`sectorforge::generate_prefix`] and
//! keep the partial [`GeneratedSector`]. Because every stage seeds a *fresh*
//! `ChaCha8Rng` from `blake3("sectorforge:{seed}:{stage}:{disc}")` and stages
//! share no stream, a prefix run is byte-identical to the corresponding prefix
//! of a full run for the same `(config, seed, nonces)`. Re-rolling step K bumps
//! only step K's nonce, leaving earlier stages frozen.
//!
//! # The DAG rule (ITERATIVE_GENERATION.md §2.3)
//!
//! Editing or re-rolling a step invalidates that step **and every later step**
//! (worlds depend on anomalies, factions on worlds, routes on systems, overlays
//! on everything). We implement this for free: bump the edited step's nonce (for
//! a re-roll) or clear `accepted_through` back to the edited step (for a knob
//! edit), then **always re-run the prefix through the current step**.
//!
//! # Preview cutoff nuance (verified, by design — see [`GenStep::preview_through`])
//!
//! A literal `Stage::Placement` cutoff builds **no** [`GeneratedSystem`]s
//! (systems are constructed at stage 4 = `Stage::Systems`; bare placement hexes
//! are orchestrator scratch, not a [`GeneratedSector`] field), so it would
//! render an empty-of-systems grid. For a *useful* preview on the early
//! structural steps we render the prefix through at least `Stage::Systems` so
//! systems appear at their placed hexes — while a **re-roll** of that step still
//! bumps the step's *own* `stage()` nonce (re-rolling Placement bumps
//! `Stage::Placement` ⇒ systems move; re-rolling Regions bumps `Stage::Regions`).
//! `Size` runs nothing (empty grid). This is the clean v1 choice.

use camino::Utf8PathBuf;
use std::sync::{Arc, Mutex};

use sectorforge::config::AppConfig;
use sectorforge::factions::FactionDef;
use sectorforge::random_sector::{build_random_config, SectorSize, MAX_CUSTOM_DIM};
use sectorforge::regions::RegionsConfig;
use sectorforge::sector_model::GeneratedSector;
use sectorforge::{generate_prefix, RerollNonces, SectorProgress, Stage};

use sectorforge_gui_core::jobs::{spawn_job, FromJobPanic, JobHandle};

use crate::builder::data_catalogs::DataCatalogs;

use super::BuilderState;

/// The seven user-facing steps of the iterative wizard (ITERATIVE_GENERATION.md
/// §2.3). Declaration order is presentation order; the rail walks
/// [`GenStep::ALL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GenStep {
    /// Size & seed — config only, no engine run (renders an empty grid).
    Size,
    /// System placement — `Stage::Placement`.
    Placement,
    /// Warp regions — `Stage::Regions`.
    Regions,
    /// System contents (systems + worlds) — `Stage::Systems`.
    Systems,
    /// Factions — `Stage::Factions`.
    Factions,
    /// Routes (+ effects / hidden / rebalance / controls) — `Stage::RouteControls`.
    Routes,
    /// Finalize & overlays (everything through chronicle) — `Stage::Chronicle`.
    Finalize,
}

impl GenStep {
    /// Canonical presentation order (also the DAG order). The step rail and the
    /// Back/Next gestures walk this slice.
    pub const ALL: [GenStep; 7] = [
        GenStep::Size,
        GenStep::Placement,
        GenStep::Regions,
        GenStep::Systems,
        GenStep::Factions,
        GenStep::Routes,
        GenStep::Finalize,
    ];

    /// The engine [`Stage`] this step *owns* — i.e. the stage whose re-roll nonce
    /// a "re-roll this step" gesture bumps, per the ITERATIVE_GENERATION.md §2.3
    /// table. `Size` owns no stage (config only ⇒ `None`).
    ///
    /// | Step | `stage()` |
    /// |---|---|
    /// | `Size` | `None` |
    /// | `Placement` | `Stage::Placement` |
    /// | `Regions` | `Stage::Regions` |
    /// | `Systems` | `Stage::Systems` |
    /// | `Factions` | `Stage::Factions` |
    /// | `Routes` | `Stage::RouteControls` |
    /// | `Finalize` | `Stage::Chronicle` |
    #[must_use]
    pub fn stage(&self) -> Option<Stage> {
        match self {
            GenStep::Size => None,
            GenStep::Placement => Some(Stage::Placement),
            GenStep::Regions => Some(Stage::Regions),
            GenStep::Systems => Some(Stage::Systems),
            GenStep::Factions => Some(Stage::Factions),
            GenStep::Routes => Some(Stage::RouteControls),
            GenStep::Finalize => Some(Stage::Chronicle),
        }
    }

    /// The cutoff the **preview** is actually rendered through (see the module
    /// "Preview cutoff nuance"). Identical to [`Self::stage`] except the early
    /// structural steps render through `Stage::Systems` so placed systems are
    /// visible:
    ///
    /// - `Size` → `None` (no run; empty grid).
    /// - `Placement` / `Regions` → `Stage::Systems` (so systems appear).
    /// - everything else → its own [`Self::stage`].
    ///
    /// A re-roll still bumps the step's own [`Self::stage`] nonce, so re-rolling
    /// Placement (which renders through Systems) moves the systems exactly as
    /// intended.
    #[must_use]
    pub fn preview_through(&self) -> Option<Stage> {
        match self {
            GenStep::Size => None,
            GenStep::Placement | GenStep::Regions => Some(Stage::Systems),
            other => other.stage(),
        }
    }

    /// Index into [`Self::ALL`].
    #[must_use]
    pub fn index(&self) -> usize {
        GenStep::ALL.iter().position(|s| s == self).unwrap_or(0)
    }

    /// Short rail label.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            GenStep::Size => "Size & seed",
            GenStep::Placement => "System placement",
            GenStep::Regions => "Warp regions",
            GenStep::Systems => "System contents",
            GenStep::Factions => "Factions",
            GenStep::Routes => "Routes",
            GenStep::Finalize => "Finalize & overlays",
        }
    }

    /// The next step in DAG order, or `None` at [`GenStep::Finalize`].
    #[must_use]
    pub fn next(&self) -> Option<GenStep> {
        GenStep::ALL.get(self.index() + 1).copied()
    }

    /// The previous step in DAG order, or `None` at [`GenStep::Size`].
    #[must_use]
    pub fn prev(&self) -> Option<GenStep> {
        self.index().checked_sub(1).map(|i| GenStep::ALL[i])
    }
}

/// Result the off-thread prefix runner posts back over the channel
/// (ITERATIVE_GENERATION.md §S2). Mirrors `random_run.rs::RandomJobResult`, but
/// carries the partial [`GeneratedSector`] inline — there is **no** session
/// boundary on a prefix run (the preview merges into `session.preview`, the live
/// `state.sector` is untouched until commit).
pub enum IterativeJobResult {
    /// The prefix run finished; `through` echoes the cutoff so a stale result
    /// (a newer dispatch superseded it) can still be identified, and `sector` is
    /// the partial [`GeneratedSector`] to stash into `session.preview`.
    Done {
        through: Stage,
        sector: Box<GeneratedSector>,
    },
    /// The prefix run (or input synthesis) failed.
    Failed(String),
}

impl FromJobPanic for IterativeJobResult {
    fn from_job_panic(message: String) -> Option<Self> {
        Some(Self::Failed(format!(
            "Iterative prefix run panicked: {message}"
        )))
    }
}

/// Transient wizard state (ITERATIVE_GENERATION.md §S1). **Never serialised,
/// never on the undo stack** — the §V2 / invariant #5 carve-out. Written
/// directly to [`super::BuilderState::iterative_gen`]; no `BuilderCommand`.
pub struct IterativeGenSession {
    /// Working generation knobs — baseline-scaffolded (from
    /// [`build_random_config`]) then user-edited in place. Transient; the panel
    /// reads/writes `config.generation.*` directly.
    pub config: AppConfig,
    /// The root seed every stage RNG is keyed from. Minted by the caller for a
    /// fresh wizard; never re-derived inside the pipeline.
    pub root_seed: String,
    /// Per-stage re-roll nonces. Lives on the **session** (transient), never in
    /// `sector.json`. Nonce 0/absent ⇒ `""` suffix ⇒ byte-identical to one-shot.
    pub nonces: RerollNonces,
    /// Transient, session-level **warp-regions override** (ITERATIVE_GENERATION.md
    /// §5 + invariant #5). [`RegionsConfig`] lives in a *separate* TOML
    /// (`data/routes/regions.toml`) and is sourced into
    /// [`super::BuilderState::data_catalogs`] — **not** `[generation]`. The §5
    /// footnote says to "edit it via an in-memory patch applied at load,
    /// preserving the split": this `Option` is that patch.
    ///
    /// - `None` ⇒ no patch: the prefix run uses the project's
    ///   `data_catalogs.regions` exactly as today, so the output is
    ///   **byte-identical** to the legacy path (invariants #2 / #6).
    /// - `Some(cfg)` ⇒ `cfg` is substituted for the regions catalog **only when
    ///   assembling the `ProjectInput`** for a prefix/commit run
    ///   ([`BuilderState::synthesize_project_input_with`]). It is **never**
    ///   written into `data_catalogs` or `sector.json` during preview — it is
    ///   pure transient session state, written directly (no `BuilderCommand`,
    ///   invariant #5 carve-out). Only the new-project **commit**
    ///   ([`BuilderState::commit_new_project`]) folds it into the freshly written
    ///   project's regions catalog, and that is a session boundary
    ///   (`open_project`) requiring no command.
    ///
    /// Lazily initialised by the panel from the *effective* catalog config on
    /// the first Regions-step edit.
    pub regions_override: Option<RegionsConfig>,
    /// Transient, session-level **data-catalog override** for a wizard launched
    /// from a builder with **no project open** (so `data_catalogs.worlds` is
    /// `None` and no prefix/commit run could otherwise assemble a
    /// [`sectorforge::ProjectInput`] — the user's "nothing works"). When `Some`,
    /// this full catalog set (seeded from the bundled `_base` preset at launch —
    /// worlds, factions, names, regions, route rules, …) is substituted for
    /// [`super::BuilderState::data_catalogs`] **only while assembling the
    /// `ProjectInput`** for a prefix preview ([`BuilderState::spawn_prefix`]) and
    /// the final full run, then installed into the freshly written project at
    /// **commit** ([`BuilderState::commit_new_project`]) so worlds.toml /
    /// factions.toml / … are written and survive the reopen.
    ///
    /// - `None` ⇒ the builder already has a project loaded; the prefix run uses
    ///   the live `data_catalogs` exactly as before (byte-identical to the
    ///   project-open path).
    /// - `Some(catalogs)` ⇒ blank-builder launch; the override supplies the world
    ///   pool the wizard needs. Like [`Self::regions_override`] it is pure
    ///   transient session state (no `BuilderCommand`, §V2 / invariant #5
    ///   carve-out); it touches document state only at the commit session
    ///   boundary (`open_project` + `*self = new_state`), exactly like the
    ///   one-shot random generator's `apply_result`.
    pub catalogs_override: Option<DataCatalogs>,
    /// The full faction roster available to **pick from** in the Factions step,
    /// grouped hierarchically (top faction → branch → force) by the picker.
    /// Seeded once at launch from the source roster — the open project's factions,
    /// or the bundled `_base` roster for a blank-builder launch — while the
    /// wizard's *working* roster (`catalogs_override.factions`) starts **empty**.
    /// The user adds entries from this palette one at a time, or authors custom
    /// ones. Pure transient session state (invariant #5 carve-out); never written
    /// to `sector.json`, never on the undo stack.
    pub faction_palette: Vec<FactionDef>,
    /// The step the user is currently inspecting / tuning.
    pub current_step: GenStep,
    /// Furthest step committed-in-session (advanced by Next). `None` until the
    /// user steps past `Size`. A knob edit on step K clears this back to K
    /// (downstream DAG invalidation, §2.3).
    pub accepted_through: Option<GenStep>,
    /// Last prefix-run result, fed to the embedded `SectorView`. `None` until the
    /// first run lands; replaced wholesale on every successful run.
    pub preview: Option<GeneratedSector>,
    /// Destination project folder. `None` until the user picks one (required to
    /// Commit a new project, not to preview).
    pub dest: Option<Utf8PathBuf>,
    /// Handle to the in-flight background prefix run (same machinery as
    /// `RandomGenState::job`). `Some` between dispatch and result drain.
    pub job: Option<JobHandle<IterativeJobResult>>,
    /// Live progress snapshot from the running prefix worker (latest
    /// [`SectorProgress`] event), read by the panel's progress line. Cleared when
    /// the job finishes.
    pub progress: Arc<Mutex<Option<SectorProgress>>>,
    /// Bumped on every [`BuilderState::spawn_prefix`] / cancel; stale worker
    /// results are dropped on [`BuilderState::pump_prefix`].
    pub revision: u64,
}

impl IterativeGenSession {
    /// Open a fresh wizard. `baseline` is the project's current [`AppConfig`]
    /// (so `inputs.*` catalog paths, project id/title, output knobs are
    /// inherited); the **generation knobs** are then overwritten with the
    /// byte-deterministic [`build_random_config`] roll for `(seed, size)` so the
    /// wizard opens on exactly the defaults a one-shot
    /// `generate_random_sector_from` would have produced (ITERATIVE_GENERATION.md
    /// §2.2 equivalence / §5). `seed` is the root seed (mint it before calling
    /// for a brand-new wizard).
    ///
    /// Invariant #4 (square sectors): the rolled config's
    /// `sector_width == sector_height == size.dim()`; the panel's size field must
    /// drive both via [`BuilderState::set_grid_dim`].
    #[must_use]
    pub fn new(baseline: AppConfig, seed: String, size: SectorSize) -> Self {
        let rolled = build_random_config(&seed, size);
        let mut config = baseline;
        // Inherit catalog paths / project / output knobs from the baseline, but
        // adopt the rolled generation block (seed, square dims, counts, placement
        // / world-selection / routes / relations) so the wizard's starting point
        // is byte-identical to the one-shot roller.
        config.generation = rolled.generation;
        config.generation.seed = seed.clone();
        Self {
            config,
            root_seed: seed,
            nonces: RerollNonces::default(),
            regions_override: None,
            catalogs_override: None,
            faction_palette: Vec::new(),
            current_step: GenStep::Size,
            accepted_through: None,
            preview: None,
            dest: None,
            job: None,
            progress: Arc::new(Mutex::new(None)),
            revision: 0,
        }
    }

    /// True while a prefix worker is in flight.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    /// Overall completion in `[0.0, 1.0]` for a progress bar (from the job
    /// handle). `0.0` when no job is running.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        self.job.as_ref().map_or(0.0, JobHandle::progress)
    }

    /// Latest [`SectorProgress`] snapshot (cloned out of the shared cell), or
    /// `None` when nothing is running / no event has landed yet.
    #[must_use]
    pub fn progress_snapshot(&self) -> Option<SectorProgress> {
        self.progress.lock().unwrap().clone()
    }
}

/// Map a [`SectorProgress`] milestone to a coarse completion fraction in
/// `[0.0, 1.0)` for the iterative-preview progress bar, relative to the prefix
/// cutoff `through`. Returns `None` for fine-grained sub-events we don't anchor
/// the bar on — the worker holds the bar at its last value through those.
///
/// Each engine [`Stage`] owns a `1 / (through + 1)` slice of the bar; the long
/// per-item stages (systems, route controls, system state, influence field,
/// chronicle scans) interpolate *inside* their slice from their `current/total`
/// counters, so the bar climbs smoothly instead of stepping once per stage.
///
/// UI-only: the fraction never feeds back into generation, so it has no bearing
/// on determinism or golden output.
fn preview_progress_fraction(p: &SectorProgress, through: Stage) -> Option<f32> {
    use SectorProgress as P;
    let (stage, within): (Stage, f32) = match p {
        P::WorldPoolBuilt { .. } => (Stage::WorldPool, 1.0),
        P::SystemsPlaced { .. } => (Stage::Placement, 1.0),
        P::RegionsBuilt { .. } => (Stage::Regions, 1.0),
        P::SystemBuilt { current, total, .. } => (Stage::Systems, item_frac(*current, *total)),
        P::FactionsAssigned { .. } => (Stage::Factions, 0.5),
        P::FactionsAggregated { .. } => (Stage::Factions, 1.0),
        P::RoutesGenerated { .. } => (Stage::Routes, 1.0),
        P::RegionEffectsProgress { current, total, .. } => {
            (Stage::RegionRouteEffects, item_frac(*current, *total))
        }
        P::RegionEffectsApplied { .. } => (Stage::RegionRouteEffects, 1.0),
        P::HiddenRouteLayerProgress { current, total, .. }
        | P::HiddenRouteLayerEmitProgress { current, total, .. } => {
            (Stage::HiddenRoutes, item_frac(*current, *total))
        }
        P::HiddenRoutesApplied { .. } => (Stage::HiddenRoutes, 1.0),
        P::RouteControlsProgress { current, total } => {
            (Stage::RouteControls, item_frac(*current, *total))
        }
        P::RouteControlsDerived { .. } => (Stage::RouteControls, 1.0),
        P::SystemStateDerived { current, total } => {
            (Stage::SystemState, item_frac(*current, *total))
        }
        P::ManifestBuilt { .. } => (Stage::Manifest, 1.0),
        P::InfluenceFieldAnchorsProjected { current, total, .. }
        | P::InfluenceFieldCellsResolved { current, total, .. } => {
            (Stage::InfluenceField, item_frac(*current, *total))
        }
        P::InfluenceFieldComplete { .. } => (Stage::InfluenceField, 1.0),
        P::ChronicleSystemsScanned { current, total, .. }
        | P::ChronicleRoutesScanned { current, total, .. } => {
            (Stage::Chronicle, item_frac(*current, *total))
        }
        P::ChronicleComplete { .. } | P::Complete { .. } => (Stage::Chronicle, 1.0),
        // Stage-boundary / overlay / timing / counter-less events aren't anchored
        // (StageStarted, OverlayDerived, *Started, StageElapsed, bridge checks,
        // influence bands), nor are the no-event stages (StabilityRebalance,
        // Archetypes, PowerProjection, Relations, Economy) — hold the last value.
        _ => return None,
    };
    let denom = (through as usize + 1) as f32;
    let filled = (stage as usize as f32 + within) / denom;
    Some(filled.clamp(0.0, 0.999))
}

/// `current / total` clamped to `[0.0, 1.0]`, treating an empty stage as full so
/// a zero-item stage still advances the bar to its slice boundary.
#[inline]
fn item_frac(current: usize, total: usize) -> f32 {
    if total == 0 {
        1.0
    } else {
        (current as f32 / total as f32).clamp(0.0, 1.0)
    }
}

/// Upper bound on the number of candidate route pairs (~`effective_systems²/2`)
/// the generator will build. The routes stage materialises this many pairs in a
/// single O(n²) allocation; past this a synchronous [`BuilderState::commit_new_project`]
/// run — which is **uncancellable** (it passes `|| false`) — would allocate
/// hundreds of MB and hang or OOM the process. Sized to admit the densest shipped
/// preset (`Huge` = 80×80, up to ~2560 systems ⇒ ~3.3M pairs) while rejecting a
/// runaway near-1.0 density (80×80 ⇒ 6400 systems ⇒ ~20.5M pairs).
pub(crate) const MAX_GEN_ROUTE_PAIRS: u64 = 5_000_000;

/// Pre-flight sanity gate for a wizard config (ITERATIVE_GENERATION.md Phase S guard):
/// reject anything structurally invalid or so large/dense that a synchronous full
/// run would hang or OOM, **before** dispatching a preview or committing. Returns
/// a human-readable reason on rejection.
///
/// Checks, in order: (1) the square-sector dimension against [`MAX_CUSTOM_DIM`] —
/// the same ergonomic cap the one-shot random generator + CLI enforce; (2) an
/// inverted worlds-per-system range (`min > max`), which would make the systems
/// stage sample an empty range and panic; (3) the route-pair cost budget
/// ([`MAX_GEN_ROUTE_PAIRS`]) using the *effective* system count (placement caps to
/// the hex capacity, so a huge `system_count` on a tiny grid is not falsely
/// rejected).
///
/// Allocation-free and deterministic — it never draws RNG, so for a config that
/// passes it has **no effect** on generated output (the nonce-0 / valid path stays
/// byte-identical, invariants #2/#6).
pub(crate) fn precheck_generatable(config: &AppConfig) -> Result<(), String> {
    let g = &config.generation;

    // (1) Square-sector dimension cap — align the wizard with the random
    // generator + CLI. `set_grid_dim` already clamps the UI field, so this is the
    // defence-in-depth path that also catches a config loaded/edited out of band.
    let dim = g.sector_width.max(g.sector_height);
    if dim > MAX_CUSTOM_DIM {
        return Err(format!(
            "Sector size {dim}×{dim} exceeds the maximum supported grid \
             {MAX_CUSTOM_DIM}×{MAX_CUSTOM_DIM}. Lower the grid dimension on the Size step."
        ));
    }

    // (2) Inverted worlds-per-system range — `min > max` panics the systems stage
    // (`gen_range(min..=max)` on an empty range). Reject with a clear message.
    if g.min_worlds_per_system > g.max_worlds_per_system {
        return Err(format!(
            "Worlds-per-system range is inverted: min ({}) must not exceed max ({}).",
            g.min_worlds_per_system, g.max_worlds_per_system
        ));
    }

    // (3) Route-cost budget — the dominant O(n²) allocation. Use the *effective*
    // system count: placement caps to the hex capacity, so `system_count` far
    // above `dim²` costs no more than a full grid and must not be over-rejected.
    let cells = u64::from(g.sector_width) * u64::from(g.sector_height);
    let systems = (g.system_count as u64).min(cells);
    let pairs = systems.saturating_mul(systems.saturating_sub(1)) / 2;
    if pairs > MAX_GEN_ROUTE_PAIRS {
        return Err(format!(
            "This configuration is too dense to generate: {systems} systems on a {dim}×{dim} \
             grid would build ~{pairs} candidate route pairs, far past what one run can hold. \
             Lower the system count / world density or shrink the sector."
        ));
    }

    Ok(())
}

impl BuilderState {
    /// ITERATIVE_GENERATION.md §S2 — dispatch a background prefix run through
    /// `through`, off the UI thread. Assembles a [`sectorforge::ProjectInput`]
    /// from the **session** config + the loaded data catalogs (NOT the live
    /// `state.config` — the wizard is editing its own working knobs), then calls
    /// [`sectorforge::generate_prefix`] with the session nonces, forwarding every
    /// [`SectorProgress`] into the session progress cell. The result is drained
    /// by [`Self::pump_prefix`] and stashed into `session.preview`.
    ///
    /// No-op (returns silently) when there is no active session, or when the
    /// worlds catalog is missing (no [`sectorforge::ProjectInput`] can be built —
    /// the panel should surface that separately). Cancels any prior in-flight run
    /// first, exactly like `RandomGenState::spawn`.
    ///
    /// Determinism (invariants #1/#2): all entropy is the engine's — the nonces
    /// thread the `:r{n}` suffix into each stage discriminator, and nonce-0 is
    /// byte-identical to the one-shot path. The runner never calls
    /// `rand::thread_rng()`.
    pub fn spawn_prefix(&mut self, ctx: &egui::Context, through: Stage) {
        // Pre-flight sanity gate (ITERATIVE_GENERATION.md Phase S guard): refuse to
        // dispatch a run for an oversized / too-dense / structurally invalid config.
        // Otherwise the off-thread job would build an O(n²) route-pair vector
        // (hundreds of MB at a near-1.0 density) and the preview would stall —
        // surface *why*, like the missing-worlds-catalog case below.
        match self
            .iterative_gen
            .as_ref()
            .map(|s| precheck_generatable(&s.config))
        {
            None => return,
            Some(Err(reason)) => {
                self.cancel_prefix();
                self.feedback.last_command_error = Some(format!("Cannot build preview: {reason}"));
                return;
            }
            Some(Ok(())) => {}
        }

        let Some(session) = self.iterative_gen.as_mut() else {
            return;
        };

        // Build the read-only ProjectInput from the *session* config + catalogs.
        // `synthesize_project_input_with` reads `self.config`, so temporarily swap
        // the session knobs in, snapshot the input, then restore. This keeps a
        // single catalog-assembly code path (worlds/names/factions/regions/…)
        // without duplicating it here.
        //
        // §5 / invariant #5: the session's transient `regions_override` (if any)
        // is threaded into the assembly *here* — it substitutes for the regions
        // catalog for this run only, and is NOT written into `data_catalogs`
        // (preview must not mutate document state). `None` ⇒ verbatim catalog ⇒
        // byte-identical to the legacy path (invariants #2/#6).
        let session_config = session.config.clone();
        let regions_override = session.regions_override.clone();
        let catalogs_override = session.catalogs_override.clone();
        let saved_config = std::mem::replace(&mut self.config, session_config);
        // Blank-builder launch: the session carries a `_base`-seeded catalog set
        // because the live `data_catalogs` has no world pool. Swap it in for the
        // read-only assembly, then restore — the preview never persistently
        // mutates the live document (invariant #5). `None` ⇒ a project is open ⇒
        // use the live catalogs exactly as before.
        let saved_catalogs =
            catalogs_override.map(|c| std::mem::replace(&mut self.data_catalogs, c));
        let input = self.synthesize_project_input_with(regions_override.as_ref());
        if let Some(saved) = saved_catalogs {
            self.data_catalogs = saved;
        }
        self.config = saved_config;

        let Some(input) = input else {
            // No worlds catalog — cannot run. Leave any prior preview in place
            // and surface *why* nothing happened, otherwise the panel just sits
            // on the empty-state with no explanation (this silent return is
            // exactly the "nothing happens" the user hit).
            self.feedback.last_command_error = Some(
                "Cannot build preview: this project has no worlds catalog loaded.".to_string(),
            );
            return;
        };

        // A fresh run is dispatching — clear any stale preview-error banner from a
        // previous failed/aborted attempt so it doesn't shadow this run.
        self.feedback.last_command_error = None;

        let session = self.iterative_gen.as_mut().expect("session present above");
        session.revision = session.revision.wrapping_add(1);
        if let Some(job) = session.job.take() {
            job.cancel();
        }
        *session.progress.lock().unwrap() = None;

        let revision = session.revision;
        let nonces = session.nonces.clone();
        let progress_cell = Arc::clone(&session.progress);

        session.job = Some(spawn_job(
            "builder-iterative-prefix",
            revision,
            "Generating preview…",
            ctx.clone(),
            move |job_ctx| {
                // Both the progress and cancel closures borrow `job_ctx` (its
                // `set_progress` / `is_cancelled` take `&self`), so capture it by
                // reference here rather than cloning the private cancel cell.
                let job_ctx = &job_ctx;
                // Monotonic [0.0, 1.0) bar fill derived from the per-stage
                // milestones relative to `through` (see `preview_progress_fraction`).
                // Kept in a running `max(...)` so a coarse sub-event can never drag
                // the bar backwards. UI-only — it never feeds back into generation,
                // so determinism / golden output are untouched. (Was a constant
                // 0.5, which read as a frozen, half-full bar — the user's "nothing
                // happens".)
                let mut bar_frac = 0.0_f32;
                let result = generate_prefix(
                    input,
                    through,
                    &nonces,
                    |p: SectorProgress| {
                        if let Some(f) = preview_progress_fraction(&p, through) {
                            bar_frac = bar_frac.max(f);
                        }
                        // Small floor so the bar reads as "started" the instant the
                        // first event lands, before the first anchored stage.
                        job_ctx.set_progress(bar_frac.max(0.03));
                        *progress_cell.lock().unwrap() = Some(p);
                    },
                    || job_ctx.is_cancelled(),
                );
                match result {
                    Ok(sector) => {
                        job_ctx.set_progress(1.0);
                        IterativeJobResult::Done {
                            through,
                            sector: Box::new(sector),
                        }
                    }
                    Err(e) => IterativeJobResult::Failed(format!("Preview generation failed: {e}")),
                }
            },
        ));
    }

    /// ITERATIVE_GENERATION.md §S2 — per-frame drain of the prefix-run channel,
    /// mirroring `RandomGenState::pump`. On a fresh, current-revision
    /// [`IterativeJobResult::Done`] it stashes the partial sector into
    /// `session.preview` and returns `true` (the panel should request a repaint
    /// so the [`SectorView`] picks up the new sector). On `Failed` it records the
    /// message in `feedback.last_command_error` and returns `true`. Returns
    /// `false` while the worker is still running, no job/session exists, or a
    /// stale result was dropped.
    ///
    /// Call this once per frame from the panel (the random-gen wizard pumps its
    /// job the same way inside `generate_random.rs::show`).
    pub fn pump_prefix(&mut self) -> bool {
        let Some(session) = self.iterative_gen.as_mut() else {
            return false;
        };
        let drained = session.job.as_ref().and_then(|job| {
            use std::sync::mpsc::TryRecvError;
            match job.receiver.try_recv() {
                Ok(result) => Some((job.revision, result)),
                Err(TryRecvError::Disconnected) => Some((
                    job.revision,
                    IterativeJobResult::Failed("preview worker disconnected".to_string()),
                )),
                Err(TryRecvError::Empty) => None,
            }
        });

        let Some((revision, result)) = drained else {
            return false;
        };
        session.job = None;
        *session.progress.lock().unwrap() = None;
        if revision != session.revision {
            // A newer dispatch / cancel superseded this job; drop the result.
            return false;
        }

        match result {
            IterativeJobResult::Done { sector, .. } => {
                session.preview = Some(*sector);
                true
            }
            IterativeJobResult::Failed(msg) => {
                self.feedback.last_command_error = Some(msg);
                true
            }
        }
    }

    /// Detach the in-flight prefix run (the worker keeps running; its result is
    /// dropped on the next [`Self::pump_prefix`] because the revision no longer
    /// matches). No-op without an active session.
    pub fn cancel_prefix(&mut self) {
        let Some(session) = self.iterative_gen.as_mut() else {
            return;
        };
        if let Some(job) = session.job.take() {
            job.cancel();
        }
        session.revision = session.revision.wrapping_add(1);
        *session.progress.lock().unwrap() = None;
    }

    /// ITERATIVE_GENERATION.md Phase C1 — **new-project commit**. Materialise the
    /// in-progress wizard into a brand-new project on disk and reopen it.
    ///
    /// This is the *only* document mutation the wizard performs, and for a fresh
    /// project it is a **session boundary** — exactly like
    /// `generate_random.rs::apply_result` / the new-project + new-from-preset
    /// wizards — so it legitimately replaces the whole [`BuilderState`] and
    /// clears undo **without** a `BuilderCommand` (invariant #5 carve-out, §2.4).
    ///
    /// Steps (mirroring the one-shot random path so the committed sector equals a
    /// one-shot sector for the same `(seed, size, nonces)`):
    ///
    /// 1. Run the **full** pipeline synchronously through `Stage::LAST` via
    ///    [`sectorforge::generate_prefix`] with the session nonces — nonce-0 is
    ///    byte-identical to `generate_with_progress_and_cancel` (§2.2 / invariant
    ///    #2). All entropy flows through the engine's stage RNG; this call never
    ///    touches `rand::thread_rng()` (invariant #1). We run a fresh full sector
    ///    rather than trusting `session.preview`, which may be an early-step
    ///    partial. The transient regions-override (§5) is folded into the run's
    ///    `ProjectInput` here too (via [`Self::synthesize_project_input_with`]) so
    ///    the committed sector matches the preview; `None` keeps the verbatim
    ///    catalog (byte-identical).
    /// 2. Write the project tree to `dest` — config + every catalog (with the
    ///    regions-override, if any, folded into the regions catalog so the
    ///    persisted file matches the preview) + the full-sector
    ///    `out/sector.json` — via the builder's own
    ///    [`crate::builder::project_io::save_project_as`] (the in-memory project,
    ///    just like the random worker scaffolds a preset tree + writes
    ///    `sector.json`). Then [`crate::builder::project_io::open_project`] reads
    ///    it all back fully wired and `*self = new_state`.
    /// 3. The reopened state has `iterative_gen == None`, so the wizard is cleared
    ///    by the session boundary itself.
    ///
    /// The five post-gen derivations (personae/sites/hooks/missions/prose) are
    /// **not** persisted here — the random path doesn't persist them either; the
    /// builder recomputes them lazily when the relevant tab is first opened.
    ///
    /// On success returns `Ok(())`. Returns `Err(message)` (leaving the session
    /// intact so the user can retry) when there is no active session, no chosen
    /// `dest`, no worlds catalog (no [`sectorforge::ProjectInput`] can be built),
    /// or generation / disk I/O fails.
    ///
    /// # Errors
    ///
    /// Surfaces a human-readable message for: missing session, missing
    /// destination, missing worlds catalog, a [`sectorforge::SectorError`] from
    /// the full run, a write error from `save_project_as`, or a reopen error from
    /// `open_project`.
    pub fn commit_new_project(&mut self) -> Result<(), String> {
        let Some(session) = self.iterative_gen.as_ref() else {
            return Err("No iterative session is open.".to_string());
        };
        let Some(dest) = session.dest.clone() else {
            return Err("Choose a destination folder before committing.".to_string());
        };

        // Pre-flight sanity gate (ITERATIVE_GENERATION.md Phase S guard): never start a
        // full, *uncancellable* run (this path passes `|| false`) for an oversized /
        // too-dense / structurally invalid config — it would hang or OOM. Reject up
        // front, leaving the session (and chosen dest) intact for a fixed retry, and
        // before any disk write so nothing is half-materialised.
        precheck_generatable(&session.config)?;

        // We are about to run the full pipeline ourselves; detach any in-flight
        // preview job so its (now superseded) result is dropped on the next pump.
        self.cancel_prefix();

        // Build the read-only ProjectInput from the *session* config + the live
        // data catalogs, reusing the single catalog-assembly path. Mirror
        // `spawn_prefix`: temporarily swap the session knobs into `self.config`,
        // snapshot the input, then restore so we don't leave the wizard's config
        // installed if the run fails.
        // Audit finding #28: one destructuring borrow instead of four repeated
        // `self.iterative_gen.as_ref().expect(...)` chains.
        let session = self.iterative_gen.as_ref().expect("session present above");
        let session_config = session.config.clone();
        let nonces = session.nonces.clone();
        // §5 / invariant #5: fold the transient regions-override into this run too,
        // so the committed sector matches the preview. `None` ⇒ verbatim catalog
        // (byte-identical to the legacy path).
        let regions_override = session.regions_override.clone();
        // Blank-builder launch: the `_base`-seeded catalog set the preview ran on
        // (see `spawn_prefix`). Fold it into the full run too, then persist it into
        // the new project below so the reopened sector has its world/faction pools.
        let catalogs_override = session.catalogs_override.clone();
        let saved_config = std::mem::replace(&mut self.config, session_config.clone());
        let saved_catalogs = catalogs_override
            .clone()
            .map(|c| std::mem::replace(&mut self.data_catalogs, c));
        let input = self.synthesize_project_input_with(regions_override.as_ref());
        if let Some(saved) = saved_catalogs {
            self.data_catalogs = saved;
        }
        self.config = saved_config;

        let Some(input) = input else {
            return Err(
                "No worlds catalog is loaded — the wizard cannot build a sector to commit."
                    .to_string(),
            );
        };

        // 1. Full, synchronous run through the last stage. Nonce-0 ⇒ byte-identical
        //    to the one-shot path; re-rolls apply via the per-stage nonce suffix.
        let sector = generate_prefix(input, Stage::LAST, &nonces, |_| {}, || false)
            .map_err(|e| format!("Generating the full sector failed: {e}"))?;

        // 2. Materialise the project tree at `dest` — **failure-atomically** (§P7).
        //    The old path wrote the committed config / sector / catalogs straight
        //    into `self` *before* the fallible `save_project_as` + `open_project`,
        //    so a save or reopen error left `self` half-applied (mutated document,
        //    no valid project on disk, wizard session still live — corrupt, stuck
        //    state). Instead we build the to-be-written project in a **scratch**
        //    `BuilderState` and only let `*self = open_project(...)` (step 3)
        //    replace the live document once BOTH the save and the reopen have
        //    succeeded. `self`'s own config / sector / data_catalogs are never
        //    touched until that final swap, so any `Err` below returns with the
        //    wizard session — and `self` — fully intact for a retry.
        //
        //    This is byte-identical to the old success path: `save_project_as`
        //    reads only `config` / `data_catalogs` / `sector` from the state it is
        //    handed (everything else it touches — manifest digests, project_path,
        //    dirty flags, mtime/watcher — it *writes*), so a scratch carrying those
        //    three identical values writes identical files. The scratch (and the
        //    throwaway watcher `save_project_as` spawns on it) is then dropped, and
        //    the final state comes wholly from `open_project` (invariant #5: this
        //    is the commit session boundary, so no `BuilderCommand` is involved).
        let mut scratch = BuilderState::new_blank(
            sector.id.as_ref(),
            sector.title.as_ref(),
            sector.seed.as_ref(),
            sector.width,
            sector.height,
        );
        scratch.config = session_config;
        // Base the scratch catalogs on the **live** builder catalogs so that, with
        // no `catalogs_override`, the persisted catalogs are verbatim what the old
        // path wrote (it saved `self.data_catalogs`); `new_blank`'s empty default
        // would otherwise drop them.
        scratch.data_catalogs = self.data_catalogs.clone();
        scratch.sector = sector.into();
        // Blank-builder launch: persist the wizard's `_base`-seeded catalogs into
        // the new project so worlds.toml / factions.toml / names / … are written
        // (`save_project_as` only writes a catalog that is present in
        // `data_catalogs`), and so the reopened roster is not empty. The session
        // config installed above already carries the matching `[inputs]` paths.
        if let Some(catalogs) = catalogs_override {
            scratch.data_catalogs = catalogs;
        }
        // §5: persist the transient regions-override into the new project's
        // regions catalog so the reopened sector matches the preview. This is the
        // *only* point the override touches catalog state, and it does so on the
        // scratch project that `open_project` reads back, never during preview.
        // `save_project_as` only writes `regions.toml` when `inputs.regions` is
        // set, so ensure the standard path is present.
        if let Some(regions) = regions_override {
            scratch.data_catalogs.regions = Some(regions);
            if scratch.config.inputs.regions.is_none() {
                scratch.config.inputs.regions = Some("data/regions/regions.toml".to_string());
            }
        }
        crate::builder::project_io::save_project_as(&mut scratch, &dest)
            .map_err(|e| format!("Writing the new project to {dest} failed: {e}"))?;
        // The disk tree is fully written; the scratch state has served its purpose.
        drop(scratch);

        // 3. Reopen the freshly written project — the session boundary, and the
        //    sole point `self` is mutated. The reopened state carries
        //    `iterative_gen == None`, so the wizard is cleared by the replacement
        //    itself; no `BuilderCommand` is involved. If this fails, `self` is
        //    still untouched (the scratch is already dropped).
        let new_state = crate::builder::project_io::open_project(&dest)
            .map_err(|e| format!("Reopening the committed project at {dest} failed: {e}"))?;
        *self = new_state;
        Ok(())
    }

    /// ITERATIVE_GENERATION.md §S3 — re-roll `step`. Bumps `step`'s **own** stage
    /// nonce (so only that stage and downstream change — invariants #2 / §2.3 DAG
    /// rule) then re-runs the prefix through the **current** step's preview
    /// cutoff. `GenStep::Size` owns no stage, so re-rolling it instead mints a
    /// fresh `root_seed` (the only legitimate fresh-entropy point) and rolls the
    /// generation knobs from it, leaving every nonce at 0.
    ///
    /// No-op without an active session.
    pub fn reroll_step(&mut self, ctx: &egui::Context, step: GenStep) {
        let Some(session) = self.iterative_gen.as_mut() else {
            return;
        };
        match step.stage() {
            Some(stage) => {
                session.nonces.bump(stage);
            }
            None => {
                // Size step: re-mint the root seed and re-roll the knobs. The
                // mint is the sole non-deterministic step (seed only); every
                // downstream draw still flows through stage_rng.
                let seed = sectorforge::random_sector::mint_seed();
                let size = SectorSize::Custom {
                    dim: session.config.generation.sector_width,
                };
                let rolled = build_random_config(&seed, size);
                session.config.generation = rolled.generation;
                session.config.generation.seed = seed.clone();
                session.root_seed = seed;
                session.nonces = RerollNonces::default();
            }
        }
        // DAG: a re-roll of step K invalidates K and everything after it.
        self.invalidate_from(step);
        self.rerun_preview(ctx);
    }

    /// ITERATIVE_GENERATION.md §S3 — apply a config edit made on `step`. The
    /// caller has already mutated `session.config` in place (the panel writes
    /// knobs directly — transient state). This records the downstream
    /// invalidation (clears `accepted_through` back to `step`, §2.3) and re-runs
    /// the prefix through the current step. No-op without an active session.
    ///
    /// Use this after **any** knob write so the preview reflects every downstream
    /// re-derivation.
    pub fn note_config_edit(&mut self, ctx: &egui::Context, step: GenStep) {
        if self.iterative_gen.is_none() {
            return;
        }
        self.invalidate_from(step);
        self.rerun_preview(ctx);
    }

    /// Invariant #4 — set the (square) grid dimension from a single field. Writes
    /// **both** `sector_width` and `sector_height` to `dim`, so they can never
    /// diverge (`GEN_SECTOR_NOT_SQUARE` never trips). The panel's size field must
    /// route through here rather than touching either width or height directly.
    /// Marks the `Size` step edited and re-runs the preview. No-op without a
    /// session.
    ///
    /// `dim` is clamped to `1..=MAX_CUSTOM_DIM` so the wizard cannot exceed the
    /// largest grid the generator is expected to handle (the same ergonomic cap
    /// the one-shot random generator + CLI enforce); [`precheck_generatable`]
    /// is the defence-in-depth gate for configs that arrive out of band.
    pub fn set_grid_dim(&mut self, ctx: &egui::Context, dim: u32) {
        let Some(session) = self.iterative_gen.as_mut() else {
            return;
        };
        let dim = dim.clamp(1, MAX_CUSTOM_DIM);
        session.config.generation.sector_width = dim;
        session.config.generation.sector_height = dim;
        self.note_config_edit(ctx, GenStep::Size);
    }

    /// Advance the wizard to the next step (Next button): records `current_step`
    /// as accepted, moves forward, and re-runs the prefix through the new step.
    /// No-op at [`GenStep::Finalize`] or without a session.
    pub fn step_next(&mut self, ctx: &egui::Context) {
        let Some(session) = self.iterative_gen.as_mut() else {
            return;
        };
        let Some(next) = session.current_step.next() else {
            return;
        };
        // Mark the step we are leaving as accepted-through (monotone forward).
        let leaving = session.current_step;
        if session.accepted_through.is_none_or(|a| a < leaving) {
            session.accepted_through = Some(leaving);
        }
        session.current_step = next;
        self.rerun_preview(ctx);
    }

    /// Step the wizard back (Back button): moves to the previous step and
    /// re-runs the prefix through it. Leaves `accepted_through` alone (stepping
    /// back does not un-accept; a knob edit is what invalidates downstream).
    /// No-op at [`GenStep::Size`] or without a session.
    pub fn step_back(&mut self, ctx: &egui::Context) {
        let Some(session) = self.iterative_gen.as_mut() else {
            return;
        };
        let Some(prev) = session.current_step.prev() else {
            return;
        };
        session.current_step = prev;
        self.rerun_preview(ctx);
    }

    /// DAG invalidation (ITERATIVE_GENERATION.md §2.3): clear `accepted_through`
    /// back to *before* `step`, because `step` and everything after it is now
    /// stale. Internal helper for the re-roll / edit ops.
    fn invalidate_from(&mut self, step: GenStep) {
        let Some(session) = self.iterative_gen.as_mut() else {
            return;
        };
        match step.prev() {
            // Steps strictly before `step` remain accepted.
            Some(prev) => {
                if session.accepted_through.is_some_and(|a| a >= step) {
                    session.accepted_through = Some(prev);
                }
            }
            // Editing the first step invalidates everything.
            None => session.accepted_through = None,
        }
    }

    /// Re-run the prefix through the **current** step's preview cutoff (the §2.3
    /// "always re-run through the current step" rule). For `GenStep::Size` there
    /// is no engine cutoff — clear any stale preview so the panel shows the empty
    /// grid. Internal helper shared by every op above.
    fn rerun_preview(&mut self, ctx: &egui::Context) {
        let Some(session) = self.iterative_gen.as_ref() else {
            return;
        };
        match session.current_step.preview_through() {
            Some(through) => self.spawn_prefix(ctx, through),
            None => {
                // Size step: no run; drop any prior systems-bearing preview.
                if let Some(session) = self.iterative_gen.as_mut() {
                    session.preview = None;
                    if let Some(job) = session.job.take() {
                        job.cancel();
                    }
                    session.revision = session.revision.wrapping_add(1);
                    *session.progress.lock().unwrap() = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod progress_bar_tests {
    use super::*;

    #[test]
    fn item_frac_edges() {
        assert_eq!(item_frac(0, 0), 1.0); // an empty stage reads as already full
        assert_eq!(item_frac(0, 4), 0.0);
        assert_eq!(item_frac(2, 4), 0.5);
        assert_eq!(item_frac(4, 4), 1.0);
        assert_eq!(item_frac(9, 4), 1.0); // clamped, never over-fills
    }

    /// The early steps (Placement / Regions) preview through `Stage::Systems`, so
    /// the bar must climb smoothly across WorldPool → Placement → Regions →
    /// per-system build and stay inside `[0.0, 1.0)`.
    #[test]
    fn fraction_is_monotonic_and_bounded_for_systems_cutoff() {
        let through = Stage::Systems;
        let stream = [
            SectorProgress::WorldPoolBuilt {
                rows: 100,
                candidates: 80,
                excluded: 20,
            },
            SectorProgress::SystemsPlaced {
                total: 40,
                width: 8,
                height: 8,
            },
            SectorProgress::RegionsBuilt { count: 3 },
            SectorProgress::SystemBuilt {
                current: 1,
                total: 40,
                worlds: 2,
            },
            SectorProgress::SystemBuilt {
                current: 20,
                total: 40,
                worlds: 1,
            },
            SectorProgress::SystemBuilt {
                current: 40,
                total: 40,
                worlds: 3,
            },
        ];
        let mut last = 0.0_f32;
        for ev in &stream {
            let f = preview_progress_fraction(ev, through).expect("anchored milestone");
            assert!(
                (0.0..1.0).contains(&f),
                "fraction {f} out of [0,1) for {ev:?}"
            );
            assert!(f >= last, "bar went backwards: {last} -> {f} at {ev:?}");
            last = f;
        }
        // WorldPool is the first of four stages (WorldPool..=Systems) ⇒ exactly 1/4.
        let first = preview_progress_fraction(&stream[0], through).unwrap();
        assert!(
            (first - 0.25).abs() < 1e-6,
            "WorldPool should fill 1/4, got {first}"
        );
        // The final per-system event all but completes the bar.
        assert!(last > 0.9, "last fraction should be near full, got {last}");
    }

    /// The same milestone fills *less* of the bar when a longer prefix will run —
    /// the fill is relative to the cutoff, not absolute.
    #[test]
    fn fraction_is_relative_to_cutoff() {
        let ev = SectorProgress::SystemsPlaced {
            total: 10,
            width: 4,
            height: 4,
        };
        let early = preview_progress_fraction(&ev, Stage::Systems).unwrap();
        let full = preview_progress_fraction(&ev, Stage::Chronicle).unwrap();
        assert!(
            full < early,
            "longer run ⇒ smaller slice: expected {full} < {early}"
        );
    }

    /// Stage-boundary / timing events carry no completion signal, so they leave
    /// the bar where it is (`None`) rather than snapping it somewhere wrong.
    #[test]
    fn unanchored_events_return_none() {
        let through = Stage::Chronicle;
        assert!(
            preview_progress_fraction(&SectorProgress::StageStarted { name: "x" }, through)
                .is_none()
        );
        assert!(preview_progress_fraction(
            &SectorProgress::StageElapsed {
                stage: "x",
                millis: 3
            },
            through
        )
        .is_none());
    }
}

/// Safety net for the iterative-gen DAG step-navigation logic
/// (`step_next` / `step_back` / `note_config_edit` / `invalidate_from` /
/// `reroll_step`). These lock in the current `current_step` / `accepted_through`
/// / `nonces` / `root_seed` transitions before a separate change edits this
/// module.
///
/// # Headless note
///
/// The ctx-taking ops funnel through `rerun_preview`, which (for any non-`Size`
/// step) calls `spawn_prefix`. In these tests the fixture's `data_catalogs` has
/// **no worlds catalog** (`BuilderState::new_blank` ⇒ `DataCatalogs::new()`), so
/// `synthesize_project_input_with` returns `None` and `spawn_prefix` records a
/// `feedback.last_command_error` and returns **without spawning any background
/// job** (verified against `spawn_prefix` / `synthesize_project_input_with`). No
/// generation runs, so the assertions below target only the *synchronous* state
/// transitions that are already applied by the time each method returns. The
/// pure `invalidate_from` carries the bulk of the DAG-semantics coverage.
#[cfg(test)]
mod step_nav_tests {
    use super::*;
    use sectorforge::random_sector::SectorSize;

    /// Square fixture (invariant #4): `Custom { dim }` is `dim × dim` by
    /// construction. A blank builder has no worlds catalog, so every prefix run
    /// no-ops synchronously (see the module note). `#[cfg(test)]` fixtures may
    /// install the session directly — this is transient session state, not
    /// document state, and the bus carve-out plus the test carve-out both apply.
    fn session_state() -> BuilderState {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let session = IterativeGenSession::new(
            state.config.clone(),
            "seed".to_string(),
            SectorSize::Custom { dim: 8 },
        );
        state.iterative_gen = Some(session);
        state
    }

    /// Borrow the live session (panics if absent — the fixture always installs
    /// one).
    fn sess(state: &BuilderState) -> &IterativeGenSession {
        state.iterative_gen.as_ref().expect("session installed")
    }

    /// A fresh `egui::Context` is enough to drive the ctx-taking ops; the prefix
    /// run no-ops on the catalog-less fixture, so nothing is actually painted.
    fn ctx() -> egui::Context {
        egui::Context::default()
    }

    // --- invalidate_from (pure: no ctx, no preview) -------------------------

    /// `invalidate_from(Size)` always clears `accepted_through` to `None` — Size
    /// is the only step whose `prev()` is `None`, so editing it invalidates
    /// everything regardless of the prior value.
    #[test]
    fn invalidate_from_size_clears_to_none() {
        for prior in [
            None,
            Some(GenStep::Size),
            Some(GenStep::Systems),
            Some(GenStep::Finalize),
        ] {
            let mut state = session_state();
            state.iterative_gen.as_mut().unwrap().accepted_through = prior;
            state.invalidate_from(GenStep::Size);
            assert_eq!(
                sess(&state).accepted_through,
                None,
                "invalidate_from(Size) must clear to None (prior = {prior:?})"
            );
        }
    }

    /// For a non-first step, `invalidate_from(step)` clears `accepted_through`
    /// back to `step.prev()` **only when** the current value is `>= step`.
    #[test]
    fn invalidate_from_clears_back_to_prev_when_accepted_at_or_past_step() {
        // accepted_through == step ⇒ retreat to step.prev().
        let mut state = session_state();
        state.iterative_gen.as_mut().unwrap().accepted_through = Some(GenStep::Systems);
        state.invalidate_from(GenStep::Systems);
        assert_eq!(sess(&state).accepted_through, GenStep::Systems.prev());
        assert_eq!(sess(&state).accepted_through, Some(GenStep::Regions));

        // accepted_through strictly past step ⇒ still retreat to step.prev().
        let mut state = session_state();
        state.iterative_gen.as_mut().unwrap().accepted_through = Some(GenStep::Finalize);
        state.invalidate_from(GenStep::Systems);
        assert_eq!(sess(&state).accepted_through, Some(GenStep::Regions));
    }

    /// `invalidate_from(step)` leaves `accepted_through` untouched when it is
    /// strictly *before* `step` (those earlier steps are still accepted) or
    /// `None`.
    #[test]
    fn invalidate_from_is_noop_when_accepted_before_step_or_none() {
        // accepted_through < step ⇒ untouched.
        let mut state = session_state();
        state.iterative_gen.as_mut().unwrap().accepted_through = Some(GenStep::Placement);
        state.invalidate_from(GenStep::Routes);
        assert_eq!(
            sess(&state).accepted_through,
            Some(GenStep::Placement),
            "earlier accepted step must survive a later invalidation"
        );

        // accepted_through == None ⇒ untouched (stays None).
        let mut state = session_state();
        state.iterative_gen.as_mut().unwrap().accepted_through = None;
        state.invalidate_from(GenStep::Routes);
        assert_eq!(sess(&state).accepted_through, None);
    }

    /// Sweep every `step` against an `accepted_through` parked at the last step
    /// (`Finalize`): each non-`Size` step retreats it to `step.prev()`, and
    /// `Size` clears it to `None`. Locks the full DAG-retreat table in one go.
    #[test]
    fn invalidate_from_sweeps_full_dag_from_finalize() {
        for &step in &GenStep::ALL {
            let mut state = session_state();
            state.iterative_gen.as_mut().unwrap().accepted_through = Some(GenStep::Finalize);
            state.invalidate_from(step);
            let expected = step.prev(); // None only for Size
            assert_eq!(
                sess(&state).accepted_through,
                expected,
                "invalidate_from({step:?}) from Finalize should land on {expected:?}"
            );
        }
    }

    // --- step_next ----------------------------------------------------------

    /// `step_next` advances `current_step` by one and records the *leaving* step
    /// as `accepted_through` (monotone forward) when it was `None`.
    #[test]
    fn step_next_advances_and_accepts_leaving_step() {
        let mut state = session_state();
        assert_eq!(sess(&state).current_step, GenStep::Size);
        assert_eq!(sess(&state).accepted_through, None);

        state.step_next(&ctx());
        assert_eq!(sess(&state).current_step, GenStep::Placement);
        assert_eq!(sess(&state).accepted_through, Some(GenStep::Size));

        state.step_next(&ctx());
        assert_eq!(sess(&state).current_step, GenStep::Regions);
        assert_eq!(sess(&state).accepted_through, Some(GenStep::Placement));
    }

    /// Walking `step_next` from `Size` to `Finalize` ends with `current_step ==
    /// Finalize` and `accepted_through == Routes` (the last step *left*); a
    /// further `step_next` at `Finalize` is a no-op.
    #[test]
    fn step_next_is_noop_at_finalize() {
        let mut state = session_state();
        for _ in 0..GenStep::ALL.len() {
            state.step_next(&ctx());
        }
        assert_eq!(sess(&state).current_step, GenStep::Finalize);
        assert_eq!(sess(&state).accepted_through, Some(GenStep::Routes));

        // One more Next at the last step changes nothing.
        state.step_next(&ctx());
        assert_eq!(sess(&state).current_step, GenStep::Finalize);
        assert_eq!(sess(&state).accepted_through, Some(GenStep::Routes));
    }

    /// `accepted_through` only ratchets forward: when it is already past the
    /// step being left, `step_next` does **not** pull it back.
    #[test]
    fn step_next_keeps_accepted_through_monotone() {
        let mut state = session_state();
        // Pretend the user has already accepted through Finalize, then sits back
        // on Placement (current_step) — stepping forward must not lower
        // accepted_through to the (earlier) leaving step.
        {
            let s = state.iterative_gen.as_mut().unwrap();
            s.current_step = GenStep::Placement;
            s.accepted_through = Some(GenStep::Finalize);
        }
        state.step_next(&ctx());
        assert_eq!(sess(&state).current_step, GenStep::Regions);
        assert_eq!(
            sess(&state).accepted_through,
            Some(GenStep::Finalize),
            "accepted_through must not regress when stepping forward"
        );
    }

    // --- step_back ----------------------------------------------------------

    /// `step_back` retreats `current_step` by one and leaves `accepted_through`
    /// untouched (stepping back does not un-accept).
    #[test]
    fn step_back_retreats_and_preserves_accepted_through() {
        let mut state = session_state();
        // Advance to Regions, accepting Size then Placement along the way.
        state.step_next(&ctx());
        state.step_next(&ctx());
        assert_eq!(sess(&state).current_step, GenStep::Regions);
        assert_eq!(sess(&state).accepted_through, Some(GenStep::Placement));

        state.step_back(&ctx());
        assert_eq!(sess(&state).current_step, GenStep::Placement);
        assert_eq!(
            sess(&state).accepted_through,
            Some(GenStep::Placement),
            "step_back must leave accepted_through alone"
        );

        state.step_back(&ctx());
        assert_eq!(sess(&state).current_step, GenStep::Size);
        assert_eq!(sess(&state).accepted_through, Some(GenStep::Placement));
    }

    /// `step_back` at `Size` is a no-op (there is no earlier step).
    #[test]
    fn step_back_is_noop_at_size() {
        let mut state = session_state();
        assert_eq!(sess(&state).current_step, GenStep::Size);
        state.step_back(&ctx());
        assert_eq!(sess(&state).current_step, GenStep::Size);
        assert_eq!(sess(&state).accepted_through, None);
    }

    // --- note_config_edit (delegates to invalidate_from) --------------------

    /// `note_config_edit(step)` delegates to `invalidate_from(step)`: editing a
    /// knob on a step retreats `accepted_through` to `step.prev()` when it was at
    /// or past that step, and `current_step` is untouched.
    #[test]
    fn note_config_edit_delegates_to_invalidate_from() {
        let mut state = session_state();
        {
            let s = state.iterative_gen.as_mut().unwrap();
            s.current_step = GenStep::Factions;
            s.accepted_through = Some(GenStep::Finalize);
        }
        state.note_config_edit(&ctx(), GenStep::Systems);
        assert_eq!(
            sess(&state).accepted_through,
            Some(GenStep::Regions),
            "editing Systems must retreat accepted_through to Regions"
        );
        assert_eq!(
            sess(&state).current_step,
            GenStep::Factions,
            "a config edit must not move current_step"
        );
    }

    /// `note_config_edit(Size)` clears `accepted_through` to `None` (the
    /// first-step special case, via `invalidate_from`).
    #[test]
    fn note_config_edit_on_size_clears_accepted_through() {
        let mut state = session_state();
        state.iterative_gen.as_mut().unwrap().accepted_through = Some(GenStep::Routes);
        state.note_config_edit(&ctx(), GenStep::Size);
        assert_eq!(sess(&state).accepted_through, None);
    }

    // --- reroll_step --------------------------------------------------------

    /// Re-rolling a stage-owning step bumps **only that step's** stage nonce,
    /// retreats `accepted_through` via `invalidate_from`, and leaves `root_seed`
    /// alone.
    #[test]
    fn reroll_step_bumps_only_that_stage_nonce() {
        let mut state = session_state();
        state.iterative_gen.as_mut().unwrap().accepted_through = Some(GenStep::Finalize);
        let seed_before = sess(&state).root_seed.clone();

        // GenStep::Routes owns Stage::RouteControls (not Stage::Routes).
        state.reroll_step(&ctx(), GenStep::Routes);

        let s = sess(&state);
        assert_eq!(
            s.nonces.get(Stage::RouteControls),
            1,
            "re-rolling Routes bumps its owned stage nonce once"
        );
        // No other stage nonce moved.
        assert_eq!(s.nonces.get(Stage::Placement), 0);
        assert_eq!(s.nonces.get(Stage::Systems), 0);
        assert_eq!(s.nonces.get(Stage::Factions), 0);
        // DAG retreat: Routes.prev() == Factions (ALL order is
        // Size, Placement, Regions, Systems, Factions, Routes, Finalize).
        assert_eq!(s.accepted_through, Some(GenStep::Factions));
        // A stage re-roll never re-mints the root seed.
        assert_eq!(s.root_seed, seed_before);
    }

    /// Re-rolling the same step twice bumps its nonce to 2 (monotone re-roll
    /// counter).
    #[test]
    fn reroll_step_twice_bumps_nonce_to_two() {
        let mut state = session_state();
        state.reroll_step(&ctx(), GenStep::Placement);
        state.reroll_step(&ctx(), GenStep::Placement);
        assert_eq!(sess(&state).nonces.get(Stage::Placement), 2);
    }

    /// Re-rolling `Size` re-mints `root_seed` (fresh entropy), mirrors it into
    /// `config.generation.seed`, resets **all** nonces to zero, and clears
    /// `accepted_through` to `None`. Size owns no stage, so no stage nonce is
    /// bumped.
    #[test]
    fn reroll_size_remints_seed_and_resets_nonces() {
        let mut state = session_state();
        // Dirty the state first: bump a nonce and accept through the end.
        {
            let s = state.iterative_gen.as_mut().unwrap();
            s.nonces.bump(Stage::Systems);
            s.accepted_through = Some(GenStep::Finalize);
        }
        let seed_before = sess(&state).root_seed.clone();
        assert_eq!(seed_before, "seed");

        state.reroll_step(&ctx(), GenStep::Size);

        let s = sess(&state);
        // mint_seed is the sole non-deterministic step — assert it *changed* and
        // is internally consistent rather than pinning a value.
        assert_ne!(
            s.root_seed, seed_before,
            "Size re-roll must re-mint the seed"
        );
        assert_eq!(
            s.config.generation.seed, s.root_seed,
            "rolled config seed must mirror the new root_seed"
        );
        // Square geometry preserved across the re-roll of the knobs (invariant #4).
        assert_eq!(s.config.generation.sector_width, 8);
        assert_eq!(s.config.generation.sector_height, 8);
        // All nonces reset to zero, including the one bumped above.
        assert_eq!(s.nonces.get(Stage::Systems), 0);
        assert_eq!(s.nonces.get(Stage::Placement), 0);
        // Size invalidates everything downstream.
        assert_eq!(s.accepted_through, None);
    }

    // --- no-session no-ops --------------------------------------------------

    /// Every ctx-taking step-nav op is a silent no-op when no session is
    /// installed (the `let Some(session) = ... else { return }` guard).
    #[test]
    fn ops_are_noops_without_a_session() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        assert!(state.iterative_gen.is_none());
        // None of these should panic or install a session.
        state.step_next(&ctx());
        state.step_back(&ctx());
        state.note_config_edit(&ctx(), GenStep::Systems);
        state.reroll_step(&ctx(), GenStep::Placement);
        state.invalidate_from(GenStep::Systems);
        assert!(state.iterative_gen.is_none());
    }
}
