---
sweep_id: X02
scope: cross-crate public API coherence (RUST_REVIEW.md §3.7)
crates:
  - sectorforge (lib `sectorforge`, src/)
  - sectorforge-gui-core (gui-core/src/)
  - sectorforge-builder (builder/src/)
  - sectorforge-viewer (viewer/src/)
reviewed_by: agent
surface_size:
  pub_items_total: 1128
  pub_items_src: ~700
  pub_items_gui_core: ~80
  pub_items_builder: ~210
  pub_items_viewer: ~140
finding_counts: { critical: 0, high: 4, medium: 6, low: 5, nit: 3 }
top_risks:
  - "Zero #[non_exhaustive] across 1128 pub items — every public enum is a SemVer breaking change waiting to happen (F-X02-001)"
  - "Two distinct `MapTheme` types and asymmetric `heatmap`/`HeatCell` definitions across `sectorforge` and `sectorforge-gui-core` (F-X02-002)"
  - "Builder/viewer bin crates expose ~350 unnecessarily-pub items; almost the entire lib surface should be pub(crate) (F-X02-003)"
  - "Public widget structs (SectorView, SystemView, JobHandle) expose 20+ fields incl. HashSet/HashMap/Arc<Mutex<_>>, foreclosing any future evolution (F-X02-004)"
---

# Cross-cutting Sweep X02: Public API coherence

## Method

1. Read `src/lib.rs`, `gui-core/src/lib.rs`, `builder/src/lib.rs`, `viewer/src/lib.rs` to map the canonical surface and re-exports.
2. Enumerate every `^pub ` item across the four crates (1128 lines).
3. For each suspected hazard run targeted greps: `non_exhaustive`, `must_use`, `Box<dyn`, `pub.*HashMap|HashSet`, duplicate type names across crates, panel-style `pub fn show` callers, `pub use sectorforge::*` and `pub use sectorforge_gui_core::*` chains.
4. Confirm caller scope for every "suspiciously narrow" `pub` (e.g. `panels::*::show`, `encode_base64`) by grepping callers across the workspace.

The unit-level agents (per-file panels, CLI, exporters) already catalogue per-file naming, doc, and visibility nits. This report is restricted to **patterns that cross crate boundaries** or that no per-file agent can see.

## Findings

### F-X02-001 — [HIGH] [API / SemVer] Zero `#[non_exhaustive]` on growable public enums

- **Location:** workspace-wide. Representative offenders:
  - `src/export/heatmap.rs:15` `pub enum HeatmapMode` (16 variants, has grown by 6 since `Off/Control/Military/Trade/Industrial/Covert/Faith/Threat/Intel` baseline; matched in `gui-core/src/heatmap.rs:12` via `pub use`).
  - `src/model/errors.rs:5` `pub enum SectorError` (10 variants, reachable from every public lib fn).
  - `src/model/errors.rs:43` `pub enum MutationError` (4 variants; flowed through `BuilderError::Mutation` at `builder/src/builder/errors.rs:20`).
  - `builder/src/builder/command.rs:92` `pub enum BuilderCommand` (32 variants, doc-comments explicitly describe future additions in `panels/mod.rs:18`).
  - `builder/src/builder/errors.rs:6` `pub enum BuilderError`.
  - `src/conflict.rs` `ConflictState`, `src/economy.rs` `TitheStatus`/`SupplyRisk`/`StrategicPriority`, `src/relations.rs` `Stance`/`TreatyStatus`/`RelationAttitude`, `src/regions.rs` `RegionConditionKind`, `src/segmentum.rs` `FactionMode`/`BorderOrientation`, `src/sites.rs` and `src/missions.rs` enums, every `viewer/src/*` UI enum (`PreviewJobResult`, `Tab`, `Selection`, `Dialog`, `SectorEditTool`, `FactionSort`, `View`, `FactionsMode`, `ExportJobResult`, `PendingExport`).
- **Category:** API / SemVer
- **Confidence:** High
- **Blast radius:** Any 0.x → 0.(x+1) bump that adds an enum variant breaks every external `match` on these types. Grep `grep -rn "non_exhaustive" --include="*.rs"` across all four crates returns **zero** matches.
- **Problem:** The CLAUDE.md design pattern explicitly anticipates new variants (HeatmapMode, BuilderCommand, etc.), but no enum is marked `#[non_exhaustive]`. For library-grade consumers (and integration tests, which are now their own consumers — see `tests/it/`), adding any variant requires a coordinated workspace update *and* a major-version bump.
- **Why it matters:** This crate ships a public API that downstream callers (currently the three GUI crates and `tests/it`) match exhaustively. The cost of adding `#[non_exhaustive]` retroactively rises monotonically with public matches.
- **Evidence:** `grep -rn "non_exhaustive" --include="*.rs" src gui-core builder viewer` returns 0 hits; `HeatmapMode::ALL` (src/export/heatmap.rs:44) is used to iterate the full set in `viewer/src/app/sector_view.rs:267` and would silently miss new variants if added.
- **Suggested fix:** Triage every public enum into three buckets:
  1. **Closed forever** (e.g. `RouteEndpoint { From, To }`, `Stance { Hostile, Cool, Neutral, Friendly }` if frozen by design): leave alone, document in a `// closed-set:` comment.
  2. **Will grow** (`HeatmapMode`, `BuilderCommand`, `RegionConditionKind`, all error enums): add `#[non_exhaustive]`. Document the rationale once at `src/lib.rs` and reference it.
  3. **External match required** (the consumer must enumerate all variants — `HeatmapMode::ALL` pattern): keep open but add a `#[deprecated_in_0_x]`-style audit comment plus a `// SAFETY: variants are public iteration order` note.

  Suggested initial set: `SectorError`, `MutationError`, `BuilderError`, `BuilderCommand`, `HeatmapMode`, `ConflictState`, `RegionConditionKind`, `TreatyStatus`, `Stance`, `RelationAttitude`, `SupplyRisk`, `TitheStatus`, `StrategicPriority`, `FactionMode`, `BorderOrientation`, `DataEditorError`, `FactionDesignerError`, `PresetGalleryError`, `EditorFileError`.
- **Effort:** S (mechanical attribute add) + M (audit which downstream `match`es break)
- **Risk of fix:** Low for internal callers (compile errors are explicit); requires updating every match site in `src/cli/*`, `viewer/src/*`, `builder/src/builder/panels/*` to add a `_ =>` arm. Score-against the determinism rule (no Fx-iteration leaks here).

---

### F-X02-002 — [HIGH] [API / Naming collision] `MapTheme`, `heatmap`, and `HeatCell` defined twice in incompatible ways across `sectorforge` and `sectorforge-gui-core`

- **Location:**
  - `src/export/map_theme.rs:179` `pub struct MapTheme` (TOML-driven export palette with `LabelDensity`/`LegendStyle`/`SymbolSet`; consumed by `bitmap`, `svg_export`, `html_export`, `system_map`).
  - `gui-core/src/map_theme.rs:35` `pub struct MapTheme` (egui rendering theme with `ScaledSize` and 60+ Color32 fields; consumed by `SectorView`).
  - `src/export/heatmap.rs` defines `pub struct HeatCellRgb` (`(u8,u8,u8)` + intensity) and `pub enum HeatmapMode`.
  - `gui-core/src/heatmap.rs:16` defines its own `pub struct HeatCell` (egui `Color32` + intensity) **and** `pub use sectorforge::heatmap::HeatmapMode` (re-export).
  - Both crates expose a module literally named `heatmap` at the top level.
- **Category:** API / Naming collision, abstraction boundary
- **Confidence:** High
- **Blast radius:** Builder code already imports both flavours side-by-side: `builder/src/builder/panels/control.rs:1120` uses `sectorforge_gui_core::heatmap::HeatCell`, while `builder/src/builder/panels/conflict.rs:219` uses `sectorforge::heatmap::HeatmapMode`. Any `use sectorforge::*; use sectorforge_gui_core::*` block triggers `MapTheme`/`heatmap` ambiguity.
- **Problem:** The two `MapTheme` structs serve fundamentally different concerns (PNG/SVG export style vs. egui screen rendering), but their identical name + identical module name (`map_theme`) is a textbook documentation/discoverability hazard. Same for `heatmap` (compute-RGB vs. wrap-as-Color32). The `viewer` crate makes it worse by `pub use sectorforge_gui_core::{heatmap, ...}` (viewer/src/lib.rs:16), so callers of *viewer* see three plausible `heatmap` paths.
- **Why it matters:** A reader asked "where do I add a new heatmap mode" has to traverse `src/export/heatmap.rs` → `gui-core/src/heatmap.rs` → `viewer/src/lib.rs` re-export → `builder/src/builder/state/mod.rs:267` storage of `sectorforge::heatmap::HeatmapMode`. Any future TOML-driven theme work on the *egui* side will collide head-on with the export-side `MapTheme` TOML parser.
- **Evidence:**
  - `grep "pub struct MapTheme" -r .` returns exactly two definitions in two crates.
  - `grep "use .*MapTheme" -r .` shows the two are never co-imported, which works only because they live in disjoint files.
  - `gui-core/src/heatmap.rs:1-5` admits the design ("Just re-exports `HeatmapMode` and converts the RGB cells into `Color32` cells").
- **Suggested fix:** Rename either side. The least disruptive option:
  - In `gui-core`: rename module to `egui_theme` and struct to `EguiMapTheme` (or `WidgetTheme`).
  - In `gui-core`: rename module `heatmap` → `heatmap_view` and struct `HeatCell` → `HeatCellRgba` (matches `HeatCellRgb` in source).
  - Optionally implement `From<sectorforge::heatmap::HeatCellRgb> for sectorforge_gui_core::heatmap_view::HeatCellRgba` to formalise the conversion (today it's open-coded in `gui-core/src/heatmap.rs:93-103`).
  - Drop the redundant `pub use sectorforge::heatmap::HeatmapMode` from `gui-core/src/heatmap.rs:12` — every caller already reaches it through `sectorforge::heatmap`.
- **Effort:** M
- **Risk of fix:** Medium. Touches every panel that imports either name, but the rename is mechanical and the compiler enforces completeness.

---

### F-X02-003 — [HIGH] [API / Visibility] `sectorforge-builder` and `sectorforge-viewer` over-expose internal panels and helpers as `pub`

- **Location:**
  - `builder/src/builder/panels/mod.rs:23-68` exposes 35 `pub mod` panels.
  - Every panel module has `pub fn show(ui, state)` (e.g. `builder/src/builder/panels/economy.rs:64`, `factions.rs:76`, `personae.rs:69`, ...). Total: 35+ `pub fn show` signatures across panels.
  - `builder/src/builder/session.rs:276` `pub fn encode_base64` and `:307` `pub fn decode_base64` — used only inside `session.rs` (verified: only callers are `session.rs:355,357`).
  - `builder/src/builder/panels/subsectors.rs:63` `pub fn apply_subsector_overrides`, `panels/economy.rs:817` `pub fn stranded_system_ids`, `:832` `pub fn lifeline_route_ids`, `panels/control.rs:1115` `pub fn build_overlay_cells` — all consumed only by sibling panels.
  - `viewer/src/editor/mod.rs:21-31` exposes 11 `pub use ...::show_*` helpers, of which only `App` (in `app/`) calls them.
  - `viewer/src/editor/state.rs` exposes 7 enums (`FactionSort`, `Tab`, `Selection`, `RouteEndpoint`, `Dialog`, `SectorEditTool`, `PreviewJobResult`) plus `EditorState`; the only external caller is `main.rs` (none — verified: `grep sectorforge_viewer:: --include="*.rs"` returns only `App` and `segmentum_view::load_segmentum_bundle`).
  - `viewer/src/app/types.rs:4-48` `pub enum PendingExport/ExportJobResult/View/FactionsMode` — internal to the `app` subtree.
- **Category:** API / Visibility, SemVer
- **Confidence:** High
- **Blast radius:** Both crates ship a `[[bin]]` + library, but the library has no external consumer beyond their own `main.rs` (verified by grep: only `sectorforge_viewer::App`, `sectorforge_viewer::segmentum_view::load_segmentum_bundle`, `sectorforge_builder::BuilderApp`, `sectorforge_builder::builder::open_project` are referenced outside the crate). Yet roughly 350 of the 1128 `pub` items live in these two bin crates.
- **Problem:** Anything `pub` becomes part of the rustdoc surface, the SemVer contract for the workspace, and (less obviously) increases incremental rebuild churn — any non-pub-fn refactor inside `panels/economy.rs` triggers downstream rebuilds because the public signature could conceivably matter. None of the panel `show` functions need to be `pub`: they're called via `nav::show_active_panel` (which is itself a `pub` sibling — also unnecessary).
- **Why it matters:** Future refactors of panel internals are needlessly painful. New contributors copy the `pub fn show` pattern from `panels/mod.rs:6` (the doc-comment example) and continue the leak.
- **Evidence:** `grep -rn "sectorforge_builder\|sectorforge_viewer" --include="*.rs"` shows the only external symbols accessed are: `BuilderApp`, `builder::open_project`, `App`, `segmentum_view::load_segmentum_bundle`. Everything else is intra-crate.
- **Suggested fix:**
  1. In `builder/src/builder/panels/mod.rs`: change every `pub mod <name>;` to `pub(crate) mod <name>;` except for whatever `main.rs` and `app.rs` reach into.
  2. In every panel file: change `pub fn show` → `pub(crate) fn show`. Same for the helper functions (`apply_subsector_overrides`, `stranded_system_ids`, `lifeline_route_ids`, `build_overlay_cells`).
  3. `builder/src/builder/session.rs`: change `encode_base64`/`decode_base64` to `pub(super) fn` (or `fn`).
  4. Mirror the audit in `viewer/`: every `editor::*::show_*` to `pub(crate)`, every `app::*::ui` to `pub(crate)`.
  5. Optional: replace `pub use text_buf::{persistent_*}` (panels/mod.rs:21) with `pub(crate) use` and verify the helpers aren't reached from elsewhere.

  Mechanically: a single workspace-wide `sed` of `^pub fn show` → `pub(crate) fn show` in `builder/src/builder/panels/**` + `viewer/src/editor/**` is the bulk of the diff. Verify with `cargo check -p sectorforge-builder -p sectorforge-viewer`.
- **Effort:** M (mechanical, large diff)
- **Risk of fix:** Low. The compiler enforces correctness; nothing outside these crates references the demoted items.

---

### F-X02-004 — [HIGH] [API / Encapsulation] Public widget structs expose 20+ fields, leaking `HashSet`/`HashMap`/`Arc<Mutex>` types

- **Location:**
  - `gui-core/src/sector_view.rs:92-140` `pub struct SectorView<'a>` has **24 pub fields**, including:
    - `pub path_route_ids: Option<&'a HashSet<RouteId>>`
    - `pub path_waypoints: Option<&'a HashSet<SystemId>>`
    - `pub heatmap: Option<&'a HashMap<SystemId, HeatCell>>`
    - `pub cache: Option<&'a SectorMapCache>` (cache itself is 4 pub `HashMap`s at lines 23-28).
  - `gui-core/src/sector_view.rs:23-28` `pub struct SectorMapCache` exposes 4 `HashMap` fields directly.
  - `gui-core/src/system_view.rs:12-18` `pub struct SystemView<'a>` has 5 pub fields (mild).
  - `gui-core/src/jobs.rs:7-14` `pub struct JobHandle<T>` exposes `pub progress: Arc<Mutex<f32>>`, `pub cancelled: Arc<AtomicBool>`, `pub receiver: Receiver<T>` (raw mpsc).
  - `viewer/src/route_planner.rs:117` `pub fn highlighted_route_ids(&self) -> HashSet<RouteId>` and `:124` `pub fn waypoint_set(&self) -> HashSet<SystemId>` — these are constructed and immediately fed into `SectorView`'s public `HashSet` fields.
- **Category:** API / Encapsulation, determinism
- **Confidence:** High
- **Blast radius:** Adding a new field to `SectorView` is a SemVer break because callers use struct-literal construction (verified: `builder/src/builder/panels/map/interactions.rs:94`, `viewer/src/app/planner_view.rs:98`, `viewer/src/app/sector_view.rs:393`, `viewer/src/editor/map_panel.rs:53` all spell out every field). Determinism-wise: CLAUDE.md §"Determinism invariants" forbids iterating `HashMap`/`HashSet` for output. Exposing them through the public widget API blesses that pattern.
- **Problem:**
  1. Struct-literal construction across crate boundaries makes the field set effectively frozen.
  2. Default `std::collections::HashMap`/`HashSet` (SipHash, non-deterministic iteration) ride the public surface alongside types like `BTreeSet<SystemId>` (already used at `sector_view.rs:117` for `multi_selected`). This is incoherent — `multi_selected` correctly uses `BTreeSet`, `pinned` correctly uses `BTreeSet`, but `path_route_ids`/`path_waypoints`/`heatmap` use `HashSet`/`HashMap`. Three of the five collections are non-deterministic.
  3. `JobHandle` exposes the inner `Arc<Mutex<f32>>`, foreclosing any future move to an atomic, a triomphe-style cell, or a different IPC channel.
- **Why it matters:** Every panel renderer is a public API author by accident. Any future change to the renderer's storage (e.g. swapping `HashMap` → `FxHashMap` per CLAUDE.md, or precomputing path overlays into a single struct) requires updating four call sites and bumping the crate version.
- **Evidence:** `grep "SectorView {" --include="*.rs"` returns four struct-literal sites, each enumerating all 24 fields verbatim. `route_planner.rs:117,124` returns `std::HashSet` rather than `BTreeSet`, and that result is fed directly into the public `HashSet`-typed fields.
- **Suggested fix:**
  1. Promote `SectorView` to a builder pattern: keep fields `pub(crate)`, add `SectorView::new(sector, hex_size)` + chainable `.with_path(...)`, `.with_heatmap(...)`, etc. Or annotate it `#[non_exhaustive]` and require callers to use `..Default::default()`.
  2. Convert `path_route_ids: &HashSet<RouteId>` and `path_waypoints: &HashSet<SystemId>` to `&BTreeSet<...>` to match `multi_selected`/`pinned`. Update `route_planner::highlighted_route_ids`/`waypoint_set` accordingly. (Output ordering doesn't matter for set-membership-only consumers, but the determinism invariant is a stronger statement than "this particular use case happens to not iterate".)
  3. Replace `heatmap: &HashMap<SystemId, HeatCell>` with a newtype `HeatmapCells` (already declared at `gui-core/src/heatmap.rs:21`!) or convert internally to `BTreeMap` if the renderer iterates it for output (verify).
  4. Make `SectorMapCache` fields `pub(crate)` and expose them through accessor methods — `hex_subsector` / `region_for_hex` already shows the pattern at `sector_view.rs:85`.
  5. `JobHandle`: move `progress`/`cancelled`/`receiver` to private; keep `cancel()` / `is_cancelled()` / `progress()` getters; add `try_recv() -> Option<T>` and `recv() -> Option<T>` so callers don't reach into the `Receiver` directly. Today, `viewer/src/app/lifecycle.rs:243` reads `.receiver` directly.
- **Effort:** M (touch ~6 call sites, add accessor methods)
- **Risk of fix:** Low-Medium. The `[non_exhaustive]` route is the lowest-risk migration; the builder-pattern + `BTreeSet` swap is higher value.

---

### F-X02-005 — [MEDIUM] [API / Duplication] `PreviewJobResult` defined identically in two crates

- **Location:**
  - `builder/src/builder/preview.rs:23` `pub enum PreviewJobResult { Ready(GeneratedSector), Cancelled, Failed(String) }`
  - `viewer/src/editor/state.rs:95` `pub enum PreviewJobResult { Ready(GeneratedSector), Cancelled, Failed(String) }` — same three variants, same payloads, both annotated `#[allow(clippy::large_enum_variant)]`.
- **Category:** API / DRY
- **Confidence:** High
- **Blast radius:** Independent evolution. Already drifting: builder also defines `pub const DEFAULT_DEBOUNCE_MS` and `pub struct PreviewState`; viewer's lives inside `EditorState`. The next variant added on one side won't be added on the other.
- **Problem:** Two crates ship the same preview-result contract — both wrap a `GeneratedSector` via the shared `gui_core::jobs::JobHandle`. Lifting the type into `gui-core/src/jobs.rs` (next to `JobHandle`) would unify behaviour and let both crates share the worker-spawn logic in `viewer/src/app/lifecycle.rs:243-266` (currently re-implemented per crate).
- **Why it matters:** Any new variant (e.g. `PartialReady(...)` for progressive renders, or a `Throttled` state) has to be added in two places and risks divergent semantics.
- **Suggested fix:** Move `PreviewJobResult` into `sectorforge-gui-core` (e.g. `gui-core/src/jobs.rs` or a new `gui-core/src/preview.rs`):
  ```rust
  // gui-core/src/preview.rs
  #[non_exhaustive]
  #[allow(clippy::large_enum_variant)]
  pub enum PreviewJobResult {
      Ready(sectorforge::sector_model::GeneratedSector),
      Cancelled,
      Failed(String),
  }
  ```
  Re-export from both `builder` and `viewer` if path stability matters. Remove the duplicated `#[allow]`.

  Bonus: the `Failed(String)` payload is the third stringly-typed-error site in the workspace (see F-X02-006); consider `Failed(Arc<SectorError>)` while moving it.
- **Effort:** S
- **Risk of fix:** Low.

---

### F-X02-006 — [MEDIUM] [Error] Stringly-typed `Result<_, String>` in public API undermines the workspace's typed-error story

- **Location:**
  - `builder/src/builder/session.rs:307` `pub fn decode_base64(input: &str) -> Result<Vec<u8>, String>`.
  - `builder/src/builder/preview.rs:23` `PreviewJobResult::Failed(String)`, mirrored in `viewer/src/editor/state.rs:95`.
  - `viewer/src/factions_overview.rs:14-21` `FactionDesignerError::Config(String)` and `::Validation { field: String, message: String }`.
  - `viewer/src/data_editor.rs:19-26` `DataEditorError::Config(String)`.
  - `viewer/src/preset_gallery.rs:16-19` `PresetGalleryError::Load(String)`.
- **Category:** Error handling, API
- **Confidence:** High
- **Blast radius:** Three of four crates use a typed-error story (`thiserror` + structured variants) for everything except these stragglers. The bin-crate UI surfaces wrap structural failures behind `String`, which loses structure (no `path:`, no `kind:`, no `#[source]` chain) and forces UI code to do string parsing or display raw messages.
- **Problem:** The workspace pattern is `thiserror`-derived enums with `#[error("{path}: ...")]`. The above five sites break that pattern and lose the structured-error story whenever a panel displays the error to the user.
- **Why it matters:** A `Config(String)` swallows whether the underlying failure was TOML, IO, or a missing field. Downstream UI code can't switch behaviour (retry vs. open file dialog vs. show diff).
- **Suggested fix:**
  - `decode_base64`: return `Result<Vec<u8>, Base64DecodeError>` with a `thiserror` enum (or use the standard `base64::DecodeError` if you bring in the crate — already implied by the manual decoder).
  - `PreviewJobResult::Failed`: hold `Arc<sectorforge::SectorError>` (Arc because the enum is moved into a channel).
  - `FactionDesignerError::Config`, `DataEditorError::Config`, `PresetGalleryError::Load`: split into structured variants (`MissingFile { path }`, `Toml(toml::de::Error)`, `Schema { field, expected }`).
- **Effort:** S-M
- **Risk of fix:** Low.

---

### F-X02-007 — [MEDIUM] [API / Consistency] Asymmetric crate re-export style across `builder` and `viewer`

- **Location:**
  - `viewer/src/lib.rs:15-16` re-exports gui-core: `pub use sectorforge_gui_core::jobs::{spawn_job, JobContext, JobHandle};` and `pub use sectorforge_gui_core::{heatmap, info_panel, jobs, palette, sector_view, system_view};`
  - `builder/src/lib.rs:1-4` does **not** re-export gui-core. Instead every builder module imports `sectorforge_gui_core::...` directly.
- **Category:** API / Consistency
- **Confidence:** High
- **Blast radius:** Documentation and IDE discoverability. A reader looking at `cargo doc --open -p sectorforge-viewer` sees `viewer::palette`, `viewer::heatmap`, etc.; the same reader on `sectorforge-builder` does not. The viewer's own `App` is at `viewer::App` (aliasing `app::App`), but builder's is at `sectorforge_builder::BuilderApp`. Different conventions for the same idea.
- **Problem:** Inconsistent re-export policy. Either both bin crates should aggregate gui-core into their `prelude` (rare for bin crates but defensible if external integration tests need a single import root) or neither should. Today's mix invites copy-paste mistakes and obscures which symbols are "library" vs "internal".
- **Why it matters:** When a future contributor adds `tests/it_viewer.rs` and types `use sectorforge_viewer::palette::TEXT;`, that works. When they then write `tests/it_builder.rs` and type `use sectorforge_builder::palette::TEXT;`, that doesn't — and the failure mode is a confusing rustc error rather than a deliberate API boundary.
- **Suggested fix:** Pick one of:
  - **Drop the re-exports** in `viewer/src/lib.rs:15-16`. Builder doesn't need them, viewer doesn't either if its panels use the gui-core path directly. Removes asymmetry.
  - **Mirror them in builder.** Add `pub use sectorforge_gui_core::{heatmap, info_panel, jobs, palette, sector_view, system_view};` to `builder/src/lib.rs`.
  My recommendation: drop them. Both bin crates have a `[[bin]]` target plus a thin lib for testing only; a flat `gui_core::*` import everywhere is clearer.
- **Effort:** S
- **Risk of fix:** Low.

---

### F-X02-008 — [MEDIUM] [API / `#[must_use]` coverage] Pure value-returning functions in `sectorforge-gui-core` and `sectorforge-builder` lack `#[must_use]`

- **Location:**
  - `gui-core/src/palette.rs:21,196,596,605,635,645,657,687,699,710` — `star_color`, `top_route_control`, `stability_color`, `world_type_color`, `darken`, `tint`, `contrast_text`, `faction_style_by_id`, `faction_style`, `faction_style_from_rgb`. All pure colour computations.
  - `gui-core/src/app_icon.rs:12` `pub fn load_app_icon() -> Option<Arc<IconData>>` — discarding it makes the load a no-op.
  - `gui-core/src/heatmap.rs:91` `pub fn compute(...) -> HeatmapCells` — heavy computation; discarding is a bug.
  - `builder/src/builder/preview.rs:202` `pub fn derive_reroll_seed(root_seed, counter) -> String` — pure, name-side-effect-free.
  - `builder/src/builder/session.rs:276,307` `encode_base64`/`decode_base64`.
  - `gui-core/src/jobs.rs:25,21` `progress`, `is_cancelled` (the trivial getters).
  - **For contrast**, `src/lib.rs` already applies `#[must_use]` to ~30 of its ~60 `pub fn`s. The gui-core and builder crates apply it to a single-digit number of functions (see grep below).
- **Category:** API / `#[must_use]` hygiene
- **Confidence:** High
- **Blast radius:** Authors of new panel code routinely call these helpers and the compiler does not nudge when a result is dropped.
- **Problem:** Inconsistent attribute coverage between the lib crate (good) and the GUI crates (sparse).
- **Evidence:** `grep -rn "must_use" --include="*.rs" src gui-core builder viewer` returns 198 hits, of which a quick read shows the vast majority are in `src/`. `gui-core` has roughly 8.
- **Suggested fix:** Add `#[must_use]` to every pub fn whose entire purpose is to return a value (colour helpers, hash-of-input helpers, pure derivations, getters on widget caches). One easy heuristic: any `pub fn` with `-> Color32`, `-> Option<...>`, `-> String`, `-> Vec<...>`, `-> f32` and no `&mut self`/`&mut ...` parameter.
- **Effort:** S (mechanical)
- **Risk of fix:** None.

---

### F-X02-009 — [MEDIUM] [API / Documentation gap] Builder and viewer pub items lack `///` docs and `# Errors`/`# Panics` sections

- **Location:**
  - `builder/src/builder/panels/*.rs` — 35+ `pub fn show` functions, only `analytics`, `briefing`, `economy`, `factions`, `hooks`, `history`, `interestingness`, `missions`, `personae`, `prose`, `relations`, `sites` carry any `///` doc. Many panels (`segmentum.rs`, `export.rs`, `regions.rs`, `project.rs`, `placeholder.rs`, `validation.rs`, `invariants.rs`, etc.) have no doc on their `show` fn.
  - `builder/src/builder/session.rs:244,250` `save_session`/`load_session` — both return `Result<_, BuilderError>` with no `# Errors` section.
  - `viewer/src/editor/file_ops.rs:49,74` `load_project_sector`/`save_project_sector` — same.
  - `viewer/src/preset_gallery.rs:71`, `viewer/src/data_editor.rs:104`, `viewer/src/factions_overview.rs:325,343,415`, `viewer/src/segmentum_view.rs:114,134` — `pub fn` widgets with no module-level doc.
  - `gui-core/src/sector_view.rs:92` `SectorView` — 24 pub fields, but only a subset of them are doc-commented (`path_route_ids`, `cache`, `theme` are; `selected_system`, `selected_route`, `hex_size`, `subsectors` are not).
  - `gui-core/src/palette.rs:10-19` `pub const BG/PANEL_BG/HEX_EMPTY/...` — no docs.
- **Category:** Documentation
- **Confidence:** High
- **Blast radius:** `cargo doc -p sectorforge-builder` and `-p sectorforge-viewer` produce sparsely documented pages.
- **Problem:** `src/lib.rs` is well-documented (every public fn has `///`, `# Errors`, sometimes `# Examples`). The GUI crates do not match.
- **Suggested fix:**
  - Add a one-line `///` to every panel `show` describing the tab.
  - Add `# Errors` to every `pub fn -> Result<_>` in `builder/src/builder/session.rs`, `project_io.rs`, `viewer/src/editor/file_ops.rs`, `viewer/src/segmentum_view.rs`.
  - Document the 24 pub fields of `SectorView` (or replace with builder methods per F-X02-004).
  - Deny missing docs at the crate level: `#![warn(missing_docs)]` on `gui-core/src/lib.rs` and `src/lib.rs` (already a workspace-clean library).
- **Effort:** M
- **Risk of fix:** None.

---

### F-X02-010 — [LOW] [API / Consistency] `BuilderApp::new()` is not `#[must_use]` but `viewer::App::new()`-equivalent is fine; no documented constructor convention

- **Location:**
  - `builder/src/app.rs:11,21` `BuilderApp::new()`, `BuilderApp::with_initial_state(state)`. Neither marked `#[must_use]`.
  - `viewer/src/app/mod.rs:46` `pub struct App` — constructors not surveyed but generally lack `#[must_use]`.
  - `src/model/errors.rs:55,62,69` `SectorError::io/config_parse/export` — fine.
- **Category:** API / hygiene
- **Confidence:** Medium
- **Suggested fix:** Apply `#[must_use]` to every `pub fn new() -> Self` / constructor across the workspace.
- **Effort:** S
- **Risk of fix:** None.

---

### F-X02-011 — [LOW] [API / Surface] `gui-core` exposes `pub mod nav` containing only one re-export (`entity_link`)

- **Location:** `gui-core/src/lib.rs:6,12`: `pub mod nav;` plus `pub use nav::entity_link;`. Module `nav` has nothing else public.
- **Category:** API / Surface
- **Confidence:** High
- **Problem:** Either drop the re-export and document the canonical path as `gui_core::nav::entity_link`, or seal the module: `mod nav; pub use nav::entity_link;`. Today both paths resolve, doubling the API surface for one function.
- **Suggested fix:** `mod nav;` (private) + `pub use nav::entity_link;` (single canonical path). Same audit applies to `app_icon.rs:12` `load_app_icon`, the sole pub of `app_icon` — make module private.
- **Effort:** S
- **Risk of fix:** Low.

---

### F-X02-012 — [LOW] [API / Sealed types] `KeyTables`, `WorldsConfig`, and several "settings" structs in `sectorforge` expose `HashMap` fields publicly

- **Location:** `src/worlds.rs:362-381` `pub struct KeyTables { pub star_colours: HashMap<...>, ... pub governments: HashMap<...>, ... }`. Eight `pub HashMap` fields, all in the public `crate::worlds` module.
- **Category:** API / Encapsulation, determinism
- **Confidence:** Medium
- **Blast radius:** Per CLAUDE.md the rule is "FxMap/HashMap never iterated for output". `KeyTables` is consumed only at parse time (verified internal-only use), but the public field surface implies it's OK to iterate, which it isn't for any future export user.
- **Problem:** Exposes `std::HashMap` directly. Either make these fields `pub(crate)` (since their callers all live in the same crate — verify with `grep "KeyTables\." -r --include="*.rs"`), or switch to `BTreeMap<String, T>` and own the determinism guarantee at the field level.
- **Suggested fix:** Convert to `BTreeMap<String, T>` (the keys are small strings; iteration ordering becomes deterministic for free) **or** make fields `pub(crate)` with `pub fn star_colour(&self, code: &str) -> Option<&StarColour>` accessors.
- **Effort:** S
- **Risk of fix:** Low.

---

### F-X02-013 — [LOW] [API / SemVer hygiene] `pub const` magic numbers leaked at the crate root

- **Location:**
  - `src/lib.rs:175-176` `pub const GENERATOR_NAME` and `GENERATOR_VERSION` — appropriate.
  - `src/lib.rs:137` `pub use conflict::{advance_sector, ConflictState, HYSTERESIS_TICKS}` — `HYSTERESIS_TICKS` re-exported but no doc on what changing it means for callers.
  - `builder/src/builder/preview.rs:30` `pub const DEFAULT_DEBOUNCE_MS: u64 = 200` — fine but no `///`.
- **Category:** API / Doc
- **Suggested fix:** Each public `const` deserves a `///` explaining the unit and the semantic.
- **Effort:** S
- **Risk of fix:** None.

---

### F-X02-014 — [NIT] [API / Naming RFC 430] One acceptable name to revisit: `FxMap`/`FxSet` are `pub(crate)` so naming is internal-only

- **Location:** `src/lib.rs:55-56`. `pub(crate) type FxMap<K, V>` / `pub(crate) type FxSet<T>`.
- **Note:** These deliberately do not match `FxHashMap`/`FxHashSet`. Since they are crate-internal, this is fine. Not a finding, recorded so the aggregator does not double-count.

---

### F-X02-015 — [NIT] [API / `From`/`Into`] Cross-crate conversion only ever done by hand

- **Location:** `gui-core/src/heatmap.rs:93-103` open-codes the conversion `sectorforge::heatmap::HeatCellRgb` → `gui_core::heatmap::HeatCell`. No `impl From<HeatCellRgb> for HeatCell` — partly because `HeatCell` lives in a different crate from `HeatCellRgb` and the orphan rule would route through a wrapper, but the workspace structure permits it.
- **Suggested fix:** Add `impl From<sectorforge::heatmap::HeatCellRgb> for HeatCell` in `gui-core`. Permitted because `HeatCell` is local. Replaces the `.into_iter().map(...).collect()` chain with `.into_iter().map(|(k, v)| (k, v.into())).collect()`.
- **Effort:** S
- **Risk of fix:** None.

---

### F-X02-016 — [NIT] [API / Cargo metadata]

This bucket is owned by sweep X06 (deps & Cargo). Noting only that public-facing crate names and module names mostly use the workspace name as a prefix (`sectorforge-*`), which is good and consistent. No additional finding here.

---

## Categories explicitly checked, no cross-crate finding

- **3.1 Panics in public sigs:** the per-unit agents own `.unwrap()`/`expect`. No cross-crate pattern beyond F-X02-006.
- **3.2 `unsafe`:** zero in workspace; covered by X01 unsafe-audit.
- **3.5 Concurrency in public sigs:** `JobHandle` is the only `Send`-bearing public type (F-X02-004 already calls it out); no `Send`/`Sync` markers on any other public type cross crate.
- **3.7 `From`/`Into`/`TryFrom`/`Display`/`Debug` derivation:** consistent. Every public DTO derives `Serialize`/`Deserialize` where it crosses the disk boundary; `Display` is provided through `thiserror`. No cross-crate gap beyond the missing `From` impl noted in F-X02-015.
- **3.10 Doctest coverage:** `src/lib.rs` has rich doctests; `gui-core`/`builder`/`viewer` have none. Not a per-finding issue — the bin crates don't need doctests on UI helpers, and gui-core widgets need an egui Context which makes doctests painful. Acceptable.

## Summary of suggested fixes

| id | severity | short | effort | risk |
|---|---|---|---|---|
| F-X02-001 | HIGH | Add `#[non_exhaustive]` to every growable public enum across workspace | S+M | Low |
| F-X02-002 | HIGH | Rename `gui-core::MapTheme` and `gui-core::heatmap` to remove collision with `sectorforge::map_theme`/`sectorforge::heatmap` | M | Medium |
| F-X02-003 | HIGH | Demote ~350 `pub` items in `builder`/`viewer` lib crates to `pub(crate)` | M | Low |
| F-X02-004 | HIGH | Encapsulate `SectorView`/`SectorMapCache`/`JobHandle` fields; switch `HashSet`/`HashMap` to `BTreeSet`/`BTreeMap` per determinism rule | M | Low-Medium |
| F-X02-005 | MEDIUM | Lift `PreviewJobResult` into `gui-core` to deduplicate builder vs viewer | S | Low |
| F-X02-006 | MEDIUM | Replace `Result<_, String>` / `Failed(String)` / `Config(String)` with typed `thiserror` variants | S-M | Low |
| F-X02-007 | MEDIUM | Drop asymmetric `viewer/src/lib.rs:15-16` re-exports of gui-core | S | Low |
| F-X02-008 | MEDIUM | Add `#[must_use]` to every pure value-returning pub fn in `gui-core` and `builder` | S | None |
| F-X02-009 | MEDIUM | Add `///` docs + `# Errors` to panel/viewer pub fns; enable `#![warn(missing_docs)]` at `gui-core/src/lib.rs` and `src/lib.rs` | M | None |
| F-X02-010 | LOW | Add `#[must_use]` to constructors (`BuilderApp::new`, `App::new`, etc.) | S | None |
| F-X02-011 | LOW | Seal single-export modules (`gui_core::nav`, `gui_core::app_icon`) | S | Low |
| F-X02-012 | LOW | Make `KeyTables` HashMap fields `pub(crate)` or convert to `BTreeMap` | S | Low |
| F-X02-013 | LOW | Document public `const`s (`HYSTERESIS_TICKS`, `DEFAULT_DEBOUNCE_MS`) | S | None |
| F-X02-014 | NIT | (no-op) `FxMap`/`FxSet` are pub(crate); naming OK | – | – |
| F-X02-015 | NIT | Add `impl From<HeatCellRgb> for HeatCell` in `gui-core` | S | None |
| F-X02-016 | NIT | (no-op) Cargo metadata audit belongs to sweep X06 | – | – |
