---
title: FIELD_REVIEW — Builder & Viewer per-tab dead/improper field audit
generated: 2026-06-02
scope: sectorforge-builder (24 tabs) + sectorforge-viewer (12 app views + 6 editor tabs)
method: mechanical reference-count over all state fields, then per-tab read of every panel
invariants_referenced: ["§R4 command-bus", "§U2 undo ring", "§V1/§V2 diagnostics", "CLAUDE.md determinism"]
status: review-only — no code changed
---

# FIELD_REVIEW

A field-level audit of every tab in the **builder** and **viewer** modules, looking for
(a) **dead/unused fields** and (b) **fields implemented improperly**.

## How to read this document

Findings use a stable, greppable row schema so this file can be diffed and re-run later:

```
[ID] | <crate-path>:<line> | <field-or-widget> | <CAT> | <SEV> | <finding> | <fix>
```

**Category (`CAT`)**

| Code | Meaning |
|---|---|
| `D` | Dead field — written but never read, or read but never set; or a widget value computed then discarded |
| `I` | Improper §R4 — a **live-sector** user edit written directly via `sector_mut()`/`state.sector.X =` instead of `state.run(BuilderCommand::…)`, so it is **not on the undo/redo log** |
| `X` | Inconsistent / duplicate — a suitable `BuilderCommand` already exists but this path writes directly; or two controls drive the same value |
| `N` | No-op widget — rendered but its `.changed()`/return is ignored, so interacting does nothing |
| `G` | Ignored input — a field/control is collected but the consuming logic never uses it |
| `S` | Stub — panel/section/field is a placeholder, or surfaced UI is unreachable |

**Severity (`SEV`)** — `High` (dead user-facing surface, or a documented invariant broken broadly) · `Med` (real edit silently un-undoable, or a control that lies about its effect) · `Low` (cosmetic, dead scratch field, doc drift).

**Confidence** — `✔confirmed` = verified by reading the code in this session; `~reported` = found by a per-tab sweep agent and consistent with the surrounding pattern, not individually re-read.

---

## Methodology

1. **Global reachability pass (mechanical).** Extracted all 154 `pub` fields of `BuilderState`
   (`builder/src/builder/state/mod.rs`) and all 28 fields of `EditorState`
   (`viewer/src/editor/state.rs`) and counted references, bucketed by directory
   (panels vs state-internals vs rest, editor vs rest-of-viewer). This finds **truly dead**
   fields — set/declared but read nowhere.
2. **§R4 bypass scan.** Grepped every panel for `sector_mut()` / `state.sector.X =` writes
   and cross-checked against `BuilderCommand` dispatches (42 `.run(` sites across panels).
3. **Per-tab read.** Every panel in `builder/src/builder/panels/**` (≈29k LoC) and every view
   in `viewer/src/**` (≈10k LoC) was read and classified against the schema above.
4. **Verification.** The headline findings (dead diagnostic panels, single-variant combo,
   stale stub comments, the CoW question) were re-read directly and are marked `✔confirmed`.

### One correction to a tempting-but-wrong claim

`LiveSector` implements `DerefMut` as `Arc::make_mut(&mut self.0)`
(`builder/src/builder/state/mod.rs:138-142`). Therefore **`state.sector.systems[i].x = …` and
`state.sector_mut().systems[i].x = …` are identical** — both trigger copy-on-write. Writing
through the bare `.sector` field is **not** a CoW/determinism hazard. The only defect in any of
the `I` findings below is the **undo/redo gap** (§R4 / §U2), never data corruption or a
broken background-export snapshot.

---

## Summary scoreboard

| Area | High | Med | Low | Clean tabs |
|---|---|---|---|---|
| Builder — core editing (map/system/world) | 1 | ~22 | 3 | map/* helpers, orbital, conflict |
| Builder — political (factions/control/intel/relations) | 0 | ~9 | 2 | relations (sector-clean) |
| Builder — spatial/economy (regions/routes/subsectors/economy) | 0 | 1 | 0 | routes, subsectors, economy, surface_regions |
| Builder — narrative overlays (history/personae/hooks/sites/missions/prose) | 0 | 6 | 2 | personae, hooks, sites, missions, prose (sector-clean) |
| Builder — runtime/meta (gen/search/diff/analytics/export/…) | 0 | 0 | 2 | 14 of 16 fully clean |
| Viewer — app views (12) | 0 | 3 | 2 | most views |
| Viewer — editor tabs (6) | 0 | 1 | 2 | most panels |
| **Totals** | **2** | **~42** | **13** | — |

The two `High` items are the headline takeaways; everything `Med` is dominated by a single
structural pattern (§R4 direct-write editors). Read the two cross-cutting sections first.

---

# Cross-cutting findings (read these first)

## XC-1 — `High` — The VALIDATION and INVARIANTS panels are unreachable dead UI ✔confirmed

`builder/src/builder/panels/nav.rs:84-88`:

```rust
// Validation + invariants are surfaced as collapsing footers on every
// tab so the user never has to leave the working surface to read the
// active diagnostics (§V1 / §V2).
let _ = validation::show;
let _ = invariants_panel::show;
```

These two lines reference the function **items** only to silence dead-code warnings — neither
function is ever *called*. There is **no `BuilderTab::Validation` / `BuilderTab::Invariants`**
variant (`state/types.rs:95-120`), and no other call site exists anywhere in `builder/src`.

- `validation.rs` (260 LoC) and `invariants.rs` (239 LoC) are fully implemented panels with
  per-error focus buttons that **the user can never open**.
- The underlying `validation_report` still drives the status-bar health pip via
  `status.rs` → `health_level()`, and `invariants.rs`/`validation.rs` still write the §V2
  selection mailbox — so the *data* is partly surfaced, but the *panels* are dead.
- The comment actively misdescribes the behaviour (claims footers render; they do not).

**Fix:** either invoke both panels as the promised footers in `show_active_panel`, or add
`BuilderTab::Validation` / `Invariants` variants, or delete the two panels + the misleading
comment. Pick one — today the code claims a feature it does not ship.

## XC-2 — `Med` — Detail editors mutate the sector directly, bypassing the command bus (§R4)

`CLAUDE.md` §R4: *"Mutations in the builder always go through the command bus… Never write
directly to `BuilderState` fields from inside a panel — that breaks undo/redo."* The deep
entity-detail editors were built with direct `sector_mut()` writes and never migrated. Each
write does call `state.dirty = true` + `mark_validation_dirty()`, so saving and validation are
fine — but **none of these edits can be undone or redone**, and in several cases a suitable
command already exists and is used elsewhere (the `X` dual-path inconsistency).

Affected panels (detail in the per-tab tables): `world.rs` (~12 sites — every world DTO field
plus tags/notes/factions/claims), `system.rs` (~6 — kind/star/tags/notes/bulk-primary-factions),
`system_map.rs` (2), `control.rs` (~7 — world-faction presence + claims + bulk convert),
`history.rs` (~6 — chronicle event edits + wizard add), `intel.rs` (2 + baseline derive),
`regions.rs` (1 — `apply_route_effects`), `map/context_menu.rs` (1 — post-`AddWorld` orbit pin).

**Not in scope of §R4 (correctly outside the bus, *not* defects):** `factions.rs`, `relations.rs`,
`personae.rs`, `hooks.rs`, `sites.rs`, `missions.rs`, `prose.rs` edit `data_catalogs.*`
(TOML mirrors tracked via `dirty_files`), **not** the live sector. Catalog edits are not
expected to be undoable, so their direct writes are by design.

**Fix (general):** route each live-sector edit through an existing or new `BuilderCommand`.
Highest value where the command **already exists**: `SetWorldOrbit`, `RenameWorld`,
`SetStar`/`SetStarSpectral`, `ReplaceRoutes` — these paths bypass commands that are already
wired and used by neighbouring code.

## XC-3 — `Low` — Two state-field doc comments are stale ✔confirmed

`builder/src/builder/state/mod.rs:455-460` describes `selected_persona_id` as *"PERSONAE panel
is a Phase D stub today"* and `selected_hook_id` as *"Mirrors the persona stub above."* Both are
false: `personae.rs` and `hooks.rs` are real panels, and both fields are fully wired —
row highlight + click-select (`personae.rs:208/232/259`, `hooks.rs:167/189/201/218`) and
cross-tab focus (`state/selection.rs:110/115/142/148`, covered by `state/tests.rs:295/299`).
**Fix:** delete the "stub" wording.

## XC-4 — `Low` — `stable_ids_on_rename` has no UI in either module ✔confirmed

Both `BuilderState::stable_ids_on_rename` (default `true`) and
`EditorState::stable_ids_on_rename` (default `true`, consumed at
`viewer/src/app/sector_view.rs:613` via `reindex_ids`) are **never toggled by any widget** —
zero panel references in the builder, none in the viewer editor. The §49 "stable vs compact
renumber" mode is therefore permanently stuck at `true`. **Fix:** add a checkbox
(builder preferences / viewer settings), or drop the field and hard-code `true` at the call site.

---

# Builder — per-tab findings

## MAP tab — `panels/map/*`, `nav.rs`

| ID | Location | Field/Widget | CAT | SEV | Finding | Confidence |
|---|---|---|---|---|---|---|
| MAP-1 | `panels/nav.rs:87-88` | `validation::show`, `invariants_panel::show` | S | High | See **XC-1** — referenced via `let _ =`, never called | ✔confirmed |
| MAP-2 | `panels/map/context_menu.rs:324` | world `orbit` after `AddWorld` | I/X | Med | Post-`AddWorld` orbit fixup writes `worlds[..].orbit` directly though `SetWorldOrbit` exists | ~reported |
| MAP-3 | `panels/map/context_menu.rs:607` | `coord` param | D | Low | Suppressed `let _ = coord;` in the AddRoute early-return branch — dead parameter | ~reported |

`panels/map/mod.rs`, `interactions.rs`, `dialogs.rs`, `theme.rs`, `cache.rs` — **CLEAN.**
All sector edits dispatch `AddSystem`/`RemoveSystem`/`AddRoute`/`MoveSystem`/`SwapSystems`/
`RenameSystem`/`RenameRegion`; `paint_region_hex`/`erase_region_hex` are the documented §D3
direct paths; `map_tool`, `hex_size`, `map_view_cache`, theme fields are transient/derived.

## SYSTEM / WORLD tabs — `system.rs`, `system_map.rs`, `world.rs`, `orbital.rs`, `conflict.rs`

| ID | Location | Field/Widget | CAT | SEV | Finding | Confidence |
|---|---|---|---|---|---|---|
| SYS-1 | `system.rs:505` | system `kind` | I | Med | "Apply kind" writes `systems[i].kind` directly; no `SetSystemKind` command | ~reported |
| SYS-2 | `system.rs:614-631` | system `star` | I/X | Med | Star section writes `sys.star` directly though `SetStar`/`SetStarSpectral` exist and are used by `system_map.rs` | ~reported |
| SYS-3 | `system.rs:669-685` | `tags`, `notes` | I | Med | Written on lost-focus directly; no command | ~reported |
| SYS-4 | `system.rs:740-746` | world `orbit` after `AddWorld` | I/X | Med | Direct orbit pin though `SetWorldOrbit` exists | ~reported |
| SYS-5 | `system.rs:1399-1415` | `primary_factions` (bulk add/clear) | I | Med | `apply_bulk_primary_faction`/`apply_bulk_clear_factions` mutate in place; no command (also reached from MAP right-click) | ~reported |
| SMAP-1 | `system_map.rs:301-313` | duplicate-world payload | I/S | Med | Whole world payload overwrite; comment admits "no `DuplicateWorld` command yet" — only the `AddWorld` is undoable | ~reported |
| SMAP-2 | `system_map.rs:348-350` | world `orbit` after `AddWorld` | I/X | Med | Same as SYS-4 | ~reported |
| WRL-1 | `world.rs:200-205` | world `orbit`, `name` | I/X | Med | Direct writes though `SetWorldOrbit` / `RenameWorld` exist | ✔confirmed |
| WRL-2 | `world.rs:232-268` | `star_colour(_code)`, `world_type` | I | Med | Classification combos write DTO directly; no command | ✔confirmed |
| WRL-3 | `world.rs:285-363` | `atmosphere`/`temperature`/`biosphere`/`population`/`tech_level`/`government` | I | Med | Six environment/society combos write DTO directly | ✔confirmed |
| WRL-4 | `world.rs:407-457` | `notable_features` add/remove | I | Med | Direct vec edits; no command | ~reported |
| WRL-5 | `world.rs:666-682` | world `tags`, `notes` | I | Med | Direct writes; no command | ~reported |
| WRL-6 | `world.rs:725-846` | world faction `presences` add/remove | I | Med | Direct vec edits; no command | ~reported |
| WRL-7 | `world.rs:890-970` | world `claims` add/remove | I | Med | Direct vec edits; no command | ~reported |

`orbital.rs` — **CLEAN** (all via `SetOrbitalAssets`/`SetBlockadeReport`, diff-before-dispatch).
`conflict.rs` — **CLEAN** (`SetWorldConflict`/`SetWorldStability`/`SetSystemConflict`;
`conflict_ticks_to_advance` is transient builder state — correct).

> Note: `world.rs` shows the §R4 gap most starkly — *every* `GeneratedWorld` field is editable
> and *none* of those edits are undoable, while `SetWorldOrbit`/`RenameWorld` sit unused for two
> of them.

## FACTIONS / CONTROL / INTEL / RELATIONS — `factions.rs`, `control.rs`, `intel.rs`, `relations.rs`

| ID | Location | Field/Widget | CAT | SEV | Finding | Confidence |
|---|---|---|---|---|---|---|
| CTL-1 | `control.rs:314-332` | world-faction `presences` edit/remove | I | Med | Direct `sector_mut()` presence edits (influence/intel_confidence/dimensions); no command | ✔confirmed |
| CTL-2 | `control.rs:428-444` | world-faction `presences` add | I | Med | Direct add-presence write; no command | ~reported |
| CTL-3 | `control.rs:505-539` | system `control.state`, `primary_factions` (Recompute button) | I | Med | Explicit-button writes bypass the bus | ~reported |
| CTL-4 | `control.rs:862` | bulk claim-type convert | I | Med | `apply_bulk_convert` rewrites claims across all worlds directly | ~reported |
| CTL-5 | `control.rs:1015-1085` | world `claims` add/remove | I | Med | Direct vec edits; no command | ~reported |
| CTL-6 | `control.rs:523-524`, `:676` | `primary_factions` per-frame, `apply_faction_power` | G | — | **Acceptable** re-derivations (recompute, not user edits) — listed so they are not re-flagged | ✔confirmed |
| INT-1 | `intel.rs:105`, `:134` | system/world `intel` | I | Med | Direct `sector_mut()` intel edits; no command | ~reported |
| INT-2 | `intel.rs:84` | `derive_intel(sector_mut())` | I/G | Low | "Generate baseline intel" bulk-rewrites intel; borderline derive-vs-edit — at minimum should be undoable or labelled non-undoable | ~reported |
| REL-1 | `relations.rs:614` | `u8_slider` `id_salt` param | D | Low | Accepted then `let _ = id_salt;` — dead parameter, misleading signature | ~reported |

`factions.rs` — **CLEAN re §R4**: all edits target `data_catalogs.factions` (catalog, tracked via
`dirty_files`), not the sector (see XC-2 carve-out). `relations.rs` — **CLEAN re §R4**: catalog +
config edits only; `relations_auto_recompute`/`relations_selected_pair` are live UI state.
`intel_observer`/`intel_player_min_confidence` are display-lens fields — correct as direct writes.

## REGIONS / ROUTES / SUBSECTORS / ECONOMY — `regions.rs`, `surface_regions.rs`, `routes.rs`, `subsectors.rs`, `economy.rs`

| ID | Location | Field/Widget | CAT | SEV | Finding | Confidence |
|---|---|---|---|---|---|---|
| REG-1 | `regions.rs:369-374` | `apply_route_effects` → `sector_mut().routes` | I/X | Med | "Apply effects to routes" button writes routes directly though `ReplaceRoutes` exists and handles exactly this elsewhere | ~reported |

`routes.rs` — **CLEAN** (verified by sweep): all four bulk-filter fields
(`route_bulk_filter_type/stability/tag/region`) are consumed by `route_matches_bulk`
(≈534-558); all four hidden-route fields (`kind/k_nearest/exclude_blackout/endpoints`) feed
`HiddenRoutesConfig` (≈885-888). `subsectors.rs` — **CLEAN**: all five override maps are both
populated and consumed by `apply_subsector_overrides`. `economy.rs` — **CLEAN**: all override
tables consumed; sector rewrites go through `recompute_economy` (derivation). `surface_regions.rs`
— **CLEAN** (via `SetSurfaceRegions`, diff-before-commit). *(`routes.rs:1165`'s
`state.sector.regions = …` is `#[cfg(test)]` scaffolding, not a panel mutation.)*

## NARRATIVE OVERLAYS — `history.rs`, `personae.rs`, `hooks.rs`, `sites.rs`, `missions.rs`, `prose.rs`

| ID | Location | Field/Widget | CAT | SEV | Finding | Confidence |
|---|---|---|---|---|---|---|
| HIS-1 | `history.rs:487/589/600` | chronicle `events[idx]` field edits | I | Med | Date/kind/weight/summary/narrative/factions/consequences edited directly via `sector_mut()`; no command | ~reported |
| HIS-2 | `history.rs:634/645` | event `remove`, `manual=true` pin | I | Med | Delete + manual-pin write the chronicle vec directly | ~reported |
| HIS-3 | `history.rs:911` | wizard commit `events.push` + sort | I | Med | New event pushed + sorted directly; should be `BuilderCommand::AddChronicleEvent` | ~reported |
| PER-1 | `personae.rs:259` | `personae_edit_target` | G | Low | Set on "edit" click but `show_manual_editor` never reads it to pre-expand the matching `[[manual]]` row | ~reported |
| XC-3 | `state/mod.rs:455-460` | `selected_persona_id`, `selected_hook_id` | D(comment) | Low | "stub" comments are stale — fields fully wired | ✔confirmed |

`personae.rs`, `hooks.rs`, `sites.rs`, `missions.rs`, `prose.rs` — **CLEAN re §R4** (all edits go
to `data_catalogs.*`; reports are caches, never written directly). Sweep confirmed
`sites_filter_kind`/`missions_filter_kind` **are** applied to their lists, and
`sites_player_edition`/`missions_player_edition` both trigger `recompute_*` and gate columns.
`hooks_edit_target` vs `selected_hook_id` duplication is intentional per §HK2.

> History edits carry `manual=true` so they survive regeneration (correct), but the channel is
> wrong — undo/redo doesn't cover any chronicle edit.

## RUNTIME / META — `generation`, `generate_random`, `search`, `diff`, `analytics`, `interestingness`, `briefing`, `segmentum`, `export`, `validation`, `invariants`, `files`, `project`, `worlds_editor`, `project_tree`, `status`

| ID | Location | Field/Widget | CAT | SEV | Finding | Confidence |
|---|---|---|---|---|---|---|
| GEN-1 | `generation.rs:277` | `world_selection_mode_combo` | N | Low | `WorldSelectionMode` has a single variant (`WeightedRows`, `src/loading/config.rs:214-216`) — the combo can only ever show one option | ✔confirmed |
| RND-1 | `random_run.rs:167` | `RandomGenState::error` | D | Low | Written by `pump()` on worker failure but never read — failures route to `ModalKind::Message` instead (`generate_random.rs:222-224`) | ~reported |

All other runtime panels — **CLEAN**. Sweep confirmed every field of `SearchState`, `DiffState`,
`AnalyticsState`, `SegmentumState`, `ExportState`, `PreviewState` is consumed by its panel;
`validation.rs`/`invariants.rs` read their reports and write only the selection mailbox (their
**reachability** problem is XC-1, not a field problem); `files`/`project`/`worlds_editor`/
`project_tree`/`status` only touch project/dirty/derivation state correctly.

---

# Viewer — per-tab findings

The viewer has **no command bus** — editor panels mutate `EditorState.sector` directly +
`mark_dirty()` by design, so §R4 does not apply. Findings here are dead fields, ignored
controls, and label/behaviour mismatches.

## App-level views — `app/*`, plus `route_planner`, `factions_overview`, `dashboard`, `segmentum_view`, `data_editor`, `preset_gallery`

All **46 fields of `struct App`** (`viewer/src/app/mod.rs:42`) are live (read **and** written) —
**no dead App fields.**

| ID | Location | Field/Widget | CAT | SEV | Finding | Confidence |
|---|---|---|---|---|---|---|
| VAPP-1 | `app/layout.rs:187` (+ `editor_views.rs:347`) | "SAVE & EXPORT ALL" button | G/X | Med | Sets `pending_export = SectorPng` only — SVG/HTML silently skipped despite the "ALL" label | ~reported |
| VAPP-2 | `preset_gallery.rs` (`target`, `add_to_existing`, `width`/`height`) | `PresetGalleryState` | G | Med | Toolbar collects target/add-to-existing/dimensions, but `scaffold` (≈line 202) is called identically regardless — the controls have no effect | ~reported |
| VAPP-3 | `app/planner_view.rs:234` | "PLAN" button | N | Low | Sets `dirty=true`, but FROM/TO/metric controls already set it — the button is a redundant trigger | ~reported |
| VAPP-4 | `app/relations_view.rs` | whole view | S | Low | Debug-level dump (`{:?}` attitude list); no filter/sort/selection/navigation | ~reported |
| VAPP-5 | `app/analytics_views.rs:207` | snapshot `name` | G | Low | Snapshot names auto-generated, displayed, never user-editable (read-only by intent — flagged for clarity) | ~reported |

`sector_view`, `system_view`, `factions_view`, `regions_view`, `trade_view`, `lifecycle`,
`segmentum`, `route_planner`, `factions_overview`, `dashboard`, `segmentum_view`, `data_editor`
— **CLEAN**.

## Editor tabs — `editor/*` (Tab: Map, Routes, Factions, Settings, Generation, Wishes)

| ID | Location | Field/Widget | CAT | SEV | Finding | Confidence |
|---|---|---|---|---|---|---|
| VED-1 | `editor/state.rs:112` | `EditorState::system_side` | D | Med | Never read in any editor panel — shadowed by the live `App.system_side` (`mod.rs:57`, different default 800.0 vs 700.0) that actually drives `system_view.rs:159` | ✔confirmed |
| VED-2 | `editor/state.rs:156` | `stable_ids_on_rename` | D | Low | See **XC-4** — consumed at `sector_view.rs:613` but no UI toggle, stuck at `true` | ✔confirmed |
| VED-3 | `editor/generation_panel.rs:51` & `:241-242` | duplicate SEED widget | X | Med | Two SEED text-edits both bind `config.generation.seed`; the second is mis-placed inside the ROUTES `horizontal` row next to "ENSURE CONNECTED" | ~reported |
| VED-4 | `editor/wishes_panel.rs:124-133` | near-miss preview vs `mark_dirty` | X | Low | Previewing a near-miss seed applies the sector without `mark_dirty`, while "APPLY WINNING SEED" (line 117) does — undocumented asymmetry | ~reported |

`map_panel`, `world_panel`, `system_panel`, `factions_panel` (all four
filter/sort/pin fields consumed), `routes_panel`, `settings_panel`, `dialogs`, `toolbar`,
`ui_helpers`, `file_ops`, `enums` — **CLEAN**.

---

# Confirmed-clean inventory

Tabs/panels read and found to have **no dead or improper fields**:

- **Builder:** `map/mod`, `map/interactions`, `map/dialogs`, `map/theme`, `map/cache`, `orbital`,
  `conflict`, `surface_regions`, `routes`, `subsectors`, `economy`, `personae`*, `hooks`*,
  `sites`*, `missions`*, `prose`*, `generation`(except GEN-1), `search`, `diff`, `analytics`,
  `interestingness`, `briefing`, `segmentum`, `export`, `files`, `project`, `worlds_editor`,
  `project_tree`, `status`. (* = clean re §R4; catalog edits intentionally off-bus.)
- **Viewer:** `sector_view`, `system_view`, `factions_view`, `regions_view`, `trade_view`,
  `lifecycle`, `segmentum`, `route_planner`, `factions_overview`, `dashboard`, `segmentum_view`,
  `data_editor`, and all editor panels except those in VED-1..4.

---

# Recommended remediation order

1. **XC-1 (High)** — decide the fate of `validation.rs`/`invariants.rs`: render them or delete
   them. They are the only fully-dead user-facing surface.
2. **XC-2 / `X` items (Med)** — close the easy undo gaps first, where the command already exists:
   `world.rs` orbit+name (`SetWorldOrbit`/`RenameWorld`), `system.rs`/`system_map.rs` star +
   post-`AddWorld` orbit (`SetStar`/`SetStarSpectral`/`SetWorldOrbit`), `regions.rs` route effects
   (`ReplaceRoutes`). Then add the missing commands for the remaining `world.rs`/`control.rs`/
   `history.rs` edits, or explicitly document those editors as non-undoable.
3. **VED-1, VED-3, RND-1, GEN-1 (Med/Low)** — delete the shadowed `EditorState::system_side`,
   remove the duplicate viewer SEED widget, surface or drop `RandomGenState::error`, collapse the
   single-variant world-selection combo to a label.
4. **VAPP-1/2 (Med)** — make "SAVE & EXPORT ALL" honest (chain SVG/HTML or rename), and either
   wire `PresetGalleryState` target/dimensions/add-to-existing into `scaffold` or gate them.
5. **XC-3, XC-4, PER-1, REL-1, VAPP-3/4/5, VED-4 (Low)** — comment fixes, dead-param removal,
   minor label/behaviour cleanups.

*No determinism invariant (Fx-iteration, stage-RNG, byte-stable output) is implicated by any
finding above — the §R4 items are purely an undo/redo coverage gap.*
