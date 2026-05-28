---
unit_id: U015
crate: sectorforge-builder
paths:
  - builder/src/builder/state/mod.rs
  - builder/src/builder/state/types.rs
  - builder/src/builder/state/derivations.rs
  - builder/src/builder/state/tests.rs
  - builder/src/builder/state/generation_ops.rs
  - builder/src/builder/state/nav.rs
  - builder/src/builder/state/regions_ops.rs
  - builder/src/builder/state/selection.rs
  - builder/src/builder/state/undo.rs
  - builder/src/builder/command.rs
  - builder/src/builder/project_io.rs
  - builder/src/builder/session.rs
  - builder/src/builder/workspace.rs
  - builder/src/builder/preview.rs
  - builder/src/app.rs
  - builder/src/lib.rs
  - builder/src/main.rs
loc_reviewed: 6361
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 6, medium: 13, low: 7, nit: 4 }
top_risks:
  - "Sector-mutating ops bypass the command bus and break undo/redo (F-015-001)"
  - "Every `BuilderState` field is `pub`; panels write directly to undo-bearing fields (F-015-002)"
  - "`trigger_auto_save` swallows serialisation and IO errors silently (F-015-005)"
  - "`reload_catalog` swallows TOML parse errors on watcher reloads (F-015-006)"
  - "Session round-trip duplicates 100+ field defaults — guaranteed drift (F-015-007)"
  - "`atomic_write` leaks `.tmp.<pid>` siblings on rename failure (F-015-008)"
---

# Review: builder core wiring (state, command bus, project IO, app lifecycle)

## Summary

The command bus is well-structured: every variant of `BuilderCommand` has matched
`apply`/`revert` halves and a determinism test, and the `run` path correctly
invalidates the index, derivation cache, dirty flag, and validation timer in one
place. The weak spots are around the boundary the rest of the code uses to talk
to that bus. Several `BuilderState` methods (`regenerate_world`,
`regenerate_partial`, `apply_preview`, `reroll_seed`, and every method in
`regions_ops.rs`) mutate `self.sector` directly without minting a command,
violating the CLAUDE.md R4 invariant. The entire `BuilderState` is a 100+-field
flat record with every field `pub`, so panels routinely set `state.dirty`,
`state.invariant_report`, and even `state.sector` from outside the bus. Project
IO is unwrap-free and atomic, but quietly swallows errors on watcher reloads
and on auto-save. `session.rs::into_state` duplicates the entire `new_blank`
default block, which has already drifted in subtle ways and will keep drifting.

## Findings

### F-015-001 — [HIGH] [Correctness / CLAUDE.md R4] Sector-mutating ops bypass the command bus
- **Location:**
  - `builder/src/builder/state/generation_ops.rs:102-158` (`regenerate_world`)
  - `builder/src/builder/state/generation_ops.rs:181-217` (`apply_preview`)
  - `builder/src/builder/state/generation_ops.rs:225-272` (`regenerate_partial`)
  - `builder/src/builder/state/generation_ops.rs:164-175` (`reroll_seed` — mutates `self.config.generation.seed`)
  - `builder/src/builder/state/regions_ops.rs:11-105` (entire file: `add_region`, `remove_region`, `paint_region_hex`, `erase_region_hex`, `update_region`)
- **Category:** Project invariants (R4 — Builder mutations go through the command bus)
- **Confidence:** High
- **Blast radius:** Every undo/redo path. After a `regenerate_world`,
  `apply_preview`, partial regen, or any region paint, the user presses Ctrl+Z
  and the editor reverts the previous structural command instead, silently
  dropping the user's intervening edits from the undo history. `apply_preview`
  swaps the whole sector — undo will not put it back.
- **Problem:** CLAUDE.md says "Mutations in the builder always go through the
  command bus. Call `state.run(BuilderCommand::...)`." None of the methods above
  do; they mutate `self.sector` and stamp `self.dirty = true` /
  `self.mark_validation_dirty()` by hand. `regions_ops.rs:1-4` has a comment
  acknowledging this ("don't go through the command bus per §D3") but the
  CLAUDE.md rule is stricter than §D3 and is the one that governs after the spec.
- **Why it matters:** Silent loss of user work on Ctrl+Z. Also breaks
  `command_log_determinism_blake3` style guarantees: replaying a session won't
  reproduce the same sector because preview/region/partial-regen work isn't in
  the log.
- **Evidence:** `apply_preview` at `generation_ops.rs:209` does
  `self.sector = preview_sector;` and never touches `self.command_log`.
  `paint_region_hex` at `regions_ops.rs:42-54` calls `self.sector.add_region_hex(...)`
  directly. `regenerate_world` mutates `self.sector.systems[sys_idx].worlds[w_idx]`
  in place at `generation_ops.rs:149-151`.
- **Suggested fix:** Add `BuilderCommand` variants for each: `ApplyPreview {
  before: Box<GeneratedSector>, after: Box<GeneratedSector> }`,
  `RegeneratePartial { before: Vec<GeneratedSystem>, after: Vec<GeneratedSystem> }`,
  `RegenerateWorld { world: WorldId, before: Box<GeneratedWorld>, after: Box<GeneratedWorld> }`,
  `RerollSeed { before: String, after: String }`, and per-region commands
  (`AddRegion`, `RemoveRegion`, `PaintRegionHex`, `EraseRegionHex`, `UpdateRegion`).
  Run each through `state.run(...)`. Where the before/after payload is "the
  whole sector" (preview apply), accept that one command is large rather than
  losing undoability.
- **Effort:** L
- **Risk of fix:** Medium — needs careful before/after capture and tests.

### F-015-002 — [HIGH] [API design / §3.7] Every `BuilderState` field is `pub` — panels write the bus's bookkeeping
- **Location:** `builder/src/builder/state/mod.rs:76-518` (whole struct definition)
- **Category:** API surface / encapsulation
- **Confidence:** High
- **Blast radius:** Cross-cutting. `git grep -n 'state\.\(sector\|command_log\|command_cursor\|snapshots\|index\|dirty\|invariant_report\|validation_report\|derivation_cache\) *=' builder/src/builder/panels/` returns dozens of hits in `relations.rs`, `economy.rs`, `regions.rs`, `subsectors.rs`, `control.rs`, `system.rs`, `personae.rs`, `missions.rs`, `hooks.rs`. Notable cases: `panels/relations.rs:983` does `state.sector = GeneratedSector::empty(...)` (in test code, but the field allows it from non-test panels too); `panels/regions.rs:370` directly recomputes invariants outside the bus.
- **Problem:** The CLAUDE.md R4 invariant ("never write directly to
  `BuilderState` fields from inside a panel — that breaks undo/redo") cannot be
  enforced by the compiler because `pub sector`, `pub command_log`,
  `pub command_cursor`, `pub snapshots`, `pub dirty`, `pub invariant_report`,
  `pub derivation_cache` are all writable from anywhere in the crate. There is
  no `pub(crate)` to keep panels honest.
- **Why it matters:** New panels naturally do `state.dirty = true` instead of
  routing through `run`, which breaks the invariant chain (no invariant
  recheck, no validation re-arm, no auto-save). The current panels already do
  this in 20+ places.
- **Suggested fix:** Make the undo-bearing fields `pub(crate)` and expose
  immutable getters (`sector(&self) -> &GeneratedSector`, `command_log(&self) -> &[BuilderCommand]`,
  `command_cursor(&self) -> usize`, `is_dirty(&self) -> bool`,
  `derivation_cache(&self) -> &DerivationCache`, etc.). The display-only
  bookkeeping (selection ids, scratch ui state, `validation_report`) can stay
  `pub` if needed but the structural fields should not be. Apply a sweep over
  panels in a follow-up unit (out of U015 scope).
- **Effort:** M
- **Risk of fix:** Medium — every panel that touches one of these fields will
  need a small refactor; the panel-implementer agent can do it mechanically.

### F-015-003 — [HIGH] [Correctness] `recompute_*` overwrite serialised sector data outside the bus
- **Location:**
  - `builder/src/builder/state/derivations.rs:36-160` (`recompute_economy`
    writes `self.sector.economy = Arc::new(report)` at line 150 and may run
    `apply_stability_nudge(&report, &mut self.sector)` at line 153)
  - `builder/src/builder/state/derivations.rs:171-179` (`recompute_relations`
    writes `self.sector.relations = Arc::new(matrix)`)
  - `builder/src/builder/state/derivations.rs:189-208` (`recompute_chronicle`
    writes `self.sector.chronicle = report`)
- **Category:** Project invariants (R4) / data loss
- **Confidence:** High
- **Blast radius:** Every auto-recompute trigger (relations, economy,
  chronicle) — these fields are part of the JSON-serialised `GeneratedSector`
  and are written to `out/sector.json` by `trigger_auto_save`. Undo cannot
  restore them.
- **Problem:** These three fields live inside `GeneratedSector` and round-trip
  through `serde_json::to_string_pretty(&state.sector)` in `save_project_as`
  (line 616). The functions stamp `self.dirty = true` and trigger auto-save,
  but they push *no* `BuilderCommand`, so undo doesn't roll them back. After a
  manual relations edit + auto-recompute, undoing the manual edit leaves the
  matrix derived from the post-edit world.
- **Why it matters:** Same R4 violation as F-015-001 but for derivations rather
  than structural edits.
- **Suggested fix:** Either (a) treat the report Arcs as cached derived state
  not part of the truth (i.e. recompute lazily from `sector.systems +
  data_catalogs` at render time, never persisted), or (b) snapshot
  `before`/`after` Arcs into a `Recompute{Economy,Relations,Chronicle}` command
  and route through `run`. (a) is the cleaner long-term answer.
- **Effort:** M
- **Risk of fix:** Medium — touches save/load round-trip; needs a golden test
  refresh.

### F-015-004 — [HIGH] [Correctness] `app.rs::pump_active_state` clones modal then drops every dialog except 4
- **Location:** `builder/src/app.rs:118-157` (`show_modal`)
- **Category:** Correctness
- **Confidence:** High
- **Blast radius:** Any modal kind not in the match arm
  (`SaveAs`, `PlaceSystem`, `ConfirmRevertSnapshot`, `NewFromPreset`) is
  silently filtered out at line 132 by `_ => return;` — these get *no* outer
  window and rely on panels rendering them inline. If any panel forgets to
  surface its panel-managed modal, the modal is invisible but `state.modal`
  stays `Some(...)`, blocking subsequent dialogs (the watcher's
  `ConflictResolver` skips setting `modal` if one is already armed —
  `project_io.rs:807` — so an unrenderable `SaveAs` would mute all watcher
  conflict events too).
- **Problem:** The early `return` is silent. There's no debug assert that the
  current modal is owned by an active panel.
- **Why it matters:** Subtle UX dead-lock if a panel is closed while it owns a
  modal.
- **Suggested fix:** Either route every modal through `app.rs` with explicit
  arms, or document the contract on `ModalKind`, and add a debug-only assertion
  that "panel-owned" modals can only be set while the corresponding tab is
  active. Long-term: split `ModalKind` into two enums — host-managed and
  panel-managed — so the type system enforces ownership.
- **Effort:** S
- **Risk of fix:** Low

### F-015-005 — [HIGH] [Error model / §3.4] `trigger_auto_save` swallows errors silently
- **Location:** `builder/src/builder/state/undo.rs:68-78`
- **Category:** Error handling
- **Confidence:** High
- **Blast radius:** Every successful `run`/`undo`/`redo`/`revert_to_snapshot`
  call. A serialisation failure (`serde_json::to_string_pretty`) or write
  failure (`fs::write`) is silently dropped; `state.dirty` stays `true` so the
  next attempt retries, but the user has no visibility that the save loop is
  failing. On a full disk or a path that has become read-only mid-session,
  every Ctrl+Z silently fails to persist.
- **Problem:** Two `let Ok(_) = ... else` style early returns that throw away
  both the error and the user's chance to react.
- **Why it matters:** Data-loss adjacent: the on-disk file diverges from the
  in-memory edits without notice.
- **Suggested fix:** Capture the last auto-save error on `BuilderState` (e.g.
  `pub last_auto_save_error: Option<String>`), and surface it on the status
  bar. Bump the severity of repeated failures.
  ```rust
  pub fn trigger_auto_save(&mut self) {
      let Some(path) = self.auto_save_path.as_ref() else { return; };
      match serde_json::to_string_pretty(&self.sector) {
          Ok(text) => match std::fs::write(Path::new(path.as_std_path()), text) {
              Ok(()) => { self.dirty = false; self.last_auto_save_error = None; }
              Err(e) => self.last_auto_save_error = Some(format!("io: {e}")),
          },
          Err(e) => self.last_auto_save_error = Some(format!("serde: {e}")),
      }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-015-006 — [HIGH] [Error model / §3.4] `reload_catalog` swallows TOML parse errors on every catalog
- **Location:** `builder/src/builder/project_io.rs:832-924`
- **Category:** Error handling
- **Confidence:** High
- **Blast radius:** Every external edit reload path. If the user breaks
  `factions.toml` in an external editor, the watcher fires, `drain_watcher_events`
  reads the file, and `reload_catalog` silently consumes the failure with
  `if let Ok(file) = toml::from_str::<FactionsFile>(text)`. The in-memory
  catalog stays at the previous parse; the next save reverts the disk file to
  the in-memory version, *silently overwriting the user's external edit*.
- **Problem:** 14 of 14 catalogs in this function use `if let Ok(_) = ...
  return;` — there is no fallback path that raises `ModalKind::Message` or
  re-arms `ConflictResolver` when parsing fails.
- **Why it matters:** Data loss. The user's intent (external edit) is silently
  reverted.
- **Suggested fix:** Change the per-catalog stanzas to surface the error as a
  modal:
  ```rust
  match toml::from_str::<FactionsFile>(text) {
      Ok(file) => state.data_catalogs.factions = Some(file),
      Err(e) => state.modal = Some(ModalKind::Message(format!(
          "Failed to reload {rel}: {e}. The on-disk file was not loaded; resolve and re-save."
      ))),
  }
  return;
  ```
  Or return a `Result<(), BuilderError>` and have `drain_watcher_events` route
  the error.
- **Effort:** S
- **Risk of fix:** Low

### F-015-007 — [HIGH] [Maintainability] `session.rs::into_state` duplicates `new_blank`'s 100+-field init block
- **Location:** `builder/src/builder/session.rs:95-242` vs.
  `builder/src/builder/state/mod.rs:520-664`
- **Category:** Maintainability / drift
- **Confidence:** High
- **Blast radius:** Every new `BuilderState` field. The two literals must be
  kept in lock-step by hand; the file already shows evidence of drift risk
  (defaults for `hex_size`, `tick_log_capacity`, `system_view_side` are
  duplicated as magic numbers in both places).
- **Problem:** Adding a new field to `BuilderState` requires editing both
  initialiser blocks. Forgetting either is an easy mistake and compiles cleanly
  (`Default` is not derived).
- **Why it matters:** Silent semantic divergence between freshly opened and
  session-restored projects.
- **Suggested fix:** Either implement `Default` for `BuilderState` and call
  `..BuilderState::default()` from both call sites, or factor out a
  `fn defaults_with_sector(sector: GeneratedSector, config: AppConfig,
  index: BuilderIndex) -> BuilderState` helper that both `new_blank` and
  `into_state` call. With a helper, each site only sets the few fields that
  actually differ (session restores `command_log`, `command_cursor`,
  `snapshots`, `pinned_*`, `stable_ids_on_rename`).
- **Effort:** M
- **Risk of fix:** Low

### F-015-008 — [MEDIUM] [Resource / §3.9] `atomic_write` leaks `.tmp.<pid>` siblings on rename failure
- **Location:** `builder/src/builder/project_io.rs:739-755`
- **Category:** Resource management
- **Confidence:** High
- **Blast radius:** Every save path. When `fs::rename` fails (cross-fs,
  permission, dest is a directory, etc.), the tmp file is left orphaned and
  there is no cleanup pass.
- **Problem:** The function writes
  `parent.join(format!(".{file_name}.tmp.{}", std::process::id()))` then `fs::rename`s
  it into place. If the rename fails, the tmp file is leaked; nothing in the
  loader scrubs dotfiles either, so they accumulate over the project's life.
- **Why it matters:** Disk leakage and confusing `.foo.toml.tmp.12345` siblings
  in the project tree.
- **Suggested fix:** Wrap the rename in a guard that removes the tmp file on
  drop unless explicitly committed:
  ```rust
  struct TmpGuard<'a>(&'a Path, bool);
  impl Drop for TmpGuard<'_> { fn drop(&mut self) { if !self.1 { let _ = fs::remove_file(self.0); } } }
  // ...
  let mut guard = TmpGuard(Path::new(tmp.as_str()), false);
  fs::rename(...)?;
  guard.1 = true;
  ```
  Or call `fs::remove_file(tmp)` in the error path explicitly.
- **Effort:** S
- **Risk of fix:** Low

### F-015-009 — [MEDIUM] [Correctness] `enforce_command_log_capacity` clamps snapshot positions to 0 silently
- **Location:** `builder/src/builder/state/undo.rs:51-62`
- **Category:** Correctness
- **Confidence:** High
- **Blast radius:** Long sessions on a bounded log. When a snapshot's
  `command_log_position` was, say, 5, and the log capacity trims 10 entries,
  the snapshot now anchors at `5.saturating_sub(10) == 0`. The snapshot still
  references the correct sector (the `sector` field is owned), but
  `revert_to_snapshot` then sets `command_cursor = snap.command_log_position.min(self.command_log.len())`
  to 0 — which silently discards the redo tail for *every other snapshot* and
  for the user's current position in the log.
- **Problem:** No warning, no flag — the snapshot looks valid but its undo
  anchor is meaningless.
- **Why it matters:** Revert to a snapshot taken before the ring buffer trim
  silently rewinds the entire redo history.
- **Suggested fix:** Track per-snapshot validity. Either drop snapshots whose
  position was trimmed (less surprising), or store a `valid_anchor: bool` flag
  and disable revert to those:
  ```rust
  fn enforce_command_log_capacity(&mut self) {
      // ...
      self.snapshots.retain_mut(|snap| {
          if snap.command_log_position >= drop {
              snap.command_log_position -= drop;
              true
          } else {
              // Snapshot anchor was trimmed; keep the sector but mark stale.
              snap.command_log_position = 0;
              snap.anchor_stale = true;
              true
          }
      });
  }
  ```
- **Effort:** M
- **Risk of fix:** Low

### F-015-010 — [MEDIUM] [Correctness] `SetStarSpectral::apply` returns `SystemNotFound` when system has no star
- **Location:** `builder/src/builder/command.rs:602-619` (specifically line 615)
- **Category:** Error model
- **Confidence:** High
- **Blast radius:** Defence-in-depth check in the menu path. The matching test
  at line 1437 (`set_star_spectral_errors_when_no_star`) asserts this exact
  wrong-variant behaviour, so a fix would need a `MutationError::MissingStar`
  variant or a more general `MutationError::Precondition(String)`.
- **Problem:** Using `SystemNotFound` for "system has no star" makes the error
  message wrong (it says the system doesn't exist when it does).
- **Why it matters:** A surfaced error like "system not found" pointing at an
  existing system id mis-leads downstream error handling and the user.
- **Suggested fix:** Add a new variant in
  `sectorforge::sector_model::mutation::MutationError` (e.g.
  `StarMissing(String)`), use it here, and update the test. Update the menu
  show-condition unchanged.
- **Effort:** S
- **Risk of fix:** Low

### F-015-011 — [MEDIUM] [Perf / §3.6] `run` rebuilds the entire `BuilderIndex` and clears every cached derivation on every command
- **Location:** `builder/src/builder/state/undo.rs:32-45` (and identical
  pattern at `undo.rs:88-94` and `:104-111`)
- **Category:** Performance (interactive editing path)
- **Confidence:** High
- **Blast radius:** Every Ctrl+Z / Ctrl+Y and every panel-driven mutation. For
  a thousand-system sector, `BuilderIndex::rebuild` is O(systems + worlds +
  routes), and `derivation_cache.clear()` invalidates every memoised overlay
  unconditionally. A "rename region" (`RenameRegion` — affects no systems, no
  worlds, no routes, no derivations) triggers the same rebuild as `AddSystem`.
- **Problem:** No per-command granularity. Every command pays the worst-case
  invalidation cost.
- **Why it matters:** GUI editing latency. Large projects already feel sluggish
  when typing in rename dialogs that dispatch on every keystroke.
- **Suggested fix:** Give `BuilderCommand` an `invalidates(&self) ->
  InvalidationKind` method that returns a small bitfield of "rebuild index?
  clear derivation cache? recompute map cache digest?" Then in `run` /
  `undo` / `redo` only do what's needed. Default to "rebuild everything" for
  unknown/structural variants so the safe path stays the default.
- **Effort:** M
- **Risk of fix:** Medium — needs care to preserve correctness in test corpus.

### F-015-012 — [MEDIUM] [Perf / §3.6] `recompute_economy` clones the full report and walks the world list four times
- **Location:** `builder/src/builder/state/derivations.rs:36-160`
- **Category:** Performance (per-recompute, frequent on auto-recompute)
- **Confidence:** Medium
- **Blast radius:** Triggered on every economy-touching mutation when
  `economy_auto_recompute` (implicit) is true. The function:
  1. Walks `report.systems` once to build `sys_idx` (line 45).
  2. Walks `report.worlds` to patch overrides (line 51).
  3. Walks `report.worlds` again to re-aggregate per-system (line 73).
  4. Walks `report.systems` once to write back, then again for per-system
     overrides, then again for sector totals.
  5. Sets `self.sector.economy = Arc::new(report)` at line 150, then if
     `feed_stability` calls `self.sector.economy.as_ref().clone()` at line 152
     to get *another* full clone for `apply_stability_nudge`.
- **Problem:** The `report` value is already owned and locally available; the
  Arc dance + clone at line 152 is gratuitous.
- **Why it matters:** Auto-recompute fires on every relations / world / faction
  edit; this is a noticeable per-edit cost.
- **Suggested fix:** Hoist `apply_stability_nudge` *before* the `Arc::new` so
  it operates on the owned `report`:
  ```rust
  let mut report = ...; // patched as today
  if cfg.feed_stability {
      apply_stability_nudge(&report, &mut self.sector);
  }
  self.sector.economy = std::sync::Arc::new(report);
  ```
  Also drop the unused `sys_idx` lookup (line 44-47, 155 `let _ = sys_idx;`) —
  no caller uses it.
- **Effort:** S
- **Risk of fix:** Low — verify with the determinism / golden tests.

### F-015-013 — [MEDIUM] [Correctness] `advance_conflict_ticks` reads back its own command from the log
- **Location:** `builder/src/builder/state/derivations.rs:395-486`
- **Category:** Correctness / fragility
- **Confidence:** High
- **Blast radius:** Every "advance N ticks" dispatch. After `self.run(cmd)?`,
  the function reads `self.command_log[self.command_cursor.saturating_sub(1)]`
  to extract the `before_world` / `before_system` snapshots populated by
  `apply`. If `command_log_capacity == 1` (legal), the ring buffer trim in
  `enforce_command_log_capacity` could drop that very command before the read,
  yielding the *previous* command and corrupting the tick log.
- **Problem:** Coupling between `run`'s book-keeping and a follow-up read.
- **Why it matters:** Bug under a small / unbounded capacity setting; also
  fragile to any future change in `run`'s ordering.
- **Suggested fix:** Have `BuilderCommand::apply` either return the diff
  payload (refactor signature to `apply(...) -> Result<ApplyOutcome,
  MutationError>`) or stash the diffs on the state directly (e.g.
  `state.last_apply_outcome`) before the trim happens. Then read the diffs
  from there instead of re-grepping the log.
- **Effort:** M
- **Risk of fix:** Low

### F-015-014 — [MEDIUM] [Correctness] `AdvanceConflictTicks::revert` doesn't roll back full sector state
- **Location:** `builder/src/builder/command.rs:650-670` (apply) and
  `905-932` (revert)
- **Category:** Correctness
- **Confidence:** Medium
- **Blast radius:** Undo of a ticks-advance command.
  `sectorforge::conflict::advance_sector` may mutate more than
  `sys.conflict`, `sys.control.dominant`, and `world.conflict` (it could touch
  routes, stability, presence rolls, etc.). The revert only restores those
  three fields.
- **Problem:** The before-snapshot is narrow. Anything the conflict tick wrote
  outside those three fields stays applied after undo.
- **Why it matters:** Drift between "sector after N ticks then undo" and
  "sector before N ticks" — breaks the determinism that the rest of the
  command bus is careful about.
- **Suggested fix:** Either snapshot the full sector in `before_sector:
  Box<GeneratedSector>` (heavy but trivially correct) or audit
  `advance_sector` to enumerate exactly what it touches and snapshot each.
  The first is simpler and avoids reviewer drift if `advance_sector` grows.
  Add an integration test that does
  `state.run(AdvanceConflictTicks{...})?; state.undo()?; assert_eq!(state.sector, before);`.
- **Effort:** S (heavy revert) / M (audit)
- **Risk of fix:** Low

### F-015-015 — [MEDIUM] [Tests / §3.10] `state/tests.rs` only round-trips one command variant
- **Location:** `builder/src/builder/state/tests.rs:12-78`
- **Category:** Test coverage
- **Confidence:** High
- **Blast radius:** Regression detection. `state/tests.rs` only exercises
  `BuilderCommand::AddSystem` through `state.run`. Every other variant
  (`RemoveSystem`, `MoveSystem`, `RenameSystem`, `SwapSystems`, `ReplaceSystem`,
  `AddWorld`, `RemoveWorld`, `AddRoute`, `RemoveRoute`, `ReplaceRoutes`,
  `AddFaction`, `RemoveFaction`, `SetArchetype`, `AutoAssignArchetypes`,
  `SetOrbitalAssets`, `SetBlockadeReport`, `SetSurfaceRegions`,
  `SetWorldConflict`, `SetSystemConflict`, `SetWorldStability`, `SetRouteType`,
  `SetRouteStability`, `SetRegionKind`, `RenameRegion`, `SetStar`,
  `SetStarSpectral`, `RenameWorld`, `SetWorldOrbit`, `AdvanceConflictTicks`)
  has only an isolated `apply`/`revert` test in `command.rs`. Nothing tests
  that `state.run(cmd)?; state.undo()?; ...; state.redo()?;` round-trips for
  any variant other than `AddSystem`.
- **Problem:** The high-value test surface (the bus + cache invalidation + index
  rebuild + auto-save trigger) is barely exercised.
- **Why it matters:** A regression where `run` forgets to rebuild the index
  for, say, `RemoveSystem` would not be caught.
- **Suggested fix:** Add one `run → undo → redo` round-trip test per variant.
  Use a small macro or a table-driven test to keep boilerplate low.
- **Effort:** M
- **Risk of fix:** Low

### F-015-016 — [MEDIUM] [Tests] No coverage for IO error paths in project_io
- **Location:** `builder/src/builder/project_io.rs:926-1053`
- **Category:** Test coverage
- **Confidence:** High
- **Blast radius:** Save / open failure modes. The tests cover happy path +
  TOML parse error in `sectorforge.toml`. Not covered: dest-already-exists
  (`new_project`), missing `worlds.toml`, malformed `sector.json`, save on a
  read-only directory, `atomic_write` rename failure (would catch F-015-008).
- **Problem:** Error variants in `BuilderError` (e.g. `IoFailed`, `Serde`) are
  un-tested.
- **Suggested fix:** Add tests for each error path; `tempfile::TempDir` plus
  setting read-only permissions covers most. The atomic-write test can simulate
  rename failure by pre-creating a directory at the target name.
- **Effort:** S
- **Risk of fix:** Low

### F-015-017 — [MEDIUM] [API design] `BuilderWorkspace::active` documents a `try_active` method that doesn't exist
- **Location:** `builder/src/builder/workspace.rs:58-64`
- **Category:** API / doc
- **Confidence:** High
- **Blast radius:** Anyone trying to use the non-panicking variant. The doc
  comment at line 60 says "Panics when the workspace is empty — callers must
  `push` first or use [`Self::try_active`]." but `try_active` is not defined.
- **Problem:** Doc lies. `active()` indexes `self.states[self.active]` and will
  panic on empty workspace; the only construction route (`new`) seeds at least
  one state but a future `BuilderWorkspace::default()` call (the struct
  derives `Default`) creates an empty workspace, then any `active()` call
  panics with no friendly message.
- **Why it matters:** Reachable panic with no clear remediation.
- **Suggested fix:** Add the documented method:
  ```rust
  pub fn try_active(&self) -> Option<&BuilderState> {
      self.states.get(self.active)
  }
  pub fn try_active_mut(&mut self) -> Option<&mut BuilderState> {
      self.states.get_mut(self.active)
  }
  ```
- **Effort:** XS
- **Risk of fix:** Low

### F-015-018 — [MEDIUM] [Perf / §3.6] `selection::focus_entity` uses `Vec::remove(0)` to enforce nav-stack cap
- **Location:** `builder/src/builder/state/selection.rs:55-60`
  (also `:72-75`, `:88-90`)
- **Category:** Performance
- **Confidence:** High
- **Blast radius:** Every nav action past the 64-entry cap. `Vec::remove(0)` is
  O(n) per push because every later element shifts left. The cap is 64 so the
  cost is bounded, but the pattern is repeated three times.
- **Problem:** Using `Vec` as a bounded FIFO.
- **Suggested fix:** Switch `nav_back_stack` / `nav_forward_stack` to
  `VecDeque<EntityRef>` so the cap can be enforced with O(1) `pop_front`.
- **Effort:** XS
- **Risk of fix:** Low

### F-015-019 — [MEDIUM] [Docs / contract] `PartialRegenRect` docs disagree about inclusivity
- **Location:** `builder/src/builder/state/types.rs:499-531` ("inclusive
  rectangle") vs. `builder/src/builder/state/mod.rs:148` ("§G5: half-open
  axial-hex rectangle")
- **Category:** Documentation
- **Confidence:** High
- **Blast radius:** Anyone reasoning about regen rect bounds. The `contains`
  impl at `types.rs:524-530` uses `<= max`, so the type is in fact inclusive;
  the field doc string is wrong.
- **Problem:** The field doc in `state/mod.rs:148` says "half-open". A reader
  porting code based on that doc would miscount edge hexes.
- **Suggested fix:** Update `state/mod.rs:148` to "inclusive axial-hex
  rectangle".
- **Effort:** XS
- **Risk of fix:** None

### F-015-020 — [LOW] [Idiomatic] Modal `clone()` in `show_modal` matches-and-then-rematches
- **Location:** `builder/src/app.rs:118-157`
- **Category:** Idiomatic Rust
- **Confidence:** Medium
- **Blast radius:** Once per frame. The function first clones
  `self.workspace.active().modal` at line 119 just to discriminate, then takes
  ownership inside `egui::Window::show`'s closure where another `match`
  decides what to render. Two separate matches over `ModalKind` for the same
  value.
- **Problem:** The double-match is a smell; one of the two arms can borrow.
- **Suggested fix:** Cache `let title = match modal_kind_label(&modal) { ... };`
  and pass `&modal` into the inner branch using a `match modal { ... }` only
  once. Or do all dispatch on the (active_mut, modal kind) pair in one place
  and avoid the clone of large variants like `ConfirmRevertSnapshot` and
  `NewFromPreset`.
- **Effort:** S
- **Risk of fix:** Low

### F-015-021 — [LOW] [Perf] `save_project_as` clones digests element-by-element instead of moving the map
- **Location:** `builder/src/builder/project_io.rs:608-611`
- **Category:** Performance (once per save)
- **Confidence:** High
- **Blast radius:** Once per save. The code clones every key/value of `digests`
  into `state.sector.manifest.input_digests`. Both are `BTreeMap<String,
  String>`. The local `digests` is unused after this point so `std::mem::take`
  or `digests.clone()` then drop both work.
- **Suggested fix:** Move:
  ```rust
  state.sector.manifest.input_digests = std::mem::take(&mut digests).into_iter().collect();
  ```
  or just `state.sector.manifest.input_digests = digests.clone();` if the map
  type matches (assignment if same type works as well — check the
  `input_digests` field type and use `=` directly).
- **Effort:** XS
- **Risk of fix:** None

### F-015-022 — [LOW] [Idiomatic] `recompute_economy` keeps a dead `sys_idx` lookup
- **Location:** `builder/src/builder/state/derivations.rs:44-47, 155`
  (`let _ = sys_idx;`)
- **Category:** Dead code
- **Confidence:** High
- **Problem:** `sys_idx` is built but never read. The `let _ = sys_idx;` on
  line 155 explicitly silences the warning, signalling the author knows.
- **Suggested fix:** Delete the variable and the suppression.
- **Effort:** XS

### F-015-023 — [LOW] [Idiomatic] `BuilderState::new_blank` is a 130-line struct literal — no Default impl
- **Location:** `builder/src/builder/state/mod.rs:521-663`
- **Category:** Maintainability
- **Confidence:** High
- **Problem:** The struct has 115+ fields all initialised inline. Combined with
  F-015-007, every new field requires syncing two places. A `Default` impl
  would let both sites do `BuilderState { sector, index, config: default_config(...), ..Default::default() }`.
- **Suggested fix:** Derive or hand-write `Default` for `BuilderState`. Many
  field types already have sensible defaults; the few that don't
  (`GeneratedSector`, `AppConfig`, `BuilderIndex`) are exactly the ones every
  caller already supplies.
- **Effort:** M
- **Risk of fix:** Low

### F-015-024 — [LOW] [Perf] `regenerate_partial` does O(n^2) replacement scan
- **Location:** `builder/src/builder/state/generation_ops.rs:241-262`
- **Category:** Performance
- **Confidence:** High
- **Blast radius:** Per partial-regen click. Inner loop at line 258 calls
  `self.sector.systems.iter_mut().find(|s| s.id == new_sys.id)` for every
  replacement, so a 100-system regen is 10 000 comparisons.
- **Suggested fix:** Build an `id → index` lookup once before the loop, or
  collect a `BTreeMap<SystemId, GeneratedSystem>` of replacements then iterate
  `self.sector.systems` once.
- **Effort:** XS
- **Risk of fix:** None

### F-015-025 — [LOW] [Idiomatic] `decode_base64` doesn't validate length-mod-4 or padding placement
- **Location:** `builder/src/builder/session.rs:307-342`
- **Category:** Robustness
- **Confidence:** Medium
- **Problem:** The decoder accepts inputs whose padding doesn't match the
  formal Base64 spec (e.g. `=` in the middle of a chunk is treated as zero,
  not rejected). Round-trip works because the encoder produces canonical
  output, but a hand-edited `.sgforge` file could feed bogus data through and
  silently produce wrong bytes.
- **Suggested fix:** After the loop, reject `i != 0` (unfinished group). In
  the loop, reject `is_pad` for positions 0/1 and reject any non-pad after
  the first pad.
- **Effort:** S
- **Risk of fix:** Low

### F-015-026 — [LOW] [Determinism / CLAUDE.md] `selection::nav_back_stack.remove(0)` cap should also drop forward stack on overflow
- **Location:** `builder/src/builder/state/selection.rs:55-91`
- **Category:** Correctness
- **Confidence:** Medium
- **Problem:** When the back stack overflows and drops its oldest entry, the
  forward stack is *not* cleared — so a Ctrl+Alt+→ after many navigations
  could redo to a target whose corresponding back entry was just evicted, and
  the user can no longer return.
- **Suggested fix:** Either accept this as acceptable degradation and document
  it, or trim both stacks symmetrically. Currently doc says nothing.
- **Effort:** XS

### F-015-027 — [NIT] [Style] `command.rs:366` `let _ = si;` comment "silence unused warning under some configs"
- **Location:** `builder/src/builder/command.rs:366`
- **Category:** Style
- **Confidence:** High
- **Problem:** Inside the `RemoveWorld` apply arm, `si` is bound from
  `.enumerate()` but only used for the dropped name. Rename to `_si` and the
  suppression goes away.
- **Suggested fix:** `for (_si, sys) in ...` — drop the `let _ = si;` line.
- **Effort:** XS

### F-015-028 — [NIT] [Style] `default_config` is duplicated between `state/mod.rs:666` and `project_io.rs:43`
- **Location:**
  - `builder/src/builder/state/mod.rs:666-731` (`default_config`)
  - `builder/src/builder/project_io.rs:43-108` (`default_app_config`)
- **Category:** DRY
- **Confidence:** High
- **Problem:** Two essentially identical functions building an `AppConfig` from
  `(id, title, seed, width, height)` differ only in `project.version` (`None`
  vs `Some("0.1.0")`). Either consolidate into a single helper that takes a
  `version: Option<String>` arg, or pick one as canonical and have the other
  call it.
- **Suggested fix:** Move the helper into `state/mod.rs` (or a shared
  `defaults.rs` module) and let `project_io.rs` call it with the version
  override.
- **Effort:** S
- **Risk of fix:** Low

### F-015-029 — [NIT] [Style] `state/mod.rs:115` comment claims "Folded into MapViewCache digest" but cache field is in `regions_ops.rs`
- **Location:** `builder/src/builder/state/mod.rs:226-229`
- **Category:** Doc
- **Confidence:** Low
- **Problem:** The comment on `subsector_target_systems` says the value is
  "folded into the [`MapViewCache`] digest", but the digest is computed in
  `panels/map.rs` (out of unit), and the type doc on `MapViewCache` itself
  (`types.rs:353`) doesn't list the inputs. Hard to follow.
- **Suggested fix:** Add a `# Digest inputs` section on `MapViewCache` that
  enumerates the live inputs.
- **Effort:** XS

### F-015-030 — [NIT] [Doc] `PreviewState::sector` is `pub` and writable from any panel
- **Location:** `builder/src/builder/preview.rs:36-50`
- **Category:** Encapsulation
- **Confidence:** High
- **Problem:** Even though preview state isn't part of undoable history, the
  `pub sector: Option<GeneratedSector>` field can be overwritten from outside
  the worker pipeline, breaking the "results posted by older revisions are
  dropped" guarantee.
- **Suggested fix:** Make `sector`, `job`, `timer`, `revision`, `error`
  `pub(crate)` and expose read-only accessors plus the one mutation route
  (`schedule` / `clear` / `apply_result` / `pump`).
- **Effort:** S
- **Risk of fix:** Low

## Rubric coverage (§3)

- **3.1 Panics & failure surface.** Project-IO and command-bus modules are
  unwrap-free in non-test code. One realistic panic vector is `BuilderWorkspace::active`
  on an empty workspace (F-015-017). `derivations.rs:423` uses `.unwrap_or(0)`
  on an empty deque — safe. `generation_ops.rs:121` parses a star colour code
  with `.unwrap_or(StarColour::Yellow)` — safe fallback.
- **3.2 unsafe & soundness.** No `unsafe` in unit. No findings.
- **3.3 Ownership / cloning.** Hot spots flagged: F-015-012 (economy report
  full clone for stability nudge), F-015-021 (digest map element clone),
  F-015-024 (find-in-loop). Otherwise the code's clones are at version-boundary
  edges (config → command payload) and acceptable.
- **3.4 Error handling.** F-015-005 (auto-save swallows), F-015-006 (reload
  swallows), F-015-010 (wrong error variant). `BuilderError` is a clean
  `thiserror` enum; `?` carries enough context via `#[from]` chains. No
  `Box<dyn Error>` in public signatures. `#[non_exhaustive]` is missing on
  `BuilderError` and `BuilderCommand` — should add to allow future variants
  without a major bump (NIT, not separately filed).
- **3.5 Concurrency & async.** No async. `FileWatcher` thread is correct
  (cancel flag, join on drop). `PreviewState::pump` correctly handles
  `Disconnected`. No findings.
- **3.6 Performance.** F-015-011 (per-command full index rebuild),
  F-015-012 (economy recompute clones),
  F-015-018 (Vec::remove(0) for stack cap),
  F-015-024 (O(n^2) partial regen).
- **3.7 Idiomatic Rust / API design.** F-015-002 (everything `pub`),
  F-015-017 (`BuilderWorkspace::active` doc lies),
  F-015-019 (rect doc disagreement),
  F-015-020 (double match on modal),
  F-015-030 (`PreviewState` fields `pub`).
  Also: `BuilderCommand` and `BuilderError` lack `#[non_exhaustive]` — would
  let the bus grow without forcing every downstream `match` to break.
- **3.8 Dependencies / Cargo.** No new direct deps observed. `base64` was
  deliberately hand-rolled to honour R9 (no new crates). `notify` likewise
  swapped for in-house polling. Both are documented. No findings here.
- **3.9 Memory & resource.** F-015-008 (tmp file leak),
  F-015-009 (snapshot anchor invalidation).
- **3.10 Testing.** F-015-015 (state-level command coverage),
  F-015-016 (IO error paths).
- **3.11 Documentation.** F-015-019 (rect inclusivity),
  F-015-017 (try_active doc),
  F-015-029 (cache digest inputs),
  plus general TODO/FIXME inventory: none found in unit (good).

## Project-specific invariants

- **CLAUDE.md R4 (command bus).** Violated in 5+ places (F-015-001, F-015-003).
  The `Fx*` iteration ban is moot for this unit — only `BTreeMap` / `BTreeSet`
  used.
- **CLAUDE.md RNG invariant.** Preview / regen paths derive seeds via
  `sectorforge::rng::digest_bytes` and `preview::derive_reroll_seed` — both
  use the stage-keyed digest. No `thread_rng()`. Clean.
- **Output byte-stability.** Auto-save and `save_project_as` use
  `serde_json::to_string_pretty` against a deterministic `GeneratedSector`.
  Iteration order is `BTreeMap`/`BTreeSet`. Clean.

## Summary of suggested fixes

- F-015-001 — HIGH — Add commands for `apply_preview`, `regenerate_partial`,
  `regenerate_world`, `reroll_seed`, and all region ops; route through
  `state.run` — L / Medium risk.
- F-015-002 — HIGH — Make undo-bearing `BuilderState` fields `pub(crate)`,
  expose getters, refactor panels — M / Medium risk.
- F-015-003 — HIGH — Move `recompute_*` Arc writes out of `sector` (lazy
  derive at render) or wrap in commands — M / Medium risk.
- F-015-004 — HIGH — Make `app.rs::show_modal` exhaustively handle every
  `ModalKind` or assert ownership — S / Low risk.
- F-015-005 — HIGH — Capture and surface auto-save IO/serialise errors — S /
  Low risk.
- F-015-006 — HIGH — Surface `reload_catalog` parse errors via
  `ModalKind::Message` instead of swallowing — S / Low risk.
- F-015-007 — HIGH — Extract shared default-init helper or impl `Default` for
  `BuilderState` — M / Low risk.
- F-015-008 — MEDIUM — Add a `TmpGuard` so `atomic_write` cleans tmp on
  failure — S / Low risk.
- F-015-009 — MEDIUM — Mark snapshots whose anchor was trimmed and refuse /
  warn on revert — M / Low risk.
- F-015-010 — MEDIUM — Add a `MutationError::StarMissing` variant — S / Low risk.
- F-015-011 — MEDIUM — Give `BuilderCommand` an `invalidates()` method to scope
  index/cache rebuilds — M / Medium risk.
- F-015-012 — MEDIUM — Avoid the double economy-report clone and drop the dead
  `sys_idx` table — S / Low risk.
- F-015-013 — MEDIUM — Return diffs from `apply` instead of re-reading from the
  log — M / Low risk.
- F-015-014 — MEDIUM — Snapshot full sector on `AdvanceConflictTicks` revert
  or audit `advance_sector` — S/M / Low risk.
- F-015-015 — MEDIUM — Add `run → undo → redo` round-trip tests per command
  variant — M / Low risk.
- F-015-016 — MEDIUM — Test IO failure paths in project_io — S / Low risk.
- F-015-017 — MEDIUM — Add `BuilderWorkspace::try_active{,_mut}` — XS / Low risk.
- F-015-018 — MEDIUM — Switch nav stacks to `VecDeque` — XS / Low risk.
- F-015-019 — MEDIUM — Fix `PartialRegenRect` inclusivity doc in `state/mod.rs:148`
  — XS / None.
- F-015-020 — LOW — Single-match modal dispatch in `show_modal` — S / Low risk.
- F-015-021 — LOW — Move-assign `digests` map instead of element clone — XS /
  None.
- F-015-022 — LOW — Delete dead `sys_idx` lookup — XS / None.
- F-015-023 — LOW — Impl `Default` for `BuilderState` — M / Low risk.
- F-015-024 — LOW — Replace `iter_mut().find` loop with an id→idx map — XS /
  None.
- F-015-025 — LOW — Tighten `decode_base64` length/padding validation — S /
  Low risk.
- F-015-026 — LOW — Decide whether nav forward stack should clear on back-cap
  overflow; document — XS / None.
- F-015-027 — NIT — Rename `si` to `_si` in `RemoveWorld::apply` — XS / None.
- F-015-028 — NIT — Consolidate `default_config` / `default_app_config` — S /
  Low risk.
- F-015-029 — NIT — Document `MapViewCache` digest inputs — XS / None.
- F-015-030 — NIT — Tighten `PreviewState` field visibility to `pub(crate)` —
  S / Low risk.
