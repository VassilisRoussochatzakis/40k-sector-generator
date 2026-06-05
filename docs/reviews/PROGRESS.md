# Review execution — progress tracker

Tracks the **fix** status of the findings verified in this directory. The
`AREA_*.md` files describe *what* the findings are; this file tracks *what has
been done about them*, against the [README](README.md) "Recommended execution
sequence". Update this file whenever a finding moves status.

**Legend:** ✅ DONE · 🔄 IN PROGRESS · ⏳ PENDING · ⏸️ DEFERRED (blocked / owner-decision)

> Note: the **Status** column inside each `AREA_*.md` records whether a finding
> *reproduced* (`✅ Confirmed` / `⚠️ Partial` / `🔄 Moved` / `🟢 Already fixed`).
> That is **not** the same as fix status — this file is the fix tracker.

## Area roll-up

| Area | Findings | ✅ Done | 🔄 In progress | ⏳ Pending | ⏸️ Deferred |
|---|---|---|---|---|---|
| A `src/model` + generation | 12 | 0 | 0 | 12 | 0 |
| B `src/analysis` | 14 | 1 (B-S3) | 0 | 13 | 0 |
| C export/validate/worlds/cli | 13 | 2 (C1,C-S2) | 0 | 11 | 0 |
| D builder command + state | 14 | 12 | 0 | 0 | 2 (D-S3/D5) |
| E builder panels | 17 | 4 (E1,E2,E3,E-S1) | 0 | 13 | 0 |
| F viewer + gui-core | 15 | 0 | 0 | 15 | 0 |
| G tests | 13 | 1 (G2) | 0 | 12 | 0 |

## Execution sequence (README order)

1. **P0 bus fixes** — triaged E2 → E1 → D1, plus D4.
   - D1 region add/remove/paint/erase commands — ✅ DONE (`7a06824`)
   - D4 move/swap route-controls — ✅ DONE, documented carve-out (`7a06824`)
   - D2/D11 chronicle undo precedence — ✅ DONE, on-bus active / off-bus passive split (`7a06824`)
   - **E2** per-frame `primary_factions` off-bus write — ✅ DONE (this session)
   - **E1** `ApplyFactionPower` command — ✅ DONE (this session)
2. **`labeled()` extraction** (E3 / E-S1, 33 files) — ✅ DONE (this session)
3. **C1** diff-drift fix — ✅ DONE · **`enum_slug!` macro** (B-S3, 61/62 sites) — ✅ DONE (this session)
4. **G2 content golden** (`sector.json` / `sector.md`) — ✅ DONE (this session) · **gate cleared — A–F god-file splits now have their safety net**
5. **Dedup waves + god-file splits** (behind G2) — ⏳ PENDING (now unblocked)

## Detailed log

### 2026-06-05 — session start

- **AREA_D** confirmed fully closed by commit `7a06824` (all 14 findings).
  `D-S3`/`D5` (154-field god-struct split) intentionally ⏸️ DEFERRED behind the
  G2 content-golden net per the review's own sequencing.

- **E2** — `builder/src/builder/panels/control.rs` §C5. Replaced the per-frame
  off-bus write `systems[i].primary_factions = derived` with a **change-gated,
  dirty-tracked passive reconcile** (`if !locked && stored != derived { write;
  dirty }`).
  **Decision (owner-visible):** chose README option *(a) derivation/read-model*
  — keep the passive reconcile **off** the undo bus — over option *(b) commit via
  `EditSystem`*. Rationale: `primary_factions` has many stored readers (export
  render/labels, info_panel, analysis, validate) so the field must stay
  populated; and dispatching a command during render would inject an undo entry
  on mere tab navigation and *fight* undo (undo → re-derive from unchanged
  presence → re-dispatch). This mirrors the **LD4 chronicle §R4 carve-out** the
  D2/D11 fix (`7a06824`) just established: passive view-refresh of denormalized
  document state stays off-bus to protect the redo tail; the **active** "↺
  Re-derive" button remains on-bus via `EditSystem`.
  _Known residual (pre-existing, out of scope):_ for an unlocked system whose
  presence changes while the CONTROL tab is not open, the stored field stays
  stale until next view — same window as the old code; any reader needing the
  live value calls `derive_system_control()` directly.

- **E1** — `control.rs` §C6 + `builder/src/builder/command.rs`. Added
  `BuilderCommand::ApplyFactionPower { before: Vec<(FactionId, PowerProfile)>,
  after: BTreeMap<FactionId, PowerProfile> }` — `dep_classes = [Factions]`
  (fans out to PowerProjection + faction overlays), `before` captured on
  `apply`, `revert` restores. The "↺ Apply to faction totals" button now routes
  through `state.run`; manual `dirty`/`mark_validation_dirty` dropped (bus rails
  handle it). Added `apply_faction_power_round_trip` test; extended
  `dep_classes_cover_all_variants` (38 → 39).

- **Verification (E1+E2)** — `cargo test -p sectorforge-builder` 317/317 pass
  (incl. `dep_classes_cover_all_variants`, `apply_faction_power_round_trip`);
  `cargo test --test it -- golden` 13/13 pass (byte-stable — `primary_factions`
  feeds render/labels, unchanged); `cargo clippy -p sectorforge-builder
  --all-targets -- -D warnings` clean.

- **G2 / G-S3** — `tests/it/golden_generation.rs` + `tests/goldens/`. Added a
  full-content golden for the exported `sector.json` (~1.8 MB) and `sector.md`
  (~286 KB) of the m42 fixture, gated by `UPDATE_GOLDEN_JSON` /
  `UPDATE_GOLDEN_MD`. Chose **full-file pin over a blake3 hash** (the golden_png
  style) because G2's purpose is a *reviewable* diff of text drift (renamed
  field / dropped markdown row) — a hash detects drift but can't show it.
  Failure output reports the first differing line, not the whole file. Blessed
  then re-verified reproducible: **15/15 golden tests pass**. This is the safety
  net the A–F god-file splits are gated on — **gate now cleared**.
  _Size trade-off:_ the two blessed files add ~2.1 MB to the repo. If a lean
  history is preferred over diff-ability, switch to a committed blake3 hash pair.

- **C1 / C-S2** — `src/validate/diff.rs:370`. Replaced the stringly-typed
  `match *k { … _ => 0.0 }` resource closure with `sector_balance.get(k)` (the
  canonical `ResourceVector::get`; `&&str`→`&str` coerces, so no explicit deref).
  A new `RESOURCE_KEYS` entry now flows into economy diffs automatically instead
  of silently reading 0.0 — kills the data-loss class. Behaviour-identical for
  the current set; `diff` + `golden` tests green.

- **B-S3** — added `macro_rules! enum_slug!` in `src/macros.rs` (wired crate-wide
  via `#[macro_use]`), with a normal arm + a `const` arm; slugs written verbatim
  so output is byte-identical. Converted **61 of 62** hand-written `as_slug`
  enums across analysis / gen / export / model / loading / validate (5 by hand —
  personae + validation, incl. the one `const` enum `ValidationCode`; 56 via a
  general-purpose subagent gated on the G2 content golden). The 1 holdout,
  `SectorSize` in `gen/random_sector.rs`, keeps its hand-written `as_slug`: it has
  a struct variant `Custom { dim }`, so it is not the fieldless enum the macro
  targets (forcing it through the macro is a compile error). Verified: golden
  15/15 byte-identical, lib 191/191, `it` 93/93, workspace clippy clean.

- **E3 / E-S1** — hoisted the byte-identical private `fn labeled` from 33 builder
  panel files to `pub fn labeled` in `gui-core/src/ui_kit.rs` (sibling to the
  existing `field`), removed all 33 copies, and folded the import into each
  file's existing `ui_kit` use. 9 files also needed an unused-import cleanup
  (`Ui` / `RichText` / `palette` used only inside the deleted fn). The mechanical
  sweep was delegated to a `panel-implementer` subagent; verified here:
  `grep 'fn labeled' builder/src` → empty, **workspace clippy clean**, builder
  **317/317**, `it` suite **93/93**.

### Open decisions / notes
- **Commit cadence:** user chose *accumulate* — E1/E2 + G2 + doc updates sit in
  the working tree uncommitted, to fold into a larger commit after the next wave.
- **G2 file size** (~2.1 MB) — flagged above; revisit before the eventual commit
  if repo leanness is preferred.
