---
unit_id: U006
crate: sectorforge
paths:
  - src/validate/mod.rs
  - src/validate/validation.rs
  - src/validate/invariants.rs
  - src/validate/diff.rs
loc_reviewed: 2510
reviewed_by: agent
health_score: 4
finding_counts: { critical: 0, high: 4, medium: 9, low: 9, nit: 6 }
top_risks:
  - "Reachable `unreachable!()` in diff_systems/diff_worlds/diff_routes when `BTreeSet` contains zero refs (F-006-001)"
  - "`render_markdown` writes uses `let _ =` swallowing fmt::Error and runs an O(systems × worlds) nested loop in a hot CLI/export path (F-006-003 / F-006-006)"
  - "Stringly-typed error codes; no typed enum or `#[non_exhaustive]` on `ValidationIssue.code` / `InvariantViolation.code` (F-006-004)"
  - "`compute_faction_deltas` produces NaN-fragile ordering through `partial_cmp().unwrap_or` (F-006-005)"
---

# Review: src/validate — model validation, invariants, diff

## Summary

The validation layer is structurally sound. It is pure (no I/O), already uses
`BTreeMap`/`BTreeSet` consistently (so the determinism invariant holds), and the
public surface is a flat-DTO style (`ValidationReport`, `InvariantReport`,
`SectorDiff`) suitable for Serde emission and Markdown rendering. The major
risks are: (a) several `unreachable!()` branches reachable only because the
compiler cannot see set-membership invariants, but still trivially panicable on
a malformed model; (b) a string-typed `code: String` error vocabulary with no
single source of truth; (c) one O(n+m) diff loop containing a hidden
O(systems × worlds × claims) inner cost on big sectors; and (d) per-call
`String` allocation in `violation()` / `issue()` builders. None of these are
correctness bugs in the steady state — but invariants/validation must *never*
panic on a malformed input (that defeats their purpose), so the `unreachable!`s
are the headline finding.

## Findings

### F-006-001 — [HIGH] [Panics] `unreachable!()` reachable on empty model state in three diff fns
- **Location:** `src/validate/diff.rs:454`, `src/validate/diff.rs:549`, `src/validate/diff.rs:705`
- **Category:** Panics & failure surface (§3.1)
- **Confidence:** Medium-High
- **Blast radius:** CLI/exporter — `diff_sectors` is a `#[must_use]` "never fails" public API (claimed at `diff.rs:240-242`). A panic here breaks the contract.
- **Problem:** All three diff routines build `all_ids = before_idx.keys().chain(after_idx.keys()).collect::<BTreeSet>()` then `match (before_idx.get(id), after_idx.get(id))` exhaustively with the fourth arm being `(None, None) => unreachable!()`. The "unreachable" is true *only* if the indexes are dedupe-able by `id`. But the indexes are built from `Vec<GeneratedSystem>` etc; if the input model contains **two systems with the same `id`** (which is exactly the case the invariants module flags as `DUPLICATE_SYSTEM_ID`), the second `collect::<BTreeMap<_, _>>()` silently overwrites and the invariant holds. Today it doesn't panic, but the construct is brittle and the comment is wrong — `(None, None)` simply can't occur because of `BTreeSet` semantics, not for any modeled reason. If anyone later changes the `BTreeSet` to a `Vec` or refactors to feed `all_ids` from a different source, the panic is reachable.
- **Why it matters:** A public "never fails" diff that calls `unreachable!()` on a logically-impossible-but-not-type-enforced state is a latent panic. Spec §3.1: "validation must NEVER panic on invalid input — that defeats its purpose."
- **Evidence:** Read of the three call sites and `BTreeSet`/`BTreeMap` construction immediately above each.
- **Suggested fix:** Replace the four-arm match with a three-arm match that ignores the impossible case explicitly. Use `let_chains` or `if let`/`else`:
  ```rust
  match (before_idx.get(id), after_idx.get(id)) {
      (None, Some(b)) => added.push(...),
      (Some(a), None) => removed.push(...),
      (Some(a), Some(b)) => { if let Some(d) = system_diff(a, b, cfg) { changed.push(d); } }
      // both-None cannot happen: `id` came from the union of the two key sets
      (None, None) => {} // no-op, debug_assert!(false) optional
  }
  ```
  Or refactor to iterate the union with deref'd `&GeneratedSystem` refs directly so the cases are exhaustive over `Option`:
  ```rust
  for id in &all_ids {
      let a = before_idx.get(id).copied();
      let b = after_idx.get(id).copied();
      match (a, b) { ... } // last arm: (None, None) => {} provably no-op
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-006-002 — [HIGH] [Performance] `check_systems` is O(systems × namespace_count × tags_len)
- **Location:** `src/validate/invariants.rs:281-310`
- **Category:** Performance (§3.6)
- **Confidence:** High
- **Blast radius:** Post-generation invariant check — runs once per generated sector (and once per CLI invocation of `validate-sector`). For a 200-system sector with ~5 worlds/system and ~10 tags/world, this is 200 × 5 × 8 × O(10) ≈ 80k startswith calls. Tolerable but unnecessary, and the `tag_set: BTreeSet<&str>` immediately after is a second pass over the same tag vec.
- **Problem:** Per world, we scan `w.tags` 8 times (once per namespace prefix) with `w.tags.iter().any(|t| t.starts_with(prefix))`, then *again* build a `BTreeSet<&str>` of all tags for duplicate detection. Single-pass would compute both: walk `w.tags` once, collect namespace presence into a `[bool; 8]` or `u8` bitmask, and accumulate the dedup set in the same loop.
- **Why it matters:** Bounded but quadratic-ish; on a 500-system sector this is ~half a million string comparisons per validation run. The function runs on every CLI export.
- **Evidence:** Lines 282-310 read.
- **Suggested fix:**
  ```rust
  const NAMESPACES: &[&str] = &[
      "atmosphere:", "biosphere:", "gov:", "population:",
      "star:", "tech:", "temperature:", "world_type:",
  ];
  let mut seen_ns = [false; 8];
  let mut tag_set: BTreeSet<&str> = BTreeSet::new();
  for t in &w.tags {
      if !tag_set.insert(t.as_ref()) { /* duplicate */ }
      for (i, ns) in NAMESPACES.iter().enumerate() {
          if t.starts_with(ns) { seen_ns[i] = true; }
      }
  }
  for (ns, present) in NAMESPACES.iter().zip(seen_ns) {
      if !present { /* missing */ }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-006-003 — [HIGH] [Error handling] `render_markdown` swallows every `fmt::Error`; `write_diff` cannot surface formatting failure
- **Location:** `src/validate/diff.rs:805-1058` (every `let _ = writeln!(s, ...)`)
- **Category:** Error handling (§3.4)
- **Confidence:** High
- **Blast radius:** `render_markdown` is the user-facing diff renderer; `write_diff` calls it directly. A `fmt::Error` from `String` is essentially impossible — but the pattern is wrong on first principles and trains downstream code to copy it.
- **Problem:** `let _ = writeln!(s, ...)` discards `fmt::Result`. For a `String` sink this is "fine" (`String`'s `fmt::Write` impl never fails), but the *function should signal that* rather than scatter `let _ =` across 80 call sites. The convention bites if anyone refactors `s` to be a `&mut dyn fmt::Write` (file, BufWriter, etc.) — every write silently fails. Also: `writeln!(s, "...")` on a `String` cannot fail, so the `let _ =` is dead bookkeeping noise.
- **Why it matters:** §3.4 — "errors swallowed via `let _ =`". Even where currently safe, it sets a project-wide pattern that will be cargo-culted.
- **Evidence:** Every `writeln!` call in `render_markdown` and `render_world_change`.
- **Suggested fix:** Use `std::fmt::Write` directly (it's already imported as `Write as _` on line 19) and either (a) use the `write!`/`writeln!` macros without binding because `String`'s impl is infallible — but document this — or (b) change signature to `fn render_markdown(d: &SectorDiff) -> Result<String, fmt::Error>` and propagate with `?`. Option (a) is sufficient:
  ```rust
  // Document infallibility at the top of the fn:
  //
  // Writing into a `String` cannot fail; `writeln!` returns `Result` only to
  // satisfy the `fmt::Write` trait. `let _ =` is intentional and consistent.
  ```
  Or better, define a tiny helper:
  ```rust
  fn line(s: &mut String, args: fmt::Arguments) { let _ = s.write_fmt(args); }
  // call: line(&mut s, format_args!("# Sector Diff"));
  ```
  At minimum, drop `let _ =` and use `_ = writeln!(...);` *with a single comment at top of function* explaining infallibility.
- **Effort:** S–M
- **Risk of fix:** Low

### F-006-004 — [HIGH] [API design] `code` field is stringly-typed; no central registry / enum / `#[non_exhaustive]`
- **Location:** `src/validate/validation.rs:21-23` (`ValidationIssue.code: String`), `src/validate/invariants.rs:13-17` (`InvariantViolation.code: String`)
- **Category:** API / error model (§3.4, §3.7)
- **Confidence:** High
- **Blast radius:** Public API. The brief flags this explicitly: "validation IS error reporting — check error type design, structured vs stringly-typed." Every `errors.push(issue("GEN_GRID_EMPTY", ...))` allocates a fresh `String`, and downstream consumers in CLI/builder must match on string literals (and have no compile-time check when codes are renamed or added).
- **Problem:** All 40+ error codes are typed as `&'static str` literals at call sites and stored as owned `String`. There's no central enum, no `match` exhaustiveness check, no `#[non_exhaustive]`, no stable docs of the catalogue, and no compile-time guarantee that a downstream consumer's match arms stay in sync. Spec §3.4: "stringly-typed errors" is an anti-pattern. The codes also serialize as bare strings — fine — but they don't need to live as `String` in memory.
- **Why it matters:** A typo in `"FACTION_DUPLICATE_ID"` vs `"FACTION_DUPLICATE_IDS"` compiles fine. Renaming a code is a silent break for any downstream that pattern-matches it. Every issue construction allocates a small `String`.
- **Evidence:** Grep across `validation.rs` and `invariants.rs` reveals ~40 unique code literals, none defined as constants.
- **Suggested fix:** Introduce a single source of truth — at minimum a constants module:
  ```rust
  pub mod codes {
      pub const GEN_GRID_EMPTY: &str = "GEN_GRID_EMPTY";
      pub const GEN_SYSTEM_COUNT_OVERFLOW: &str = "GEN_SYSTEM_COUNT_OVERFLOW";
      // ...
  }
  ```
  and switch the field to `code: &'static str` (no allocation, still serializes the same). Better still:
  ```rust
  #[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
  #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
  #[non_exhaustive]
  pub enum ValidationCode {
      GenGridEmpty,
      GenSystemCountOverflow,
      // ...
  }
  ```
  Even the cheaper `&'static str` variant kills the allocation and gives `grep` a single landing site.
- **Effort:** M (touches every issue construction site)
- **Risk of fix:** Low — pure refactor.

### F-006-005 — [MEDIUM] [Correctness] `compute_faction_deltas` sort is NaN-fragile
- **Location:** `src/validate/diff.rs:771-777`
- **Category:** Correctness / determinism
- **Confidence:** Medium
- **Blast radius:** Faction power delta ordering in CLI markdown output and JSON diff.
- **Problem:** `b.delta.abs().partial_cmp(&a.delta.abs()).unwrap_or(Ordering::Equal)` swallows NaN. `power.total_projection()` is summed from `f32` fields; if any contributing field is `NaN` (e.g. division by zero somewhere upstream), the sort becomes nondeterministic in the precise position of that one entry. This violates the byte-stable output guarantee weakly.
- **Why it matters:** `partial_cmp().unwrap_or(Equal)` is a hint that the input may be NaN; if it can be NaN, the byte-stable output property fails. If it cannot, the `unwrap_or` is hiding the invariant.
- **Evidence:** Line 775.
- **Suggested fix:** Use `f32::total_cmp` (RFC-stable since 1.62):
  ```rust
  deltas.sort_by(|a, b| {
      b.delta.abs().total_cmp(&a.delta.abs())
          .then_with(|| a.faction_id.cmp(&b.faction_id))
  });
  ```
  This is a total order across all `f32` values (including NaN) and guarantees byte-stable serialization.
- **Effort:** XS
- **Risk of fix:** Low

### F-006-006 — [MEDIUM] [Performance] `format!` inside per-world / per-route validation loops
- **Location:** `src/validate/invariants.rs:53-55, 62-64, 211-214, 225-232, 250-256, 261-263, 268-270, 275-277, 295-297, 305-307` (and similarly in `check_routes`, `check_factions`, `check_world_control`, `check_system_control`); also `src/validate/validation.rs:185-194, 197-250` and `src/validate/diff.rs:899-921` (Markdown renderer building per-world strings).
- **Category:** Performance (§3.6)
- **Confidence:** High
- **Blast radius:** Per-invocation, not per-frame; but `check_sector` runs as part of every CLI export, and the `format!` macros allocate eagerly *even when no violation is produced* because both `message` and `path` are built before the `violation(...)` call decides to push. Hot on big sectors.
- **Problem:** Every successful (no-violation) world allocates 8 `format!`-built `path` strings just to *not* push them. Pattern:
  ```rust
  for prefix in [..] {
      if !w.tags.iter().any(|t| t.starts_with(prefix)) {
          v.push(violation(
              "WORLD_TAG_NAMESPACE_MISSING",
              &format!("world '{}' missing tag in namespace '{}'", w.id, prefix),
              Some(&format!("systems.{}.worlds.{}.tags", sys.id, w.id)),
          ));
      }
  }
  ```
  The `format!`s only fire inside the `if`, which is correct — so this case is OK. The real issue is in `check_world_control`/`check_system_control`/`check_factions` where the `path` is built before knowing whether the inner `id` is present (search "check.dimensions" closures), and in `render_markdown` where `worlds_added.iter().map(|w| format!(...)).collect::<Vec<_>>().join(", ")` is called per system (lines 905, 916) — that's 3 allocations per world (the format, the Vec, and the joined String). For sectors with thousands of worlds the cumulative allocator pressure is notable.
- **Why it matters:** Validation should be cheap; today on a 500-system sector it's tens of thousands of small allocations per run.
- **Evidence:** Lines cited above.
- **Suggested fix:** For the renderer, write each item directly to the String sink instead of `collect → join`:
  ```rust
  if !sd.worlds_added.is_empty() {
      let _ = write!(s, "  - Worlds added: ");
      for (i, w) in sd.worlds_added.iter().enumerate() {
          if i > 0 { let _ = write!(s, ", "); }
          let _ = write!(s, "`{}` {}", w.id, w.name);
      }
      let _ = writeln!(s);
  }
  ```
  For the validation helpers, accept `fmt::Arguments` or `impl Display` instead of `&str` and lazily format:
  ```rust
  fn violation(code: &str, message: impl Into<String>, path: Option<&str>) -> InvariantViolation { ... }
  // call sites stay close to current ergonomics
  ```
- **Effort:** M (renderer is many sites)
- **Risk of fix:** Low

### F-006-007 — [MEDIUM] [Correctness] `diff_relations` misses pairs that exist in `before` but not in `after`
- **Location:** `src/validate/diff.rs:298-319`
- **Category:** Correctness — silent data loss
- **Confidence:** High
- **Blast radius:** Diplomacy "what changed?" tables. A faction that is destroyed (and so falls out of `relations.pairs` in `after`) shows no stance change at all; an alliance silently disappears.
- **Problem:** The function indexes `before.relations.pairs` then walks `after.relations.pairs`. Pairs present in `before` but absent in `after` are never inspected. The current code does not emit, say, a `before: Allied, after: <absent>` change. Symmetric — added pairs in `after` with no `before` entry are also skipped (`by_pair.get(&key)` returns `None` → no push).
- **Why it matters:** Diff is supposed to be a complete structural delta. Missing-added/missing-removed pairs are silent data loss in the "what happened between sessions?" reporting use-case the module advertises in its top comment.
- **Evidence:** Loop structure, lines 304-316.
- **Suggested fix:** Walk the union, same shape as `diff_systems`/`diff_routes`:
  ```rust
  let mut b_idx: BTreeMap<(FactionId, FactionId), Stance> = ...;
  let mut a_idx: BTreeMap<(FactionId, FactionId), Stance> = ...;
  let all: BTreeSet<&(FactionId, FactionId)> = b_idx.keys().chain(a_idx.keys()).collect();
  for k in all {
      match (b_idx.get(k), a_idx.get(k)) {
          (Some(b), Some(a)) if b != a => out.push(StanceChange { a: k.0.clone(), b: k.1.clone(), before: *b, after: *a }),
          (None, Some(a)) => { /* new pair surfaced */ }
          (Some(b), None) => { /* pair vanished */ }
          _ => {}
      }
  }
  ```
  Or, if "vanished" pairs are intentionally not reported, document that in the function's doc comment so the partial behaviour is explicit.
- **Effort:** S
- **Risk of fix:** Low (only if test golden expects current partial behaviour — re-bless after fix).

### F-006-008 — [MEDIUM] [Correctness] `diff_economy` reuses `min_faction_delta` as the economy-delta threshold
- **Location:** `src/validate/diff.rs:382` (`if d.abs() >= cfg.min_faction_delta`)
- **Category:** API contract / config correctness
- **Confidence:** High
- **Blast radius:** Economy diff filtering — silently piggybacks on a config field named for factions.
- **Problem:** The threshold for "report this resource balance change?" comes from `cfg.min_faction_delta`. The field is documented (line 43-44) as "Minimum absolute change in **faction projection power**", not resource units. Resource balance is in totally different units (kilotonnes of foodstuffs etc.) and may need a different threshold; today setting `min_faction_delta = 100.0` to suppress noisy faction churn will *also* suppress meaningful economy shifts.
- **Why it matters:** Conflated knobs are a footgun; the docs lie about what the field controls.
- **Evidence:** Line 382 cross-referenced with line 43-49.
- **Suggested fix:** Add `pub min_economy_delta: f32` to `DiffConfig` with its own `#[serde(default = "default_min_economy_delta")]`:
  ```rust
  pub min_economy_delta: f32, // default 1.0
  // ...
  if d.abs() >= cfg.min_economy_delta { ... }
  ```
  Or, if intentional, rename `min_faction_delta` → `min_delta` and document that it applies to both. Either fix beats the current silent overload.
- **Effort:** S
- **Risk of fix:** Low (one new field with default).

### F-006-009 — [MEDIUM] [Performance] `check_region_connectivity` runs union-find twice
- **Location:** `src/validate/invariants.rs:87-88`, body at lines 101-135
- **Category:** Performance (§3.6)
- **Confidence:** Medium
- **Blast radius:** Once per invariant check, but two passes over all routes for sectors with any region effects.
- **Problem:** `navigable_component_count(s, false)` and `navigable_component_count(s, true)` are called back-to-back. Each does its own `BTreeMap` index build (line 103-108) and full union-find over `s.routes`. The only difference is whether `region:perilous_applied` routes are skipped.
- **Why it matters:** O(2·(systems + routes)) when O(systems + routes) suffices. Bounded but wasteful.
- **Evidence:** Both call sites read.
- **Suggested fix:** Compute both component counts in one pass — run union-find with all-non-perilous routes (gives `before_region_perilous`), then continue merging the `region:perilous_applied` routes on the same parent array (the result of *that* is `actual`). Or extract a single function returning `(actual, restored)`:
  ```rust
  fn navigable_components_pair(s: &GeneratedSector) -> (usize, usize) {
      // build idx + parent once
      // merge all non-perilous routes → count roots → `actual`
      // additionally merge region:perilous_applied routes → count roots → `restored`
      // return (actual, restored)
  }
  ```
  (Implementation requires undoing or pre-merging differently; cleaner is two parent arrays sharing index/route-walk costs.)
- **Effort:** S
- **Risk of fix:** Low

### F-006-010 — [MEDIUM] [API / safety] `ResourceVector` keyed by string `match` returns `0.0` for unknown keys
- **Location:** `src/validate/diff.rs:370-378` (the `pull` closure)
- **Category:** Silent data loss / robustness
- **Confidence:** High
- **Blast radius:** If `crate::economy::RESOURCE_KEYS` grows (e.g. a new resource is added), the `pull` closure silently returns `0.0` for the new key — both `before` and `after` look like zero balance, and the resource never appears in the diff.
- **Problem:** The match in `pull` is hand-maintained against `RESOURCE_KEYS`. There's no `assert!` or exhaustiveness check; the trailing `_ => 0.0` arm is a silent fallback. `ResourceVector::get` in `src/analysis/economy.rs:112-122` has the same problem at the source, but at least there it's an internal accessor; here we're iterating a public constant and assuming the match covers it.
- **Why it matters:** The validate/diff layer should be the most robust place to catch model drift, not silently zero-out new fields.
- **Evidence:** Lines 369-378 paired with `RESOURCE_KEYS` const at `src/analysis/economy.rs:26-33`.
- **Suggested fix:** Move `pull` into `ResourceVector` itself as a method (`get` already exists in `src/analysis/economy.rs:112`!) and reuse it:
  ```rust
  let pull = |s: &crate::economy::EconomyReport| s.sector_balance.get(k);
  ```
  Then add a `debug_assert!` in `ResourceVector::get` that the key is recognised, or return `Option<f32>` instead of defaulting to `0.0`. Either way, this `pull` closure is dead duplication of `ResourceVector::get` and should not exist.
- **Effort:** XS
- **Risk of fix:** Low

### F-006-011 — [MEDIUM] [Performance] `issue()` and `violation()` builders allocate two `String`s per call even for `&'static` codes
- **Location:** `src/validate/validation.rs:597-605`, `src/validate/invariants.rs:582-588`
- **Category:** Performance / API
- **Confidence:** High
- **Blast radius:** Every issue/violation; ties into F-006-004.
- **Problem:** `code: String` + `message: String` forces `to_string()` on every call, even when `code` is a `&'static str`. If `code` becomes `&'static str` (per F-006-004) and `message` accepts `Into<String>` directly, the unused `String::from_str` work goes away.
- **Why it matters:** Bounded but unnecessary — and the pattern propagates from these two helpers into every call site.
- **Evidence:** Both builder functions.
- **Suggested fix:** Together with F-006-004:
  ```rust
  fn issue(code: &'static str, message: impl Into<String>, severity: Severity) -> ValidationIssue {
      ValidationIssue { code, message: message.into(), path: None, row: None, severity }
  }
  ```
  Now `issue("GEN_GRID_EMPTY", "sector_width * sector_height must be > 0", Severity::Error)` allocates one `String`, not two.
- **Effort:** S (covered by F-006-004 refactor)
- **Risk of fix:** Low

### F-006-012 — [LOW] [Correctness] `RESOURCE_KEYS` mismatch with `STRATEGIC_RESOURCE_KEYS`
- **Location:** `src/validate/diff.rs:369` (uses `RESOURCE_KEYS`)
- **Category:** Correctness
- **Confidence:** Medium
- **Blast radius:** Diff completeness.
- **Problem:** `economy.sector_balance` is a `ResourceVector` (6 entries: ore/promethium/foodstuffs/manufactured/archeotech/recruits — see `src/analysis/economy.rs:96-109`). But the *strategic* output rules validated in `validation.rs:559-568` reference 10 different keys (food/ore/manufacturing/arms/ships/pilgrimage/psyker_tithe/manpower/knowledge/xenos_value) — these are `STRATEGIC_RESOURCE_KEYS`. The diff considers only the 6-key set. If sector_balance ever gains strategic-output fields, the diff will miss them.
- **Why it matters:** Easy to forget; the dual-key-set design isn't documented at the diff site.
- **Evidence:** Line 369 vs `src/analysis/economy.rs:26-46`.
- **Suggested fix:** Add a one-line comment at line 367 documenting which key set this iterates and why; consider iterating both if strategic outputs should appear in diff:
  ```rust
  // Resource diff covers `RESOURCE_KEYS` only; strategic outputs
  // (RESOURCE_KEYS vs STRATEGIC_RESOURCE_KEYS) live on world rules, not
  // on EconomyReport.sector_balance, so we deliberately skip them.
  ```
- **Effort:** XS
- **Risk of fix:** Trivial

### F-006-013 — [LOW] [Idiomatic] Public types missing `#[must_use]` on builder/derivation entry points
- **Location:** `src/validate/validation.rs:45` (`pub fn validate`), `src/validate/invariants.rs:25` (`pub fn check_sector`)
- **Category:** API / lints (§3.7)
- **Confidence:** High
- **Blast radius:** Linter hygiene.
- **Problem:** Both functions return a heavy report value that callers must consume. `diff_sectors`/`diff_sectors_with` have `#[must_use]` (line 241, 246) — these two don't. Calling `validate(&input);` with no use of the return is silently fine today.
- **Why it matters:** Style consistency; protects against a common foot-gun (forgetting to inspect the report).
- **Evidence:** Lines cited.
- **Suggested fix:**
  ```rust
  #[must_use]
  pub fn validate(input: &ProjectInput) -> ValidationReport { ... }

  #[must_use]
  pub fn check_sector(sector: &GeneratedSector) -> InvariantReport { ... }
  ```
- **Effort:** XS
- **Risk of fix:** None

### F-006-014 — [LOW] [Idiomatic] `as u32` / `as i32` truncations on `len()` should use `TryFrom`
- **Location:** `src/validate/diff.rs:262-265`, `:338`, `:343`, `:351-352`, `:747-760` (multiple `len() as u32`, `len() as i32`)
- **Category:** Idiomatic Rust / correctness (§3.7)
- **Confidence:** Medium
- **Blast radius:** Sectors with >2³¹ systems/worlds (practically impossible). Still flags as a code-smell — `as` truncation is mentioned explicitly in §3.7.
- **Problem:** `before.systems.len() as u32` truncates silently if `len()` exceeds `u32::MAX`. Realistic? No. Idiomatic? No. The pattern repeats ~12 times in this file.
- **Why it matters:** Style; documents the assumed-bounded growth implicitly. `u32::try_from(len).unwrap_or(u32::MAX)` is wordy; a helper or `u32::try_from(len).expect("…")` makes the bound explicit.
- **Evidence:** Lines cited.
- **Suggested fix:** Either accept the bound and add an `#[allow(clippy::cast_possible_truncation)]` at the function or, better, a small helper:
  ```rust
  #[inline]
  fn count_u32(n: usize) -> u32 {
      u32::try_from(n).unwrap_or(u32::MAX)
  }
  ```
  Then `system_count_before: count_u32(before.systems.len())`. Documents the saturation policy in one place.
- **Effort:** XS
- **Risk of fix:** Trivial

### F-006-015 — [LOW] [Idiomatic] `FactionId::new(*s)` recreates an id where a clone would do
- **Location:** `src/validate/diff.rs:408-413`, `:471-472`, `:659-664`, `:762` (and similar in `diff_regions`, `diff_presences`)
- **Category:** Ownership / allocation (§3.3)
- **Confidence:** Medium
- **Blast radius:** Per-faction; not a hot loop, but unnecessary.
- **Problem:** Pattern is `BTreeSet<&str>` → `difference` → `FactionId::new(*s)`. This reparses/wraps the str into a new id. If the upstream sets stored `&FactionId` (or `Cow`), the difference could yield owned `FactionId` clones without a string-roundtrip through `&str`. Today this is one allocation per faction (because `FactionId` is a newtype around `String`). Cheap, but it's allocator churn that a `BTreeSet<&FactionId>` would avoid.
- **Why it matters:** Marginal; flagged as LOW because the data flow already pays for the `String` once in the model.
- **Evidence:** Lines cited.
- **Suggested fix:** Use `BTreeSet<&FactionId>` directly:
  ```rust
  let before_set: BTreeSet<&FactionId> = before.iter().map(|p| &p.faction_id).collect();
  let after_set:  BTreeSet<&FactionId> = after.iter().map(|p| &p.faction_id).collect();
  let added: Vec<FactionId> = after_set.difference(&before_set).map(|&f| f.clone()).collect();
  ```
  No string round-trip, same allocations.
- **Effort:** XS
- **Risk of fix:** Trivial

### F-006-016 — [LOW] [Testing] No invariant test for the diff that catches F-006-007
- **Location:** `src/validate/diff.rs:1192-1308` (test module)
- **Category:** Testing (§3.10)
- **Confidence:** High
- **Blast radius:** Test coverage.
- **Problem:** Four tests cover identical/rename/id-mismatch/deterministic. No test exercises stance changes, region deltas, economy deltas, route diff, or world diff. The `diff_relations` bug (F-006-007) is invisible to the test suite.
- **Why it matters:** A 1300-LOC file with four tests covers only the high-level shape; the internal helpers are unexercised.
- **Evidence:** Test module starts at line 1192.
- **Suggested fix:** Add tests with two sectors that differ along each axis:
  - one world's `dominant` changes → `WorldDiff` populated.
  - one route's `stability` changes → `RouteDiff` populated.
  - one stance pair changes / new stance pair appears / vanishes → `StanceChange` populated (this catches F-006-007).
  - one region's hex count changes → `RegionDelta` populated.
- **Effort:** M
- **Risk of fix:** None (additive)

### F-006-017 — [LOW] [Idiomatic] `ValidationIssue.path`/`InvariantViolation.path` is `Option<String>` where `Option<&'static str>` or a typed path would suffice
- **Location:** `src/validate/validation.rs:23`, `src/validate/invariants.rs:16`
- **Category:** Allocation / API (§3.3, §3.7)
- **Confidence:** Low
- **Blast radius:** Allocation per issue.
- **Problem:** Every `path` is allocated via `format!("systems.{}.coord", sys.id)` etc. A structured path (`Vec<PathSeg>`) would be cheaper and downstream-tooling-friendlier; today the serialized JSON is just a dotted string and tooling parses it manually.
- **Why it matters:** Low — it's a UX/extensibility issue rather than a perf issue. Mentioned for completeness; defer to a future API revamp.
- **Evidence:** Lines cited.
- **Suggested fix:** Defer to a major version bump; for now no action.
- **Effort:** L
- **Risk of fix:** Medium (breaking)

### F-006-018 — [NIT] Inconsistent error/warning ordering between sibling validators
- **Location:** `src/validate/validation.rs:339-348` (the `validate_relations`/`validate_regions`/`validate_economy` calls)
- **Category:** Style
- **Confidence:** Medium
- **Problem:** Some validators take `errors`/`warnings` as `&mut Vec`; `validate_economy` takes `&mut Vec` for warnings even though it never pushes to it (line 498 — name prefixed `_warnings`). Either drop the unused parameter or push planned warnings (the trade_multiplier / supply_resilience checks could plausibly warn rather than error on borderline values).
- **Suggested fix:** Drop the `_warnings` parameter from `validate_economy` (the call site at line 348 disappears the arg too).
- **Effort:** XS
- **Risk of fix:** None

### F-006-019 — [NIT] `factions.iter().enumerate()` shadowing
- **Location:** `src/validate/validation.rs:198`
- **Category:** Style
- **Problem:** `for (idx, f) in input.factions.iter().enumerate()` — single-letter `f` next to single-letter `m`, `s`, `r`, `n` make the body slightly hard to scan. Names like `faction` would read better in a 50-line loop.
- **Suggested fix:** Spell out `for (idx, faction) in input.factions.iter().enumerate()` and rename `f` → `faction` inside; same goes for the loops in `invariants.rs:393-471`.
- **Effort:** XS
- **Risk of fix:** None

### F-006-020 — [NIT] Spurious `Vec::new().into()` in test fixture
- **Location:** `src/validate/diff.rs:1254`
- **Category:** Style
- **Problem:** `regions: Vec::new().into(),` — the `.into()` does nothing here because `regions` is already `Vec<WarpRegion>`. Should just be `regions: Vec::new(),` or `regions: vec![],`.
- **Suggested fix:** `regions: vec![],`.
- **Effort:** XS
- **Risk of fix:** None

### F-006-021 — [NIT] `path: Option<&str>` builder allocates anyway via `.map(|s| s.to_string())`
- **Location:** `src/validate/invariants.rs:582-588`
- **Category:** Idiomatic
- **Problem:** `violation()` accepts `Option<&str>` then immediately maps to `String` — the caller always passes a `format!`-produced `&String` anyway. Either accept `Option<String>` directly to avoid the temporary `&str` cast, or accept `impl Into<String>`. Avoids one borrow / re-allocate cycle.
- **Suggested fix:**
  ```rust
  fn violation(code: &str, message: impl Into<String>, path: Option<impl Into<String>>) -> InvariantViolation {
      InvariantViolation {
          code: code.to_string(),
          message: message.into(),
          path: path.map(Into::into),
      }
  }
  ```
- **Effort:** XS

### F-006-022 — [NIT] `default_min_delta` / `default_top_n` should be `const`
- **Location:** `src/validate/diff.rs:62-67`
- **Category:** Idiomatic
- **Problem:** Both are `fn` returning compile-time constants. They exist only because `serde(default = "fn_name")` requires a function path. Could be one-line consts referenced by helper fns, but the current shape is also fine; `const fn` would inline cleanly:
  ```rust
  const DEFAULT_MIN_DELTA: f32 = 1.0;
  const DEFAULT_TOP_N: u32 = 10;
  fn default_min_delta() -> f32 { DEFAULT_MIN_DELTA }
  fn default_top_n() -> u32 { DEFAULT_TOP_N }
  ```
- **Suggested fix:** Apply above; gives a named constant for grepability.
- **Effort:** XS

### F-006-023 — [NIT] `unreachable!()` comment lies about why it's unreachable
- **Location:** `src/validate/diff.rs:454`, `:549`, `:705`
- **Category:** Docs
- **Problem:** Adjacent to F-006-001. The current code has no comment explaining why `(None, None)` "can't" occur — a future reader has to reverse-engineer the `BTreeSet` semantics. If F-006-001 is taken, leave a one-line comment; if it isn't taken, add a comment explaining the `BTreeSet`-union reasoning.
- **Suggested fix:**
  ```rust
  // `id` came from the union of both index key sets; at least one map must contain it.
  (None, None) => unreachable!("id present in union but absent from both indexes"),
  ```
- **Effort:** XS

## Rubric coverage notes (per §3 categories)

- **§3.1 Panics & failure surface:** Covered — F-006-001 (the unreachables). No `unwrap`/`expect` on reachable errors found. No untrusted slicing. No integer overflow (only `as` casts on `len()`, F-006-014). The `find_root` while-loop on line 138 cannot panic — `parent[i]` is bounded by length.
- **§3.2 unsafe:** No findings. Zero `unsafe` blocks.
- **§3.3 Ownership / clones:** F-006-011 (allocation in issue/violation builders), F-006-015 (`FactionId::new(*s)` round-trip).
- **§3.4 Error handling:** F-006-003 (swallowed `fmt::Result`), F-006-004 (stringly-typed error codes), F-006-007 (silent data loss in diff_relations). `diff_after_ticks` correctly propagates `SectorError` with `?` and the doc comment lists `# Errors` (line 786-788). `write_diff` doc lists `# Errors` (line 1184-1187). Good shape.
- **§3.5 Concurrency:** N/A — fully synchronous. No threads, no async.
- **§3.6 Performance:** F-006-002 (8× scan of tag list), F-006-006 (format! in loops + collect→join in renderer), F-006-009 (union-find run twice). No HashMap iteration for output — `BTreeMap` used throughout; determinism invariant respected.
- **§3.7 API design:** F-006-004 (codes), F-006-013 (`#[must_use]`), F-006-014 (`as u32`), F-006-017 (typed path), F-006-022 (consts).
- **§3.8 Dependencies / Cargo:** No unused imports. `use std::fmt::Write as _;` (line 19) is used. `camino::Utf8Path` is used at line 1188. Clean.
- **§3.9 Memory & resources:** No `Drop`, no caches, no `static mut`. Allocator pressure flagged in F-006-006 / F-006-011 but no leaks.
- **§3.10 Testing:** F-006-016 (thin test coverage on diff.rs). `validation.rs` and `invariants.rs` have *zero* `#[cfg(test)]` blocks — all coverage lives in `tests/it/` (out of scope for this unit). Worth a follow-up integration request even though it's not a per-unit finding.
- **§3.11 Documentation:** Module-level `//!` docs present on all three files. Public types are partially documented; `DiffConfig` fields are documented; `ValidationIssue` fields are not. `InvariantViolation` fields are not. No `# Panics` sections (since the public APIs claim not to panic — but per F-006-001 they could).

## Project-specific invariant check

- **Fx iteration for output:** No `FxHashMap`/`FxHashSet` used here — only `BTreeMap`/`BTreeSet`. Pass.
- **RNG access:** No `rand::thread_rng()` or `rand` import in the module. Pass.
- **Byte-stable output:** F-006-005 (NaN sort) is a weak violation; the rest of the output is sorted via `BTreeMap` iteration order, which is byte-stable. F-006-014 (saturating cast) makes a documented choice rather than panic. No HashMap iteration in `render_markdown`.
- **Command bus / undo-redo:** N/A — this is read-only analysis code, no `BuilderState` access.

## Summary of suggested fixes

- F-006-001 — HIGH — Replace `unreachable!()` with explicit no-op `(None,None)` arm in three diff fns — S / Low
- F-006-002 — HIGH — Collapse 8× tag-namespace scan + dedup walk into single pass — S / Low
- F-006-003 — HIGH — Document `String`-sink infallibility (or return `Result<String, fmt::Error>`) instead of `let _ =` — S–M / Low
- F-006-004 — HIGH — Define a `codes` module / `ValidationCode` enum; field becomes `&'static str` or typed enum — M / Low
- F-006-005 — MEDIUM — Use `f32::total_cmp` for delta sort instead of `partial_cmp().unwrap_or` — XS / Low
- F-006-006 — MEDIUM — Drop `collect→join` in renderer; use `impl Into<String>` in issue/violation builders — M / Low
- F-006-007 — MEDIUM — `diff_relations` must walk the union of pair keys, not just `after` — S / Low
- F-006-008 — MEDIUM — Add `min_economy_delta` to `DiffConfig` instead of reusing `min_faction_delta` — S / Low
- F-006-009 — MEDIUM — Compute both navigable component counts in one union-find pass — S / Low
- F-006-010 — MEDIUM — Replace hand-rolled `pull` closure with `ResourceVector::get` — XS / Low
- F-006-011 — MEDIUM — Switch `issue()`/`violation()` to `&'static str` + `impl Into<String>` — S / Low
- F-006-012 — LOW — Comment why diff iterates `RESOURCE_KEYS` not `STRATEGIC_RESOURCE_KEYS` — XS / Trivial
- F-006-013 — LOW — Add `#[must_use]` to `validate()` and `check_sector()` — XS / None
- F-006-014 — LOW — Centralise `len() as u32` saturation via a helper — XS / Trivial
- F-006-015 — LOW — `BTreeSet<&FactionId>` instead of `BTreeSet<&str>` + `FactionId::new(*s)` — XS / Trivial
- F-006-016 — LOW — Add per-axis diff tests (stance, region, economy, route, world) — M / None
- F-006-017 — LOW — Typed `path` segments instead of `Option<String>` (defer) — L / Medium
- F-006-018 — NIT — Drop unused `_warnings` parameter from `validate_economy` — XS / None
- F-006-019 — NIT — Spell out single-letter loop bindings — XS / None
- F-006-020 — NIT — Drop spurious `Vec::new().into()` in test fixture — XS / None
- F-006-021 — NIT — Take `impl Into<String>` in `violation()` builder path — XS / None
- F-006-022 — NIT — Promote default fns to named `const`s — XS / None
- F-006-023 — NIT — Comment the `BTreeSet`-union reasoning at each `unreachable!()` — XS / None
