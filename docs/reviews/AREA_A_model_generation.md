# AREA A — src/model + generation — verification

Dated 2026-06-05. Covers the data-model layer (`src/model/sector_model/mod.rs`, `mutation.rs`, `model/taxonomy.rs`, `model/rng.rs`) and the generation pipeline (`src/gen/generation/mod.rs`, `gen/generation/routes.rs`, `gen/hidden_routes.rs`). All line numbers were re-verified against the live tree; the review's cited lines matched exactly in every case.

| ID  | Sev | Status | Effort | One-line |
|-----|-----|--------|--------|----------|
| A1  | MED | ✅ Confirmed | L | Render vocab (RoutePattern/strides/stable_pattern_hash) lives in the data-model god-file alongside DTOs |
| A2  | MED | ✅ Confirmed | M | WorldDto holds 9 enum fields as Arc<str> — stringified on build, string-compared later |
| A3  | MED | ✅ Confirmed | S | Route weighting compares feature tags by raw string literals; rename-safe alternative exists |
| A4  | MED | ⚠️ Partial | S | Four hand-written parse tables with no exhaustiveness guard; one round-trip test exists but covers only WorldType |
| A5  | MED | ✅ Confirmed | L | 157 pub fields across struct bodies make command-bus invariant unenforceable at type level |
| A6  | LOW | ✅ Confirmed | S | GENERATOR_NAME/VERSION `.to_string().into()` double-allocs ×4 in gen/generation/mod.rs |
| A7  | LOW | ✅ Confirmed | M | O(n) linear scans in get_system/get_world; no BTreeMap index |
| A8  | LOW | ✅ Confirmed | S | get_system/get_world/all_worlds/get_worlds_for_system lack #[must_use] |
| A9  | LOW | ✅ Confirmed | S | (*self.regions).clone() full deep-copy ×4 per hex edit — Arc::make_mut opportunity |
| A10 | LOW | 🔄 Moved | S | .unwrap() ×2 are at lines 455–456 (review cited :455) — confirmed correct, no drift |
| A11 | LOW | ✅ Confirmed | S | format!("{b:02x}") per byte in hex() — can use write! into pre-sized buffer |
| A12 | LOW | ✅ Confirmed | S | GenerationManifest built with divergent defaults in two places |

---

### A1 — Render vocab lives in the data-model god-file

- **Review sev / bucket:** MED / P2
- **Status:** ✅ Confirmed
- **Location:** `src/model/sector_model/mod.rs:700–810` (verified — no drift)
- **Evidence:**
  ```rust
  fn stable_pattern_hash(route_type: RouteType, key: &str) -> u32 { … }  // line 700
  pub enum RoutePattern { Solid, Dashed, DotDash, … }                     // line 732
  pub fn strides(self) -> &'static [f32] { … }                            // line 775
  ```
- **Why it matters:** `RoutePattern`/`strides`/`stable_pattern_hash` are rendering vocabulary; they are imported directly by `export/bitmap/`, `export/svg_export/`, `export/render_core/`, and `gui-core/palette.rs`, yet they live in the DTO module alongside `GeneratedSector` and `WorldDto`. The file is 1516 LOC. Splitting render vocab out decouples the DTO layer from render consumers.
- **Fix:** Extract `pub enum RoutePattern`, `stable_pattern_hash`, `strides`, `RouteViewMode`, `RouteKind::patterns` into `src/model/sector_model/routes_view.rs` and re-export from the module root. No callers need to change their import paths if re-exported.
- **Effort:** L (the enum and its impls span ~150 LOC; all callers use `crate::sector_model::RoutePattern` so a re-export is safe, but the split requires auditing every impl block that touches both DTO and render fields).
- **Risk / deps:** No golden exposure (patterns are pure data). Must verify that `impl RouteType { fn pattern_key }` stays with `RouteType` or moves with `RoutePattern`. No determinism impact.

---

### A2 — WorldDto stores enum fields as Arc<str>

- **Review sev / bucket:** MED / P1.5
- **Status:** ✅ Confirmed
- **Location:** `src/model/sector_model/mod.rs:248–279` (verified — no drift)
- **Evidence:**
  ```rust
  pub struct WorldDto {
      pub world_type: Arc<str>,
      pub atmosphere: Arc<str>,
      // … 7 more Arc<str> fields
  }
  impl From<&crate::worlds::World> for WorldDto {
      fn from(world: &crate::worlds::World) -> Self {
          Self { world_type: Arc::from(world.world_type.to_string()), … }
  ```
- **Why it matters:** A renamed enum variant compiles cleanly but breaks any downstream code that string-compares against `world_type` (e.g. template rendering, diff logic). Holding the real enums with `#[serde(into/from)]` makes mismatches a compile error.
- **Fix:** Replace the 9 `Arc<str>` fields with the actual `worlds.rs` enum types (`WorldType`, `Atmosphere`, etc.). Add `#[serde(into = "WorldDtoRaw", from = "WorldDtoRaw")]` where `WorldDtoRaw` preserves the existing JSON schema. The `notable_features` field can stay as `Vec<NotableFeature>` with its existing `Display` impl.
- **Effort:** M (struct definition + serde shim + all construction sites; mutation.rs and builder panels may construct `WorldDto` directly).
- **Risk / deps:** JSON schema for `sector.json` must stay identical — the serde shim is the safety net. Run golden tests after: `cargo test --test it -- golden`.

---

### A3 — Route weighting matches feature tags by raw string literals

- **Review sev / bucket:** MED / P1.5
- **Status:** ✅ Confirmed
- **Location:** `src/gen/generation/routes.rs:52–68` (verified — no drift)
- **Evidence:**
  ```rust
  if combined_tags.iter().any(|t| {
      let s = t.as_ref();
      s == "feature:trade_hub"
          || s == "feature:freeport"
          || s == "feature:major_spaceyard"
          || s == "feature:administrative_hub"
          || s == "feature:subsector_hegemon"
  }) { w *= 2.0; }
  ```
- **Why it matters:** Tags are generated in `world_placement.rs:310` as `format!("feature:{}", snake(f.as_ref()))` where `f` is `NotableFeature`. If a variant is renamed (e.g. `TradeHub` → `TradeDepot`), `AsRef<str>` and `snake()` would produce `"feature:trade_depot"` but the literals here still say `"feature:trade_hub"` — silent weight mismatch, no compile error.
- **Fix:** Pull the tag string from the enum: `NotableFeature::TradeHub.as_ref()` → `snake()` → `format!("feature:{}", …)` — or add a `NotableFeature::tag(self) -> &'static str` method and compare against that. The modifier-driven block at lines 76–89 already does the right thing (builds the tag from the config string, so it's a separate concern).
- **Effort:** S (replace two literal blocks with enum-keyed comparisons; the `NotableFeature::VARIANTS` + `AsRef<str>` impl already exist in `worlds.rs`).
- **Risk / deps:** No golden exposure. No determinism impact (weights are deterministic inputs).

---

### A4 — Four hand-written parse tables with partial exhaustiveness guard

- **Review sev / bucket:** MED / P1
- **Status:** ⚠️ Partial
- **Location:** `src/model/taxonomy.rs:35–218` (verified — no drift)
- **Evidence:**
  ```rust
  pub fn parse_star_colour_variant(s: &str) -> Option<StarColour> { … }   // line 35
  pub fn parse_world_type_variant(s: &str) -> Option<WorldType> { … }     // line 48
  pub fn parse_government_variant(s: &str) -> Option<Government> { … }    // line 78
  pub fn parse_notable_feature_variant(s: &str) -> Option<NotableFeature> { … }  // line 114
  ```
- **Why it matters:** A new variant added to `worlds.rs` silently returns `None` from the corresponding parse fn — no compile error. The only round-trip test (`variant_name_round_trip_for_world_type`, line 233) covers one variant of one enum; `StarColour`, `Government`, and `NotableFeature` are untested. The status is Partial because the test infrastructure exists — just one table is covered.
- **Fix (option A — tests):** Add `#[test]` that iterates each enum's `VARIANTS` slice and asserts `parse_*_variant(&v.to_string()) == Some(v)` — the `VARIANTS` constants already exist in `worlds.rs`. This catches drift at test time with zero new dependencies.
- **Fix (option B — strum):** Add `strum` to workspace deps and derive `EnumString`/`AsRefStr` on the four enums, deleting the four `taxonomy.rs` parse fns. `strum` is not currently in the workspace.
- **Effort:** S for option A (four test fn bodies, ~20 lines total). M for option B (strum adoption, broader impact on `Display`/`FromStr` impls across `worlds.rs`).
- **Risk / deps:** Option A is zero-risk. Option B touches `worlds.rs` `Display`/`FromStr` impls and may ripple into serde round-trips — run golden tests.

---

### A5 — ~157 pub fields across sector_model/mod.rs structs

- **Review sev / bucket:** MED / P2 (god-file)
- **Status:** ✅ Confirmed
- **Location:** `src/model/sector_model/mod.rs:16–1516` (verified — field count 157 by `grep "^\s*pub [a-zA-Z_][a-zA-Z0-9_]*:"`)
- **Evidence:**
  ```rust
  pub struct GeneratedSector { pub id: Arc<str>, pub title: Arc<str>,
      pub seed: Arc<str>, pub generator_name: Arc<str>, …  // 18 structs total
  pub struct WorldDto { pub star_colour: Arc<str>, …       // all fields fully public
  ```
- **Why it matters:** With all fields public, there is no way to enforce the CLAUDE.md command-bus invariant at compile time — callers can write `sector.systems[i].primary_factions = …` directly. The review correctly notes this is a documentation / accessor problem, not a current bug.
- **Fix:** For the most mutation-sensitive structs (`GeneratedSector`, `GeneratedSystem`, `GeneratedWorld`), move mutation-only fields to `pub(crate)` and expose them through the `mutation.rs` API. Add a doc comment at the top of `mod.rs` citing the invariant. This is a prerequisite for enforcing bus discipline by type.
- **Effort:** L (touches every caller in `builder/`, `viewer/`, `tests/it/`; likely needs a `pub(crate)` field audit pass plus `#[cfg(test)]` carve-outs per CLAUDE.md policy).
- **Risk / deps:** Wide cascade. Do after golden tests (G2) are in place. Sequential with any builder panel work.

---

### A6 — GENERATOR_NAME/VERSION double-alloc ×4

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `src/gen/generation/mod.rs:619–620, 840–841` (verified — no drift)
- **Evidence:**
  ```rust
  generator_name: crate::GENERATOR_NAME.to_string().into(),
  generator_version: crate::GENERATOR_VERSION.to_string().into(),
  ```
  (`GENERATOR_NAME: &'static str` in `src/lib.rs:178`)
- **Why it matters:** `.to_string()` allocates a `String`; `.into()` allocates a second `Arc<str>`. `Arc::from(crate::GENERATOR_NAME)` skips the intermediate `String`. Cosmetic, happens only at sector build time (not hot path).
- **Fix:** Replace `.to_string().into()` with `Arc::from(crate::GENERATOR_NAME)` at all four sites. The `new_empty` constructor in `mod.rs:296` already uses `.into()` correctly on a `&'static str` (single alloc).
- **Effort:** S (four one-line edits).
- **Risk / deps:** No golden exposure. No determinism impact. `generated_at_policy: "not recorded by default".to_string().into()` at line 839 also has the same pattern and can be fixed at the same time (a fifth site, not counted in A6's original ×4).

---

### A7 — O(n) linear scans in get_system / get_world

- **Review sev / bucket:** LOW / P2
- **Status:** ✅ Confirmed
- **Location:** `src/model/sector_model/mod.rs:332–357` (verified — no drift)
- **Evidence:**
  ```rust
  pub fn get_system(&self, id: &SystemId) -> Option<&GeneratedSystem> {
      self.systems.iter().find(|s| s.id == *id)
  }
  pub fn get_world(&self, id: &WorldId) -> Option<&GeneratedWorld> {
      for sys in &self.systems { for w in &sys.worlds { if w.id == *id { return Some(w) } } }
  ```
- **Why it matters:** `get_world` is O(systems × worlds). Called by builder and analysis code; not a hot loop today, but adds latency on sectors with many systems (max ~500 systems × ~8 worlds = ~4000 comparisons per lookup).
- **Fix:** Add a `BTreeMap<SystemId, usize>` index (system index by id) and a `BTreeMap<WorldId, (usize, usize)>` index (system-index, world-index) built lazily or maintained by `mutation.rs`. Alternatively, callers that need repeated lookups can build a local map.
- **Effort:** M (index field on `GeneratedSector`; update every mutation site; serde must `skip_serializing` the index).
- **Risk / deps:** The index field must be excluded from golden JSON (`#[serde(skip)]`). Must not change sector.json byte layout.

---

### A8 — Read accessors lack #[must_use]

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `src/model/sector_model/mod.rs:332–357` (verified — no #[must_use] preceding any of the four accessor fns)
- **Evidence:**
  ```rust
  pub fn get_system(&self, id: &SystemId) -> Option<&GeneratedSystem> { … }
  pub fn get_system_mut(&mut self, id: &SystemId) -> Option<&mut GeneratedSystem> { … }
  pub fn get_world(&self, id: &WorldId) -> Option<&GeneratedWorld> { … }
  pub fn all_worlds(&self) -> impl Iterator<Item = &GeneratedWorld> { … }
  ```
- **Why it matters:** Without `#[must_use]`, callers can silently drop the returned `Option` or `Iterator`, discarding a lookup result with no compiler warning. Clippy enforces this for free if the attribute is present.
- **Fix:** Add `#[must_use]` to `get_system`, `get_system_mut`, `get_world`, `get_worlds_for_system`, `all_worlds`. (Other methods in the file already carry the attribute correctly.)
- **Effort:** S (five one-line additions).
- **Risk / deps:** None. Pure annotation; no behaviour change.

---

### A9 — (*self.regions).clone() full deep-copy ×4 per hex edit

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `src/model/sector_model/mutation.rs:457, 470, 481, 494` (verified — no drift)
- **Evidence:**
  ```rust
  pub fn add_region(&mut self, …) {
      let mut regions = (*self.regions).clone();   // deep-copy of entire Vec<WarpRegion>
      regions.push(…);
      self.regions = std::sync::Arc::new(regions);
  }
  ```
  (Pattern repeated at lines 470, 481, 494 for `remove_region`, `add_region_hex`, `remove_region_hex`.)
- **Why it matters:** Each region edit clones the entire `Vec<WarpRegion>` even when the `Arc` has exactly one reference (the common case during a builder session). `Arc::make_mut` elides the clone when the refcount is 1.
- **Fix:** Replace `(*self.regions).clone()` with `Arc::make_mut(&mut self.regions)` which returns `&mut Vec<WarpRegion>` directly; remove the re-assign `self.regions = Arc::new(…)`. Example:
  ```rust
  Arc::make_mut(&mut self.regions).push(WarpRegion { … });
  ```
- **Effort:** S (four function bodies, mechanical substitution).
- **Risk / deps:** No golden exposure. Semantically identical when refcount > 1 (clones on write as before); cheaper when refcount == 1.

---

### A10 — Infallible .unwrap() ×2 in hidden_routes.rs

- **Review sev / bucket:** LOW / P3
- **Status:** 🔄 Moved (review cited :455; actual lines are 455–456 — exact match, no drift)
- **Location:** `src/gen/hidden_routes.rs:455–456` (verified)
- **Evidence:**
  ```rust
  let a = endpoint_by_id.get(from.as_str()).copied().unwrap();
  let b = endpoint_by_id.get(to.as_str()).copied().unwrap();
  ```
- **Why it matters:** `endpoint_by_id` is built from `endpoints` (line 401–402) and `pairs` is built only from those same endpoints (lines 410–426), so the invariant holds. But the panic message on failure gives no context. `.expect("hidden-route pair references unknown endpoint — invariant violated")` makes future debugging faster.
- **Fix:** Replace both `.unwrap()` with `.expect("hidden-route pair endpoint not in index — invariant: pairs are built from endpoints only")`.
- **Effort:** S (two one-line edits).
- **Risk / deps:** None. Semantically identical when invariant holds; better panic message otherwise. No golden exposure.

---

### A11 — format!("{b:02x}") per byte in rng.rs hex()

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `src/model/rng.rs:71–77` (verified — no drift)
- **Evidence:**
  ```rust
  pub fn hex(bytes: &[u8]) -> String {
      let mut s = String::with_capacity(bytes.len() * 2);
      for b in bytes {
          s.push_str(&format!("{b:02x}"));  // allocates a String per byte
      }
      s
  }
  ```
- **Why it matters:** `format!("{b:02x}")` allocates a temporary `String` per byte; `write!(&mut s, "{b:02x}")` writes directly into the pre-allocated buffer with zero intermediate allocation. For 32-byte blake3 hashes this is 32 needless allocations per hash formatting call. Called on every sector generation for `seed_hash` and `settings_digest`.
- **Fix:**
  ```rust
  use std::fmt::Write as _;
  for b in bytes { write!(&mut s, "{b:02x}").unwrap(); }
  ```
  The `unwrap()` is safe — `write!` on a `String` is infallible.
- **Effort:** S (three-line change).
- **DETERMINISM RISK:** Any change to `hex()` must produce **byte-identical output** — it feeds `seed_hash`/`settings_digest` fields written into `sector.json` and compared in golden tests. The `write!` replacement produces identical lowercase hex; verify by running `cargo test --test it -- golden` after the change. Do not change the format specifier (`{b:02x}`) or byte ordering.

---

### A12 — GenerationManifest built with divergent defaults in two places

- **Review sev / bucket:** LOW / P3
- **Status:** ✅ Confirmed
- **Location:** `src/model/sector_model/mod.rs:303–319` (new_empty) vs `src/gen/generation/mod.rs:837–862` (build_manifest) (verified — no drift)
- **Evidence:**
  ```rust
  // mod.rs:305 — new_empty path:
  generated_at_policy: "unknown".into(),
  seed_hash: "".into(),
  settings_digest: "".into(),

  // gen/mod.rs:839 — build_manifest path:
  generated_at_policy: "not recorded by default".to_string().into(),
  seed_hash: seed_hash.into(),            // computed from config
  settings_digest: settings_digest.into(), // computed from config
  ```
- **Why it matters:** The two build sites use different sentinel strings for `generated_at_policy` (`"unknown"` vs `"not recorded by default"`). If a consumer checks this field to detect an incomplete manifest, it must handle both strings. Adding a new `GenerationManifest` field requires updating two independent construction sites.
- **Fix:** Extract a `GenerationManifest::empty(id, seed)` constructor (for the builder/GUI path) alongside the existing `build_manifest` fn (for the generation pipeline). `new_empty` in `mod.rs` delegates to `GenerationManifest::empty(…)`. A single location defines the sentinel strings.
- **Effort:** S (add one constructor fn; update `new_empty` to call it).
- **Risk / deps:** No golden exposure (the empty manifest is only produced by the builder's new-sector path, not by generation). No determinism impact.

---

## Determinism / invariant spot-check

- **Fx-maps:** Neither `src/model/sector_model/mod.rs` nor `src/gen/generation/mod.rs` iterate `FxHashMap`/`FxHashSet` for output. All output-facing collections use `Vec` (sorted by `sorted_systems`/`sorted_routes` at gen time) or `BTreeMap`. Clean.
- **RNG:** `src/model/rng.rs` exports `stage_rng`/`derive_stage_seed`/`weighted_index`. No `rand::thread_rng()` appears in `src/gen/` or `src/model/`. `src/gen/random_sector.rs` explicitly documents the absence of `thread_rng`. Clean.
- **Square-sector invariant:** `src/validate/validation.rs:81` enforces `GEN_SECTOR_NOT_SQUARE`. `src/gen/generation/mod.rs` passes `sector_width`/`sector_height` through without enforcement (relies on pre-gen validation). The CLAUDE.md carve-out for proptest is in place. Clean.
- **Byte-stable writers:** A11's `hex()` fix is the only change in this area that touches generation output. It must be verified against goldens as noted above.

---

## Suggested local order

1. **A8** — `#[must_use]` on accessors. Zero-risk, five lines. Good warm-up.
2. **A10** — `.expect(…)` replacements. Two lines, no risk.
3. **A6** — `Arc::from` double-alloc fix. Four/five lines. S effort.
4. **A9** — `Arc::make_mut` in mutation.rs. Four function bodies, mechanical. S effort.
5. **A11** — `write!` in `rng.rs::hex`. **Run golden tests immediately after** (`cargo test --test it -- golden`). Three lines, highest risk of the S-effort items.
6. **A12** — `GenerationManifest::empty` constructor. Eliminates divergent-defaults drift before the god-file split touches construction sites.
7. **A4** — Add round-trip tests for StarColour/Government/NotableFeature parse tables. S effort, zero risk, eliminates silent-parse-drift class from three of four tables.
8. **A3** — Replace raw `"feature:…"` literals with enum-keyed tag comparisons. S effort; do alongside or after A4 since both touch the same type-safety theme.
9. **A2** — WorldDto real-enum refactor. M effort; needs serde shim + golden test run. Gate on G2 (content golden) being in place first.
10. **A7** — BTreeMap accessor index. M effort; requires careful serde exclusion. Gate on A2 (or do independently if A2 is deferred).
11. **A1 / A5** — God-file split and field visibility tightening. L effort each; do last, behind golden test coverage and after the M-effort type-safety fixes are settled.

**Gating note:** A2's serde shim and A1's module split both touch byte-stable output paths. Neither should land before the G2 content golden (pinning `sector.json`/`sector.md` output) is committed — that golden is the safety net confirming the JSON schema has not changed.
