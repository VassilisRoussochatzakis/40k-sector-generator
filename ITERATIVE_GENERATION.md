# ITERATIVE_GENERATION.md

> **What this file is.** A self-contained implementation brief for adding an **iterative
> (stage-by-stage) random-generation mode** to the `sectorforge-builder`. Today the builder
> generates a random sector in one shot; this feature lets a user walk the generation pipeline
> one step at a time — **size → system placement → regions → system contents → factions → routes
> → finalize** — reviewing, tuning the knobs for, and re-rolling each step before committing the
> whole sector.
>
> **Audience.** A future Claude (or a human) implementing the feature. It is written to be executed
> the way [CLAUDE.md](CLAUDE.md) demands: **delegate to subagents, decompose by independence, copy
> the invariants into every brief, and verify the loop is closed.** §7 gives the exact dispatch plan.
>
> **How to read it.** §1–§3 are the model you must hold in your head (the pipeline, the design, the
> non-negotiable invariants). §4 is the phased build. §5–§6 are reference tables. §7 is the subagent
> plan. §8 is the acceptance checklist. The Appendix is every `path:line` anchor in one place.
>
> Every `path:line` below was captured from the live tree and cross-checked by an independent
> verifier. Line numbers drift as the code moves — **treat them as starting anchors, not contracts;
> re-locate the symbol if a number is stale.**

---

## 0. TL;DR

Introduce one engine seam and one builder panel.

1. **Engine seam (`src/`):** a `Stage` enum over the existing 18-stage pipeline, and a
   `generate_prefix(project, through: Stage, …, nonces)` entry point that runs the pipeline
   **up to and including** a chosen stage and returns the partial `GeneratedSector`. The current
   one-shot orchestrator becomes a thin wrapper that calls `generate_prefix(.., Stage::Chronicle, ..)`.
   A `RerollNonces` map lets a single stage be re-rolled deterministically without disturbing the others.
2. **Builder panel (`builder/`):** a new `BuilderTab::IterativeGen` + `panels/iterative_gen.rs` that
   holds a transient `IterativeGenSession`, renders the current step's knobs and a live `SectorView`
   preview (built by re-running the prefix), and offers **Re-roll / Back / Next / Commit**.
3. **Commit:** for a *new* project, commit via the existing session-boundary path (`open_project`,
   identical to today's one-shot). For *regenerating inside an open project*, commit through a new
   undoable `BuilderCommand`.

The whole design rests on one property (proved in §2.2): **re-running the pipeline prefix with the
same config + seed reproduces the full run's prefix byte-for-byte**, because every stage seeds a
*fresh* RNG from `blake3("sectorforge:{seed}:{stage}:{disc}")` and stages share no stream. That is
what keeps the golden tests green and makes "step straight through == one-shot" a testable invariant.

---

## 1. The canonical generation pipeline (ground truth)

The orchestrator is **`generate_with_progress_and_cancel`** at `src/gen/generation/mod.rs:247`:

```rust
fn generate_with_progress_and_cancel<F, C>(
    project: ProjectInput,
    progress: F,        // FnMut(SectorProgress) — stage-boundary + per-entity events
    should_cancel: C,   // FnMut() -> bool — cooperative cancellation
) -> Result<GeneratedSector, SectorError>
```

It threads a mutable `GeneratedSector` (`src/model/sector_model/mod.rs:44`) plus local scratch
(`WorldCandidatePool`, the `Vec<HexCoord>` placements, the anomaly-hex set) through **18 stages**.
The table below is the **verified** order, impl site, and RNG stage-key for each stage. Two facts
were corrected against a naive first read and are flagged ⚠️ — they matter for where re-roll nonces go.

| # | Stage (engine) | Impl fn @ `path:line` | RNG key `(stage, discriminator)` | Notes |
|---|---|---|---|---|
| 1 | `WorldPool` | `world_pool::build_pool` @ `mod.rs:309` | — none | Filters world rows → `WorldCandidatePool`. |
| 2 | `Placement` | `placement::place_systems` @ `placement.rs:10` (rng @ `:37`) | `("placement","sector")` | Fisher-Yates shuffle of hex cells + min-distance relax → sorted `Vec<HexCoord>`. |
| 3 | `Regions` | `regions::build_regions` @ `mod.rs:339` → `regions.rs:276` (rng @ `:288`) | `("regions","sector")` | ⚠️ **Draws RNG** (shuffles grid, weight-picks conditions). Runs **before** worlds so anomaly hexes bias world candidates. |
| 4 | `Systems` (loop) | `systems::build_system_with_bias` @ `mod.rs:360` → `systems.rs:42` (rng @ `:52`) | `("system", <sys_id>)` | Per system: star colour, name, then ↓. |
| 4a | └ `Worlds` (sub) | `world_placement::generate_worlds_for_system` @ `world_placement.rs:36` (rng @ `:63`) | `("world", <world_id>)` | World count, candidate pick (star-colour + anomaly bias), features, names. Sorted by orbit. |
| 5 | `Factions` | `factions::assign_factions` @ `mod.rs:383` → `factions.rs:19` (rng @ `mod.rs:382`) | `("factions","sector")` | Assigns factions to worlds; derives claims/control/stability; aggregates summaries. |
| 6 | `Routes` (public) | `routes::generate_routes` @ `mod.rs:402` → `routes.rs:26` (rng @ `mod.rs:401`) | `("routes","sector")` ⚠️ **reserved/unused** | rng is constructed and passed in but `let _ = rng;` (`routes.rs:160`). Selection is **deterministic** (descending-weight sort + density top-k). No output draw today. |
| 7 | `RegionRouteEffects` | `regions::apply_route_effects_with_progress` @ `mod.rs:418` → `regions.rs:568` | — none | Storm → perilous, calm → better. Idempotent. |
| 8 | `HiddenRoutes` | `hidden_routes::append_hidden_routes_with_regions_and_progress` @ `mod.rs:484` → `hidden_routes.rs:274` | — none | Webway / black-ship / smuggling layers per faction presence. |
| 9 | `StabilityRebalance` | `routes::rebalance_public_stability` @ `mod.rs:538` → `routes.rs:407` | — none | Buckets route stabilities to hit `stability_targets`. |
| 10 | `RouteControls` | `route_control::derive_route_controls` @ `mod.rs:553` → `analysis/route_control.rs:215` | — none | Per-route faction control. |
| 11 | `SystemState` (loop) | `surface_region::derive_regions` @ `surface_region.rs:81`; `conflict::derive_world/system_conflict` @ `analysis/conflict.rs:65/120`; `orbital_assets::derive_orbital_assets` @ `orbital_assets.rs:91`; `intel::derive_system_intel` @ `analysis/intel.rs:136` — loop @ `mod.rs:576` | — none | Pure per-system/world derived fields. |
| 12 | `Manifest` + init | `build_manifest` @ `mod.rs:608` (def `:811`); sort systems/routes/factions by id; init `GeneratedSector` @ `mod.rs:615` | — none | Settings digest + seed hash; sort for determinism. |
| 13 | `Archetypes` | `archetypes::apply_all` @ `mod.rs:641` → `archetypes.rs:178` | — none | Faction-kind rule engine (Imperial stack, Necron phase, Tyranid front…). |
| 14 | `PowerProjection` | `power_projection::project_sector` + `apply_to_factions` @ `mod.rs:649-650` → `analysis/power_projection.rs:45/165` | — none | Route-graph projection + decay. |
| 15 | `InfluenceField` | `influence_field::build_with_progress` @ `mod.rs:660` → `analysis/influence_field.rs:93` | — none | Voronoi-style territory bands. |
| 16 | `Relations` | `relations::derive_with_threshold` @ `mod.rs:716` → `analysis/relations/derive.rs:46` (rng @ `:245`) | `("relations","<faction_a_id>:<faction_b_id>")` | 25% perturbation per pair to break symmetric ties. |
| 17 | `Economy` | `economy::derive_with` (+ opt `apply_stability_nudge`) @ `mod.rs:732/735` → `analysis/economy/derive.rs:62/505` | — none | Per-world/system market snapshot. |
| 18 | `Chronicle` | `history::derive_with_progress` @ `mod.rs:747` → `analysis/history/mod.rs:58`; per-event rng in `history/build.rs:23` | `("history-event","<anchor_key>:<EventKind>:<ordinal>")` | Subsector/system/world/route/region events; date + era synthesis per event. |

**RNG sites that are *in* the pipeline (the complete set):** placement, regions, system, world,
factions, routes (reserved), relations, history-event. Every other `stage_rng` call in the tree
(`config`, `personae`, `missions`, `prose`, `sites`, `search`, `stitch`, `viewer_reroll`) belongs to
the random-config roller or to analysis/CLI paths that are **not** inside
`generate_with_progress_and_cancel`.

**Progress events:** `SectorProgress` (`src/gen/generation/mod.rs:29`) is emitted at every stage
boundary and per-entity loop (`SystemsPlaced`, `RegionsBuilt`, `SystemBuilt`, `FactionsAssigned`,
`RoutesGenerated`, `InfluenceField*`, `Chronicle*`, `StageElapsed`, …). The iterative panel reuses
these for its progress bar.

---

## 2. Design

### 2.1 Chosen architecture — "prefix re-run"

The session holds the evolving knobs (`AppConfig` / `GenerationConfig`), the `root_seed`, the data
catalogs, and a per-stage **re-roll nonce** map. To show the state *as of step K*, it calls a new
engine entry point that **runs the pipeline from stage 1 through the last engine stage of step K and
returns the partial `GeneratedSector`** (overlays past the cutoff left at their default/empty values).

Why this and not true in-place incremental generation:

- **Correctness & determinism for free.** Re-running the prefix with the same `(config, seed, nonces)`
  is byte-identical to the corresponding prefix of a full run (see §2.2). True in-place generation
  would require promoting the orchestrator's local scratch (`WorldCandidatePool`, placements,
  anomaly hexes) onto the session and proving each stage idempotent — far more surface area and a
  real risk to the golden suite.
- **Minimal seam.** One new parameter (`through: Stage`) and one new argument (`nonces`) on the
  orchestrator; everything else is the existing, tested pipeline.
- **Cost is bounded and tunable.** Small/medium sectors re-run in well under a frame budget off the
  UI thread (reuse the existing `RandomGenState` background-job machinery, `random_run.rs:72`). For
  large sectors, cache the `GeneratedSector` produced at each *accepted* step and re-run only the
  delta when stepping forward; re-rolling a stage invalidates that stage's cache and everything after it.

*(Optional future optimization, not required for v1: because stages 13–18 are pure functions of the
finished structural sector, an accepted checkpoint at `SystemState` lets the overlay tail be recomputed
without redoing structural work. v1 may re-run the full prefix each time for simplicity.)*

### 2.2 The determinism guarantee (the linchpin — state it in every test)

`src/model/rng.rs`:

```text
derive_stage_seed(root_seed, stage, disc) = blake3("sectorforge:{root_seed}:{stage}:{disc}")  // 32 bytes
stage_rng(root_seed, stage, disc)         = ChaCha8Rng::from_seed(derive_stage_seed(...))
```

Two consequences that the entire feature depends on:

1. **Stages share no RNG stream.** Each `stage_rng` call constructs a *fresh* `ChaCha8Rng` from its
   own keyed seed. There is no global RNG and no implicit consumption of a shared stream. Therefore
   the entropy a stage draws is a pure function of `(root_seed, stage, discriminator)` and is **independent
   of which other stages ran before it.**
2. **⇒ Prefix-run ≡ full-run prefix.** Running stages `1..=K` produces, for every stage `≤ K`, the
   exact same bytes as a full `1..=18` run, given the same `(config, seed, nonces)`. The cutoff cannot
   perturb earlier stages because nothing downstream feeds entropy upstream.

This is **why** stepping straight through with default knobs and **zero re-rolls must equal the
one-shot output** (§4 Phase T, the equivalence test) and why the golden suite stays byte-stable
(§3). Write it into the doc-comment of `generate_prefix` and into every relevant test.

### 2.3 User-facing steps vs. engine stages (mind the DAG)

The user thinks in ~7 steps; the engine has 18 stages. The mapping groups the knobless derivations
into an automatic tail and — critically — **respects the dependency that `Regions` (stage 3) runs
before world population (stage 4a) because anomaly hexes reweight the world candidate pool.** That
forces regions *before* "system contents" in the step order, even though a user might naively expect
to fill systems first. Present the steps in engine order; explain the dependency in the UI.

| Step (user) | Engine stages covered | `generate_prefix(through = …)` |
|---|---|---|
| 1. **Size & seed** | (config only) | *(no run; show empty grid)* |
| 2. **System placement** | `Placement` | `Stage::Placement` |
| 3. **Warp regions** | `Regions` | `Stage::Regions` |
| 4. **System contents** | `Systems` + `Worlds` | `Stage::Systems` |
| 5. **Factions** | `Factions` | `Stage::Factions` |
| 6. **Routes** | `Routes` → `RegionRouteEffects` → `HiddenRoutes` → `StabilityRebalance` → `RouteControls` | `Stage::RouteControls` |
| 7. **Finalize & overlays** | `SystemState` → `Manifest` → `Archetypes` → `PowerProjection` → `InfluenceField` → `Relations` → `Economy` → `Chronicle` | `Stage::Chronicle` |

**DAG rule for re-rolls / edits:** changing a step's knobs invalidates that step **and every later
step** (they are downstream). Re-rolling step 3 (Regions) must therefore also re-run steps 4–7,
because worlds depend on anomalies, factions depend on worlds, routes depend on systems, overlays
depend on everything. Implement this as "re-run the prefix from the edited stage's index forward" —
the prefix-run architecture gives it for free (just bump the nonce and re-run through the current step).

### 2.4 Where state lives — transient session vs. document state

Per the [CLAUDE.md command-bus invariant](CLAUDE.md) and the §V2 transient carve-out:

- **The in-progress wizard is transient view/session state.** `IterativeGenSession` (current step,
  working config, reroll nonces, the cached preview `GeneratedSector`, dest path) **never lands in
  `sector.json`** until commit. It is written **directly** to `BuilderState`, *not* through the command
  bus — exactly like selection/drag/modal scratch in `state/panel_state.rs`. Re-rolling a stage just
  bumps a nonce and re-runs the preview; nothing is undoable because nothing is document state yet.
- **Only the final commit touches document state.** Two cases:
  - **New project** (the default, mirrors today): write `sector.json` to the chosen folder and
    `open_project(&dest)` → `*state = new_state`. This is a **session boundary**, not a mutation of
    the current sector, so the carve-out applies and **no `BuilderCommand` is required**
    (`generate_random.rs:264` `apply_result` does exactly this today).
  - **Regenerate inside the open project** (advanced): replacing `state.sector` *is* a document
    mutation and **must** go through the bus → a new `BuilderCommand::ReplaceSectorFromGeneration`
    (§4 Phase C, optional). This is the only place a command is needed.

---

## 3. Invariants — copy verbatim into every subagent brief

> These are non-negotiable. A subagent that does not know them will break them.

1. **RNG only through `src/model/rng.rs`.** All draws go through `stage_rng(root_seed, stage, disc)`
   (blake3 → ChaCha8). **Never** `rand::thread_rng()`, never seed from time/PID/anything outside the
   stage RNG inside the pipeline. (Seed *minting* for a brand-new sector may use `mint_seed`
   `random_sector.rs:290`, but that only produces the `root_seed` fed into `stage_rng`.)
2. **Re-roll nonces are append-only and back-compatible.** Nonce `0`/absent ⇒ discriminator is the
   **unchanged** legacy string ⇒ byte-identical output ⇒ golden tests unaffected. A nonce `n > 0`
   appends a fixed suffix (`":r{n}"`). Never reorder or renumber existing stage keys.
3. **No `FxMap`/`FxHashMap`/`FxSet` iteration for output.** Use `BTreeMap`/`BTreeSet` or sort keys
   before emission. Every pipeline stage already sorts its output by id before serialization
   (`mod.rs:602-606`, `world_placement.rs:114`) — preserve that.
4. **Sectors are square.** `sector_width == sector_height` everywhere. The size step exposes **one**
   "grid dimension" field bound to both; never add a path that lets them diverge. `SectorSize::Custom`
   carries a single `dim` (`random_sector.rs:60`), the existing custom UI already locks the two equal
   (`generate_random.rs:162`, resolves to `Custom { dim: w.max(h) }`), and `GEN_SECTOR_NOT_SQUARE`
   (`src/validate/validation.rs`) rejects divergence. Keep all of that.
5. **Document-state mutations go through the command bus.** `state.sector` / `data_catalogs` /
   chronicle / presence / claim / roster / relations- or economy-overrides → `state.run(BuilderCommand::…)`.
   Transient view/session state (selection, drag scratch, modal fields, **and the iterative-gen
   session**) is exempt and written directly.
6. **Output writers stay byte-stable.** After any change near `bitmap`/`svg_export`/`html_export`/
   `render`, or any RNG/stage change, run the golden suite (dispatch `test-runner`):
   `cargo test --test it -- golden`.

---

## 4. Implementation plan

Phased. **Phase E changes `pub` items in `src/` that the builder depends on → it is sequential and
goes first** (per the CLAUDE.md "anything that changes a re-exported type" recipe). Phases S/P/C live
in `builder/` and follow E. Each step lists its **file**, the **change**, and its **definition of done**.

### Phase E — engine seam (`src/`, do first, then `cargo check --workspace`)

**E0. Enumerate call sites before touching the orchestrator.** Fan out `rust-explorer` across `src/`,
`builder/`, `viewer/`, `tests/` for every caller of `generate_with_progress_and_cancel` and its public
wrappers (`generate_sector`, `generate_sector_with_progress`, `generate_random_sector_from_with_progress`).
Cross-check the list with a second search strategy. **DoD:** an exhaustive caller list you will re-check
after the refactor compiles.

**E1. Add the `Stage` enum.** In `src/gen/generation/mod.rs`, declare an ordered enum mirroring §1.
Declaration order = pipeline order, so derived `Ord` *is* the pipeline order.

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Stage {
    WorldPool, Placement, Regions, Systems, Factions, Routes,
    RegionRouteEffects, HiddenRoutes, StabilityRebalance, RouteControls,
    SystemState, Manifest, Archetypes, PowerProjection, InfluenceField,
    Relations, Economy, Chronicle,
}
impl Stage { pub const LAST: Stage = Stage::Chronicle; }
```

**DoD:** compiles; `Stage::Placement < Stage::Chronicle` holds.

**E2. Add `RerollNonces`.** New small type (same module or `src/gen/generation/reroll.rs`):

```rust
#[derive(Clone, Default)]
pub struct RerollNonces(std::collections::BTreeMap<Stage, u64>);   // BTreeMap, not Fx — invariant #3

impl RerollNonces {
    pub fn bump(&mut self, s: Stage) -> u64 { let n = self.0.entry(s).or_default(); *n += 1; *n }
    /// "" when nonce is 0/absent (byte-compat, invariant #2); ":r{n}" otherwise.
    pub fn suffix(&self, s: Stage) -> String {
        match self.0.get(&s) { Some(n) if *n > 0 => format!(":r{n}"), _ => String::new() }
    }
}
```

**DoD:** `RerollNonces::default().suffix(any)` is `""`.

**E3. Add `generate_prefix` and demote the old orchestrator to a wrapper.** In
`src/gen/generation/mod.rs`:

```rust
pub fn generate_prefix<F, C>(
    project: ProjectInput,
    through: Stage,
    nonces: &RerollNonces,
    progress: F,
    should_cancel: C,
) -> Result<GeneratedSector, SectorError>
where F: FnMut(SectorProgress), C: FnMut() -> bool { /* body = current orchestrator, guarded */ }

// back-compat wrapper — identical behavior & bytes to today
pub fn generate_with_progress_and_cancel<F, C>(project: ProjectInput, progress: F, should_cancel: C)
    -> Result<GeneratedSector, SectorError>
where F: FnMut(SectorProgress), C: FnMut() -> bool {
    generate_prefix(project, Stage::LAST, &RerollNonces::default(), progress, should_cancel)
}
```

Inside the moved body, guard each stage block with its `Stage` and weave the nonce suffix into the
discriminator. Examples (apply the same shape to **every** RNG stage from §1):

```rust
if Stage::Placement <= through {
    let mut rng = stage_rng(seed, "placement", &format!("sector{}", nonces.suffix(Stage::Placement)));
    /* …existing placement… */
}
if Stage::Regions <= through {                                   // ⚠️ regions DOES draw rng
    let mut rng = stage_rng(seed, "regions", &format!("sector{}", nonces.suffix(Stage::Regions)));
    /* …existing build_regions… */
}
// Systems / Worlds: append the stage suffix to the per-entity discriminator
let disc = format!("{sys_id}{}", nonces.suffix(Stage::Systems));   // ("system", "<sys_id>[:rN]")
let disc = format!("{world_id}{}", nonces.suffix(Stage::Worlds));  // add a Stage::Worlds? see note
```

> **Note on `Systems` vs `Worlds`.** §1 treats Worlds as a sub-stage of Systems. For v1, re-roll them
> together under a single `Stage::Systems` nonce (one user step "system contents"). If you want
> independent world re-rolls later, add a `Stage::Worlds` variant; the world-level reroll machinery
> already exists (`regenerate_world_payload` `world_placement.rs:321`, discriminator
> `"reroll:{world_id}:{counter}"`) and is the precedent to generalize.

The non-RNG stages (7–15, 17) are guarded by `if Stage::X <= through { … }` with no discriminator
change. `Routes` (stage 6) keeps its reserved key unchanged for now (⚠️ it draws nothing); still gate
it so step 6 can stop there.

**DoD:** `cargo check --workspace`; the back-compat wrapper produces identical output (the
equivalence test in Phase T proves this); **`cargo test --test it -- golden` is green** (nonce-0 path
is byte-identical).

**E4. Re-verify call sites.** Re-run the E0 search; fix any caller the wrapper signature didn't cover.
Re-export `Stage` / `RerollNonces` / `generate_prefix` from `src/lib.rs` alongside the existing
generation exports so `builder/` can use them. **DoD:** `cargo check --workspace` clean; downstream
crates compile.

### Phase S — builder session state (`builder/`)

**S1. Define the session struct.** New `builder/src/builder/state/iterative_gen.rs` (or fold into
`panel_state.rs` next to the other transient structs):

```rust
/// Transient wizard state — never serialized, never on the undo stack (invariant #5 carve-out).
pub struct IterativeGenSession {
    pub config: sectorforge::AppConfig,        // working knobs; baseline-scaffolded then user-edited
    pub root_seed: String,
    pub nonces: sectorforge::RerollNonces,
    pub current_step: GenStep,                 // Size, Placement, Regions, Systems, Factions, Routes, Finalize
    pub accepted_through: Option<GenStep>,     // furthest committed-in-session step
    pub preview: Option<sectorforge::GeneratedSector>,  // last prefix-run result, for SectorView
    pub dest: Option<camino::Utf8PathBuf>,     // None until the user picks a folder
    pub job: Option<()>,                       // handle to the in-flight background prefix run
}
```

Map `GenStep` → `Stage` cutoff exactly as the §2.3 table. Hang it on `BuilderState` as
`pub iterative_gen: Option<IterativeGenSession>` and write it **directly** (no command).

**S2. Background prefix runner.** Reuse the `RandomGenState`/`spawn` pattern (`random_run.rs:72`):
add a `spawn_prefix(&mut self, through: Stage)` that, off the UI thread, assembles a `ProjectInput`
from `session.config` + the loaded data catalogs and calls
`sectorforge::generate_prefix(project, through, &session.nonces, progress_cb, cancel_cb)`, then stashes
the result into `session.preview`. Thread `SectorProgress` to a progress bar. **DoD:** stepping to any
step populates `preview` without blocking the UI.

**S3. Re-roll + edit ops.** Add `generation_ops.rs`-style helpers (mirror the existing
`regenerate_world` / `reroll_seed` at `generation_ops.rs:191/218`):

- `reroll_step(step)` → `session.nonces.bump(step.stage())`, then `spawn_prefix(session.current_step.stage())`.
- `edit_knob(step, change)` → mutate `session.config`, **clear `accepted_through` back to `step`**
  (downstream invalidation, §2.3 DAG rule), then re-run the prefix.
- Always re-run **through the current step**, so the preview reflects every downstream re-derivation.

**DoD:** re-rolling step K twice with the same nonce is identical; bumping the nonce changes only step
K and later (verified visually + by the Phase T reroll test).

### Phase P — the panel (`builder/`, the canonical "add a panel" recipe)

Model the layout on `panels/factions.rs:86` (list + detail) and `panels/map/mod.rs:29` (canvas +
inspector + embedded `SectorView`). The existing generation-parameters surface
(`panels/generation.rs:37`) is the closest knob-form precedent.

**P1. Create `builder/src/builder/panels/iterative_gen.rs`:**

```rust
pub fn show(ui: &mut egui::Ui, state: &mut crate::builder::BuilderState) { /* … */ }
```

Layout:
- `SidePanel::left` (~280px, resizable): the **step rail** — the 7 `GenStep`s as a vertical list,
  current highlighted, accepted ones checked, downstream ones disabled until reached.
- `CentralPanel`: top = the **current step's knob form** (§5 maps each step to its config fields);
  bottom = the **live preview** via `SectorView` from `sectorforge-gui-core`
  (`map/interactions.rs:106` is the embedding pattern — build the struct, allocate exact size, render),
  fed from `session.preview`.
- Bottom action bar: **🎲 Re-roll this step** · **◀ Back** · **▶ Next** · **✓ Commit** · **✕ Cancel**.

All knob widgets read from / write to `session.config` **directly** (transient). All randomness uses
`sectorforge::model::rng::stage_rng` — never `thread_rng`. The **size** step exposes one grid-dimension
field bound to width *and* height (invariant #4); reuse the lock from `generate_random.rs:162`.

**P2. Wire the tab (exhaustive-match plumbing — the compiler enforces all four edits):**

1. `panels/mod.rs` (~`:50-85`): add `pub mod iterative_gen;`.
2. `state/types.rs:174` `enum BuilderTab`: add `IterativeGen`.
3. `state/types.rs:229` `label()`: `Self::IterativeGen => "ITERATIVE"`.
4. `state/types.rs:261` `ALL`: add `BuilderTab::IterativeGen`.
5. `panels/nav.rs:248` `show_active_panel`: `BuilderTab::IterativeGen => iterative_gen::show(ui, state),`.
6. `panels/nav.rs` `TAB_CLUSTERS`: add it to a cluster (e.g. alongside Export/Validation in
   "Analysis & Output").
7. `state/types.rs:207` `is_catalog_editor()`: returns `false` (it is not a catalog editor) unless
   the panel grows its own catalog.

**P3. Entry point.** Add a launcher next to the existing **🎲 Random sector…** button in the project
sidebar (`panels/project.rs:99`): an **"Iterative sector…"** button that initializes
`state.iterative_gen = Some(IterativeGenSession::new(baseline, seed))` and switches
`state.active_tab = BuilderTab::IterativeGen`.

**DoD:** the tab appears, dispatches, has a non-empty label, and the nav exhaustiveness tests
(`nav.rs:288`) pass.

### Phase C — commit (`builder/`)

**C1. New-project commit (default).** On **✓ Commit** with a chosen `dest`: write `sector.json` for
`session.preview` (which must be a *full* run — force `spawn_prefix(Stage::LAST)` and any post-gen
derivations the one-shot path runs: personae/sites/hooks/missions/prose, `random_sector.rs:660+`),
then `open_project(&dest)` exactly as `apply_result` (`generate_random.rs:264`,
`project_io.rs:409`). Clear `state.iterative_gen`. **Session boundary → no command (invariant #5
carve-out).**

**C2. (Optional) In-session regenerate.** If committing into the *already-open* project, route through
the bus instead:

1. `command.rs:96` `enum BuilderCommand`: add
   `ReplaceSectorFromGeneration { before: Box<GeneratedSector>, after: Box<GeneratedSector> }`
   (box the payloads — they are large).
2. `apply` (`command.rs:514`): `*sector = (*self.after).clone();`
   `revert` (`command.rs:969`): `*sector = (*self.before).clone();`
3. `dep_classes` (`command.rs:453`): return **all** structural classes
   (`SystemsWorlds`, `Routes`, `Factions`, `Regions`) — a whole-sector swap invalidates everything;
   over-invalidation is safe.
4. Bump the `all_variants` tripwire (`command.rs:1332`, currently **40**) to 41 and add the variant to
   the fixture, or `dep_classes_cover_all_variants` fails to compile/pass.
5. Call it via `state.run(BuilderCommand::ReplaceSectorFromGeneration { … })`.

**DoD:** new-project commit opens the generated sector as today; (if built) in-session commit is a
single undoable step round-tripping through `apply`∘`revert`.

### Phase T — tests (dispatch `test-runner`)

1. **Equivalence (the §2.2 guarantee).** Stepping straight through with default knobs and **zero
   re-rolls** equals the one-shot `generate_random_sector_from` for the same `(seed, SectorSize, baseline)`,
   byte-for-byte. Equivalently at the engine layer:
   `generate_prefix(p, Stage::LAST, &RerollNonces::default(), …) == generate_with_progress_and_cancel(p, …)`.
2. **Prefix monotonicity.** For any `K`, the stages `≤ K` of `generate_prefix(p, K, …)` are byte-identical
   to the same stages of the full run.
3. **Re-roll determinism.** `nonces.bump(stage)` once, run twice → identical; different nonce →
   different but reproducible; a nonce on stage K leaves stages `< K` unchanged.
4. **Golden suite stays green:** `cargo test --test it -- golden` (nonce-0 back-compat).
5. **Panel wiring:** `BuilderTab::IterativeGen` is in `ALL`, has a label, and has a dispatch arm
   (existing `nav.rs:288` tests cover this once the variant exists).
6. **Command round-trip** (only if C2 built): apply → mutate → revert restores the sector.

**DoD:** all of the above pass; `cargo test --workspace` and `cargo test --test it -- golden` green.

---

## 5. Per-step knob reference

| Step | Engine stage(s) | Config struct.field(s) | Default | Source |
|---|---|---|---|---|
| **Size & seed** | (config) | `GenerationConfig.sector_width`/`sector_height` (locked equal), `.seed` | from `SectorSize`; minted if blank | `config.rs:103`, `random_sector.rs:60`, `random_sector.rs:290` |
| **System placement** | `Placement` | `GenerationConfig.system_count`; `PlacementConfig.mode` (UniformGrid/WeightedGrid/Clustered) / `.cluster_bias` / `.minimum_system_distance` | count = round(density·cells), density 0.25–0.40; UniformGrid; 0.0; 1 | `config.rs:141` |
| **Warp regions** | `Regions` | `RegionsConfig.enabled` / `.count` / `.mean_size` / `.apply_to_routes` / `.conditions` *(from `data/routes/regions.toml`, **not** `[generation]`)* | false / 2 / 6 / true | `regions.rs:152` |
| **System contents** | `Systems`+`Worlds` | `GenerationConfig.min_worlds_per_system` / `.max_worlds_per_system` / `.world_feature_count`; `WorldSelectionConfig.same_star_colour_bias` / `.strict_same_star_colour` / `.avoid_duplicate_world_type_in_system` | 1–2 / 4–7; 3–5; 1.25 / false / false | `config.rs:103`, `config.rs:191`, `world_placement.rs:36` |
| **Factions** | `Factions` | faction roster (data catalogs) + assignment weights | — | `factions.rs:19` |
| **Routes** | `Routes` (+effects/hidden/rebalance/controls) | `RouteGenerationConfig.enabled` / `.route_density` / `.max_route_distance` / `.ensure_connected_graph` / `.stability_targets` | true / 0.30 / 4 / true / none | `config.rs:271`, `routes.rs:35` |
| **Finalize & overlays** | `SystemState`…`Chronicle` | (advanced) `RelationsGenerationConfig.min_world_presence`; economy cfg; history cfg | 1 | `config.rs:316` |

Seed initial knob values from `build_random_config(size)` (`random_sector.rs:314`), the public,
byte-deterministic roller, so the wizard opens on the same defaults a one-shot would have produced.
`RegionsConfig` lives in a separate TOML (`data/routes/regions.toml`); edit it via an in-memory patch
applied at load, preserving the split.

---

## 6. RNG re-roll recipe (exact)

For any stage `S` with a fixed discriminator `base` (e.g. `"sector"` for placement/regions/factions/
routes, `"<sys_id>"` for systems, `"<world_id>"` for worlds):

```text
disc = format!("{base}{}", nonces.suffix(S))
     = base            when nonce(S) == 0   ← legacy bytes, goldens unaffected (invariant #2)
     = base + ":r{n}"  when nonce(S) == n>0 ← deterministic distinct re-roll
rng  = stage_rng(root_seed, "<stage key>", &disc)
```

This generalizes the world-level precedent already in the tree
(`regenerate_world_payload`, `world_placement.rs:321`, discriminator `"reroll:{world_id}:{counter}"`,
driven by `world_reroll_counter` in `GenerationState`, `panel_state.rs:174`) and the full-sector
re-roll (`derive_reroll_seed`, `preview.rs:208`; `reroll_seed`, `generation_ops.rs:218`). Re-roll
counters belong on the **session** (transient), not in `sector.json`.

---

## 7. Subagent dispatch plan

Execute this file the CLAUDE.md way. Sequencing is dictated by one real data dependency: **Phase E
changes `pub` items in `src/` that `builder/` consumes, so E precedes S/P/C.** Within a phase, fan out.

1. **E0 (parallel, `rust-explorer` ×4):** call sites of `generate_with_progress_and_cancel` &
   wrappers across `src/`, `builder/`, `viewer/`, `tests/`. Merge; **cross-check with a 5th search**
   before editing (the list is load-bearing).
2. **E1–E4 (sequential, one agent carries the whole engine seam):** this is a single coherent change —
   do **not** fragment it across plan/code/test agents. Brief it with §3 invariants verbatim and the
   §1 stage table. Follow with `cargo check --workspace` (via `test-runner`).
3. **S + P (mostly parallel, `panel-implementer`):** S (session struct + ops) and P (panel + tab
   wiring) share `BuilderState` but are largely independent — the panel can be stubbed against the
   session API. `panel-implementer` knows the `BuilderState`/`BuilderCommand`/derivations pattern and
   will not bypass the bus. Brief it that the session is **transient (no command)** and the size field
   **locks width=height**.
4. **C (sequential after P):** `panel-implementer` for C1; if C2 is in scope, the same agent adds the
   command and bumps the `all_variants` tripwire (40 → 41).
5. **T (parallel, `test-runner`):** the equivalence + monotonicity + reroll tests, then the golden
   suite. Never let cargo output into the main thread.
6. **Verify the loop is closed:** after E compiles, re-run the E0 search to confirm no caller was
   missed; after T, confirm the golden suite actually ran and is green (not merely "green before").

Brief every agent with: exact files/symbols in scope, the §3 invariants, the required output shape
(`path:line` citations / unified diff / pass-fail with test names), and a definition of done.

---

## 8. Acceptance checklist

- [ ] `Stage`, `RerollNonces`, `generate_prefix` exist and are re-exported from `src/lib.rs`.
- [ ] `generate_with_progress_and_cancel` is a thin wrapper; its output is unchanged.
- [ ] `cargo test --test it -- golden` is **green** (nonce-0 back-compat).
- [ ] Equivalence test: straight-through, zero-reroll == one-shot, byte-for-byte.
- [ ] Prefix-monotonicity and reroll-determinism tests pass.
- [ ] `BuilderTab::IterativeGen` dispatches, has a label, is in `ALL`; nav exhaustiveness tests pass.
- [ ] The panel: 7-step rail, per-step knobs, live `SectorView` preview, Re-roll/Back/Next/Commit/Cancel.
- [ ] Re-rolling step K visibly changes only step K and downstream; earlier steps frozen.
- [ ] Size step exposes a single grid-dimension field; width==height always; `GEN_SECTOR_NOT_SQUARE`
      never trips.
- [ ] Session state is transient (absent from `sector.json`, absent from the undo log).
- [ ] New-project commit opens the sector via `open_project` (session boundary). (Optional) in-session
      commit is one undoable `ReplaceSectorFromGeneration` and `all_variants` is bumped.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] [GUIDE.md](GUIDE.md) updated with the new panel + engine seam (CLAUDE.md doc rule).

---

## 9. Risks & gotchas

- **Regions-before-worlds ordering (⚠️).** `Regions` draws RNG *and* feeds the world candidate pool.
  Surface it as step 3 (before "system contents") and re-run worlds whenever regions change. Getting
  this backwards silently drops anomaly bias.
- **Routes RNG is reserved, not live (⚠️).** Don't wire a "re-roll routes for different edges" promise
  to entropy that isn't consumed yet (`routes.rs:160` `let _ = rng;`). Today routes are a deterministic
  function of systems + density; "re-roll routes" only changes output if `route_density`/distance knobs
  change, or once stochastic edge selection is implemented behind the reserved key.
- **Golden drift.** Any non-zero nonce, reordered stage, or altered discriminator changes bytes. Keep
  nonce-0 byte-identical (invariant #2); run the golden suite after every engine touch.
- **Large-sector cost.** Naive full-prefix re-run on every knob nudge can lag on `Huge` (80×80). Debounce
  knob edits, run off-thread (`random_run.rs` pattern), and cache the accepted-step `GeneratedSector`.
- **Commit payload size.** `ReplaceSectorFromGeneration` carries two whole sectors; box them and prefer
  the new-project session-boundary path unless in-session undo is explicitly required.
- **`build_random_config` is byte-contracted.** It backs determinism tests (`random_sector.rs:314`).
  Read its rolled defaults to seed the wizard, but don't change its roll order.

---

## Appendix — `path:line` anchors

**Engine / pipeline (`src/`)**
- Orchestrator: `src/gen/generation/mod.rs:247` `generate_with_progress_and_cancel`
- Progress enum: `src/gen/generation/mod.rs:29` `SectorProgress`
- Stages: placement `placement.rs:10` (rng `:37`) · regions `regions.rs:276` (rng `:288`) ·
  systems `systems.rs:42` (rng `:52`) · worlds `world_placement.rs:36` (rng `:63`) ·
  factions `factions.rs:19` (rng `mod.rs:382`) · routes `routes.rs:26` (rng `mod.rs:401`, reserved) ·
  region-effects `regions.rs:568` · hidden-routes `hidden_routes.rs:274` ·
  rebalance `routes.rs:407` · route-controls `analysis/route_control.rs:215` ·
  system-state loop `mod.rs:576` · manifest `mod.rs:608`/`:811` · archetypes `archetypes.rs:178` ·
  power-projection `analysis/power_projection.rs:45`/`:165` · influence-field `analysis/influence_field.rs:93` ·
  relations `analysis/relations/derive.rs:46` (rng `:245`) · economy `analysis/economy/derive.rs:62`/`:505` ·
  chronicle `analysis/history/mod.rs:58` (rng `history/build.rs:23`)
- Intermediate type: `src/model/sector_model/mod.rs:44` `GeneratedSector`
- RNG: `src/model/rng.rs:8-12` `derive_stage_seed`, `:14-17` `stage_rng`, `:31-68` `weighted_index`

**Config (`src/`)**
- `src/loading/config.rs:103` `GenerationConfig` · `:141` `PlacementConfig` · `:191` `WorldSelectionConfig`
  · `:271` `RouteGenerationConfig` · `:316` `RelationsGenerationConfig`
- `src/gen/regions.rs:152` `RegionsConfig`
- `src/gen/random_sector.rs:60` `SectorSize` · `:290` `mint_seed` · `:314` `build_random_config`
  · `:319` `build_random_config_inner` · `:622` `generate_random_sector_from_with_progress`
- `src/validate/validation.rs` `GEN_SECTOR_NOT_SQUARE`

**Builder (`builder/`)**
- Random-gen flow: `panels/project.rs:99` (button) · `panels/generate_random.rs:87` (wizard),
  `:162` (size lock), `:264` (`apply_result`) · `random_run.rs:72` (`spawn`) · `project_io.rs:409` (`open_project`)
- Command bus: `command.rs:96` `BuilderCommand` · `:453` `dep_classes` · `:514` `apply` · `:969` `revert`
  · `:1332` `all_variants`/tripwire (count **40**) · `state/undo.rs:49` `run` · `:219` `undo` · `:259` `redo`
- Derivations: `state/derivations.rs:196`
- Transient state: `state/panel_state.rs` (Selection/MapView/DragPending/Feedback; `GenerationState` `:174`)
- RNG re-roll precedents: `world_placement.rs:321` `regenerate_world_payload` ·
  `state/generation_ops.rs:191` `regenerate_world` / `:218` `reroll_seed` · `preview.rs:208` `derive_reroll_seed`
- Panels & tabs: `panels/mod.rs:1` (contract), `:20-86` (decls) · `state/types.rs:174` `BuilderTab`,
  `:207` `is_catalog_editor`, `:229` `label`, `:261` `ALL` · `panels/nav.rs:248` `show_active_panel`,
  `:288` tests · `panels/factions.rs:86` (list+detail) · `panels/map/mod.rs:29` &
  `panels/map/interactions.rs:106` (`SectorView` embed) · `panels/generation.rs:37` (knob form)

**Commands to run** (through `test-runner` / `clippy-fixer`, never inline):
```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo test --test it -- golden
cargo clippy --workspace --all-targets -- -D warnings
```
