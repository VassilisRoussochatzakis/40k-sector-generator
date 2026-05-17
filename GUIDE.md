# sectorforge — User Guide

`sectorforge` is a deterministic Warhammer 40k star sector generator. It reads
a project directory (an Excel workbook plus TOML configuration files) and
produces a reproducible sector as JSON, Markdown, and CSV.

The world taxonomy and Excel parsing live in [src/worlds.rs](src/worlds.rs).
Everything else in this crate builds a sector-scale layer around it.

---

## 1. Quick start

From the repository root:

```bash
# Build
cargo build --release

# Validate the bundled example project (M42 workbook + sample TOML files)
cargo run --bin sectorforge -- validate --project examples/m42_project

# Generate a sector
cargo run --bin sectorforge -- generate --project examples/m42_project --allow-warnings

# Inspect what the workbook contains
cargo run --bin sectorforge -- inspect-worlds --workbook "M42 Sector Generator.xlsx"
```

`generate` runs pre-generation validation, then sector generation, then
post-generation invariant checks (spec §11.11). If any invariant fails the
sector is not written and the process exits non-zero.

After `generate`, look in [examples/m42_project/out/](examples/m42_project/out/):

```
out/
  manifest.json              # seed, version, input digests, counts
  sector.json                # canonical machine-readable sector
  sector.md                  # human-readable summary
  validation_report.json     # pre-generation validation note
  systems/
    sys-0001.json            # one JSON per system
    sys-0002.json
    ...
  csv/
    systems.csv
    worlds.csv
    routes.csv
```

---

## 2. CLI commands

### `sectorforge validate --project <DIR>`

Loads everything but does not generate. Prints workbook stats, errors, and
warnings. Exits non-zero on any error.

| Flag | Meaning |
|---|---|
| `--json` | Emit the validation report as JSON to stdout |
| `--strict` | Treat warnings as errors (exit 1 if any warning) |

### `sectorforge generate --project <DIR>`

Runs validation, then generation, then writes output files. Will refuse to
continue if validation reports errors. Refuses to continue with warnings
unless `--allow-warnings` is passed.

| Flag | Meaning |
|---|---|
| `--seed <SEED>` | Override `[generation].seed` from `sectorforge.toml` |
| `--out <DIR>` | Override `[outputs].directory` |
| `--allow-warnings` | Continue past warnings (errors still block) |

### `sectorforge generate-system --project <DIR>`

Generate a single standalone system. Reuses the project's catalogs (workbook,
names, factions) but emits just one `GeneratedSystem`, with factions assigned,
not a full sector. Useful for one-off NPC system generation, scripted system
seeding, or scratchpad work.

| Flag | Meaning |
|---|---|
| `--seed <SEED>` | Override `[generation].seed` |
| `--index <N>` | 1-based system index (default `1`). Drives `sys-NNNN` id and the per-system RNG |
| `--coord-q <Q>` | Axial hex `q` coord (default `0`) |
| `--coord-r <R>` | Axial hex `r` coord (default `0`) |
| `--out <PATH>` | Write JSON to this path. If omitted, JSON goes to stdout |
| `--markdown` | Also emit a Markdown snippet. With `--out`, writes alongside as `<out>.md`; otherwise prints after the JSON |

Example:

```bash
cargo run --bin sectorforge -- generate-system \
    --project examples/m42_project \
    --index 12 --coord-q 3 --coord-r 4 \
    --seed scenario-A \
    --out /tmp/sys-0012.json --markdown
```

### `sectorforge validate-sector --sector <PATH>`

Load a previously generated `sector.json` and check the spec §11.11
post-generation invariants: unique system/world IDs, coordinates in bounds and
unique, route endpoints exist, route distances match `hex_distance`, undirected
edges deduplicated, faction summary references coherent, world tag namespaces
present, manifest counts match. Exits non-zero on any violation.

| Flag | Meaning |
|---|---|
| `--json` | Emit the invariant report as JSON |

### `sectorforge render-markdown --sector <PATH>`

Load a previously generated `sector.json` and print the Markdown overview.
Same output the `generate` command writes to `sector.md`; useful for
regenerating Markdown from a stored JSON without rerunning generation.

| Flag | Meaning |
|---|---|
| `--out <PATH>` | Write to a file instead of stdout |

### `sectorforge inspect-worlds --workbook <XLSX>`

Standalone diagnostic for the workbook. Prints key-table sizes, generation
row counts, candidate counts, and top-weight star colours / world types /
notable features. Useful when authoring or debugging a workbook.

---

## 3. Project directory layout

A project is a folder that contains a `sectorforge.toml` and a `data/`
sub-tree. The bundled example is at
[examples/m42_project/](examples/m42_project/):

```
my-sector-project/
  sectorforge.toml
  data/
    worlds/m42_sector_generator.xlsx
    names/system_names.toml
    names/world_names.toml
    factions/factions.toml
    routes/route_rules.toml
  out/                       # created by generate
```

### `sectorforge.toml`

The main config. Minimal version:

```toml
[project]
id = "my-sector"
title = "My Generated Sector"

[inputs]
world_workbook = "data/worlds/m42_sector_generator.xlsx"
system_names   = "data/names/system_names.toml"
world_names    = "data/names/world_names.toml"
factions       = "data/factions/factions.toml"
route_rules    = "data/routes/route_rules.toml"

[generation]
seed                  = "my-seed-string"
sector_width          = 8
sector_height         = 10
system_count          = 24
min_worlds_per_system = 2
max_worlds_per_system = 6
allow_empty_hexes     = true
world_feature_count   = 3
strict_world_rows     = true

[generation.placement]
mode                    = "uniform_grid"   # or "weighted_grid", "clustered"
minimum_system_distance = 1

[generation.world_selection]
mode                     = "weighted_rows"
require_complete_rows    = true
same_star_colour_bias    = 1.25
strict_same_star_colour  = false

[generation.routes]
enabled                = true
max_route_distance     = 4
route_density          = 0.30
ensure_connected_graph = true

[outputs]
directory               = "out"
formats                 = ["json", "markdown", "csv"]
pretty_json             = true
write_per_system_files  = true
write_manifest          = true
```

### `data/names/system_names.toml`

```toml
[system_names]
prefixes     = ["Acheron", "Belisarius", ...]
suffixes     = ["Reach", "Terminus", ...]
single_names = ["Malfi", "Scintilla", ...]
```

If only `single_names` is present, those names are used directly. If only
`prefixes` + `suffixes` are present, names are composed. With both, the
generator coin-flips between the two styles. With neither, the fallback is
`System {index}`.

### `data/names/world_names.toml`

```toml
[location_names]
fallback_pattern = "{system_name} {roman}"

[world_names]
prefixes = ["Saint", "Port", ...]
roots    = ["Iocanthos", "Solace", ...]   # required if you want non-fallback names
suffixes = ["Prime", "Secundus", ...]
```

When `roots` is empty, world names fall back to the
`{system_name} {roman}` pattern.

### `data/factions/factions.toml`

Each entry produces one faction in the generated sector. Preferred-* values
must use the **variant name** form from `src/worlds.rs` (e.g. `"HiveWorld"`,
not `"Hive World"`). Validation warns on unknown values.

```toml
[[factions]]
id   = "imperial_administration"
name = "Imperial Administration"
kind = "imperial"
weight = 10.0
default_disposition = "lawful"
preferred_world_types       = ["HiveWorld", "BastionWorld"]
preferred_governments       = ["MilitaryGovernor", "MagistrateCouncil"]
preferred_notable_features  = ["AdministrativeHub", "PoliceState"]
```

Assignment algorithm: base weight × 1.5 for matching world type, × 1.4 for
matching government, × 1.3 per matching notable feature. Up to 3 factions
per world (capped by population density). Per-world factions are emitted
sorted by influence (Dominant > Significant > Minor > Hidden) then catalog
order.

`primary_factions` for a system is the top-3 by **influence-weighted score**
(spec §10.9): sum of `influence.weight()` over the faction's presence on
that system's worlds (Dominant=3, Significant=2, Minor=1, Hidden=0.5). Ties
break by world-appearance count, then catalog order, then faction id.

### `data/routes/route_rules.toml`

```toml
[routes]
default_weight       = 1.0
max_distance         = 4
prefer_trade_hubs    = true
avoid_warp_phenomena = true

[[routes.modifiers]]
when = { notable_feature = "TradeHub" }
multiplier = 2.0

[[routes.modifiers]]
when = { notable_feature = "WarpPhenomena" }
multiplier = 0.25

[[routes.modifiers]]
when = { world_type = "ForgeWorld" }
multiplier = 1.5
```

Routes connect systems whose hex distance ≤ `max_distance`. Weights factor
in distance falloff, then standard hub/avoid bonuses, then your custom
modifiers. With `ensure_connected_graph = true`, the generator adds bridge
edges so every system reaches every other.

---

## 4. Determinism

Every run with the same seed + inputs produces byte-identical output. This
is enforced by:

- A user-controlled seed string (`[generation].seed`).
- Per-stage RNG streams derived as `blake3("sectorforge:{seed}:{stage}:{discriminator}")`.
- `BTreeMap` ordering for all maps that hit serialization.
- Stable, sorted ID strings: `sys-0001`, `sys-0001-w01`, `route-sys-0002-sys-0007`.
- An input-digest map in `manifest.json` so you know which files produced
  the output.

The integration test
[tests/golden_generation.rs](tests/golden_generation.rs)
asserts byte equality across two runs with identical seed.

To get different output, change the seed:

```bash
sectorforge generate --project examples/m42_project --seed alternative-seed
```

---

## 5. The Excel workbook

`sectorforge` uses the existing `worlds.rs` parser. The workbook must have:

- A **`Key`** sheet — columns A-I list the canonical values for star colour,
  world type, atmosphere, temperature, biosphere, population, tech level,
  government, and notable feature.
- A **`Generator Template`** sheet — each data row is one weighted candidate
  world. The parser reads columns A-I as enum strings, column J as the
  counter, column K as the weight.

A row is "usable" only when **all** required fields parse AND the weight is
finite and > 0. Rows that don't qualify are reported by `validate` and
`inspect-worlds`. The default `require_complete_rows = true` mode discards
them.

To add new candidates, fill in additional rows in `Generator Template`.

---

## 6. Output formats

### `sector.json`

The canonical machine-readable output. Top-level shape:

```jsonc
{
  "id": "m42-sector",
  "title": "M42 Generated Sector",
  "seed": "m42-default-seed",
  "generator_name": "sectorforge",
  "generator_version": "0.1.0",
  "width": 8, "height": 10,
  "systems":  [ /* GeneratedSystem ... */ ],
  "routes":   [ /* GeneratedRoute ... */ ],
  "factions": [ /* GeneratedFaction ... */ ],
  "manifest": { /* seed, digests, counts */ }
}
```

Each `GeneratedSystem` has an `id`, `name`, `coord`, `star`, list of
`worlds`, plus `primary_factions`, `tags`, and `notes`.

Each `GeneratedWorld` wraps a `WorldDto` view of `worlds::World` — variant
names are stable (e.g. `"HiveWorld"`).

### `sector.md`

Human-readable. Sections: title + seed, summary counts, ASCII sector map,
system index table, one block per system (coords, star, world table), then
routes and factions tables.

### `csv/*.csv`

`systems.csv`, `worlds.csv`, `routes.csv` for spreadsheet use. Multi-value
fields (factions, tags, features) are `;`-separated within a single cell.

### `manifest.json`

```jsonc
{
  "project_id": "m42-sector",
  "generator_name": "sectorforge",
  "generator_version": "0.1.0",
  "seed": "m42-default-seed",
  "seed_hash": "blake3:...",
  "input_digests": {
    "sectorforge.toml": "blake3:...",
    "data/worlds/m42_sector_generator.xlsx": "blake3:...",
    "data/names/system_names.toml": "blake3:...",
    ...
  },
  "settings_digest": "blake3:...",
  "system_count": 24,
  "world_count": 100,
  "route_count": 38
}
```

By default `generated_at_policy` is `"not recorded by default"` — the
manifest doesn't include a wall-clock timestamp so byte-stable output is
preserved.

---

## 7. Library use

`sectorforge` is also a library crate (`pub lib` named `sectorforge`).
Add to `Cargo.toml`:

```toml
[dependencies]
sectorforge = { path = "../40k-sector-generator" }
```

Then in code:

```rust
use camino::Utf8PathBuf;

let project_dir = Utf8PathBuf::from("examples/m42_project");
let mut input = sectorforge::load_project(&project_dir)?;
input.config.generation.seed = "custom-seed".to_string();

let report = sectorforge::validate_project(&input)?;
assert!(report.ok);

let output_cfg = input.config.outputs.clone();
let sector = sectorforge::generate_sector(input)?;
sectorforge::export_sector(&sector, &output_cfg, "out")?;
```

Public surface:

| Function | Purpose |
|---|---|
| `load_project(dir)` | Read sectorforge.toml + all referenced files |
| `validate_project(&input)` | Pre-generation validation, returns `ValidationReport` |
| `generate_sector(input)` | Deterministic sector generation, returns `GeneratedSector` |
| `generate_system_standalone(input, index, coord)` | Deterministic single-system generation, returns `GeneratedSystem` |
| `validate_sector(&sector)` | Post-generation invariant check (spec §11.11), returns `InvariantReport` |
| `render_sector_markdown(&sector)` | Pure Markdown render, returns `String` |
| `render_system_markdown(&system)` | Pure Markdown render for one standalone system |
| `load_sector_json(path)` | Read a previously generated `sector.json` back into a `GeneratedSector` |
| `write_sector_json(path, &sector)` | Pretty-JSON sector writer |
| `write_system_json(path, &system)` | Pretty-JSON standalone system writer |
| `write_sector_markdown(path, &sector)` | Markdown writer |
| `export_sector(&sector, &cfg, dir)` | Write JSON / Markdown / CSV / manifest + bitmaps |
| `inspect_world_workbook(path)` | Workbook diagnostics (used by `inspect-worlds`) |

---

## 8. Validation reference

Validation runs over both project config and the workbook. Errors block
generation; warnings only block when `--strict` (validate) or absence of
`--allow-warnings` (generate) is set.

Common codes:

| Code | Meaning |
|---|---|
| `GEN_GRID_EMPTY` | `sector_width * sector_height == 0` |
| `GEN_SYSTEM_COUNT_OVERFLOW` | `system_count` exceeds grid cells |
| `GEN_WORLD_COUNT_RANGE` | `min_worlds_per_system > max_worlds_per_system` |
| `WB_NO_USABLE_ROWS` | Workbook produced zero usable candidates |
| `WB_EXCLUDED_ROWS` | At least one row was excluded (warning) |
| `KEY_TABLE_EMPTY` | A Key-sheet column has no parseable entries |
| `FACTION_DUPLICATE_ID` | Two factions share an `id` |
| `FACTION_BAD_WEIGHT` | Faction weight is ≤ 0 or non-finite |
| `FACTION_UNKNOWN_*` | Faction references a string that isn't a variant name |
| `ROUTE_BAD_DEFAULT_WEIGHT` / `ROUTE_BAD_MULTIPLIER` | Route weights / multipliers must be > 0 and finite |
| `NAME_POOL_EMPTY` | All system name lists are empty (fallback names will be used) |

---

## 9. Tests

```bash
cargo test          # all tests
cargo test --lib    # unit tests only
```

Notable suites:

- [src/world_pool.rs::tests](src/world_pool.rs#L257) — candidate filtering and conversion
- [src/rng.rs::tests](src/rng.rs#L65) — stage seeds and weighted selection
- [src/sector_model.rs::tests](src/sector_model.rs#L160) — axial hex distance
- [tests/golden_generation.rs](tests/golden_generation.rs) — full end-to-end + determinism
- [tests/invariants_tests.rs](tests/invariants_tests.rs) — post-generation invariants, JSON round-trip, standalone system generation, faction-influence ordering
- [tests/validation_tests.rs](tests/validation_tests.rs) — adverse inputs

---

## 10. Customization recipes

**Generate a sparser frontier sector.**
Lower `system_count`, drop `route_density` to `0.15`, raise
`max_worlds_per_system` slightly. Add a route modifier that multiplies
`WarpPhenomena` routes down to `0.1`.

**Force one star colour per system to be very strict.**
In `[generation.world_selection]` set `strict_same_star_colour = true`.
All worlds in each system will then share the system's primary star colour.

**Use your own workbook.**
Drop your `.xlsx` in `data/worlds/` and update `[inputs].world_workbook`.
The workbook must have a `Key` sheet and a `Generator Template` sheet with
the column layout described in section 5.

**Reproduce a previous sector exactly.**
Pin the seed, keep `sectorforge.toml` unchanged, and keep every file
referenced from `[inputs]` byte-identical. `manifest.json` lists every
input digest so you can verify match before running.

---

## 11. Where to look in the source

| File | Purpose |
|---|---|
| [src/worlds.rs](src/worlds.rs) | Canonical world enums + Excel parser (do not modify casually) |
| [src/world_pool.rs](src/world_pool.rs) | Adapts `GenerationRow` to weighted candidates |
| [src/generation.rs](src/generation.rs) | Placement, systems, worlds, factions, routes. `build_system` is the unit reused by sector + standalone APIs |
| [src/sector_model.rs](src/sector_model.rs) | Output DTOs (`GeneratedSector` etc.) with `Serialize` + `Deserialize` |
| [src/validation.rs](src/validation.rs) | All pre-generation checks |
| [src/invariants.rs](src/invariants.rs) | Spec §11.11 post-generation invariants |
| [src/render.rs](src/render.rs) | Pure Markdown rendering (sector + standalone system) |
| [src/export.rs](src/export.rs) | JSON / Markdown / CSV / manifest writers |
| [src/main.rs](src/main.rs) | Clap-based CLI |
| [src/config.rs](src/config.rs) | `sectorforge.toml` schema |
| [src/rng.rs](src/rng.rs) | Stage-based deterministic RNG |
| [src/taxonomy.rs](src/taxonomy.rs) | Variant-name string ↔ enum bridge |
