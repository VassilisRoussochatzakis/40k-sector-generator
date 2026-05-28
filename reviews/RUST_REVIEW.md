# RUST_REVIEW.md

**Agentic code-quality review protocol for a ~93,000-LOC Rust workspace.**

This file is the single source of truth for orchestrating a multi-agent review of this
codebase with Claude Code. It defines *how the work is partitioned*, *what each agent
looks for*, *how severe each finding is*, and *the exact format every finding must take*
so that dozens of parallel agents produce output that aggregates cleanly into one report.

The target tree:

```
.
├── src/         # core library / application logic
├── viewer/      # rendering / display surface
├── builder/     # build / asset / data pipeline
├── gui-core/    # shared UI primitives & windowing
└── tests/       # integration / end-to-end test suite (review + optimize)
```

> **Read this entire file before spawning any sub-agent.** Every sub-agent must be given
> this file (or the relevant sections) in its own context, because the value of the review
> depends on uniform criteria and a uniform output schema.

---

## 0. Operating assumptions & token budget

- **No meaningful token limit.** Favor thoroughness over brevity. Do **not** sample files,
  skim, or "spot check." Every `.rs` file in scope must be read in full by exactly one
  *primary* reviewing agent, and may be read again by cross-cutting agents.
- **Determinism over speed.** A finding that is wrong is worse than a finding that is slow
  to produce. When unsure, lower the confidence score rather than dropping the finding.
- **No code changes during review.** This pass is *read + report only*. Agents propose
  diffs in their reports but must not modify, format, or `cargo fix` the tree. A separate
  remediation pass consumes the aggregated report.
- **Ground every claim in a file:line.** No finding may reference "the code" abstractly.
  If an agent cannot cite a location, the finding is not yet real.
- **Prefer compiler/tool evidence.** When `clippy`, `miri`, or the type system can confirm
  a suspicion, run it and cite the output rather than reasoning by eye.

---

## 1. Orchestration model

Run the review in five phases. Phases 0 and 1 are sequential and done by the **orchestrator**
(the top-level Claude Code session). Phases 2 and 3 are massively parallel sub-agents.
Phase 4 is a single aggregation agent.

### Phase 0 — Recon (orchestrator, ~1 pass)

Build a factual map of the workspace before any judgment is made.

1. Locate every `Cargo.toml`. Determine whether this is a single crate or a Cargo workspace,
   and reconstruct the **crate dependency graph** between `src`, `viewer`, `builder`,
   `gui-core` (who depends on whom; is `gui-core` the shared base?).
2. Record per-crate: edition (2015/2018/2021/2024), MSRV if declared, enabled feature flags,
   `[profile.*]` settings, and whether `unsafe` is forbidden via `#![forbid(unsafe_code)]`.
3. Run a **baseline tool sweep** and save raw output to `reviews/_baseline/`:
   ```bash
   cargo build --workspace --all-targets 2>&1 | tee reviews/_baseline/build.txt
   cargo clippy --workspace --all-targets --all-features -- -W clippy::all -W clippy::pedantic -W clippy::nursery 2>&1 | tee reviews/_baseline/clippy.txt
   cargo test  --workspace --no-run 2>&1 | tee reviews/_baseline/test-build.txt
   cargo doc   --workspace --no-deps 2>&1 | tee reviews/_baseline/doc.txt
   cargo tree  --workspace --duplicates 2>&1 | tee reviews/_baseline/dup-deps.txt
   # If available (install only if the user allows):
   cargo +nightly udeps --workspace 2>&1 | tee reviews/_baseline/udeps.txt   || true
   cargo audit 2>&1 | tee reviews/_baseline/audit.txt                          || true
   cargo geiger --workspace 2>&1 | tee reviews/_baseline/geiger.txt           || true
   ```
   If the project does not build, that is itself the **highest-priority finding** — record it
   and continue the review against the source as written (do not block).
4. Produce `reviews/_baseline/inventory.md`: a table of every module with LOC count, `unsafe`
   block count, public-item count, and a one-line "what this does." Use this to size work units.
5. Generate the **work-unit manifest** (Phase 1).

### Phase 1 — Partition (orchestrator)

93k LOC will not fit one agent's context and should not. Partition into **work units** that
respect module boundaries.

- **Target unit size: 1,500–4,000 LOC** of coherent, related code (ideally a module subtree
  or a small set of tightly coupled files). Never split a single file across two primary agents.
- **One primary agent per unit.** It owns that unit's findings.
- Keep units inside a single crate/dir; do not straddle `src`/`viewer`/`builder`/`gui-core`/`tests`.
- Order units so that foundational code (`gui-core`, core types in `src`) is reviewed first;
  later agents can reference the dependency map produced in Phase 0.
- The `tests/` tree is partitioned the same way, but its agents run the **test-optimization
  mandate (§6.5)** instead of the source rubric. Schedule them *after* the source units they
  exercise have been mapped in Phase 0, so a test agent knows what each test is supposed to cover.
- Write the manifest to `reviews/_manifest.md` as a checklist so progress is resumable:
  ```
  - [ ] U001  gui-core/src/layout/      (3,210 LOC)  → reviews/gui-core/layout.review.md
  - [ ] U002  gui-core/src/event/       (1,880 LOC)  → reviews/gui-core/event.review.md
  - [ ] U003  src/model/                (3,950 LOC)  → reviews/src/model.review.md
  ...
  ```

### Phase 2 — Per-unit primary review (parallel sub-agents)

Each primary agent:
1. Reads its assigned files **in full**, plus the crate's public API surface and the relevant
   `Cargo.toml`.
2. Applies the **full rubric in §3** to every file, category by category. It does not stop at
   the first issue in a file.
3. Runs targeted tooling where cheap (e.g. `cargo clippy -p <crate>` filtered to its paths).
4. Writes exactly one report file per unit using the schema in §2. Empty categories are
   reported as "No findings" — silence is ambiguous and not allowed.
5. Emits a unit-level summary block (counts by severity, top 3 risks, overall health 1–5).

### Phase 3 — Cross-cutting sweeps (parallel sub-agents, whole-tree scope)

Some properties are invisible at the unit level and need a dedicated agent that reads across
modules. Spawn one agent per sweep below. Each writes `reviews/_xcut/<name>.review.md`.

| Sweep | Mandate |
|---|---|
| `unsafe-audit` | Every `unsafe` block in the workspace: justification, invariants, soundness, whether a safe alternative exists. See §3.2. |
| `public-api` | Coherence of public surface: needless `pub`, leaking private types, semver hazards, missing `#[non_exhaustive]`, naming consistency (§3.7). |
| `error-model` | Are error types consistent across crates? `Result` vs panic policy, error conversion graph, `?` ergonomics, context loss (§3.4). |
| `concurrency` | All `unsafe impl Send/Sync`, shared-state patterns, lock ordering, channel topology, `async` runtime usage, blocking-in-async (§3.5). |
| `perf-hotpath` | Allocation/clone density, the render/update loop, the build pipeline's IO, big-O surprises (§3.6). |
| `dependencies` | `Cargo.toml` hygiene: unused, duplicated, over-broad features, heavy deps for small needs, supply-chain/`audit` results (§3.8). |
| `panic-surface` | Every `unwrap`/`expect`/`panic!`/`unreachable!`/indexing/slicing/`unwrap_unchecked` and whether it can fire on real input (§3.1). |
| `testing` | Coverage gaps, untested error paths, missing property/fuzz targets, flaky or ignored tests, doctest absence (§3.10). **Scopes the inline `#[cfg(test)]` unit tests inside `src`/`viewer`/`builder`/`gui-core`; the standalone `tests/` tree is owned by the §6.5 test-optimization agents.** Coordinate so coverage gaps aren't double-counted. |

### Phase 4 — Aggregation (single agent)

1. Read every `reviews/**/*.review.md`.
2. **Deduplicate** findings that the same root cause produced in multiple files (e.g. a clone
   pattern repeated 40×). Collapse into one finding with an occurrence list.
3. Re-rank globally by severity × confidence × blast-radius.
4. Produce `reviews/REVIEW_SUMMARY.md`:
   - Executive summary (≤1 page): overall health, top 10 issues, themes.
   - Severity histogram per crate.
   - **Theme clusters** ("pervasive `.clone()` in hot paths", "inconsistent error handling
     between viewer and builder", "no tests on the builder pipeline").
   - A prioritized remediation backlog (quick wins → structural refactors).
   - An explicit "what we are confident about vs. what needs human judgment" section.

---

## 2. Finding schema (mandatory output format)

Every report file is Markdown with this exact structure. Tooling-friendliness matters more
than prose here, so keep the field names verbatim.

````markdown
---
unit_id: U017
crate: viewer
paths:
  - viewer/src/render/pipeline.rs
loc_reviewed: 2840
reviewed_by: agent
health_score: 3        # 1 (alarming) .. 5 (exemplary)
finding_counts: { critical: 1, high: 4, medium: 9, low: 12, nit: 7 }
top_risks:
  - "Unbounded Vec growth in frame buffer (F-017-003)"
  - "Silent error swallowing in texture load (F-017-007)"
---

# Review: viewer/src/render/pipeline.rs

## Summary
2–4 sentences. Overall impression, biggest theme, whether it's structurally sound.

## Findings

### F-017-001 — [HIGH] [Performance] Per-frame allocation in draw loop
- **Location:** `viewer/src/render/pipeline.rs:212-219`
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Hot path — runs every frame (~60–144 Hz)
- **Problem:** A fresh `Vec<Vertex>` is allocated and dropped each call to `draw_batch`.
- **Why it matters:** Steady-state allocator churn; GC-like pauses under load; cache misses.
- **Evidence:** `clippy` flagged nothing (not a lint), confirmed by reading the loop body.
- **Suggested fix:** Hoist the buffer to a reusable field cleared with `.clear()` (retains
  capacity) or a scratch arena. Sketch:
  ```rust
  // before: let mut verts = Vec::new();  // inside the loop
  // after:  self.scratch.clear();        // field reused across frames
  ```
- **Effort:** S (small, localized)
- **Risk of fix:** Low

### F-017-002 — [LOW] [Idiom] Manual index loop where iterator reads cleaner
...
````

**Field rules:**
- `health_score` per unit: 1=needs rewrite, 2=major work, 3=workable with debt, 4=solid,
  5=exemplary/reference-quality.
- Finding IDs are `F-<unit>-<seq>`, stable, never reused.
- `Confidence` ∈ {High, Medium, Low}. Low-confidence findings are still reported, flagged so
  the human can adjudicate.
- `Effort` ∈ {S, M, L, XL}. `Risk of fix` ∈ {Low, Medium, High}.
- Always include a **concrete suggested fix or a sketch**, never "consider refactoring."

---

## 3. The review rubric

This is the core. Apply **every** subsection to **every** file in scope. Each subsection
lists what to hunt for; the boxed examples show the anti-pattern and the preferred form.

### 3.1 Panics & failure surface

Hunt for every way the program can abort on input it might actually see.

- `unwrap()` / `expect()` on `Option`/`Result` where the `None`/`Err` is reachable from
  external input, IO, parsing, or user data. (`unwrap` on a value the code *just* constructed
  and proved present is acceptable but note it.)
- Indexing `slice[i]` / `map[k]` / `s[range]` that can be out of bounds → prefer `.get()`.
- Integer arithmetic that can overflow (in release it wraps silently; in debug it panics):
  prefer `checked_`, `saturating_`, `wrapping_` per intent.
- `panic!`, `unreachable!`, `todo!`, `unimplemented!`, `assert!`/`assert_eq!` in library code
  reachable at runtime.
- Slicing with `..n` / `[a..b]` where `n`/`b` derive from untrusted length.
- `.unwrap_unchecked()`, `get_unchecked()` — these are `unsafe` panics-removed; route to §3.2.
- Division / modulo by a value that can be zero.
- `String`/`str` byte-vs-char-boundary slicing.

> **Anti-pattern**
> ```rust
> let cfg = std::fs::read_to_string(path).unwrap();           // dies on missing file
> let first = parts[0];                                        // dies on empty input
> let mid = total / count;                                     // dies / wraps if count == 0
> ```
> **Preferred**
> ```rust
> let cfg = std::fs::read_to_string(path)
>     .with_context(|| format!("reading config at {}", path.display()))?;
> let first = parts.first().ok_or(ParseError::Empty)?;
> let mid = total.checked_div(count).ok_or(MathError::DivByZero)?;
> ```

For each panic site, the finding must state **whether it is reachable on realistic input**
and the severity follows from that (library + reachable + on-IO = HIGH or CRITICAL).

### 3.2 `unsafe` & soundness

`unsafe` gets the highest scrutiny. For **every** `unsafe` block:

- Is there a `// SAFETY:` comment, and does it actually state the invariants relied upon?
- Are those invariants *true* and *locally checkable*, or do they depend on far-away code?
- Could a future safe edit elsewhere silently break this block's assumptions? (Encapsulation:
  is the unsafe wrapped in a safe API that upholds the invariant?)
- Raw pointers: provenance, aliasing (`&mut` uniqueness), lifetime/use-after-free, alignment,
  null, dangling.
- `transmute`: layout assumptions, `#[repr]` requirements, size/validity (e.g. `bool` must be
  0/1, `char` must be a valid scalar). Prefer `from_bits`/`bytemuck`/`as` where possible.
- `unsafe impl Send`/`Sync`: is it genuinely sound? (route to concurrency, §3.5)
- FFI: ownership across the boundary, who frees, `#[repr(C)]`, panics must not unwind across
  FFI (`catch_unwind` / `extern "C"` abort behavior), null/error conventions.
- `MaybeUninit` / `assume_init`: every field initialized before read.
- `get_unchecked`, `unwrap_unchecked`: is the bound/variant *provably* upheld at the call site?
- Could `miri` catch UB here? Recommend a `cargo +nightly miri test` target for the module.

> Flag any `unsafe` used purely as a perf shortcut where a safe equivalent benchmarks the same.
> "We used `get_unchecked` to skip bounds checks" is only justified with a benchmark **and** a
> proof the index is in range; otherwise it's a latent CVE.

### 3.3 Ownership, borrowing, lifetimes & cloning

The Rust-specific heart of "tighter, faster."

- **Gratuitous `.clone()`** — especially of `String`, `Vec`, large structs, `Arc` (cheap but
  noisy), inside loops, or to "make the borrow checker happy." Ask: could this borrow instead?
  Could ownership be moved? Could it take `&str`/`&[T]` instead of `String`/`Vec<T>`?
- **`.to_string()` / `.to_owned()` / `.to_vec()`** where a borrow would do.
- Function signatures taking `String`/`Vec<T>`/`PathBuf` by value when they only read →
  prefer `&str`/`&[T]`/`&Path`. Conversely, taking `&T` then cloning internally — push the
  clone decision to the caller or take `impl Into<T>`.
- Returning owned data that forces caller allocation when a borrow + lifetime would serve.
- `Rc`/`Arc`/`RefCell`/`Mutex` used to dodge the borrow checker rather than for genuine shared
  ownership — often a sign the data model wants restructuring (e.g. indices into a slab/arena
  instead of pointer-chasing graphs).
- **Reference cycles** with `Rc`/`Arc` → memory leak; should there be `Weak`?
- Lifetime annotations that are over-broad or could be elided; `'static` bounds that force
  unnecessary owning/cloning.
- Unnecessary `Box`/heap indirection on small types; `Box<dyn Trait>` where generics/`impl
  Trait` would monomorphize and inline.
- `collect()` into a `Vec` only to iterate again immediately — keep the iterator lazy.

> **Anti-pattern**
> ```rust
> fn render(title: String, items: Vec<Item>) { for i in &items { draw(&i, &title); } }
> // caller forced to clone its own data to call this
> ```
> **Preferred**
> ```rust
> fn render(title: &str, items: &[Item]) { for i in items { draw(i, title); } }
> ```

### 3.4 Error handling & the `Result` model

- Library code returning `Result` with a *typed* error (`thiserror`/hand-rolled enum), app/bin
  edges using `anyhow`/`eyre` — is this layering consistent across crates? Flag mixing.
- `?` losing context (no `with_context`/source chain) so production errors are unactionable.
- Errors swallowed: `let _ = fallible();`, `if let Ok(x) = ...` with the `Err` arm dropped,
  `.ok()` discarding the error, `.unwrap_or_default()` masking failures.
- Stringly-typed errors (`Err("bad input".to_string())`) — no programmatic handling possible.
- `Box<dyn Error>` in a public library API where callers need to match on variants.
- Error enums missing `#[non_exhaustive]` (semver hazard) or not implementing `std::error::Error`/`Display`/`source()`.
- `panic!` used as control flow where `Result` is the right call.
- Inconsistent error→exit-code mapping in the `builder` binary.

### 3.5 Concurrency & async

- Every `unsafe impl Send`/`Sync` re-audited for soundness (link to §3.2).
- Shared mutable state: is the locking discipline sound? **Lock ordering** consistent across
  call sites (deadlock risk)? Locks held across `.await` (and thus across yields)?
- `Mutex`/`RwLock` held longer than necessary; cloning data while holding a lock vs. dropping
  the guard first.
- `std::sync::Mutex` vs `parking_lot` vs async `tokio::sync::Mutex` — right tool? Async mutex
  used where a sync one would do (and vice versa)?
- **Blocking in async**: `std::fs`, `std::thread::sleep`, heavy CPU, or blocking IO on an async
  runtime worker thread → starves the executor. Should be `spawn_blocking` / async IO.
- Channels: bounded vs unbounded (unbounded = latent OOM under backpressure); dropped receivers
  causing send errors; `select!` fairness.
- Atomics with wrong `Ordering` (`Relaxed` where `Acquire`/`Release` needed) — subtle, high-severity.
- Data races only reachable under threading — reason about interleavings, not just the happy path.
- Spawned tasks/threads whose `JoinHandle` is dropped (errors silently lost) or never joined.
- `Arc<Mutex<T>>` where a message-passing or `Arc<T>`-with-immutable design would be simpler.

### 3.6 Performance

Performance findings must name the **context** (hot loop vs. startup vs. once-per-build) — an
allocation in a 144 Hz render loop is HIGH; the same at program init is a NIT.

- Allocation in hot paths (see §3.3); reusable buffers (`Vec::clear` retains capacity).
- `Vec`/`HashMap` built without `with_capacity` when the size is known → repeated reallocation.
- `format!`/`String` concatenation in loops vs. `write!` into a reused buffer.
- `HashMap` default SipHash where a faster hasher (`FxHashMap`, `ahash`) is acceptable (not
  for untrusted keys — note the DoS tradeoff).
- Linear scans where a `HashMap`/`BTreeMap`/sort+binary-search changes the big-O.
- Repeated work that could be hoisted/memoized; recomputation inside loops.
- `collect()` → re-iterate; chained iterator adapters that allocate intermediates.
- Excessive monomorphization bloat (huge generic fns instantiated many ways) vs. `dyn`.
- Bounds checks in tight numeric loops — prefer iterators (which the compiler elides) over
  indexing, rather than reaching for `unsafe`.
- `#[inline]` missing on tiny cross-crate hot fns, or present where it bloats with no benefit.
- IO: unbuffered `Read`/`Write` (wrap in `BufReader`/`BufWriter`); per-item syscalls; reading
  whole files when streaming suffices (relevant to `builder`).
- For the render path (`viewer`/`gui-core`): per-frame heap traffic, redundant GPU
  uploads/state changes, layout/relayout thrash, full redraws where damage-tracking would do.
- Recommend `cargo build --release` + `criterion` benches or `flamegraph`/`perf` before
  asserting a fix helps; mark perf claims that lack a benchmark as Medium confidence.

### 3.7 Idiomatic Rust & API design

- Naming per RFC 430 (types `UpperCamel`, fns/vars `snake_case`, consts `SCREAMING_SNAKE`).
- Constructor conventions (`new`, `with_*`, `try_new`), `Default` where sensible, builder
  pattern for many-arg construction.
- Trait usage: `From`/`Into`, `TryFrom`, `Display`/`Debug`, `AsRef`/`Deref` not abused,
  `Iterator` impls, `PartialEq`/`Eq`/`Hash` consistency.
- `impl Trait` in arg/return position where it clarifies; generics vs `dyn` chosen deliberately.
- Newtypes for units/IDs instead of bare `u32`/`usize` (prevents mixing a `WidgetId` with a
  `NodeId`).
- `#[must_use]` on types/fns whose result must not be ignored.
- Pattern matching exhaustiveness; `match` over `if let` chains; `let ... else` for early exit;
  `if let` chains / `matches!` for clarity.
- Visibility: minimum `pub` necessary; `pub(crate)`/`pub(super)` instead of blanket `pub`;
  private types not leaking through public signatures.
- Module organization: god-modules, circular `mod` references, files that should be split.
- `#[non_exhaustive]` on public enums/structs that will grow (semver).
- Avoid `as` casts that silently truncate (`u64 as u32`) → `try_into()`/`TryFrom`.
- Prefer `?` and combinators over nested `match`; prefer `&str` methods over manual byte work.
- Dead code, unused `pub`, commented-out blocks, `#[allow(dead_code)]` masking real rot.

### 3.8 Dependencies & Cargo hygiene

- Unused dependencies (`cargo udeps`), duplicated transitive versions (`cargo tree -d`).
- Over-broad features (e.g. `tokio` with `features=["full"]` when a few suffice; default
  features not disabled where a lighter subset works).
- Heavyweight crate pulled in for a trivial need (e.g. full `regex` for one fixed-prefix check;
  `chrono` where `time` or `std` suffices).
- Pinning: are versions appropriately constrained? Any `git`/`path` deps that should be crates?
- `cargo audit` advisories (RUSTSEC); unmaintained crates.
- `unsafe` surface of dependencies (`cargo geiger`) for security-sensitive paths.
- Workspace dedup: shared deps hoisted to `[workspace.dependencies]`?
- Build-time cost: proc-macro-heavy deps, `build.rs` doing too much — relevant to `builder`.
- License compatibility if this ships (note, don't adjudicate legally).

### 3.9 Memory & resource management

- `Drop` impls: correctness, ordering assumptions, panicking in `Drop` (double-panic = abort).
- Resources (files, sockets, GPU handles, locks) released deterministically; RAII vs. manual.
- `mem::forget`/`ManuallyDrop` misuse → leaks.
- Large stack values (big arrays/structs by value) risking stack overflow → `Box`.
- `Arc`/`Rc` cycles (§3.3); growing caches/maps with no eviction (unbounded memory).
- `static mut` (almost always wrong/UB) → `OnceLock`/`LazyLock`/atomics.

### 3.10 Testing & verification

- Are error paths tested, not just the happy path? Untested `?`-returning branches.
- Property tests (`proptest`/`quickcheck`) for parsers, serializers, math, layout invariants.
- Fuzz targets (`cargo-fuzz`) for anything parsing untrusted bytes (relevant to `builder`/`viewer` loaders).
- Doctests on public APIs (also serve as compile-checked docs).
- `#[ignore]`d or flaky tests, tests with `sleep`-based timing, tests asserting nothing.
- Snapshot tests (`insta`) for rendering/serialization output where appropriate.
- `miri` coverage for `unsafe`-heavy modules.
- Coverage gaps: which public functions have **zero** tests touching them?

### 3.11 Documentation & maintainability

- `//!` crate/module docs explaining purpose and invariants; `///` on public items.
- Docs that explain *why*, not just restate the signature.
- `# Safety` sections on `unsafe fn`; `# Panics`/`# Errors` sections where relevant.
- `# Examples` that are doctests.
- TODO/FIXME/HACK/XXX inventory with an assessment of which are real risks.
- Magic numbers without named constants; unexplained bit-twiddling.
- Inconsistent abstractions / leaky abstractions across the crate boundary.

---

## 4. Severity classification

Severity is the function the aggregator sorts on. Apply it uniformly.

| Severity | Definition | Examples |
|---|---|---|
| **CRITICAL** | Memory safety / UB, data corruption, security hole, or a crash on common input. Ship-blocker. | Unsound `unsafe`, data race, use-after-free, `unwrap` on routine IO in a hot library path, OOM via unbounded channel. |
| **HIGH** | Reachable panic on realistic-but-not-constant input, meaningful perf regression in a hot path, silent data/error loss, broken public API contract. | Per-frame allocation in render loop, swallowed errors, lock held across `.await` causing stalls, semver-breaking public type leak. |
| **MEDIUM** | Real maintainability/correctness/perf cost but bounded blast radius. | Pervasive `.clone()` off the hot path, stringly-typed errors, missing `with_capacity`, inconsistent error model within a crate. |
| **LOW** | Idiomatic improvement with clear benefit, low risk. | Manual index loop → iterator, `&str` instead of `String` arg, missing `#[must_use]`. |
| **NIT** | Style/polish; no functional impact. | Naming, doc phrasing, import ordering, redundant `return`. |

When in doubt between two levels, pick the higher and lower the **confidence** instead. Note
the **multiplier**: a MEDIUM pattern that recurs 200× across the tree should be surfaced by the
aggregator as a HIGH *theme*, even though each instance is MEDIUM.

---

## 5. Tooling reference

Sub-agents should *use* the toolchain, not just read. Suggested commands (read-only intent):

```bash
# Lints — the single highest-value automated signal
cargo clippy --workspace --all-targets --all-features -- \
  -W clippy::all -W clippy::pedantic -W clippy::nursery -W clippy::cargo

# Per-crate, to scope an agent's noise
cargo clippy -p <crate> --all-targets

# Undefined-behavior detection on unsafe-heavy modules (nightly)
cargo +nightly miri test -p <crate>

# Dependency hygiene
cargo tree -d                  # duplicate versions
cargo +nightly udeps           # unused deps
cargo audit                    # RUSTSEC advisories
cargo geiger                   # unsafe surface of the dep graph

# Format drift (report only; do not run `fmt --write` during review)
cargo fmt --all -- --check

# API surface diff for the public-api sweep
cargo public-api -p <crate>    # if installed

# Perf (remediation/validation phase, not strictly review)
cargo build --release && cargo bench           # criterion benches if present
# flamegraph / perf / samply for profiling hot paths
```

Interpreting clippy: do **not** blindly accept every `pedantic`/`nursery` lint — some are
opinionated. The agent's job is to triage clippy output into *real* findings (cite the lint
name) and discard the noise, explaining why for anything dismissed.

---

## 6. Directory-specific guidance

The rubric is universal, but each crate has a characteristic risk profile. Weight attention
accordingly.

### `gui-core/` — shared UI primitives & windowing
Reviewed **first**; everything else depends on it, so its API design and soundness have the
largest blast radius.
- **API design (§3.7)** is paramount — churn here ripples through `viewer` and `src`. Scrutinize
  `pub` surface, `#[non_exhaustive]`, trait coherence, newtypes for IDs/handles.
- **`unsafe` (§3.2)** likely concentrated here (windowing, raw-handle FFI, GPU/native interop):
  audit every block, FFI ownership, `Send`/`Sync` claims on handles.
- Event/dispatch hot paths: allocation per event, vtable churn, downcasting.
- Layout invariants — candidates for property tests.

### `viewer/` — rendering / display surface
The **performance-critical** crate; the rubric's §3.6 weighting is highest here.
- Per-frame allocation, redundant uploads/state changes, redraw vs. damage tracking.
- Resource lifecycle (§3.9): GPU buffers/textures/handles freed deterministically; no leaks
  across frames; bounded caches.
- Any decoders/loaders parsing untrusted asset bytes → §3.1 panics + §3.10 fuzzing.
- Math/transform code → overflow, precision, property tests.
- `unsafe` for GPU/buffer mapping → §3.2 + `miri` where feasible.

### `builder/` — build / asset / data pipeline
**IO-, error-, and correctness-critical**; likely the binary edge.
- **Error model (§3.4)**: this is where `anyhow`/context + clean exit codes matter most;
  every failure should be actionable, nothing swallowed.
- **IO performance (§3.6)**: buffering, streaming vs. read-to-end, parallelism (rayon/threads),
  syscall counts, large-file handling.
- Untrusted input parsing → §3.1 + fuzzing (§3.10).
- Path handling: `Path`/`PathBuf` correctness, traversal/escaping, cross-platform separators.
- Determinism/reproducibility of build output; idempotency; partial-failure cleanup.
- `build.rs` and proc-macro/dependency build cost (§3.8).

### `src/` — core library / application logic
The **data-model and orchestration core**; correctness and ownership design dominate.
- **Ownership/borrowing (§3.3)**: data model shape — are graphs modeled with `Rc<RefCell<…>>`
  (smell) vs. arenas/slab+indices? Cycles/leaks?
- Error model consistency (§3.4) as the layer that composes `viewer`/`builder`/`gui-core`.
- State management & invariants; are illegal states unrepresentable (typestate, enums over
  bool-soup)?
- Concurrency (§3.5) if this hosts the async runtime / shared state.
- Public API (§3.7) if `src` is a consumed library.

### `tests/` — integration / end-to-end test suite *(distinct mandate: optimize, don't just review)*

> This is the **fifth agent domain**, and it is different from the other four. Source agents ask
> *"is this code good?"* Test agents ask *"is this suite **fast, trustworthy, and worth its
> maintenance cost**, and does it actually cover what matters?"* A slow, flaky, or redundant test
> suite is a tax on every future change, so findings here are weighted toward **throughput and
> signal quality**, not just style. Use the same finding schema (§2) and severities (§4), but
> draw findings from the checklist below rather than the source rubric. Tag every finding
> `[Tests]` in the category field.

Spawn `tests/` agents per work unit (same 1,500–4,000 LOC sizing). Before judging, each agent
should establish a **baseline** for its unit and record it in the report:

```bash
# Per-test timing — the single most important signal for optimization
cargo test --workspace -- -Z unstable-options --report-time 2>&1 | tee reviews/_baseline/test-time.txt   # nightly
# stable fallback: cargo-nextest gives per-test timing, parallelism, and slow-test detection
cargo nextest run --workspace 2>&1 | tee reviews/_baseline/nextest.txt
cargo nextest run --workspace --no-fail-fast                              # surface ALL failures, not just first
# Detect flakiness: run the suite repeatedly; anything non-deterministic is a finding
for i in $(seq 1 10); do cargo nextest run --workspace; done
# Coverage — find what the suite does NOT exercise
cargo llvm-cov --workspace --html 2>&1 | tee reviews/_baseline/coverage.txt
```

**Test-optimization checklist (the §6.5 mandate):**

*Speed & throughput*
- **Slowest tests first.** Rank tests by wall-clock; the top 10% usually dominate total time.
  Each slow test is a finding with a target (parallelize, shrink fixture, replace sleep, mock IO).
- **`thread::sleep` / fixed timeouts** used to "wait for" async work → replace with polling,
  condvars, channels, or fake clocks. These are the #1 cause of both slowness *and* flakiness.
- **Real IO / network / filesystem / DB** where a temp dir (`tempfile`), in-memory fake, or
  mock would be faster and hermetic. Per-test process spawning or container startup is a red flag.
- **Expensive setup repeated per test** that could be a shared, lazily-built fixture
  (`OnceLock`/`LazyLock`, or `#[fixture]` if using `rstest`) — without introducing shared
  *mutable* state that breaks isolation.
- **Serialized tests** (`serial_test`, global locks, `--test-threads=1`) — is the serialization
  genuinely required, or an artifact of shared state that should be removed so tests parallelize?
- **Compile time of the test target itself**: giant single test files, heavy generic test
  helpers, or dev-dependencies that balloon build time. Splitting integration tests across files
  matters because each file in `tests/` is its own compiled binary.
- **Over-broad integration tests** doing end-to-end work to assert a unit-level fact → push down
  to a fast unit test; reserve E2E for genuine cross-component contracts.

*Trustworthiness*
- **Flaky / non-deterministic tests**: time-, ordering-, thread-, or hash-iteration-dependent
  assertions; reliance on `HashMap` iteration order; uncontrolled randomness (seed it).
- **`#[ignore]`d tests** — each is either dead weight to delete or a real gap to re-enable; never
  leave it ambiguous. Same for commented-out tests.
- **Tests that assert nothing** (run code, no `assert`), or assert something trivially true, or
  whose assertion can't actually fail. These give false confidence — flag as HIGH.
- **Over-mocking** so the test validates the mock, not the code; **under-asserting** so real
  regressions slip through.
- **Tautological / change-detector tests** that just mirror the implementation and break on any
  refactor without catching real bugs.
- Tests catching panics broadly (`should_panic` without `expected = "..."`) that would pass on
  the *wrong* panic.

*Coverage & gaps (consume the §3.10 findings + coverage report)*
- **Error paths**: most suites test the happy path only. Identify untested `Err`/`None` branches
  and the public functions with zero coverage.
- **Property tests** (`proptest`/`quickcheck`) for parsers, serializers, math, and layout
  invariants instead of a handful of hand-picked cases.
- **Fuzz targets** (`cargo-fuzz`) for anything in `builder`/`viewer` that parses untrusted bytes.
- **Snapshot tests** (`insta`) for rendering/serialization output, with a note on snapshot
  hygiene (committed, reviewed, not blindly accepted).
- **Doctests** as cheap, compile-checked coverage of public API examples.

*Redundancy & maintainability*
- **Duplicate coverage**: multiple tests exercising the identical path with no added value →
  consolidate or parameterize (`rstest` cases / table-driven tests).
- **Copy-pasted setup** across files → shared `mod common;` test helpers / builders.
- **Brittle fixtures**: large hardcoded blobs, absolute paths, environment assumptions, golden
  files that are hard to regenerate.
- **Poor failure messages**: bare `assert!(x == y)` → `assert_eq!` or a message, so a CI failure
  is diagnosable without a local repro.
- **Test organization**: unit tests living far from the code they test; integration tests that
  belong as unit tests and vice versa; missing `#[cfg(test)]` gating that leaks test code into
  release builds.

**Test agent report addendum.** In addition to the standard schema, each `tests/` unit report
ends with:
- A **slowest-tests table** (test name, current time, proposed optimization, expected speedup).
- A **flakiness ledger** (test name, failure rate over N runs, suspected cause).
- A **coverage-gap list** (public items / error paths with no test).
- An estimate of **total-suite-time reduction** if the unit's findings are applied.

---

## 7. Aggregator output contract (`reviews/REVIEW_SUMMARY.md`)

The final deliverable must contain, in order:

1. **Executive summary** — overall health (1–5), the 10 highest-priority findings with IDs and
   one-line rationale, and the 3–5 dominant themes.
2. **Severity histogram** — table of counts by severity × crate.
3. **Theme clusters** — recurring patterns with occurrence counts and representative IDs.
4. **Critical & High findings** — full detail, grouped by crate, each with its suggested fix.
5. **Remediation backlog** — ordered for action:
   - *Quick wins* (S effort, Low risk, ≥Medium severity) — do first.
   - *Targeted fixes* (M effort).
   - *Structural refactors* (L/XL) — with a short rationale and rough sequencing.
6. **Test-suite health** — current total suite wall-clock, count of slow/flaky/ignored/empty
   tests, headline coverage number, and the **projected suite-time reduction** if the `tests/`
   findings are applied. Roll up the per-unit slowest-tests tables and flakiness ledgers here.
7. **Confidence ledger** — what the review is confident about vs. what needs human judgment or
   profiling/benchmarking to confirm.
8. **Coverage statement** — LOC reviewed, units completed vs. manifest, any files skipped and why.

---

## 8. Reviewer discipline (read before every finding)

- **Cite or it didn't happen.** Every finding has a `file:line`.
- **Propose, don't just diagnose.** Each finding carries a concrete fix or sketch.
- **Severity reflects reachability and blast radius, not how clever the bug is.**
- **Don't fight the borrow checker on the author's behalf in prose** — if a clone looks
  necessary, say what restructuring would remove it, or mark it acceptable.
- **Respect intentional choices.** A documented `unsafe` perf optimization with a benchmark is
  not a finding; an undocumented one is. Look for the rationale before flagging.
- **No formatting/style noise masquerading as substance** — those are NITs, kept separate.
- **Prefer fewer, higher-quality findings over volume.** Duplicates and speculation dilute the
  report; the aggregator will penalize noise.
- **When the type system or a tool can prove it, prove it.** Reasoning-by-eye is Medium
  confidence at best for soundness/perf claims.

---

*End of protocol. Begin at Phase 0.*
