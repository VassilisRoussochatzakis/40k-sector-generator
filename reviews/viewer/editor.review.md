---
unit_id: U021
crate: viewer
paths:
  - viewer/src/editor/mod.rs
  - viewer/src/editor/state.rs
  - viewer/src/editor/dialogs.rs
  - viewer/src/editor/factions_panel.rs
  - viewer/src/editor/map_panel.rs
  - viewer/src/editor/generation_panel.rs
  - viewer/src/editor/enums.rs
  - viewer/src/editor/world_panel.rs
  - viewer/src/editor/system_panel.rs
  - viewer/src/editor/routes_panel.rs
  - viewer/src/editor/file_ops.rs
  - viewer/src/editor/settings_panel.rs
  - viewer/src/editor/toolbar.rs
  - viewer/src/editor/ui_helpers.rs
  - viewer/src/editor/wishes_panel.rs
  - viewer/src/lib.rs
  - viewer/src/main.rs
loc_reviewed: 3210
reviewed_by: agent
health_score: 2
finding_counts: { critical: 0, high: 5, medium: 9, low: 7, nit: 4 }
top_risks:
  - "Architectural: viewer hosts a full sector mutator that bypasses the builder's command bus (F-021-001)"
  - "Determinism: `rand::random::<f64>()` used to seed generation, violating stage-keyed RNG invariant (F-021-002)"
  - "Reachable panics in map_panel hot path via `.expect(\"sector presence checked above\")` (F-021-003)"
  - "Generation panel mutates source-of-truth `state.sector` from preview path with no undo (F-021-006)"
  - "Heavy duplication of panel scaffolding with builder + editor-only path forks (F-021-004)"
---

# Review: viewer editor submodule + viewer crate top-level

## Summary

The viewer crate is documented in [CLAUDE.md](CLAUDE.md) as the **read-only** egui frontend, with the builder being the writer. In fact `viewer/src/editor/` is a fully featured mutating sector editor: it creates, edits, saves projects, runs generation, applies winning seeds, drag-moves systems, edits factions, etc. None of this routes through the builder's `BuilderCommand` bus (§R4). It is a parallel write path with no undo/redo, no derivation cache, no command-history serialization — purely "edit in place". That is the single most important finding in this unit and the source of most secondary findings (duplication of `empty_*` constructors with builder, RNG bypass, no command audit trail). The submodule itself is internally clean — narrow types, mostly idiomatic egui — but its existence violates the documented architecture.

The per-file code quality is otherwise OK. There are a handful of reachable `.expect`/`.unwrap` (one in the hot map render path), determinism violations from `rand::random::<f64>()`, and a noticeable amount of `.to_string()` per-frame chatter in panels that re-collect option lists on every redraw. Tests are essentially absent (one state test).

## Findings

### F-021-001 — [HIGH] [Architecture/Error model] viewer hosts a sector mutator that bypasses the builder command bus
- **Location:** `viewer/src/editor/mod.rs:1-32`; mutation sites e.g. `viewer/src/editor/dialogs.rs:166`, `viewer/src/editor/map_panel.rs:152-256`, `viewer/src/editor/system_panel.rs:140-186`, `viewer/src/editor/factions_panel.rs:279-300`, `viewer/src/editor/wishes_panel.rs:117-136`.
- **Category:** Architecture / Error model / project invariants (§R4)
- **Confidence:** High
- **Blast radius:** Whole editor flow. Every write path here is invisible to the builder's undo/redo, snapshotting, file watcher, and derivation cache.
- **Problem:** [CLAUDE.md](CLAUDE.md) ("Viewer (read-only)") and the workspace table both describe `sectorforge-viewer` as the read-only consumer of generated sectors, with mutations flowing through `BuilderCommand` in `sectorforge-builder`. The `editor/` submodule in `viewer/` is a full write surface: drag-to-move (`map_panel.rs:159-180`), `delete_system` (`system_panel.rs:177-187`), faction add/remove/pin (`factions_panel.rs:281-300`), route create/edit (`routes_panel.rs:118-150`), `set_sector` after `RUN SEARCH` (`wishes_panel.rs:120-124`), `save_project_sector` (`dialogs.rs:213-218`). None of these go through `BuilderCommand`; they mutate `state.sector` directly. The CLAUDE.md hard rule "Mutations in the builder always go through the command bus" is therefore either being end-run in a sibling crate or the rule's scope statement is wrong.
- **Why it matters:** Two divergent edit pathways are now maintained for the same domain. Bugs and data invariants must be re-implemented in both (see F-021-004 below — the editor maintains its own `empty_system`/`empty_world`/`empty_route` constructors). Anyone editing in viewer gets no undo. The viewer can re-`generate(...)` and overwrite `state.sector` outright (`wishes_panel.rs:120-123`), losing user edits silently.
- **Evidence:** `viewer/src/editor/state.rs:170-199` `set_sector` resets selection/dialog/preview but never records anything for undo; no `BuilderCommand` import anywhere under `viewer/src/`.
- **Suggested fix:** Either
  (a) Remove `editor/` from `viewer` and make the read-only viewer launch the builder for edits (smallest change, restores invariant); or
  (b) Promote a `MutationCommand` enum into `gui-core` so the two crates share one command bus; viewer routes its writes through it. This is the only way both `BuilderState` (with derivation cache) and `EditorState` (without) can stay consistent.
  Either way, document in [CLAUDE.md](CLAUDE.md) that the viewer has an editor mode, or remove the editor.
- **Effort:** L (a) or XL (b)
- **Risk of fix:** Medium — touches user-facing flow, but no logic change required to the mutators themselves under (a).

### F-021-002 — [HIGH] [Determinism] `rand::random::<f64>()` used to seed generator from the editor
- **Location:** `viewer/src/editor/generation_panel.rs:30`, `viewer/src/editor/generation_panel.rs:259`
- **Category:** Determinism invariant
- **Confidence:** High
- **Blast radius:** Generation reproducibility. Seeds rolled by the UI are not derivable from a parent stage seed, so a builder session can never be re-run by another machine from a session log.
- **Problem:** [CLAUDE.md](CLAUDE.md) hard rule: "All RNG draws go through `src/model/rng.rs` (stage-keyed via `blake3`). Do not introduce `rand::thread_rng()` or seed from anything outside the stage RNG." `generation_panel.rs:30` does `input.config.generation.seed = f64::to_string(&rand::random::<f64>());` — direct `rand::random` (thread RNG) feeding the generator's seed string. Same on `:259`.
- **Why it matters:** Every "🎲 Randomize seed" / "RE-ROLL (NEW SEED)" produces a value drawn from `thread_rng`, which is not session-reproducible and not stage-keyed. The seed itself is then deterministic downstream, but its origin is not — meaning two reviewers cannot replay each other's session.
- **Evidence:** Direct read.
- **Suggested fix:** Route through a stage RNG. Expose a tiny `model::rng::user_action_seed_string(prev_seed: &str, action_id: &str) -> String` in `sectorforge` (keyed by blake3 of project_id || prev_seed || monotonic counter) and call that from the panel. Alternatively, surface a textbox the user types into and remove the random button entirely.
- **Effort:** S
- **Risk of fix:** Low

### F-021-003 — [HIGH] [Panic] `.expect("sector presence checked above")` in the per-frame map draw
- **Location:** `viewer/src/editor/map_panel.rs:50`, `viewer/src/editor/map_panel.rs:92`
- **Category:** Panics & failure surface (§3.1)
- **Confidence:** High
- **Blast radius:** Hot path — runs every frame the editor's map tab is visible.
- **Problem:** Lines 50–51 do `let sector = state.sector.as_ref().expect("sector presence checked above");`. The "check above" is the `if let` at line 20 that returns early when `state.sector.is_none()`. The current implementation is sound today, but: (1) it asserts a state machine invariant inside the most expensive per-frame path; (2) any future refactor that adds an `await`/non-trivial reentrancy (egui's `show` does invoke arbitrary closures that may touch `state`) would silently break this; (3) `state.sector` could in principle be cleared by a sibling closure inside `SectorView::show` if a future maintainer routes ctx events.
- **Why it matters:** This is exactly the kind of `expect("checked above")` that turns into a CRITICAL crash on regression.
- **Evidence:** Direct read; the call is wrapped in `ui.allocate_new_ui(...)` whose closure has full `&mut Ui` and `&mut state` is not borrowed yet in that scope.
- **Suggested fix:** Re-pattern with `if let`:
  ```rust
  let Some(sector) = state.sector.as_ref() else { return };
  ```
  before the inner block, then use the binding. Drop the `expect`s.
- **Effort:** XS
- **Risk of fix:** Low

### F-021-004 — [HIGH] [Duplication] Editor re-implements `empty_*` constructors that already live (or should live) in `gui-core`/`sectorforge`
- **Location:** `viewer/src/editor/state.rs:245-370` (`empty_sector`, `empty_system`, `empty_world`, `empty_route`, `empty_faction`); compare `viewer/src/app/sector_view.rs:504`, `viewer/src/app/system_view.rs:192` which also call them; the builder has its own equivalents under `builder/src/builder/panels/{system.rs,world.rs,routes.rs,factions.rs}`.
- **Category:** Duplication (§3.7)
- **Confidence:** High
- **Blast radius:** Future schema changes to `GeneratedSystem` / `GeneratedFaction` etc. must be applied in three places to stay in sync; default values for fields (e.g. `GenerationManifest::generator_version = "0.1.0"`) drift.
- **Problem:** `state::empty_sector` hard-codes `generator_name: "sectorforge".into(), generator_version: "0.1.0".into()` (`state.rs:251-252` and again at `:257-258`), `world_type: "dead"` (`:323`), `atmosphere: "none"` (`:324`), etc. These should be `Default` impls on the DTOs or factory methods in the `sectorforge` crate, called from a single place. Right now the viewer editor + viewer's app/sector_view + builder's panels all have their own near-copies.
- **Why it matters:** Direct violation of the §3.7 rubric "shareable abstractions that belong in gui-core". Also: when the user "creates an empty sector" via the editor, the manifest's `generator_version` is **stale** because it's a literal in the editor source, not pulled from `env!("CARGO_PKG_VERSION")` or `sectorforge::version()`.
- **Evidence:** Direct read; matching code in app/sector_view.rs:504 calls into the same `empty_system` so the duplication is already partly recognized.
- **Suggested fix:** Move `empty_sector`/`empty_system`/`empty_world`/`empty_route`/`empty_faction` into `sectorforge::sector_model` as `pub fn empty_*` or `impl Default`, calling `env!("CARGO_PKG_VERSION")` for the manifest's generator_version. Remove `state::empty_*`; have editor + builder both call the canonical version. Lifts roughly 120 LOC out of `state.rs`.
- **Effort:** M
- **Risk of fix:** Low — pure refactor with golden tests as backstop.

### F-021-005 — [HIGH] [Panic] `state.wishes.as_mut().unwrap()` reachable after dialog dismissal
- **Location:** `viewer/src/editor/wishes_panel.rs:30`
- **Category:** Panics & failure surface (§3.1)
- **Confidence:** Medium-High
- **Blast radius:** Crash on wishes tab on an empty project.
- **Problem:** Line 30 does `let wishes = state.wishes.as_mut().unwrap();`. The previous block (`:17-28`) returns early when `state.wishes.is_none()`, *except* if the user clicked "+ CREATE wishes.toml" — in which case `state.wishes = Some(...)` is set and execution continues past the `return`-less block. That path happens to be safe in the current control flow, but only because the `if state.wishes.is_none()` branch unconditionally `return`s after the `vertical_centered` block (`:27`). However, when `state.project_input` is `Some` and `state.wishes` is `None`, the early return at `:27` fires *after* mutating state, so on the next frame `unwrap` is reached. Today that next-frame path sees `Some(_)` and is fine. Brittle: any reorder of the early-return makes this UB-class crash.
- **Why it matters:** Same anti-pattern as F-021-003 — `unwrap` defended by reasoning about UI control flow.
- **Evidence:** Direct read.
- **Suggested fix:**
  ```rust
  let Some(wishes) = state.wishes.as_mut() else { return };
  ```
  Drop the early-return block above and use the let-else directly.
- **Effort:** XS
- **Risk of fix:** Low

### F-021-006 — [MEDIUM] [Correctness] "APPLY WINNING SEED" / "PREVIEW" overwrite `state.sector` silently, losing user edits
- **Location:** `viewer/src/editor/wishes_panel.rs:117-136`
- **Category:** Correctness, error handling
- **Confidence:** High
- **Blast radius:** User who has been editing a sector and then clicks "APPLY WINNING SEED" or "PREVIEW" silently has all their work replaced.
- **Problem:** `apply_seed` block (`:117-125`) calls `sectorforge::generation::generate(input.clone())` and on `Ok(sec)` does `state.sector = Some(sec); state.mark_dirty();`. There is **no confirmation dialog**, and **no preservation** of the existing dirty edits. The "PREVIEW" block (`:127-136`) goes further: it overwrites `state.sector` and explicitly comments "Previewing doesn't mark dirty" — meaning the user can preview, lose their edits, and have no `*` to indicate the sector was replaced. The `Err(...)` branch is silently swallowed in both cases (the `if let Ok(...)` form — also a §3.4 anti-pattern).
- **Why it matters:** Real user-visible data loss.
- **Evidence:** Direct read of `if let Ok(sec) = ...` with no `else`.
- **Suggested fix:** When `state.dirty` is true, gate both buttons through a `Dialog::Message`-style confirmation. Replace `if let Ok(sec)` with explicit match and route `Err(e)` to `state.dialog = Dialog::Message(format!("generation failed: {e}"))`. Consider stashing the existing sector into a single-step undo slot before overwrite.
- **Effort:** S
- **Risk of fix:** Low

### F-021-007 — [MEDIUM] [Concurrency / blocking UI] `wishes_panel::run_search` blocks the UI thread
- **Location:** `viewer/src/editor/wishes_panel.rs:62-75`
- **Category:** Concurrency / responsiveness (§3.5)
- **Confidence:** High
- **Blast radius:** UI freeze for the duration of a search (budget defaults to thousands of candidates).
- **Problem:** `if ui.button("RUN SEARCH").clicked() { match sectorforge::search::run_search(input, wishes) ...}` runs the search synchronously inside an egui repaint. The crate already has `crate::jobs::JobHandle` (see `state.rs:128` and the test scaffolding) — there is precedent for running long work on a worker thread, but this path doesn't use it.
- **Why it matters:** A common-budget search is a multi-second hang of the entire editor.
- **Evidence:** Direct read; compare `state.preview_job` infrastructure that *was* set up for exactly this in `generation_panel.rs`.
- **Suggested fix:** Wrap `run_search` in `crate::jobs::spawn_job` (same pattern as `preview_job` in `state.rs:128`/`:219-235`). Store the handle on `EditorState`, poll in `app/lifecycle.rs`, write the outcome back via `apply_search_outcome(revision, ...)`.
- **Effort:** M
- **Risk of fix:** Low (mirrors the existing preview pattern).

### F-021-008 — [MEDIUM] [Determinism / cleanliness] `coord_lookup: HashMap` in routes/map paths
- **Location:** `viewer/src/editor/routes_panel.rs:36-40`, `viewer/src/editor/map_panel.rs:232-236`
- **Category:** Determinism, idiomatic (§3.7 rubric & CLAUDE.md FxMap rule)
- **Confidence:** Medium
- **Blast radius:** None for output correctness (these maps are only used for `.get()` lookups). But this is the exact pattern that grows into an iteration-ordering bug.
- **Problem:** `std::collections::HashMap<SystemId, HexCoord>` is used for a one-shot lookup of N items where N is system count (≤1000 in practice). HashMap with random-seeded SipHash is overhead here; BTreeMap or even a `Vec<(id, coord)>` with `.iter().find()` would be deterministic and faster. The CLAUDE.md "Never iterate FxMap for output" rule generalizes — using HashMap in code touching the sector domain invites later bugs.
- **Why it matters:** A future maintainer who adds `.iter()` over `coord_lookup` (e.g. to emit a debug overlay) immediately gets non-deterministic order. Better to pick the right structure now.
- **Evidence:** Direct read.
- **Suggested fix:** Replace `HashMap` with `BTreeMap` or build a small `Vec<(SystemId, HexCoord)>` and `.iter().find(...)`. There are only ≤1000 systems; constant factors are irrelevant.
- **Effort:** XS
- **Risk of fix:** Low

### F-021-009 — [MEDIUM] [Error handling] `if let Ok(...)` swallows errors in load_project / load_wishes
- **Location:** `viewer/src/editor/state.rs:191-197`, `viewer/src/editor/file_ops.rs:65-69`
- **Category:** Error handling (§3.4)
- **Confidence:** High
- **Blast radius:** Silently broken projects load with `wishes = None` / `project_input = None` and the user has no clue why "Generation requires project context" appears.
- **Problem:**
  ```rust
  if let Ok(w) = sectorforge::search::load_wishes(&wishes_path) { self.wishes = Some(w); }
  ```
  (state.rs:194). And in file_ops.rs:65-69:
  ```rust
  if let Ok(utf8_root) = camino::Utf8PathBuf::from_path_buf(project_root) {
      if let Ok(pi) = sectorforge::input::load_project(&utf8_root) { input = Some(pi); }
  }
  ```
  Both errors are silently dropped. Symptom: user sees `dim("Generation requires project context.")` (`generation_panel.rs:14-21`) with no idea their `sectorforge.toml` failed to parse.
- **Why it matters:** Direct §3.4 violation; debuggability cost is high.
- **Evidence:** Direct read.
- **Suggested fix:** Surface errors via `state.dialog = Dialog::Message(format!("...: {e}"))` or log them. At minimum:
  ```rust
  match sectorforge::search::load_wishes(&wishes_path) {
      Ok(w) => self.wishes = Some(w),
      Err(e) => eprintln!("warn: failed to load {}: {e}", wishes_path),
  }
  ```
- **Effort:** XS
- **Risk of fix:** Low

### F-021-010 — [MEDIUM] [Performance] Per-frame allocation of system/world/option vectors in factions and routes panels
- **Location:** `viewer/src/editor/factions_panel.rs:82-97`, `viewer/src/editor/factions_panel.rs:25-30`, `viewer/src/editor/routes_panel.rs:22-40`
- **Category:** Performance (§3.6) — per-frame GUI path
- **Confidence:** High
- **Blast radius:** Allocator pressure proportional to system_count × frame rate while the panel is visible. For a 100-system sector at 60 fps that's `~600 Vec allocations/sec` plus `~6000 String clones/sec` (each `s.id.clone()` and `s.name.to_string()`).
- **Problem:** Every frame, the panels rebuild `system_ids: Vec<SystemId>`, `world_ids: Vec<WorldId>`, `system_labels: Vec<(SystemId,String)>`, `system_kv: Vec<(&str,&str)>`, `world_refs: Vec<&str>`, and (in factions panel) two `BTreeSet<String>` of kinds/dispositions. None of these are cached. Factions panel additionally sorts the `visible` Vec on every frame (`factions_panel.rs:116-140`).
- **Why it matters:** The §3.6 rubric calls out "per-frame heap traffic in GUI render paths". This is a clean case.
- **Evidence:** Direct read.
- **Suggested fix:** Move these caches onto `EditorState` as `Vec`s rebuilt on `mark_dirty` (or pass them as derived state). Use `&[T]` references in helper signatures. For factions, cache `visible_order: Vec<usize>` and a generation counter; rebuild only when sector or filter/sort/pin state changes. Easy 5–10× allocation reduction on this tab.
- **Effort:** S
- **Risk of fix:** Low

### F-021-011 — [MEDIUM] [Correctness] `route.id` recomputed on every dirty change to *every* route
- **Location:** `viewer/src/editor/routes_panel.rs:145-150`
- **Category:** Correctness — silent duplicate IDs / wasted work
- **Confidence:** High
- **Blast radius:** Routes can have stable IDs replaced when an unrelated field changed (e.g. type/stability/distance only). If two routes happen to share the same `(from, to)`, their IDs collide silently.
- **Problem:**
  ```rust
  if dirty {
      for r in &mut sector.routes {
          r.id = sectorforge::ids::route_id(&r.from_system_id, &r.to_system_id);
      }
      ...
  }
  ```
  This walks every route on any change to any route — even if `dirty` was set by a DragValue on `distance`. The id rebuild is unnecessary and worse, the design forces IDs to be a pure function of `(from, to)`. Two routes with the same `(from, to)` (e.g. one stable warp lane + one secret passage between the same two stars) get the same id; nothing here prevents that.
- **Why it matters:** Silent ID collision violates the determinism contract (id_history can't track distinct routes).
- **Evidence:** Direct read.
- **Suggested fix:** Recompute id only for the rows where `from`/`to` actually changed. Detect collisions and either reject the change or suffix the id (`_2`, `_3`). Lift route-id collision policy to a shared helper in `sectorforge::ids`.
- **Effort:** S
- **Risk of fix:** Low

### F-021-012 — [MEDIUM] [Correctness] `place_sys_kind` combo loses match on unknown values
- **Location:** `viewer/src/editor/dialogs.rs:254-272`
- **Category:** Correctness / match exhaustiveness (§3.7)
- **Confidence:** High
- **Blast radius:** UI silently falls back to `Star` if a `SystemKind` variant was added to `sectorforge::sector_model::SystemKind` but not to this `kinds` array.
- **Problem:** The `kinds` array enumerates five SystemKind variants by stringly-typed Debug names. The match `match kind_s.as_str() { "Star" => ..., _ => SystemKind::Star }` silently coerces unknown / future variants to Star. The combo box's `selected_text` is constructed via `format!("{kind:?}")` — so adding a new variant would simply not appear in the combo and would round-trip through "_" => Star, losing the original selection.
- **Why it matters:** Brittle in a domain where new SystemKinds are plausible (Nebula, RogueStar, etc.).
- **Evidence:** Direct read.
- **Suggested fix:** Define `SystemKind::ALL: &[SystemKind]` and `SystemKind::as_key()/from_key()` in `sectorforge::sector_model`. Iterate `SystemKind::ALL` in the combo; round-trip via `from_key`. `#[non_exhaustive]` the enum and let the compiler force this site to update.
- **Effort:** S
- **Risk of fix:** Low

### F-021-013 — [MEDIUM] [Correctness] orbital `i32 as u8` truncation on out-of-range input
- **Location:** `viewer/src/editor/world_panel.rs:47-54`
- **Category:** Integer overflow / `as` truncation (§3.7)
- **Confidence:** Medium
- **Blast radius:** None today because `DragValue::range(1..=99)` clamps, but the cast itself is `as u8` which would silently truncate larger values if range is ever widened.
- **Problem:**
  ```rust
  let mut orbit_i = i32::from(w.orbit);
  if ui.add(egui::DragValue::new(&mut orbit_i).range(1..=99)).changed() {
      w.orbit = orbit_i.clamp(1, 99) as u8;
      ...
  }
  ```
  The `clamp` is correct, the `as u8` is defensively redundant — but if `range(...)` is dropped or changed, the truncation becomes silent. §3.7 says "prefer `TryFrom`".
- **Why it matters:** §3.7 hygiene; a future widen-the-range PR makes this UB-class silent truncation.
- **Evidence:** Direct read.
- **Suggested fix:** `w.orbit = u8::try_from(orbit_i.clamp(1, 99)).unwrap_or(1);` or use `egui::DragValue::new(&mut w.orbit)` directly with `u8` and let egui clamp.
- **Effort:** XS
- **Risk of fix:** Low

### F-021-014 — [LOW] [Idiomatic] `if let Some(_)` blocks where `let ... else` would flatten control flow
- **Location:** `viewer/src/editor/dialogs.rs:9-332` (multiple), `viewer/src/editor/map_panel.rs:152-256` (multiple)
- **Category:** Idiomatic (§3.7)
- **Confidence:** High
- **Blast radius:** Readability only.
- **Problem:** Many of the mutate-after-render blocks use `if let Some(sector) = state.sector.as_mut() { ... }` which leaves the rest of the function at the same indentation and obscures invariants. After the early-return `if let Some(...) = &state.sector` at the top of `show_map`, the later `state.sector.as_mut()` calls are essentially safe-but-relitigated.
- **Why it matters:** Combined with the F-021-003 expect-pattern, these blocks make it easy to lose track of where `sector` is guaranteed.
- **Evidence:** Direct read.
- **Suggested fix:** Hoist `state.sector` borrowing once via `let Some(sector) = state.sector.as_mut() else { return };` at the top of each mutation block, then operate on `sector` directly. Combine with F-021-003.
- **Effort:** S
- **Risk of fix:** Low

### F-021-015 — [LOW] [Idiomatic] String-typed enum lookups for RouteType/RouteStability
- **Location:** `viewer/src/editor/routes_panel.rs:86-102`, `viewer/src/editor/routes_panel.rs:157-182`
- **Category:** Idiomatic (§3.7)
- **Confidence:** High
- **Blast radius:** Maintenance.
- **Problem:** `route_type_str`/`route_stab_str` convert enum → static string, and `route_type_from_str`/`route_stab_from_str` convert string → enum. `RouteType` already exposes `.key()` / `RouteType::from_key`. `RouteStability` doesn't; the match in `route_stab_from_str` could miss future variants silently (returns `None`, which the panel ignores at `:99-100`).
- **Why it matters:** Adding a new `RouteStability` variant requires editing this file with no compiler help.
- **Evidence:** Direct read; compare `RouteType::from_key` already used at `:162`.
- **Suggested fix:** Add `RouteStability::key()` and `RouteStability::from_key()` to `sectorforge::sector_model`. Replace both helpers with the canonical conversions. `#[non_exhaustive]` both enums.
- **Effort:** S
- **Risk of fix:** Low

### F-021-016 — [LOW] [Ownership] Repeated `.to_string()` of `EcoString`/`String` fields per frame
- **Location:** `viewer/src/editor/system_panel.rs:36`, `viewer/src/editor/factions_panel.rs:198`, `viewer/src/editor/settings_panel.rs:18,26,34`, etc.
- **Category:** Ownership / cloning (§3.3)
- **Confidence:** High
- **Blast radius:** Per-frame allocator pressure.
- **Problem:** Each editable field does `let mut name = fac.name.to_string()`, hands it to `text_field`, then on `.changed()` does `fac.name = name.into()`. So every frame allocates a fresh String per editable text field (system name, faction name+kind+disposition, settings.id+title+seed, etc.), only writing back if it changed. With ~10 editable fields visible at once that's ~600 String allocations/sec at 60 fps.
- **Why it matters:** Same anti-pattern as F-021-010 but field-level. Compounds.
- **Evidence:** Direct read.
- **Suggested fix:** Use `text_field_id` (already in `ui_helpers.rs:81`) on the typed `EcoString`/`SystemId` etc. directly, by adding `From<String>` and `AsRef<str>` impls where missing. Or use `egui::TextEdit::singleline(&mut buf)` with `buf` owned by the panel for the duration of the input session via egui's `Memory` rather than rebuilt every frame.
- **Effort:** M
- **Risk of fix:** Low — `text_field_id` already exists; the work is wiring up the trait bounds.
- **Note:** `text_field_id` already calls `value.as_ref().to_string()` and writes back on change — same allocation profile. A true fix would buffer in egui memory per widget id.

### F-021-017 — [LOW] [Idiomatic] `system_kv` & `opt_kv` rebuild every frame instead of being passed as derived state
- **Location:** `viewer/src/editor/factions_panel.rs:88-97`, `viewer/src/editor/routes_panel.rs:26-34`
- **Category:** Idiomatic / performance overlap with F-021-010
- **Confidence:** High
- **Suggested fix:** Lift to `EditorState::derived` recomputed on `mark_dirty`. Avoids reallocation and avoids the `system_labels` intermediate Vec entirely.
- **Effort:** S
- **Risk of fix:** Low

### F-021-018 — [LOW] [Idiomatic] `kinds.iter()` over `BTreeSet<String>` is OK but the BTreeSet is rebuilt per frame
- **Location:** `viewer/src/editor/factions_panel.rs:25-30`
- **Category:** Performance (overlap with F-021-010)
- **Confidence:** High
- **Problem:** Two `BTreeSet<String>` collections built from `sector.factions.iter()` every frame just to populate two filter dropdowns. The set discriminant rarely changes (only when a faction is added/edited).
- **Suggested fix:** Cache derived `(kinds, dispositions)` on `EditorState`, invalidate on `mark_dirty`. Or use a `BTreeSet<&str>` with refs into the sector to avoid the per-frame `.to_string()`.
- **Effort:** S
- **Risk of fix:** Low

### F-021-019 — [LOW] [Testing] One inline test in 3210 LOC
- **Location:** `viewer/src/editor/state.rs:372-418` is the only `#[cfg(test)]` block in the entire editor submodule.
- **Category:** Testing (§3.10)
- **Confidence:** High
- **Blast radius:** No regression net for any of the mutation logic above.
- **Suggested fix:** Add unit tests for:
  - `editor/state::next_system_index` (handles empty + nonempty)
  - `editor/file_ops::save_project_sector` for the invalid-name path (already produces a typed error — easy to assert with `tempfile`)
  - Round-trip `set_sector → mark_dirty → save_project_sector(tmp) → load_project_sector(tmp)` equivalence
- **Effort:** M
- **Risk of fix:** Low

### F-021-020 — [LOW] [Error model] `editor/file_ops` returns relative paths via `EXAMPLES_DIR` constant
- **Location:** `viewer/src/editor/file_ops.rs:18-47`
- **Category:** Correctness / portability
- **Confidence:** High
- **Blast radius:** Anyone launching the viewer from outside the repo root sees an empty project list and silent save failures.
- **Problem:** `const EXAMPLES_DIR: &str = "examples";` is a relative path, so `list_projects` reads CWD-relative. `save_project_sector` writes CWD-relative. The viewer binary doesn't `chdir` to the workspace root in `main.rs`. Symptom: install the binary, run it from `~`, click OPEN → empty list, no diagnostic.
- **Why it matters:** Real-world deployment broken; silent.
- **Evidence:** Direct read of file_ops.rs:18 and main.rs (no chdir).
- **Suggested fix:** Resolve `examples/` relative to a discoverable project root (walk up from `std::env::current_exe()`, or accept `--projects-dir` CLI flag, or fall back to `$HOME/.local/share/sectorforge/projects`). Emit a diagnostic in `list_projects` when the directory doesn't exist instead of returning `Vec::new()`.
- **Effort:** S
- **Risk of fix:** Low

### F-021-021 — [NIT] [Docs] `lib.rs` module-doc mentions only the viewer
- **Location:** `viewer/src/lib.rs:1-3`
- **Problem:** The crate-level `//!` doc says "GUI module: egui-based viewer for generated sectors" — does not mention the editor submodule. With the editor doing the lion's share of behavioural code, this is misleading.
- **Suggested fix:** Add a sentence: "Also hosts an experimental in-tree editor (`editor`) — see §F-021-001 for the architectural note."
- **Effort:** XS
- **Risk of fix:** Nil

### F-021-022 — [NIT] [Idiomatic] `f64::to_string(&rand::random::<f64>())` is awkward
- **Location:** `viewer/src/editor/generation_panel.rs:30`, `viewer/src/editor/generation_panel.rs:259`
- **Problem:** `f64::to_string(&x)` is the same as `x.to_string()` but in fully-qualified form, which is unusual. (Setting aside F-021-002 which is the real issue.)
- **Suggested fix:** `format!("{}", rand_value)` or simply `rand_value.to_string()` after addressing F-021-002.
- **Effort:** XS
- **Risk of fix:** Nil

### F-021-023 — [NIT] [Docs] `main.rs` `resolve_project_dir` is a one-line wrapper
- **Location:** `viewer/src/main.rs:107-112`
- **Problem:** `fn resolve_project_dir(cli: &Cli) -> Option<Utf8PathBuf> { if let Some(dir) = &cli.project { return Some(dir.clone()); } None }` is the same as `cli.project.clone()`.
- **Suggested fix:** Inline as `cli.project.clone()`, delete the helper. Saves five LOC.
- **Effort:** XS
- **Risk of fix:** Nil

### F-021-024 — [NIT] [Idiomatic] `if !cancel { state.dialog = Dialog::...rebuild... }` pattern duplicated across every dialog
- **Location:** `viewer/src/editor/dialogs.rs:85-87`, `:168-177`, `:230-232`, `:322-329`
- **Problem:** Every dialog variant manually re-packs its fields back into the same enum on the non-confirm/non-cancel path. The `std::mem::replace` at the top + manual repack at the bottom is tedious and easy to drop a field on add (which happened: `irregular_dimensions` was added to `NewSector` and is correctly repacked, but the pattern is fragile).
- **Suggested fix:** Either (a) hold dialog state in dedicated `Option<NewSectorState>` etc. fields on `EditorState` instead of inside the `Dialog` enum, eliminating the unpack/repack; or (b) use `&mut state.dialog` with a `let Dialog::NewSector { .. } = &mut state.dialog else { ... }` pattern.
- **Effort:** S
- **Risk of fix:** Low

## Rubric coverage

- **3.1 Panics & failure surface:** F-021-003 (HIGH), F-021-005 (HIGH), F-021-013 (cast). No `unreachable!`/`todo!` found.
- **3.2 unsafe & soundness:** No findings. Zero `unsafe` in scope.
- **3.3 Ownership/borrowing/cloning:** F-021-016 (LOW), F-021-014 (LOW). No `Arc<Mutex<>>`/`Rc<RefCell<>>` misuse in `state.rs` — straightforward owned struct, which is the right shape.
- **3.4 Error handling:** F-021-006 (MED), F-021-009 (MED), F-021-001 (HIGH cross-cut). Special focus answered: yes, the editor is a real mutator; no, it doesn't use the command bus.
- **3.5 Concurrency & async:** F-021-007 (MED). The `JobHandle` infrastructure is there but unused for search.
- **3.6 Performance:** F-021-010 (MED), F-021-016/017/018 (LOW). All in per-frame GUI render paths — confirmed hot.
- **3.7 Idiomatic/API design:** F-021-004 (HIGH), F-021-012 (MED), F-021-015 (LOW), F-021-014 (LOW), F-021-024 (NIT).
- **3.8 Deps/Cargo hygiene:** No unused imports flagged in this unit. `rand` dependency in viewer is only justified by F-021-002 — if that finding is fixed by routing through `sectorforge`'s stage RNG, `rand` can be dropped from `viewer/Cargo.toml`. Flag for X06.
- **3.9 Memory & resource management:** No findings. `Drop` not implemented anywhere here; `EditorState` is a plain struct; no growing caches.
- **3.10 Testing & verification:** F-021-019 (LOW).
- **3.11 Documentation & maintainability:** F-021-021 (NIT), F-021-023 (NIT). Module docs are uniformly present at the top of each panel file, which is good.

## Special focus answers

- **Editor is a real mutator** (F-021-001 evidence). It writes through `state.sector.as_mut()` in every panel and persists via `editor::file_ops::save_project_sector`. It does **not** import or use `BuilderCommand` — undo/redo is not supported and there is no command audit. This is a documented architectural rule violation (§R4) or a documentation gap, depending on intent.
- **Shareable abstractions belong in gui-core:** F-021-004 (constructors), F-021-010 + F-021-017 (per-frame system/world option lists — both editor and builder need this), and the dialog repack pattern (F-021-024). The faction chip drawing already lives in gui-core (`palette::draw_faction_chip` is reused — good). The state-machine "dialog with repack" pattern could become a `gui_core::dialog!` macro or a `DialogState<T>` newtype.
- **dialogs.rs reachable unwraps:** None of the panic patterns in `dialogs.rs` are reachable on user dismissal — the file uses pattern-matched moves throughout (no `unwrap`). Confirmed clean on that axis. (`state.rs:30` has `state.wishes.as_mut().unwrap()` — addressed as F-021-005.)
- **state.rs ownership:** Clean. No `Arc<Mutex<...>>` outside the `JobHandle` which legitimately needs it (cross-thread cancel flag + progress). No `Rc<RefCell<>>`. `BTreeSet<FactionId>` used for `faction_pinned` — correct deterministic choice.

## Summary of suggested fixes

- F-021-001 — HIGH — Remove `editor/` from viewer OR route writes through a shared command bus — L/Medium
- F-021-002 — HIGH — Replace `rand::random::<f64>()` seed generation with a stage RNG helper — S/Low
- F-021-003 — HIGH — Replace `.expect("sector presence checked above")` with `let-else` in `map_panel.rs` — XS/Low
- F-021-004 — HIGH — Move `empty_*` constructors into `sectorforge::sector_model`; pull `generator_version` from `CARGO_PKG_VERSION` — M/Low
- F-021-005 — HIGH — Replace `state.wishes.as_mut().unwrap()` with `let-else` — XS/Low
- F-021-006 — MED — Confirm-before-overwrite on APPLY/PREVIEW; surface generation errors — S/Low
- F-021-007 — MED — Run `run_search` via `crate::jobs::spawn_job` like preview — M/Low
- F-021-008 — MED — Replace `HashMap<SystemId, HexCoord>` with `BTreeMap` (or small Vec scan) — XS/Low
- F-021-009 — MED — Surface load_wishes / load_project errors instead of silent drop — XS/Low
- F-021-010 — MED — Cache per-frame system/world option vectors on `EditorState` — S/Low
- F-021-011 — MED — Recompute `route.id` only for rows where from/to changed; collision policy — S/Low
- F-021-012 — MED — `SystemKind::ALL` + `as_key/from_key`; mark enum `#[non_exhaustive]` — S/Low
- F-021-013 — MED — Use `u8::try_from(clamp)` or bind DragValue directly to `u8` — XS/Low
- F-021-014 — LOW — Replace `if let Some(sector) = state.sector.as_mut()` blocks with `let-else` — S/Low
- F-021-015 — LOW — Add `RouteStability::key/from_key`; drop hand-rolled match — S/Low
- F-021-016 — LOW — Wire `text_field_id` (or egui-memory-backed buffers) for typed string fields — M/Low
- F-021-017 — LOW — Lift `system_kv` / `opt_kv` to derived state — S/Low
- F-021-018 — LOW — Cache filter-option `BTreeSet`s on `EditorState`; rebuild on dirty — S/Low
- F-021-019 — LOW — Add unit tests for file_ops, next_system_index, save/load round-trip — M/Low
- F-021-020 — LOW — Resolve `examples/` relative to discoverable root or `--projects-dir` — S/Low
- F-021-021 — NIT — Update `lib.rs` module doc to mention editor — XS/Nil
- F-021-022 — NIT — Use `x.to_string()` instead of `f64::to_string(&x)` — XS/Nil
- F-021-023 — NIT — Inline `resolve_project_dir` as `cli.project.clone()` — XS/Nil
- F-021-024 — NIT — Hoist dialog state out of the `Dialog` enum (or `&mut state.dialog` pattern) — S/Low
