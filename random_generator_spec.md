# Random System and Random Sector Generator — Rust Implementation Specification

## 1. Purpose

This document defines the requirements for adding a **random system generator** and a **random sector generator** to the existing Rust application. The application already has base data and can already emit sector and system data. The new functionality must therefore be **data-driven**, using the existing catalogs, tables, configuration, and output conventions instead of embedding concrete sample values in code.

The intended consumer of this specification is an LLM or developer that will generate Rust code for the feature.

## 2. Scope

Implement generation of:

1. A single random system.
2. A complete random sector made of multiple systems.
3. Derived sector-level routes, faction summaries, counts, and manifest data.
4. Optional Markdown rendering that matches the existing sector Markdown structure.
5. Deterministic generation from a seed.

The generator must be able to produce JSON with the same structural shape as the existing sector JSON and standalone system JSON outputs. It must also be able to render a Markdown overview compatible with the existing sector Markdown output.

## 3. Non-goals

The implementation must **not**:

- Hard-code concrete star, world, faction, route, name, or feature values from the attached examples.
- Treat the attached generated sector as the only possible sector.
- Require values from the sample output to be copied into source code.
- Invent a new JSON schema unless explicitly versioned and backwards-compatible.
- Store generated timestamps by default if the existing manifest policy says not to.
- Depend on global mutable randomness or nondeterministic iteration order.

## 4. Source Data Assumptions

The base data already exists. The generator should reference it abstractly through typed loaders and catalogs.

Expected input sources include, but are not limited to:

- A sector generation configuration/profile.
- A faction catalog.
- System name source data.
- World name source data.
- World-generation rows or tables.
- Star-generation rows or tables.
- Route-generation rules.
- Existing settings and input digests used for manifest creation.

Do not inline the actual contents of those sources in the generator. The Rust code should load them through existing or newly introduced data access modules.

## 5. Core Design Principles

### 5.1 Data-driven generation

All domain values must come from loaded data catalogs. Examples:

- Star codes, star display names, and spectral types must come from star-related source data.
- World type, atmosphere, temperature, biosphere, population, tech level, government, and notable features must come from world-generation source data.
- Faction IDs, names, kinds, dispositions, and relationship defaults must come from the faction catalog.
- Route type, stability, distance rules, tags, and probability rules must come from route rules.
- Names must come from configured name lists or a composable name generator backed by those lists.

### 5.2 Determinism

Given the same seed, generator version, configuration, and input data digests, the generated output must be byte-stable after pretty JSON serialization, except where the existing manifest policy explicitly allows omitted or variable fields.

Requirements:

- Use a seedable PRNG.
- Derive independent random streams for each subsystem using stable labels.
- Avoid iteration over unordered maps when output order matters.
- Sort derived arrays by stable keys.
- Use stable tie-breakers.

Recommended approach:

```text
master_seed = user_seed
system_stream(seed, system_index)
world_stream(seed, system_index, world_index)
route_stream(seed)
faction_stream(seed, system_index, world_index)
name_stream(seed, namespace, index)
```

### 5.3 Backwards compatibility

The generator must emit the same object layout as the current generated outputs:

- Sector object with metadata, systems, routes, factions, and manifest.
- System object with identity, coordinates, star, worlds, primary factions, tags, and notes.
- World object with identity, orbit/index, source row information, world attributes, factions, tags, and notes.
- Route object with identity, endpoints, distance, type, stability, and tags.
- Faction summary object with identity, descriptive fields, system presence, and world presence.

## 6. Rust Architecture

Recommended module layout:

```text
src/
  data/
    mod.rs
    config.rs
    factions.rs
    names.rs
    routes.rs
    worlds.rs
    stars.rs
  model/
    mod.rs
    sector.rs
    system.rs
    world.rs
    route.rs
    faction.rs
    manifest.rs
  generator/
    mod.rs
    context.rs
    rng.rs
    names.rs
    system.rs
    world.rs
    sector.rs
    routes.rs
    factions.rs
    tags.rs
    manifest.rs
  render/
    mod.rs
    markdown.rs
    map.rs
  validate/
    mod.rs
    invariants.rs
  cli/
    mod.rs
```

Recommended crates:

```toml
serde = { version = "*", features = ["derive"] }
serde_json = "*"
toml = "*"
rand = "*"
rand_chacha = "*"
blake3 = "*"
thiserror = "*"
anyhow = "*"
indexmap = "*"
clap = { version = "*", features = ["derive"] }
```

Use project-pinned versions in the real codebase rather than wildcard versions.

## 7. Data Model Requirements

Use `serde`-serializable Rust structs. Domain values that come from external data should generally remain strings or newtype wrappers around strings, not hard-coded Rust enums, unless the codebase already generates enums from data.

### 7.1 Sector

The generated sector JSON must contain:

```rust
pub struct Sector {
    pub id: String,
    pub title: String,
    pub seed: String,
    pub generator_name: String,
    pub generator_version: String,
    pub width: u32,
    pub height: u32,
    pub systems: Vec<System>,
    pub routes: Vec<Route>,
    pub factions: Vec<FactionPresenceSummary>,
    pub manifest: Manifest,
}
```

Requirements:

- `id` must be stable for a given project/profile unless explicitly provided.
- `title` must be generated or provided by config.
- `seed` must record the user-facing seed string.
- `width` and `height` define the coordinate bounds.
- `systems` must be sorted by `index`.
- `routes` must be sorted by canonical route key.
- `factions` must be sorted by faction ID or catalog order, consistently.
- `manifest` must reflect actual generated counts and source digests.

### 7.2 System

```rust
pub struct System {
    pub id: String,
    pub index: u32,
    pub name: String,
    pub coord: HexCoord,
    pub star: Star,
    pub worlds: Vec<WorldInstance>,
    pub primary_factions: Vec<String>,
    pub tags: Vec<String>,
    pub notes: Vec<String>,
}

pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}

pub struct Star {
    pub colour_code: String,
    pub colour_name: String,
    pub spectral_type: String,
    pub source_row_index: usize,
}
```

Requirements:

- `id` must follow the existing system ID format.
- `index` must be one-based and sequential within the sector.
- `coord` uses axial hex-grid coordinates.
- `name` must be unique within the sector unless config allows reuse.
- `star` must be chosen from loaded star data.
- `worlds` must be sorted by `index` and `orbit`.
- `primary_factions` must be derived, not independently invented.
- `tags` and `notes` may be empty unless generation rules add values.

### 7.3 World Instance

```rust
pub struct WorldInstance {
    pub id: String,
    pub index: u32,
    pub name: String,
    pub orbit: u32,
    pub source_row_index: usize,
    pub world: WorldProfile,
    pub factions: Vec<WorldFactionPresence>,
    pub tags: Vec<String>,
    pub notes: Vec<String>,
}

pub struct WorldProfile {
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

pub struct WorldFactionPresence {
    pub faction_id: String,
    pub influence: String,
    pub relationship_to_government: String,
}
```

Requirements:

- `id` must combine the parent system ID and a stable world index using the existing world ID convention.
- `index` and `orbit` must be one-based and stable.
- `source_row_index` must identify the source row used to create the world profile.
- `world` fields must be copied or transformed only according to configured source-data rules.
- `notable_features` must preserve the configured number/order policy.
- `factions` must reference valid faction IDs.
- `tags` must be derived from the world profile and faction-independent world attributes.
- `notes` may be empty unless generation rules add values.

Important: do not assume the world profile’s star fields must always be identical to the parent system star fields unless the loaded rules explicitly require that. Preserve the existing source-data semantics.

### 7.4 Route

```rust
pub struct Route {
    pub id: String,
    pub from_system_id: String,
    pub to_system_id: String,
    pub distance: u32,
    pub route_type: String,
    pub stability: String,
    pub tags: Vec<String>,
}
```

Requirements:

- Route endpoints must reference existing system IDs.
- Route IDs must be canonical and stable.
- Store each undirected route once.
- Canonicalize endpoint order before generating the ID.
- `distance` must be calculated from axial hex coordinates.
- `route_type`, `stability`, and `tags` must come from route rules.
- Routes must not self-reference.
- Duplicate routes are invalid.

### 7.5 Faction Presence Summary

```rust
pub struct FactionPresenceSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub disposition: String,
    pub system_presence: Vec<String>,
    pub world_presence: Vec<String>,
}
```

Requirements:

- Start from the faction catalog.
- Include only factions with generated presence, unless config says to emit all factions.
- `system_presence` must contain systems where the faction appears in at least one world or is otherwise assigned by a valid rule.
- `world_presence` must contain worlds where the faction appears.
- Presence arrays must be sorted by canonical generated ID order.
- Summary descriptive fields must come from the catalog.

### 7.6 Manifest

```rust
pub struct Manifest {
    pub project_id: String,
    pub generated_at_policy: String,
    pub generator_name: String,
    pub generator_version: String,
    pub seed: String,
    pub seed_hash: String,
    pub profile: Option<String>,
    pub input_digests: BTreeMap<String, String>,
    pub settings_digest: String,
    pub system_count: u32,
    pub world_count: u32,
    pub route_count: u32,
}
```

Requirements:

- `system_count`, `world_count`, and `route_count` must match generated arrays.
- `input_digests` must reflect the exact source data used.
- `settings_digest` must reflect generation settings.
- `seed_hash` must be computed from the seed using the existing digest convention.
- `generated_at_policy` must follow the current project behavior.

## 8. Generator Inputs

Create a top-level request type:

```rust
pub struct SectorGenerationRequest {
    pub project_id: String,
    pub title: Option<String>,
    pub seed: String,
    pub profile: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub system_count: Option<u32>,
    pub output_markdown: bool,
}
```

Create a system-level request type:

```rust
pub struct SystemGenerationRequest {
    pub index: u32,
    pub system_id: Option<String>,
    pub coord: Option<HexCoord>,
    pub name_hint: Option<String>,
    pub seed: String,
    pub profile: Option<String>,
}
```

The request object may be expanded, but it must remain clear which fields are user-provided and which are generated.

## 9. Generation Context

All generator functions should share a context object:

```rust
pub struct GenerationContext {
    pub seed: String,
    pub profile: Option<String>,
    pub config: GenerationConfig,
    pub catalogs: CatalogBundle,
    pub rng: RngManager,
    pub used_system_names: NameRegistry,
    pub used_world_names: NameRegistry,
}
```

The context must provide:

- Access to source catalogs.
- Deterministic random streams.
- ID allocation.
- Name uniqueness tracking.
- Validation hooks.
- Digest calculation.

## 10. Random System Generator Requirements

### 10.1 Public API

Provide an API similar to:

```rust
pub fn generate_system(
    ctx: &mut GenerationContext,
    request: SystemGenerationRequest,
) -> Result<System, GenerationError>;
```

For sector generation, also provide an internal API that accepts preallocated coordinates and an existing system index:

```rust
pub fn generate_system_for_sector(
    ctx: &mut GenerationContext,
    sector_id: &str,
    index: u32,
    coord: HexCoord,
) -> Result<System, GenerationError>;
```

### 10.2 System generation steps

The system generator must:

1. Allocate or validate the system index.
2. Allocate or validate the system ID.
3. Allocate or validate the axial coordinate.
4. Generate a unique system name.
5. Select a star row from the star source data.
6. Build the `Star` object from the selected source row.
7. Determine how many worlds the system contains.
8. Generate each world in orbit order.
9. Assign factions to generated worlds.
10. Generate tags for each world.
11. Derive system-level primary factions.
12. Validate the final system.
13. Return a fully populated `System`.

### 10.3 World count selection

World count must be data/config-driven.

Acceptable strategies:

- Weighted distribution from configuration.
- Distribution inferred from a generation profile.
- A fixed count supplied in the request for testing.
- A min/max range with weighted bias.

Do not derive world count by copying a specific example system.

### 10.4 Star selection

Star selection must use loaded star data.

Requirements:

- Weighted sampling if source data provides weights.
- Uniform sampling only if no weights exist.
- Preserve `source_row_index`.
- Copy star fields from the selected source row.
- Validate that all required star fields are present.

### 10.5 World profile selection

For each world:

1. Select a world source row using the world-generation data.
2. Apply any configured constraints:
   - Orbit position.
   - Parent star rule.
   - Sector profile.
   - Rarity/weight.
   - Faction or route influence hooks.
3. Copy the relevant world profile fields into `WorldProfile`.
4. Preserve `source_row_index`.
5. Select or preserve notable features according to source-data rules.
6. Generate the world name.
7. Generate world faction presence.
8. Generate tags.

### 10.6 Orbit and index rules

World `index` and `orbit` must:

- Start at one.
- Increase monotonically.
- Match each other unless configuration supports non-contiguous or special orbits.
- Be stable for the same seed and source data.

### 10.7 World naming

World names must come from the existing name source data or configured name generator.

Requirements:

- Avoid duplicate world names within a system unless config allows duplicates.
- Optionally avoid duplicate world names across the sector.
- Support modifiers, prefixes, suffixes, and station-like names if the source name rules support them.
- Use deterministic tie-breaking when a name collision occurs.

### 10.8 Faction assignment per world

World faction assignment must be rules-driven.

Inputs to faction selection may include:

- World type.
- Government.
- Population.
- Tech level.
- Notable features.
- Parent system context.
- Existing faction distribution targets.
- Faction catalog disposition.
- Profile-specific weights.

Requirements:

- Every assigned faction ID must exist in the faction catalog.
- Faction count per world must be configurable.
- Faction influence values must come from a configured ordered tier list.
- At most one faction should have the highest influence tier unless rules explicitly permit otherwise.
- Relationship-to-government should default from faction disposition or a relationship rule table.
- Uninhabited or empty-world profiles may have zero factions if rules allow.
- Factions must be sorted by influence rank and then stable faction order.

### 10.9 Primary faction derivation

System `primary_factions` must be derived from world-level factions.

Suggested scoring:

```text
score = sum(influence_weight for each world presence)
```

Where `influence_weight` comes from config.

Then:

1. Sort by descending score.
2. Resolve ties by number of world appearances.
3. Resolve remaining ties by catalog order or faction ID.
4. Keep the configured maximum number of primary factions.
5. Omit the field’s contents if no factions are present, but keep the array.

### 10.10 Tag generation

World tags must be derived from world profile fields.

Required tag namespaces:

```text
star:
world_type:
atmosphere:
temperature:
biosphere:
population:
tech:
gov:
feature:
```

Requirements:

- Convert source display values to lower snake case.
- Use stable namespace prefixes.
- Generate one tag per scalar world profile field.
- Generate one tag per notable feature.
- Preserve deterministic tag order:
  1. atmosphere
  2. biosphere
  3. feature tags sorted or source-ordered per config
  4. government
  5. population
  6. star
  7. tech
  8. temperature
  9. world type
- Do not add faction tags unless a separate, explicit rule is introduced.
- Deduplicate tags while preserving order.

### 10.11 System validation

A generated system is valid only if:

- The system ID is unique.
- The index is positive.
- The coordinate exists within the intended grid when generated as part of a sector.
- The star object is complete.
- The worlds array is non-empty unless config allows empty systems.
- World IDs are unique.
- World IDs use the parent system ID.
- World indexes and orbits are valid.
- World source row indexes are valid.
- World profile fields are present.
- World factions reference known factions.
- World tags match the world profile.
- Primary factions are derivable from world factions.

## 11. Random Sector Generator Requirements

### 11.1 Public API

Provide an API similar to:

```rust
pub fn generate_sector(
    request: SectorGenerationRequest,
    data_sources: DataSourceBundle,
) -> Result<Sector, GenerationError>;
```

Optionally provide a lower-level API for already-loaded catalogs:

```rust
pub fn generate_sector_with_context(
    ctx: &mut GenerationContext,
    request: SectorGenerationRequest,
) -> Result<Sector, GenerationError>;
```

### 11.2 Sector generation steps

The sector generator must:

1. Load configuration and catalogs.
2. Resolve sector dimensions.
3. Resolve system count or density.
4. Allocate system coordinates on the axial grid.
5. Generate systems for those coordinates.
6. Generate routes between systems.
7. Aggregate faction presence.
8. Build the sector manifest.
9. Validate all invariants.
10. Serialize JSON and optionally render Markdown.

### 11.3 Grid model

The sector grid uses axial hex coordinates with `q` and `r`.

Requirements:

- `q` must be within the configured width.
- `r` must be within the configured height.
- No two systems may occupy the same coordinate.
- Sector map rendering must align with the existing staggered text-map style.
- Coordinate allocation must be deterministic.

### 11.4 System count and density

System count must be configurable.

Acceptable strategies:

- Explicit `system_count`.
- Density percentage applied to grid capacity.
- Profile-derived count.
- Weighted range from config.

Validation:

- System count must not exceed grid capacity.
- System count must be positive unless explicitly generating an empty sector for tests.
- If count is too high for route/connectivity rules, return a clear error.

### 11.5 Coordinate allocation

Coordinate allocation must support:

- Uniform random placement.
- Optional clustering.
- Optional named regions or bands if profile data exists.
- Optional minimum distance between systems.
- Stable ordering by generated system index.

Recommended default:

1. Generate all possible grid coordinates.
2. Shuffle deterministically.
3. Select the configured number of coordinates.
4. Sort selected coordinates by allocation order or configured map order.
5. Assign system indexes in stable order.

### 11.6 Sector-level system generation

For each allocated coordinate:

- Call the random system generator.
- Pass the preassigned index and coordinate.
- Ensure system names remain unique sector-wide.
- Use the same catalog bundle and seed context.
- Collect generated systems into index order.

### 11.7 Route generation

Routes connect generated systems.

Requirements:

- Candidate route endpoints must be valid generated systems.
- Distance must be computed using axial hex distance:

```text
distance = (abs(q1 - q2) + abs(r1 - r2) + abs((q1 + r1) - (q2 + r2))) / 2
```

- Route candidates must be filtered by configured route rules.
- Route type, stability, and tags must be selected from route source data.
- Route IDs must be deterministic and canonical.
- Duplicate undirected edges are invalid.

Recommended route algorithm:

1. Build all candidate edges between systems.
2. Compute axial distance for each candidate.
3. Discard candidates that violate max distance or profile rules.
4. Ensure baseline connectivity if the profile requires a connected sector:
   - Build a minimum spanning tree or nearest-neighbor backbone.
   - Use deterministic tie-breakers.
5. Add additional random routes until route density/target is met.
6. Assign route metadata from route rules.
7. Sort routes by endpoint IDs.

### 11.8 Route metadata rules

Route metadata must be data-driven.

Inputs may include:

- Distance.
- Endpoint system tags.
- Endpoint star data.
- Faction presence.
- Profile settings.
- Random stream.

Rules should determine:

- Route type.
- Stability.
- Optional route tags.

Do not hard-code concrete route types, stability names, or tag names.

### 11.9 Faction aggregation

After all systems and worlds are generated:

1. Iterate all world faction presences.
2. Group by faction ID.
3. For each faction:
   - Load descriptive fields from the faction catalog.
   - Add parent system ID to `system_presence`.
   - Add world ID to `world_presence`.
4. Sort presence arrays by generated ID order.
5. Emit summaries in catalog order or canonical faction ID order.

Validation:

- Every world faction must appear in the sector faction summary.
- Every summary world presence must reference an existing world.
- Every summary system presence must reference an existing system.
- Summary presence must match actual world-level data.

### 11.10 Manifest generation

The sector manifest must be generated last.

Requirements:

- Include project ID, generator name, generator version, seed, seed hash, profile, input digests, settings digest, and generated counts.
- Counts must be calculated from the final generated sector object.
- Input digests must be computed from all loaded source files used in generation.
- Settings digest must be computed from the effective resolved config, not merely the user request.
- The generated-at behavior must match the current manifest policy.

### 11.11 Sector validation

A sector is valid only if:

- Top-level counts match the generated arrays.
- All system IDs are unique.
- All system indexes are one-based and sequential.
- All coordinates are within bounds.
- All coordinates are unique.
- All world IDs are unique across the sector.
- All routes reference existing systems.
- All routes have accurate axial distances.
- All routes are unique when treated as undirected edges.
- All faction presences are coherent.
- All tags can be regenerated from their source fields.
- Manifest counts match actual counts.
- Serialization round-trips through `serde_json`.

## 12. Markdown Rendering Requirements

The Markdown renderer should be separate from generation.

Public API:

```rust
pub fn render_sector_markdown(sector: &Sector) -> String;
```

The renderer must produce:

1. Title heading containing sector ID and sector title.
2. Seed line.
3. Generator line.
4. Sector summary list:
   - Sector size.
   - System count.
   - World count.
   - Route count.
   - Faction count.
5. Sector map code block.
6. System index table.
7. Per-system sections with world tables.
8. Routes table.
9. Factions table.

Requirements:

- Markdown output must be deterministic.
- It must not perform generation.
- It must not mutate the sector object.
- It must render empty arrays gracefully.
- Display formatting may transform machine values into display values, but JSON values must remain unchanged.

## 13. Error Handling

Create a domain error type:

```rust
#[derive(thiserror::Error, Debug)]
pub enum GenerationError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("data source error: {0}")]
    DataSource(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("name generation failed: {0}")]
    NameGeneration(String),

    #[error("route generation failed: {0}")]
    RouteGeneration(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}
```

Requirements:

- Do not panic on malformed input data.
- Return actionable errors.
- Include enough context to find the failing source row or generated object.
- Avoid leaking unrelated internal details in user-facing CLI output.

## 14. Configuration Requirements

Create or extend a generation config type:

```rust
pub struct GenerationConfig {
    pub generator_name: String,
    pub generator_version: String,
    pub default_width: u32,
    pub default_height: u32,
    pub system_count: CountRule,
    pub world_count: CountRule,
    pub coordinate_policy: CoordinatePolicy,
    pub route_policy: RoutePolicy,
    pub faction_policy: FactionPolicy,
    pub naming_policy: NamingPolicy,
    pub tag_policy: TagPolicy,
    pub manifest_policy: ManifestPolicy,
}
```

All policies should be serializable/deserializable so the effective settings can be digested.

### 14.1 Count rules

Support:

```rust
pub enum CountRule {
    Fixed(u32),
    Range { min: u32, max: u32 },
    Weighted(Vec<WeightedCount>),
    Density { numerator: u32, denominator: u32 },
}
```

### 14.2 Weighted selection

Introduce a reusable weighted table abstraction:

```rust
pub struct Weighted<T> {
    pub item: T,
    pub weight: u32,
}
```

Requirements:

- Reject empty weighted tables.
- Reject all-zero weighted tables.
- Use deterministic selection from the relevant RNG stream.
- Preserve source row indexes.

## 15. Serialization Requirements

JSON serialization must:

- Use snake_case field names where current JSON uses snake_case.
- Preserve existing field names exactly.
- Pretty-print deterministically when writing files.
- Avoid omitting empty arrays if current output includes them.
- Avoid introducing null fields except where existing schema expects nullable data.
- Round-trip generated objects through deserialization.

Recommended output writers:

```rust
pub fn write_sector_json(path: &Path, sector: &Sector) -> Result<(), GenerationError>;
pub fn write_system_json(path: &Path, system: &System) -> Result<(), GenerationError>;
pub fn write_sector_markdown(path: &Path, sector: &Sector) -> Result<(), GenerationError>;
```

## 16. CLI Requirements

Add or extend CLI commands:

```text
generate system
generate sector
validate sector
render markdown
```

### 16.1 Generate system

Inputs:

- Seed.
- Optional profile.
- Optional index.
- Optional coordinate.
- Optional output path.

Outputs:

- Standalone system JSON.
- Optional Markdown snippet if supported.

### 16.2 Generate sector

Inputs:

- Seed.
- Optional profile.
- Optional width/height.
- Optional system count.
- Optional output directory.
- Optional markdown flag.

Outputs:

- Sector JSON.
- Per-system JSON files if configured.
- Sector Markdown if requested.

### 16.3 Validate sector

Inputs:

- Sector JSON path.

Outputs:

- Validation success or detailed invariant failures.

## 17. Testing Requirements

### 17.1 Unit tests

Required tests:

- Seeded RNG produces stable results.
- Weighted selection handles edge cases.
- Tag generation matches world profile fields.
- Hex distance calculation is correct.
- Route ID canonicalization is stable.
- Faction aggregation matches world faction assignments.
- Manifest counts match sector contents.

### 17.2 Property tests

Recommended property tests:

- Generated coordinates are unique.
- Generated routes reference existing systems.
- Generated world IDs are unique.
- Generated faction summaries never reference missing worlds.
- Generated tags are deduplicated.
- Generated sectors round-trip through JSON.

### 17.3 Golden tests

Use one or more locked seeds with test fixture catalogs.

Golden tests must assert:

- Same seed and same source digests produce identical JSON.
- Different seed usually produces different JSON.
- Markdown rendering is stable for a fixed sector object.

Do not use production catalogs as the only golden-test dependency. Create small fixture catalogs for fast, explicit tests.

## 18. Acceptance Criteria

The implementation is complete when:

1. A user can generate a standalone random system JSON from a seed.
2. A user can generate a complete random sector JSON from a seed.
3. The generated sector includes systems, routes, faction summaries, and manifest data.
4. Generated system JSON matches the existing standalone system structure.
5. Generated sector JSON matches the existing sector structure.
6. Markdown rendering matches the existing sector Markdown organization.
7. Generation is deterministic for a fixed seed, config, and source data.
8. The code validates generated output before writing it.
9. The implementation does not hard-code concrete sample values from attached outputs.
10. The implementation uses existing base data through catalogs/loaders.
11. Tests cover deterministic generation, validation, route generation, faction aggregation, and Markdown rendering.

## 19. Implementation Prompt for the Coding LLM

Use the following condensed instruction when asking an LLM to implement the Rust feature:

```text
Implement a data-driven random system and random sector generator in Rust.

The existing project already has source data and already emits sector/system JSON. Preserve the current JSON object layout. Do not hard-code concrete sample values from generated examples. Load star, world, faction, route, and name values from the existing catalogs/configuration.

Add serde models if missing, a deterministic seeded RNG manager, data-driven weighted selection, system generation, world generation, route generation using axial hex distance, faction aggregation, tag generation, manifest construction, validation, JSON writers, optional Markdown rendering, and CLI commands for generating a system or sector.

Generation must be deterministic for the same seed, profile, config, and input digests. All generated IDs, ordering, tags, routes, faction presence arrays, and manifest counts must be stable. Add unit tests, property-style tests where practical, and golden tests with fixture catalogs.
```

## 20. Developer Notes

- Keep generation and rendering separate.
- Keep catalogs and generated models separate.
- Prefer explicit validation over silent correction.
- Make every randomized choice traceable to a source table and RNG stream.
- Keep source row indexes whenever rows are selected from tabular source data.
- Treat attached generated files as examples of shape and compatibility, not as data to copy.
- When in doubt, make behavior configurable and preserve the current output schema.
