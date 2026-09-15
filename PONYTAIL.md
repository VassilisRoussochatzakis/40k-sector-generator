# PONYTAIL.md — over-engineering audit

Two rounds. **Round 1 is applied and closed** (74/80, four commits on 2026-07-23).
**Round 2 is the current backlog** — repo-wide re-audit on 2026-07-25, nothing applied.

**Legend:** `delete` dead code · `stdlib` std ships it · `native` an existing dep/crate layer ships it · `yagni` unused flexibility · `shrink` same logic, fewer lines.
⚠g = golden- or map-snapshot-sensitive: after applying, run `cargo test --test it -- golden` **without** any `UPDATE_*` env vars and require zero diffs. Snapshot-suite items additionally need the gui-core map snapshots run unblessed.

---

## Round 1 (2026-07-19) — applied

80 findings, 74 fully applied across `677ee9c` (manifests/CI), `5f2756e` (root crate), `8605d7f` (builder + gui-core), `7cb4411` (viewer); docs synced in `530e1a4` and again in this round. Every one of the 80 was re-verified symbol-by-symbol on 2026-07-25.

Six did not fully land:

| # | What | State |
|---|---|---|
| 2 | taxonomy parse tables | **partial** — the three `parse_*_variant` fns are `VARIANTS`-derived, but `star_colour_variant_name` (`src/model/taxonomy.rs:23`) is still a hand-written 7-arm table. Carried forward as R2-M8. |
| 9 | `Option<FactionId>` dropdowns | **partial** — all three now call `widgets::enum_combo`, but three near-identical `optional_faction_combo` wrappers remain. Carried forward as R2-P10. |
| 35 | `population_rank`/`tech_rank` | **half**, as predicted — the export copy matches on the enum; `src/analysis/history/subsectors.rs:176` still string-matches, and there are still two copies. Carried forward as R2-A18. |
| 59 | `log` + `env_logger` | **deferred by design** — kept deliberately by a prior review; cutting loses `RUST_LOG` filtering. Still awaiting an explicit decision. Do not bundle silently. |
| 6, 33 | `into_state` / `load_session` | applied, but `load_session`'s removal orphaned `save_session`. Carried forward as R2-B5. |

Two Round-1 cuts had consequences worth knowing: deleting `load_session` (#33) left `save_session` writing a `.sgforge` file nothing can read, and dropping the CI `fuzz-build` job (#31) left `fuzz/` with nothing that ever compiles it.

---

## Round 2 (2026-07-25)

Method: 8 parallel area auditors over the whole tree, each briefed with the determinism invariants and required to prove every "zero callers / byte-identical / never constructed" claim by grep, diff, ablation compile, or a scratch probe test. Load-bearing delete claims were re-checked independently in the main thread.

### How to apply

- Line numbers were correct on 2026-07-25 and will drift — **re-locate by symbol, not by line**.
- Work by area; dispatch one subagent per independent area (CLAUDE.md routing). After each batch: `cargo check --workspace`, then tests via `test-runner`.
- Re-verify a finding's premise before cutting. The audit was careful, but a second grep is cheaper than a revert.
- **Determinism invariants apply to every edit:** BTree ordering for anything reaching output; all RNG through the stage-keyed RNG in `src/model/rng.rs`; builder document mutations through the command bus (transient UI state is exempt).
- Sectors must stay square (`sector_width == sector_height`).
- Update GUIDE.md / docs/MAP.md for any cut that removes something they document. Commit per batch directly on main.

### Decide these first — they are scope calls, not mechanical cuts

Roughly 2,700 of the total sits behind five judgment calls. Nothing else in Round 2 depends on them.

- [ ] **D1.** `delete` **viewer feature removal (~1,490).** Three viewer surfaces duplicate a builder tab that does the job better: faction **DESIGNER** mode (~800 — authors a *new* `factions.toml` at a picked path, outside the viewer's documented in-place-edit scope, and cannot express style/legend fields the builder's 1,224-line FACTIONS panel edits); the editor **GENERATION** tab + its whole debounced live-preview apparatus (~540 — `PreviewJobResult` is variant-for-variant the builder's, and `apply_preview_result` mirrors `PreviewState::apply_result` line for line); the editor **WISHES** tab (~150 — its "+ ADD CONSTRAINT" can only ever push `Constraint::RouteGraphConnected`, there is no kind picker). These are user-facing features. Removing them is defensible but it is your call, not the auditor's. `viewer/src/factions_overview.rs`, `viewer/src/editor/generation_panel.rs`, `viewer/src/editor/wishes_panel.rs`
- [ ] **D2.** `yagni` **§39 LD3 off-thread background derivations (~450 prod + ~250 test).** `ensure_fresh` already recomputes the visible tab synchronously each frame; this machinery exists only to pre-warm tabs the user has not opened, via a 7-arm `spawn_job` dispatch whose "job types" are each one `derive_with` call, plus a 7-variant payload enum, a `FromJobPanic` impl that can only return `None`, and a fingerprint stale-guard. Exactly one production caller. **Spec'd as §39 LD3 in `docs/BUILDER_REQS.txt`** — deleting it drops a spec'd requirement. `builder/src/builder/derivation_jobs.rs`, `state/derivations.rs:284,454,532`, `app.rs:155-156`
- [ ] **D3.** `yagni` **13 narrow field-setter `BuilderCommand` variants (~375 prod + ~325 test).** `SetArchetype`/`SetOrbitalAssets`/`SetBlockadeReport`/`SetSystemConflict`/`SetStar`/`SetStarSpectral`/`SetSurfaceRegions`/`SetWorldConflict`/`SetWorldStability`/`RenameWorld`/`SetWorldOrbit`/`SetRegionKind`/`RenameRegion` are mechanically special cases of `EditSystem`/`EditWorld`/`EditRegion`, which already whole-entity-clone. Each costs a decl + apply arm + revert arm + `dep_classes` line + a round-trip test. Sweeping rather than proportionate, and it grows each undo entry's `before` from one field to a boxed entity. `builder/src/builder/command.rs:162-306,474-491,621-844,1026-1188`
- [ ] **D4.** `shrink` **`worlds_enum!` macro (~390).** `src/worlds.rs` keeps each of 9 enums' variant lists three times (decl + `VARIANTS` + `display_name`). Collapsing them needs a **new** macro beside `enum_slug!`. Real duplication, but a macro rewrite. ⚠g `src/worlds.rs:653`
- [ ] **D5.** `delete` **orphaned `fuzz/` tree (~150).** Round 1 dropped the CI `fuzz-build` job, so nothing now compiles the 6 targets + 70-line manifest; they are nightly-only and will rot silently. Either restore a gating job or delete the tree. `fuzz/`

Two more that trade duplication for a new abstraction — cheaper calls, same shape:

- [ ] **D6.** `shrink` 7 copies of `show_save_row` → one `catalog_save_row` in `panels/mod.rs` (~94). Trades 7 duplicated ~25-line blocks for 1 new ~25-line helper + 7 short calls. `builder/.../panels/{relations,hooks,personae,prose,sites,missions,history}.rs`
- [ ] **D7.** `native` **49 hand-written `impl Display { f.write_str(self.as_slug()) }`** blocks, one per `enum_slug!` invocation, all byte-identical — emit `Display` from the *existing* macro (~95 in scope, ~250 repo-wide). Extending a macro you already have, not adding one. `src/macros.rs:16` + 19 sites

---

### src/model + src/gen — net −861

- [ ] **M1.** `shrink` ⚠g `apply_route_effects_with_progress`'s 5-arm condition match — `EmpyricBleed` is a byte-copy of `Turbulence`, `BeaconChain` of `CalmCorridor`, and all five hardcoded tags equal `format!("region:{}", cond.as_slug())`. One `match cond -> Option<RouteStability>` + one slug-derived call site; `apply_route_stability_with_bridge_progress` then inlines. Route-safety monotonicity preserved. `src/gen/regions.rs:595-687` (~65)
- [ ] **M2.** `shrink` ⚠g The 8× "stage into `BTreeMap<SystemId, T>`, then re-walk `&mut sector.systems` to write back" ceremony in the archetype `apply_*` fns — every rule is strictly per-system. **Ceremony only; the eight bespoke bodies stay** (a parameterised collapse was tried and comes out longer). `src/gen/archetypes.rs:208,241,287,340,370,412,441,464` (~50)
- [ ] **M3.** `delete` ⚠g `ConditionEntry::label` — write-only. `build_regions_reroll` names regions itself and never reads it; the shipped fixture's 8 custom labels are provably dropped. `src/gen/regions.rs:199-201` (~35, 4 files)
- [ ] **M4.** `yagni` ⚠g `faction_style_rgb_with_overrides`'s string round-trip for `border_override` — its one real caller stringifies a typed `FactionBorder` just so the fn can re-parse it. Take `Option<FactionBorder>`; deletes `parse_border` and `FactionBorder::as_str`. The other caller passes all four overrides as `None`, i.e. is `faction_style_rgb(..)`. `src/gen/faction_style.rs:219-229,40-48,233-257` (~35)
- [ ] **M5.** `shrink` ⚠g The two narrow route-distance refresh blocks in `move_system`/`swap_systems` — byte-identical collect-and-write-back of what `recompute_route_distances` does 20 lines later. `src/model/sector_model/mutation.rs:146-161,228-247` (~34)
- [ ] **M6.** `yagni` `calm_corridor_max_distance` + its whole `mod tests` — a one-caller wrapper whose body is `config_max.max(rules_max)`, with 19 lines of test proving `u32::max` works. `src/gen/generation/mod.rs:275,1028` (~30)
- [ ] **M7.** `yagni` `generate_random_sector` — top of a 3-deep wrapper chain with zero production callers; the CLI enters at `_from`, the builder at `_from_with_progress`. `src/gen/random_sector.rs:522-550` (~29)
- [ ] **M8.** `shrink` ⚠g `star_colour_variant_name` + the four `parse_*_variant` fns — the 7-arm table returns exactly what derived `Debug` prints, and with it gone all four parsers share one body. *(Round-1 #2 residual.)* `src/model/taxonomy.rs:23-61` (~30)
- [ ] **M9.** `delete` `GeneratedSector::add_faction` + the `MutationError::DuplicateFaction` variant it solely constructs — zero production callers. `src/model/sector_model/mutation.rs:347-371,40-41` (~27)
- [ ] **M10.** `shrink` 12 byte-identical two-line `INVARIANT (§R4)` comment blocks stapled to individual structs; the module doc already states it verbatim and lists the same structs. `src/model/sector_model/mod.rs` ×12 (~24)
- [ ] **M11.** `shrink` `PowerProfile::total_projection` / `PresenceDimensions::local_control_score` — 9-term dot products written as 20-line right-nested `mul_add` towers. Sequential `mul_add` statements: same ops, same order, bit-identical. `src/model/sector_model/mod.rs:1421-1444,1197-1218` (~24)
- [ ] **M12.** `shrink` 7 identical 5-line `progress(RandomProgress {..})` blocks → a free `fn emit(..)` (free fn, not a closure — a closure conflicts with the `&mut dyn FnMut` reborrow). `src/gen/random_sector.rs:603,612,658,664,670,676,682` (~23)
- [ ] **M13.** `yagni` `HiddenPresence` — a 2-variant enum + `from_flag` + `is_present` wrapping one `bool`, one construction, two reads. Style call: `JsonFormat::from_flag` is the same house pattern. `src/gen/sites.rs:375-395` (~21)
- [ ] **M14.** `delete` `build_system_with_bias` — dead middle hop in a 3-deep chain, `pub` inside a private module. `src/gen/generation/systems.rs:42` (~20)
- [ ] **M15.** `shrink` `region_bias`'s six arms — each repeats the same `let mut b; if id_has {b += k}; b` dance. Same pass: `KnightHousehold` and `Other` are unreachable from the only caller. `src/gen/surface_region.rs:178-244` (~20)
- [ ] **M16.** `yagni` `WorldGenParams` — a 10-field context struct with one construction and one destructure, built then immediately unpacked. `src/gen/generation/world_placement.rs:24` (~18)
- [ ] **M17.** `delete` Three never-instantiated `define_id!` impls — inherent `is_empty` (shadowed by `Deref<Target = str>`), `From<&String>`, `PartialEq<$name> for str`; each emitted 7×. *Proven by ablation compile.* `src/model/ids.rs:46-49,89-93,125-129` (~17)
- [ ] **M18.** `native` ⚠g The hand-rolled recursive `find`/`union` in the routes stage — `src/gen/routes.rs:224` already ships `union` + `find_root` over the same parent array. Compression vs halving never changes which root `find` returns. `src/gen/generation/routes.rs:372` (~16)
- [ ] **M19.** `shrink` Two redundant `weighted_index` tests re-asserting what a third already covers. `src/model/rng.rs:174-183,277-287` (~16)
- [ ] **M20.** `shrink` ⚠g `pick_features`' twin drain loops — the 3-tier loop and the `key_table_features` block are the same weighted drain. Normalise all four tiers to `Vec<(NotableFeature, f64)>`; draw-identical. `src/gen/generation/world_placement.rs:219-259` (~14)
- [ ] **M21.** `shrink` `ensure_connected_routes` / `route_component_count` build the same DSU preamble. `src/gen/routes.rs:112-128,193-206` (~14)
- [ ] **M22.** `delete` 11 identity arms in `legacy_top_faction_id` (`"ork" => "ork".to_string()`) — all exactly what the `_` fallback returns. `src/gen/factions.rs:105,126-136` (~12)
- [ ] **M23.** `shrink` ⚠g The four `assets.push(OrbitalAsset {..})` blocks in `derive_orbital_assets`. Keep the slug literals explicit — `"defense"`/`"blockade"` ≠ the variants' `as_slug()`, so deriving them shifts asset ids. `src/gen/orbital_assets.rs:131-198` (~12)
- [ ] **M24.** `stdlib` `ordered_pair` / `order_pair` — the same `if a <= b` swap in two files; both become one line via `Ord::min`/`max`. `src/gen/routes.rs:216-222`, `src/gen/hidden_routes.rs:518-524` (~12)
- [ ] **M25.** `native` ⚠g `spectral_type_fallback` — a 7-arm match returning exactly `StarColour::code()`, arm for arm. *(Side note: the emitted `spectral_type` field is therefore always byte-equal to `colour_code`; removing the field itself would move goldens.)* `src/gen/generation/systems.rs:192` (~11)
- [ ] **M26.** `yagni` `assign_factions_inner` + the triple-redundant empty guard firing three times on one path. `src/gen/generation/factions.rs:71` (~11)
- [ ] **M27.** `shrink` The byte-identical 11-line coord bounds-check in `add_system_full` and `move_system`. Square invariant untouched. `src/model/sector_model/mutation.rs:72-82,125-135` (~11)
- [ ] **M28.** `shrink` ⚠g `aggregate_factions`' three `contains` guards — every vector is unconditionally `sort()`+`dedup()`ed 25 lines later, so the O(n²) guard is redundant. `src/gen/generation/factions.rs:441-484` (~9)
- [ ] **M29.** `stdlib` ⚠g `pick_region_dominant`'s hand-rolled max → `min_by(|a,b| b.1.total_cmp(&a.1))`, which preserves the existing first-wins tie-break exactly. `src/gen/surface_region.rs:149-176` (~9)
- [ ] **M30.** `yagni` ⚠g `RegionKnobs` — private two-`u32` struct, one construction, one read. `src/gen/random_sector.rs:158-163,323,466-472` (~9)
- [ ] **M31.** `yagni` `emit_layer`'s two redundant string params — all three call sites pass the same literal twice, and `hidden_suffix(kind)` in the same file already maps each `RouteType` to it. `src/gen/hidden_routes.rs:304-335` (~8)
- [ ] **M32.** `stdlib` `dominant_route_condition` hand-rolls a max over a 2-element array. `src/gen/regions.rs:535-551` (~8)
- [ ] **M33.** `shrink` `influence_rank` cloned verbatim into the integration suite. Make the generation one `pub(crate)`. `src/gen/generation/factions.rs:383` (~8)
- [ ] **M34.** `native` `RouteType::from_key` — a 10-arm slug→variant table restating what `enum_slug!` generated 50 lines above. `src/model/sector_model/mod.rs:655-666` (~8)
- [ ] **M35.** `native` `GeneratedSector::get_world` — a nested double `for` duplicating the `all_worlds()` iterator two lines below. (Outright deletion also defensible: all 12 call sites are tests.) `src/model/sector_model/mod.rs:438-448` (~8)
- [ ] **M36.** `yagni` Two stacked `#[cfg(test)]` hidden-route wrappers, only the innermost of which has a production caller. `src/gen/hidden_routes.rs:248-274` (~8)
- [ ] **M37.** `yagni` `regions::build_regions` — a one-caller wrapper whose one caller is itself a wrapper. `src/gen/regions.rs:253-258` (~7)
- [ ] **M38.** `delete` `RerollNonces::get` — `pub` API, zero production callers. `src/gen/generation/reroll.rs:99` (~7)
- [ ] **M39.** `delete` `LocationNamesTable::reuse_workbook_location_names` — a config knob nothing reads, plus the `default_true` fn that exists only for it. Delete the Rust field only; leave the 5 TOML lines (serde ignores them) to avoid an `input_digests` golden re-pin. `src/gen/names.rs:27-28,36,42-44` (~7)
- [ ] **M40.** `yagni` ⚠g `SectorSize::dim()` — two `pub` accessors for one value; `dim()`'s only caller is `dims()`. `src/gen/random_sector.rs:84-105` (~7)
- [ ] **M41.** `delete` `RegionRouteEffectsProgress::Completed` — emitted twice, and the sole consumer discards it because the same summary is the function's return value. `src/gen/regions.rs:526-528` (~6)
- [ ] **M42.** `delete` `generate_routes`' `rng` param and the `route_rng` feeding it — the only two mentions of `rng` in a 986-line file are the signature and `let _ = rng; // reserved for future stochastic edges`. Zero draw impact. `src/gen/generation/routes.rs:43`, `mod.rs:511` (~6)
- [ ] **M43.** `delete` The unreachable `system_count > grid cells` branch — `target` is `min(total_cells)` one line earlier. *(Worth knowing: an over-sized `system_count` is silently clamped, not rejected.)* `src/gen/generation/placement.rs:32` (~6)
- [ ] **M44.** `delete` `impl From<&String> for FactionKind` — nothing converts a `&String`. `src/model/sector_model/mod.rs:947-951` (~5)
- [ ] **M45.** `yagni` `RandomPhase::index()` — `pub`, sole caller is the private fn two lines below. `src/gen/random_sector.rs:203-207` (~5)
- [ ] **M46.** `delete` `let _invariants = crate::validate_sector(&sector);` + its 3-line comment — binds a full report and drops it. `src/gen/random_sector.rs:643-655` (~4)

### src/analysis + src/validate — net −1,440

- [ ] **A1.** `shrink` ⚠g `search::evaluate`'s 25 near-identical `Constraint` arms — each is "count → compare → build `ConstraintReport`" with the same 4 fields and the same `(min-n).max(0.0)` miss formula. Four builders; only the 3 graph-shaped arms stay inline. Format specifiers must be preserved verbatim. `src/analysis/search.rs:667` (~170)
- [ ] **A2.** `delete` `ValidationCode` enum + `enum_slug!` + `ALL` + `code_registry_tests` — a half-built registry covering 15 of ~30 codes, each variant spelled 4× to serve 1 call site. Zero hits outside `validation.rs`. `src/validate/validation.rs:676-799` (~125, 20% of the file)
- [ ] **A3.** `delete` `SystemStateName` + `StanceName` mirror enums — variant-for-variant identical to `SystemState` and `Stance`, same snake_case serde, same slugs. `src/analysis/search.rs:206,280` (~75 + 2 macro parity rows)
- [ ] **A4.** `shrink` `InfluenceFieldProgress` — a 1:1 mirror of `SectorProgress::InfluenceField*` whose sole consumer is a 41-line relabelling match. Emit `SectorProgress::*` directly. `src/analysis/influence_field.rs:56` + `src/gen/generation/mod.rs:806` (~68)
- [ ] **A5.** `native` Builder re-implements `kind_slug` + `event_kind_from_str` verbatim because both are `pub(super)`. Widen to `pub`, delete the copies. **The copies have already drifted — see Bug 2 below.** `src/analysis/history/labels.rs:7`, `rules.rs:62` (~65)
- [ ] **A6.** `shrink` Four parallel `match canonical_kind(kind)` tables over the same 17 keys → one `const KIND_TABLE` + `find`. `src/analysis/personae.rs:614,910,932,1009` (~60)
- [ ] **A7.** `delete` ⚠g `economy/tables.rs` arms keyed on strings no enum can produce — `MiningWorld`/`KnightWorld`/`CivilisedWorld`/`FortressWorld`/`AgriculturalWorld`/`QuarantinedWorld` aren't `WorldType` variants; `STC`/`Archeotech`/`Imperial`/`Mechanicus`/`PreImperial`/`Industrial`/`Renaissance`/`Medieval`/`Iron`/`Stone` aren't `TechLevel` (that fn is `if tech=="Primitive" {0.2} else {1.0}`). Two `_ => default_*` fallbacks are unreachable. *Shipped `presets/*/economy.toml` `[economy.by_population]` keys are already no-ops.* `src/analysis/economy/tables.rs:10,114,126,246,258` (~55)
- [ ] **A8.** `shrink` Five copies of the same `entry().or_insert(0) += 1` tally → one `tally(..) -> BTreeMap<Arc<str>, u32>`; output byte-identical. `src/analysis/analytics.rs:205` (~46→20)
- [ ] **A9.** `shrink` ⚠g Three near-identical "cap + re-sort + flush buffer" tails in the sibling chroniclers, plus the same tuple type declared in three files. Care: `routes.rs` truncates without a topo tiebreak, so unifying can reorder tied route events. `src/analysis/history/{systems,worlds,routes}.rs` (~45→20)
- [ ] **A10.** `shrink` ⚠g Five copies of "index both sides by id → `BTreeSet` key union → 4-arm match with a no-op `(None,None)`" → one `merge_by_id`. Must keep the sorted-key union. `src/validate/diff.rs:328,421,519,672,714` (~42)
- [ ] **A11.** `delete` Eight `impl Display` blocks nothing ever formats — every consumer calls `.as_slug()` or a label table. `src/analysis/search.rs:228,262,304,341`, `hooks.rs:148`, `missions.rs:124,145,168` (~40)
- [ ] **A12.** `shrink` 20 hand-written 6-field `ValidationIssue{..}` literals alongside an `issue()` helper that only covers the path-less case. Add `issue_at`. `src/validate/validation.rs:265+` (~35)
- [ ] **A13.** `native` ⚠g `navigable_component_count` + `find_root` re-implement `routes::route_component_count`; the two `find_root` bodies are byte-identical. `src/validate/invariants.rs:242-284` (~35)
- [ ] **A14.** `stdlib` ⚠g `insert_top_n`'s bounded-buffer scan — `slots` is fully materialized and the vec is sorted 5 lines later. **Not output-neutral:** it replaces the *first* max on ties, so near-miss lists shift. `src/analysis/search.rs:1251` (~31)
- [ ] **A15.** `shrink` `derive_observer_view` / `derive_world_observer_view` — the same 30 lines twice, differing only in whether control comes from `sys` or `w`. The world case passes `slice::from_ref(w)`. `src/analysis/intel.rs:158,200` (~28)
- [ ] **A16.** `delete` ⚠g `ValidationIssue.row` — written `None` 17×, `Some` **zero** times. No path produces a row number, so three builder render branches are permanently dead. Drops a `"row": null` key from `validate --json`; not in any checked-in golden. `src/validate/validation.rs:24` + `builder/.../panels/validation.rs:86,206,305,457` (~25)
- [ ] **A17.** `shrink` `RuleIndex` interns kind/disposition strings into `FxMap<&str,u32>` purely to build `(u32,u32)` keys, then does two `?`-chained lookups to get back. Key on `(&str,&str)` directly (lookup-only, never iterated for output). *5 presets ship `kind_rules`; zero ship `disposition_rules`.* `src/analysis/relations/derive.rs:139` (~25)
- [ ] **A18.** `native` ⚠g History's stringly `population_rank`/`tech_rank` duplicate the typed pair, with 6 arms matching strings no enum emits. *(Round-1 #35 residual.)* `src/analysis/history/subsectors.rs:176` (~25)
- [ ] **A19.** `shrink` `check_world_control` / `check_system_control` — the same 18-line closure twice, differing only in code string and path prefix. `src/validate/invariants.rs:650-721` (~25)
- [ ] **A20.** `delete` `default_title` is unreachable in production — every `default_pool` arm, including the `_` placeholder, supplies ≥2 titles. Use `.unwrap_or_default()`. `src/analysis/personae.rs:932` (~25)
- [ ] **A21.** `shrink` ⚠g `apply_world_type_rule` + `apply_feature_rule` spell out the same 10 score fields by hand, twice, when `StrategicOutput::fields_mut()` owns the canonical list. `src/analysis/economy/tables.rs:298` (~21)
- [ ] **A22.** `delete` `BriefingProfile::restrict_to_factions` — a knob nothing sets. No `AudiencePreset` arm, no CLI flag, no builder path, never deserialized. `src/analysis/briefing.rs:60,94,253-265` (~20)
- [ ] **A23.** `shrink` `merge_with_defaults` — the same 3-line `if user.X.is_empty()` six times. `src/analysis/personae.rs:973` (~20)
- [ ] **A24.** `yagni` `DiffConfig::min_economy_delta` — never set independently; the builder literally assigns `min_faction_delta` into it, no `.toml` carries the key, and its only test asserts the two are equal. `src/validate/diff.rs:51,63,384,1452` (~20)
- [ ] **A25.** `shrink` `history/context.rs` — a whole file for a 5-field borrow struct with no behaviour (20 lines, 11 of them doc/imports). Move into `build.rs`. `src/analysis/history/context.rs:9` (~20, deletes a file)
- [ ] **A26.** `stdlib` `RelationAttitude::level()` and `max_attitude` reinvent the derived `Ord` — declaration order matches `level()`'s `-2..=3` exactly. `src/analysis/relations/config.rs:274`, `derive.rs:576` (~17)
- [ ] **A27.** `shrink` The `CellsResolved` progress emit is copy-pasted three times in one loop body solely because two `continue` paths precede the third. `src/analysis/influence_field.rs:210,228,252` (~16)
- [ ] **A28.** `shrink` ⚠g `merge_endpoints` hand-writes accumulate-then-divide over 4 fields in two guarded branches, but a zero-`count` `Aggregate` is already `Default`. `src/analysis/route_control.rs:77` (~15)
- [ ] **A29.** `shrink` ⚠g Economy-derive bookkeeping that buys nothing: `world_counts` feeds a param the callee re-`.max(1)`s and can read off `sys_ref`; `system_refs` is built twice from an identical expression; `stranded_world_idx` collects indices for a second pass no borrow requires. `src/analysis/economy/derive.rs:173,219,224` (~15)
- [ ] **A30.** `native` `history_progress_stride` is byte-identical to `progress_stride`, and `should_report_history_progress` is the fourth copy of the same guard. Hoist one. `src/analysis/history/progress.rs:41` (~15 here, ~45 repo-wide)
- [ ] **A31.** `shrink` `derive_world_stability` resolves "kind of this presence" three times with the same 6-line body. `src/analysis/stability.rs:128` (~14)
- [ ] **A32.** `native` `strategic_value`'s population/tech tables duplicate `population_rank`/`tech_rank` value-for-value. Promote those to `pub(crate)`. (The table must stay — enum declaration order differs, so `as u8` is not a shortcut.) `src/analysis/control.rs:638` (~14)
- [ ] **A33.** `shrink` `route_control` re-implements three helpers `stability.rs` already owns — `clamp`, `endpoint_has_tag`, `kind_of`. `src/analysis/route_control.rs:40,104,231` (~12)
- [ ] **A34.** `delete` ⚠g `HistoryEra::weight` — a knob nothing reads. `synthesise_era` picks the first era whose `allowed_events` matches and never consults it. **The builder ships a `DragValue` tooltipped "How heavily events cluster in this era" that does nothing, and every checked-in `data/history.toml` tunes it (0.8/1.1/1.2/1.4/1.5) under a false impression.** `src/analysis/history/config.rs:77,118` + `builder/.../panels/history.rs:336` (~12 + ~40 preset lines)
- [ ] **A35.** `shrink` ⚠g `build_persona` stringifies the anchor twice; removes two unreachable `PersonaAnchor::Faction` arms. The discriminator feeds `stage_rng`, so names/titles shift. `src/analysis/personae.rs:685,889` (~12)
- [ ] **A36.** `shrink` The field-wise "add two `ResourceVector`s" loop is written three times. Give `ResourceVector` the symmetric `add_assign` — `StrategicOutput` is the precedent. `src/analysis/economy/derive.rs:494` + builder `derivations.rs:613,658` (~12)
- [ ] **A37.** `delete` Four `derive()` forwarders with no production caller — `economy::derive` has **zero** callers anywhere; `prose`/`history` only in their own tests; `personae` only in integration tests. Everything real calls `derive_with`. `src/analysis/{economy/derive.rs:50, prose.rs:132, history/mod.rs:49, personae.rs:260}` (~22)
- [ ] **A38.** `yagni` Five dead params in `hooks.rs` (`_cfg` ×3, plus `_: &GeneratedSector` and `_cfg`), all private with one call site; plus a no-op `let _ = sector;`. `src/analysis/hooks.rs:221,363,509,623` (~11)
- [ ] **A39.** `stdlib` `node_presence` is a doc-commented wrapper around `usize::max`; `pick_str` is a one-caller wrapper whose `unwrap_or_default` fallback is dead. `src/analysis/personae.rs:382,659` (~11)
- [ ] **A40.** `native` `is_hidden_kind`/`is_merchant_kind` — one-line forwarders to `FactionKind::is_hidden()`/`is_merchant()`, whose own doc comments say the logic was already folded upstream. `src/analysis/relations/tables.rs:207,211` (~10)
- [ ] **A41.** `yagni` ⚠g `HistoryEvent::summary` is always `narrative.clone()` — `build.rs:50` is literally `summary: text.clone(), narrative: text`. **Verified over the m42 golden's 427 chronicle events: differences = 0.** `src/analysis/history/model.rs:41` (~10 code, ~427 duplicated strings per golden)
- [ ] **A42.** `yagni` `MetricTarget::ceil`'s `INFINITY` default + the `is_infinite()` soft-decay branch are unreachable — all 32 `band(..)` calls set a finite ceil and nothing deserializes the config. `src/analysis/interestingness.rs:335` (~9)
- [ ] **A43.** `native` Builder's `profile_key` re-matches the 5 strings `enum_slug!(ProfileId)` already generates, character-for-character. `builder/.../panels/interestingness.rs:555` (~9)
- [ ] **A44.** `shrink` `SystemStateParams` exists only to dodge an 8-arg signature on a private one-call-site fn, then unpacks itself field-for-field. `src/analysis/control.rs:545` (~9)
- [ ] **A45.** `native` The `EconomyResourceMin` resource match duplicates `ResourceVector::get` — same six arms, same `_ => 0.0`. `src/analysis/search.rs:1042` (~9)
- [ ] **A46.** `shrink` `prose::sector_overview`'s `faction_names` lookup is a guaranteed no-op — the table is built from the same slice it is then queried with. Byte-identical output; the param becomes removable. `src/analysis/prose.rs:187,196-207` (~8)
- [ ] **A47.** `delete` `Severity::Info` is never constructed — `ValidationReport` has only `errors` + `warnings`, all 42 constructions are `Error`/`Warning`, and it is `Serialize`-only so it cannot arrive from JSON. `src/validate/validation.rs:34,40` (~8, 4 files)
- [ ] **A48.** `shrink` `load_economy_file` / `load_relations_file` — the same nine lines twice. One `load_toml_or_default<T: DeserializeOwned + Default>`. `src/analysis/economy/derive.rs:38`, `relations/derive.rs:652` (~8)
- [ ] **A49.** `shrink` ⚠g Three near-identical `RESOURCE_KEYS.iter().filter(..).collect()` blocks differing only by the comparison. Slice order unchanged, so output stays byte-stable. `src/analysis/economy/derive.rs:101,120,125` (~7)
- [ ] **A50.** `stdlib` `let mut ids: BTreeMap<&str, ()>` built by two push loops is a hand-rolled `BTreeSet` — one line. `src/analysis/route_control.rs:236` (~6)
- [ ] **A51.** `delete` power_projection leftovers: `bfs_distances` takes an unused `_id`; `doctrine_multiplier` is `pub` (re-exported) with zero callers outside its file; `best.as_ref().map(..).unwrap_or(true)` is `is_none_or`. `src/analysis/power_projection.rs:73,112,198` (~6)

### src/export + src/cli + crate roots — net −1,400

*(`worlds_enum!` is D4 above.)*

- [ ] **X1.** `shrink` The `Command` enum's brace-variants + the 245-line `run()` destructure-and-forward → `#[derive(Args)]` structs and 25 one-line arms. `src/cli/mod.rs:488` (~200)
- [ ] **X2.** `delete` ⚠g `KeyTables` + `from_enums()` — 8 maps read only for `.len()`; `key_table_features` round-trips enum→display→parse. **This is the only finding in the audit that can move RNG draw order** — prove `NotableFeature::VARIANTS` order equals the old map order first; ripples through 14 files. `src/worlds.rs:366` (~110 workspace-wide)
- [ ] **X3.** `shrink` ⚠g `impl AsRef<str> for NotableFeature` — 100 arms each returning the bare variant name, i.e. the existing `Display`/`{:?}`. *All 100 extracted and checked: `variant == literal` for every one.* `src/worlds.rs:533` (~100)
- [ ] **X4.** `native` ⚠g Legend twins — `DisplayBucket` destructure ×4, stability tables ×4, control-glyph table ×2, and identical `legend_height` line-count math ×2. Hoist to `render_core::labels`. `src/export/{bitmap,svg_export}/legend.rs` (~110)
- [ ] **X5.** `delete` 7 unused `FromStr` impls (`WorldType`, `Atmosphere`, `Temperature`, `Biosphere`, `Population`, `TechLevel`, `Government`) — no `parse::<T>()` anywhere. `src/worlds.rs:403` (~77)
- [ ] **X6.** `shrink` `empty_manifest()` + `sample_sector()` duplicated verbatim in the two backend test modules (diff: 2 lines). `src/export/{bitmap,svg_export}/tests.rs` (~70)
- [ ] **X7.** `delete` Five crate-root item re-export blocks (`economy`/`history`/`map_theme`/`relations`/`regions`) + the `build_pool`/`apply_authored_features`/`regenerate_world_payload` trio and its 28-line rationale comment — all reachable via the module aliases, zero direct importers. `src/lib.rs:148` (~57)
- [ ] **X8.** `native` ⚠g `hex_center` / `map_bounds` / legend-width table duplicated i32-vs-f32. Shared f32 core in `render_core`; bitmap wraps with `.round() as i32`. `src/export/{bitmap,svg_export}/geom.rs` (~35)
- [ ] **X9.** `native` ⚠g `compute_heat_tints` duplicated verbatim (differs only by an import alias) + `compute_heatmap`'s Off-guard. Move to `render_core::grid`. Lookups only, never iterated — no determinism risk. `src/export/{bitmap,svg_export}/grid.rs` (~30)
- [ ] **X10.** `delete` `pub struct WorldEntry` + `pub struct System` — referenced only by each other. `src/worlds.rs:334` (~27)
- [ ] **X11.** `shrink` ⚠g `render_system_markdown` ≡ `format_system_section` character-for-character except `#` vs `##`. One fn taking the heading level. `src/export/render.rs:178,581` (~25)
- [ ] **X12.** `stdlib` ⚠g `build_faction_palette_json` hand-rolls JSON emission; `serde_json::to_string(&BTreeMap<&str, Entry>)` emits the same bytes (BTreeMap sorts, fields in decl order). `src/export/html_export.rs:192` (~24)
- [ ] **X13.** `delete` 6 write-only subsector fields — `neighboring_subsector_ids`, `star_colour_counts`, `route_type_counts`, `tag_counts`, `world_share_basis_points`, `contested_system_count` — plus the unused `Serialize`/`Deserialize` derives on the 5 subsector types (`Subsector` is absent from every golden JSON and never deserialized). `src/export/subsectors/mod.rs:42` (~20)
- [ ] **X14.** `yagni` `Canvas::rect` — only `svg_export/routes.rs:124` calls it, on a concrete `SvgCanvas`, so `BitmapCanvas::rect` is unreachable; `Canvas::polygon`'s `stroke_w` is `1.0` at its single call site. `src/export/render_core/canvas.rs:28` (~20)
- [ ] **X15.** `shrink` `load_or_regenerate` is `resolve_sector_with_cfg` with `C = ()`. `src/cli/common.rs:244` (~15)
- [ ] **X16.** `shrink` ⚠g The world stability block is 7 consecutive `push_str(&format!(..))` for one paragraph. `src/export/render.rs:819` (~13)
- [ ] **X17.** `delete` `dir_slug` byte-identical across crates. `src/cli/random.rs:158`, `builder/.../panels/mod.rs:43` (~12)
- [ ] **X18.** `native` ⚠g `contrast_text` duplicated (`Rgba<u8>` vs `Color32`); `world_type_color` is the in-repo precedent. **Watch the rounding:** one uses `mul_add`, the other plain `+`, so whichever form wins shifts one suite. `src/export/system_map.rs:384`, `gui-core/src/palette.rs:839` (~12)
- [ ] **X19.** `yagni` `SubsectorConfig::{max_iterations, include_empty_subsectors, faction_control_top_n}` never set outside `Default`, plus the `let use_inhabited = true;` residue threaded through 7 call sites. `src/export/subsectors/mod.rs:109` (~12)
- [ ] **X20.** `shrink` ⚠g `at_war` and `hostile` diplomacy blocks are identical loops. `src/export/render.rs:428` (~11)
- [ ] **X21.** `delete` `GenerationRow::counter` + its `WorldCandidate` mirror — written once, never read; no fixture carries the key. `src/worlds.rs:312` (~10)
- [ ] **X22.** `shrink` `--formats` / `--light` / `--exclude` declared with duplicated doc-comments on two subcommands → one shared `#[derive(Args)]` group. `src/cli/mod.rs:87,150` (~10)
- [ ] **X23.** `yagni` `export_all` forwards to `export_all_with_progress` with a no-op closure; one caller. `src/export/writers.rs:149` (~7)
- [ ] **X24.** `delete` `RenderOptions::faction_fill` — set at 3 sites, read at none (every real read is on `SystemRenderOptions`/`BitmapConfig`). *Re-verified in the main thread.* `src/export/render_core/options.rs:13` (~6)
- [ ] **X25.** `yagni` `WorldsConfig::from_path` — one caller, seven lines below in the same file; all 6 external callers use `from_str`. `src/worlds_toml.rs:113` (~6)
- [ ] **X26.** `delete` `parse_heatmap` has no arm for `ConflictIntensity`, so that mode is **unreachable from the CLI** while listed in `HeatmapMode::ALL`. `src/cli/common.rs:140` (~5)
- [ ] **X27.** `yagni` `derive_interestingness` plain wrapper, one caller (a bench); `_with` has 5. `src/lib.rs:836` (~5)

### builder — panels — net −1,040

*(`show_save_row` is D6 above.)*

- [ ] **P1.** `stdlib` `ensure_{sites,personae,prose,missions,hooks}_catalog_if_needed` + the unreachable `let Some(cfg) = .. else { return }` after all 12 call sites → `get_or_insert_default()` (already the idiom at `routes.rs:841`; MSRV 1.87 ≥ 1.83). Not a bus violation — the direct `data_catalogs` write and its `note_catalog_edit` arming are unchanged. `panels/{sites,personae,prose,missions,hooks}.rs` (~59)
- [ ] **P2.** `shrink` Three hand-rolled "type an id or pick an existing one" widgets → one `roster::pick_or_type`. `panels/missions.rs:866`, `hooks.rs:746` (~55)
- [ ] **P3.** `native` Four hand-written enum→display match tables → `panels::system::pretty_slug(x.as_slug())`. `panels/missions.rs:1032`, `hooks.rs:892` (~51)
- [ ] **P4.** `shrink` The `labeled(..)` + `if combo_enum(..) { changed = true }` block repeated 7× at 16 lines each → one `enum_row(..) -> bool` called with `changed |=`. `panels/world/environment.rs:27` (~45)
- [ ] **P5.** `shrink` 12 hand-rolled read-only `labeled(.., |ui| ui.label(..))` rows in `show_detail_card`, six repeating the same "value, or dimmed —, or joined CSV" body. `panels/sites.rs:454` (~45)
- [ ] **P6.** `shrink` 5-dialog `egui::Window` boilerplate repeated verbatim → one `text_prompt(..) -> (commit, close)`. A 6th identical site sits at `system_map.rs:721`. `panels/map/dialogs.rs:12` (~42)
- [ ] **P7.** `shrink` `show_worlds_link`'s inline add-world block is a byte-for-byte re-implementation of `system_map::add_world_at_orbit`, including identical error strings. `panels/system/mod.rs:309` (~42)
- [ ] **P8.** `yagni` The `AnchorScope` shadow enum + `anchor_scope()` + three copy-pasted `selectable_value`+rebuild blocks — the enum exists solely to drive one combo. `panels/hooks.rs:522` (~40)
- [ ] **P9.** `shrink` Five identical `labeled(ui, L, H, |ui| ui.monospace(role(&summary.F)))` blocks + their `role` closure → one loop over a slice literal. `panels/system/mod.rs:478` (~30)
- [ ] **P10.** `shrink` Three copies of `optional_faction_combo` — `surface_regions.rs` and `orbital.rs` diff as **IDENTICAL**; `conflict.rs` differs only in the label closure. Hoist beside `faction_name` with the formatter as a param. *(Round-1 #9 residual.)* `panels/{surface_regions,orbital,conflict}.rs` (~30)
- [ ] **P11.** `shrink` Four `ui_kit::combo(id, pretty_slug(..)).show_ui(..)` blocks (necron/tyranid/gsc/tau) + the same shape at `identity.rs:82`. `enum_combo` does *not* fit — these are required, non-`Option` values that would gain a bogus `—` row. `panels/system/archetype.rs:77` (~30)
- [ ] **P12.** `native` Hand-rolled `Option<SystemState>` combo + the same 7-variant array literal repeated at `bulk_ops.rs:117` → `widgets::enum_combo` + the shared `SYSTEM_STATES` const. `panels/system/mod.rs:436` (~30)
- [ ] **P13.** `shrink` `CLAIM_TYPES`, `INFLUENCE_TIERS`, `DOMINANCE_STATES` are byte-identical duplicates of the same three consts in `control/mod.rs`. Move to `presence_widgets.rs`. `panels/world/claims.rs:161` (~27)
- [ ] **P14.** `shrink` `anchor_label` and `focus_anchor` match the same 3 `HookAnchor` variants with the same guard twice, plus a one-caller `show_anchor_link` wrapper. `panels/hooks.rs:821` (~28)
- [ ] **P15.** `shrink` 7 copies of the `add_enabled(cond, SelectableLabel) / if clicked / if !cond on_hover_text` triple. `panels/map/context_menu/render.rs` ×7 (~25)
- [ ] **P16.** `yagni` The per-row "Show on map" button column in `show_site_list` and the "Show world on map" button at the card foot — both byte-identical to handlers already on the same row/card. `panels/sites.rs:407` (~25)
- [ ] **P17.** `shrink` Merge `on_catalog_edited` into `after_catalog_commit` in the 5 panels where the former is literally the 2-line wrapper. `after_catalog_commit` has 7 other direct callers and stays. `panels/{sites,missions,hooks,personae,relations}.rs` (~20)
- [ ] **P18.** `shrink` Merge 3 of the 5 `faction_combo` copies (`orbital.rs`'s is the superset). **The other two are genuinely different and stay** — `search.rs:867` is `&mut String`, `sites.rs:950` is `&mut Option<FactionId>` with a custom-typing row. `panels/relations.rs:861`, `routes.rs:495` (~20)
- [ ] **P19.** `shrink` The two rail-row plate bodies, differing only in subline format and badge label+colour → `roster::rail_row`. `panels/missions.rs:258`, `hooks.rs:213` (~20)
- [ ] **P20.** `yagni` `should_dismiss_sector_context_menu(esc, focused, outside)` — a 3-arg `a || !b || c` wrapper with one production caller, whose twin menu already inlines the identical expression. `panels/map/context_menu/resolve.rs:85` (~20)
- [ ] **P21.** `delete` `SectorMenuTarget::SubsectorBorder` — never constructed in production (one `#[cfg(test)]` construction only). Drop the variant, its `"CLOSE"`-only render arm, the match alternative, and the half-test. `panels/map/context_menu/mod.rs:91` (+ `state/types.rs:613`) (~19)
- [ ] **P22.** `shrink` `mod.rs`'s `col!` macro + 16 hand-written round-robin section calls + the `let _ = next;` apology → `const SECTIONS: &[fn(..)]` + one loop. Only `features.rs:257` needs widening to `&mut`. `panels/world/mod.rs:134` (~19)
- [ ] **P23.** `shrink` `show_control_section`'s eight hand-written 8-line `labeled(..)` blocks, six of them just `who(&c.<field>)`. `panels/world/overlays.rs:37` (~22)
- [ ] **P24.** `shrink` The `.split(',').map(trim).filter(!is_empty).collect()` round-trip written 4× → one `csv_field`. `panels/sites.rs:818` (~18)
- [ ] **P25.** `shrink` "Known status" and "True status" are the same 18-line combo twice. `enum_combo` does not fit (it binds `&mut Option<T>`). `panels/sites.rs:774` (~18)
- [ ] **P26.** `shrink` `region_kind_label`'s hand-rolled snake→Title Case is identical to `title_case_slug` in `export.rs:264`. (`search.rs:1023`'s `humanize_slug` is *sentence*-case, genuinely different — leave it.) `panels/surface_regions.rs:39` (~18)
- [ ] **P27.** `shrink` The `let wid` / `let mut draft` / `let mut changed` preamble + 9-line `if changed { state.run(EditWorld{..}) }` epilogue, copy-pasted into three sections. Keeps the command bus intact. `panels/world/environment.rs:22` (~18)
- [ ] **P28.** `shrink` `show_anchor_link` rebuilds link text for all three `PersonaAnchor` arms when `anchor_label` two fns below already formats them. `panels/personae.rs:369` (~15)
- [ ] **P29.** `native` Both `show_filter_row` bodies hand-roll the `Option<Enum>` combo → `widgets::enum_combo`. `filter_kind` is transient UI state, so bus-exempt. `panels/missions.rs:199`, `hooks.rs:159` (~15)
- [ ] **P30.** `shrink` `horiz` bool round-trip for the layout toggle → two `ui.selectable_value(&mut state.system_view.layout, ..)` calls. `panels/system/preview.rs:27` (~14)
- [ ] **P31.** `stdlib` Three hand-rolled `Option`/`bool` dances in `show_hex_map` → `Option::unzip`, `bool::then_some`, `Option::is_some_and`. Includes a no-op cast. `panels/map/interactions.rs:57` (~13)
- [ ] **P32.** `shrink` Three copies of the 5-line `if value == cur { format!("• {}", x) } else { .. }` selection marker → one line. `panels/map/context_menu/render.rs:390,415,487` (~12)
- [ ] **P33.** `native` Hand-rolled `get_temp_mut_or` / `TextEdit` / `insert_temp` buffer → `panels::persistent_singleline`, the crate's own helper used by every other text field in the directory. `panels/system/bulk_ops.rs:63` (~10)
- [ ] **P34.** `delete` `orbit: u8` on `SystemMenuTarget::World` and the `orbit` param of `render_world_menu`, which opens with `let _ = orbit;`. `panels/system_map.rs:577` (~8)
- [ ] **P35.** `native` Hardcoded 7-variant `SystemState` array in the FLIP CONTROL STATE submenu → `panels::SYSTEM_STATES` (already the documented single source, used by `control/mod.rs` and `history.rs`). `panels/map/context_menu/render.rs:310` (~8)
- [ ] **P36.** `delete` `sites_panel.edit_target` — both writes sit one line after `selection.site_id` is set to the identical value; no cross-tab writer exists despite the doc claiming one. *(The field stays on the other 3 panel states, which do have real cross-tab writers.)* `panels/sites.rs:376` (~8)
- [ ] **P37.** `yagni` `SystemMenuAction::OpenWorldTab` — its arm is byte-identical to `FocusWorld`'s. Keep the menu row, dispatch `FocusWorld`. `panels/system_map.rs:46` (~7)
- [ ] **P38.** `yagni` `feature_display_name` — 10 lines for one caller. `panels/world/features.rs:244` (~7)
- [ ] **P39.** `shrink` The star-colour selector round-trips through a `String` code: `code()` → `find(|v| v.code() == current)` is the identity function. `panels/world/identity.rs:140` (~6)
- [ ] **P40.** `shrink` The first two early-returns of `show_persona_detail` emit the same placeholder → one `let (Some(a), Some(b)) = .. else`. `panels/personae.rs:295` (~6)
- [ ] **P41.** `delete` Duplicate `const SYS_STAR_GRID_ANCHOR` whose doc comment admits it must be hand-synced with its twin. `panels/system_map.rs:58` (~5)
- [ ] **P42.** `delete` Duplicate "Open in ROUTES ▸" row dispatching the same `SectorMenuAction::FocusRoute { id }` as the "FOCUS ROUTE" row 50 lines above. `panels/map/context_menu/render.rs:433` (~5)

### builder — core (non-panels) — net −931 prod

*(§39 LD3 is D2, the command-variant collapse is D3.)*

- [ ] **B1.** `shrink` `revert` re-inlines the world lookup that `world_mut` already encapsulates, 11 times — `apply` uses the helper, `revert` spells out the 6-line `flat_map` walk by hand. `builder/src/builder/command.rs` ×11 (~50; ~20 if D3 lands)
- [ ] **B2.** `shrink` The warp-region copy-on-write dance written out 7 times (`clone` → `find` → `Arc::new`). `system_mut`/`world_mut` exist for the other two entity kinds; add `with_region_mut`. `builder/src/builder/command.rs` ×7 (~28; ~12 if D3 lands)
- [ ] **B3.** `delete` Four `BuilderError` variants nobody ever constructs — `ValidationFailed`, `InvariantViolated`, `EntityNotFound`, `StaleSnapshot`. *Zero hits; the `ValidationFailed` matches elsewhere are `sectorforge::SectorError`'s, a different enum. Re-verified in the main thread.* `builder/src/builder/errors.rs` (~8)
- [ ] **B4.** `yagni` `builder/Cargo.toml:41` and `viewer/Cargo.toml:36` still restate `[[bin]] name/path` for an auto-discovered `src/main.rs` whose default name is already the package name — the cut Round-1 #70 made at the root, missed on the two members. (`[[bench]] harness = false` is **not** a default and must stay.) (~6)
- [ ] **B5.** `delete` `save_session` writes a `.sgforge` file nothing can read — Round 1 removed `load_session`, leaving one `#[cfg(test)]` caller that writes the file and immediately deletes it. Drop the fn, two re-exports, and the smoke test's save/read-back tail (keep the headless paint loop, which is the real test). `SessionFile` itself stays. **Spec'd as D6** — same class of call as D2. `builder/src/builder/session.rs:71-75` (~20)

### gui-core — net −472

- [ ] **G1.** `yagni` `selectable_void_plate` + `paint_void_plate` + `star_hash`/`STAR_COUNT` + the `VOID_TOP`/`VOID_BOTTOM`/`STARLIGHT` tokens — ~110 lines of hashed per-frame starfield serving exactly **one** call site (`panels/nav.rs:184`), where `selectable_plate` has an identical signature. **§BEAUTY chrome: an aesthetic call, not a mechanical one.** `gui-core/src/card.rs:48,205-306`, `design.rs:208-222` (~110)
- [ ] **G2.** `yagni` ⚠g `RenderMapTheme`'s 23 geometry knobs are constants in disguise — `star_radius_mul`, 7 ring bumps/thicknesses, 5 route-overlay muls, 2 glow alphas, `pending_route_preview_min`, 5 label `ScaledSize`s. Nothing constructs the struct except `Default` + `from_map_theme`, which writes only *other* fields; no consumer assigns one. Bonus dead data: 5 `ScaledSize.min` floors are written and never read (only `.mul` is used). `gui-core/src/map_theme.rs:85-113,250-277` (~45)
- [ ] **G3.** `shrink` `if let Some(v) = &x { kv(ui, "K", &v.to_uppercase()) }` ×16 plus two hand-spelled `||`-chained `is_some()` guards → one `opt_kv_block(ui, title, &[(&str, Option<&str>)])`. `gui-core/src/info_panel/{world,system}.rs` (~40)
- [ ] **G4.** `delete` `validation_color` — zero production callers; only its own 4 unit tests and one assertion inside `theme.rs`'s `#[cfg(test)] mod tests`. `gui-core/src/palette.rs:182-190` + tests (~31)
- [ ] **G5.** `yagni` `bundled-fonts` is a flag nobody toggles — both consumers set it in `default` and all three OFL faces are committed, so the `#[cfg(not(..))]` no-op branch, the `display_family()` fork, and ~20 lines of "enable ONLY after committing…" prose are unreachable in any shipped build. Costs you the `--no-default-features` stock-egui fallback. `gui-core/Cargo.toml:7-12`, `fonts.rs`, `design.rs:74-100` (~30, −1 feature flag)
- [ ] **G6.** `shrink` TAGS / NOTES / list-of-strings blocks copy-pasted 8× across four `info_panel` files → one `list_block(ui, heading, items, upper)`. (~30)
- [ ] **G7.** `shrink` The 4-shape route-control glyph match exists twice — `legend_control_row` re-paints what `draw_route_control_glyph` already paints. Same four arms, same primitives; only Interdiction's axis differs. `gui-core/src/info_panel/mod.rs:104-155` vs `palette.rs:442-484` (~25)
- [ ] **G8.** `shrink` ⚠g Three copies of the route-segment resolve prologue in `paint_routes` / `paint_path_route_glow` / `paint_selected_route` — token-identical apart from the surrounding filter. `gui-core/src/sector_view/view.rs:495,574,609` (~22)
- [ ] **G9.** `shrink` `primary_button` / `danger_button` / `ghost_button` share a 14-line prologue; only `pad` and a width floor differ. `gui-core/src/widgets.rs:27,192,229` (~22)
- [ ] **G10.** `shrink` ⚠g Four near-identical `*_font_px` gates and four identical `*_MIN_VISIBLE_PX = 3.0` consts. `gui-core/src/sector_view/render.rs:20-23,486-513` (~20)
- [ ] **G11.** `shrink` Two cache-key structs that are the same struct — `SectorOverviewCacheKey`'s 8 fields are a subset of `HeatmapCacheKey`'s 10, each with an identical `from_sector`. One `SectorFingerprint::of(sector)`. `gui-core/src/heatmap.rs:33-62`, `info_panel/overview.rs:18-43` (~20)
- [ ] **G12.** `shrink` ⚠g The alpha-restamp idiom `Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)` appears 11× across six files, and `palette.rs` already has a private `fade` doing the same. One `pub fn with_alpha`. (~18)
- [ ] **G13.** `shrink` `draw_faction_style_rgb_preview` and `draw_faction_swatch` are the same function — allocate → `painter_at` → `rect_filled` → `rect_stroke` → centred glyph. One fn with a size arg. `gui-core/src/palette.rs:928-963` (~16)
- [ ] **G14.** `shrink` ⚠g `draw_hex` / `draw_hex_fill` share the `size < 3.0` circle fallback and the `hex_vertices → convex_polygon` body verbatim, differing only in `Stroke`. `gui-core/src/sector_view/render.rs:355-381,627-638` (~14)
- [ ] **G15.** `shrink` `CHROME`/`STATUS` global scaffolding duplicated verbatim (`static RwLock<T>` + `set_x` + `x()`); byte-identical modulo identifiers. `gui-core/src/palette.rs:51-64,139-152` (~12)
- [ ] **G16.** `delete` `region_hex_counts` intermediate map with a dead third field — built, then immediately re-looped into `region_centroids` destructuring `(q, r, _)`. Insert directly in the first loop. `gui-core/src/sector_view/cache.rs:64-84` (~8)
- [ ] **G17.** `yagni` Three pure alias wrappers — `info_panel::kv` → `ui_kit::kv`, `render::hex_center` → `hex_center_xy`, `system_view::layout_center` (one caller, `show()` already inlines the equivalent). (~9)

### viewer — net −1,660

*(The three feature removals are D1; the ~170 below are what remains regardless.)*

- [ ] **V1.** `shrink` The page shell `CentralPanel::default().frame(Frame::none().fill(chrome_bg()).inner_margin(14.0))` is retyped at **12 sites**. One `fn page(ctx, add)` beside the existing `require_sector`. gui-core exports no page-shell helper. `viewer/src/app/*.rs` ×12 (~45)
- [ ] **V2.** `shrink` Grid-header label runs — 6 copies of `ui.label(RichText::new(X).color(chrome_text_dim()).strong())` in `regions_view`, 4 more in `trade_view`. `viewer/src/app/{regions_view,trade_view}.rs` (~35)
- [ ] **V3.** `shrink` ⚠g The zoom-toward-cursor + drag-pan block is duplicated verbatim between the sector map and the planner map, down to the `(scroll / 400.0).exp()` and `.clamp(5.0, 250.0)` constants. `viewer/src/app/{sector_view.rs:346,planner_view.rs:60}` (~24)
- [ ] **V4.** `delete` `EditorState.project_input` + the `ProjectInput`-loading branch of `load_project_sector` — **dead once D1 lands**; every surviving reader is inside the three cut surfaces. `viewer/src/editor/{state.rs,file_ops.rs}` (~21)
- [ ] **V5.** `shrink` Five byte-identical `Utf8PathBuf::from_path_buf(..) else { self.export_status = "path is not valid utf-8"; return }` guards → one `require_utf8`. (The two at `:394`/`:399` carry different messages — leave them.) `viewer/src/app/export_ui.rs` (~12)
- [ ] **V6.** `yagni` `combo_str_id<T>` / `combo_kv_id<T>` are generic over `T: AsRef<str> + From<String>` with one concrete instantiation each. Take `&mut WorldId` / `&mut SystemId`. `viewer/src/editor/ui_helpers.rs:88-115` (~9)
- [ ] **V7.** `yagni` `PresenceStats::system_count()` / `world_count()` — one-caller wrappers over `.len()`. `viewer/src/factions_overview.rs:38-46` (~9)
- [ ] **V8.** `shrink` `segmentum_view`'s `link_table` and `link_detail` each re-run the same three-arm `RouteViewMode` match whose `_` arm duplicates `Detailed`. `viewer/src/segmentum_view.rs:613,661` (~9)
- [ ] **V9.** `yagni` `show_header` / `show_table_header`'s `edit_mode: bool` param is `false` at both call sites, making the `" - edit mode"` suffix and the "BULK" column unreachable. `viewer/src/factions_overview.rs:340,428` (~8)

### tests + benches — net −795

- [ ] **T1.** `delete` **14 `if !dir.exists() { return; }` skip-guards on `presets_dir()`.** `presets/` is 76 git-tracked files under `CARGO_MANIFEST_DIR` with no `include`/`exclude`, so the branch cannot fire — but if it ever did it would silently turn **12 tests into green no-ops**. `tests/it/random_sector_tests.rs` ×12, `analytics_and_presets.rs:59,76` (~40)
- [ ] **T2.** `delete` **`bench_derive_warm_500`** — the closure named `derive_warm/500` calls no derive and no cache; its entire timed body is two `Arc::clone`s and a `chronicle.clone()`, so it can never fail the §42 PERF6 50 ms budget it cites. The genuine warm path is `builder_mutations.rs:176`. `benches/generation.rs:154` (~35)
- [ ] **T3.** `shrink` 13 `generate_prefix(input, cutoff, nonces, |_| {}, || false).expect(..)` sites, 8 blown to 6–8 lines purely by the two no-op closures. The compact form is already used twice in the same file. `tests/it/iterative_gen_tests.rs` (~50)
- [ ] **T4.** `delete` `benches/render_png.rs` as a standalone target — its timed closure is `render_sector_image` + `encode_png_bytes`, which `generation.rs` already times at the identical scales, and the file re-declares `project_dir`/`SCALES`/`fixture` to do it. Keep the FU-9 combined number as a ~14-line fn inside `generation.rs`. (~55)
- [ ] **T5.** `shrink` Two hand-rolled bless/read/compare golden blocks re-implementing `golden_generation.rs::assert_content_golden` line for line. Move it + `first_line_diff` into `shared.rs`. Both bless paths and both committed goldens survive verbatim. `tests/it/export_byte_goldens.rs:147,187` (~38)
- [ ] **T6.** `delete` `full_preset_loads_validates_and_enables_overlays` — verbatim the loop body of `themed_baselines_...`. Fold in via `once("_full").chain(THEMED_BASELINES)`; every message already interpolates `{preset}`. `tests/it/random_sector_tests.rs:213` (~37)
- [ ] **T7.** `shrink` `cli_behavior.rs`'s private `run()` + 7 hand-rolled `Command::new(bin())` blocks in three other files → `shared::run_cli` / `run_cli_ok`. (~35)
- [ ] **T8.** `shrink` Three `bench_function` blocks in `bench_command_apply` — identical `iter_batched` bodies differing only in the `BuilderCommand`. Also move the per-iteration `clone()` out of the timed closure. `builder/benches/builder_mutations.rs:79` (~34)
- [ ] **T9.** `shrink` Two route boundary triples + two regions boundary triples — twelve bodies whose setup is byte-identical apart from one field and the `enabled` flag. Table loops with the value named in the message. `tests/it/validation_tests.rs:447,509` (~60)
- [ ] **T10.** `shrink` 4 render/serialize bench fns — four copies of the same `SCALES` loop differing only in group name and the measured call. Also kills 12 of 15 redundant `generate_sector` setup calls. `benches/generation.rs:200` (~29)
- [ ] **T11.** `shrink` 7 copies of the build-a-`StrategicOutputRule`-and-insert block (the scope key is always one of two literals) → two file-local helpers. `tests/it/validation_tests.rs:611` (~28)
- [ ] **T12.** `shrink` 5 copies of the clone-row-0 / clear / push-N-stubs workbook setup → one `m42_rows(usable, excluded)`. Keep the 5th site's append form (it asserts `orig + 3`). `tests/it/validation_tests.rs:61` (~28)
- [ ] **T13.** `shrink` The 29-line hand-built `GeneratedSector` literal in `empty_sector()` — 8 fields plus a 17-line manifest, none of which either test reads, when `imports_test.rs:32` already spreads `..Default::default()`. `tests/it/imports_test.rs:6` (~28)
- [ ] **T14.** `shrink` Two bad-format-flag tests with identical bodies bar the flag pair and the stderr needle → one table test. `tests/it/cli_behavior.rs:196` (~30)
- [ ] **T15.** `delete` The "spot-check a few invariants in case JSON equality ever short-circuits" block + the `ok`-flag re-assert — both strictly implied by the preceding assertion, and the stated NaN rationale cannot apply (the comparison is on `String`s, not `Value`s). `tests/it/cli_gui_parity.rs:63,113` (~26)
- [ ] **T16.** `shrink` The 2-line `tempdir()` + `Utf8PathBuf::from_path_buf(..)` preamble at **24 sites** → `shared::tempdir_utf8()`. The `TempDir` guard must stay bound. (~24)
- [ ] **T17.** `shrink` `bench_run_search` + `bench_search_perf7` — the two timed closures are byte-identical; only group name, template, constraint and budget differ. `benches/seed_search.rs:90` (~22)
- [ ] **T18.** `shrink` The per-bench-file prelude — `project_dir()` byte-identical in 5 files, `fixture`/`fixture_for` in 3, `SCALES` in 3. Put them in `benches/common/mod.rs` (subdir, so cargo won't auto-register a target). (~20)
- [ ] **T19.** `delete` `route_monotonicity_holds_across_two_densities` — a strict subset of the test above it: same seed strategy, same generator, same two assertion fns, and **no cross-density comparison is ever made**. Both its density ranges lie inside the survivor's. `tests/it/route_monotonicity.rs:191` (~20)
- [ ] **T20.** `shrink` `bench_validate_pre`/`_post`/`_500` + `bench_invariants_500` — four fns that are two calls at two scales. `benches/generation.rs:68` (~20)
- [ ] **T21.** `shrink` The 8-line seed-determinism proptest preamble repeated verbatim in 4 suites → `shared::prop_seed_deterministic`. Also drops a now-unused import from 4 files. (~19)
- [ ] **T22.** `delete` The G31 "ok-aggregation" pair — both re-run inputs already covered verbatim elsewhere. Move the two unique asserts into those tests. `tests/it/validation_tests.rs:1176` (~19)
- [ ] **T23.** `shrink` 4 copies of the `generate_system_standalone(load_project(..), idx, HexCoord{q,r})` block + 3 stray `let project = fixture_project();` lines. `tests/it/invariants_tests.rs:114` (~19)
- [ ] **T24.** `shrink` `wishes_unsatisfiable(budget)` and `wishes(budget)` → `wishes(budget, constraint)`; the `SearchConfig` blocks are byte-identical and the duplicated doc comment has already drifted onto the wrong fn. `benches/seed_search.rs:52` (~18)
- [ ] **T25.** `shrink` `big_examples_load_and_are_square` + `big_examples_have_expected_dims` — the exact-dim asserts already subsume both weaker ones, and both tests load the same two examples. 4 loads become 2. `tests/it/loading_tests.rs:90` (~18)
- [ ] **T26.** `shrink` `read_relative_rejects_parent_dir_escape` + `..._absolute_path_escape` — bodies differ only in the injected value, and one guard emits both messages. `tests/it/loading_tests.rs:37` (~18)
- [ ] **T27.** `delete` `random_max_80x80_completes` — `SectorSize::Huge` **is** 80×80 and config synthesis reads only `size.dims()`, so `Custom { dim: 80 }` produces a byte-identical config. `Custom{dim}` pass-through is already covered at dim 9. `tests/it/random_sector_tests.rs:167` (~16)
- [ ] **T28.** `shrink` 5 repetitions of load → optional seed override → generate → render → encode. No memoisation, so the determinism test's two-independent-generations property survives. `tests/it/golden_png.rs:26` (~13)
- [ ] **T29.** `delete` `golden_generation.rs`'s private `fixture_sector()` `OnceLock` — duplicates `shared::fixture_sector()` in a second static, costing one extra full m42 generation per binary run. `iterative_gen_tests.rs:17` defines a third copy of `fixture_input`. (~13)
- [ ] **T30.** `shrink` `random_custom_size_respects_dims` duplicates the whole tmp/generate/assert block for two extra asserts. Move them into `assert_complete` (which also pins the square invariant for every size) and add a third table row. `tests/it/random_sector_tests.rs:138` (~11)
- [ ] **T31.** `shrink` `export_writes_bitmap_png` / `export_writes_interactive_html` — byte-identical bar the format and filename. `tests/it/golden_generation.rs:222` (~10)
- [ ] **T32.** `delete` `sector_round_trips_through_json` — a strict subset of `sector_serde_is_idempotent` immediately below it, whose own comment says so. `tests/it/invariants_tests.rs:20` (~10)

---

## Net

**≈ −8,750 lines, −0 deps, −1 feature flag.**

By area: analysis+validate −1,440 · viewer −1,660 · export+cli+roots −1,400 · panels −1,040 · builder core −931 prod · model+gen −861 · tests+benches −795 · gui-core −472 · manifests −6.

Strip the five scope calls (D1–D5, ≈2,700) and **≈6,050 lines need no judgment at all** — they are dead code, duplicated blocks, and stdlib reinventions.

No dependency drops out. Every declared dep in all four manifests is genuinely imported (checked crate-by-crate against `use`/path references across the whole tree). The only removable knob is gui-core's `bundled-fonts` feature (G5).

---

## Not findings — negative results worth keeping

Recorded so the next audit doesn't re-derive them.

- **Builder panels are free of rot.** A sweep across all 46 panel modules: zero `#[allow(dead_code)]`, zero zero-caller fns out of 974 checked, zero write-only struct fields, zero commented-out blocks, all 27 `BuilderTab` variants dispatched. What remains there is duplication only.
- **`show_add_presence_row` / `show_add_claim_row` are not duplicates.** 136 and 140 diff lines against ~100- and ~80-line bodies, with real type divergence; `panels/presence_widgets.rs:1-15` documents that a prior audit already extracted the one shared piece and left the rest diverged on purpose.
- **The eight archetype `apply_*` bodies should not be collapsed.** Accumulators differ in kind (max/min/sum/bucketed-weighted-sum), three need world-level context the presence loop doesn't carry, and the classifiers are 4-, 4-, 5- and 7-arm ladders over four output enums. A generic helper needs 3–4 closures per rule and comes out longer. Only the staging ceremony (M2) is worth cutting.
- **No scoring table has all-equal weights** — hook weights 60–90, mission 40–75, `profile_targets` 0.4–1.2, `kind_profile` 40 varied rows. The sole exception is `HistoryEra::weight` (A34), which is inert.
- **`partial_cmp().unwrap_or(Equal)` is gone** from `src/analysis/` and the workspace generally (one line at `analytics.rs:290` is below the reporting floor).
- **All 12 `tests/it/shared.rs` helpers have external callers.** `assert_blake3_golden` is used by all four hash-pinning files; no hand-rolled hash-and-compare remains; no test in scope asserts nothing.
- **`WeightedAnchored` has two impls**, so it is not a single-implementation trait. `SystemLayout::Orbital`, `SystemPick::EmptyOrbit`/`Background`, the `cache: None` fallbacks in `SectorView`, and every `RenderMapTheme` *colour* field are all genuinely reached.
- **`svg_export_tests.rs:225`'s `tag_depth_balance` walker** cannot be replaced without adding `quick-xml` — leave it.
- **gui-core owning raw egui paint primitives is intentional**, which is why it has no `clippy.toml` ban list. Not a finding.

---

## Correctness bugs found — out of ponytail scope, reported not fixed

1. **`regions.toml` path split-brain.** `builder/src/builder/project_io.rs:119,177-184` creates regions at `data/regions/regions.toml` and `state/iterative_gen.rs:857` hardcodes the same, while the CLI generator (`src/gen/random_sector.rs:420,626,754`), all six presets, all three examples, and `panels/regions.rs:620` use `data/routes/regions.toml`. A builder-made project is self-consistent, but load a CLI-generated project into the builder and `iterative_gen.rs:857` repoints the config at a path that doesn't exist there.
2. **Builder history-slug copies have drifted from the engine.** The builder's private `kind_slug`/`event_kind_from_str` (`panels/history.rs:1540-1605`) duplicate `src/analysis/history/{labels,rules}.rs`, and the copies now disagree: the builder emits `prefer_event = "mandate"`/`"charter"`, which the engine's parser rejects. Fixing A5 fixes this.
3. **A UI knob that does nothing.** The builder's history-era `DragValue`, tooltipped "How heavily events cluster in this era", writes `HistoryEra::weight`, which nothing reads (A34). Every checked-in `data/history.toml` has been tuned under this impression.
4. **Silent config no-ops.** `presets/*/economy.toml`'s `[economy.by_population]` keys, and the `[economy.by_world_type]`/tech-level keys named in A7, match on strings no enum can produce — they are ignored without warning.
5. **`HeatmapMode::ConflictIntensity` is unreachable from the CLI** — `parse_heatmap` has no arm for it though it is listed in `HeatmapMode::ALL` (X26).
