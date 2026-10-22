# Adding a new world type

`WorldType` is a closed Rust enum, declared through the module-private `world_enum!` macro ([src/worlds.rs:61](../src/worlds.rs#L61)). Most downstream code keys off the variant's **string name** (e.g. `"HiveWorld"`) rather than matching the enum, so adding a variant is mostly a one-line edit plus a few keyed lookups.

This checklist covers the full surface. Skip steps explicitly marked *optional* if your variant inherits sensible defaults.

## 1. Rust enum + string round-trip — [src/worlds.rs](../src/worlds.rs)

Add one `Variant => "Display Label"` line to the `world_enum! { … WorldType { … } }` declaration (line ~61), keeping the (roughly alphabetical) order. The macro ([src/worlds.rs:11](../src/worlds.rs#L11)) generates the `pub enum`, `WorldType::VARIANTS` (in declaration order) and `display_name()` from that single line, so the three can't drift apart. Declaration order therefore drives UI dropdowns and golden tests.

The label must be **unique** — `FromStr` (line ~372) resolves by linear search over `VARIANTS` comparing `display_name()`, so a duplicate string makes one variant unparseable. `FromStr` needs no manual arm.

The `Debug`/`Display` impls use the Rust identifier (`Self::HiveWorld` prints `HiveWorld`), so the variant name is what TOML files reference. Pick a `CamelCase` name that won't need quoting in TOML.

## 2. Taxonomy string→enum — [src/model/taxonomy.rs](../src/model/taxonomy.rs) *(no action needed)*

`parse_world_type_variant` (line ~32) — the serde/loader-facing map, which accepts the *Rust identifier* form (no spaces or dashes) — is derived from `WorldType::VARIANTS` via `format!("{v:?}")`. It picks up the new variant for free once step 1 is done.

## 3. Economy defaults — [src/analysis/economy/tables.rs](../src/analysis/economy/tables.rs)

Two string-keyed match tables provide built-in behavior. Either extend them or supply equivalents via TOML overrides.

1. **`default_world_type_vector`** (line ~11): production/consumption vector (ore / promethium / foodstuffs / manufactured / archeotech / recruits). Falls through to a neutral default if absent.
2. **`default_strategic_world_type`** (line ~101): strategic output bucket. Same fallback behavior.

*Optional* if your project's `economy.toml` already covers the variant — note the two tables have **different** TOML keys, and they merge differently:

- `[economy.by_world_type.<Name>]` replaces the vector outright (the TOML entry wins over `default_world_type_vector`).
- `[resources.world_type.<Name>]` is a *partial overlay* applied on top of `default_strategic_world_type` — only the fields it names are overridden.

Two further string-keyed tables in [src/analysis/economy/derive.rs](../src/analysis/economy/derive.rs) — `base_resilience` (line ~291) and `strategic_needs_for_world` (line ~320) — have no TOML escape hatch. Both fall through to a neutral `_` arm, so they're optional flavor, not required edits.

## 4. Worlds data — [presets/\_full/data/worlds/worlds.toml](../presets/_full/data/worlds/worlds.toml)

Add weighted `[[generation]]` rows for the variant. Without rows, the generator will never roll the variant. Repeat for any other project you want it to appear in — each `presets/*/` and `examples/*/` carries its own `data/worlds/worlds.toml`.

A minimal contribution is one row per plausible `(star_colour, atmosphere, temperature, biosphere, population, tech_level, government, notable_feature)` tuple. Weights are relative — copy the magnitudes of an adjacent variant with similar prevalence.

If you want feature affinity, add entries under `[features.by_world_type]`:

```toml
[features.by_world_type]
YourVariant = [
  { feature = "AdministrativeHub", weight = 1.5 },
]
```

## 5. Faction preferences — [presets/\_full/data/factions/factions.toml](../presets/_full/data/factions/factions.toml) *(optional)*

If the variant should bias which factions claim it, add the variant name to the relevant `preferred_world_types = [...]` lists. Factions without an entry treat it as neutral.

## 6. Builder + viewer UI

No code change needed. Both UIs iterate `WorldType::VARIANTS`, so the dropdowns pick it up automatically once step 1 is done.

## 7. Tests

```bash
cargo test --workspace                       # enum coverage + loader tests
cargo test --test it -- golden               # byte-stable output check
```

One test **will** fail by construction: `assert_eq!(WorldType::VARIANTS.len(), 24)` in [builder/src/builder/panels/world/mod.rs:289](../builder/src/builder/panels/world/mod.rs#L289). Bump the count.

If the new variant appears in golden inputs, golden tests will fail — review the diff, confirm it's expected, and re-bake the goldens. The CLAUDE.md determinism invariant still applies: only re-bake when *you* changed the rendering or generation logic, not because a flaky byte changed.

## 8. Cross-check

Quick grep to confirm no stragglers:

```bash
grep -rn "HiveWorld" src/ builder/ viewer/ gui-core/ --include="*.rs"
```

Most hits are test fixtures (including `src/gen/world_pool.rs:384`, which is inside `#[cfg(test)] mod tests`). The production code that genuinely branches per world type, outside `src/worlds.rs` and the economy tables of step 3, is:

- `src/gen/surface_region.rs:88` — surface-region mix per world type.
- `src/gen/sites.rs:273` — candidate site kinds per world type.

Both end in a `_ =>` fallback, so a new variant compiles and generates without an arm; add one only if the default mix is wrong for it. Anything else the grep turns up outside those files needs equivalent handling.

## What this checklist deliberately omits

- **Renaming** an existing variant. That breaks savefiles and every TOML overlay. Use serde aliases (see `RouteType` in [src/model/sector_model/mod.rs:551](../src/model/sector_model/mod.rs#L551)) rather than a rename.
- **Adding new economy axes** (a new resource). Out of scope; touches the `ResourceVector` struct itself.
- **Adding a new world property axis** (e.g. a sibling of `Temperature`). Separate effort — needs a new key-tab enum and weight table.
