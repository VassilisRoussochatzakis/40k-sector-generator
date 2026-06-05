# AREA D — builder command bus + state — verification

Verified 2026-06-05 against live source. Scope: `builder/src/builder/command.rs` (1922 LOC),
`builder/src/builder/state/mod.rs` (935 LOC), `builder/src/builder/state/derivations.rs` (717 LOC),
`builder/src/builder/state/regions_ops.rs` (106 LOC).

---

## Resolution (2026-06-05)

All 14 findings closed. Workspace green: `cargo test --workspace` 0 failures,
`cargo test --test it -- golden` 13/13, `cargo clippy --workspace --all-targets
-D warnings` clean.

| ID | Outcome |
|----|---------|
| D7 | Dead `let _ = si;` + the `.enumerate()` removed from `RemoveWorld::apply`. |
| D8 | Dead `sys_idx` `BTreeMap` (+ silencer) removed from `recompute_economy`. |
| D9 | `builder_command_size_is_bounded` test caps `size_of::<BuilderCommand>()` ≤ 256 B. |
| D10 | `revalidate_now` sets `last_validation_skip_reason`; status bar shows "validation skipped: …". |
| D4 | Closed as a **justified carve-out** — verified `derive_route_controls` reads only endpoint faction-presences + tags, never coordinates, so move/swap cannot stale `controls` (only `distance`, which they already refresh). Documented at the apply arms. |
| D1 / D-S2 | Region add/remove/paint/erase **and** the REG1/REG3 `update_region` edits now route through new `AddRegion` / `RemoveRegion` / `EditRegion` commands (`dep_classes → Regions`). Brush strokes coalesce to one undoable `EditRegion` on drag-release (live preview during the drag) so the undo log + auto-save aren't spammed per hex. |
| D2 / D11 | `recompute_chronicle_undoable` dispatches `EditChronicle` through the bus for the "Regenerate chronicle" button + `history_auto_recompute` trigger (option B). The passive LD4 refresh (`recompute_chronicle`) stays off-bus so viewing the HISTORY tab never evicts the redo tail; manual events preserved on both paths. |
| D6 | Already fixed; added a comment at the economy install pointing to the `ensure_fresh` fingerprint gate. |
| D-S1 / D3 | **Proportionate** resolution (the trait/macro rewrite was assessed net-negative — de-enuming breaks the serialized command log, and the named "missing arm" risk is already compiler-prevented by the three exhaustive `_`-less matches). Added `system_mut`/`world_mut` helpers collapsing ~12 repeated find-or-`NotFound` chains, the `dep_classes_cover_all_variants` exhaustive test, and a co-maintenance doc note. |
| D-S3 / D5 | **Deferred** by decision — the 154-field god-struct split is a high-churn field-move the review itself sequences *after* a G2 content-golden safety net that does not yet exist. To be done in a focused pass alongside G2. |

---

## Summary table

| ID   | Sev | Status          | Bus-verdict                   | Effort | One-line                                                              |
|------|-----|-----------------|-------------------------------|--------|-----------------------------------------------------------------------|
| D-S1 | P2  | ✅ Confirmed     | —                             | L      | 35 variants × 3 match fns = 105 arms; convention-only pairing        |
| D-S2 | P0  | ✅ Confirmed     | REAL bypass (D1) + Needs owner decision (D2/D11) | L | Document-state mutations outside bus; see per-finding verdicts |
| D-S3 | P2  | ✅ Confirmed     | —                             | L      | 154 `pub(crate)` fields in one struct                                 |
| D1   | HIGH P0 | ✅ Confirmed | REAL bypass                   | M      | Region add/paint/erase bypass bus; §D3 cite authoritative per comment |
| D2   | HIGH P0 | ✅ Confirmed | Needs owner decision          | M      | `recompute_chronicle` overwrites `sector.chronicle` off-bus; manual events preserved, no interleave hole with `EditChronicle` at present |
| D3   | MED P2  | ✅ Confirmed | —                             | L      | Three parallel match fns; 1922 LOC confirmed                          |
| D4   | MED P0  | ⚠️ Partial   | Carve-out (justified)         | S      | Move/swap refresh `distance` but not `controls`; controls are faction-derived, not position-derived — real staleness but narrow scope |
| D5   | MED P2  | ✅ Confirmed | —                             | L      | 154 `pub(crate)` fields confirmed (review said ~110; actual is higher) |
| D6   | MED P2  | 🟢 Already fixed | —                           | —      | Staleness gate confirmed at `ensure_fresh`; economy not recomputed per-frame |
| D7   | LOW P3  | 🔄 Moved (line drift) | —                    | S      | `let _ = si;` now at `command.rs:478`, not 478 (unchanged line)      |
| D8   | LOW P3  | ✅ Confirmed | —                             | S      | `sys_idx` BTreeMap built at `derivations.rs:200-202`, silenced at :311 |
| D9   | LOW P3  | ✅ Confirmed | —                             | S      | No `size_of` guard or `#[allow(large_enum_variant)]` on `BuilderCommand` |
| D10  | LOW P3  | ✅ Confirmed | —                             | S      | Validation silently skipped when worlds catalog missing; no status hint |
| D11  | LOW P0  | ✅ Confirmed | Needs owner decision          | S      | `EditChronicle` vs `recompute_chronicle` precedence not documented    |

---

## Findings

### D-S1 — hand-maintained three-way match symmetry
- **Review sev / bucket:** systemic / P2
- **Status:** ✅ Confirmed
- **Bus verdict (bypass findings only):** —
- **Location:** `builder/src/builder/command.rs:363,410,857` (verified)
- **Evidence:**
  ```rust
  pub fn dep_classes(&self) -> &'static [DepClass] { match self { … } }   // line 363
  pub fn apply(&mut self, sector: &mut GeneratedSector) -> … { match self { … } } // line 410
  pub fn revert(&self, sector: &mut GeneratedSector) -> … { match self { … } }    // line 857
  ```
- **Why it matters:** 35 variants × 3 match functions = 105 arms maintained by hand.
  Adding a variant requires edits in three separate places with no compiler enforcement that all
  three are updated consistently — a fourth `dep_classes` arm is the most likely omission.
- **Fix:** Extract a `Command` trait with `fn dep_classes`, `fn apply`, `fn revert`; or write a
  `command_impl!` paired-arm macro that emits all three arms from a single declaration site.
- **Effort:** L
- **Risk / deps:** Mechanical refactor touching all 105 arms; golden tests + clippy must pass.
  Low semantic risk if done arm-by-arm with the existing test suite.

---

### D-S2 — document state mutated outside the bus (systemic)
- **Review sev / bucket:** systemic / P0
- **Status:** ✅ Confirmed
- **Bus verdict (bypass findings only):** REAL bypass (D1); Needs owner decision (D2/D11)
- **Location:** `state/regions_ops.rs` (D1), `state/derivations.rs:358–378` (D2/D11)
- **Evidence:** See D1 and D2 per-finding sections below.
- **Why it matters:** Two classes of out-of-bus `sector.*` writes: region structural mutations
  (D1, clear bypass) and chronicle replacement (D2/D11, has compensating logic but no formal
  precedence contract).
- **Fix:** Resolve D1 via region commands; document D2/D11 precedence explicitly.
- **Effort:** L (aggregate)
- **Risk / deps:** D1 fix requires new `AddRegion`/`PaintRegion`/`EraseRegion` commands and
  command-bus wiring in the panels that call `add_region` / `paint_region_hex` /
  `erase_region_hex`.

---

### D-S3 — BuilderState ~154-field god-struct
- **Review sev / bucket:** systemic / P2
- **Status:** ✅ Confirmed (field count is higher than review estimated)
- **Bus verdict (bypass findings only):** —
- **Location:** `builder/src/builder/state/mod.rs:152–676` (verified)
- **Evidence:**
  ```rust
  pub struct BuilderState {
      pub(crate) sector: LiveSector,          // document
      pub(crate) derivations: DerivationLedger, // cache
      pub(crate) sector_context_menu: Option<SectorContextMenu>, // transient UI
      // … 151 more pub(crate) fields …
  }
  ```
- **Why it matters:** 154 `pub(crate)` fields in one struct (review estimated ~110 — actual is
  ~40% higher). Document state, derivation caches, and transient UI scratch all live flat.
  Reasoning about which fields must go through the bus versus which are carve-out exempt
  requires reading 676 lines.
- **Fix:** Fold panel-local scratch into sibling `*State` structs (e.g. `HistoryPanelState`,
  `ConflictPanelState`). `SearchState`, `DiffState`, `AnalyticsState`, `SegmentumState`,
  `ExportState` already demonstrate the pattern — extend it to remaining scratch groups
  (briefing, interestingness, regions-grow, routes-bulk-filter, subsector overrides).
- **Effort:** L
- **Risk / deps:** Pure field-move refactor; no semantic change. Requires updating every
  panel that references moved fields.

---

### D1 — region add/paint/erase bypass the bus
- **Review sev / bucket:** HIGH / P0
- **Status:** ✅ Confirmed
- **Bus verdict (bypass findings only):** REAL document-state bypass (undo/redo hole)
- **Location:** `builder/src/builder/state/regions_ops.rs:12–68` (verified; review cited :19,:47,:61,:84)
- **Evidence:**
  ```rust
  pub fn add_region(&mut self, name: &str, kind: …, centre: …) -> String {
      let id = self.next_region_id();
      let actual_id = self.sector.add_region(&id, name, kind, centre);
      self.dirty = true;
      self.invariant_report = Some(check_sector(&self.sector));
      actual_id
  }
  ```
- **Why it matters:** `sector.regions` is serialised into `sector.json` (document state).
  Region add/remove/paint/erase cannot be undone or redone. The file-level comment explicitly
  cites `§D3` ("Overlay edits don't go through the command bus per §D3") as the justification,
  but `§D3` in CLAUDE.md is the finding tag for the god-file problem in `command.rs`, not a
  carve-out rule. The CLAUDE.md §R4 carve-out covers transient UI (selection, drag, scroll) —
  regions are serialised output, not transient UI. The justification in the source comment
  is **not authoritative** relative to CLAUDE.md.
  Meanwhile `SetRegionKind` and `RenameRegion` do go through the bus — creating an
  inconsistency within the same domain: kind and name are undoable, but add/paint/erase are not.
- **Fix:** Add `BuilderCommand::AddRegion { name, kind, centre, result_id }`,
  `PaintRegionHex { id, hex }`, `EraseRegionHex { id, hex }`, `RemoveRegion { id, before }`.
  Route the `regions_ops.rs` methods through these commands (or replace them).
  Update `dep_classes` to return `&[D::Regions]` for all four.
- **Effort:** M
- **Risk / deps:** Panels calling `state.add_region(…)` directly must be updated to
  `state.run(BuilderCommand::AddRegion { … })`. Map paint tool is the primary caller.

---

### D2 — `recompute_chronicle` overwrites `sector.chronicle` off-bus
- **Review sev / bucket:** HIGH / P0
- **Status:** ✅ Confirmed
- **Bus verdict (bypass findings only):** Needs owner decision
- **Location:** `builder/src/builder/state/derivations.rs:358–378` (verified; review cited :373)
- **Evidence:**
  ```rust
  pub fn recompute_chronicle(&mut self) {
      let manual: Vec<…> = self.sector.chronicle.events
          .iter().filter(|e| e.manual).cloned().collect();
      let mut report = sectorforge::history::derive_with(&self.sector, &cfg);
      report.events.extend(manual);
      report.events.sort_by(…);
      self.sector.chronicle = report;   // ← direct write, off-bus
  ```
- **Why it matters:** `sector.chronicle` is document state (serialised to `sector.json`).
  However, the code explicitly drains and preserves `manual = true` events before overwriting,
  so user-authored events survive the recompute. There is no immediate data-loss hole.
  The real concern is precedence: if a user (a) edits a manual event via `EditChronicle`,
  then (b) the auto-recompute fires because an upstream dependency changed, the
  `recompute_chronicle` call at step (b) will re-read `sector.chronicle` and preserve the
  manual edit — but the undo log still contains the pre-(b) `EditChronicle` snapshot, so
  undoing the `EditChronicle` after the recompute will revert the edit and the undo stack
  and the live chronicle diverge silently. This is an edge case but is architecturally undefined.
- **Fix (option A — simple):** Document the precedence contract in a module-level comment:
  "recompute never loses manual events; EditChronicle always wins on conflict; the undo log
  may show stale before-snapshots after a recompute." Sufficient if the auto-recompute is only
  user-triggered (button), not per-frame.
- **Fix (option B — clean):** Route `recompute_chronicle` through
  `EditChronicle { before, after }` so the recomputed chronicle is on the undo stack.
  Auto-recompute on catalog change would then be undoable.
- **Effort:** S (option A) / M (option B)
- **Risk / deps:** Option B requires capturing before/after for every auto-recompute trigger,
  which may clutter the undo log. Owner decision needed on whether catalog-triggered
  re-derives should be undoable.

---

### D3 — three parallel match functions (god-file)
- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed
- **Bus verdict (bypass findings only):** —
- **Location:** `builder/src/builder/command.rs:363,410,857` (verified; 1922 LOC confirmed)
- **Evidence:**
  ```rust
  // command.rs is 1922 lines; three independent match fns:
  pub fn dep_classes(&self)    -> &'static [DepClass] { match self { … } }
  pub fn apply(&mut self, …)   -> Result<…>           { match self { … } }
  pub fn revert(&self, …)      -> Result<…>           { match self { … } }
  ```
- **Why it matters:** 35 variants × 3 = 105 arms. Drift risk on every new variant.
  See D-S1 for the systemic framing.
- **Fix:** Same as D-S1 — `Command` trait or paired-arm macro.
- **Effort:** L
- **Risk / deps:** Same as D-S1.

---

### D4 — MoveSystem/SwapSystems don't recompute incident-route `controls`
- **Review sev / bucket:** MED / P0
- **Status:** ⚠️ Partial
- **Bus verdict (bypass findings only):** Carve-out (justified) — but real staleness gap exists
- **Location:** `src/model/sector_model/mutation.rs:97–136` (move), `:154–197` (swap); `builder/src/builder/command.rs:442,444` (apply delegation)
- **Evidence:**
  ```rust
  // mutation.rs:move_system — refreshes distance, not controls
  for (rid, dist) in updated {
      if let Some(r) = self.routes.iter_mut().find(|r| r.id == rid) {
          r.distance = dist;
      }
  }
  // command.rs:442 — delegation
  Self::MoveSystem { id, from: _, to } => sector.move_system(id, *to),
  ```
- **Why it matters:** `GeneratedRoute::controls: Vec<RouteControl>` records per-faction
  dominance over a route, derived from the faction presences at the route's endpoints
  (`derive_route_controls` in `src/analysis/route_control.rs`). Moving a system changes which
  worlds are "nearby," but `controls` is derived from faction presences at the endpoint systems
  — **not from coordinates**. Therefore a move does not actually stale the controls data
  unless the user also changes factions at those systems. `AddRoute` does recompute controls
  (command.rs:495). The gap is real but its scope is narrow: if the user moves a system and
  then inspects the route-control overlay without recomputing relations, the overlay is correct
  (controls depend on faction presences at the same systems, which a coord-change doesn't alter).
  The staleness would only manifest if `recompute_relations` re-derives control data, which it
  does not — it derives the diplomatic matrix, not route controls. The bug is that a
  `MoveSystem` undo followed by `AddRoute` (which does recompute controls) could leave
  controls in an inconsistent state if the user has intervening relation changes.
- **Fix:** After `move_system`/`swap_systems` in `apply`, iterate incident routes and call
  `derive_route_controls` — mirroring the `AddRoute` arm (command.rs:492–499). This is
  O(incident_routes × systems) and harmless per move event.
- **Effort:** S
- **Risk / deps:** The lib-level `move_system`/`swap_systems` in `mutation.rs` should not
  call `derive_route_controls` because that creates an analysis-layer dependency in the
  model layer. The recompute belongs in the builder command's `apply` arm.

---

### D5 — BuilderState ~110-field god-struct (field count)
- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed (actual count exceeds review estimate)
- **Bus verdict (bypass findings only):** —
- **Location:** `builder/src/builder/state/mod.rs:152` (struct start, verified)
- **Evidence:** `grep -c "^    pub(crate)" state/mod.rs` → **154** fields.
  Review cited ~110; actual is ~40% higher.
- **Why it matters:** See D-S3.
- **Fix:** See D-S3.
- **Effort:** L
- **Risk / deps:** See D-S3.

---

### D6 — economy staleness gate prevents per-frame recompute
- **Review sev / bucket:** MED / P2
- **Status:** 🟢 Already fixed
- **Bus verdict (bypass findings only):** —
- **Location:** `builder/src/builder/state/derivations.rs:140–151` (`ensure_fresh`; review cited :306 which is mid-body of `recompute_economy`)
- **Evidence:**
  ```rust
  pub fn ensure_fresh(&mut self, kind: DerivationKind) {
      if !self.derivations.is_stale(kind) { return; }
      let current = self.derivation_fingerprint(kind);
      if self.derivations.fingerprints.get(&kind) == Some(&current) {
          self.derivations.mark_fresh(kind, current);
          return;   // ← fingerprint-stable: no recompute
      }
      self.recompute_derivation(kind);
  }
  ```
- **Why it matters:** The LD1/LD2 derivation ledger (BLAKE3 fingerprint per dep class) gates
  every `recompute_economy` call behind a staleness check AND a fingerprint equality check.
  Economy is only recomputed when a dependency actually changed, not every frame. The concern
  in the review is not present in the live code.
- **Fix:** None needed. Consider adding a comment at the review's cited line 306
  (`self.sector.economy = Arc::new(report)`) pointing to `ensure_fresh` as the guard.
- **Effort:** —
- **Risk / deps:** None.

---

### D7 — dead `let _ = si;` in `RemoveWorld::apply`
- **Review sev / bucket:** LOW / P3
- **Status:** 🔄 Moved (line drift — still present, different line)
- **Bus verdict (bypass findings only):** —
- **Location:** `builder/src/builder/command.rs:478` (verified; review cited :478 — line unchanged)
- **Evidence:**
  ```rust
  let _ = si; // silence unused warning under some configs
  ```
- **Why it matters:** `si` is the outer loop index from `sector.systems.iter().enumerate()`.
  It is indeed unused — the loop body uses `sys.worlds` and `pos` but never `si`.
  The `let _ = si` suppression hides a stylistic smell; prefixing the variable `_si` in
  the loop pattern or removing the enumeration entirely is cleaner.
- **Fix:** Change `for (si, sys) in sector.systems.iter().enumerate()` to
  `for sys in &sector.systems`, removing both the `si` binding and the silencer.
- **Effort:** S
- **Risk / deps:** None — `si` is not used for anything.

---

### D8 — dead `sys_idx` BTreeMap in `recompute_economy`
- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Bus verdict (bypass findings only):** —
- **Location:** `builder/src/builder/state/derivations.rs:200–202,311` (verified; review cited :311)
- **Evidence:**
  ```rust
  let mut sys_idx: BTreeMap<sectorforge::ids::SystemId, usize> = BTreeMap::new();
  for (i, s) in report.systems.iter().enumerate() {
      sys_idx.insert(s.system_id.clone(), i);
  }
  // … 100+ lines later …
  let _ = sys_idx;
  ```
- **Why it matters:** `sys_idx` is built (allocating a full BTreeMap of SystemId → index),
  then silenced. It was presumably intended for a fast lookup during the override-application
  pass, but the actual override loop iterates `report.worlds` directly without using it.
  Dead allocation on every economy recompute.
- **Fix:** Delete the `sys_idx` construction (lines 200–203) and the `let _ = sys_idx;` silencer
  (line 311).
- **Effort:** S
- **Risk / deps:** None — confirm no other code path in the function reads `sys_idx` (search
  confirms it is not used between construction and silencer).

---

### D9 — no `size_of` guard or `large_enum_variant` lint on `BuilderCommand`
- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Bus verdict (bypass findings only):** —
- **Location:** `builder/src/builder/command.rs:94` (enum declaration, verified)
- **Evidence:**
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub enum BuilderCommand {
      AddSystem { … }
      RemoveSystem { before: Option<Box<GeneratedSystem>>, removed_routes: Vec<GeneratedRoute>, … }
      // … 33 more variants …
  }
  ```
- **Why it matters:** `BuilderCommand` is cloned into the undo ring buffer (one clone per
  command, up to 200 entries). Some variants carry heap-allocated payloads
  (`Box<GeneratedSystem>`, `Vec<GeneratedRoute>`) which are cheap to clone via `Box`/`Vec`.
  However, the inline (non-heap) fields of the largest variants (e.g. `AutoAssignArchetypes`
  which holds `Vec<(SystemId, ArchetypeState)>`) contribute to enum discriminant size.
  Without a `static_assertions::assert_eq_size!` or `#[deny(clippy::large_enum_variant)]`
  check, a future contributor could add a large inline payload and silently inflate every
  ring-buffer clone. The repo's two existing `#[allow(large_enum_variant)]` attributes are in
  other crates (noted in IMPROVEMENT_REVIEW baseline), not here.
- **Fix:** Add a `static_assertions::const_assert!(std::mem::size_of::<BuilderCommand>() < 256)`
  guard just after the enum, or add a targeted `#[deny(clippy::large_enum_variant)]` attribute
  above the enum definition. The exact threshold can be tuned after measuring.
- **Effort:** S
- **Risk / deps:** Requires `static_assertions` in `builder/Cargo.toml` if not already present,
  or reliance on the clippy lint.

---

### D10 — validation silently skipped when worlds catalog is missing
- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Bus verdict (bypass findings only):** —
- **Location:** `builder/src/builder/state/derivations.rs:493–498` (verified; review cited :493)
- **Evidence:**
  ```rust
  pub fn revalidate_now(&mut self) {
      self.validation_dirty_since = None;
      if let Some(input) = self.synthesize_project_input() {
          self.validation_report = Some(validate(&input));
      }
      // ← no else: validation_report stays stale/None silently
  }
  ```
- **Why it matters:** When `synthesize_project_input` returns `None` (worlds catalog missing),
  the validation report is simply not updated. `validation_dirty_since` is cleared, so the
  debounce timer stops. The status bar shows the previous validation state (or nothing) with
  no indication that validation was skipped. A user who has not loaded a `worlds.toml` will
  see no validation feedback and no indication why.
- **Fix:** In the `else` branch, push a synthetic `ValidationReport` with a single warning
  "Validation skipped — no worlds catalog loaded", or set a dedicated
  `last_validation_skip_reason` string field and surface it in the status bar.
- **Effort:** S
- **Risk / deps:** Small; no downstream callers break. Requires deciding where the hint
  surfaces (status bar is the natural location given how `last_save_error` /
  `last_catalog_error` are handled).

---

### D11 — `EditChronicle` vs `recompute_chronicle` precedence undefined
- **Review sev / bucket:** LOW / P0
- **Status:** ✅ Confirmed
- **Bus verdict (bypass findings only):** Needs owner decision
- **Location:** `builder/src/builder/command.rs:817` (`EditChronicle::apply`),
  `builder/src/builder/state/derivations.rs:358` (`recompute_chronicle`); (both verified)
- **Evidence:**
  ```rust
  // command.rs:817 — EditChronicle lands sector.chronicle on the undo stack
  Self::EditChronicle { before, after } => {
      *before = Some(Box::new(sector.chronicle.clone()));
      sector.chronicle = (**after).clone();
      Ok(())
  }
  // derivations.rs:358 — recompute_chronicle writes sector.chronicle off-bus
  self.sector.chronicle = report;   // preserves manual events but not on undo stack
  ```
- **Why it matters:** The undo log captures the chronicle state at the moment `EditChronicle`
  was dispatched. If `recompute_chronicle` fires after that (e.g. because `history_auto_recompute`
  is on and a system edit triggers it), the live `sector.chronicle` advances past the snapshot.
  Undoing the `EditChronicle` restores the snapshot from before the user's manual edit, not
  the post-recompute state — so the manual event the user pinned is removed by the undo.
  This is a silent data-loss path when `history_auto_recompute = true`.
  The risk is low in the default case (`history_auto_recompute` defaults to `false` per
  `state/mod.rs:418` doc comment) but undefined for users who enable it.
- **Fix (option A — document):** Add a comment at `history_auto_recompute` stating that
  enabling it makes chronicle recomputes non-undoable, and that `EditChronicle` snapshots
  may become stale after an auto-recompute. Acceptable if auto-recompute stays off by default
  and is treated as a power-user option.
- **Fix (option B — route through bus):** Have `recompute_chronicle` dispatch
  `BuilderCommand::EditChronicle { before: current_chronicle, after: recomputed }` through
  the command bus. This makes recomputes undoable and removes the precedence ambiguity.
- **Effort:** S (option A) / M (option B)
- **Risk / deps:** Option B enlarges the undo log with auto-triggered entries — owner should
  decide if catalog-triggered recomputes should be undoable (same decision as D2).

---

## Suggested local order

1. **D7 + D8** (S, zero risk) — delete dead `si` enumeration and `sys_idx` map from `recompute_economy`. Warm-up; confirms golden tests still pass.
2. **D10** (S) — surface a hint when validation is skipped due to missing worlds catalog.
3. **D9** (S) — add `size_of` guard on `BuilderCommand`.
4. **D4** (S) — recompute incident-route `controls` in `MoveSystem`/`SwapSystems` apply arms.
5. **D11 + D2** (S/M, owner decision first) — document or bus-route chronicle precedence; decide whether catalog-triggered recomputes should be undoable (these two share the same architectural question).
6. **D1** (M, P0 bus bypass) — add `AddRegion`/`PaintRegion`/`EraseRegion`/`RemoveRegion` commands; update map paint panel callers.
7. **D-S1 / D3** (L, same fix) — `Command` trait or paired-arm macro; land after D1 so new commands are included in the new structure.
8. **D-S3 / D5** (L) — fold panel scratch into sibling `*State` structs; best done after the god-file split in D3 so the struct boundary is already reduced.
