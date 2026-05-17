# Rust Star Sector Generator — Application Specification Using Existing `worlds.rs`

**Document purpose:** This Markdown file is a code-generation-ready application specification for a Rust command-line and library application that generates a star sector from input data files. It explicitly assumes the project already contains a `worlds.rs` module that defines the world-domain enums, world structs, Excel loading logic, and `calamine`-based parsing for the M42 Sector Generator workbook.

**Working title:** `sectorforge`

**Primary language:** Rust

**Application type:** CLI application plus reusable library crate

**Most important integration rule:** Do **not** recreate the world taxonomy in the new generator. The uploaded/existing `worlds.rs` module is the authoritative world specification. The generator must import and use its types and loading functions wherever possible.

---

## 1. Existing World Specification Assumptions

The project already has a Rust file named `worlds.rs`. The sector generator must treat it as the canonical source for all world-level vocabulary and spreadsheet parsing.

### 1.1 Canonical world-domain ownership

`worlds.rs` owns these concepts:

- `StarColour`
- `WorldType`
- `Atmosphere`
- `Temperature`
- `Biosphere`
- `Population`
- `TechLevel`
- `Government`
- `NotableFeature`
- `GenerationRow`
- `World`
- `WorldEntry`
- `System`
- `KeyTables`
- `KeyTables::from_xlsx(path)`
- `load_generation_rows(path)`

The new application must not define duplicate enums for these concepts in another module. Any output-facing type should either reuse these types directly or wrap/convert them through a serialization DTO layer.

### 1.2 Workbook ownership

The existing module expects an `.xlsx` workbook with at least these sheets:

- `Key`
- `Generator Template`

The `Key` sheet provides lookup values for columns A through I:

| Column | Meaning |
|---:|---|
| A | Star Colour |
| B | World Type |
| C | Atmosphere |
| D | Temperature |
| E | Biosphere |
| F | Population |
| G | Tech Level |
| H | Government |
| I | Notable Feature |

The `Generator Template` sheet provides weighted generation rows. In the current module, the parsed `GenerationRow` reads:

| Column | Parsed as |
|---:|---|
| A | `star_colour: Option<StarColour>` |
| B | `world_type: Option<WorldType>` |
| C | `atmosphere: Option<Atmosphere>` |
| D | `temperature: Option<Temperature>` |
| E | `biosphere: Option<Biosphere>` |
| F | `population: Option<Population>` |
| G | `tech: Option<TechLevel>` |
| H | `government: Option<Government>` |
| I | `notable_feature: Option<NotableFeature>` |
| J | `counter: Option<usize>` |
| K | `weight: Option<f64>` |

The existing `WorldEntry` struct also has fields for system seed, star type, and location name, but the current `load_generation_rows` path primarily returns `GenerationRow` values. Therefore the surrounding application must include a small adapter layer for resolving generation rows into final sector systems.

### 1.3 Required behavior around `worlds.rs`

The code generator must follow these rules:

1. Import `worlds.rs` as `crate::worlds` or as a module inside the library crate.
2. Use `worlds::load_generation_rows` to load the Excel world-generation workbook.
3. Use `worlds::GenerationRow` as the raw weighted world candidate row type.
4. Use `worlds::World` as the resolved world-domain type where practical.
5. Do not rename enum variants unless a deliberate migration is requested.
6. Do not replace the `FromStr` logic in `worlds.rs`; instead, report validation errors when workbook values fail to parse into known variants.
7. Do not rely on `Display` output being user-facing unless a later implementation improves those `Display` impls. The current `Display` implementations may render debug-style variant names.
8. Add Serde derives to `worlds.rs` only if modifying the file is allowed. If not, create output DTOs that convert from `worlds.rs` types into serializable strings.
9. Treat `GenerationRow.weight` as the numeric selection weight. Rows with no usable positive weight must be excluded from weighted selection and reported in validation output.
10. Treat missing required world fields in `GenerationRow` as either invalid for fully resolved worlds or as partial/template rows only if an explicit generation profile enables fallback resolution.

---

## 2. Product Summary

`sectorforge` generates a deterministic star sector from input files. Its world-generation layer is powered by the existing `worlds.rs` module and the Excel workbook it parses. The broader application adds sector-scale concerns around those worlds:

- sector size and coordinate layout
- system placement
- deterministic random generation
- system naming
- star/system grouping
- route generation
- factions and claims
- exports to JSON, Markdown, and optional CSV
- validation and inspection commands

The application must be useful both as:

1. A command-line generator for users who maintain input tables and want generated sector outputs.
2. A Rust library that exposes deterministic generation functions for tests, GUIs, editors, web services, or future game tools.

---

## 3. Non-Goals

The first implementation must not attempt to solve every possible worldbuilding feature.

The following are non-goals unless later requested:

- Replacing the existing world taxonomy.
- Replacing the existing Excel workbook parser with a new schema.
- Simulating astrophysics accurately.
- Implementing a graphical UI.
- Implementing a database server.
- Implementing multiplayer or network synchronization.
- Generating prose with an LLM at runtime.
- Encoding setting-specific lore outside the supplied data files.

---

## 4. Top-Level Design Goals

### 4.1 Deterministic output

Given the same inputs, seed, generator version, and command options, the application must produce byte-stable or near-byte-stable output.

Requirements:

- Use an explicit seeded RNG.
- Never use wall-clock time except when the user explicitly requests an automatic seed.
- Include the resolved seed in every generation manifest.
- Derive per-stage RNG streams from the root seed.
- Sort maps and exported lists before serialization.
- Avoid nondeterministic `HashMap` iteration in output ordering.
- Include a content digest of input files in the output manifest.

### 4.2 Use existing world module

The world domain is already specified. The new application should build around it.

Requirements:

- The world candidate pool comes from `worlds::load_generation_rows`.
- The generator must not duplicate enum definitions.
- Output serialization must represent world fields in readable stable strings.
- Validation must report how many workbook rows became usable candidates.
- Validation must report rows with missing fields, zero/negative weights, unparsable values, or duplicate/ambiguous features.

### 4.3 Data-driven generation

The generator should be controlled by input data files, not hardcoded content.

Requirements:

- Sector configuration must come from TOML, YAML, or JSON.
- Names, factions, routes, tags, and output preferences must come from files.
- The world workbook remains the primary source of world combinations and world feature weights.
- Cross-references must be validated before generation.

### 4.4 Code-generation-friendly architecture

The application should be easy to generate from this spec.

Requirements:

- Keep modules small and cohesive.
- Use strongly typed structs.
- Centralize error handling.
- Separate loading, validation, generation, and export.
- Keep CLI-specific logic outside core generation logic.
- Make every generation stage independently testable.

---

## 5. Recommended Crate Layout

Use a single Cargo package with both a library and binary, or a workspace with `sectorforge-core` and `sectorforge-cli`. For simplicity, the first implementation can be one package with `src/lib.rs` and `src/main.rs`.

```text
sectorforge/
  Cargo.toml
  src/
    lib.rs
    main.rs
    worlds.rs                  # existing uploaded module; authoritative world spec
    config.rs                  # app config structs and config loading
    errors.rs                  # shared error enum
    ids.rs                     # deterministic ID helpers
    rng.rs                     # seeded RNG utilities
    validation.rs              # pre-generation validation
    world_pool.rs              # adapts worlds::GenerationRow into weighted candidates
    sector_model.rs            # generated sector/system DTOs
    generation/
      mod.rs
      context.rs
      sector.rs
      systems.rs
      worlds.rs                # generation logic using crate::worlds types; avoid name conflict carefully
      routes.rs
      factions.rs
      names.rs
    input/
      mod.rs
      project.rs
      names.rs
      factions.rs
      route_rules.rs
    export/
      mod.rs
      json.rs
      markdown.rs
      csv.rs
      manifest.rs
    cli/
      mod.rs
      commands.rs
  tests/
    golden_generation.rs
    validation_tests.rs
    world_pool_tests.rs
  examples/
    m42_project/
      sectorforge.toml
      data/
        worlds/
          m42_sector_generator.xlsx
        names/
          system_names.toml
          world_names.toml
        factions/
          factions.toml
        routes/
          route_rules.toml
```

### 5.1 Naming collision guidance

Because `worlds.rs` already exists, avoid confusing module paths.

Recommended options:

- Keep the uploaded module as `crate::worlds`.
- Put generator world logic under `crate::generation::worlds`.
- In files that use both, import with aliases:

```rust
use crate::worlds as world_spec;
use crate::generation::worlds as world_gen;
```

Or use explicit imports:

```rust
use crate::worlds::{GenerationRow, World, NotableFeature};
```

---

## 6. Recommended Dependencies

Exact versions can be selected by the implementer, but the application should use these crates or equivalent alternatives:

```toml
[dependencies]
calamine = "*"        # already required by worlds.rs
clap = { version = "*", features = ["derive"] }
serde = { version = "*", features = ["derive"] }
serde_json = "*"
serde_yaml = "*"
toml = "*"
thiserror = "*"
anyhow = "*"          # CLI boundary only, not core library errors
rand = "*"
rand_chacha = "*"
blake3 = "*"
indexmap = { version = "*", features = ["serde"] }
schemars = { version = "*", features = ["derive"] }
tracing = "*"
tracing-subscriber = "*"
camino = { version = "*", features = ["serde1"] }
```

Optional but useful:

```toml
[dev-dependencies]
insta = "*"
pretty_assertions = "*"
tempfile = "*"
```

### 6.1 Serde strategy for `worlds.rs`

There are two acceptable approaches.

#### Preferred approach: add derives to `worlds.rs`

If modifying `worlds.rs` is allowed, add these derives to all world-domain enums and structs:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
```

For `StarColour`, preserve `Copy`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
```

For output readability, add `#[serde(rename_all = "snake_case")]` only if changing serialized names is acceptable. If long-term compatibility matters, use explicit `serde(rename = "...")` per variant.

#### Non-invasive approach: output DTO conversion

If `worlds.rs` must remain untouched, create serializable DTOs:

```rust
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub struct WorldDto {
    pub star_colour: String,
    pub star_colour_code: String,
    pub world_type: String,
    pub atmosphere: String,
    pub temperature: String,
    pub biosphere: String,
    pub population: String,
    pub tech_level: String,
    pub government: String,
    pub notable_features: Vec<String>,
}
```

Then implement:

```rust
impl From<&crate::worlds::World> for WorldDto {
    fn from(world: &crate::worlds::World) -> Self {
        Self {
            star_colour: world.star_colour.short_name().to_string(),
            star_colour_code: world.star_colour.code().to_string(),
            world_type: format!("{}", world.world_type),
            atmosphere: format!("{}", world.atmosphere),
            temperature: format!("{}", world.temperature),
            biosphere: format!("{}", world.biosphere),
            population: format!("{}", world.population),
            tech_level: format!("{}", world.tech_level),
            government: format!("{}", world.government),
            notable_features: world.notable_features.iter().map(|f| format!("{}", f)).collect(),
        }
    }
}
```

The DTO approach is safer for code generation because it avoids requiring changes to the existing file.

---

## 7. Project Directory Input Format

The application should generate from a project directory. The project directory contains one main config file plus data folders.

```text
my-sector-project/
  sectorforge.toml
  data/
    worlds/
      m42_sector_generator.xlsx
    names/
      system_names.toml
      world_names.toml
    factions/
      factions.toml
    routes/
      route_rules.toml
    profiles/
      generation_profiles.toml
  out/
    # generated files go here
```

### 7.1 Main config: `sectorforge.toml`

Example:

```toml
[project]
id = "m42-sector"
title = "M42 Generated Sector"
description = "A generated sector using the M42 world workbook."
version = "0.1.0"

[inputs]
world_workbook = "data/worlds/m42_sector_generator.xlsx"
system_names = "data/names/system_names.toml"
world_names = "data/names/world_names.toml"
factions = "data/factions/factions.toml"
route_rules = "data/routes/route_rules.toml"
generation_profiles = "data/profiles/generation_profiles.toml"

[generation]
seed = "m42-default-seed"
sector_width = 8
sector_height = 10
subsector_width = 4
subsector_height = 5
system_count = 48
min_worlds_per_system = 1
max_worlds_per_system = 8
allow_empty_hexes = true
world_feature_count = 3
strict_world_rows = true

[generation.placement]
mode = "weighted_grid"
cluster_bias = 0.35
minimum_system_distance = 1

[generation.world_selection]
mode = "weighted_rows"
require_complete_rows = true
allow_partial_rows = false
same_star_colour_bias = 1.25
avoid_duplicate_world_type_in_system = false

[generation.routes]
enabled = true
max_route_distance = 4
route_density = 0.30
ensure_connected_graph = true

[outputs]
directory = "out"
formats = ["json", "markdown"]
pretty_json = true
write_per_system_files = true
write_manifest = true
```

### 7.2 Names file: `system_names.toml`

```toml
[system_names]
prefixes = ["Acheron", "Belisarius", "Cyrene", "Drusus", "Eidolon"]
suffixes = ["Reach", "Terminus", "Anchorage", "Gate", "Crown"]
single_names = ["Malfi", "Scintilla", "Vaxanide", "Quaddis"]

[location_names]
reuse_workbook_location_names = true
fallback_pattern = "{system_name} {roman}"

[world_names]
prefixes = ["Saint", "Port", "New", "Black", "Ash"]
roots = ["Iocanthos", "Solace", "Klybo", "Meridian", "Lacuna"]
suffixes = ["Prime", "Secundus", "Tertius", "Station", "Deep"]
```

### 7.3 Factions file: `factions.toml`

```toml
[[factions]]
id = "imperial_administration"
name = "Imperial Administration"
kind = "imperial"
weight = 10
default_disposition = "lawful"
preferred_world_types = ["HiveWorld", "CivilizedWorld", "BastionWorld"]
preferred_governments = ["MilitaryGovernor", "MagistrateCouncil"]

[[factions]]
id = "mechanicus"
name = "Adeptus Mechanicus"
kind = "mechanicus"
weight = 6
default_disposition = "insular"
preferred_world_types = ["ForgeWorld", "ResearchStation", "Orbital"]
preferred_notable_features = ["ArchaeotechRuins", "ForbiddenTech", "MajorSpaceyard"]

[[factions]]
id = "free_traders"
name = "Free Trader Compact"
kind = "merchant"
weight = 4
default_disposition = "opportunistic"
preferred_notable_features = ["Freeport", "TradeHub", "TheSilentTrade"]
```

Important: faction preference values must reference either `worlds.rs` enum variant names or a documented string representation derived from them. The validator must detect mismatches.

### 7.4 Route rules file: `route_rules.toml`

```toml
[routes]
default_weight = 1.0
max_distance = 4
prefer_populated_worlds = true
prefer_trade_hubs = true
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

### 7.5 Generation profiles file

Profiles allow users to run multiple generator styles without editing the main config.

```toml
[profiles.default]
description = "Balanced sector generation."
system_count_multiplier = 1.0
world_count_bias = "balanced"
route_density_multiplier = 1.0

[profiles.frontier]
description = "Sparse frontier with fewer routes and more dangerous worlds."
system_count_multiplier = 0.75
world_count_bias = "low"
route_density_multiplier = 0.6
world_type_weight_modifiers = { DeathWorld = 1.5, FrontierWorld = 2.0, ForgeWorld = 0.7 }
notable_feature_weight_modifiers = { HostileXenos = 1.6, WarpPhenomena = 1.4, Freeport = 1.3 }

[profiles.coreward]
description = "Dense, developed region."
system_count_multiplier = 1.2
world_count_bias = "high"
route_density_multiplier = 1.4
world_type_weight_modifiers = { HiveWorld = 1.8, ForgeWorld = 1.5, AgriWorld = 1.3 }
```

---

## 8. Core Domain Model for Generated Sector

The generated sector model must be separate from `worlds.rs`. `worlds.rs` describes worlds and template rows. The sector model describes generated output.

### 8.1 Identifier types

Use stable string IDs instead of random UUIDs.

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct SectorId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct SystemId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct WorldId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct FactionId(pub String);
```

ID format requirements:

- Sector IDs come from config.
- System IDs should be deterministic: `sys-0001`, `sys-0002`, etc.
- World IDs should be deterministic within system: `sys-0001-w01`, `sys-0001-w02`, etc.
- Route IDs should be deterministic: `route-sys-0001-sys-0002`, with the lower system ID first.

### 8.2 Coordinates

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}
```

Use axial hex coordinates by default. The exported Markdown may also show user-friendly grid coordinates.

### 8.3 Generated sector

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeneratedSector {
    pub id: String,
    pub title: String,
    pub seed: String,
    pub generator_version: String,
    pub width: u32,
    pub height: u32,
    pub systems: Vec<GeneratedSystem>,
    pub routes: Vec<GeneratedRoute>,
    pub factions: Vec<GeneratedFaction>,
    pub manifest: GenerationManifest,
}
```

### 8.4 Generated system

Avoid naming this type `System` because `worlds.rs` already defines `System`.

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeneratedSystem {
    pub id: String,
    pub index: usize,
    pub name: String,
    pub coord: HexCoord,
    pub star: GeneratedStar,
    pub worlds: Vec<GeneratedWorld>,
    pub primary_factions: Vec<String>,
    pub tags: Vec<String>,
    pub notes: Vec<String>,
}
```

### 8.5 Generated star

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeneratedStar {
    pub colour_code: String,
    pub colour_name: String,
    pub spectral_type: Option<String>,
    pub source_row_index: Option<usize>,
}
```

`colour_code` and `colour_name` must come from `worlds::StarColour::code()` and `worlds::StarColour::short_name()`.

`spectral_type` may come from workbook metadata if an extended parser reads it, or from an additional input table if available.

### 8.6 Generated world

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeneratedWorld {
    pub id: String,
    pub index: usize,
    pub name: String,
    pub orbit: u8,
    pub source_row_index: usize,
    pub world: WorldDto,
    pub factions: Vec<WorldFactionPresence>,
    pub tags: Vec<String>,
    pub notes: Vec<String>,
}
```

`world` should represent a resolved `crate::worlds::World` either directly or through `WorldDto`.

### 8.7 Generated route

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeneratedRoute {
    pub id: String,
    pub from_system_id: String,
    pub to_system_id: String,
    pub distance: u32,
    pub route_type: RouteType,
    pub stability: RouteStability,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteType {
    StableWarpLane,
    ChartedPassage,
    DangerousPassage,
    SecretPassage,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteStability {
    Stable,
    Unstable,
    Hazardous,
    Lost,
}
```

---

## 9. World Candidate Pool Adapter

The most important new component is `world_pool.rs`. It adapts `worlds::GenerationRow` into a weighted candidate pool suitable for deterministic selection.

### 9.1 Candidate type

```rust
#[derive(Debug, Clone)]
pub struct WorldCandidate {
    pub row_index: usize,
    pub star_colour: crate::worlds::StarColour,
    pub world_type: crate::worlds::WorldType,
    pub atmosphere: crate::worlds::Atmosphere,
    pub temperature: crate::worlds::Temperature,
    pub biosphere: crate::worlds::Biosphere,
    pub population: crate::worlds::Population,
    pub tech_level: crate::worlds::TechLevel,
    pub government: crate::worlds::Government,
    pub primary_feature: Option<crate::worlds::NotableFeature>,
    pub counter: Option<usize>,
    pub weight: f64,
}
```

### 9.2 Candidate construction rules

Given `Vec<worlds::GenerationRow>`:

1. Iterate rows with zero-based `row_index` after the skipped workbook header.
2. For `require_complete_rows = true`, include only rows where all of these are present:
   - `star_colour`
   - `world_type`
   - `atmosphere`
   - `temperature`
   - `biosphere`
   - `population`
   - `tech`
   - `government`
   - `weight`
3. Exclude rows where `weight <= 0.0` or `weight` is NaN/infinite.
4. Store `notable_feature` as `primary_feature` if present.
5. Report excluded rows in `ValidationReport`.
6. If zero usable candidates remain, generation must fail before RNG use.

### 9.3 Partial-row mode

Partial-row mode is optional and should be disabled by default.

If `allow_partial_rows = true`, missing fields can be resolved using fallback tables. This mode must be explicit because it changes interpretation of the workbook.

Fallback strategy:

- Missing `star_colour`: choose from all available `StarColour` values weighted by observed candidate frequency.
- Missing `world_type`: choose from observed candidate frequency.
- Missing `atmosphere`, `temperature`, `biosphere`, `population`, `tech`, `government`: choose conditionally using already selected fields when enough matching rows exist; otherwise use global frequency.
- Missing `weight`: use `1.0` only if `default_missing_weight = 1.0` is configured.

The first implementation should prefer strict complete-row mode.

### 9.4 Resolving a candidate into `worlds::World`

```rust
impl WorldCandidate {
    pub fn to_world(&self, features: Vec<crate::worlds::NotableFeature>) -> crate::worlds::World {
        crate::worlds::World {
            star_colour: self.star_colour,
            world_type: self.world_type.clone(),
            atmosphere: self.atmosphere.clone(),
            temperature: self.temperature.clone(),
            biosphere: self.biosphere.clone(),
            population: self.population.clone(),
            tech_level: self.tech_level.clone(),
            government: self.government.clone(),
            notable_features: features,
        }
    }
}
```

### 9.5 Feature selection

Each generated world should have `generation.world_feature_count` features. Default: `3`.

Rules:

1. Start with the selected candidate's `primary_feature`, if present.
2. Add additional features from candidate rows and/or `KeyTables.notable_features` until the target count is reached.
3. Avoid duplicate features on the same world.
4. Prefer features seen in rows matching the selected world type.
5. Then prefer features seen in rows matching star colour.
6. Then fall back to global feature frequency.
7. If the available feature pool is smaller than the requested count, use fewer features and add a validation warning.

Recommended internal representation:

```rust
pub struct FeaturePool {
    pub by_world_type: BTreeMap<String, Vec<WeightedFeature>>,
    pub by_star_colour: BTreeMap<String, Vec<WeightedFeature>>,
    pub global: Vec<WeightedFeature>,
}

pub struct WeightedFeature {
    pub feature: crate::worlds::NotableFeature,
    pub weight: f64,
}
```

### 9.6 System-level star colour consistency

By default, a generated system has one primary star colour. Worlds in that system should be biased toward candidates with the same `star_colour`.

Algorithm:

1. Choose a primary star colour for the system by weighted sampling from candidate row weights grouped by `star_colour`.
2. When selecting worlds for that system, multiply candidate weights by `same_star_colour_bias` if candidate.star_colour equals the system primary star colour.
3. If `strict_same_star_colour = true`, filter out all candidates with different star colours.
4. The default should be bias, not strict filtering.

---

## 10. Generation Pipeline

The application must make generation stages explicit. This makes code generation, testing, and debugging easier.

### 10.1 Pipeline overview

```text
load project config
  ↓
resolve project paths
  ↓
load world workbook through worlds.rs
  ↓
load names, factions, routes, profiles
  ↓
validate all input data
  ↓
build deterministic GenerationContext
  ↓
build WorldCandidatePool
  ↓
place star systems in sector grid
  ↓
generate each system
  ↓
generate worlds for each system
  ↓
assign names
  ↓
assign factions and tags
  ↓
generate routes
  ↓
validate generated sector invariants
  ↓
export files
```

### 10.2 Generation context

```rust
pub struct GenerationContext {
    pub config: AppConfig,
    pub root_seed: String,
    pub root_seed_hash: [u8; 32],
    pub world_tables: crate::worlds::KeyTables,
    pub world_rows: Vec<crate::worlds::GenerationRow>,
    pub world_pool: WorldCandidatePool,
    pub names: NameTables,
    pub factions: Vec<FactionDef>,
    pub route_rules: RouteRules,
}
```

### 10.3 Stage RNGs

Use deterministic stage seeds.

```rust
pub fn derive_stage_seed(root_seed: &str, stage: &str, discriminator: &str) -> [u8; 32] {
    let input = format!("sectorforge:{root_seed}:{stage}:{discriminator}");
    *blake3::hash(input.as_bytes()).as_bytes()
}
```

Examples:

- `stage = "placement"`, `discriminator = "sector"`
- `stage = "system"`, `discriminator = "sys-0001"`
- `stage = "world"`, `discriminator = "sys-0001-w01"`
- `stage = "routes"`, `discriminator = "sector"`

Use `ChaCha8Rng` or `ChaCha20Rng` seeded from the 32-byte hash.

---

## 11. System Placement

### 11.1 Coordinate space

Use an axial hex grid with `q` from `0..sector_width` and `r` from `0..sector_height`.

### 11.2 Placement modes

Supported modes:

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementMode {
    UniformGrid,
    WeightedGrid,
    Clustered,
}
```

#### Uniform grid

Select `system_count` unique coordinates uniformly from all available coordinates.

#### Weighted grid

Bias toward configured subsectors or center/edge weights.

#### Clustered

Create several anchor points, then place systems around them with distance falloff.

### 11.3 Placement validation

Before placing systems:

- Ensure `sector_width * sector_height >= system_count` unless empty hexes are disabled.
- Ensure `minimum_system_distance` is not impossible.
- Ensure subsector dimensions divide sector dimensions or define how partial subsectors are handled.

After placing systems:

- No duplicate coordinates.
- Number of systems equals requested count.
- All coordinates are in bounds.
- Systems sorted by deterministic index.

---

## 12. Star and System Generation

### 12.1 System count

The final system count is:

```text
final_system_count = round(config.generation.system_count * profile.system_count_multiplier)
```

Clamp to available grid cells.

### 12.2 System naming

Priority order:

1. If the workbook metadata parser provides a location name for a selected system seed, use it when `reuse_workbook_location_names = true`.
2. Else choose a `single_names` entry if available.
3. Else combine prefix + suffix.
4. Else fallback to `System {index}`.

Names must be unique. If a generated name duplicates an earlier system name, append a deterministic suffix:

```text
Acheron Reach
Acheron Reach II
Acheron Reach III
```

### 12.3 Star colour

Choose a system primary star colour by weighted sampling from the world candidate pool, grouped by `StarColour`.

Export both:

- `colour_code` from `StarColour::code()`
- `colour_name` from `StarColour::short_name()`

### 12.4 Star spectral type

The current `worlds.rs` comments mention star type metadata in the workbook, but the current `load_generation_rows` function does not expose it. Therefore support these options:

1. **Extended workbook parser:** add a new function that parses columns L-N into metadata records without changing `load_generation_rows`.
2. **External star type table:** allow `data/stars/star_types.toml` to provide weighted spectral labels.
3. **Derived fallback:** derive a simple spectral label from `StarColour`.

Fallback mapping:

| StarColour | Fallback spectral type |
|---|---|
| OrangeDwarf | `O-dwarf` |
| BlueWhite | `B` |
| Amber | `A` |
| Fuchsia | `F` |
| Green | `G` |
| Khaki | `K` |
| Maroon | `M` |

This fallback is not astrophysically strict. It is a setting-friendly label derived from the existing enum naming.

---

## 13. World Generation

### 13.1 World count per system

Use a configurable distribution.

```toml
[generation.world_count_distribution]
1 = 10
2 = 16
3 = 20
4 = 18
5 = 14
6 = 10
7 = 7
8 = 5
```

If absent, use a default distribution bounded by `min_worlds_per_system` and `max_worlds_per_system`.

### 13.2 World candidate filtering

For each world:

1. Start with all usable `WorldCandidate`s.
2. Apply profile weight modifiers.
3. Apply system star colour bias or filter.
4. Apply optional per-system constraints.
5. Sample by final weight.
6. Select notable features.
7. Convert candidate plus features into `worlds::World`.
8. Wrap as `GeneratedWorld` with ID, orbit index, name, source row index, tags, and faction presence.

### 13.3 Weighted selection pseudocode

```rust
pub fn choose_world_candidate(
    pool: &WorldCandidatePool,
    system_star_colour: StarColour,
    config: &WorldSelectionConfig,
    rng: &mut impl Rng,
) -> Result<&WorldCandidate, SectorError> {
    let mut weighted: Vec<(&WorldCandidate, f64)> = Vec::new();

    for candidate in pool.candidates.iter() {
        let mut weight = candidate.weight;

        if config.strict_same_star_colour && candidate.star_colour != system_star_colour {
            continue;
        }

        if candidate.star_colour == system_star_colour {
            weight *= config.same_star_colour_bias;
        }

        if weight.is_finite() && weight > 0.0 {
            weighted.push((candidate, weight));
        }
    }

    weighted_choice(weighted, rng)
}
```

### 13.4 Orbit assignment

Orbit numbers should be deterministic and simple.

Default:

- Worlds are assigned orbits `1..=world_count` in generation order.
- Optionally sort after generation by broad temperature habitability:
  - Freezing/Cold farther out
  - Temperate middle
  - Hot/Boiling inner
- Keep sorting deterministic and document the rule in output manifest.

First implementation recommendation: assign orbit by generation order and avoid simulated orbital sorting.

### 13.5 World naming

Priority order:

1. Workbook location name if exposed and configured.
2. Name pool specific to world type.
3. Generic world name pool.
4. System name + Roman numeral.

Example:

```text
Acheron Reach I
Acheron Reach II
Acheron Reach III
```

### 13.6 Tags generated from world fields

Tags should be stable lower-case strings.

Examples:

- `world_type:hive_world`
- `atmosphere:toxic`
- `temperature:freezing`
- `population:extremely_dense`
- `tech:archaeotech`
- `feature:warp_phenomena`
- `feature:trade_hub`

The implementation should provide a helper to convert enum variant names into stable snake_case.

---

## 14. Faction Assignment

Faction assignment is optional but recommended.

### 14.1 Faction definition

```rust
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct FactionDef {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub weight: f64,
    pub default_disposition: String,
    pub preferred_world_types: Vec<String>,
    pub preferred_governments: Vec<String>,
    pub preferred_notable_features: Vec<String>,
}
```

### 14.2 Assignment algorithm

For each generated world:

1. Start with all faction definitions.
2. Base weight is `faction.weight`.
3. Multiply by `1.5` for matching world type.
4. Multiply by `1.4` for matching government.
5. Multiply by `1.3` for each matching notable feature.
6. Select zero to three factions depending on population and world type.
7. Avoid exact duplicate faction entries on one world.
8. Add selected factions to the system-level `primary_factions` list if they appear on multiple worlds in that system.

### 14.3 Presence type

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorldFactionPresence {
    pub faction_id: String,
    pub influence: FactionInfluence,
    pub relationship_to_government: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactionInfluence {
    Hidden,
    Minor,
    Significant,
    Dominant,
}
```

---

## 15. Route Generation

Routes connect systems. They should be generated after systems and worlds because world features can affect route weights.

### 15.1 Candidate route selection

For every pair of systems:

1. Compute hex distance.
2. Exclude if distance exceeds `max_route_distance`.
3. Start with route weight from route rules.
4. Increase weight if either system has TradeHub, Freeport, MajorSpaceyard, AdministrativeHub, or SubsectorHegemon features.
5. Decrease weight if either system has WarpPhenomena, Quarantined, WarZone, or DaemonicCorruption.
6. Sample routes until target density is reached.
7. If `ensure_connected_graph = true`, add minimum spanning or nearest-neighbor links to connect isolated components.

### 15.2 Route distance

Axial hex distance:

```rust
pub fn hex_distance(a: HexCoord, b: HexCoord) -> u32 {
    let dq = (a.q - b.q).abs();
    let dr = (a.r - b.r).abs();
    let ds = ((-a.q - a.r) - (-b.q - b.r)).abs();
    dq.max(dr).max(ds) as u32
}
```

### 15.3 Route stability

Default stability rules:

- Route involving `WarpPhenomena`: likely `Hazardous`
- Route involving `WarZone`: likely `Unstable`
- Route involving `TradeHub`: likely `Stable`
- Long route near `max_route_distance`: lower stability
- Otherwise `ChartedPassage` with `Stable` or `Unstable`

---

## 16. Validation

Validation must happen before generation and after generation.

### 16.1 Input validation

Validate project config:

- Required sections exist.
- Referenced files exist.
- Numeric values are in valid ranges.
- Output formats are known.
- `system_count` fits inside grid.
- `min_worlds_per_system <= max_worlds_per_system`.

Validate world workbook through `worlds.rs`:

- Workbook can be opened.
- `Key` sheet exists.
- `Generator Template` sheet exists.
- `load_generation_rows` succeeds.
- At least one key table entry exists for every major world dimension.
- At least one generation row is loaded.
- At least one row becomes a usable `WorldCandidate`.
- Report rows with missing required fields.
- Report rows with missing, zero, negative, NaN, or infinite weights.

Validate factions:

- IDs are unique.
- Weights are positive.
- Preferred world type strings map to known `worlds::WorldType` values or accepted variant names.
- Preferred government strings map to known `worlds::Government` values or accepted variant names.
- Preferred feature strings map to known `worlds::NotableFeature` values or accepted variant names.

Validate route rules:

- Multipliers are positive.
- Conditions refer to known fields.
- Distance values are possible for the configured grid.

### 16.2 Validation report

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationReport {
    pub ok: bool,
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
    pub world_workbook: WorldWorkbookValidation,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
    pub row: Option<usize>,
    pub severity: Severity,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorldWorkbookValidation {
    pub row_count: usize,
    pub usable_candidate_count: usize,
    pub excluded_row_count: usize,
    pub key_table_counts: BTreeMap<String, usize>,
}
```

### 16.3 Error policy

- `validate` command should print all errors and warnings and exit nonzero if errors exist.
- `generate` should run validation first.
- `generate --allow-warnings` may continue with warnings.
- `generate` must never continue with errors.

---

## 17. Error Types

Core library should use a shared error enum.

```rust
#[derive(Debug, thiserror::Error)]
pub enum SectorError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config at {path}: {message}")]
    ConfigParse {
        path: String,
        message: String,
    },

    #[error("failed to load world workbook at {path}: {message}")]
    WorldWorkbookLoad {
        path: String,
        message: String,
    },

    #[error("validation failed with {error_count} errors")]
    ValidationFailed {
        error_count: usize,
        warning_count: usize,
    },

    #[error("no usable world candidates were found")]
    NoWorldCandidates,

    #[error("weighted selection failed for {context}")]
    WeightedSelectionFailed {
        context: String,
    },

    #[error("export failed for {path}: {message}")]
    ExportFailed {
        path: String,
        message: String,
    },
}
```

### 17.1 Wrapping existing `worlds.rs` errors

`worlds::load_generation_rows` returns `Result<(KeyTables, Vec<GenerationRow>), String>`. Wrap the string:

```rust
let (tables, rows) = crate::worlds::load_generation_rows(workbook_path)
    .map_err(|message| SectorError::WorldWorkbookLoad {
        path: workbook_path.to_string(),
        message,
    })?;
```

---

## 18. CLI Specification

Use `clap` derive.

### 18.1 Commands

```text
sectorforge validate --project <DIR>
sectorforge generate --project <DIR> [--seed <SEED>] [--profile <PROFILE>] [--out <DIR>]
sectorforge inspect-worlds --workbook <XLSX>
sectorforge schema [--format json]
sectorforge explain --project <DIR> --system <SYSTEM_ID>
```

### 18.2 `validate`

Loads all input files and prints validation results.

Options:

```text
--project <DIR>       Project directory containing sectorforge.toml
--json                Print validation report as JSON
--strict              Treat warnings as errors
```

### 18.3 `generate`

Generates the sector.

Options:

```text
--project <DIR>       Project directory
--seed <SEED>         Override seed from config
--profile <PROFILE>   Use generation profile
--out <DIR>           Override output directory
--format <FORMAT>     Repeatable; json, markdown, csv
--allow-warnings      Continue if validation warnings exist
```

### 18.4 `inspect-worlds`

Debug command for the existing workbook and `worlds.rs` parser.

Output should include:

- key table counts
- generation row count
- usable candidate count
- excluded row count
- top star colours by weight
- top world types by weight
- top notable features by frequency

Example:

```text
World workbook: data/worlds/m42_sector_generator.xlsx
Key tables:
  star_colours: 7
  world_types: 23
  atmospheres: 7
  temperatures: 5
  biospheres: 6
  populations: 6
  tech_levels: 6
  governments: 30
  notable_features: 101
Generator rows: 1482
Usable candidates: 1468
Excluded rows: 14
```

### 18.5 `schema`

Writes JSON Schemas for config and output DTOs.

Outputs:

- `sectorforge-config.schema.json`
- `sector-output.schema.json`
- `validation-report.schema.json`

---

## 19. Output Files

### 19.1 Output directory

Default output layout:

```text
out/
  manifest.json
  validation_report.json
  sector.json
  sector.md
  systems/
    sys-0001.json
    sys-0002.json
  csv/
    systems.csv
    worlds.csv
    routes.csv
```

### 19.2 Manifest

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct GenerationManifest {
    pub project_id: String,
    pub generated_at_policy: String,
    pub generator_name: String,
    pub generator_version: String,
    pub seed: String,
    pub profile: Option<String>,
    pub input_digests: BTreeMap<String, String>,
    pub settings_digest: String,
    pub system_count: usize,
    pub world_count: usize,
    pub route_count: usize,
}
```

Because deterministic output is required, `generated_at_policy` should default to `
not recorded by default`. If a timestamp is needed, it must be opt-in and must be excluded from golden deterministic tests.

### 19.3 JSON sector output

`sector.json` must be the canonical machine-readable output.

Minimum shape:

```json
{
  "id": "m42-sector",
  "title": "M42 Generated Sector",
  "seed": "m42-default-seed",
  "generator_version": "0.1.0",
  "width": 8,
  "height": 10,
  "systems": [
    {
      "id": "sys-0001",
      "index": 1,
      "name": "Acheron Reach",
      "coord": { "q": 3, "r": 5 },
      "star": {
        "colour_code": "M",
        "colour_name": "maroon",
        "spectral_type": "M",
        "source_row_index": 42
      },
      "worlds": [
        {
          "id": "sys-0001-w01",
          "index": 1,
          "name": "Acheron Reach I",
          "orbit": 1,
          "source_row_index": 42,
          "world": {
            "star_colour": "maroon",
            "star_colour_code": "M",
            "world_type": "HiveWorld",
            "atmosphere": "Toxic",
            "temperature": "Temperate",
            "biosphere": "Poisoned",
            "population": "ExtremelyDense",
            "tech_level": "Standard",
            "government": "MilitaryGovernor",
            "notable_features": ["TradeHub", "PoliceState", "HeavyIndustry"]
          },
          "factions": [],
          "tags": ["world_type:hive_world", "feature:trade_hub"],
          "notes": []
        }
      ],
      "primary_factions": [],
      "tags": [],
      "notes": []
    }
  ],
  "routes": [],
  "factions": [],
  "manifest": {}
}
```

### 19.4 Markdown output

`sector.md` should be readable by a human.

Required sections:

1. Title and seed
2. Summary counts
3. Sector map table
4. System index
5. One section per system
6. Routes
7. Factions
8. Generation notes and warnings

Example system section:

```markdown
## SYS-0001 — Acheron Reach

- **Coordinates:** q=3, r=5
- **Star:** M / maroon / M
- **Primary factions:** Imperial Administration, Free Trader Compact

| Orbit | World | Type | Atmosphere | Population | Tech | Government | Features |
|---:|---|---|---|---|---|---|---|
| 1 | Acheron Reach I | HiveWorld | Toxic | ExtremelyDense | Standard | MilitaryGovernor | TradeHub; PoliceState; HeavyIndustry |
| 2 | Acheron Reach II | Orbital | Airless | Minimal | High | MechanicusForgeLord | MajorSpaceyard; LocalTech; NavalOutpost |
```

### 19.5 CSV outputs

CSV is optional but useful for spreadsheets.

`systems.csv` columns:

```text
id,index,name,q,r,star_colour_code,star_colour_name,spectral_type,world_count,primary_factions,tags
```

`worlds.csv` columns:

```text
id,system_id,index,name,orbit,source_row_index,star_colour_code,world_type,atmosphere,temperature,biosphere,population,tech_level,government,notable_features,factions,tags
```

`routes.csv` columns:

```text
id,from_system_id,to_system_id,distance,route_type,stability,tags
```

---

## 20. Input Digest and Reproducibility

### 20.1 Digest requirements

The manifest must include BLAKE3 or SHA-256 digests for:

- `sectorforge.toml`
- world workbook `.xlsx`
- names files
- factions files
- route rules files
- generation profiles

Digest map key should be the normalized project-relative path.

Example:

```json
{
  "input_digests": {
    "sectorforge.toml": "blake3:...",
    "data/worlds/m42_sector_generator.xlsx": "blake3:...",
    "data/names/system_names.toml": "blake3:..."
  }
}
```

### 20.2 Determinism test

A golden deterministic test must:

1. Copy a fixture project into a temp dir.
2. Run generation twice with the same seed.
3. Compare `sector.json` exactly after excluding explicitly nondeterministic fields, if any.
4. Confirm generated system/world/route counts.

---

## 21. Configuration Structs

### 21.1 Main config structs

```rust
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct AppConfig {
    pub project: ProjectConfig,
    pub inputs: InputConfig,
    pub generation: GenerationConfig,
    pub outputs: OutputConfig,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct ProjectConfig {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct InputConfig {
    pub world_workbook: String,
    pub system_names: Option<String>,
    pub world_names: Option<String>,
    pub factions: Option<String>,
    pub route_rules: Option<String>,
    pub generation_profiles: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct GenerationConfig {
    pub seed: String,
    pub sector_width: u32,
    pub sector_height: u32,
    pub subsector_width: Option<u32>,
    pub subsector_height: Option<u32>,
    pub system_count: usize,
    pub min_worlds_per_system: usize,
    pub max_worlds_per_system: usize,
    pub allow_empty_hexes: bool,
    pub world_feature_count: usize,
    pub strict_world_rows: bool,
    pub placement: PlacementConfig,
    pub world_selection: WorldSelectionConfig,
    pub routes: RouteGenerationConfig,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct OutputConfig {
    pub directory: String,
    pub formats: Vec<OutputFormat>,
    pub pretty_json: bool,
    pub write_per_system_files: bool,
    pub write_manifest: bool,
}
```

### 21.2 Selection config

```rust
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct WorldSelectionConfig {
    pub mode: WorldSelectionMode,
    pub require_complete_rows: bool,
    pub allow_partial_rows: bool,
    pub same_star_colour_bias: f64,
    pub strict_same_star_colour: bool,
    pub avoid_duplicate_world_type_in_system: bool,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorldSelectionMode {
    WeightedRows,
}
```

---

## 22. Library API

Expose a stable API from `src/lib.rs`.

```rust
pub mod worlds;
pub mod config;
pub mod errors;
pub mod validation;
pub mod world_pool;
pub mod sector_model;
pub mod generation;
pub mod export;

pub use config::AppConfig;
pub use errors::SectorError;
pub use sector_model::GeneratedSector;

pub fn load_project(path: impl AsRef<Path>) -> Result<ProjectInput, SectorError>;

pub fn validate_project(input: &ProjectInput) -> Result<ValidationReport, SectorError>;

pub fn generate_sector(input: ProjectInput) -> Result<GeneratedSector, SectorError>;

pub fn export_sector(
    sector: &GeneratedSector,
    output_config: &OutputConfig,
    output_dir: impl AsRef<Path>,
) -> Result<(), SectorError>;
```

### 22.1 ProjectInput

```rust
pub struct ProjectInput {
    pub root_dir: camino::Utf8PathBuf,
    pub config: AppConfig,
    pub world_tables: crate::worlds::KeyTables,
    pub world_rows: Vec<crate::worlds::GenerationRow>,
    pub names: NameTables,
    pub factions: Vec<FactionDef>,
    pub route_rules: RouteRules,
    pub profiles: GenerationProfiles,
    pub input_digests: BTreeMap<String, String>,
}
```

---

## 23. Workbook Metadata Extension

The existing `worlds.rs` comments refer to system metadata like system seed, star type, and location name. The current row loader does not expose all metadata needed by the broader sector generator.

Implement this as an extension rather than changing the meaning of `GenerationRow`.

### 23.1 Template metadata type

```rust
#[derive(Debug, Clone)]
pub struct TemplateMetadataRow {
    pub row_index: usize,
    pub system_seed: Option<f64>,
    pub star_type: Option<String>,
    pub location_name: Option<String>,
}
```

### 23.2 Loader

```rust
pub fn load_template_metadata(path: &str) -> Result<Vec<TemplateMetadataRow>, SectorError> {
    // Open workbook with calamine.
    // Find "Generator Template" sheet.
    // Skip header.
    // Parse columns L, M, N.
    // Return one metadata row per generation row using the same row_index convention as world_pool.
}
```

### 23.3 Joining metadata to world candidates

Join by `row_index`.

```rust
pub struct WorldCandidate {
    pub row_index: usize,
    pub metadata: Option<TemplateMetadataRow>,
    // existing candidate fields...
}
```

Do not require metadata for generation. Missing metadata should not invalidate a row.

---

## 24. Compatibility Rules and Profiles

The first implementation can select rows directly from workbook weights. Later implementations may add compatibility adjustments.

### 24.1 Compatibility rule type

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CompatibilityRule {
    pub when: CompatibilityCondition,
    pub modifier: f64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CompatibilityCondition {
    pub star_colour: Option<String>,
    pub world_type: Option<String>,
    pub atmosphere: Option<String>,
    pub temperature: Option<String>,
    pub biosphere: Option<String>,
    pub population: Option<String>,
    pub tech_level: Option<String>,
    pub government: Option<String>,
    pub notable_feature: Option<String>,
}
```

### 24.2 Modifier application

For every candidate:

```text
final_weight = base_workbook_weight
             * star_colour_bias
             * profile_world_type_modifier
             * profile_feature_modifier
             * compatibility_modifiers
```

Rules:

- Never allow final weight below zero.
- Ignore zero-weight candidates.
- Treat NaN/infinite as validation errors.
- Record applied profile name in manifest.

---

## 25. Logging and Diagnostics

Use `tracing`.

### 25.1 Log levels

- `error`: fatal errors
- `warn`: validation warnings and skipped optional data
- `info`: loaded file counts, generation summary
- `debug`: candidate pool stats, selected profile, route generation details
- `trace`: individual random choices, only when requested

### 25.2 Diagnostic files

When `outputs.write_diagnostics = true`, write:

```text
out/diagnostics/
  world_candidate_pool.json
  weighted_star_colours.json
  validation_details.json
  rng_stage_seeds.json
```

`rng_stage_seeds.json` must be optional because exposing seeds may be unwanted in some contexts.

---

## 26. Testing Requirements

### 26.1 Unit tests

Required unit tests:

- `world_pool_excludes_rows_missing_weight`
- `world_pool_excludes_rows_with_zero_or_negative_weight`
- `world_pool_requires_complete_rows_when_strict`
- `world_candidate_to_world_preserves_fields`
- `feature_selection_avoids_duplicates`
- `stage_seed_is_stable`
- `hex_distance_known_examples`
- `route_id_orders_system_ids`
- `system_ids_are_stable`
- `world_ids_are_stable`

### 26.2 Integration tests

Required integration tests:

- `validate_fixture_project_succeeds`
- `generate_fixture_project_succeeds`
- `generate_same_seed_same_output`
- `generate_different_seed_different_output`
- `inspect_worlds_reports_candidate_count`
- `missing_workbook_fails_validation`
- `missing_key_sheet_fails_validation`
- `no_usable_world_rows_fails_validation`

### 26.3 Snapshot tests

Use snapshot tests for:

- `sector.md`
- `validation_report.json`
- `manifest.json`
- one generated `sys-0001.json`

Snapshots should use a tiny fixture workbook or mock `GenerationRow` list so tests stay fast.

### 26.4 Testing without Excel

For most unit tests, do not depend on `calamine` or real `.xlsx` files. Build `GenerationRow` values directly in Rust.

Example:

```rust
fn row(weight: f64) -> GenerationRow {
    GenerationRow {
        star_colour: Some(StarColour::Maroon),
        world_type: Some(WorldType::HiveWorld),
        atmosphere: Some(Atmosphere::Toxic),
        temperature: Some(Temperature::Temperate),
        biosphere: Some(Biosphere::Poisoned),
        population: Some(Population::ExtremelyDense),
        tech: Some(TechLevel::Standard),
        government: Some(Government::MilitaryGovernor),
        notable_feature: Some(NotableFeature::TradeHub),
        counter: Some(1),
        weight: Some(weight),
    }
}
```

---

## 27. Implementation Milestones

### Milestone 1: Project loading and `worlds.rs` integration

Deliverables:

- `src/worlds.rs` included unchanged or minimally modified with derives.
- `config.rs` loads `sectorforge.toml`.
- `load_project` resolves paths.
- `worlds::load_generation_rows` is called and wrapped in `SectorError`.
- `inspect-worlds` command prints workbook statistics.

Acceptance criteria:

- Running `sectorforge inspect-worlds --workbook path.xlsx` loads the workbook or prints a clear error.
- Running `sectorforge validate --project examples/m42_project` checks the workbook and config.

### Milestone 2: Candidate pool and validation

Deliverables:

- `world_pool.rs`
- `ValidationReport`
- strict complete-row validation
- feature pool construction

Acceptance criteria:

- Candidate pool excludes invalid rows.
- Validation reports exact counts.
- Unit tests cover missing fields and bad weights.

### Milestone 3: Deterministic sector generation

Deliverables:

- seeded RNG helpers
- coordinate placement
- system generation
- world generation
- stable IDs

Acceptance criteria:

- Same seed produces identical `GeneratedSector`.
- Different seed changes placement and/or selections.
- Generated worlds use `worlds::World` data resolved from workbook rows.

### Milestone 4: Exports

Deliverables:

- JSON export
- Markdown export
- manifest
- optional CSV export

Acceptance criteria:

- `sector.json` contains all generated systems and worlds.
- `sector.md` is readable.
- Manifest includes seed, version, input digests, and counts.

### Milestone 5: Routes and factions

Deliverables:

- faction assignment
- route generation
- route stability rules
- route/faction validation

Acceptance criteria:

- Generated sector contains deterministic routes.
- Factions appear on appropriate worlds based on preferences.
- Route generation respects max distance.

---

## 28. Code Generation Notes

A code generator should follow these practical instructions:

1. Start by creating the crate layout and copying the existing `worlds.rs` into `src/worlds.rs`.
2. Do not generate new definitions for `StarColour`, `WorldType`, `Atmosphere`, `Temperature`, `Biosphere`, `Population`, `TechLevel`, `Government`, or `NotableFeature`.
3. Generate DTOs for serialization instead of forcing `worlds.rs` to derive Serde, unless modifying `worlds.rs` is explicitly allowed.
4. Generate `world_pool.rs` before the main generator. The candidate pool is the bridge between the workbook and sector generation.
5. Generate validation before generation. Generation should consume already-validated input.
6. Keep all filesystem access in project loading and export modules.
7. Keep the generator pure: `generate_sector(input)` should not write files.
8. Keep CLI thin: parse args, call library functions, print user-facing messages.
9. Use deterministic sorted output everywhere.
10. Prefer explicit errors over panics.

---

## 29. Acceptance Criteria for the Finished Application

The application is complete when all of the following are true:

- It compiles with the existing `worlds.rs` module included.
- It can load the M42 workbook through `worlds::load_generation_rows`.
- It can validate the workbook and report candidate-pool statistics.
- It can generate a sector with a fixed seed.
- It can export `sector.json`, `sector.md`, and `manifest.json`.
- It uses stable IDs for sectors, systems, worlds, and routes.
- It does not duplicate the world enum taxonomy.
- It provides a CLI `validate`, `generate`, and `inspect-worlds` command.
- It has unit tests for world pool construction and deterministic RNG.
- It has at least one golden test proving identical output for identical seed and input files.

---

## 30. Minimal First Implementation Scope

If implementing the smallest useful version, build only this:

1. `sectorforge.toml` loader.
2. Existing `worlds.rs` module included as `crate::worlds`.
3. `worlds::load_generation_rows` wrapper.
4. Strict `WorldCandidatePool` from complete weighted rows.
5. Deterministic system placement on a hex grid.
6. Deterministic world selection from weighted candidates.
7. Three notable features per world.
8. JSON and Markdown export.
9. `validate`, `generate`, and `inspect-worlds` commands.

Defer these until later:

- factions
- route generation
- profiles
- workbook metadata columns L-N
- CSV export
- advanced compatibility rules
- subsector maps

---

## 31. Important Edge Cases

The implementation must handle:

- Workbook path missing.
- Workbook exists but has no `Key` sheet.
- Workbook exists but has no `Generator Template` sheet.
- Workbook rows parse but all weights are missing.
- Workbook rows parse but all weights are zero or negative.
- Workbook contains values not recognized by `FromStr` in `worlds.rs`.
- Name pools are empty.
- Requested system count exceeds grid capacity.
- Requested feature count exceeds available feature pool.
- Route graph cannot be connected within max distance.
- Duplicate faction IDs.
- Invalid config enum strings.
- Output directory already exists.

Default policy for output directory:

- Create it if missing.
- Overwrite generated files if `--force` is passed.
- Without `--force`, refuse to overwrite non-empty directories unless files are known generated outputs.

---

## 32. Example Main Flow

```rust
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Command::Validate { project, json, strict } => {
            let input = sectorforge::load_project(project)?;
            let report = sectorforge::validate_project(&input)?;
            print_validation_report(&report, json);
            if !report.ok || (strict && !report.warnings.is_empty()) {
                std::process::exit(1);
            }
        }
        Command::Generate { project, seed, profile, out, allow_warnings } => {
            let mut input = sectorforge::load_project(project)?;
            if let Some(seed) = seed {
                input.config.generation.seed = seed;
            }
            apply_profile_override(&mut input, profile)?;
            let report = sectorforge::validate_project(&input)?;
            if !report.ok || (!allow_warnings && has_blocking_warnings(&report)) {
                print_validation_report(&report, false);
                std::process::exit(1);
            }
            let sector = sectorforge::generate_sector(input)?;
            let output_dir = out.unwrap_or_else(|| sector_output_dir(&sector));
            sectorforge::export_sector(&sector, &sector.manifest.output_config, output_dir)?;
        }
        Command::InspectWorlds { workbook } => {
            let stats = sectorforge::inspect_world_workbook(&workbook)?;
            print_world_stats(stats);
        }
        Command::Schema { format } => {
            write_schemas(format)?;
        }
        Command::Explain { project, system } => {
            explain_generated_system(project, system)?;
        }
    }

    Ok(())
}
```

---

## 33. Final Design Summary

The core idea is simple:

- `worlds.rs` owns world definitions and Excel parsing.
- `world_pool.rs` turns workbook rows into weighted candidates.
- `generation` turns weighted candidates into deterministic systems and worlds.
- `sector_model.rs` defines the generated output structure.
- `export` writes JSON, Markdown, CSV, and manifests.
- `cli` gives users validate/generate/inspect commands.

This keeps the existing world spec intact while adding a robust sector-generation layer around it.
