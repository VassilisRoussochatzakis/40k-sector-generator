# Code-Quality Review — verification pass (2026-06-05)

This directory is the **verified, documented** form of [`docs/IMPROVEMENT_REVIEW.md`](../IMPROVEMENT_REVIEW.md).
Every finding in that review was re-opened against the **live tree** (not the review's snapshot),
its current `path:line` re-pinned, the issue confirmed or refuted with a real code excerpt, and given a
concrete codebase-specific fix + effort + risk. One verification doc per review area:

| Area | File | Scope |
|---|---|---|
| A | [AREA_A_model_generation.md](AREA_A_model_generation.md) | `src/model` + generation |
| B | [AREA_B_analysis.md](AREA_B_analysis.md) | `src/analysis` |
| C | [AREA_C_export_validate_worlds_cli.md](AREA_C_export_validate_worlds_cli.md) | export / validate / worlds / cli |
| D | [AREA_D_builder_command_state.md](AREA_D_builder_command_state.md) | builder command bus + state |
| E | [AREA_E_builder_panels.md](AREA_E_builder_panels.md) | builder panels |
| F | [AREA_F_viewer_gui_core.md](AREA_F_viewer_gui_core.md) | viewer + gui-core |
| G | [AREA_G_tests.md](AREA_G_tests.md) | tests |

## Headline

**The review is accurate.** Across ~98 findings, **zero were false positives** and only **one was already fixed**
(D6 — the per-frame economy-recompute concern; a BLAKE3 staleness gate already prevents it). Everything else
reproduced. Differences from the review are **line drift** and **count corrections**, not invalidations — several
corrections *raise* the payoff (the `enum_slug!` and color findings are bigger than the review estimated).

| Area | Findings | ✅ Confirmed | 🔄 Moved | ⚠️ Partial | 🟢 Already fixed | ❌ Not repro |
|---|---|---|---|---|---|---|
| A | 12 | 10 | 1 | 1 | 0 | 0 |
| B | 14 | 11 | 2 | 1 | 0 | 0 |
| C | 13 | 12 | 1 | 0 | 0 | 0 |
| D | 14 | 11 | 1 | 1 | 1 | 0 |
| E | 17 | 15 | 0 | 2 | 0 | 0 |
| F | 15 | 13 | 1 | 1 | 0 | 0 |
| G | 13 | 11 | 1 | 1 | 0 | 0 |

## P0 — bus-bypass verdicts (the decisions that gate fixes)

The review flagged 5 command-bus bypasses and asked each to be checked against the §R4 carve-out
(transient UI state is exempt; document state — anything in `sector.json` — must go through the bus).
Verified verdicts:

| Finding | Where | Verdict | Action |
|---|---|---|---|
| **E2** `primary_factions` written every frame | `control.rs:768` | 🔴 **REAL bypass** — worst smell; document state (`#[serde(default)]`), no dirty flag even set | Move to derivation/read-model, or commit via `EditSystem` only on change. **Start here.** |
| **E1** `apply_faction_power` off-bus | `control.rs:956` | 🔴 **REAL bypass** — `power: PowerProfile` is serialized document state; only `state.dirty=true` set | Add `BuilderCommand::ApplyFactionPower`. |
| **D1** region add/paint/erase | `regions_ops.rs:19,47,61,84` | 🔴 **REAL bypass** — `sector.regions` serializes; `SetRegionKind`/`RenameRegion` already use the bus | Add `AddRegion`/`PaintRegion`/`EraseRegion` commands. The `§D3` code comment is **not** a carve-out rule — §D3 is the god-file refactor tag, not an exemption. |
| **D4** `MoveSystem`/`SwapSystems` don't recompute `controls` | `command.rs` | 🟡 **Carve-out, narrow real gap** — `controls` are faction-presence-derived, not position-derived, so a move doesn't logically stale them; but `AddRoute` recomputes and move/swap don't | S-effort: add the same recompute in the move/swap apply arm. |
| **D2 / D11** `recompute_chronicle` vs `EditChronicle` | `derivations.rs:373` / `command.rs:817` | ⚪ **Needs owner decision** — manual events *are* preserved (no immediate data loss), but undo-precedence when `history_auto_recompute` interleaves with `EditChronicle` is undefined | Decide whether catalog-triggered recompute belongs on the undo stack **before** coding. Resolve D2 + D11 together. |

## Corrections vs. the review (these change scope or payoff)

| Finding | Review said | Verified | Impact |
|---|---|---|---|
| B-S3 `enum_slug!` | "~20 enums" | **28** `as_slug` in `src/analysis/`, **62** total in `src/` | Macro payoff is **3× larger** than stated. |
| E-S1 / E3 `labeled()` | ×32 files | **33** files (incl. `map/mod.rs`) | Slightly larger; still the safe warm-up. |
| E-S3 `EditWorld` idiom | ×26 (+`EditSystem` ×9) | **16** `EditWorld`, **10** `EditSystem` | Smaller than stated; still worth the helper. |
| E4 `format!("{v:?}")` keys | ×12 in `world.rs` | **9** (3 ad-hoc + 7 `EnumPicker::debug_key`) | Some are a reusable impl, not raw call sites. |
| F-S2 / F5 hardcoded colors | "~20" / "×11" | **30** total `Color32::from_rgb`; **~20 semantic** to convert, **~10 intentional** (dark banners, data-viz, F12 split) | Don't blanket-replace — ~10 are correct. |
| B-S1 `SectorReport` trait | "9 modules" | **7** with the full derive+render+write triple | Trait covers 7; config-load is asymmetric (only economy/relations expose `load_*_file`). |
| B-S2 `WeightedAnchored` | hooks ≈ missions, `merge_manual` shared | `cap_per_anchor` **is** verbatim-dup, but `merge_manual` has **diverged** (hooks dedupes by id; missions appends) | Merge is a behavioral decision, not a pure mechanical extract. |
| A4 taxonomy round-trip | "no exhaustiveness guard" (4 tables) | A round-trip test **exists** but covers only `WorldType`; 3 tables (`StarColour`/`Government`/`NotableFeature`) unguarded | Smaller fix than implied. |
| A5 / D5 god-struct fields | "~199" / "~110" | **157** (`GeneratedSector`+DTOs) / **154** (`BuilderState`) | Counts corrected. |
| B10 `mul_add` rewrite | (no caveat) | A `WEIGHTS.dot` rewrite is **not** bit-identical → **golden risk** | Must gate on `cargo test --test it -- golden`. |
| C3 emit triple | ×13 | **12** structurally identical + `diff.rs` variant | Helper covers 12 cleanly. |
| F-S1 two editing stacks | "two save paths, dup logic" | Worse: a **per-frame sync bridge** (`app/mod.rs:193–230`) copies `editor.sector → App.sector`; the editor path **omits the `reindex_ids` call** the App path makes → live divergence risk, not just duplication | Unification is correctness, not only cleanup. |
| G1 / G-S1 "proptest" docs | docs claim seed-varying proptest | **Confirmed false** — `proptest` appears only in module-doc headers; tests run `derive_with` twice on one memoized `OnceLock` fixture (idempotency, not reproducibility) | Either add real `proptest!` or fix the docs — don't trust the claim. |

## Recommended execution sequence (review's plan, adjusted by verification)

1. **P0 bus fixes, triaged** — **E2** first (clearest: per-frame, no dirty flag) → **E1** (`ApplyFactionPower`) → **D1** (region commands). **D4** is a small recompute add. **Do not code D2/D11** until the undo-precedence question gets an owner decision.
2. **`labeled()` extraction** (E3/E-S1, now 33 files) — safe, mechanical, touches every god-file. Warm-up.
3. **`enum_slug!` macro** (B-S3, 62 sites — bigger win than estimated) **+ C1 diff drift fix** (`SectorBalance::get` — kills the silent resource-drop data-loss class). Two silent-drift classes gone.
4. **G2 content golden BEFORE any god-file split** — this is the safety net. Pin blessed `sector.json`/`sector.md` to `tests/goldens/` behind `UPDATE_*` (mirror `golden_png.rs`). Areas A/B/C/D/E/F all have L-effort splits that need this net first.
5. **Dedup waves + god-file splits** behind the golden net: `SectorReport` trait (B-S1), `edit_world`/`edit_system` helpers (E-S3), CLI `resolve`/`emit_report` (C2/C3), viewer stack unification (F-S1/F1/F2/F7), then the mechanical file splits (A1, B11, D3/D5, E4/E7, F3/F8, C6).

Each area doc ends with its own `## Suggested local order` for intra-area sequencing.

## How to read an area doc

Each finding block carries: review severity + priority bucket, verified status, current `path:line`, a ≤6-line
code excerpt as evidence, why it matters (determinism / undo / perf / drift / correctness), a concrete fix, an
effort rating (S `<1h` / M `≤half-day` / L `multi-day`), and risk/deps (golden-test exposure, ordering, callers).

> Determinism reminders that recur in the docs: RNG only via `src/model/rng.rs`; `Fx*` maps are lookup-only
> (emit via `BTree`/sorted); writers are byte-stable — anything touching `bitmap`/`svg`/`html`/`render` or
> `rng.rs` (A11) must run `cargo test --test it -- golden`, and `gui-core` map changes (F3/F4/F10) also need
> `UPDATE_MAP_SNAPSHOTS=1` re-bless. These are flagged per-finding.
