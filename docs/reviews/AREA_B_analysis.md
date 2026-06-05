# AREA B — src/analysis — verification

Verified 2026-06-05 against live `main` branch. Scope: `src/analysis/` — primarily `economy.rs` (1785 LOC), `relations.rs` (1689 LOC), `hooks.rs` (904 LOC), `missions.rs` (826 LOC), `search.rs` (1466 LOC), plus `analytics.rs`, `control.rs`, `personae.rs`, `interestingness.rs`, `prose.rs`.

| ID   | Sev     | Status          | Effort | One-line                                                       |
|------|---------|-----------------|--------|----------------------------------------------------------------|
| B-S1 | systemic | ✅ Confirmed    | L      | 7 modules with full derive/render_markdown/write_report triple |
| B-S2 | systemic | ⚠️ Partial      | M      | cap_per_anchor verbatim dup; merge_manual logic diverges       |
| B-S3 | systemic | ✅ Confirmed    | M      | 28 as_slug fns in analysis alone (62 across src/)             |
| B1   | HIGH    | 🔄 Moved        | M      | system_supply_risk O(systems·worlds·resources·edges) at line 1253 |
| B3   | MED     | ✅ Confirmed    | M      | O(F²·rules) linear scans per pair in compute_pair at line 733  |
| B4   | MED     | ✅ Confirmed    | S      | cap_per_anchor body copy-pasted verbatim at hooks:204 missions:232 |
| B5   | MED     | ✅ Confirmed    | M      | field-wise add/scale/clamp hand-rolled ×3 structs              |
| B6   | MED     | 🔄 Moved        | M      | canonical_pair String allocs at line 1115; hot cooccurrence loop confirmed |
| B7   | LOW     | ✅ Confirmed    | S      | format!("{}",x).into() enum map keys ×5 in analytics.rs:220–357 |
| B8   | LOW     | ⚠️ Partial      | S      | insert_top_n O(top) scan confirmed; top is 5 by default        |
| B9   | LOW     | ✅ Confirmed    | S      | 15 partial_cmp sites in analysis; 22 across src/               |
| B10  | LOW     | ✅ Confirmed    | S      | 9-deep mul_add at economy.rs:262 (golden risk: must stay bit-identical) |
| B11  | LOW     | ✅ Confirmed    | L      | economy.rs 1785 LOC / relations.rs 1689 LOC confirmed          |
| B12  | LOW     | ✅ Confirmed    | S      | unlabeled 30.0/200.0/400.0 thresholds at multiple sites        |

---

### B-S1 — 7 report modules duplicate derive→render_markdown→write_report

- **Review sev / bucket:** systemic / P1 #4
- **Status:** ✅ Confirmed
- **Location:** `src/analysis/economy.rs`, `hooks.rs`, `interestingness.rs`, `missions.rs`, `personae.rs`, `prose.rs`, `relations.rs` (verified by pattern scan)
- **Evidence:**
  ```rust
  // All 7 share this surface:
  pub fn derive(sector: &GeneratedSector) -> <X>Report
  pub fn derive_with(sector: &GeneratedSector, cfg: &<X>Config) -> <X>Report
  pub fn render_markdown(report: &<X>Report, ...) -> String
  pub fn write_report(output_dir: &Utf8Path, report: &<X>Report, ...) -> Result<(), SectorError>
  ```
- **Why it matters:** Any structural change (new field, output-format switch, error type) must be applied in 7 places. Config-loading is further asymmetric: only `economy` and `relations` expose a `load_*_file` fn; the other 5 modules load config upstream in CLI runners, making a generic `SectorReport` trait harder but still feasible.
- **Fix:** Define `trait SectorReport { type Config: Default + DeserializeOwned; const BASE_NAME: &'static str; fn derive_with(sector, cfg) -> Self; fn render_markdown(&self, cfg) -> String; }`. Add a `load_config_file<C: DeserializeOwned>(path) -> Result<C>` free function in `analysis/mod.rs`. Provide blanket `derive` / `write_report` defaults. Note: `briefing` has `render_markdown` but no `write_report` — exclude it, leaving the 7-module core.
- **Effort:** L
- **Risk / deps:** No output ordering change. `render_markdown` signatures vary slightly (some take `cfg` as a second arg, some don't) — needs a `Render` associated type or a separate trait method. Not a golden risk if the render body is unchanged.

---

### B-S2 — hooks.rs ≈ missions.rs: cap_per_anchor verbatim; merge_manual diverges

- **Review sev / bucket:** systemic / P1 #4
- **Status:** ⚠️ Partial
- **Location:** `src/analysis/hooks.rs:204`, `src/analysis/missions.rs:232` (verified)
- **Evidence:**
  ```rust
  // hooks.rs:204
  fn cap_per_anchor(hooks: &mut Vec<Hook>, cap: u32) {
      let mut counts: BTreeMap<String, u32> = BTreeMap::new();
      hooks.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.id.cmp(&b.id)));
      hooks.retain(|h| { let entry = counts.entry(key).or_insert(0); ... });
  }
  // missions.rs:232 — byte-identical logic, different Vec type
  fn cap_per_anchor(missions: &mut Vec<MissionSeed>, cap: u32) {
  ```
- **Why it matters:** `cap_per_anchor` body is verbatim except for the element type. The merge_manual step is **different**: hooks dedupes by id before appending manual entries (`manual_ids` retain), while missions appends manual items unconditionally after capping. This divergence is a latent bug risk if one module evolves the merge logic and the other doesn't follow.
- **Fix:** A `WeightedAnchored` trait with `fn weight(&self) -> u32`, `fn id(&self) -> &str`, `fn anchor_key(&self) -> String` enables one generic `cap_per_anchor<T: WeightedAnchored>`. The `merge_manual` steps should be aligned (pick hooks' id-dedup strategy as the correct one) and extracted as `fn merge_manual<T: WeightedAnchored>(out, manual)`.
- **Effort:** M
- **Risk / deps:** Merge-logic alignment is a **behavioral change** in `missions` — the current missions code doesn't dedup by id before appending manual items. Confirm intended behavior before standardizing. No output ordering change if sort key is unchanged.

---

### B-S3 — ~28 hand-written as_slug + Display enums in analysis (62 across src/)

> ✅ **RESOLVED 2026-06-05** — `macro_rules! enum_slug!` (`src/macros.rs`,
> `#[macro_use]`) with a normal + `const` arm; verbatim slugs → byte-identical
> output. 61/62 enums converted; `SectorSize` (struct variant `Custom { dim }`)
> left hand-written as it is not a fieldless enum. Golden 15/15, lib 191/191,
> `it` 93/93, clippy clean. See [PROGRESS.md](PROGRESS.md).

- **Review sev / bucket:** systemic / P1 #2
- **Status:** ✅ Confirmed
- **Location:** 28 `as_slug` definitions in `src/analysis/` (e.g. `hooks.rs:132`, `missions.rs:114`, `missions.rs:144`, `missions.rs:170`, `economy.rs:739`, `economy.rs:769`, `economy.rs:798`, `relations.rs:49`, `relations.rs:283`, `relations.rs:367`, plus 18 more across `personae`, `search`, `intel`, `importance`, `analytics`, `history/model`)
- **Evidence:**
  ```rust
  // hooks.rs:132 — pattern repeated 62 times across src/
  pub fn as_slug(&self) -> &'static str {
      match self { Self::ConvoyEscort => "convoy_escort", ... }
  }
  impl core::fmt::Display for HookKind {
      fn fmt(&self, f: &mut ...) -> ... { f.write_str(self.as_slug()) }
  }
  ```
- **Why it matters:** A new variant that omits its `as_slug` arm silently falls through to no match (panics at runtime on exhaustive match) or returns a wrong slug on `_` arms. The codebase already has the `score_newtype!` macro precedent. 62 sites is the deletion opportunity.
- **Fix:** Add `macro_rules! enum_slug! { ... }` in `src/macros.rs` (or use `strum` with `EnumString` + `AsRefStr` features). The macro emits `as_slug`, `Display`, and optionally `FromStr`. Priority: analysis enums with `serde(rename_all = "snake_case")` already have the mapping implicit — `strum::AsRefStr` with `#[strum(serialize_all = "snake_case")]` is the zero-boilerplate path.
- **Effort:** M
- **Risk / deps:** `#[non_exhaustive]` enums in the public API (`HookKind`, `MissionKind`, etc.) prevent exhaustive match in external crates; the macro must not add one. No ordering or golden risk.

---

### B1 — system_supply_risk O(systems·worlds·resources·edges)

- **Review sev / bucket:** HIGH / P2
- **Status:** 🔄 Moved (line drift: review cited :1254, actual loop starts at :1253)
- **Location:** `src/analysis/economy.rs:1253` (verified — review cited 1254, off by one)
- **Evidence:**
  ```rust
  if let Some(sys) = sys_ref {
      for world in &sys.worlds {                               // O(worlds)
          for resource in strategic_needs_for_world(...) {    // O(resources)
              if sy.strategic_output.get(resource) >= 30.0 { continue; }
              let incoming: Vec<&DependencyEdge> = deps
                  .iter()                                      // O(edges)
                  .filter(|e| e.to_system_id == sy.system_id && e.resource == *resource)
                  .collect();
  ```
- **Why it matters:** Called once per system in the sector; each call iterates the full `deps` slice (all edges in the sector) for each (world, resource) pair. For a large sector with many trade edges this is the dominant O(S·W·R·E) hotspot in the economy derivation path.
- **Fix:** Before calling `system_supply_risk`, build a `BTreeMap<(&str, &str), Vec<&DependencyEdge>>` keyed on `(to_system_id, resource)` from the full `deps` slice — one O(E log E) pass. Each call to `system_supply_risk` then does a O(W·R·log E) lookup, eliminating the inner full-scan.
- **Effort:** M
- **Risk / deps:** The `deps` slice comes from `derive_dependency_edges` at economy.rs:1146, which already returns a `Vec<DependencyEdge>`. Pre-bucketing must happen at the call site in `derive_with`. No output ordering change (risk bucket comparisons are commutative). Not a golden risk unless the `SupplyRisk` values change — they won't if the filter logic is equivalent.

---

### B3 — O(F²·rules) pair scans: kind_rules + disposition_rules re-scanned per pair

- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed
- **Location:** `src/analysis/relations.rs:733` (kind_rules loop), `src/analysis/relations.rs:750` (disposition_rules loop) — called inside the O(F²) pair loop at line 682
- **Evidence:**
  ```rust
  // Inside the O(F²) pair loop:
  for r in &cfg.kind_rules {               // O(rules) per pair
      if (r.a == *a.kind && r.b == *b.kind) || ... { base = Some(...); break; }
  }
  for r in &cfg.disposition_rules {        // O(rules) per pair
      if (r.a == *a.disposition && ...) { delta += r.delta; ... }
  }
  ```
- **Why it matters:** With F factions and R rules, compute_pair is O(R) per pair → O(F²·R) total. For typical sectors F≈20–60, R≈10–30 (config-driven) so the constant is small, but with the documented 1000-faction catalog (C(1000,2)≈500k pairs × R rules) this becomes material.
- **Fix:** Pre-index `cfg.kind_rules` into a `BTreeMap<(FactionKind, FactionKind), KindRule>` (symmetric: insert both orderings). Do the same for `disposition_rules` as a `BTreeMap<(Disposition, Disposition), Vec<DispositionRule>>`. Build the indexes once before the pair loop in `derive_with_threshold`. No output change — lookups replace the same linear search.
- **Effort:** M
- **Risk / deps:** `FactionKind` and `Disposition` are `Arc<str>` fields — keys would be `(Arc<str>, Arc<str>)`. The canonical ordering must still be applied (use `canonical_pair` on the key before insert). No golden risk.

---

### B4 — cap_per_anchor duplicated verbatim: hooks.rs:204 & missions.rs:232

- **Review sev / bucket:** MED / P1 #4
- **Status:** ✅ Confirmed
- **Location:** `src/analysis/hooks.rs:204`, `src/analysis/missions.rs:232` (verified)
- **Evidence:**
  ```rust
  // hooks.rs:204 and missions.rs:232 are byte-identical except Vec element type:
  fn cap_per_anchor(hooks: &mut Vec<Hook>, cap: u32) {
      let mut counts: BTreeMap<String, u32> = BTreeMap::new();
      hooks.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.id.cmp(&b.id)));
      hooks.retain(|h| { let entry = counts.entry(key).or_insert(0); if *entry < cap { *entry += 1; true } else { false } });
  }
  ```
- **Why it matters:** Identical logic in two private functions. Any bug fix or behavior change (e.g. secondary sort key) must be applied twice; the merge_manual divergence (B-S2) shows this pattern is already drifting.
- **Fix:** Part of B-S2's `WeightedAnchored` trait extraction. Can be done independently with a free generic function `fn cap_per_anchor<T, F>(items: &mut Vec<T>, cap: u32, key: F) where F: Fn(&T) -> String`.
- **Effort:** S
- **Risk / deps:** No output ordering change if sort comparator is identical. Depends on B-S2 trait design if going the full route.

---

### B5 — field-wise add/scale/clamp hand-rolled: StrategicOutput, ResourceVector, PresenceDimensions

- **Review sev / bucket:** MED / P1 #7
- **Status:** ✅ Confirmed
- **Location:** `src/analysis/economy.rs:219` (`StrategicOutput::add_assign`, 10 fields), `src/analysis/economy.rs:232` (`StrategicOutput::scale`, 10 fields), `src/analysis/economy.rs:246` (`StrategicOutput::clamp_scores`, 10 fields), `src/analysis/economy.rs:1399` (`ResourceVector add`, 6 fields), `src/analysis/control.rs:110` (`scale_dimensions`, 10 fields), `src/analysis/control.rs:123` (`clamp_dimensions`, 10 fields), `src/analysis/control.rs:137` (`add_dimensions`, 10 fields)
- **Evidence:**
  ```rust
  // economy.rs:246 — one of three identical patterns on StrategicOutput:
  fn clamp_scores(mut self) -> Self {
      self.food = self.food.clamp(0.0, 100.0);
      self.ore  = self.ore.clamp(0.0, 100.0);
      // ... ×10 fields
      self
  }
  ```
- **Why it matters:** Adding an 11th field to `StrategicOutput` or `PresenceDimensions` requires updating 3–4 separate functions; omitting one silently produces wrong totals. The `.clamp(0.0, 100.0)` repetition is a typo attractor (different upper bound per field, e.g. `visibility` uses `k.max(0.3)` not a simple clamp).
- **Fix:** Add `fn fields_mut(&mut self) -> [(&mut f32, f32); N]` returning `(field_ref, scale_cap)` tuples, or use a derive macro for the arithmetic operations. For `PresenceDimensions`, note `visibility` has a special scale floor (`k.max(0.3)`) — any generic implementation must accommodate per-field metadata.
- **Effort:** M
- **Risk / deps:** `PresenceDimensions` lives in `src/model/sector_model/mod.rs:1024` — its `scale`/`clamp`/`add` helpers live in `src/analysis/control.rs`. A macro solution must span two modules or move the helpers. No golden risk if arithmetic is identical; a `fields_mut` refactor is safe to verify with `cargo test --test it -- golden`.

---

### B6 — canonical_pair allocates two Strings per call in the cooccurrence hot loop

- **Review sev / bucket:** MED / P2
- **Status:** 🔄 Moved (review cited :1228; the allocation is in the `canonical_pair` fn at :1115, called from the loop at :1235–1338)
- **Location:** `src/analysis/relations.rs:1115` (`canonical_pair` fn), `src/analysis/relations.rs:1228` (hot cooccurrence build loop) (verified)
- **Evidence:**
  ```rust
  fn canonical_pair(a: &str, b: &str) -> (String, String) {
      if a <= b { (a.to_string(), b.to_string()) }
      else      { (b.to_string(), a.to_string()) }
  }
  // Called in the O(F²·worlds) loop at :1235 and O(F²) pair loop at :686
  ```
- **Why it matters:** Two heap allocations per pair-event in the co-occurrence build loop. For 60 factions × 50 worlds × 10 world-pair combos this is ~30k allocations per derivation call. The BTreeMap itself also stores `(String, String)` keys permanently, so even a scratch-key optimization needs a per-lookup borrowed key.
- **Fix:** Key on `(u32, u32)` faction indices stored in a lookup built from `sector.factions`. `canonical_pair_idx(a_idx: u32, b_idx: u32) -> (u32, u32)` is a single compare + swap, zero allocations. The `BTreeMap<(String, String), CooccurStats>` becomes `BTreeMap<(u32, u32), CooccurStats>`. **Important:** output ordering is deterministic because `(u32, u32)` BTreeMap order is by index, not by id string. The final `pairs` Vec is sorted by tension (not by map key), so output ordering is unaffected. No golden risk.
- **Effort:** M
- **Risk / deps:** The index lookup must be consistent: build once, use everywhere in `derive_with_threshold`. The `stance_between` public method (line 228) takes `&str` id, not index — keep an id→index map alongside or convert only the internal cooccurrence path.

---

### B7 — format!("{}",x).into() map keys for closed enums in analytics.rs

- **Review sev / bucket:** LOW / P1.5
- **Status:** ✅ Confirmed
- **Location:** `src/analysis/analytics.rs:220`, `:228`, `:335`, `:339`, `:357` (verified)
- **Evidence:**
  ```rust
  // analytics.rs:335 — ClaimType has as_slug() yet format!() is used:
  let key: Arc<str> = format!("{}", c.claim_type).into();
  // Also: route_type (:220), stability (:228), dominance (:339), system state (:357)
  ```
- **Why it matters:** `RouteType`, `RouteStability`, `ClaimType`, `DominanceState`, `SystemState` all have `as_slug()` methods (verified — Display delegates to `as_slug`). Using `format!("{}", x)` allocates a heap `String` then converts it to `Arc<str>`, doubling allocations. More critically it bypasses the `as_slug` method's compile-time exhaustiveness.
- **Fix:** Replace `format!("{}", x).into()` with `Arc::from(x.as_slug())` at all 5 sites in `analytics.rs`. Zero behavioral change; the slug values are identical because `Display` delegates to `as_slug`.
- **Effort:** S
- **Risk / deps:** The BTreeMap keys are `Arc<str>` and appear in `SectorAnalysis` output fields (`route_type_distribution`, etc.) — their string values are serialized to JSON/markdown, so the slug values must stay stable. No ordering risk (BTreeMap on `Arc<str>` has identical order to BTreeMap on `String` for identical strings).

---

### B8 — insert_top_n O(top) scan in search.rs

- **Review sev / bucket:** LOW / P3
- **Status:** ⚠️ Partial
- **Location:** `src/analysis/search.rs:1305` (verified)
- **Evidence:**
  ```rust
  fn insert_top_n(buf: &mut Vec<CandidateReport>, cand: CandidateReport, top: usize) {
      if buf.len() < top { buf.push(cand); return; }
      let mut worst_idx = 0usize;
      for (i, r) in buf.iter().enumerate() {  // O(top) scan
          if r.total_miss > buf[worst_idx].total_miss { worst_idx = i; }
      }
      if cand.total_miss < buf[worst_idx].total_miss { buf[worst_idx] = cand; }
  }
  ```
- **Why it matters:** `top` defaults to 5 (via `default_report_top()`), so the current O(top) scan costs ~5 comparisons and is negligible. The review's `BinaryHeap` recommendation is correct in principle but is only a real win if `top` is configurable upward (it is — it's a `u32` config field). At `top=5`, the complexity improvement is unmeasurable.
- **Fix:** Low priority given the default. If `top` is expected to grow (e.g. user sets `report_top: 50`), replace with a `BinaryHeap<Reverse<OrderedFloat<f32>>>` keyed on `total_miss`. Otherwise defer — this is the right fix for the wrong size of problem.
- **Effort:** S
- **Risk / deps:** `CandidateReport` would need `Ord` or a wrapper. No output ordering change (output is sorted by `total_miss` after collection at line 1288). No golden risk.

---

### B9 — partial_cmp().unwrap_or(Equal) on f32 scattered at ~15 sites

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** 15 sites in `src/analysis/` (confirmed), 22 total across `src/` including `src/export/` and `src/gen/`
- **Evidence:**
  ```rust
  // economy.rs:1511–1514:
  top.sort_by(|a, b| {
      b.volume.partial_cmp(&a.volume).unwrap_or(std::cmp::Ordering::Equal)
  });
  // relations.rs:693–698, search.rs:1289–1292, importance.rs:153, analytics.rs:294 — same pattern
  ```
- **Why it matters:** NaN in a score field silently produces inconsistent sort order (NaN comparisons always yield `Less` after the unwrap). A centralized `fn cmp_f32_desc(a: f32, b: f32) -> Ordering` (or `total_cmp().reverse()`) eliminates the divergence risk and makes the NaN policy explicit.
- **Fix:** Add `pub(crate) fn cmp_f32_desc(a: f32, b: f32) -> std::cmp::Ordering { b.total_cmp(&a) }` in `src/analysis/mod.rs`. Replace all 15 `partial_cmp(...).unwrap_or(Equal)` call sites. Note: `f32::total_cmp` (stable since Rust 1.62) produces a different ordering for NaN than `unwrap_or(Equal)` — this is an intentional improvement, not a golden risk for normal (non-NaN) outputs.
- **Effort:** S
- **Risk / deps:** 22 sites across `src/` — the 7 outside `src/analysis/` (in `src/export/` and `src/gen/`) also benefit. `total_cmp` imposes a total order on NaN; if any sort is used in output-visible order (e.g. route rankings in `economy.rs:1511`), confirm no NaN scores exist. Run `cargo test --test it -- golden` after.

---

### B10 — 9-deep mul_add chain in StrategicOutput::weighted_priority_score

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `src/analysis/economy.rs:261` (verified)
- **Evidence:**
  ```rust
  pub fn weighted_priority_score(&self) -> f32 {
      self.xenos_value.mul_add(0.90,
          self.knowledge.mul_add(1.00,
              self.manpower.mul_add(0.80,
                  self.psyker_tithe.mul_add(1.10,
                      self.pilgrimage.mul_add(0.55,
                          self.ships.mul_add(1.20,
                              self.arms.mul_add(1.00,
                                  self.manufacturing.mul_add(0.85,
                                      self.food.mul_add(0.70, self.ore * 0.70)))))))))
  }
  ```
- **Why it matters:** Unreadable — adding a field or changing a weight requires tracing 9 levels of nesting. A `WEIGHTS` array + `zip` would be cleaner. **Golden risk: `mul_add` chains and a WEIGHTS zip produce bit-identical results only if the `mul_add` FMA order is replicated exactly**. A simple dot-product with `iter().zip().map(|(f,w)| f*w).sum()` uses a different associativity and will produce different low-order float bits. This breaks the byte-stable golden tests.
- **Fix:** Either: (a) keep `mul_add` but extract `const WEIGHTS: [f32; 10] = [...]` and a parallel `fields_arr(&self) -> [f32; 10]`, then reconstruct the same `mul_add` chain via a const-indexed loop; or (b) annotate the current form with a comment explaining the FMA chain and defer. Option (a) is cleaner but is non-trivial to verify as bit-identical without running goldens.
- **Effort:** S
- **Risk / deps:** **Golden risk — run `cargo test --test it -- golden` after any change.** Do not replace with a naive dot-product sum.

---

### B11 — economy.rs / relations.rs god-modules

- **Review sev / bucket:** LOW / P2
- **Status:** ✅ Confirmed
- **Location:** `src/analysis/economy.rs` (1785 LOC), `src/analysis/relations.rs` (1689 LOC) (verified by `wc -l`)
- **Evidence:** economy.rs line counts by section: config/structs 1–845, derivation 846–1408, markdown render 1410–1524, tests 1525–1785. relations.rs: config/structs 1–631, derivation 632–1113, internal helpers 1114–1397, markdown render 1398–1510, tests 1511–1689.
- **Why it matters:** At 1785 / 1689 LOC each, both files mix config types, derivation logic, risk/scoring helpers, and markdown rendering. A targeted change (e.g. adding a new resource) requires navigating all sections. The split proposed by the review (config / tables / derive / risk / render) maps cleanly onto the existing sectioned structure.
- **Fix:** Split economy.rs into `economy/config.rs`, `economy/tables.rs` (built-in world-type vectors), `economy/derive.rs`, `economy/risk.rs`, `economy/render.rs`. Similarly for relations.rs. Use `pub(super)` for internal helpers. This is purely mechanical — no logic change.
- **Effort:** L
- **Risk / deps:** Must happen after `G2` content golden is pinned (per review's suggested sequence) — the split creates churn that could mask render regressions without a golden. No ordering or golden risk from the split itself.

---

### B12 — scattered magic thresholds without named consts

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** Multiple sites in `src/analysis/economy.rs` (verified)
- **Evidence:**
  ```rust
  // economy.rs:1256 — appears at 3 sites (1176, 1256, 1287):
  if sy.strategic_output.get(resource) >= 30.0 { continue; }
  // economy.rs:1393–1395 — three related friction caps:
  f *= 1.0 - (max_piracy / 200.0).clamp(0.0, 0.5);
  f *= 1.0 - (max_interdiction / 200.0).clamp(0.0, 0.6);
  f *= 1.0 + (max_patrol / 400.0).clamp(0.0, 0.25);
  // economy.rs:1287:
  if we.supply_resilience >= 30.0 { risk = lower_risk(risk); }
  ```
- **Why it matters:** The `30.0` self-sufficiency threshold appears at lines 1176, 1256, and 1287 without a name — a calibration change requires three edits. The route friction divisors (200.0, 400.0) and caps (0.5, 0.6, 0.25) also have no semantic names.
- **Fix:** Define `const SELF_SUFFICIENT_THRESHOLD: f32 = 30.0;` and `const ROUTE_PIRACY_DIVISOR: f32 = 200.0;` etc. in the preamble of `economy.rs` (or in `economy/tables.rs` after the split). No behavioral change.
- **Effort:** S
- **Risk / deps:** No golden risk. No ordering change.

---

## Suggested local order

1. **B7** (S, no risk) — replace `format!("{}",x).into()` with `Arc::from(x.as_slug())` in `analytics.rs`. Mechanical, zero test risk.
2. **B4** (S) — extract `cap_per_anchor` generic or resolve B-S2 first; fix the merge_manual divergence while touching both files.
3. **B9** (S) — centralize `cmp_f32_desc` in `analysis/mod.rs`, replace 15 sites; run goldens after.
4. **B12** (S) — name the magic thresholds.
5. **B1** (M) — pre-bucket `deps` into `BTreeMap<(to_system_id, resource), Vec<…>>` before the supply-risk loop; run goldens.
6. **B6** (M) — key cooccurrence on `(u32,u32)` faction indices; no output ordering change.
7. **B5** (M) — `fields_mut()` or arithmetic helpers for `StrategicOutput` / `PresenceDimensions` / `ResourceVector`.
8. **B3** (M) — index `kind_rules` / `disposition_rules` before the pair loop.
9. **B-S3** (M) — `enum_slug!` macro; apply to analysis enums first.
10. **B10** (S, golden risk) — name weights + fields_arr; verify with `cargo test --test it -- golden` before committing.
11. **B-S2** (M) — `WeightedAnchored` trait; align merge_manual policy first.
12. **B-S1** (L) — `SectorReport` trait; defer until other dedup is stable.
13. **B11** (L) — god-module split; do last, behind pinned content golden (G2).
14. **B8** (S, defer) — only if `report_top` is expected to grow beyond ~20.
