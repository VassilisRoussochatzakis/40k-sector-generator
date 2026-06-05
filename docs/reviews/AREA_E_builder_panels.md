# AREA E — builder panels — verification

Dated 2026-06-05. Scope: `builder/src/builder/panels/` (45 files). Primary god-files: `control.rs` (1807 LOC), `world.rs` (1625 LOC), `system.rs` (2033 LOC), `history.rs` (1814 LOC), `routes.rs` (1533 LOC). Context-menu sub-module: `map/context_menu.rs` (1152 LOC).

## Summary table

| ID    | Sev  | Status         | Effort | One-line                                               |
|-------|------|----------------|--------|--------------------------------------------------------|
| E-S1  | HIGH | ✅ Confirmed   | S      | `labeled()` ×33 byte-identical copies across panels   |
| E-S2  | MED  | ✅ Confirmed   | M      | Master-detail shell repeated across catalog panels     |
| E-S3  | HIGH | ⚠️ Partial     | M      | `EditWorld` ×16 / `EditSystem` ×10 (review had ×26/×9)|
| E1    | HIGH | ✅ Confirmed   | M      | `apply_faction_power` + manual dirty: REAL bus bypass  |
| E2    | HIGH | ✅ Confirmed   | M      | Per-frame `primary_factions =` write: REAL bus bypass  |
| E3    | HIGH | ✅ Confirmed   | S      | Same finding as E-S1; cross-reference                  |
| E4    | MED  | ✅ Confirmed   | M      | `world.rs` 1625 LOC; 9 `format!("{:?}")` key sites     |
| E5    | MED  | ✅ Confirmed   | S      | `show_add_presence_row` + chip colours duplicated       |
| E6    | MED  | ✅ Confirmed   | S      | `SYSTEM_STATES` const duplicated byte-for-byte         |
| E7    | MED  | ✅ Confirmed   | L      | `system.rs` 2033 LOC god-file                          |
| E8    | —    | 🟢 Non-issue   | —      | "twice per frame" false — site 2 is collapsed-gated + dominated|
| E9    | MED  | ✅ Confirmed   | S      | `chronicle.events.clone()` ×2 per frame                |
| E10   | MED  | ✅ Confirmed   | M      | Filter/list block ~273 lines in `control.rs`           |
| E11   | LOW  | ⚠️ Partial     | M      | `context_menu.rs` 1152 LOC (not 177); 5 large render fns|
| E12   | LOW  | ✅ Confirmed   | M      | `search.rs::show` fn is ~307 lines (matches review)    |
| E13   | LOW  | ✅ Confirmed   | S      | Catalog dirty boilerplate ×6 panels (12 total sites)   |
| E14   | LOW  | ✅ Confirmed   | S      | `claim_chip_colours` byte-identical in 2 files         |

---

### E-S1 — `labeled()` copy-pasted across all panel files

> ✅ **RESOLVED 2026-06-05** — hoisted to `gui_core::ui_kit::labeled`; all 33
> private copies removed. Fixed together with E3 (same finding). See
> [PROGRESS.md](PROGRESS.md).

- **Review sev / bucket:** HIGH / P1 #1
- **Status:** ✅ Confirmed
- **Count:** 33 files contain a private `fn labeled(ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui))` definition (not 32 as stated — confirmed with `grep -rln "fn labeled\b"`). The body is byte-identical across all of them.
- **Evidence:**
  ```rust
  // control.rs:100  AND  world.rs:205  AND every other panel — identical:
  fn labeled(ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui)) {
      ui.horizontal(|ui| {
          let h = ui.spacing().interact_size.y;
          ui.add_sized(
              [140.0, h],
              egui::Label::new(RichText::new(label).color(palette::chrome_text_dim())),
          ).on_hover_text(help);
          add(ui);
  ```
- **Why it matters:** Any label-width or color tweak must be applied to 33 files by hand; one missed instance creates a visual inconsistency that is impossible to catch with clippy.
- **Fix:** Add `pub fn labeled(...)` to `gui-core/src/ui_kit.rs` (or a new `gui-core/src/ui_kit/form.rs`); do a global search-and-replace in builder panels removing the private copies and importing `gui_core::ui_kit::labeled`. Note: `gui-core/src/` has no private `fn labeled` today — it does not yet exist there.
- **Effort:** S
- **Risk / deps:** None; purely additive. Do first — shrinks every god-file before the split work.

---

### E-S2 — Master-detail shell repeated across catalog panels

- **Review sev / bucket:** MED / P1
- **Status:** ✅ Confirmed
- **Evidence:** `missions.rs`, `personae.rs`, `hooks.rs`, `relations.rs`, `economy.rs` all implement the same three-zone layout: (1) a filtered/sorted list on the left, (2) a detail editor on the right keyed on `state.selected_*_id`, (3) an "add new row" section with a scratch buffer whose type varies per catalog. Each also duplicates the `id_buf`/`edit_target` dance for inline-rename.
- **Why it matters:** Adding a list-level feature (e.g. multi-select delete) requires touching 5+ files independently. Catalog panel count will grow.
- **Fix:** Extract `roster_detail(ui, items, selected_id, |ui, item| { ... })` + a generic `add_row_scratch<T: Default>(ui, key, |buf| { ... })` helper in `builder/src/builder/ui/`. Eight panels would benefit.
- **Effort:** M
- **Risk / deps:** Requires settling on the scratch-buffer lifetime story (currently `ui.data_mut` temp storage vs `BuilderState` fields); resolve before extracting.

---

### E-S3 — `EditWorld` / `EditSystem` clone-mutate-dispatch idiom

> ✅ **RESOLVED 2026-06-05** — added `BuilderState::edit_world` / `edit_system`
> (clone → run closure → dispatch `EditWorld`/`EditSystem` with `before: None`)
> in [generation_ops.rs](../../builder/src/builder/state/generation_ops.rs).
> Converted **16 of the 26** call sites (10 `EditWorld` + 6 `EditSystem`); the
> other **10** are genuinely divergent and left hand-written + noted (UI-built
> drafts, full-payload graft, no-op-skip bulk loops, the `dominance_locked`
> side-table). Helpers **return** the bus error so each site keeps its exact
> modal text (the strings differ). Round-trip tests added. See
> [PROGRESS.md](PROGRESS.md).

- **Review sev / bucket:** HIGH / P1 #3
- **Status:** ⚠️ Partial → ✅ Resolved (partial-by-design)
- **Count (E-S3):** `BuilderCommand::EditWorld` has **16 call sites** in panels (not 26); `BuilderCommand::EditSystem` has **10 call sites** (matches review's ×9 approximately). `ModalKind::Message(format!(...))` appears **119 times** across all of `builder/src/` (23 within panels alone was understated; the full 119 figure includes the entire builder). The review's ×26 likely double-counted the `command.rs` application arm plus viewer; actual panel-only EditWorld = 16.
- **Evidence:** Canonical pattern in `world.rs:1077–1082`:
  ```rust
  let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
  let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
  draft.claims.remove(i);
  if let Err(e) = state.run(BuilderCommand::EditWorld {
      world: wid, before: None, after: Box::new(draft),
  }) { state.modal = Some(ModalKind::Message(format!("Edit failed: {e}"))); }
  ```
- **Why it matters:** 16 repetitions of the clone-mutate-dispatch-or-modal pattern; any change to error surfacing or before/after semantics must touch every site.
- **Fix:** `impl BuilderState { fn edit_world(&mut self, wid: WorldId, f: impl FnOnce(&mut GeneratedWorld)) -> Result<(),BuilderError> }` that clones, runs the closure, dispatches `EditWorld`, and maps errors to `self.modal`. Similarly `edit_system`. Reduces 16 blocks to 16 one-liners.
- **Effort:** M
- **Risk / deps:** Must handle the `before: None` convention; confirm no callers need the old snapshot for diffing.

---

### E1 — `apply_faction_power` off the command bus

> ✅ **RESOLVED 2026-06-05** — added `BuilderCommand::ApplyFactionPower`
> (`dep_classes=[Factions]`, `before` captured on apply, `revert` restores);
> the "↺ Apply to faction totals" button now routes through `state.run`.
> Round-trip test added. See [PROGRESS.md](PROGRESS.md).

- **Review sev / bucket:** HIGH / P0
- **Status:** ✅ Confirmed
- **Bus verdict:** **REAL bypass.** `GeneratedFaction::power: PowerProfile` carries `#[serde(default)]` (confirmed `src/model/sector_model/mod.rs:854–855`) — it is document state that serializes into `sector.json`. The call at `control.rs:957` directly mutates `state.sector.factions[*].power` without a `BuilderCommand`, only setting `state.dirty = true` afterward.
- **Location:** `builder/src/builder/panels/control.rs:956–959` (verified; no drift)
- **Evidence:**
  ```rust
  // control.rs:956–959
  let power = aggregate_faction_power(&state.sector.systems);
  sectorforge::control::apply_faction_power(&mut state.sector_mut().factions, &power);
  state.dirty = true;
  state.mark_validation_dirty();
  ```
- **Why it matters:** Pressing "↺ Apply to faction totals" is not undoable — undo stack sees no command, so Ctrl+Z cannot revert the power-total overwrite.
- **Fix:** Add `BuilderCommand::ApplyFactionPower { before: Vec<PowerProfile>, after: Vec<PowerProfile> }`. The `apply` arm calls `apply_faction_power`; `revert` restores the saved snapshots. Replace the three lines above with `state.run(BuilderCommand::ApplyFactionPower { ... })`.
- **Effort:** M
- **Risk / deps:** Needs `PowerProfile: Clone` (already is). Low risk; isolated to one button handler.

---

### E2 — Per-frame `primary_factions` write off the command bus

> ✅ **RESOLVED 2026-06-05** — replaced the per-frame off-bus write with a
> change-gated, dirty-tracked passive reconcile kept **off** the undo bus
> (README option *a*, mirroring the LD4 chronicle §R4 carve-out); the active
> Re-derive button stays on-bus. See [PROGRESS.md](PROGRESS.md) for the
> option-(a)-vs-(b) rationale.

- **Review sev / bucket:** HIGH / P0
- **Status:** ✅ Confirmed
- **Bus verdict:** **REAL bypass.** `GeneratedSystem::primary_factions: Vec<FactionId>` is serialized (`src/model/sector_model/mod.rs:146–147`, `#[serde(default, skip_serializing_if = "Vec::is_empty")]`) — definitively document state. The write at `control.rs:769` occurs every frame the system panel is open and the lock is off, with no `BuilderCommand` and no `state.dirty` call.
- **Location:** `builder/src/builder/panels/control.rs:768–770` (verified)
- **Evidence:**
  ```rust
  // control.rs:768–770
  if !locked {
      state.sector.systems[sys_idx].primary_factions = derived.clone();
  }
  ```
- **Why it matters:** The undo stack never sees these per-frame overwrites. A user who manually edits `primary_factions`, then navigates away and back with the lock off, silently loses their edit. The "Re-derive" button at line 796–807 already goes through `EditSystem` correctly (§R4 carve-out applies there), but the unconditional frame write does not.
- **Fix:** Remove the per-frame write. Instead, keep `primary_factions` at its stored value when `!locked` and display the `derived` list as a read-only preview. Only commit `derived` via `EditSystem` when the derivation result actually differs from the stored value (compare before dispatching to avoid no-op commands every frame).
- **Effort:** M
- **Risk / deps:** Must audit any downstream code that reads `system.primary_factions` expecting the auto-derived value in real time — switch those to call `derive_system_control()` directly.

---

### E3 — `labeled()` duplicated (×33)

> ✅ **RESOLVED 2026-06-05** — added `pub fn labeled` to `gui-core/src/ui_kit.rs`
> (beside `field`) and removed the private `fn labeled` from all 33 panel files,
> folding the import into each file's existing `ui_kit` use + cleaning 9 now-unused
> imports. Workspace clippy clean, builder 317/317, `it` 93/93. Closes **E-S1**.

- **Review sev / bucket:** HIGH / P1 #1
- **Status:** ✅ Confirmed
- **Count (E3):** 33 files (one more than the review's ×32). Cross-reference E-S1 — same finding, same fix.
- **Evidence:** See E-S1 above.
- **Why it matters:** Same as E-S1.
- **Fix:** Same as E-S1. This finding and E-S1 are identical; fix once, close both.
- **Effort:** S
- **Risk / deps:** None.

---

### E4 — `world.rs` 1625-line god-file + `format!("{:?}")` storage keys

- **Review sev / bucket:** MED / P1.5 + P2
- **Status:** ✅ Confirmed
- **Location:** `builder/src/builder/panels/world.rs` (1625 lines confirmed by `wc -l`)
- **Count (E4):** 9 occurrences of `format!("{…:?}")` used as string storage keys or lookup keys — 2 ad-hoc call sites (`line 556`: feature key for `already` set; `line 676`: `feature_pool` BTreeMap key), 1 string-comparison site (`line 709`), and 7 `EnumPicker::debug_key` trait impls (`lines 1466, 1477, 1488, 1499, 1510, 1521, 1532`) — not ×12 as stated. The review's count of 12 likely included internal EnumPicker and the call sites differently.
- **Evidence:**
  ```rust
  // world.rs:556 — feature key derived from Debug repr:
  let key = format!("{v:?}");
  // world.rs:1465–1467 — EnumPicker::debug_key for WorldType:
  fn debug_key(&self) -> String {
      format!("{self:?}")
  }
  // world.rs:709 — round-trip match:
  .find(|v| format!("{v:?}") == key)
  ```
- **Why it matters:** If any variant's `Debug` repr changes (e.g., a rename or a `#[debug(...)]` attribute), the storage key silently breaks the feature-weight lookup without a compile error.
- **Fix:** (a) Give `NotableFeature` an explicit `as_slug() -> &'static str` (or use `strum::AsRefStr`) and replace the `format!("{v:?}")` call sites with `v.as_slug()`. (b) Split `world.rs` into `world/identity.rs`, `world/environment.rs`, `world/society.rs`, `world/features.rs`, `world/claims.rs`.
- **Effort:** M
- **Risk / deps:** The `EnumPicker::debug_key` trait is local to `world.rs`; its callers are also in `world.rs`. The real risk is the `already` set and `feature_pool` lookup — both must be updated together, plus the `find` on line 709.

---

### E5 — Presence table + chip colours duplicated across `control.rs` and `world.rs`

- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed
- **Location:** `control.rs:574` (`fn show_add_presence_row`) and `world.rs:923` (`fn show_add_presence_row`) — two private functions with the same name and similar structure in different files. `claim_chip_colours` is also duplicated (see E14).
- **Evidence:**
  ```rust
  // control.rs:574:
  fn show_add_presence_row(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
  // world.rs:923:
  fn show_add_presence_row(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
  ```
- **Why it matters:** Presence editing logic diverges between the CONTROL and WORLD tabs with no structural guarantee of parity. A fix to one (e.g., adding an "intel" field) must be manually mirrored to the other.
- **Fix:** Extract a shared `presence_widgets.rs` module under `builder/src/builder/panels/` (or `builder/src/builder/ui/`) exporting `show_add_presence_row`, `claim_chip`, and related helpers. Both `control.rs` and `world.rs` import from it.
- **Effort:** S
- **Risk / deps:** The two functions may differ in small UI details (scroll area IDs, section headers); audit before merging.

---

### E6 — `SYSTEM_STATES` duplicated in `history.rs` and `control.rs`

- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed
- **Location:** `history.rs:60` and `control.rs:69` (verified; no drift)
- **Evidence:**
  ```rust
  // Identical in both files:
  const SYSTEM_STATES: &[SystemState] = &[
      SystemState::Pacified, SystemState::Fragmented, SystemState::Blockaded,
      SystemState::Warzone, SystemState::Infiltrated, SystemState::Quarantined,
      SystemState::Uncharted,
  ];
  ```
- **Why it matters:** Adding a new `SystemState` variant requires updating the constant in two places; one missed site gives a silent filter gap.
- **Fix:** Move to a single `pub(crate) const SYSTEM_STATES` in `builder/src/builder/panels/mod.rs` (or a new `builder/src/builder/panels/shared_consts.rs`). Both files import it.
- **Effort:** S
- **Risk / deps:** Check `DOMINANCE_STATES` and `INFLUENCE_TIERS` for the same pattern — they appear multiple times in `control.rs` only, so lower priority.

---

### E7 — `system.rs` 2033-line god-file

- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed
- **Location:** `builder/src/builder/panels/system.rs` — exactly 2033 lines (verified)
- **Evidence:** File contains archetype picker, system preview, coord/name/kind editing, star editor, tags/notes, and the three `EditSystem`-dispatch helper functions (`fn commit_archetype`, `fn commit_star`, `fn commit_system_field`) at lines 1809–1880.
- **Why it matters:** God-files make PR reviews opaque and increase the chance of merge conflicts on simultaneous edits.
- **Fix:** Split into `system/identity.rs` (name, coord, kind, tags, notes), `system/archetype.rs` (the preset picker + `commit_archetype`), `system/preview.rs` (the read-only summary panel). Re-export via `system/mod.rs` keeping the `pub(crate) fn show` entry point.
- **Effort:** L
- **Risk / deps:** Do after the content golden (G2) is in place. Low logic risk — purely mechanical split.

---

### E8 — `route_component_count` recomputed every frame

> 🟢 **NON-ISSUE 2026-06-05 — premise corrected, no change.** The "twice per
> frame" claim does not hold. Site 1 (`show_summary`, routes.rs:75) runs every
> frame — **one** union-find. Site 2 (`show_ensure_connected`, :1279) is inside
> `ui_kit::collapsing_section(.., false, ..)`, whose `CollapsingHeader::show`
> body runs **only when the section is expanded** (default-collapsed) — so it is
> not a per-frame cost; and when it does run it is immediately followed by
> `ensure_connected_routes(state, routes.clone())` (:1280), a heavier clone+connect
> pass that **dominates** the union-find, and it must stay **live** (it follows a
> checkbox handler that can mutate routes the same frame — hoisting to the top of
> `show` would make it a frame stale). There is no per-frame redundancy to remove.
> Reclassified MED → non-issue, marked DONE. See [PROGRESS.md](PROGRESS.md).

- **Review sev / bucket:** MED / P2 (reclassified → non-issue)
- **Status:** 🟢 Non-issue (premise corrected)
- **Location:** `builder/src/builder/panels/routes.rs:75` and `:1279` (verified)
- **Evidence:**
  ```rust
  // routes.rs:91–92  (show_summary, called every frame):
  fn show_summary(ui: &mut Ui, state: &BuilderState) {
      let components = route_component_count(&state.sector, &state.sector.routes);
  // routes.rs:1296 (second call site in the same tab):
      let components = route_component_count(&state.sector, &state.sector.routes);
  ```
- **Why it matters:** `route_component_count` (line 1417) runs a union-find over all routes — O(R·α(S)) — called twice per frame whenever the ROUTES tab is visible. On large sectors this is measurable.
- **Fix:** Cache the count in `BuilderState` derivations (keyed on a `blake3` digest of `sector.routes`), or in a `ui.data` temp value keyed on the routes digest. Alternatively, compute once at the top of `show` and pass by value to sub-functions.
- **Effort:** M
- **Risk / deps:** The derivations system in `state/derivations.rs` is the cleanest home; requires adding a new staleness key.

---

### E9 — `chronicle.events.clone()` twice per frame

- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed
- **Location:** `builder/src/builder/panels/history.rs:582` and `:1391` (verified)
- **Evidence:**
  ```rust
  // history.rs:582 — inside a scroll area rendered every frame:
  let events = state.sector.chronicle.events.clone();
  for ev in &events { ... }
  // history.rs:1391 — second clone in the timeline section:
  let events = state.sector.chronicle.events.clone();
  ```
- **Why it matters:** Cloning a potentially large `Vec<ChronicleEvent>` twice per frame is unnecessary. Each event has `Arc<str>` fields so the clone is non-trivial.
- **Fix:** Replace both with `&state.sector.chronicle.events` and iterate by reference. The loop bodies only read events; no mutation is needed on the cloned copy.
- **Effort:** S
- **Risk / deps:** Confirm no iterator-based borrow conflicts with `state` in the loop body (the mutable borrows on `state` happen only via button `.clicked()` responses, which are evaluated after the borrow ends in egui's immediate-mode model). Low risk.

---

### E10 — Filter/list block >273 lines in `control.rs`

- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed
- **Location:** `builder/src/builder/panels/control.rs:1203` (`fn show_world_list`), `:1288` (`fn show_world_row`), `:1393` (`fn show_add_claim_row`) — combined ~273 lines (review said ">150-line filter/list fns"; actual is larger)
- **Evidence:** `show_world_list` (1203–1287, ~84 lines) contains a full filter bar with two temp-data round-trips; `show_world_row` (1288–1392, ~104 lines) is the per-world chip-row renderer; `show_add_claim_row` (1393–1478, ~85 lines) is the add-claim form.
- **Why it matters:** The filter-bar pattern (text input → store in `ui.data` → read back → filter list) repeats in multiple panels. Extracting it reduces per-frame `ui.data_mut` round-trips and makes the filter behavior consistent (e.g., debounce, clear button).
- **Fix:** Extract `fn filter_bar(ui, salt, hint) -> String` that encapsulates the `Id`-keyed `ui.data_mut` get/store cycle. Pull `show_world_row` and `show_add_claim_row` into a new `control/claims.rs` sub-module.
- **Effort:** M
- **Risk / deps:** Filter bar helper is safe. Sub-module split has no logic change; do after E-S1 to reduce churn.

---

### E11 — `map/context_menu.rs` large render functions

- **Review sev / bucket:** LOW / P3
- **Status:** ⚠️ Partial
- **Location:** `builder/src/builder/panels/map/context_menu.rs` — 1152 lines total (review stated "177-line menu builders", which is a significant undercount of the file but may refer to a single function like `render_empty_hex_menu` at ~53 lines or `render_route_menu` at ~85 lines; the **largest** individual builder is `render_multi_selection_menu` at lines 739–888, ~149 lines)
- **Count:** Five `render_*` functions: `render_empty_hex_menu` (~53 L), `render_system_menu` (~138 L), `render_multi_selection_menu` (~149 L), `render_route_menu` (~85 L), `render_region_hex_menu` (~75 L). The overall file at 1152 lines is itself a god-file.
- **Evidence:** `fn render_system_menu(ui, state, id, coord) -> bool` spans lines 600–738, building the menu imperatively item by item with repeated `ui.selectable_label(...).clicked()` blocks.
- **Why it matters:** Menu items and their actions are defined in two places: `SectorMenuAction` enum and the `render_*` functions. A new action requires editing both. Table-driving the items would make `SectorMenuAction` the single source of truth.
- **Fix:** Introduce a `MenuItem { label, action: SectorMenuAction, enabled: bool }` struct and build `Vec<MenuItem>` from the sector state; render generically. This is more involved than the review implies — treat as P3, after god-file splits.
- **Effort:** M
- **Risk / deps:** `apply_sector_menu_action` at line 234 already handles `SectorMenuAction` dispatch. The enabled/disabled logic per item is context-sensitive and must be retained correctly.

---

### E12 — `search.rs::show` fn is ~307 lines

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `builder/src/builder/panels/search.rs:37` — `pub fn show` runs from line 37 to line 344, approximately 307 lines (review stated "308-line show fn")
- **Evidence:** The function starts at line 37 and the next top-level function is `fn show_outcome` at line 345. The file is 1111 lines total and has additional helper fns (`constraint_editor`, `row_faction`, `world_type_combo`, etc.) below line 345.
- **Why it matters:** A 307-line immediate function mixing constraint-editor, result-list, and filter-controls is hard to review and test.
- **Fix:** Extract `fn show_constraint_list(ui, state)`, `fn show_filter_controls(ui, state)`, `fn show_run_controls(ui, state)` from the monolithic `show`. The existing `show_outcome` at line 345 is already extracted correctly — apply the same discipline to the entry point.
- **Effort:** M
- **Risk / deps:** Purely cosmetic split; no logic change. Low risk.

---

### E13 — Catalog dirty-marking boilerplate ×6 panels

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Count (E13):** 6 catalog panels contain the boilerplate pattern; total of **12 call sites** with `state.dirty_files.insert`:
  - `personae.rs`: 2 sites
  - `relations.rs`: 3 sites
  - `missions.rs`: 2 sites
  - `hooks.rs`: 2 sites
  - `economy.rs`: 1 site
  - `worlds_editor.rs`: 2 sites
- **Evidence:**
  ```rust
  // personae.rs:838–842 (representative):
  state.dirty = true;
  if let Some(rel) = state.config.inputs.personae.clone() {
      state.dirty_files.insert(rel);
  } else {
      state.dirty_files.insert(DEFAULT_PERSONAE_PATH.into());
  }
  ```
- **Why it matters:** The fallback-to-default-path logic must be replicated at every catalog save site. A new catalog path config field requires updating 6 panels.
- **Fix:** `impl BuilderState { fn mark_catalog_dirty(&mut self, configured: Option<&Utf8PathBuf>, default: &str) }` encapsulating the `if let Some / else` branch. Each panel calls `state.mark_catalog_dirty(state.config.inputs.personae.as_ref(), DEFAULT_PERSONAE_PATH)`.
- **Effort:** S
- **Risk / deps:** None. Safe mechanical extraction.

---

### E14 — `claim_chip_colours` byte-identical in `world.rs` and `control.rs`

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `world.rs:1197` and `control.rs:1479` (verified; no drift)
- **Evidence:**
  ```rust
  // Both files — completely identical 13-arm match:
  fn claim_chip_colours(kind: ClaimType) -> (Color32, Color32) {
      match kind {
          ClaimType::LegalSovereignty => (Color32::from_rgb(40, 60, 100), Color32::LIGHT_BLUE),
          ClaimType::ImperialMandate  => (Color32::from_rgb(80, 70, 30),  Color32::YELLOW),
          // … 9 more arms …
          _ => (Color32::from_rgb(50, 50, 60), Color32::LIGHT_GRAY),
      }
  }
  ```
- **Why it matters:** Adding a new `ClaimType` variant requires updating two files and the catch-all `_` arm silently swallows it.
- **Fix:** Move `claim_chip_colours` (and the full chip-rendering block) into the shared `presence_widgets.rs` module proposed in E5. Export as `pub(crate) fn claim_chip_colours` and delete both private copies.
- **Effort:** S
- **Risk / deps:** Can be done as part of E5; no independent deps.

---

## Notes on intentional design (not bugs)

**Catalog panels writing `data_catalogs.*` directly:** Confirmed intentional per `worlds_editor.rs:4` ("world-data editing never routes through the command bus") and `worlds_editor.rs:338` ("worlds catalogue bypasses the undo bus"). This is a documented architectural carve-out — not flagged as a bug.

**`ModalKind` is a single enum:** Confirmed — `builder/src/builder/state/types.rs:23` defines one `pub enum ModalKind` with variants `NewProject`, `OpenProject`, `SaveAs`, `PlaceSystem`, `Message`, `ConfirmDestructive`, etc. No bool-flag modal sprawl; the review's note is accurate.

---

## Suggested local order

1. **E-S1 / E3** — extract `gui_core::ui_kit::labeled`; mechanical, zero risk, shrinks every god-file before splits. Do first.
2. **E1** — add `BuilderCommand::ApplyFactionPower`; isolated P0 fix, single button handler.
3. **E2** — remove the per-frame `primary_factions =` overwrite; requires a small design call (preview vs. commit semantics) but is self-contained in `control.rs`.
4. **E6 + E14** — deduplicate `SYSTEM_STATES` and `claim_chip_colours`; S-effort, safe, good warm-up for E5.
5. **E5** — extract `presence_widgets.rs`; depends on E14 being done first.
6. **E9** — remove `chronicle.events.clone()` x2; S-effort hot-path fix.
7. **E13** — add `mark_catalog_dirty` helper; S-effort, mechanical.
8. **E-S3** — add `edit_world` / `edit_system` helpers; M-effort, reduces 26 call sites.
9. **E8** — memoize `route_component_count`; M-effort, requires derivations staleness key.
10. **E10** — extract `filter_bar` helper + `control/claims.rs`; M-effort, after E-S1.
11. **E12** — split `search.rs::show`; M-effort, cosmetic.
12. **E4** — add `as_slug()` to `NotableFeature`; M-effort, coordinate with B-S3 `enum_slug!` macro work.
13. **E7** — split `system.rs`; L-effort, requires G2 content golden first.
14. **E11** — table-drive context menu; M-effort, P3, do last.
15. **E-S2** — generic roster-detail helper; M-effort, design discussion needed first.
