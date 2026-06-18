# IMPROVEMENT.txt Remediation Campaign — Execution Report

**Scope:** the 12-commit run on `main`, `0038337..5da5426` (parent `0038337`;
commits `6e2d9dc` … `5da5426`), each carrying a `Claude-Session:` trailer and
mapping to the PR 1–10 roadmap / findings P0-1 … P3-7 (+ review findings G2/G3).
**Sources cross-checked:** (1) `git log --stat 0038337..5da5426` + commit
messages/diffs; (2) the per-finding `[DONE]/[NO ACTION]/[IN PROGRESS]` markers and
appended `Status:` notes in `docs/IMPROVEMENT.txt`; (3) current code, spot-verified
at `path:line`. Generated 2026-06-18.

---

## 1. Executive summary

The PR 1–10 roadmap was addressed **in full**: every roadmap finding now carries a
resolution marker in `docs/IMPROVEMENT.txt`, and the campaign closed with a
consistency pass (`5da5426`) reconciling the `OPTIMIZE.txt` cross-references.

But the headline is the **real-vs-stale ratio**. Of **22 findings** addressed, only
**6 (~27%) required new code**; the other **16 (~73%) needed no implementation** —
**12 were already resolved by prior refactors**, 2 were deferred by design, and 2
were no-action. Concretely, the 12 commits split exactly **6 code-bearing / 6
docs-only**.

**The 6 real units:** `f2115e5` (P2-1, rustfmt cleanup), `7cf5c33` (P2-3,
file_watcher), `219fbfd` (P0-1, derivation-cache collision fix), `97cd515` (P0-2,
`settings_digest` broaden + golden re-pin), `b0dab4f` (P1-2, export benches),
`2141ad9` (P2-4, lock-free `JobHandle` progress).

**Why so much was stale.** `IMPROVEMENT.txt` was authored against an earlier tree.
Between authoring and execution the repo underwent perf rewrites and structural
refactors that *incidentally* satisfied most findings: files were relocated
(`src/subsectors/` → `src/export/subsectors/`, `src/economy.rs` →
`src/analysis/economy/derive.rs`), bare `unwrap`s became `.unwrap_or(0)` / let-else,
public enums were marked `#[non_exhaustive]` (`3ad88f9`), the CLI was split into
`src/cli/`, the golden test net landed (`adbe7d0`), the MSRV was pinned, and the CI
workflow was added (`f60c085`) — **all before `0038337`**. By execution time the
campaign's job had become as much *verification + documentation* as code.

---

## 2. Outcome matrix

Status legend: **Fixed** = new code landed this campaign · **Stale** = already
resolved by a prior commit (campaign only documented it) · **Deferred** = postponed
by design · **No-action** = nothing to do. "Commit" is the campaign commit; for
stale rows it also names the **prior** commit that actually resolved it.

| Finding | PR | Status | Commit | One-line |
|---|---|---|---|---|
| **P0-1** | 2 | **Fixed** | `219fbfd` | derivation-cache digest collision: `digest_input`→`Option`, cache reads/writes gated |
| **P0-2** | 3 | **Fixed** | `97cd515` | `settings_digest` broadened to hash generation+outputs wholesale (only golden shift) |
| P0-3 | 2 | Stale | doc'd `219fbfd` (prior `cf8fa27`) | viewer auto-save already matches `Result`; no `.unwrap()` |
| P0-4 | 2 | Stale | doc'd `219fbfd` (prior `1ac83ea`/`0038337`/`cea8a49`) | subsectors `.unwrap_or(0)` + economy let-else already in place |
| P1-1 | 4 | Stale | doc'd `6e2d9dc` (prior `adbe7d0`) | public-writer pixel/text goldens already landed |
| G2 | 4 | Stale | doc'd `6e2d9dc` | committed content goldens for text outputs already present |
| G3 | 4 | Stale | doc'd `6e2d9dc` | golden net already verified green |
| **P1-2** | 5 | **Fixed** | `b0dab4f` | added SVG/JSON/Markdown export Criterion bench groups |
| P1-3 | 6 | Stale | doc'd `d59ccda` | job model already consolidated on generic `JobHandle<T>` |
| **P2-1** | 1 | **Fixed** | `f2115e5` | `cargo fmt --all` rustfmt cleanup (9 files) |
| P2-2 | 1 | Stale | `7cf5c33` | default-level clippy already clean (no code); pedantic sweep deferred |
| **P2-3** | 1 | **Fixed** | `7cf5c33` | file_watcher scan-then-write-back + const poll interval |
| **P2-4** | 7 | **Fixed** | `2141ad9` | `JobHandle.progress`→`Arc<AtomicU32>`, Acquire/Release, %-throttled repaint |
| P2-5 | 10 | Deferred | doc'd `0d51bd0` | snapshot CoW (`Arc::make_mut`) deferred until profiling justifies |
| P2-6 | 3 | Stale | doc'd `97cd515` | viewer already holds `Option<Arc<GeneratedSector>>`; clones are Arc-clones |
| P3-1 | 8 | Stale | doc'd `050a881` (prior `3ad88f9`) | public enums already `#[non_exhaustive]` (93 sites) |
| P3-2 | 9 | Stale | doc'd `88097f9` | CLI already split into `src/cli/` (main.rs 40 LOC, 22 modules) |
| P3-3 | 8 | Stale | doc'd `050a881` | MSRV `rust-version="1.87"` already pinned in workspace root |
| P3-4 | 10 | Stale | doc'd `0d51bd0` (prior `f60c085`) | CI workflow already present; cargo-deny intentionally skipped |
| P3-5 | (rollup) | Deferred | `5da5426` | `PreviewJobResult` `large_enum_variant`: keep `#[allow]` vs `Box` — deferred |
| P3-6 | — | No-action | — | `KeyTables` HashMap safe for read-only lookups; leave as-is |
| P3-7 | — | No-action | — | CLI `println!`/`eprintln!` fine for a binary; structured output deferred |

**Tally:** 6 Fixed · 12 Stale · 2 Deferred · 2 No-action = 22.

> **Correction surfaced during verification:** the brief listed P2-2 as a "default
> CLI" finding. It is not — P2-2 is *"Clippy: 10 default warnings + ~430 pedantic"*
> (`docs/IMPROVEMENT.txt:420-425`). Commit `7cf5c33` records *"no code needed —
> `cargo clippy --workspace --all-targets -- -D warnings` is already clean"*, so
> it is classified Stale (default-level already clean) with the pedantic sweep
> explicitly deferred.

---

## 3. Real changes — the 6 code units

### P2-1 — rustfmt cleanup · `f2115e5` (PR 1)
- **Where:** 9 files — `builder/src/builder/panels/interestingness.rs`,
  `builder/src/builder/state/mod.rs`, `gui-core/src/widgets.rs`,
  `src/gen/random_sector.rs`, `tests/it/{relations_tests,search_and_diff,svg_export_tests}.rs`,
  `viewer/src/app/export_ui.rs`, `viewer/src/editor/enums.rs`.
- **What:** pure `cargo fmt --all` (import-list rewrap, blank-line normalization).
- **Why correct:** makes the tree rustfmt-clean so the **pre-existing** CI gate
  `cargo fmt --all -- --check` (`.github/workflows/ci.yml:24-25`) passes.
- **Verified:** `cargo fmt --all -- --check` now produces no output (no-op).

### P2-3 — file_watcher scan-then-write-back · `7cf5c33` (PR 1)
- **Where:** `builder/src/builder/file_watcher.rs:92-115`.
- **What:** `scan_once` changed from *snapshot-then-mutate* (clone the whole
  baseline into a `Vec` before iterating) to a **read-only scan** that collects
  `updates: Vec<(String, SystemTime)>`, followed by a **separate write-back loop**;
  `POLL_INTERVAL` hoisted to a module const.
- **Why correct:** removes the borrow hazard of mutating `baseline` while iterating
  it; the common case (no changes) now **allocates nothing and clones no keys**.
- **Verified:** builder tests green.

### P0-1 — derivation-cache digest collision · `219fbfd` (PR 2)
- **Where:** `builder/src/builder/derivation_cache.rs:32-35` (`digest_input` →
  `Option<String>`); call-sites gated in `builder/src/builder/state/derivations.rs`
  (`derivation_fingerprint` `:63-90`, `mark_derivation_fresh` `:159-163`,
  `derivation_status` `:169-174`, `ensure_fresh` `:201-217`,
  `dispatch_background_derivations` `:301-303`, `pump_derivation_jobs` `:485`).
- **What:** `digest_input` previously hashed a serialize-failure to the **empty-byte
  digest** — one shared key across *all* failed inputs. It now returns `None`; every
  cache read/write is `Some(fp)`-gated.
- **Why correct:** on collision the old code could serve a **stale memoized
  derivation** for a different input. With `None`, the ledger is neither read nor
  written and the value is recomputed fresh (un-memoized that round) — no stale
  entry can be served.
- **Verified:** 423 builder tests pass; happy-path digest bytes unchanged.

### P0-2 — `settings_digest` broaden-in-place · `97cd515` (PR 3)
- **Where:** `src/gen/generation/mod.rs:989-994` (computation), `:981-988` (rationale
  comment). Input structs: `src/loading/config.rs:103-131` (`GenerationConfig`),
  `:276-294` (`OutputConfig`).
- **What:** replaced a hand-listed **7-scalar `format!` string** with a wholesale
  `blake3` over `serde_json::to_vec(&(&config.generation, &config.outputs))`. Now
  covers `placement` / `world_selection` / `routes` / `relations` and **all output
  toggles**, not just 7 generation scalars.
- **Why correct:** output-affecting settings (placement, routes, output toggles)
  were absent from the old digest → two configs yielding different output could
  collide on the same `settings_digest`. The wholesale serde hash closes that.
- **Verified:** **the campaign's only golden-affecting change.** Re-pinned 4 goldens
  (`tests/goldens/html_m42_default.blake3`, `sector_m42_default.json`,
  `segmentum.md`, `segmentum_golden.json` per the `97cd515 --stat`) and proven
  **digest-only** — sector content byte-identical, only the digest field shifted.

### P1-2 — export-phase benches · `b0dab4f` (PR 5)
- **Where:** `benches/generation.rs:245-256` (`bench_render_svg` → group
  `render_sector_svg`), `:260-272` (`bench_serialize_json` → `serialize_json`),
  `:275-285` (`bench_render_markdown` → `render_sector_markdown`), registered
  `:287-301`.
- **What:** added the 3 missing output-phase Criterion groups. Existing fixtures
  already covered raster / encode / influence_field / derive.
- **Why correct:** fills the output-phase gap the finding named; its OnceLock +
  SmallInput sub-items were already present (stale).
- **Verified:** groups registered in the `criterion_group!` macro.

### P2-4 — lock-free `JobHandle` progress · `2141ad9` (PR 7)
- **Where:** `gui-core/src/jobs.rs:41` (`progress: Arc<AtomicU32>`, "stored as f32
  bits"), `:48-49` (cancel `store(..., Release)`), `:52-54` (is_cancelled
  `load(Acquire)`), `:56-58` (progress read `load(Relaxed)`), `:156-167`
  (`set_progress`: `Relaxed` store + percent-advance repaint throttle); call-site in
  `viewer/src/editor/state.rs`.
- **What:** `progress` moved from `Arc<Mutex<f32>>` to `Arc<AtomicU32>` holding the
  `f32` bit pattern (lock-free); `cancel`/`is_cancelled` downgraded from `SeqCst` to
  `Release`/`Acquire`; `request_repaint` fires **only when the rounded 0..=100
  percent advances**.
- **Why correct:** removes mutex contention on the hot progress read path; the
  repaint throttle eliminates redundant UI redraws between percent ticks.
- **Why per-percent, not a 30 Hz timer:** see §5 — `JobContext` is shared across
  threads in parallel search and must stay `Sync`; an `AtomicU32` percent compare is
  lock-free and `Sync`, a wall-clock timer would need shared mutable clock state.
- **Verified:** tests at `gui-core/src/jobs.rs:187-246` (cancel, progress
  round-trip, dispatch ordering).

---

## 4. Stale findings — already resolved before the campaign

Each confirmed against current code (`path:line`) and, where traceable, the **prior
commit** that resolved it. The campaign commit only updated documentation.

- **P0-3 viewer auto-save** — already `Result`-matched with full error handling at
  `viewer/src/app/mod.rs:244-268`; field `viewer/src/editor/state.rs:131`; tests
  `viewer/src/app/mod.rs:356-390` & `:395-403`. No `.unwrap()` since **`cf8fa27`**.
  Documented `[DONE]` in `219fbfd`.
- **P0-4 subsectors + economy** — `src/subsectors/` → `src/export/subsectors/`; bare
  unwraps now `.unwrap_or(0)` (`src/export/subsectors/mod.rs:564,629,635,640`) via
  perf rewrite **`1ac83ea`/`0038337`**. Economy `is_none()+unwrap` is now let-else
  (`src/analysis/economy/derive.rs:175-177`, moved by **`cea8a49`**); overrides wired
  via `recompute_economy()` (`builder/src/builder/state/derivations.rs:561,572`).
  Documented in `219fbfd`.
- **P1-1 / G2 / G3 golden net** — pixel/text goldens already present:
  `tests/it/golden_png.rs:21`, `tests/it/golden_generation.rs:348` & `:354`,
  `tests/it/export_byte_goldens.rs:30`, with committed
  `tests/goldens/sector_m42_default.{json,md}` and `png_m42_default.blake3`. Earliest
  introduction **`adbe7d0`** (2026-05-24). Documented `6e2d9dc` — subject literally
  *"were already landed"*.
- **P1-3 job model** — builder already uses the generic
  `gui-core::jobs::JobHandle<T>` (`gui-core/src/jobs.rs:37`, `JobContext` `:143`,
  `spawn_job` `:68`); no stub handle, `pending_jobs` already removed; tests at
  `:187-246`. Documented `d59ccda`.
- **P2-2 default clippy** — `cargo clippy --workspace --all-targets -- -D warnings`
  already clean (`7cf5c33` commit message); pedantic (~430) sweep + `workspace.lints`
  + `format!`-append→`write!` deferred to a separate opt-in PR.
- **P2-6 viewer Arc-sharing** — viewer already holds the sector as
  `Option<Arc<GeneratedSector>>`; all clones are Arc-clones. No change needed
  (documented in the PR 3 doc pass, `97cd515`).
- **P3-1 `#[non_exhaustive]`** — 93 occurrences across `src/`, added in **`3ad88f9`**
  ("review section 1", 2026-05-28). Examples: `src/export/heatmap.rs:15-16`,
  `src/export/map_theme.rs:11-12`, `src/export/writers.rs:17-18`. Documented
  `050a881`.
- **P3-2 CLI split** — `src/main.rs` is 40 LOC and delegates (`src/main.rs:8,12-13`);
  `src/cli/mod.rs` holds the clap `Command` enum (23 subcommands, `:42-486`) and
  `run` dispatch (`:488-733`), plus 22 per-command modules. Documented `88097f9`.
- **P3-3 MSRV** — `rust-version = "1.87"` pinned at `Cargo.toml:17`, inherited via
  `rust-version.workspace = true` (`Cargo.toml:5`). Documented `050a881`.
- **P3-4 CI present** — `.github/workflows/ci.yml` already exists with fmt-check,
  clippy `-D warnings`, test+golden, MSRV 1.87, ignored-integration job, and the
  `rustsec/audit-check@v1` audit job (`:58-69`). The whole workflow predates the
  campaign (introduced **`f60c085`**, ancestor of `0038337`) — **no campaign commit
  touched `.github/`**. cargo-deny intentionally skipped (see §5). Documented
  `0d51bd0`.

---

## 5. Design decisions made during execution

All three were confirmed with the user.

1. **`settings_digest` "broaden-in-place"** (P0-2) — chosen over the spec'd
   `settings_digest_v2` + deprecation path. A wholesale serde hash of
   `(generation, outputs)` covers every output-affecting setting without a
   versioned-field migration, and `input_digests` already covers the *input files*
   separately. Trade-off accepted: it shifts the golden digest, which was re-pinned
   and proven **digest-only** (sector bytes unchanged). `src/gen/generation/mod.rs:981-994`.
2. **P2-4 repaint throttle as per-percent (`AtomicU32`), not a literal 30 Hz timer**
   — `JobContext` is shared across threads in parallel search, so it **must stay
   `Sync`**. A value-based "did the rounded percent advance?" check on an
   `AtomicU32` is lock-free and `Sync`; a wall-clock 30 Hz timer would require shared
   mutable clock state and undermine that. `gui-core/src/jobs.rs:156-167`.
3. **cargo-deny skipped** (P3-4) — the existing CI `rustsec/audit-check` job already
   covers RUSTSEC advisories (`.github/workflows/ci.yml:58-69`). cargo-deny's
   license / duplicate-version checks are marginal for an internal, unpublished tool
   with pre-existing known dev-only dependency duplicates.

---

## 6. Deferred / no-action

**Deferred by design**
- **P2-5 — snapshot copy-on-write** (`[NO ACTION]`, profile-gated). Snapshots still
  deep-clone the sector; `Arc<GeneratedSector>` + `Arc::make_mut` deferred until a
  profiler justifies the change. Mapped to PR 10 (`0d51bd0`).
- **P3-5 — `PreviewJobResult` `large_enum_variant`** (`[IN PROGRESS]`, **unmapped to
  any PR**). `Ready(GeneratedSector) | Cancelled | Failed(String)` currently uses a
  justified `#[allow]`; option to `Box` the `Ready` variant and drop the allow.
  Deemed mostly cosmetic — the value travels via `mpsc` once, so the `#[allow]` is
  acceptable. Touched in the rollup `5da5426`.
- **`OPTIMIZE.txt` #9 — GUI responsiveness regression checks** (Low, partial,
  **unmapped to any PR**). `preview.rs` already has schedule/revision/stale tests;
  remaining: promote that pattern to export jobs + add a manual responsiveness
  checklist to `GUIDE.md`.

**No-action**
- **P3-6** — `worlds::KeyTables` uses `HashMap` safely for read-only lookups; switch
  to `BTreeMap` only if iteration (i.e. ordered output) is ever added.
- **P3-7** — CLI subcommands printing via `println!`/`eprintln!` is fine for a
  binary; structured (JSON-lines) output deferred until requested.

---

## 7. Verification posture

- **Determinism / golden invariants held.** The **only** golden shift in the entire
  campaign was P0-2 `settings_digest` (`97cd515`), and it was re-pinned and proven
  **digest-only** via `git diff` — the sector content is byte-identical; only the
  digest field changed. Affected goldens: `html_m42_default.blake3`,
  `sector_m42_default.json`, `segmentum.md`, `segmentum_golden.json`.
- **Every other code unit is non-golden-affecting** (formatting, file_watcher
  internals, cache gating, benches, in-memory atomics).
- **Determinism rules respected:** no new RNG paths (P2-4 is `AtomicU32`, not an RNG);
  no `Fx*`-iteration-for-output introduced; the digest change routes through the
  existing `rng::hex`/`blake3` path.
- **Test/lint status reported by the campaign:** 423 builder tests pass (P0-1);
  default-level clippy clean under `-D warnings` (P2-2); the pre-existing CI gates
  (fmt-check, clippy, test+golden, MSRV 1.87, RUSTSEC audit) remain green.

---

## 8. Leftovers, test gaps & spec backlog

These are **not** part of the campaign — they are the known remaining backlog,
captured here so the report is a complete picture.

### Test gaps (`TEST_GAPS`)
**88 total**, audited 2026-06-07 by a 28-agent sweep (excludes UI screenshot /
live-egui-frame snapshots by scope). **Currency caveat:** the audit predates this
campaign; several gaps may now be partially covered — each needs a currency check
before acting.

| Priority | Count |
|---|---|
| High | 31 |
| Med | 41 |
| Low | 16 |

Largest gap areas (categories overlap; this list is indicative, not a partition of
the 88): `src/validate/validation.rs` (31) · `builder state + command.rs` (17) ·
`src/analysis relations/control/economy` (16) · `viewer` (14) · `src/export
render/svg/html/bitmap` (11) · `gui-core` (11) · `src/model/rng.rs` (9) ·
`src/gen/routes/regions/hidden_routes` (9) · `src/cli` (8) · `src/gen/generation
placement` (8) · `src/loading + serde round-trips` (8) · `examples/*.toml` (3).

### Spec backlog
- **`BUILDER_REQS`** — T6b viewer-handoff smoke test (save builder project → open
  with `sectorforge-viewer --project` → confirm `sector.json` loads); OTR5 lock-free
  progress + handle-stub unification (*largely realized by P2-4*); a11y (menu items,
  tab focus, color-independent signals); keyboard-shortcut polish (beyond
  Ctrl-Z/Y — Ctrl-K palette, user-definable bindings).
- **`CONTEXT_MENU`** — Phase-7 / §6.9 "Open in ORBITAL" scroll-anchor jump deferred
  (inline SYSTEM tab already renders the section); arrow-key nav inside menus (egui
  handles Tab/Enter/Escape natively); `SubsectorBorder` hit-test placeholder awaiting
  full UI (§15 Future work).
- **`GUIDE`** — deferred SYSTEM-tab context-menu jumps (§6.9); `SubsectorBorder`
  placeholder (right-click on subsector label, post-v1); in-session regenerate C2
  (REGENERATE SYSTEM in-place / C2-tab partial-regen anchor).

---

*All claims above are checkable: commit hashes from `git log 0038337..5da5426`,
`path:line` from current `HEAD`.*
