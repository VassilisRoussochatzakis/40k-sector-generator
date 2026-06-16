# SectorForge Test-Gap Audit

> Generated 2026-06-07 by a 28-agent audit workflow (1 recon → 13 per-area finders → 13 adversarial verifiers → 1 synth). Raw finders surfaced 169 candidate gaps; verifiers killed false/already-covered ones and dedup merged duplicate framings → **88 confirmed-real, zero-coverage gaps** (31 high / 41 med / 16 low). Each verified by grep across `tests/`, inline `#[cfg(test)]` modules, and source line citations. **Excludes** UI screenshot / live-egui-frame snapshot tests per scope. Line numbers are as-of generation — confirm before implementing.

Two real **bugs** surfaced as a side effect (not just missing coverage): markdown pipe-injection in `render_sector_markdown` (zero escaping) and the `parse_hex_rgb` mid-char-slice crash (fixed in commit ab6d2dc but unguarded by any proptest).

## 1. Executive Summary

### By priority
| Priority | Count | Character |
|---|---|---|
| **High** | 31 | Unguarded invariants (geometry, determinism, monotonicity, referential integrity), external contracts (CLI exit codes, security guards), error paths to documented behavior. A regression passes every existing test. |
| **Med** | 41 | Branch/boundary coverage, override/precedence paths, degenerate input, command apply/revert round-trips. Real but lower blast radius or partly golden-backstopped. |
| **Low** | 16 | Defensive/near-dead code, format canaries caught transitively by goldens, cheap drift-guards. |

### By area
| Area | Gaps (H/M/L) | Notes |
|---|---|---|
| `src/validate/validation.rs` | 31 (15/13/3) | **Dominant cluster.** ~40 validation codes in source; `validation_tests.rs` covers only 3. |
| `src/model/rng.rs` | 9 (3/3/3) | Determinism keystone. `weighted_index` errors, `derive_stage_seed` keying, ChaCha8 pin uncovered. |
| `src/gen/...routes/regions/hidden_routes` | 9 (5/4/0) | Route-safety-monotonic invariant enforced piecewise in 3 modules; **no end-to-end composition test**. |
| `src/analysis` relations/control/economy | 16 (6/10/0) | Override-apply, tie-break determinism, treaty/combine rules — mostly golden-only. |
| `src/export` render/svg/html/bitmap | 11 (4/4/3) | Markdown pipe-injection (real bug), SVG escaping, heatmap determinism, empty-sector. |
| `src/cli` | 8 (4/4/0) | Exit-code table, `resolve_formats`/`parse_heatmap`/`resolve_size`, new `generate-all`. |
| `src/gen/generation` placement/systems/world_placement | 8 (3/5/0) | No inline tests in `placement.rs`/`systems.rs`; relaxation cascade, sort determinism, name dedup. |
| `src/loading` + serde round-trips | 9 (2/6/1) | Path-escape guard (security), `SectorSave`/`SectorMeta`/`FactionDef`-alias round-trips. |
| `builder/.../state` + `command.rs` | 17 (8/7/2) | Apply/revert (`RemoveWorld` position, `AdvanceConflictTicks` dominant-restore), derivation freshness, §R4 carve-out. |
| `viewer/...` | 14 (5/7/2) | Referential-integrity cascades on delete, save-as guard, `DataEditor` round-trip. |
| `gui-core/...` | 11 (3/8/0) | `top_route_control` tie-break, history predicates, truncation off-by-ones, `..base` preservation. |
| `examples/*.toml` parse | 3 (0/3/0) | `big_test`/`big_sparse_test`/`segmentum_example.toml` never parsed by any test. |

### Top 10 highest-leverage, add first
1. **`route_monotonicity.rs` (new integration)** — over seeds×densities assert `stability_level(short) <= stability_level(long)` within each `(route_type, hazard-class)` bucket, and `Webway == Stable` always. *The explicit "route safety is monotonic" invariant is enforced in 3 modules and verified end-to-end nowhere.*
2. **`GEN_SECTOR_NOT_SQUARE` fires** (`validation_tests.rs`) — non-square config → `report.ok==false` + that code; square → absent. *Canonical pre-gen guard of the non-negotiable square-sector invariant (commit 30b8c4f); zero coverage.*
3. **`weighted_index` error paths** (`rng.rs` inline) — empty pool, all-zero/negative, non-finite → `Err(WeightedSelectionFailed{context})` with context propagated. *Sole failure mode, maps to CLI exit 70; would silently become a panic/Ok-fallback under refactor.*
4. **`from_error` exit-code table** (`exit_code.rs` inline) — one `SectorError` per variant → asserted code (Io=74, ConfigParse=78, WorldDataLoad=65, Cancelled=130, ValidationFailed=1, WeightedSelectionFailed=70, `_`=70). *External process-exit contract; an arm reorder or new variant falling into `_=>70` ships silently.*
5. **`derive_stage_seed` discriminator-keying** (`rng.rs` inline) — `assert_ne!` on seeds differing only in discriminator, plus delimiter-collision pairs (`(a,b,c)` vs `(a,bc,"")`). *Decorrelation mechanism for every per-entity RNG stream; the one input axis the existing stability test omits.*
6. **`read_relative` path-escape guard** (drive via `load_project`) — `[inputs]` ref of `../escape.toml` or `/etc/passwd` → `Err` containing "escapes project root"; benign nested path loads OK. *Security invariant, trivially regressable in a loader refactor.*
7. **Markdown pipe-injection** (`render.rs` inline) — system/faction/world named `Foo | Bar` must not add a table column (assert cell count == header). *Latent correctness bug in deterministic file output: `render_sector_markdown` does zero escaping; the m42 golden has no pipes.*
8. **`resolve_formats` policy** (`common.rs` inline) — `--exclude json`→Err; empty set→Err; `--light`→`[Json]`; `png`→Bitmap; dedup; unknown token names the flag. *Encodes the load-bearing "json non-excludable" invariant (viewer/segmentum read `sector.json`).*
9. **Position-preserving `RemoveWorld` revert** (`command.rs` inline) — remove a *middle* world, revert, assert it returns to its *original index* (not tail). *Distinct from `RemoveSystem`; a tail-append regression silently reorders worlds and breaks byte-stable re-serialization after undo.*
10. **GeneratedSector serde idempotence** (`invariants_tests.rs`) — `to_string(from_str(to_string(s))) == to_string(s)` on `fixture_sector()`. *Existing round-trip checks only 4 shallow fields; a dropped-on-load field (missing `#[serde(default)]`) passes today.*

---

## 2. Per-Area Tables

### 2a. `src/validate/validation.rs` — validation codes (31 gaps)
Harness for all: load `examples/m42_project`, mutate via `Arc::make_mut` on the relevant catalog (mirror `no_factions_is_ok`), call `validate_project`, assert `report.ok` and the specific code in `errors`/`warnings`. Keep all dims **square** to isolate from `GEN_SECTOR_NOT_SQUARE` (0×0 is square).

| Target (code) | type | What to assert | Pri |
|---|---|---|---|
| `GEN_SECTOR_NOT_SQUARE` (l.84) | int | 8×6 → `!ok` + code, severity Error, msg has "8"&"6"; 8×8 → absent; add 10×8 (other ordering) | **H** |
| `GEN_SYSTEM_COUNT_OVERFLOW` + `allow_empty_hexes` (l.107) | int | 4×4(16), count=100, `allow_empty_hexes=false`→code; `=true`→absent; count==16→absent (strict `>`) | **H** |
| `KEY_TABLE_EMPTY` (l.188) | int | clear `world_tables.world_types` → code, msg has "world_types"; intact → absent | **H** |
| `WB_NO_ROWS` (l.198) | int | `world_rows.clear()` → `!ok` + `WB_NO_ROWS` + `WB_NO_USABLE_ROWS`, NOT `WB_EXCLUDED_ROWS_SEVERE` | **H** |
| `WB_NO_USABLE_ROWS` (l.205) | int | N≥2 rows all `star_colour=None` → code + `WB_EXCLUDED_ROWS_SEVERE`, NOT `WB_NO_ROWS` | **H** |
| `WB_EXCLUDED_ROWS` warning branch (l.216) | int | 3 valid+2 invalid → warning, NOT severe, `ok==true`; boundary 2+2 (n==usable) → still warning | **H** |
| `FACTION_DUPLICATE_ID` (l.263) | int | push clone of `factions[0]` → `!ok` + code, `path==factions[N]` (2nd index) | **H** |
| `FACTION_BAD_WEIGHT` (l.273) | int | weight ∈ {0.0,-1.0,NaN,+inf} → code, `path==factions[0]`; 1.0 → absent | **H** |
| `RELATIONS_PAIR_/OVERRIDE_UNKNOWN_FACTION` (l.468/485) | int | ghost id in `pair_overrides[0].a` → Error + path, slot b not flagged; clear factions → suppressed; kind-id accepted | **H** |
| `ROUTE_BAD_DEFAULT_WEIGHT` (l.324) | int | `enabled=true`, weight ∈ {0,-1,NaN} → code; `enabled=false` → absent (gating); 1.0 → absent | **H** |
| `ROUTE_BAD_MULTIPLIER` (l.338) | int | modifier multiplier ∈ {0,-1,NaN} → code, `path==routes.modifiers[0]` | **H** |
| `REGIONS_COUNT_OVERFLOW` (l.533) | int | 8×8, `enabled=true`, count=33(>32) → code; count=32 → absent (strict `>` vs `cells/2`) | **H** |
| `ECONOMY_TECH_/POP_MULTIPLIER_BAD` (l.578/593) | int | neg/NaN → keyed-path codes; `enabled=false` → suppressed; 0.0 → allowed | **H** |
| `RESOURCE_SCORE_BAD` (l.618) | int | food=150/ore=-1 under `world_type.hive_world` → 2 codes w/ per-field path; 0.0/100.0 → absent; test both scopes | **H** |
| `OUT_BITMAP_SCALE_RANGE` (l.142) | int | scale=0 & =9 → 2 codes w/ field name in msg; 1 & 8 → absent (1..=8 both ends) | M |
| `OUT_BITMAP_THEME_INVALID` (l.154) | int | `theme.name=Some("no_such")` → code (resolver→validation bridge) | M |
| `GEN_GRID_EMPTY` (l.74) | int | 0×0 → code AND not `GEN_SECTOR_NOT_SQUARE`; 8×8 → absent | M |
| `GEN_WORLD_COUNT_RANGE` (l.117) | int | min=5,max=2 → code; min==max==2 → absent | M |
| `GEN_MIN_WORLDS_ZERO` (l.124) | int | min=0 → in `warnings` only, `report.ok==true`; min=1 → absent (severity bucketing) | M |
| `GEN_FEATURE_POOL_SMALL` (l.250) | int | `world_feature_count=10_000` → warning; default → absent | M |
| `FACTION_UNKNOWN_WORLD_TYPE/GOV/FEATURE` (l.282/295/308) | int | bogus slugs → 3 warnings w/ paths, `ok==true`; valid → absent | M |
| `ROUTE_MAX_DISTANCE_ZERO` (l.331) | int | `enabled=true`, max=0 → warning; gated off; 5 → absent | M |
| route-condition type-safety (P10) | unit | `RouteCondition` fields are typed enums — bogus value → hard deserialize error at load (covered by `route_condition_misspelled_*` in `src/gen/routes.rs`); the old `ROUTE_UNKNOWN_*` warnings were removed | — |
| `NAME_POOL_EMPTY` (l.397) | int | clear all 3 of prefixes/suffixes/single_names → warning; leaving single_names → absent (AND-conjunction) | M |
| `RELATIONS_KIND_RULE_EMPTY` both branches (l.444/502) | int | empty `a` with-factions AND no-factions early-return → same code both paths | M |
| `REGIONS_COUNT_ZERO` (l.525) | int | `enabled=true`, count=0 → warning; gated off; 3 → absent | M |
| `REGIONS_MEAN_SIZE_ZERO` (l.544) | int | mean_size=0 → Error; 1 → absent | M |
| `REGIONS_CONDITION_BAD_WEIGHT` (l.551) | int | weight<0 or NaN → Error w/ path; **weight==0.0 → allowed** (asymmetry vs faction/route) | M |
| `RESOURCE_TRADE_MULTIPLIER_BAD` (l.648) | int | neg/NaN → code; 0.0 & 3.0 → absent (no upper cap, zero allowed) | M |
| `RESOURCE_SUPPLY_RESILIENCE_BAD` (l.663) | int | <-100 or >100 or NaN → code; -100/0/100 → absent (signed range, negatives valid) | M |
| `validate()` ok-aggregation (l.67) | int | warnings-only config → `ok==true` & `errors.is_empty()`; errors → `ok==false`; invariant `ok==errors.is_empty()` | M |
| `MAX_SECTOR_DIM` boundary (l.97) | int | 1024×1024 → no `GEN_SECTOR_TOO_LARGE`; 1025×1025 → code (ref the `pub` const, not literal) | M |
| `OUT_NO_FORMATS` (l.133) | int | `formats=vec![]` → warning; `[Json]` → absent | L |
| `WorldWorkbookValidation` summary fields | int | K rows `star_colour=None` → `excluded_row_count==K`, `row_count==orig+K`, `exclusion_reasons` sums to K | L |

### 2b. `src/model/rng.rs` — determinism keystone (9 gaps)
| Target | type | What to assert | Pri |
|---|---|---|---|
| `derive_stage_seed` discriminator/delimiter | unit | `assert_ne!` differing only in discriminator (`sys-1` vs `sys-2`, vs `""`); delimiter pairs `(a,b,c)`≠`(a,bc,"")`≠`(ab,c,"")` | **H** |
| `weighted_index` error paths | unit | empty/`[]`, all-zero+neg, `[+inf]` → `Err(WeightedSelectionFailed{context})`, context verbatim | **H** |
| `weighted_index` neg-skip + proportionality | unit | `[("neg",-5),("good",2),("inf",inf)]`→always idx 1 over 200 draws; `[1.0,9.0]`→b count ∈ 4300..4700 over 5000 (fixed seed) | M |
| `stage_rng` stream identity + decorrelation | unit | same key→identical 16-draw `Vec<u64>`; distinct discriminator AND distinct stage→non-identical | M |
| `stage_rng` ChaCha8 cross-version pin | golden | `stage_rng("sectorforge-test","pin","").gen::<u64>()` == captured constant (canary for `rand_chacha` bump) | M |
| `hex` | unit | `[]`→`""`, `[0x0f]`→`"0f"`, `[0xde,0xad]`→`"dead"`, len==2×input, lowercase | M |
| `weighted_index` reverse-scan fallback (l.60) | unit | `[0.0,0.0,1.0]`→always idx 2 over 200 draws (trailing-valid index) | L |
| `digest_bytes` | unit | deterministic, input-sensitive, len==64, lowercase hex; optionally pin blake3 of `b"abc"` | L |
| `hash_root_seed` | unit | deterministic, seed-sensitive, **≠`derive_stage_seed(seed,"","")`** (guards against unifying the two), len==64 | L |

### 2c. Route safety monotonicity — `routes.rs` / `regions.rs` / `hidden_routes.rs` (9 gaps)
| Target | type | What to assert | Pri |
|---|---|---|---|
| End-to-end composition (new `route_monotonicity.rs`) | proptest | over seeds×density: within `(route_type, hazard-sig)` bucket `d1<d2 ⇒ level(s1)<=level(s2)`; **every Webway==Stable**. Square sectors only; assert weak `<=`, bucket by identical hazard-sig | **H** |
| `distance_base_level` | proptest | `d1<=d2 ⇒ base(d1,max)<=base(d2,max)` over max∈1..=12; boundaries `(3,4)==1,(4,4)==3`; `max=0` no panic (use `d>=3` for the `>=max` arm) | **H** |
| `classify_route` hazard layering | unit | war_zone `+2` (not +1) & saturates via `.min(3)`; `StableWarpLane` only when `!war_zone&&!warp&&level<=1&&hub-tag`; monotone within hazard set. *(private — expose `pub(crate)` or build fixture)* | **H** |
| `hidden_route_stability` (webway always Stable) | unit | Webway==Stable for d∈{1,3,6,12,99}; SmuggLane d=1→Unstable, d≥6→Perilous; BlackShip never worse than Hazardous; strengthen `webway_links_two_aeldari_endpoints` with stability assert | **H** |
| `apply_route_effects` CalmCorridor floor-clamp | unit | extend `route()` helper with distance: short calm Hazardous→Unstable; long calm Hazardous→stays Hazardous (`.max(floor)`); short ≤ long | **H** |
| `dominant_route_condition` + `route_precedence` | unit | CalmCorridor+WarpStorm→WarpStorm; Turbulence>CalmCorridor; precedence-0 kinds→None; ladder 3>2>1>0 | M |
| `degrade` (Turbulence step) | unit | Stable→Unstable→Hazardous→Perilous→Perilous; ∀s `level(degrade(s))>=level(s)` | M |
| `stability_from_level`/`stability_level` | rt | level 0..=3 round-trips both directions; `from_level(255)==Perilous`; ordering 0<1<2<3 | M |
| `generate_routes` legacy 10% perilous-cap | unit | N perilous, cap=round(0.1×total): exactly count−limit downgraded, downgraded set is **shortest** (id tie-break). *(needs extraction or fixture)* | M |

### 2d. `src/analysis` — relations / control / economy (16 gaps)
| Target | type | What to assert | Pri |
|---|---|---|---|
| `load_relations_file`→`derive_with` delta | int | loaded cfg non-default; `to_string(derive) != to_string(derive_with)`; authored allied pair warmer-or-equal Aligned; re-run byte-identical | **H** |
| `compute_pair` PairOverride+RelationOverride precedence | unit | both on same pair: `secret_attitude` from rich wins, trust=5, cause threaded | **H** |
| `apply_relation_override` if-branch (config_a==lo, l.333) | unit | `a='alpha',b='zeta'` directional override → `a_to_b.secret==Hostile`, `b_to_a==Suspicious` (if-branch never tested) | **H** |
| `control.rs derive_world_claims` | unit | dedup-by-ClaimType keeps higher strength; score<=0 skipped; sort strength-desc then id-asc; clamp 0..=100 | **H** |
| `control.rs classify_system_state` | unit | 7-way cascade: empty→Uncharted, quarantined beats all, 2 contested→Warzone, dominant≠orbital→Blockaded, 2 dominants→Fragmented | **H** |
| `control.rs score_then_id` tie-break | unit | two presences ids `aaa`/`zzz`, **identical** scores → `aaa` wins (lex-smaller); determinism on re-run | **H** |
| `apply_relation_override` metric clamp | unit | trust=250 → ≤100 on pair + both directional views; `combine(trust)==100` | M |
| `combine_metrics` asymmetric rule | unit | `pair.trust==(a+b)/2` (mean); fear/rivalry/etc `==a.max(b)` (max) — via pair vs directional views | M |
| `treaty_status_of` derived branches | unit | imperial×merchant→Charter; imperial×mechanicus→Pact; ×chaos(AtWar)→Vendetta; same-kind Hostile no-warzone→Nonaggression | M |
| `Stance::shift` saturation | unit | `Allied.shift(-5)==Allied`; `AtWar.shift(10)==AtWar`; `Neutral.shift(2)==Hostile` | M |
| `derive_with_threshold` filter+fallback | unit | thresholds 3/2/1 → exact pair counts; all-empty-presence+threshold=2 → fallback to all → C(n,2) | M |
| `control.rs claim_for` ladder | unit | table-driven: `inquisition_*`→CovertWrit; precedence `imperial_inquisition_guard`→CovertWrit; fallbacks lawful→LegalSovereignty | M |
| `control.rs aggregate_faction_power` rollup | unit | two same-faction admin=60 → clamp 100, beats single admin=90; strategic_value 8× ratio; absent factions not in map | M |
| `economy/derive.rs apply_stability_nudge` | unit | stranded famine+20 clamped 100; non-stranded unchanged; `enabled=false`→byte-identical; only famine field touched | M |
| `economy/derive.rs friction_for` per-hazard | unit | Stable→1.0; Perilous→0.10; +patrol→>1.0,≤1.5; +piracy→≤0.5 cap; two controls→uses max | M |
| `economy/derive.rs derive_dependency_edges` | unit | two suppliers→higher-score wins; self-sufficient→no edge; Perilous-only route→no edge; output<35→no edge; sort `(to,resource,from)` | M |
| `load_economy_file`→`derive_with` delta | int | loaded≠default tables move numbers; `to_string` differ; re-run byte-identical | M |
| `analysis/mod.rs cmp_f32_desc/asc` | unit | desc(2,1)==Less, (1,2)==Greater, NaN cases==Equal both args; sort Vec w/ NaN no panic | M |

### 2e. `src/export` (11 gaps)
| Target | type | What to assert | Pri |
|---|---|---|---|
| `render_sector_markdown` pipe-injection | unit | system `Foo \| Bar`, faction `A\|B` → split-on-`\|` cell count == header; pin escape-or-encode behavior | **H** |
| `escape_xml_into` via `render_sector_svg` | unit | system `Ix <Prime> & "Co"` → `Ix &lt;Prime&gt; &amp; &quot;Co&quot;`; raw `<Prime>` absent; parallel region case | **H** |
| heatmap-on byte-stability | unit | `HeatmapMode::Control`: SVG render ×2 byte-identical; PNG ×2 identical blake3; on≠off (proves branch ran) — guards HashMap-iteration determinism | **H** |
| `render_sector_svg` real XML parse | unit | feed bytes to `quick_xml::Reader`, consume to Eof, no Err, tag depth→0; once w/ `&` name. *(promote quick-xml to dev-dep)* | M |
| bitmap `render` empty sector | unit | 0 systems/routes/factions, square 4×4 → no panic, `width>0&&height>0`, PNG len>0; SVG twin | M |
| `render_html` empty sector | unit | empty → `Ok`, `FACTION_PALETTE = {}`, `SECTOR` JSON parses w/ empty systems, `<!doctype html>` | M |
| `write_html`/`write_html_to` parity | int | returned path==`tmp/sector.html`; file bytes == `render_html`; two writes byte-identical | M |
| `encode_png_bytes` vs `write_sector_png_to` parity | int | `blake3(mem) == blake3(disk)` for same `RgbaImage` | M |
| `render_sector_markdown` empty-collection literals | unit | starless+empty → contains `_No worlds._`, `_No routes._`, `_No factions._`; starless glyph `X` | L |
| `render_sector_svg` subsector branches | unit | non-empty `&[Subsector]`+borders+labels → XML-balanced, contains `SUBSECTOR`, byte-identical ×2 | L |

### 2f. `src/cli` (8 gaps)
| Target | type | What to assert | Pri |
|---|---|---|---|
| `exit_code.rs from_error` | unit | per-variant: ValidationFailed→1, Cancelled→130, Io→74, ConfigParse/InvalidConfig→78, WorldDataLoad/NoWorldCandidates→65, ExportFailed→74, WeightedSelectionFailed→70 (compare `{:?}`, no getter) | **H** |
| `common.rs resolve_formats` | unit | `--exclude json`→Err "json cannot be excluded"; empty→Err; `--light`→`[Json]`; `png`→Bitmap; dedup; unknown names flag | **H** |
| `random.rs resolve_size` | unit | w≠h→Err "must be square"; dim=0→Err ">= 1"; dim=81→Err "<= 80"; single side→`Custom{dim}`; `(None,None,None)`→Medium | **H** |
| `generate_all.rs run_generate_all` | int | empty dir→success+"no sector projects"; 2 valid→"2/2"+both out/; 1 bad→"1/2"+other survives+FAILURE; nonexistent→74; only direct children counted | **H** |
| `common.rs parse_heatmap` | unit | `"OFF"`→Off, `"industry"`→Industrial, `"trade-volume"==tradevol`→TradeVolume, case-insensitive; unknown→Err w/ mode name | M |
| `generate.rs run_generate` bad formats | int | `--exclude json`→nonzero, stderr "json cannot be excluded", `sector.json` NOT written; `--formats bogus`→nonzero (78) | M |
| `validate.rs` exit codes | int | `--strict`→1 iff warnings (verify fixture warns first!); failing report→1; `validate-sector` invalid→1 (bypasses `from_error`) | M |
| `common.rs load_or_regenerate`/`resolve_sector_with_cfg` | unit | both supplied & neither → Err "pass exactly one of --project"; `--sector` arm → `C::default()` without calling extract (marker type) | M |
| `diff.rs run_diff` mode-select | int | `--before` alone, mixed flags → Err "pass either --before" (78) | L |
| `briefing.rs run_briefing` | int | `--preset bogus`→Err listing valid set (78); `--preset gm`→files written | L |

### 2g. `src/gen/generation` — placement / systems / world_placement (8 gaps)
| Target | type | What to assert | Pri |
|---|---|---|---|
| `place_systems` relaxation cascade + sort | unit | 4×4, count=12, `min_dist=3` → `len==12` (cascade reaches target); returned Vec **sorted**; ×2 byte-identical | **H** |
| `place_systems` count==0 early return | unit | `system_count=0`→`Vec::new()`; count=1→exactly 1 coord, q,r ∈ 0..4 | M |
| `deduplicate_name` Roman suffix | unit | `{Cadia}`→`Cadia II`; `{Cadia,Cadia II}`→`Cadia III`; empty→`Cadia` (no suffix); pins n=2 start | M |
| `generate_base_name` empty-pool fallback | unit | both pools empty→`System 7`; singles-only; pairs-only — each branch w/ fixed rng | M |
| `generate_worlds_for_system` min==max boundary | int | `min==max==3`, 6×6, `allow_empty_hexes` → every system has exactly 3 worlds (skips `gen_range`) | M |
| `build_random_config` degenerate Custom dim | unit | `Custom{dim:1}`→1×1; pin clamp(4) > cells=1; `dim:0`.dims()==(0,0); `.max(1)` prevents div-by-zero | M |
| `tags_for_world` sort + namespaces | unit | minimal World → one tag per 8 prefixes; Vec sorted; feature tags `feature:<snake>` | L |
| `place_systems` i32-overflow guard | unit | `46341*46341` computes without overflow (arithmetic only — extract or assert on math, avoid `Vec::with_capacity`) | L |

### 2h. `src/loading` + serde round-trips (9 gaps)
| Target | type | What to assert | Pri |
|---|---|---|---|
| `read_relative` path-escape guard | unit | `[inputs]` `../escape.toml` & `/abs` → Err "escapes project root"; nested relative loads OK (drive via `load_project`) | **H** |
| GeneratedSector serde idempotence | rt | `to_string(from_str(to_string(s)))==to_string(s)`; deep-check economy.enabled, chronicle.events.len | **H** |
| `SectorSave` JSON round-trip | rt | `split` → to_string → from_str → re-serialize equal; `SystemId` map-key serde; `merge` restores | M |
| `EconomyFile::into_config` no-clobber | unit | nested-only resources survive (false branch); both present → top-level wins (precedence) | M |
| `FactionDef` serde aliases | unit | `sub_faction`/`subfaction_id` → `.subfaction`; `sub_faction_name` → `.subfaction_name` | M |
| `examples/big_test`+`big_sparse_test` parse | int | `load_project` Ok; `width==height`; `system_count>0`; `subsector_width.is_some()`; catalogs non-empty | M |
| `segmentum_example.toml` parse | int | `toml::from_str::<SegmentumFile>` Ok; children non-empty; each `column<columns && row<rows` | M |
| `mint_seed` (random_sector.rs) | unit | `len()==16`, all hex; 8 mints into BTreeSet, `set.len()>1` | L |
| `PresetMeta` round-trip + checked-in files | rt | full-field round-trip; each `presets/*/preset.toml` parses; `""`→all-default | L |
| `parse_map_theme_file` bare fallback | unit | bare `name="x"` parses (fallback branch); garbage → Err "also failed as bare theme" | L |
| `AppConfig` unknown-field tolerance | unit | extra keys → Ok; misspelled required `system_cnt` → Err; misspelled optional → silent default | L |

### 2i. `builder/src/builder` — commands + state (17 gaps)
| Target | type | What to assert | Pri |
|---|---|---|---|
| `RemoveWorld` position-preserving revert | unit | remove middle W1 → captures `parent_position==Some(1)`; revert → `worlds[1].id==W1` (original index, NOT tail) | **H** |
| `AdvanceConflictTicks` apply/revert + dominant restore | unit | ticks=0→JSON byte-identical; ticks=3→3 before-vecs captured, revert restores conflict **and** `control.dominant` on all systems | **H** |
| `BuilderState::advance_conflict_ticks` wrapper | unit | ticks=0→no command pushed; ticks=2→log grows by exactly 1, undo restores | **H** |
| `run()` error path | unit | failing apply → `Err`, log/cursor/dirty unchanged, redo-tail NOT truncated, sector unchanged | **H** |
| `selection.rs` §R4 transient-state | unit | `focus_system`/`toggle_select`/`set_active_tab`/`focus_entity`/`nav_back` leave `command_log.len()` & cursor untouched; undo still reverts prior AddSystem | **H** |
| `ensure_fresh` stale-but-matching-fingerprint (l.167) | unit | flag stale w/o changing inputs → `ensure_fresh` clears via `mark_fresh` WITHOUT recompute (sentinel `report=None` stays None) | **H** |
| `recompute_economy` feed_stability cascade | unit | EconomyCfg always stales Hooks; `feed_stability=true` re-stales SystemsWorlds (→Personae stale); `=false`→Personae fresh | **H** |
| undo/redo derivation re-invalidation | unit | undo of SystemsWorlds command re-stales freshly-recomputed Hooks; redo too; precision: AddRoute undo stales Hooks not Personae | **H** |
| `AddRoute` controls derivation | unit | endpoints w/ faction presence → `controls` non-empty refs F; revert removes; re-run byte-identical (BTreeMap determinism) | M |
| `SetWorldConflict`/`SetSystemConflict`/`SetWorldStability` round-trip | unit | apply→after, revert→prior restored (compare serde_json); unknown id→matching NotFound error | M |
| `run()` redo-tail truncation | unit | AddSystem×3, undo×2, new branch → `log.len()==2`, cursor==2, redo no-op, survivors are sys-0+branch | M |
| `recompute_economy` per-system rollup | unit | 1 system, 2 worlds: ore 15+10=25→surplus; food -15+-10=-25→shortage; `sector_balance` matches | M |
| `recompute_chronicle_undoable` | unit | log grows by 1; manual event preserved; undo restores chronicle; redo re-applies | M |
| `pump_derivation_jobs` multi-kind drain | unit | Relations+Personae both stale → 2 in-flight; drain → both Fresh, outputs present | M |
| `is_background_eligible`/`dispatch` Economy exclusion | unit | stale Economy → never in-flight (mutates sector); eligible sibling still dispatched | M |
| `revalidate_now` D10 skip reason | unit | no worlds → `skip_reason==Some("no worlds catalog loaded")`; worlds present → cleared + report set | M |
| `export_block_reason` refuse branch | unit | invariant violation → `Some("1 invariant violation(s)"+"Export refused")`; strict+warnings differs from non-strict | M |
| `ReplaceSystem` before:None empty-hex | unit | drop on empty coord → `before` stays None; revert removes new system, resurrects nothing | L |
| `advance_conflict_ticks` zero-guard + change-only log | unit | ticks=0→no log entry; unchanged entities not in tick_log | L |
| `recompute_*` Fresh + validation-armed contract | unit | each marks own kind Fresh + `validation_dirty_since.is_some()` + report present | L |

### 2j. `viewer/src` (14 gaps)
| Target | type | What to assert | Pri |
|---|---|---|---|
| `sector_view.rs remove_selected_system` | unit | cascade-removes routes AND scrubs system from `faction.system_presence` + its worlds from `world_presence`; selection cleared; dirty | **H** |
| `sector_view.rs add_route_between` | unit | dedup on `route_id(from,to)` (incl. reversed pair); distance recomputed from coords (≠ default 1); selects new; dirty | **H** |
| `system_view.rs remove_planet_from_system` | unit | scrubs world from `faction.world_presence`; resets selection; dirty; not-found→no-op+status, NOT dirty | **H** |
| `dialogs.rs refresh_manifest_counts` | unit | rewrites `sector.id`+`manifest.project_id`+recounts systems/worlds/routes from live vecs (save-as rename) | **H** |
| `file_ops.rs save_project_sector` name guard | unit | `""`/`"a/b"`/`".."`→`Err(InvalidProjectName)` "forbidden"; happy path writes parseable JSON (needs serial CWD guard) | M |
| `data_editor.rs DataEditor::{load,save}` | rt | load→mutate→save→reload persists; dirty transitions; save-without-load→Err "no worlds.toml loaded" | M |
| `sector_view.rs mark_live_sector_dirty` | unit | sequential reindex: `sector_selected` + `View::System.system_id` remapped via sys_map (not dangling); stable mode unchanged | M |
| `system_view.rs add_planet_to_system` | unit | index=max+1; name `Planet {n}`; inherits star colour; starless→White; unknown→None+status | M |
| `app/mod.rs sync_derived_sector` auto_save | unit | `auto_save&&dirty&&loaded_from` → writes file + clears dirty; `auto_save=false` → not written, dirty stays | M |
| `editor/state.rs set_sector`/`mark_dirty` | unit | `set_sector` clears dirty, bumps revision, drops map_cache, resets selection/tab/dialog; `mark_dirty` sets dirty+revision+drops cache | M |
| `list_projects`+`load_project_sector` | rt | discovery filters dirs w/ out/sector.json, sorted, skips others; save→load round-trip (needs CWD guard) | L |
| `extract_world_data_dir` | unit | valid `[inputs]`→Ok; missing key/garbage/empty→Err(Toml) | L |
| `world_panel.rs parse_variant` | unit | ∀v in `T::VARIANTS`: Display round-trips; garbage→fallback unchanged | L |
| `routes_panel.rs route_*_from_str/str` | rt | 4 stability strings round-trip; unknown→None; `route_type` keys round-trip | L |
| `editor/state.rs next_system_index` | unit | empty→1; systems at 1,3→4; zero systems→1 | L |

### 2k. `gui-core/src` (11 gaps)
| Target | type | What to assert | Pri |
|---|---|---|---|
| `palette.rs top_route_control` | unit | empty→None; single→`(faction,kind,score)`; tie `(interdiction:50,patrol:50)`→Interdiction (strict `>`, probed first); cross-control max wins; raw score returned (no 40 threshold). **Use `faction_id()` newtype** | **H** |
| `info_panel/history.rs event_mentions_world/system` | unit | World-anchor match; Route-anchor matches **both** endpoints; World-anchor→system surfacing; entities-list fallback w/ kind check | **H** |
| `lib.rs human_bytes` | unit | 1023→"1023 B"; 1024→"1.0 KiB"; 1048575→"1024.0 KiB"; integer-`B` vs `{:.1}` split | M |
| `map_theme.rs from_map_theme` `..base` preservation | unit | carried: bg from data; **preserved-from-default**: selection, path_highlight, region_*, star_radius_mul == base | M |
| `palette.rs validation_color` | unit | (0,0)→dim; (0,3)→warning; (2,0)→danger; (2,5)→danger (errors mask warnings) | M |
| `sector_view/render.rs blend_heat` | unit | t=0→`from` exactly; t=1→eff 0.85→channel 85; t=0.001→floor lifts to 20; monotone | M |
| `pick_hex`/`hit_system` | unit | center→correct coord; beyond radius `hex_size*0.95`→None; nearest-wins; off-grid→None | M |
| label truncation (`short`/`short_upper`/`region_label_text`) | unit | len==max unchanged; len>max→take(max-1)+`.`; multibyte `héllo`→6 chars no panic; upper variants uppercase | M |
| `distance_to_segment` vs `point_segment_distance` | proptest | random p,a,b: `|d1-d2|<1e-3`; degenerate a==b→`(p-a).length()` (duplication-drift guard) | L |
| `world_type_color`/`star_color` | unit | `star_color` trim+uppercase, unknown→grey; `world_type_color` case-sensitive exact, `forgeworld`→grey (asymmetry) | L |
| `route_color`/`region_color`/`ScaledSize::px` | unit | 4 tiers→fields, `_`→route_unstable (pin via Unstable, non_exhaustive); 8 region variants distinct; `px` floor `max(hex*mul,min)` | L |

---

## 3. Cross-Cutting Opportunities

### Property-based invariants (highest cross-cutting value)
- **Monotonicity** — *the* signature property here. (a) `distance_base_level` over `(d1<=d2, max)`; (b) end-to-end route stability post-composition (§2c row 1) — the single most valuable test in the audit; (c) `degrade`/`stability_level` ladders are non-decreasing.
- **Determinism / idempotence** — (a) `stage_rng` same-key→identical stream, distinct-key→decorrelated; (b) `validate_sector` run twice → byte-identical report (low-value, transitively safe); (c) all command apply→revert→re-apply byte-stability; (d) heatmap-on render ×2 byte-identical (HashMap-iteration trap).
- **Round-trip** — (a) GeneratedSector serde idempotence (top-10); (b) `SectorSave`, `PresetMeta`, `SegmentumFile`, `FactionDef`-with-aliases; (c) `stability_from_level`↔`stability_level`; (d) `parse_variant`/`route_*_from_str` Display round-trips against `T::VARIANTS`.
- **Total-function panic-freedom** — `parse_hex_rgb` over arbitrary unicode `\PC*` (the actual invariant commit ab6d2dc established; the `&t[0..2]` mid-char slice was a panic=abort crash). Existing test only has 2 fixed multibyte literals.

### Regression tests for recent fixes (commit-anchored)
- **30b8c4f** (square geometry): `GEN_SECTOR_NOT_SQUARE` fires (top-10); CLI `resolve_size` w≠h guard; the "rule lives only in pre-gen validator, NOT `generate`" boundary (`generate_sector` accepts non-square, `validate_project` rejects).
- **ab6d2dc** (harden malformed/oversized input): `MAX_SECTOR_DIM` boundary 1024/1025; `GEN_SECTOR_TOO_LARGE`; `place_systems` i32-overflow arithmetic; `parse_hex_rgb` panic-freedom proptest.
- **296bb80** (generate-all): full `run_generate_all` integration suite — partial-failure isolation, exit codes, empty-dir success, direct-child-only discovery.

### Doctest coverage
The audit found zero doctests. Highest-value `///` examples for `pub`/`pub(crate)` API: `weighted_index` (show the error contract), `derive_stage_seed`/`stage_rng` (keying semantics), `resolve_formats` (the json-non-excludable rule), `stability_from_level`/`stability_level` (the Ord-substitute pair), `RenderMapTheme::from_map_theme` (`..base` preservation). These double as runnable spec for the contracts above.

---

## 4. Completeness Critique — areas warranting a follow-up pass

This audit is deep on `validation.rs`, `rng.rs`, the route layers, `analysis`, `export`, `cli`, and builder/viewer state. Likely **under-represented** surfaces:

1. **`src/gen/generation/mod.rs` — the orchestration spine.** Only the empty-faction path was flagged. The full `generate_with_progress_and_cancel` pipeline (the **cancellation** token path, progress callbacks, stage ordering at mod.rs:418/484/538) has no apply-level test. The cancel→`GenerationCancelled`→exit-130 chain is asserted nowhere end-to-end.
2. **`src/export/segmentum.rs` composition.** `segmentum_tests.rs` builds `SegmentumFile` programmatically; the actual multi-child stitch (the `#[ignore] segmentum` full-m42 test is slow-gated and may not run in CI), `stitch_seed_hash` propagation across children, and grid-placement of child sectors deserve their own audit.
3. **`src/analysis/briefing.rs` + chronicle/history/hooks generation.** Only `parse_preset` (CLI arm) was touched. The 6 briefing presets' *content* derivation, `chronicle` event generation determinism, and hook/site/mission derivations are largely unexamined here despite feeding the builder's undoable regenerate.
4. **`src/export/subsectors/` partitioning.** Referenced only as a render input. The subsector-assignment algorithm itself (which hexes/systems land in which subsector, determinism of that partition) was not audited.
5. **`gui-core` interaction wiring beyond pure helpers.** The audit (correctly, per scope) excluded live-render snapshots, but the *non-visual* `Response`/click-dispatch logic in `sector_view/view.rs` (which calls the now-flagged `pick_hex`/`distance_to_segment`) — i.e. which entity a click actually selects given a `Response` — is logic-testable and unmapped.
6. **Builder panel `show` functions.** The audit covers `BuilderState`/`BuilderCommand` thoroughly but not the panels that call them. Panel-level logic (form validation, derived-field display, command dispatch on widget events) under `builder/src/builder/panels/` is a large untouched surface — though much is genuinely UI and out of scope.
7. **Concurrency edges in derivations.** The background-job audit is good, but the **fingerprint-changed-during-flight discard** path (`background_drain_discards_result_when_fingerprint_changed` exists but multi-kind discard, and the race between `dispatch` and an intervening edit) could use property-style coverage.

A focused second pass on (1) generation orchestration + cancellation and (2) segmentum/subsector partitioning would close the biggest remaining blind spots, as both are core deterministic pipelines with thin coverage.
