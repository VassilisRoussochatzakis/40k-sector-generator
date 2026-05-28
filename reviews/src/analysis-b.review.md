---
unit_id: U010
crate: sectorforge
paths:
  - src/analysis/personae.rs
  - src/analysis/analytics.rs
  - src/analysis/hooks.rs
  - src/analysis/missions.rs
  - src/analysis/briefing.rs
  - src/analysis/prose.rs
loc_reviewed: 4952
reviewed_by: agent
health_score: 4
finding_counts: { critical: 0, high: 3, medium: 9, low: 10, nit: 6 }
top_risks:
  - "briefing::apply clones entire GeneratedSector per profile, then Arc::make_mut clones RelationsMatrix again (F-010-001)"
  - "hooks::emit_economy_hooks has an O(R^2) supply-line scan with a cubic constant factor (F-010-002)"
  - "Persona/Hook/MissionSeed `id` are bare String — no newtype discipline matching SystemId/FactionId (F-010-003)"
---

# Review: src/analysis Part B — narrative & briefing analyses (U010)

## Summary

Part B is in good shape on determinism: every public output container is `BTreeMap`/`BTreeSet`/`Vec`, RNG draws all funnel through `crate::rng::stage_rng`, lookup hashmaps (`BTreeMap<&str, ...>`) are byte-stable. No `unsafe`, no `unwrap`s on user input, no `thread_rng()`. The biggest pain is performance + ownership: `briefing::apply` deep-clones `GeneratedSector` per profile and then additionally clones relations via `Arc::make_mut`; `hooks::emit_economy_hooks` does O(R²) work in a three-deep loop; analytics/prose/hooks/missions use `format!("{:?}", enum)` to allocate map keys despite an existing `as_slug() -> &'static str`. Secondary themes: newtype discipline missing for narrative IDs, inline `#[cfg(test)]` is shallow (mostly happy-path determinism), and several public `derive_with` entry points lack `# Panics` doc surfaces.

## Findings

### F-010-001 — [HIGH] [Performance / Ownership] briefing::apply deep-clones the whole sector per profile, then clones relations again via Arc::make_mut
- **Location:** `src/analysis/briefing.rs:217` (`let mut out = sector.clone();`) and `src/analysis/briefing.rs:266` (`Arc::make_mut(&mut out.relations)`)
- **Category:** Performance / Ownership
- **Confidence:** High
- **Blast radius:** Per-export, per-profile. `all_presets()` returns 6 — exporting all profiles clones the whole sector six times, plus relations matrix six times because the source `Arc` still holds a strong ref via `&sector` (so `make_mut` always clones).
- **Problem:** `out = sector.clone()` is a wholesale `GeneratedSector` clone (every system, world, faction, presence, claim, intel sub-record, surface regions, economy, chronicle, …). `Arc::make_mut(&mut out.relations)` then clones the matrix because `&sector` still holds a second strong reference to the Arc. For a 200-system sector the per-profile cost dominates briefing-export time and spikes memory.
- **Why it matters:** Briefing redaction is doc'd as a "pure transform" but the data model leaks: callers pay for a full clone even for `GmFullTruth` where no edits happen.
- **Suggested fix:**
  1. Make `BriefingPack::sector` a `Cow<'a, GeneratedSector>` so `GmFullTruth` borrows, or split into `apply_in_place(&mut GeneratedSector, &profile)`.
  2. Replace relations mutation with a separately-carried `Vec<FactionRelation>` projection used at serialisation time.
- **Effort:** M
- **Risk of fix:** Medium — public `BriefingPack` shape change, touches CLI runners.

### F-010-002 — [HIGH] [Performance] Cubic supply-line counter in hooks::emit_economy_hooks
- **Location:** `src/analysis/hooks.rs:255-326`
- **Category:** Performance
- **Confidence:** High
- **Blast radius:** ~6·R² comparisons per export. For R~500 routes ≈ 1.5M passes, each scanning the route array again.
- **Problem:** `count = sector.routes.iter().filter().filter().count()` (hooks.rs:278-296) is recomputed for every (route, crit, endpoint) triple inside the triple-nested loop, even when the same (endpoint, crit) was just counted on the previous route iteration.
- **Suggested fix:** Pre-build `BTreeMap<(&str, &'static str), u32>` of non-perilous routes importing each crit into each endpoint once. Drops to O(R) overall.
- **Effort:** S — **Risk of fix:** Low.

### F-010-003 — [HIGH] [Idiomatic / Type-driven] Persona, Hook, MissionSeed and BriefingProfile use bare String for `id`; cross-wires possible with FactionId/SystemId
- **Location:** `src/analysis/personae.rs:136`, `src/analysis/hooks.rs:77`, `src/analysis/missions.rs:80`, `src/analysis/briefing.rs:35`. Also `BriefingProfile::observer_faction: Option<String>` (briefing.rs:46) and `restrict_to_factions: Vec<String>` (briefing.rs:67).
- **Category:** §3.7 Idiomatic Rust / API design
- **Confidence:** High
- **Suggested fix:** Add `PersonaId`, `HookId`, `MissionId` via `define_id!`, re-export from `crate::ids`, switch the four DTOs. Change `BriefingProfile::observer_faction: Option<FactionId>` and `restrict_to_factions: Vec<FactionId>`. `#[serde(transparent)]` keeps disk format unchanged.
- **Effort:** M — **Risk of fix:** Medium (public API + golden tests).

### F-010-004 — [MEDIUM] [Performance] `format!("{:?}", enum)` allocates Arc<str> per route despite existing `as_slug() -> &'static str`
- **Location:** `src/analysis/analytics.rs:203, 211, 310, 314, 332`
- **Suggested fix:** Add `as_slug() -> &'static str` to `RouteStability`, `ClaimType`, `DominanceState`, `SystemState`; switch maps to `&'static str` keys.
- **Effort:** S — **Risk of fix:** Low.

### F-010-005 — [MEDIUM] [Ownership] `manual.extend(... .clone())` clones every entry even when none collide
- **Location:** `src/analysis/personae.rs:252`, `src/analysis/hooks.rs:157`, `src/analysis/missions.rs:156`
- **Suggested fix:** Add `derive_with_owned(cfg: PersonaeConfig)` variant that uses `std::mem::take(&mut cfg.manual)` and `out.append(&mut taken)`.
- **Effort:** S — **Risk of fix:** Low.

### F-010-006 — [MEDIUM] [Error model / Determinism] build_subsectors errors silently swallowed in compute_subsector_variety
- **Location:** `src/analysis/analytics.rs:517-520`
- **Suggested fix:** Push a `HealthFlag { code: "SUBSECTOR_DERIVE_FAILED", ... }` carrying `e.to_string()`. Change function signature to `(Vec<SubsectorVariety>, Option<HealthFlag>)`.
- **Effort:** S — **Risk of fix:** Low.

### F-010-007 — [MEDIUM] [Performance] graph_diameter / articulation_points allocate fresh `Vec<u32>` + `VecDeque` per BFS source
- **Location:** `src/analysis/analytics.rs:432-454`, `src/analysis/analytics.rs:457-509`
- **Suggested fix:** Hoist `dist`/`q` outside the `for start` loop, reset with `iter_mut`/`clear`.
- **Effort:** S — **Risk of fix:** Low.

### F-010-008 — [MEDIUM] [Idiomatic] prose::system_prose paragraph 3 builds Vec<String> just to join with a space
- **Location:** `src/analysis/prose.rs:316-344`
- **Suggested fix:** Single String buffer with `write!`, `trim_end()` at the end.
- **Effort:** XS — **Risk of fix:** Low.

### F-010-009 — [MEDIUM] [Performance] cap_per_anchor allocates String keys per hook
- **Location:** `src/analysis/hooks.rs:178-194`, `src/analysis/missions.rs:165-187`
- **Suggested fix:** Use `BTreeMap<HookAnchor, u32>` keyed by the actual enum.
- **Effort:** S — **Risk of fix:** Low.

### F-010-010 — [MEDIUM] [Correctness] hooks::derive_with sorts twice; cap_per_anchor's sort is overwritten
- **Location:** `src/analysis/hooks.rs:161-166`, `src/analysis/hooks.rs:177`
- **Suggested fix:** Inline cap into a single sorted-then-retain in `derive_with`, or document the sorted-precondition. Same applies to missions.rs:152-157.
- **Effort:** XS — **Risk of fix:** Low.

### F-010-011 — [MEDIUM] [Correctness] missions::derive_with: manual missions appended **after** cap, bypassing the cap — inconsistent with hooks
- **Location:** `src/analysis/missions.rs:152-157`
- **Suggested fix:** Move `out.extend(cfg.manual.iter().cloned())` BEFORE `cap_per_anchor` mirroring hooks; protect with `MissionSeed { manual: bool }` flag retained unconditionally during cap.
- **Effort:** S — **Risk of fix:** Low.

### F-010-012 — [MEDIUM] [Performance] briefing::render_markdown builds Vec<String> + join per world for claims
- **Location:** `src/analysis/briefing.rs:442-462`
- **Suggested fix:** Inline-write into `s` with leading-comma trick.
- **Effort:** XS — **Risk of fix:** Low.

### F-010-013 — [LOW] [Panic surface] generate_name's "Unnamed Persona {n}" loop has unchecked u32 overflow
- **Location:** `src/analysis/personae.rs:373-380`
- **Suggested fix:** Use `n.checked_add(1).expect(...)` or replace with `(1u32..).find_map(...)`.

### F-010-014 — [LOW] [Idiomatic] `let _ = writeln!` / `let _ = write!` pattern repeated >40 times
- **Location:** Throughout `analytics.rs`, `personae.rs`, `hooks.rs`, `missions.rs`, `briefing.rs`, `prose.rs`.
- **Suggested fix:** Local `wln!` macro per module, or implement `Display` on the report types.

### F-010-015 — [LOW] [Error model] briefing::redact_world_presences silently floors observer visibility to 20.0
- **Location:** `src/analysis/briefing.rs:308-316`
- **Suggested fix:** Named const `DEFAULT_ABSENT_OBSERVER_VISIBILITY: f32 = 20.0;` with doc.

### F-010-016 — [LOW] [Type safety] `(raw as u8)` cast in redact_world_presences silently truncates if raw overflows
- **Location:** `src/analysis/briefing.rs:325-327`
- **Suggested fix:** `raw.clamp(0.0, 100.0).round() as u8`.

### F-010-017 — [LOW] [Idiomatic] `format!("{:?}", state)` couples map keys to rustc identifier
- **Location:** `src/analysis/analytics.rs:310, 314, 332`
- **Suggested fix:** Hand-written `as_slug() -> &'static str` matching the serde name.

### F-010-018 — [LOW] [Idiomatic] merge_with_defaults verbose per-field cascade
- **Location:** `src/analysis/personae.rs:524-558`
- **Suggested fix:** Extract `fn merge_field(user: &[String], base: Vec<String>) -> Vec<String>`.

### F-010-019 — [LOW] [Documentation] Public `derive_with` family missing `# Panics` notes
- **Location:** `analytics.rs:158`, `personae.rs:176`, `hooks.rs:137`, `missions.rs:137`, `briefing.rs:209`, `prose.rs:125`.
- **Suggested fix:** Standardise `/// # Panics\n///\n/// Never. (No unwraps on user input.)` block.

### F-010-020 — [LOW] [Testing] Inline tests cover only happy-path determinism; no coverage of capping, dedupe, override interaction, or redaction edges
- **Location:** All six `#[cfg(test)]` mods.
- **Suggested fix:** Add focused tests + a proptest on `cap_per_anchor` invariant.

### F-010-021 — [NIT] [Idiomatic] `let _ = sector;` deliberate no-op in missions::emit_world_missions
- **Location:** `src/analysis/missions.rs:318`

### F-010-022 — [NIT] [Documentation] BriefingProfile::observer_faction doc skips the absent-observer fallback
- **Location:** `src/analysis/briefing.rs:42-48`

### F-010-023 — [NIT] [Idiomatic] `out.push(...) ; out.push(...)` pairs could be `out.extend([..., ...])`

### F-010-024 — [NIT] [Idiomatic] `format!("{id:?}").to_lowercase()` for slug in briefing.rs:336

### F-010-025 — [NIT] [Encapsulation] briefing relations-redaction touches `FactionRelation` private structure inline
- **Location:** `src/analysis/briefing.rs:266-275`
- **Suggested fix:** `impl RelationsMatrix { pub fn redact_secrets(&mut self) }`.

### F-010-026 — [NIT] [Documentation] personae.rs::pick_traits doc claims "1-3 traits"
- **Location:** `src/analysis/personae.rs:398-410`

## Per-rubric coverage

- **3.1 Panics:** F-010-013, F-010-016, F-010-026. Otherwise panic-free on realistic input.
- **3.2 unsafe:** No findings. Zero `unsafe` blocks.
- **3.3 Ownership/clone:** F-010-001, F-010-005, F-010-008.
- **3.4 Error handling:** F-010-006.
- **3.5 Concurrency / async:** No findings — all single-threaded.
- **3.6 Performance:** F-010-001/002/004/005/007/008/009/012.
- **3.7 Idiomatic / API:** F-010-003/014/017/018/021/024.
- **3.8 Deps / Cargo:** No findings.
- **3.9 Memory & resource:** F-010-001 dominates.
- **3.10 Testing:** F-010-020.
- **3.11 Documentation:** F-010-019/022/026.
- **Project invariants (CLAUDE.md):** Determinism + RNG centralization + builder bus = all PASS for this unit.

## Summary of suggested fixes

- F-010-001 — HIGH — briefing::apply deep-clones sector + relations per profile — M / Medium
- F-010-002 — HIGH — O(R²) supply-line counter in emit_economy_hooks — S / Low
- F-010-003 — HIGH — Persona/Hook/MissionSeed bare String id; add PersonaId/HookId/MissionId newtypes — M / Medium
- F-010-004 — MEDIUM — `format!("{:?}", enum)` allocates Arc<str> keys; use as_slug — S / Low
- F-010-005 — MEDIUM — manual.extend clones every entry; add owned-cfg variant — S / Low
- F-010-006 — MEDIUM — compute_subsector_variety swallows build_subsectors errors — S / Low
- F-010-007 — MEDIUM — graph_diameter / articulation_points re-allocate per-BFS — S / Low
- F-010-008 — MEDIUM — prose colour paragraph builds Vec<String> then joins — XS / Low
- F-010-009 — MEDIUM — cap_per_anchor allocates String keys; use enum-keyed BTreeMap — S / Low
- F-010-010 — MEDIUM — hooks::derive_with double-sort makes cap brittle — XS / Low
- F-010-011 — MEDIUM — missions manual entries bypass cap; inconsistent with hooks — S / Low
- F-010-012 — MEDIUM — briefing::render_markdown rebuilds claim Vec per world — XS / Low
- F-010-013 — LOW — generate_name u32 overflow at 4B — XS / Low
- F-010-014 — LOW — `let _ = writeln!` pattern repeated >40 times — S / Low
- F-010-015 — LOW — magic `20.0` fallback in redact_world_presences — XS / Low
- F-010-016 — LOW — silent `as u8` cast on out-of-range f32 — XS / Low
- F-010-017 — LOW — Debug-derived map keys couple output to rustc identifier — S / Low
- F-010-018 — LOW — merge_with_defaults verbose per-field cascade — XS / Low
- F-010-019 — LOW — derive_with family missing `# Panics` doc — XS / Zero
- F-010-020 — LOW — Inline tests cover only happy-path determinism — M / Zero
- F-010-021 — NIT — `let _ = sector;` deliberate no-op — XS / Zero
- F-010-022 — NIT — observer_faction doc skips absent-observer fallback — XS / Zero
- F-010-023 — NIT — `out.push` pairs could be `out.extend([])` — XS / Zero
- F-010-024 — NIT — `format!("{id:?}").to_lowercase()` for slug — XS / Low
- F-010-025 — NIT — relations redaction should be `RelationsMatrix::redact_secrets()` — XS / Low
- F-010-026 — NIT — pick_traits doc note for guarded panic — XS / Zero
