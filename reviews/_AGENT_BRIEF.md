# Reviewer Brief — Read Before Starting

You are a primary review agent for one work unit of the sectorforge Rust workspace.
Authoritative protocol lives in `RUST_REVIEW.md` (workspace root). This file is the
condensed brief; read RUST_REVIEW.md §2 (finding schema), §3 (rubric), §4 (severity),
§6 (directory guidance) if you need full text.

## Your job

1. Read every assigned file in full.
2. Apply the rubric (§3, summarized below) to every file. Empty categories must be
   reported as "No findings" — silence is not allowed.
3. Write **one report file** at the path your task names, using the schema below.
4. Cite every finding with `path:line` — no abstract claims.
5. Propose a concrete fix or sketch for every finding — no "consider refactoring".

## Output schema (per file)

```markdown
---
unit_id: U017
crate: viewer
paths:
  - viewer/src/render/pipeline.rs
loc_reviewed: 2840
reviewed_by: agent
health_score: 3        # 1 alarming .. 5 exemplary
finding_counts: { critical: 1, high: 4, medium: 9, low: 12, nit: 7 }
top_risks:
  - "Unbounded Vec growth in frame buffer (F-017-003)"
---

# Review: <unit title>

## Summary
2-4 sentences. Biggest theme, structural soundness.

## Findings

### F-017-001 — [HIGH] [Performance] Per-frame allocation in draw loop
- **Location:** `viewer/src/render/pipeline.rs:212-219`
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Hot path — every frame
- **Problem:** A fresh `Vec<Vertex>` is allocated and dropped each call to `draw_batch`.
- **Why it matters:** Steady-state allocator churn; GC-like pauses under load.
- **Evidence:** Read of loop body.
- **Suggested fix:** Hoist buffer to a field cleared with `.clear()`.
  ```rust
  // before: let mut verts = Vec::new();  // inside loop
  // after:  self.scratch.clear();        // field reused
  ```
- **Effort:** S
- **Risk of fix:** Low
```

Finding IDs are `F-<unit>-<seq>` (e.g. `F-009-001`), stable, never reused.

## Severity (uniform across all units)

| Severity | Definition |
|---|---|
| **CRITICAL** | Memory safety / UB / data corruption / security / crash on common input. Ship-blocker. |
| **HIGH** | Panic reachable on realistic input, meaningful hot-path perf regression, silent data/error loss, broken public API contract. |
| **MEDIUM** | Real maintainability/correctness/perf cost but bounded blast radius. |
| **LOW** | Idiomatic improvement, low risk. |
| **NIT** | Style/polish, no functional impact. |

When in doubt between two levels: pick the higher, lower the **Confidence** instead.

## Rubric checklist (apply ALL to every file)

### 3.1 Panics & failure surface
`unwrap`/`expect` on reachable errors; out-of-bounds indexing; integer overflow (use
`checked_`/`saturating_`/`wrapping_`); `panic!`/`unreachable!`/`todo!`/`unimplemented!`
in library code; slicing untrusted length; `.unwrap_unchecked`/`get_unchecked`; div-by-zero;
str byte-boundary slicing. For each: **is it reachable on realistic input?** Severity follows.

### 3.2 unsafe & soundness
N/A in this codebase — there are **zero** `unsafe` blocks. If you encounter one, treat
as CRITICAL until proven sound.

### 3.3 Ownership, borrowing, lifetimes, cloning
Gratuitous `.clone()` of String/Vec/large structs/inside loops; `&str`/`&[T]`/`&Path` over
owned where a borrow would do; `Rc`/`RefCell` dodging the borrow checker rather than real
shared ownership; reference cycles needing `Weak`; over-broad lifetimes / `'static`
forcing clones; unnecessary `Box`/heap; `collect()` → iterate immediately.

### 3.4 Error handling
Library `Result` with typed error vs app/bin `anyhow`-style consistency across crates;
`?` losing context; errors swallowed (`let _ =`, `if let Ok`, `.ok()`, `.unwrap_or_default`);
stringly-typed errors; `Box<dyn Error>` in public API; missing `#[non_exhaustive]` on
public enums; `panic!` as control flow.

### 3.5 Concurrency & async
N/A async (zero in workspace). For threads/rayon: `unsafe impl Send/Sync` (none here);
shared mutable state correctness; lock ordering; `Mutex<...>` patterns. Workspace is
single-threaded except for explicit `rayon` use in `src/search.rs` / similar.

### 3.6 Performance
Hot-path allocation; `Vec`/`HashMap` without `with_capacity` when size known; `format!`
in loops; default `SipHash` where `FxHashMap` acceptable; linear scans where
`HashMap`/`BTreeMap` change big-O; recomputation inside loops; `collect → iterate`;
bounds checks in numeric loops; missing `#[inline]` on tiny cross-crate hot fns;
unbuffered IO; per-frame heap traffic in GUI render paths.
**Performance findings must name the CONTEXT** (hot loop vs. startup vs. once-per-build).

### 3.7 Idiomatic Rust & API design
Naming (RFC 430); `From`/`Into`/`TryFrom`/`Display`/`Debug`; `impl Trait` vs generics
vs `dyn`; newtypes for units/IDs; `#[must_use]`; `match` exhaustiveness;
`let ... else`; visibility minimization (`pub(crate)`/`pub(super)`); private types not
leaking through public signatures; `#[non_exhaustive]` on growable enums; silent `as`
truncation (`u64 as u32`) — prefer `TryFrom`; dead code; commented-out blocks.

### 3.8 Dependencies & Cargo hygiene (per-unit: only flag unused/over-broad imports here)
Bulk of dep work lives in cross-cutting sweep X06. At unit level: flag unused imports
that survived editing, over-broad feature use, and any direct dep that could be removed.

### 3.9 Memory & resource management
`Drop` correctness; resources released deterministically; large stack values;
`Arc`/`Rc` cycles; growing caches with no eviction; `static mut` (should be 0).

### 3.10 Testing & verification (inline `#[cfg(test)]` only — `tests/it/` is U022)
Are error paths tested? Property tests for parsers/math/layout invariants? Doctests on
public APIs? `#[ignore]`d or sleep-based tests? Coverage gaps?

### 3.11 Documentation & maintainability
`//!` module docs; `///` on public items; `# Panics`/`# Errors`/`# Examples`; TODO/FIXME
inventory; magic numbers without named constants; leaky abstractions.

## Project-specific invariants (CLAUDE.md)

These are **hard rules** — violations are at minimum HIGH:

- **Never iterate `FxMap`/`FxHashMap`/`FxSet`/`FxHashSet` for output.** Use
  `BTreeMap`/`BTreeSet`, or sort keys explicitly before emission. Fx aliases are for
  internal lookup only. Determinism is a public guarantee.
- **All RNG draws go through `src/model/rng.rs`** (stage-keyed via `blake3`). Any
  `rand::thread_rng()` or seed from outside that module is a violation.
- **Output writers must be byte-stable.** Renders in `bitmap`, `svg_export`,
  `html_export`, `render` are tested by `cargo test --test it -- golden`. Any HashMap
  iteration in those paths is suspect.
- **Builder mutations go through the command bus.** Direct writes to `BuilderState`
  fields from inside a panel break undo/redo (§R4).

## Discipline (read before each finding)

- Cite or it didn't happen — every finding has `file:line`.
- Propose, don't just diagnose — concrete fix or sketch.
- Severity = reachability × blast radius, not bug cleverness.
- Don't fight the borrow checker on the author's behalf in prose — say what
  restructuring would remove a clone, or mark it acceptable.
- Respect intentional choices — documented `unsafe` with bench is not a finding;
  undocumented is. (No `unsafe` in this codebase, so this is moot.)
- No formatting/style noise as substance — NIT bucket.
- Fewer, higher-quality findings > volume. Aggregator penalizes noise.

## Tool use

You may run targeted `cargo clippy -p <crate>` filtered to your paths. You may grep
freely. **You may not modify, format, or `cargo fix` the tree.** Read-only review.

## Definition of done

- Exactly one report file at the assigned path.
- Every assigned file mentioned at least once (in `paths:` or in findings).
- Every category in §3 addressed (even if "No findings").
- Top-level YAML frontmatter present and well-formed.
- Health score and finding counts present.
- File ends with a `## Summary of suggested fixes` block listing every fix in
  one-line `id — severity — short — effort/risk` form for the aggregator.
