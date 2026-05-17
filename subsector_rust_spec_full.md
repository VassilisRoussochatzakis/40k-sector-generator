# Rust Specification: Sector Subsector Generation

## 1. Purpose

This specification defines how a Rust implementation should group star systems from a sector JSON document into **subsectors** and what data should define each subsector.

The design is intentionally deterministic: the same sector JSON, grouping configuration, and naming policy must always produce the same subsector IDs, membership, summaries, and route classifications.

## 2. Source Shape Observed

The attached sector JSON has the following relevant top-level fields:

| Field | Observed value / role |
|---|---|
| `id` | `big-test-sector` |
| `title` | `Big Test Sector` |
| `width` | `32` |
| `height` | `32` |
| `systems` | 200 system records |
| `routes` | 280 route records |
| `factions` | 995 faction records |
| `manifest` | generator and digest metadata |

Each system has an `id`, `index`, `name`, axial-like grid coordinate `coord.q` / `coord.r`, `star`, `worlds`, `primary_factions`, `tags`, and `notes`.

Faction control and capital selection may use additional optional fields if present in future sector JSON exports. The builder should prefer explicit ownership and economic fields, but it must also support deterministic inference when they are absent.

Recommended optional source paths are:

| Concept | Preferred source path | Fallback |
|---|---|---|
| System owner | `system.owner_faction_id` | inferred from `system.primary_factions[]` and world faction presence |
| World owner | `world.owner_faction_id` | inferred from world faction presence |
| World prosperity | `world.world.economic_prosperity` | inferred from population, tech level, trade/industry features, and tags |
| World capital suitability | `world.is_capital`, `world.world.is_capital`, or capital-like tags | inferred from population, prosperity, government, tech, and features |

Each route has an `id`, `from_system_id`, `to_system_id`, `distance`, `route_type`, `stability`, and `tags`.

## 3. Coordinate Model

Systems are placed on a bounded integer grid:

```rust
0 <= system.coord.q < sector.width
0 <= system.coord.r < sector.height
```

The grouping algorithm treats `q` as the horizontal coordinate and `r` as the vertical coordinate. No geometric conversion is required for subsector assignment. If the map is rendered as a hex grid, hex geometry may affect display, but not membership.

## 4. Default Grouping Policy

For this sector size, use **8×8 subsector tiles**:

```rust
pub const DEFAULT_SUBSECTOR_WIDTH: u32 = 8;
pub const DEFAULT_SUBSECTOR_HEIGHT: u32 = 8;
```

Because the observed sector is 32×32, this yields:

```text
ceil(32 / 8) * ceil(32 / 8) = 4 * 4 = 16 subsectors
```

The implementation must make the tile size configurable so other sector dimensions can be supported.

### 4.1 Subsector Indexing

Subsectors are indexed row-major:

```text
subsector_col = q / subsector_width
subsector_row = r / subsector_height
subsector_index = subsector_row * subsector_cols + subsector_col
```

Labels are assigned row-major as `A`, `B`, `C`, ...:

```text
A B C D
E F G H
I J K L
M N O P
```

For more than 26 subsectors, use a stable spreadsheet-style label policy: `A..Z`, `AA`, `AB`, etc.

### 4.2 Bounds

Each subsector owns an inclusive rectangular coordinate range:

```rust
q_min = col * subsector_width
q_max = min(sector.width - 1, (col + 1) * subsector_width - 1)

r_min = row * subsector_height
r_max = min(sector.height - 1, (row + 1) * subsector_height - 1)
```

A system belongs to the single subsector whose bounds contain its coordinate.

## 5. Subsector Definition

A subsector is a derived object. It should not duplicate full system records unless a denormalized export is explicitly requested.

### 5.1 Required Identity Fields

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subsector {
    pub id: String,
    pub sector_id: String,
    pub label: String,
    pub name: String,
    pub index: u32,
    pub row: u32,
    pub col: u32,
    pub bounds: SubsectorBounds,

    pub system_ids: Vec<String>,
    pub route_ids_internal: Vec<String>,
    pub route_ids_border: Vec<String>,

    pub neighboring_subsector_ids: Vec<String>,
    pub connected_subsector_ids: Vec<String>,

    pub summary: SubsectorSummary,
    pub tags: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SubsectorBounds {
    pub q_min: u32,
    pub q_max: u32,
    pub r_min: u32,
    pub r_max: u32,
}
```

### 5.2 Required Summary Fields

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsectorSummary {
    pub system_count: u32,
    pub world_count: u32,
    pub internal_route_count: u32,
    pub border_route_count: u32,

    /// Strategic or graph-theoretic primary system.
    pub primary_system_id: Option<String>,

    /// Administrative/economic capital candidate for the subsector.
    pub subsector_capital_system_id: Option<String>,

    /// Best capital world within `subsector_capital_system_id`, if determinable.
    pub subsector_capital_world_id: Option<String>,

    /// Faction with the strongest approximate territorial control, if any.
    pub controlling_faction_id: Option<String>,

    pub dominant_factions: Vec<ScoredId>,
    pub faction_control: Vec<FactionControlSummary>,
    pub world_type_counts: BTreeMap<String, u32>,
    pub star_colour_counts: BTreeMap<String, u32>,
    pub population_counts: BTreeMap<String, u32>,
    pub tech_level_counts: BTreeMap<String, u32>,
    pub government_counts: BTreeMap<String, u32>,
    pub feature_counts: BTreeMap<String, u32>,
    pub route_type_counts: BTreeMap<String, u32>,
    pub route_stability_counts: BTreeMap<String, u32>,
    pub tag_counts: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredId {
    pub id: String,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionControlSummary {
    pub faction_id: String,

    /// Number of systems assigned to this faction by the ownership resolver.
    pub owned_system_count: u32,

    /// Number of inhabited systems assigned to this faction by the ownership resolver.
    pub owned_inhabited_system_count: u32,

    /// Number of worlds assigned to this faction by the ownership resolver.
    pub owned_world_count: u32,

    /// Share of all eligible systems in basis points: 10_000 = 100.00%.
    pub system_share_basis_points: u32,

    /// Share of inhabited eligible systems in basis points: 10_000 = 100.00%.
    pub inhabited_system_share_basis_points: u32,

    /// Share of all eligible worlds in basis points: 10_000 = 100.00%.
    pub world_share_basis_points: u32,

    /// Weighted score used for deterministic sorting and threshold checks.
    pub control_score: i32,

    /// Stable string enum such as "absolute", "clear", "plurality", "contested", "presence", or "trace".
    pub control_tier: String,

    /// Systems where the faction has influence but not clear ownership.
    pub contested_system_count: u32,
}
```

## 6. Build API

The recommended public API is:

```rust
pub fn build_subsectors(
    sector: &Sector,
    config: SubsectorConfig,
) -> Result<Vec<Subsector>, SubsectorBuildError>;
```

```rust
#[derive(Debug, Clone)]
pub struct SubsectorConfig {
    pub width: u32,
    pub height: u32,
    pub include_empty_subsectors: bool,

    /// Number of faction-control rows to retain per subsector.
    pub faction_control_top_n: usize,

    /// Factions that must be reported even if they are not in the top-N control list.
    pub tracked_faction_ids: Vec<String>,

    /// Whether control percentages should use all systems or only inhabited systems
    /// as the primary denominator.
    pub control_denominator: ControlDenominator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlDenominator {
    AllSystems,
    InhabitedSystems,
}

impl Default for SubsectorConfig {
    fn default() -> Self {
        Self {
            width: DEFAULT_SUBSECTOR_WIDTH,
            height: DEFAULT_SUBSECTOR_HEIGHT,
            include_empty_subsectors: true,
            faction_control_top_n: 5,
            tracked_faction_ids: Vec::new(),
            control_denominator: ControlDenominator::InhabitedSystems,
        }
    }
}
```

`include_empty_subsectors = true` is recommended for map rendering because it preserves a stable grid even where no systems exist.

## 7. Membership Algorithm

### 7.1 Assignment

```rust
fn assign_system_to_subsector(
    q: u32,
    r: u32,
    sector_width: u32,
    sector_height: u32,
    subsector_width: u32,
    subsector_height: u32,
) -> Result<(u32, u32, u32), SubsectorBuildError> {
    if q >= sector_width || r >= sector_height {
        return Err(SubsectorBuildError::CoordinateOutOfBounds { q, r });
    }

    let cols = sector_width.div_ceil(subsector_width);
    let row = r / subsector_height;
    let col = q / subsector_width;
    let index = row * cols + col;

    Ok((index, row, col))
}
```

### 7.2 Grouping Procedure

1. Validate `sector.width > 0`, `sector.height > 0`, and configured subsector dimensions are non-zero.
2. Compute `subsector_cols = sector.width.div_ceil(config.width)`.
3. Compute `subsector_rows = sector.height.div_ceil(config.height)`.
4. Create every subsector cell in row-major order if `include_empty_subsectors = true`.
5. For each system:
   - validate `coord.q` and `coord.r`;
   - assign it by integer division;
   - append its `id` to the target subsector.
6. Sort `system_ids` by source system `index`, then by `id` as a tie-breaker.

## 8. Route Classification

Routes are classified after all systems have been assigned.

```rust
pub enum RouteScope {
    Internal { subsector_id: String },
    Border { from_subsector_id: String, to_subsector_id: String },
    ExternalOrInvalid,
}
```

Rules:

1. If both endpoints are in the same subsector, the route is **internal** and its ID appears once in `route_ids_internal`.
2. If endpoints are in different subsectors, the route is a **border route** and its ID appears in `route_ids_border` for both endpoint subsectors.
3. A route with a missing endpoint is invalid and should be reported as a build error unless the caller enables a permissive mode.
4. `connected_subsector_ids` is derived from border routes and sorted by subsector index.

Route summary counts should be computed from:
- internal routes owned by the subsector;
- border routes touching the subsector.

Because border routes are stored on both touching subsectors, aggregate sector-level reports must avoid double-counting them.

## 9. Neighboring Subsectors

`neighboring_subsector_ids` describes grid adjacency, independent of routes.

For rectangular UI navigation, use cardinal neighbors:

```text
north: row - 1, col
south: row + 1, col
west:  row, col - 1
east:  row, col + 1
```

Only include neighbors within the sector subsector grid.

If a future hex-aware subsector adjacency is required, it should be added as a separate field, not as a replacement for rectangular grid neighbors.

## 10. Summary Aggregation Rules

### 10.1 World and System Counts

```rust
summary.system_count = subsector.system_ids.len() as u32;
summary.world_count = sum(system.worlds.len());
```

### 10.2 Categorical Counts

For each world in each member system, increment:

| Source path | Summary map |
|---|---|
| `world.world.world_type` | `world_type_counts` |
| `world.world.star_colour` or `star.colour_name` | `star_colour_counts` |
| `world.world.population` | `population_counts` |
| `world.world.tech_level` | `tech_level_counts` |
| `world.world.government` | `government_counts` |
| `world.world.notable_features[]` | `feature_counts` |
| `world.tags[]` and `system.tags[]` | `tag_counts` |

For each internal or touching border route, increment:

| Source path | Summary map |
|---|---|
| `route.route_type` | `route_type_counts` |
| `route.stability` | `route_stability_counts` |
| `route.tags[]` | `tag_counts` |

### 10.3 Dominant Factions

Faction scoring should be deterministic and explainable:

```rust
fn influence_weight(influence: &str) -> i32 {
    match influence {
        "dominant" => 3,
        "significant" => 2,
        "minor" => 1,
        _ => 1,
    }
}
```

Aggregation:

1. For every world faction presence, add `influence_weight(faction.influence)`.
2. For every `system.primary_factions[]` entry, add `2`.
3. Sort by descending score, then ascending faction ID.
4. Store the top `N`, where `N = 3` by default.

This avoids requiring a full faction lookup while still reflecting system-level and world-level influence.

### 10.4 Approximate Faction Control by System Ownership

Subsector control is an approximate territorial summary. It answers: "Which faction appears to control the largest share of systems in this subsector?"

This is intentionally separate from `dominant_factions`:

- `dominant_factions` measures presence and influence across worlds and systems.
- `faction_control` measures approximate ownership of systems and worlds.
- `controlling_faction_id` is set only when the leading faction passes the configured control threshold.

#### 10.4.1 Ownership Resolver

Each system should first be assigned an approximate owner. Use the following deterministic resolver:

1. If `system.owner_faction_id` exists and is non-empty, use it.
2. Otherwise, score candidate owners from system and world data.
3. If exactly one candidate has a clear lead, assign the system to that candidate.
4. If no candidate has a sufficient score or the top candidates are tied, mark the system as unowned/contested for ownership-share purposes.

Recommended ownership scoring:

```rust
fn ownership_influence_weight(influence: &str) -> i32 {
    match influence {
        "dominant" => 6,
        "significant" => 3,
        "minor" => 1,
        _ => 1,
    }
}
```

Aggregation per system:

```text
system.primary_factions[] entry               +3
world.owner_faction_id match                  +8 per owned world
world faction presence by influence            +ownership_influence_weight(influence)
capital-like world controlled by faction       +4
highest-population world controlled by faction +3
```

A candidate owns the system if:

```text
candidate_score >= 6
and candidate_score >= runner_up_score + 3
```

If the leading candidate fails either condition, the system is considered `contested` for ownership. Contested systems may still contribute to `dominant_factions`, but they do not count as owned systems for faction-control share.

#### 10.4.2 Eligible System Denominator

By default, control percentages use inhabited systems as the denominator:

```text
eligible_systems = systems where max_population_rank > 0
```

This avoids letting empty frontier systems dominate political summaries. If the caller sets `control_denominator = AllSystems`, use every member system instead.

If a subsector has no inhabited systems, fall back to all systems. If it has no systems, `faction_control` is empty and `controlling_faction_id = None`.

#### 10.4.3 Share Calculation

For each faction:

```text
owned_system_count = number of eligible systems owned by faction
owned_inhabited_system_count = number of inhabited eligible systems owned by faction
owned_world_count = number of worlds owned by faction

system_share_basis_points =
    round_half_up(owned_system_count * 10_000 / eligible_system_count)

inhabited_system_share_basis_points =
    round_half_up(owned_inhabited_system_count * 10_000 / inhabited_system_count)

world_share_basis_points =
    round_half_up(owned_world_count * 10_000 / world_count)
```

Use integer arithmetic for deterministic output:

```rust
fn basis_points(numerator: u32, denominator: u32) -> u32 {
    if denominator == 0 {
        0
    } else {
        ((numerator as u64 * 10_000 + denominator as u64 / 2) / denominator as u64) as u32
    }
}
```

#### 10.4.4 Control Score and Tiers

Sort `faction_control` by:

1. descending `control_score`;
2. descending primary share basis points;
3. descending owned system count;
4. ascending faction ID.

Recommended control score:

```text
control_score =
    owned_system_count * 100
  + owned_inhabited_system_count * 50
  + owned_world_count * 10
  + system_share_basis_points / 100
```

Determine the leading faction's `control_tier` using the configured primary denominator:

| Tier | Condition |
|---|---|
| `absolute` | leader share >= 75% |
| `clear` | leader share >= 60% |
| `plurality` | leader share >= 40% and lead over runner-up >= 15 percentage points |
| `contested` | leader share >= 25% but lead over runner-up < 15 percentage points |
| `presence` | leader share >= 10% |
| `trace` | leader share > 0% and < 10% |

Set `controlling_faction_id` only when the leader tier is `absolute`, `clear`, or `plurality`. For `contested`, `presence`, `trace`, or no owned systems, set it to `None`.

When `config.tracked_faction_ids` is non-empty, always include those factions in `faction_control` with zero counts if needed, after the top-N rows have been selected. The final vector should still be sorted deterministically by the rules above.

### 10.5 Primary System and Subsector Capital Selection

`primary_system_id` and `subsector_capital_system_id` are related but not identical:

- `primary_system_id` is the most strategically important system, emphasizing route connectivity, population, tech, and faction presence.
- `subsector_capital_system_id` is the best administrative/economic capital candidate, emphasizing population, prosperity, stability, government, and the best capital-capable world within the system.

Implementations may expose both. If the consuming application only needs one headline system, prefer `subsector_capital_system_id` for political maps and `primary_system_id` for route/network maps.

#### 10.5.1 Primary System Selection

`primary_system_id` is the most important system in the subsector. Use a configurable scoring function:

```rust
score =
    route_degree * 4
  + max_population_rank * 3
  + max_tech_rank * 2
  + world_count
  + primary_faction_count;
```

Tie-breakers:

1. lower `system.index`;
2. lexicographically smaller `system.id`.

Suggested default ranks:

```rust
fn population_rank(value: &str) -> i32 {
    match value {
        "Uninhabited" => 0,
        "Minimal" => 1,
        "SoleSettlement" => 2,
        "LightlyPopulated" => 3,
        "DenselyPopulated" => 4,
        "ExtremelyDense" => 5,
        _ => 0,
    }
}

fn tech_rank(value: &str) -> i32 {
    match value {
        "Primitive" => 0,
        "Low" => 1,
        "Standard" => 2,
        "High" => 3,
        "Archaeotech" => 4,
        "XenoHybrid" => 4,
        _ => 0,
    }
}
```

#### 10.5.2 Prosperity Ranking

Capital scoring should use explicit prosperity data when present. Suggested rank mapping:

```rust
fn prosperity_rank(value: &str) -> i32 {
    match value {
        "Destitute" => 0,
        "Poor" => 1,
        "Struggling" => 2,
        "Stable" => 3,
        "Prosperous" => 4,
        "Affluent" => 5,
        "Opulent" => 6,
        _ => 0,
    }
}
```

If no explicit prosperity field exists, infer a conservative prosperity rank from stable fields:

```text
inferred_prosperity =
    population_rank
  + tech_rank
  + trade_feature_bonus
  + industrial_feature_bonus
  + route_access_bonus
  - hazard_or_ruin_penalty
```

Then clamp to `0..=6`.

Suggested bonuses and penalties:

| Signal | Adjustment |
|---|---:|
| tag or feature contains `trade`, `market`, `commerce`, `port`, or `hub` | +1 |
| tag or feature contains `industrial`, `shipyard`, `mining`, or `manufacturing` | +1 |
| system has 3 or more touching routes | +1 |
| route stability includes a stable route touching the system | +1 |
| tag or feature contains `ruin`, `hazard`, `warzone`, `quarantine`, or `blockade` | -2 |

String matching should be case-insensitive and performed on normalized ASCII-lowercase tokens. Unknown or missing prosperity values should produce warnings, not errors.

#### 10.5.3 Capital World Selection

A subsector capital should identify both a system and, if possible, the best world within that system. For each world, compute:

```text
world_capital_score =
    population_rank * 8
  + prosperity_rank * 7
  + tech_rank * 4
  + government_bonus
  + capital_feature_bonus
  + owner_alignment_bonus
  + stability_bonus
```

Recommended bonuses:

| Signal | Bonus |
|---|---:|
| explicit `is_capital = true` or tag/feature contains `capital`, `admin`, `bureaucratic`, or `seat` | +10 |
| government is organized/state-like, such as `Republic`, `Corporate`, `Imperial`, `Federation`, or `Theocracy` | +3 |
| world owner matches `controlling_faction_id` | +3 |
| world is in a system with at least one stable route | +2 |
| world is uninhabited | -20 |

Select the highest-scoring world as `subsector_capital_world_id`. If every world has a negative score, leave the capital world as `None` but the system may still be selected if it is the best available system.

#### 10.5.4 Capital System Selection

For each system, compute:

```text
capital_score =
    best_world_capital_score
  + max_population_rank * 10
  + max_prosperity_rank * 8
  + route_degree * 4
  + stable_route_degree * 3
  + max_tech_rank * 3
  + owned_by_controlling_faction_bonus
  + multi_world_bonus
  + hazard_penalty
```

Recommended system-level adjustments:

| Signal | Adjustment |
|---|---:|
| system owner matches `controlling_faction_id` | +6 |
| system has 2 or more inhabited worlds | +3 |
| system has 4 or more worlds total | +2 |
| system has border routes to 2 or more other subsectors | +2 |
| system has only unstable routes | -3 |
| system has hazard/ruin/quarantine/blockade tags | -5 |

Tie-breakers:

1. higher `best_world_capital_score`;
2. higher `max_population_rank`;
3. higher `max_prosperity_rank`;
4. higher `stable_route_degree`;
5. lower `system.index`;
6. lexicographically smaller `system.id`.

Set `subsector_capital_system_id` to the winning system. Set `subsector_capital_world_id` to the winning system's best world if one exists.

#### 10.5.5 Empty and Frontier Subsectors

For an empty subsector:

```text
primary_system_id = None
subsector_capital_system_id = None
subsector_capital_world_id = None
controlling_faction_id = None
faction_control = []
```

For a frontier subsector with systems but no inhabited worlds, `primary_system_id` may still be selected from route connectivity, but `subsector_capital_system_id` should only be selected if at least one system has a non-negative capital score. Otherwise leave the capital fields empty.

## 11. Validation Requirements

The builder must return an error for:

- zero sector width or height;
- zero subsector width or height;
- duplicate system IDs;
- duplicate system coordinates, unless the caller explicitly allows stacking;
- systems outside sector bounds;
- routes whose endpoints cannot be found;
- generated subsector index overflow.

Warnings, not errors, are appropriate for:

- unknown categorical values;
- unknown or missing prosperity values;
- systems with no worlds;
- worlds with no factions;
- empty tags or notes;
- sector dimensions that do not divide evenly into subsector dimensions.

## 12. Serialization Shape

A compact JSON export should look like this:

```json
{
  "id": "subsector-a",
  "sector_id": "big-test-sector",
  "label": "A",
  "name": "Subsector A",
  "index": 0,
  "row": 0,
  "col": 0,
  "bounds": { "q_min": 0, "q_max": 7, "r_min": 0, "r_max": 7 },
  "system_ids": ["sys-0001", "sys-0002"],
  "route_ids_internal": ["route-sys-0001-sys-0002"],
  "route_ids_border": [],
  "neighboring_subsector_ids": ["subsector-b", "subsector-e"],
  "connected_subsector_ids": ["subsector-b"],
  "summary": {
    "system_count": 2,
    "world_count": 7,
    "internal_route_count": 1,
    "border_route_count": 0,
    "primary_system_id": "sys-0001",
    "subsector_capital_system_id": "sys-0001",
    "subsector_capital_world_id": "world-sys-0001-01",
    "controlling_faction_id": "example_faction",
    "dominant_factions": [
      { "id": "example_faction", "score": 5 }
    ],
    "faction_control": [
      {
        "faction_id": "example_faction",
        "owned_system_count": 2,
        "owned_inhabited_system_count": 2,
        "owned_world_count": 4,
        "system_share_basis_points": 10000,
        "inhabited_system_share_basis_points": 10000,
        "world_share_basis_points": 5714,
        "control_score": 544,
        "control_tier": "absolute",
        "contested_system_count": 0
      }
    ]
  },
  "tags": [],
  "notes": []
}
```

Full exports may optionally embed full system and route records, but normalized IDs are preferred for save files and incremental updates.

## 13. Derived Subsector Grid for the Attached Sector

Using the default 8×8 policy, the attached 32×32 sector produces the following grid:

| ID | Label | Bounds `(q,r)` | Systems | Worlds | Internal routes | Border route refs |
|---|---:|---|---:|---:|---:|---:|
| `subsector-a` | A | q 0..=7, r 0..=7 | 15 | 56 | 26 | 8 |
| `subsector-b` | B | q 8..=15, r 0..=7 | 12 | 50 | 13 | 12 |
| `subsector-c` | C | q 16..=23, r 0..=7 | 14 | 55 | 12 | 14 |
| `subsector-d` | D | q 24..=31, r 0..=7 | 12 | 46 | 8 | 7 |
| `subsector-e` | E | q 0..=7, r 8..=15 | 14 | 53 | 25 | 17 |
| `subsector-f` | F | q 8..=15, r 8..=15 | 10 | 38 | 10 | 13 |
| `subsector-g` | G | q 16..=23, r 8..=15 | 10 | 37 | 5 | 16 |
| `subsector-h` | H | q 24..=31, r 8..=15 | 15 | 70 | 7 | 8 |
| `subsector-i` | I | q 0..=7, r 16..=23 | 9 | 38 | 7 | 13 |
| `subsector-j` | J | q 8..=15, r 16..=23 | 13 | 48 | 12 | 14 |
| `subsector-k` | K | q 16..=23, r 16..=23 | 12 | 54 | 6 | 16 |
| `subsector-l` | L | q 24..=31, r 16..=23 | 12 | 50 | 4 | 13 |
| `subsector-m` | M | q 0..=7, r 24..=31 | 12 | 50 | 17 | 3 |
| `subsector-n` | N | q 8..=15, r 24..=31 | 13 | 45 | 10 | 12 |
| `subsector-o` | O | q 16..=23, r 24..=31 | 15 | 61 | 16 | 9 |
| `subsector-p` | P | q 24..=31, r 24..=31 | 12 | 41 | 10 | 9 |


Notes:

- `Internal routes` are owned by one subsector.
- `Border route refs` are references from this subsector to routes crossing into another subsector.
- Border route refs are counted once per touching subsector, so they should not be summed as unique sector routes without deduplication.

## 14. Implementation Checklist

- [ ] Parse `Sector` with Serde.
- [ ] Validate sector dimensions, system IDs, coordinates, and routes.
- [ ] Compute subsector grid dimensions with `u32::div_ceil`.
- [ ] Create row-major subsector cells.
- [ ] Assign every system to exactly one subsector.
- [ ] Classify every route as internal or border.
- [ ] Populate route and neighbor connectivity.
- [ ] Resolve approximate system and world ownership for faction-control summaries.
- [ ] Compute `faction_control` and `controlling_faction_id` using deterministic share thresholds.
- [ ] Score `primary_system_id` for strategic importance.
- [ ] Score `subsector_capital_system_id` and `subsector_capital_world_id` from population, prosperity, stability, government, and route access.
- [ ] Aggregate summaries from systems, worlds, factions, tags, and routes.
- [ ] Sort all ID vectors and scored vectors for deterministic output.
- [ ] Serialize subsectors as normalized records.
- [ ] Add tests for boundary coordinates: `(0,0)`, `(7,7)`, `(8,0)`, `(31,31)`.

## 15. Key Invariants

A correct implementation must satisfy:

```text
sum(subsector.system_count) == sector.systems.len()

every system ID appears in exactly one subsector

for every internal route:
    endpoint_subsector(from) == endpoint_subsector(to)

for every border route:
    endpoint_subsector(from) != endpoint_subsector(to)

subsector.bounds never exceed sector bounds

if controlling_faction_id is Some(id), then id appears in faction_control with tier absolute, clear, or plurality

if subsector_capital_system_id is Some(id), then id appears in system_ids

if subsector_capital_world_id is Some(id), then it belongs to subsector_capital_system_id

subsector IDs and labels are stable for a fixed sector size and grouping config
```

## 16. Recommended Unit Tests

### Boundary Assignment

```rust
assert_eq!(assign(0, 0), "subsector-a");
assert_eq!(assign(7, 7), "subsector-a");
assert_eq!(assign(8, 0), "subsector-b");
assert_eq!(assign(0, 8), "subsector-e");
assert_eq!(assign(31, 31), "subsector-p");
```

### Route Classification

```rust
// Same subsector.
assert_eq!(classify_route("sys-0001", "sys-0002"), RouteScope::Internal { .. });

// Different subsectors.
assert_eq!(classify_route("sys-0001", "sys-0042"), RouteScope::Border { .. });
```

### Faction Control

```rust
// A faction owning 6 of 8 eligible systems has absolute or clear control,
// depending on the configured tier thresholds.
let control = faction_control_for("faction-alpha");
assert_eq!(control.owned_system_count, 6);
assert!(control.system_share_basis_points >= 7_500 || control.control_tier == "clear");
```

### Subsector Capital

```rust
// The capital should prefer a populous, prosperous, stable route hub over
// a less inhabited system with the same route degree.
let capital = subsector.summary.subsector_capital_system_id.as_deref();
assert_eq!(capital, Some("sys-prosperous-hub"));
```

### Determinism

Build subsectors twice from the same sector and config. The serialized output must be byte-for-byte identical after pretty-printing with sorted map keys.
