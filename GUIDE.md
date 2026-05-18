# sectorforge — User Guide

`sectorforge` is a deterministic Warhammer 40k star sector generator. It reads
a project directory (CSV data files plus TOML configuration files) and
produces a reproducible sector as JSON, Markdown, CSV, and bitmap images.

The world taxonomy and CSV parsing live in [src/worlds.rs](src/worlds.rs).
Everything else in this crate builds a sector-scale layer around it: candidate
pools, deterministic placement, systems, worlds, routes, factions,
subsector clustering, validation, export, and an interactive GUI viewer/editor.

---

## 0. Prerequisites

You need **Rust** installed via [rustup](https://rustup.rs/).

**macOS:** Install Xcode Command Line Tools first (`xcode-select --install`), then run rustup installer.

**Linux (Ubuntu/Debian):** `sudo apt install pkg-config libx11-dev libxcb1-dev libxi-dev libxinerama-dev libxcursor-dev libxrandr-dev`

**Windows:** Run the rustup installer; it includes a Rust-compatible MSVC build toolchain.

Verify installation:

```bash
rustc --version   # any 1.70+ works
cargo --version
```

## 1. Quick start

From the repository root:

```bash
# Build both binaries (sectorforge CLI + sectorforge-gui)
cargo build --release

# Validate the bundled example project (M42 world data + sample TOML files)
cargo run --bin sectorforge -- validate --project examples/m42_project

# Generate a sector
cargo run --bin sectorforge -- generate --project examples/m42_project --allow-warnings

# Inspect world-data directory contents
cargo run --bin sectorforge -- inspect-worlds --data-dir examples/m42_project/data/worlds

# Launch the GUI viewer/editor (cargo alias is `sgui`)
cargo sgui --project examples/m42_project
```

`generate` runs pre-generation validation, then sector generation, then
post-generation invariant checks (spec §11.11). If any invariant fails the
sector is not written and the process exits non-zero.

After `generate`, look in [examples/m42_project/out/](examples/m42_project/out/):

```
out/
  manifest.json                # seed, version, input digests, counts
  sector.json                  # canonical machine-readable sector
  sector.md                    # human-readable summary
  validation_report.json       # pre-generation validation note
  systems/
    sys-0001.json              # one JSON per system
    sys-0002.json
     ...
  csv/
    systems.csv
    worlds.csv
    routes.csv
  sector.png                   # bitmap overview (if `bitmap` in output formats)
  systems/sys-NNNN.png         # per-system bitmap renderings
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

Runs validation, then generation, then writes output files. Refuses to
continue if validation reports errors. Refuses to continue with warnings
unless `--allow-warnings` is passed.

| Flag | Meaning |
|---|---|
| `--seed <SEED>` | Override `[generation].seed` from `sectorforge.toml` |
| `--out <DIR>` | Override `[outputs].directory` |
| `--allow-warnings` | Continue past warnings (errors still block) |

### `sectorforge generate-system --project <DIR>`

Generate a single standalone system. Reuses the project's catalogs (world data,
names, factions) but emits just one `GeneratedSystem` with factions assigned,
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

### `sectorforge inspect-worlds --data-dir <DIR>`

Standalone diagnostic for a world-data directory (containing `key.csv` + `generator.csv`).
Prints key-table sizes, generator row counts, candidate counts, and top-weight
star colours / world types / notable features. Useful when authoring or debugging data.

---

## 3. Project directory layout

A project is a folder that contains a `sectorforge.toml` and data sub-directories.
The bundled example is at [examples/m42_project/](examples/m42_project/):

```
my-sector-project/
  sectorforge.toml
  data/
    worlds/                        # CSV world data: key.csv + generator.csv
      key.csv
      generator.csv
    names/system_names.toml
    names/world_names.toml
    factions/factions.toml
    routes/route_rules.toml
  out/                             # created by generate
```

### `sectorforge.toml`

The main config. Minimal version:

```toml
[project]
id = "my-sector"
title = "My Generated Sector"
description = "Optional description."        # optional
version = "0.1.0"                             # optional

[inputs]
world_data_dir        = "data/worlds"
system_names          = "data/names/system_names.toml"     # optional
world_names           = "data/names/world_names.toml"      # optional
factions              = "data/factions/factions.toml"      # optional
route_rules           = "data/routes/route_rules.toml"     # optional
generation_profiles   = "data/generation/profiles.toml"    # optional (digest tracked, content reserved)

[generation]
seed                       = "my-seed-string"
sector_width               = 8
sector_height              = 10
subsector_width            = 4     # reserved; subsector layout is currently k-means clustered, not tile-sized
subsector_height           = 5     # reserved (see above)
system_count               = 24
min_worlds_per_system      = 2
max_worlds_per_system      = 6
allow_empty_hexes          = true
world_feature_count        = 3
strict_world_rows          = true

[generation.placement]
mode                       = "uniform_grid"             # "uniform_grid" | "weighted_grid" | "clustered"
cluster_bias               = 0.0                        # attraction toward generation center (0 = none)
minimum_system_distance    = 1

[generation.world_selection]
mode                                 = "weighted_rows"
require_complete_rows                = true
allow_partial_rows                   = false      # allow rows missing optional fields
same_star_colour_bias                = 1.25       # bias toward matching star colours
strict_same_star_colour              = false      # all worlds in a system share the primary star colour
avoid_duplicate_world_type_in_system = false      # prevent repeated world types per system

[generation.routes]
enabled                    = true
max_route_distance         = 4
route_density              = 0.30
ensure_connected_graph     = true

[outputs]
directory                  = "out"
formats                    = ["json", "markdown", "csv", "bitmap"]
pretty_json                = true
write_per_system_files     = true
write_manifest             = true
write_diagnostics          = false   # reserved flag; no extra diagnostic files emitted yet

[outputs.bitmap]
sector_scale        = 5          # integer scale multiplier for the sector map (1..=8)
system_scale        = 4          # integer scale multiplier for per-system maps (1..=8)
render_systems      = true       # generate per-system bitmap renders as well
faction_fill        = true       # §8: tint each sector hex AND halo each per-system planet by dominant faction's FactionStyle.fill
heatmap             = "off"      # §10: per-system heatmap tint applied to the PNG.
                                 # one of: off | control | military | trade | industrial
                                 #         covert | faith | threat | intel
```

The CLI accepts `--heatmap <mode>` and `--no-faction-fill` on `generate` to
override the project's bitmap settings without editing the TOML.

### `data/names/system_names.toml`

```toml
[system_names]
prefixes      = ["Acheron", "Belisarius", ...]
suffixes      = ["Reach", "Terminus", ...]
single_names  = ["Malfi", "Scintilla", ...]
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
roots    = ["Iocanthos", "Solace", ...]    # required if you want non-fallback names
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
id    = "imperial_administration"
name  = "Imperial Administration"
kind  = "imperial"
weight = 10.0
default_disposition = "lawful"
preferred_world_types        = ["HiveWorld", "BastionWorld"]
preferred_governments        = ["MilitaryGovernor", "MagistrateCouncil"]
preferred_notable_features   = ["AdministrativeHub", "PoliceState"]
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
default_weight          = 1.0
max_distance            = 4
prefer_populated_worlds = true
prefer_trade_hubs       = true
avoid_warp_phenomena    = true

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

`when` accepts any combination of `notable_feature`, `world_type`, and
`government` keys. Routes connect systems whose hex distance ≤ `max_distance`.
Weights factor in distance falloff, then the standard
`prefer_populated_worlds` / `prefer_trade_hubs` / `avoid_warp_phenomena`
bonuses, then your custom modifiers. With `ensure_connected_graph = true`,
the generator adds bridge edges so every system reaches every other.

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

## 5. World data files

`sectorforge` uses CSV files to define world generation candidates. The directory
must contain two files in `data/worlds/`:

- **`key.csv`** — columns list the canonical values for star colour, world type,
  atmosphere, temperature, biosphere, population, tech level, government, and
  notable feature. Each column header is the field name; each row entry is a
  valid variant name.
- **`generator.csv`** (previously `Generator Template` sheet in Excel) — each data
  row is one weighted candidate world. Columns map to enum strings for all required
  fields, plus a counter column and a weight column.

A row is "usable" only when **all** required fields parse AND the weight is
finite and > 0. Rows that don't qualify are reported by `validate` and
`inspect-worlds`. The default `require_complete_rows = true` mode discards
them.

To add new candidates, append rows to `generator.csv`.

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
   "systems": [ /* GeneratedSystem ... */ ],
   "routes": [ /* GeneratedRoute ... */ ],
   "factions": [ /* GeneratedFaction ... */ ],
   "manifest": { /* seed, digests, counts */ }
}
```

Each `GeneratedSystem` has an `id`, `name`, `coord`, `star`, list of
`worlds`, plus `primary_factions`, `tags`, `notes`, and a `control`
summary (see "Faction control model" below).

Each `GeneratedWorld` wraps a `WorldDto` view of `worlds::World` — variant
names are stable (e.g. `"HiveWorld"`) — and also carries `claims`
(per-faction legal/military/religious claims) and a `control`
multi-winner snapshot.

#### Faction control model

Derived deterministically after faction placement (no extra RNG draws). See
[src/control.rs](src/control.rs) and the design doc
[faction_sector_control_and_power_design.md](faction_sector_control_and_power_design.md);
items not yet implemented are listed in [NEXT.md](NEXT.md).

* **Per presence** (`systems[].worlds[].factions[]`): `influence`, `dominance`
  (Rumored / Presence / Influence / Contested / Controlled / Stronghold),
  `dimensions` (admin, military, orbital, economic, industrial, ideological,
  covert, logistics, legitimacy, visibility — each 0–100), and
  `intel_confidence`. `industrial` is a first-class dimension separate from
  `economic` (forge / manufacturing output vs. trade); `PowerProfile.industrial`
  is derived from it directly.
* **Per world** (`systems[].worlds[].control`): `dominant`, `sovereign`,
  `occupier`, `economic_hegemon`, `popular_authority`, `hidden_master`,
  `contested`, `control_score`. `claims` is a parallel list of typed claims
  (LegalSovereignty / ImperialMandate / ReligiousMandate / DynasticRight /
  CommercialCharter / MilitaryOccupation / AncientDomain / HuntingGround /
  CovertWrit / Rebellion / TreatyRight).
* **Per system** (`systems[].control`): aggregated `state` (Pacified /
  Fragmented / Blockaded / Warzone / Infiltrated / Quarantined / Uncharted),
  plus `dominant`, `sovereign`, `orbital_controller`, `economic_hegemon`,
  `hidden_master`, and `top_factions`.
* **Per faction** (`factions[].power`): `PowerProfile` with `administrative`,
  `military`, `naval`, `economic`, `industrial`, `ideological`, `covert`,
  `logistical`, `legitimacy`. Call `PowerProfile::total_projection()` for a
  single weighted total.

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
     "data/worlds/key.csv": "blake3:...",
     "data/worlds/generator.csv": "blake3:...",
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

## 7. Subsectors

After a sector is generated, `sectorforge::build_subsectors` groups its
systems into clusters using greedy farthest-first seeding plus Lloyd
refinement over hex distance (see [src/subsectors.rs](src/subsectors.rs)).
Each cluster gets:

- A spreadsheet-style label (`A`..`Z`, `AA`..) assigned row-major over capital coords.
- A `name` derived from its chosen capital system (`"Subsector Aurelia"`).
- The list of every (q, r) hex it covers, including empty ones — drives map borders.
- Internal vs. border route classification.
- Neighbor / connected subsector adjacency.
- A summary: world-type counts, faction control basis-points, dominant
  factions, controlling faction (if any), capital system + capital world,
  per-faction `control_tier` (`absolute` / `clear` / `plurality` /
  `contested` / `presence` / `trace`).

Cluster count derives from `target_systems_per_subsector` (default 12):
`K = ceil(system_count / target)`. Controllable via `SubsectorConfig`:

```rust
let subs = sectorforge::build_subsectors(&sector, sectorforge::SubsectorConfig {
    target_systems_per_subsector: 12,
    max_iterations: 24,
    include_empty_subsectors: true,
    faction_control_top_n: 5,
    tracked_faction_ids: vec![],
    control_denominator: sectorforge::ControlDenominator::InhabitedSystems,
})?;
```

The GUI's sector view uses these clusters for the subsector overlay /
detail panel. Subsectors are derived on demand from a `GeneratedSector` —
they are not persisted into `sector.json`.

Note: the `subsector_width` / `subsector_height` keys in `sectorforge.toml`
are accepted by the config parser but the current clustering ignores them.

---

## 8. GUI viewer

`sectorforge-gui` is an interactive viewer/editor for generated sectors,
built with egui + eframe. It exposes the following views via the top
navigation bar:

- **Sector** — hex map with zoom/pan, colored by primary star colour,
  faction tint, and subsector overlay. Click a hex to drill into the system.
  The bottom controls expose a **HEATMAP** dropdown that tints every system
  hex by a per-mode score: `CONTROL` (dominant-faction colour ×
  control-score intensity), `MILITARY`, `TRADE`, `INDUSTRY`, `COVERT`,
  `FAITH`, `THREAT` (military × covert restricted to hostile/zealous), or
  `INTEL` (low-visibility hexes glow). See
  [src/gui/heatmap.rs](src/gui/heatmap.rs).
- **System** — per-system detail panel: worlds, coords, star type, tags,
  factions, neighboring systems.
- **Edit** — sector editor (rename systems, add/remove worlds, adjust tags
  and per-world factions). The **Factions** tab shows a deterministic colour
  + glyph chip per faction (derived from `kind`, `id`, `disposition` — see
  [src/gui/palette.rs](src/gui/palette.rs) `faction_style`) and lets you
  filter by kind/disposition, sort by total power, and pin favourites to
  the top.
- **Data** — CSV data editor for `key.csv` / `generator.csv` from inside
  the app.
- **Planner** — route planner: pick `from` / `to` systems and pathfind over
  the existing route graph. Two metrics: `Safest` (Dijkstra with hazard
  weights — avoid Unstable / Hazardous / Dangerous) or `Shortest` (BFS over
  hop count). `Perilous` routes are always impassable.

The GUI also supports exporting bitmap PNGs at a configurable scale:
sector overview, a single system map, or all per-system maps. The current
HEATMAP selection in the sector view is carried into the exported PNG.

### Launching the GUI

A `cargo sgui` alias is registered in [.cargo/config.toml](.cargo/config.toml):

```bash
# From a project directory (auto-loads out/sector.json if present)
cargo sgui --project examples/m42_project

# Direct path to a sector.json
cargo sgui examples/m42_project/out/sector.json

# Empty editor (no sector loaded — starts in edit mode)
cargo sgui
```

With no args, the GUI falls back to `examples/m42_project/out/sector.json`
when that file exists, otherwise launches empty.

**Note:** The GUI requires a graphical display (X11/Wayland on Linux, native on macOS/Windows).
It will not run on headless servers. For CLI-only workflows, use `sectorforge generate`
and inspect the output files.

### Library-level GUI usage

The GUI module is exposed as `sectorforge::gui::App`. The struct takes a
`GeneratedSector` in `App::new(sector)` or launches empty via `App::new_empty()`.
Use `app.with_project_dir(dir)` to attach a project directory for regeneration
and data-editor preloading.

---

## 9. Library use

`sectorforge` is also a library crate (`pub lib` named `sectorforge`).
**This crate is not published on crates.io** — you must reference it via path.
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

let subs = sectorforge::build_subsectors(&sector, sectorforge::SubsectorConfig::default())?;
println!("{} subsectors", subs.len());
```

Public surface:

| Function | Purpose |
|---|---|
| `load_project(dir)` | Read sectorforge.toml + all referenced files |
| `validate_project(&input)` | Pre-generation validation, returns `ValidationReport` |
| `generate_sector(input)` | Deterministic sector generation, returns `GeneratedSector` |
| `generate_system_standalone(input, index, coord)` | Deterministic single-system generation, returns `GeneratedSystem` |
| `validate_sector(&sector)` | Post-generation invariant check (spec §11.11), returns `InvariantReport` |
| `build_subsectors(&sector, cfg)` | Derive subsector clusters from a generated sector |
| `render_sector_markdown(&sector)` | Pure Markdown render, returns `String` |
| `render_system_markdown(&system)` | Pure Markdown render for one standalone system |
| `load_sector_json(path)` | Read a previously generated `sector.json` back into a `GeneratedSector` |
| `write_sector_json(path, &sector)` | Pretty-JSON sector writer |
| `write_system_json(path, &system)` | Pretty-JSON standalone system writer |
| `write_sector_markdown(path, &sector)` | Markdown writer |
| `export_sector(&sector, &cfg, dir)` | Write JSON / Markdown / CSV / manifest + bitmaps |
| `inspect_world_workbook(path)` | World-data diagnostics (used by `inspect-worlds`) |

Re-exported types: `AppConfig`, `SectorError`, `ProjectInput`, `InvariantReport`,
`InvariantViolation`, `GeneratedSector`, `GeneratedSystem`, `HexCoord`,
`Subsector`, `SubsectorConfig`, `SubsectorBuildError`, `ControlDenominator`,
`ValidationIssue`, `ValidationReport`.

---

## 10. Validation reference

Validation runs over both project config and the world data. Errors block
generation; warnings only block when `--strict` (validate) or absence of
`--allow-warnings` (generate) is set.

Common codes:

| Code | Meaning |
|---|---|
| `GEN_GRID_EMPTY` | `sector_width * sector_height == 0` |
| `GEN_SYSTEM_COUNT_OVERFLOW` | `system_count` exceeds grid cells |
| `GEN_WORLD_COUNT_RANGE` | `min_worlds_per_system > max_worlds_per_system` |
| `WB_NO_USABLE_ROWS` | World data produced zero usable candidates |
| `WB_EXCLUDED_ROWS` | At least one row was excluded (warning) |
| `KEY_TABLE_EMPTY` | A key.csv column has no parseable entries |
| `FACTION_DUPLICATE_ID` | Two factions share an `id` |
| `FACTION_BAD_WEIGHT` | Faction weight is ≤ 0 or non-finite |
| `FACTION_UNKNOWN_*` | Faction references a string that isn't a variant name |
| `ROUTE_BAD_DEFAULT_WEIGHT` / `ROUTE_BAD_MULTIPLIER` | Route weights / multipliers must be > 0 and finite |
| `NAME_POOL_EMPTY` | All system name lists are empty (fallback names will be used) |

---

## 11. Tests

```bash
cargo test           # all tests
cargo test --lib     # unit tests only
```

Notable suites:

- [src/world_pool.rs::tests](src/world_pool.rs) — candidate filtering and conversion
- [src/rng.rs::tests](src/rng.rs) — stage seeds and weighted selection
- [src/sector_model.rs::tests](src/sector_model.rs) — axial hex distance
- [src/subsectors/mod.rs::tests](src/subsectors/mod.rs) — clustering coverage, capital naming, route classification, determinism
- [tests/golden_generation.rs](tests/golden_generation.rs) — full end-to-end + determinism
- [tests/invariants_tests.rs](tests/invariants_tests.rs) — post-generation invariants, JSON round-trip, standalone system generation, faction-influence ordering
- [tests/invariants_proptest.rs](tests/invariants_proptest.rs) — proptest fuzz: invariants + determinism across random seeds, sector sizes, world ranges
- [tests/validation_tests.rs](tests/validation_tests.rs) — adverse inputs

Benchmarks (criterion):

```bash
cargo bench --bench generation            # full sample
cargo bench --bench generation -- --quick # ~10s smoke
```

Benches in [benches/generation.rs](benches/generation.rs) cover `generate_sector` at three sector sizes (8×10 / 16×20 / 24×30), `validate_project`, and `validate_sector_invariants`.

---

## 12. Customization recipes

**Generate a sparser frontier sector.**
Lower `system_count`, drop `route_density` to `0.15`, raise
`max_worlds_per_system` slightly. Add a route modifier that multiplies
`WarpPhenomena` routes down to `0.1`.

**Force one star colour per system to be very strict.**
In `[generation.world_selection]` set `strict_same_star_colour = true`.
All worlds in each system will then share the system's primary star colour.

**Use your own world data.**
Place `key.csv` and `generator.csv` in a directory and update `[inputs].world_data_dir`.
Each column must have a valid header name and rows filled with variant names from the
canonical enum list. Add rows to introduce new candidate worlds.

**Reproduce a previous sector exactly.**
Pin the seed, keep `sectorforge.toml` unchanged, and keep every file
referenced from `[inputs]` byte-identical. `manifest.json` lists every
input digest so you can verify match before running.

**Resize subsector clusters.**
Pass a custom `SubsectorConfig` with a different `target_systems_per_subsector`
to `build_subsectors`. There is currently no `sectorforge.toml` knob for
this — it is a library-level setting consumed by the GUI and by external
callers of the API.

---

## 13. Where to look in the source

| File | Purpose |
|---|---|
| [src/lib.rs](src/lib.rs) | Public API surface and re-exports (with doc-tests + `# Errors` on every fallible fn) |
| [src/main.rs](src/main.rs) | Clap-based CLI (`sectorforge` binary) |
| [src/gui/main.rs](src/gui/main.rs) | GUI binary entry point (`sectorforge-gui`) |
| [src/worlds.rs](src/worlds.rs) | Canonical world enums + CSV parser (do not modify casually) |
| [src/world_pool.rs](src/world_pool.rs) | Adapts `GenerationRow` to weighted candidates |
| [src/generation.rs](src/generation.rs) | Placement, systems, worlds, factions, routes. `build_system` is the unit reused by sector + standalone APIs |
| [src/sector_model.rs](src/sector_model.rs) | Output DTOs (`GeneratedSector` etc.) with `Serialize` + `Deserialize` |
| [src/control.rs](src/control.rs) | Faction presence → dimension scores, claims, multi-winner control summaries, and per-faction `PowerProfile` aggregation |
| [src/validation.rs](src/validation.rs) | All pre-generation checks |
| [src/invariants.rs](src/invariants.rs) | Spec §11.11 post-generation invariants |
| [src/render.rs](src/render.rs) | Pure Markdown rendering (sector + standalone system). Includes faction display buckets (§15) and per-world / per-system stability (§11.1) |
| [src/export.rs](src/export.rs) | JSON / Markdown / CSV / manifest writers + bundle export |
| [src/bitmap/mod.rs](src/bitmap/mod.rs) | Sector PNG rendering (`image` crate); coordinates hex grid + routes + systems + legend |
| [src/bitmap/primitives.rs](src/bitmap/primitives.rs) | Pixel-level drawing primitives + embedded 5×7 font, shared with `system_map` |
| [src/system_map.rs](src/system_map.rs) | Per-system PNG rendering; honours `outputs.bitmap.faction_fill` to halo each planet by its dominant faction (§8) |
| [src/subsectors/mod.rs](src/subsectors/mod.rs) | Subsector clustering (k-means / Lloyd) + public API |
| [src/subsectors/summary.rs](src/subsectors/summary.rs) | Ownership resolution, faction-control tallies, capital selection |
| [src/config.rs](src/config.rs) | `sectorforge.toml` schema |
| [src/input.rs](src/input.rs) | Project loader (config + inputs + digests) |
| [src/names.rs](src/names.rs) | Name table types |
| [src/factions.rs](src/factions.rs) | Faction file types |
| [src/routes.rs](src/routes.rs) | Route-rules file types |
| [src/rng.rs](src/rng.rs) | Stage-based deterministic RNG |
| [src/taxonomy.rs](src/taxonomy.rs) | Variant-name ↔ enum bridge |
| [src/ids.rs](src/ids.rs) | Canonical id-string formatting |
| [src/errors.rs](src/errors.rs) | `SectorError` type |
| [src/faction_style.rs](src/faction_style.rs) | Pure-data per-faction style (RGB fill/accent + glyph + border); shared by GUI + PNG renderers |
| [src/heatmap.rs](src/heatmap.rs) | Pure-data per-system heatmap scoring (`HeatmapMode`); GUI + bitmap consumers share scoring |
| [src/importance.rs](src/importance.rs) | §10.3 / §15: `display_importance` per faction + kind-group aggregation into legend buckets. Shared `DEFAULT_MINOR_FRACTION` / `DEFAULT_DISPLAY_CAP` consumed by the PNG legend, GUI sector overview, and Markdown renderer so all three stay in sync |
| [src/stability.rs](src/stability.rs) | §11.1: static `StabilityState` per world + per system (public_order / corruption / fear / rebellion / xenos_threat / warp_instability / famine). Pure derivation from tags, world type, factions present, and existing control summary — no sim ticks |
| [src/route_control.rs](src/route_control.rs) | §3: per-route per-faction `RouteControl` (patrol / toll / interdiction / piracy / secrecy / confidence). Derived from endpoint-system faction presence + faction kind + endpoint tags (`quarantined`, `war_zone`). Stored on `GeneratedRoute.controls` (`#[serde(default)]`). Surfaced in the Markdown renderer, sector PNG (per-route midpoint glyph + `ROUTE CONTROL` legend), and GUI `system_summary` (`ROUTES` block keyed off the selected system) |
| [src/gui/app/mod.rs](src/gui/app/mod.rs) | Top-level eframe app + navigation |
| [src/gui/app/export_ui.rs](src/gui/app/export_ui.rs) | PNG export dialog + sector JSON bundle export |
| [src/gui/sector_view.rs](src/gui/sector_view.rs) | Hex map render widget |
| [src/gui/system_view.rs](src/gui/system_view.rs) | System detail panel widget |
| [src/gui/data_editor.rs](src/gui/data_editor.rs) | CSV data editor UI |
| [src/gui/route_planner.rs](src/gui/route_planner.rs) | Route planner (Safest / Shortest) |
| [src/gui/info_panel.rs](src/gui/info_panel.rs) | Text formatting widgets |
| [src/gui/editor/](src/gui/editor/) | Sector/world editing UI (map, settings, factions, routes, worlds, systems) |
| [src/gui/palette.rs](src/gui/palette.rs) | Color palette for GUI; egui wrapper around [src/faction_style.rs](src/faction_style.rs) (`faction_style`, glyph + border) |
| [src/gui/heatmap.rs](src/gui/heatmap.rs) | egui wrapper around [src/heatmap.rs](src/heatmap.rs) — same scoring, returns `Color32` cells |
