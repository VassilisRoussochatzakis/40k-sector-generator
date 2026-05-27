# Adding a new world type

`WorldType` is a closed Rust enum ([src/worlds.rs:57](../src/worlds.rs#L57)). Most downstream code keys off the variant's **string name** (e.g. `"HiveWorld"`) rather than matching the enum, so adding a variant is mostly mechanical edits in one file plus a few keyed lookups.

This checklist covers the full surface. Skip steps explicitly marked *optional* if your variant inherits sensible defaults.

## 1. Rust enum + string round-trip — [src/worlds.rs](../src/worlds.rs)

All four sites are in `src/worlds.rs` and must stay in sync:

1. **Enum variant** (line ~57): add to `pub enum WorldType { … }` in alphabetical order.
2. **`FromStr`** (line ~452): add the display-string → variant arm. Use the same human-readable form as `display_name`.
3. **`VARIANTS` array** (line ~884): add the variant. Order here drives UI dropdowns and golden tests.
4. **`display_name`** (line ~911): add the variant → human string arm.

The `Debug`/`Display` impls use the Rust identifier (`Self::HiveWorld` prints `HiveWorld`), so the variant name is what TOML files reference. Pick a `CamelCase` name that won't need quoting in TOML.

## 2. Taxonomy string→enum — [src/model/taxonomy.rs](../src/model/taxonomy.rs)

Add the `"YourVariantName" => WorldType::YourVariant` arm at line ~50. This is the serde/loader-facing map and accepts the *Rust identifier* form (no spaces or dashes).

## 3. Economy defaults — [src/analysis/economy.rs](../src/analysis/economy.rs)

Two string-keyed match tables provide built-in behavior. Either extend them or supply equivalents via TOML overrides.

1. **`default_world_type_vector`** (line ~290): production/consumption vector (ore / promethium / foodstuffs / manufactured / archeotech / recruits). Falls through to a neutral default if absent.
2. **`default_strategic_world_type`** (line ~419): strategic output bucket. Same fallback behavior.

*Optional* if your project's `economy.toml` already covers the variant via `[resources.world_type.<Name>]` — the TOML rule wins.

## 4. Worlds data — [WOW/data/worlds/worlds.toml](../WOW/data/worlds/worlds.toml)

Add weighted `[[generation]]` rows for the variant. Without rows, the generator will never roll the variant.

A minimal contribution is one row per plausible `(star_colour, atmosphere, temperature, biosphere, population, tech_level, government, notable_feature)` tuple. Weights are relative — copy the magnitudes of an adjacent variant with similar prevalence.

If you want feature affinity, add entries under `[features.by_world_type]`:

```toml
[features.by_world_type]
YourVariant = [
  { feature = "AdministrativeHub", weight = 1.5 },
]
```

## 5. Faction preferences — [WOW/data/factions/factions.toml](../WOW/data/factions/factions.toml) *(optional)*

If the variant should bias which factions claim it, add the variant name to the relevant `preferred_world_types = [...]` lists. Factions without an entry treat it as neutral.

## 6. Builder + viewer UI

No code change needed. Both UIs iterate `WorldType::VARIANTS`, so the dropdowns pick it up automatically once step 1 is done.

## 7. Tests

```bash
cargo test --workspace                       # enum coverage + loader tests
cargo test --test it -- golden               # byte-stable output check
```

If the new variant appears in golden inputs, golden tests will fail — review the diff, confirm it's expected, and re-bake the goldens. The CLAUDE.md determinism invariant still applies: only re-bake when *you* changed the rendering or generation logic, not because a flaky byte changed.

## 8. Cross-check

Quick grep to confirm no stragglers:

```bash
grep -rn "HiveWorld" src/ builder/ viewer/ gui-core/ --include="*.rs"
```

If there are hard-coded variant references outside `src/worlds.rs`, `src/model/taxonomy.rs`, and `src/gen/world_pool.rs:394` (the only known hard match at time of writing), add equivalent handling for your variant.

## What this checklist deliberately omits

- **Renaming** an existing variant. That breaks savefiles and every TOML overlay. Use serde aliases (see `RouteType` in [src/model/sector_model/mod.rs:403](../src/model/sector_model/mod.rs#L403)) rather than a rename.
- **Adding new economy axes** (a new resource). Out of scope; touches the `ResourceVector` struct itself.
- **Adding a new world property axis** (e.g. a sibling of `Temperature`). Separate effort — needs a new key-tab enum and weight table.
