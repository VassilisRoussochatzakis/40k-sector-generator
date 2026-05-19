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

### `sectorforge analyze` (§8 NEW.md)

Read-only analytics dashboard for a finished sector. Computes faction balance
(Gini coefficient + per-faction projection share), contested-world ratio,
average claims per world, claim-kind + dominance counts, world-type / star-colour
/ population / route distributions, route-graph connectivity (component count,
diameter, articulation points, isolated systems), per-subsector political variety,
and a list of health flags driven by the `[analyze]` config block.

Run against either a project (regenerates from `sectorforge.toml`) or a saved
`sector.json`:

```bash
# Regenerate + analyze.
cargo run --bin sectorforge -- analyze --project examples/m42_project

# Analyze a previously-saved sector.json.
cargo run --bin sectorforge -- analyze --sector examples/m42_project/out/sector.json

# Write analysis.md + analysis.json into a directory.
cargo run --bin sectorforge -- analyze --sector path/to/sector.json --out out/
```

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Generate from project and analyze the result |
| `--sector <PATH>` | Analyze an existing `sector.json` |
| `--out <DIR>` | Write `analysis.md` + `analysis.json` into `<DIR>` (overrides stdout) |
| `--json` | Emit JSON to stdout instead of Markdown |
| `--strict` | Exit 1 if any health flag fires (useful in CI) |

Optional `[analyze]` block in `sectorforge.toml` controls thresholds:

```toml
[analyze]
warn_faction_share        = 0.50   # flag if any single faction exceeds this share of projection power
warn_if_disconnected      = true   # flag if the route graph has >1 component
warn_if_articulation      = true   # flag every system whose removal would fragment the route graph
warn_contested_ratio      = 0.66   # info-level flag if more than this fraction of inhabited worlds are contested
tiny_sector_threshold     = 5      # sectors below this are flagged low-confidence rather than failing structural metrics
```

### `sectorforge new --out <DIR> --preset <NAME>` (§9 NEW.md)

Scaffold a fresh project from a bundled preset under `presets/` (by default).
The destination must not exist. The optional `--seed` flag rewrites the
`[generation].seed` line of the new project's `sectorforge.toml`.

```bash
# List available presets.
cargo run --bin sectorforge -- list-presets

# Scaffold a new project from a preset, override the seed.
cargo run --bin sectorforge -- new \
    --out my-sector \
    --preset embattled-frontier \
    --seed 2026-campaign-A

# Then generate as usual.
cargo run --bin sectorforge -- generate --project my-sector --allow-warnings
```

Bundled presets:

| ID | Flavour |
|---|---|
| `m42-classic` | Balanced sample sector. Good starting point. |
| `embattled-frontier` | Sparse, clustered placement, low route density, more hazardous lanes. |
| `dead-sector` | Low population, large hop distances, ruins-leaning generation. |
| `mercantile-crossroads` | Dense placement, short hops, high route density, trade-hub feel. |

Presets are pure-data overlays. Each `presets/<id>/` is either a complete project
tree or a thin overlay with `inherits = "<base>"` in `preset.toml` that overlays
onto another preset's tree (shared data avoids duplication).

### `sectorforge list-presets`

Print the available presets with one-line descriptions. Reads
`presets/<id>/preset.toml` for metadata. Internal bundles whose id starts with
`_` are hidden.

### `sectorforge search` (§2 NEW.md)

Constraint-directed deterministic seed search. Lets you declare what the
generated sector should look like — faction balance bands, world-type
presences under particular dominant factions, route-graph properties,
system-state counts — and enumerates seeds derived from a base seed until
one satisfies every constraint, or the budget is exhausted.

The search itself is reproducible: same `base_seed` + same `wishes.toml` +
same project ⇒ same winning seed. Candidate seeds are derived via
`blake3("sectorforge:{base_seed}:search:{n}")`; `n=0` always returns the
base seed verbatim, so passing a known-good seed as the base wins the
search trivially without enumerating.

```bash
cargo run --bin sectorforge -- search \
    --project examples/m42_project \
    --wishes wishes.toml \
    --out out/
```

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project the candidates are evaluated against |
| `--wishes <FILE>` | `wishes.toml` listing constraints and search settings |
| `--base-seed <S>` | Override the wishes/project base seed (becomes the `n=0` candidate) |
| `--budget <N>` | Override the wishes file's search budget |
| `--out <DIR>` | Write `search.md` + `search.json` into `<DIR>` |
| `--json` | Emit JSON to stdout instead of Markdown |
| `--strict` | Exit 1 if no candidate satisfied all constraints |

Example `wishes.toml`:

```toml
[search]
base_seed = "campaign-A"
budget    = 128
report_top = 5

# At least one Mechanicus-dominated forge world.
[[constraints]]
kind = "world_type_exists"
world_type = "ForgeWorld"
dominant_faction_id = "mechanicus"
min_count = 1

# Chaos must hold between 25% and 40% of total faction projection.
[[constraints]]
kind = "faction_share_min"
faction_id = "chaos_undivided"
min = 0.25

[[constraints]]
kind = "faction_share_max"
faction_id = "chaos_undivided"
max = 0.40

# At least three contested worlds with three or more competing claimants.
[[constraints]]
kind = "contested_world_min"
min = 3
n_way = 3

# Route graph must be connected with no single point of failure.
[[constraints]]
kind = "route_graph_connected"

[[constraints]]
kind = "no_articulation_points"
```

Supported constraint `kind`s:

| Kind | Parameters |
|---|---|
| `faction_share_min` / `faction_share_max` | `faction_id`, `min`/`max` (0.0..=1.0) |
| `faction_world_count_min` / `faction_world_count_max` | `faction_id`, `min`/`max` |
| `faction_system_count_min` / `faction_system_count_max` | `faction_id`, `min`/`max` |
| `world_type_exists` | `world_type`, optional `dominant_faction_id`, `min_count` |
| `contested_world_min` | `min`, optional `n_way` (min competing claims) |
| `contested_world_max` | `max` |
| `system_state_count_min` / `system_state_count_max` | `state` (snake_case), `min`/`max` |
| `route_graph_connected` | — |
| `no_articulation_points` | — |
| `diameter_max` | `max_hops` |
| `isolated_systems_max` | `max` |
| `contested_ratio_min` / `contested_ratio_max` | `min`/`max` (0.0..=1.0) |
| `stance_count_min` / `stance_count_max` | `stance` (`allied`/`aligned`/`neutral`/`rival`/`hostile`/`at_war`), `min`/`max` |
| `region_count_min` / `region_count_max` | `region_kind` (`warp_storm`/`turbulence`/`calm_corridor`/`blackout`/`anomaly`), `min`/`max` |
| `economy_stranded_max` | `max` (cap stranded worlds) |
| `economy_resource_min` | `resource` (one of `ore`/`promethium`/`foodstuffs`/`manufactured`/`archeotech`/`recruits`), `min` (net sector balance) |

Unknown faction ids are caught by a preflight check before any search
runs, so an over-constrained or typo'd wish set fails immediately with a
clear message. When no candidate satisfies the constraints, the report
includes the top `report_top` near-misses ranked by total miss distance,
so you know which constraint to relax.

### `sectorforge diff` (§10 NEW.md)

Deterministic model-aware diff between two sectors. Two modes:

```bash
# Compare two saved sectors (different seeds, different ticks, etc.).
cargo run --bin sectorforge -- diff \
    --before path/to/before.json \
    --after  path/to/after.json \
    --out out/

# Generate a sector, advance N conflict ticks, diff before vs. after.
cargo run --bin sectorforge -- diff \
    --project examples/m42_project \
    --ticks 5 \
    --out out/
```

| Flag | Meaning |
|---|---|
| `--before <PATH>` + `--after <PATH>` | Compare two `sector.json` files |
| `--project <DIR>` + `--ticks <N>` | Generate + advance, diff before/after |
| `--out <DIR>` | Write `diff.md` + `diff.json` |
| `--json` | Emit JSON to stdout instead of Markdown |
| `--skip-worlds` | Drop per-world detail from the report |
| `--skip-routes` | Drop per-route detail |

Entity matching uses the generator's stable IDs (`sys-NNNN`, `route-...`),
so renaming a world is reported as a modification rather than a
delete+add. The diff is a pure derivation — same inputs ⇒ same output —
covered by golden-style tests against the bundled example project.

The Markdown digest is organised by stratum: schema warnings, system-level
changes (state, dominant/sovereign/occupier flips, primary-faction
add/remove, world add/remove/change), route changes, a faction power
delta table filtered by `min_faction_delta`, **Diplomacy changes**
(§4 stance flips per pair), **Warp regions** (§5 added/removed/changed
regions), and **Economy** (§12 scalar sector balance deltas plus newly /
no-longer stranded world lists). Sector id or `generator_version`
mismatch is reported but does not refuse the diff — the report is
marked as best-effort instead.

### `sectorforge history` (§1 NEW.md)

Deterministic chronicle generator. Walks every world's claims, dominance,
archetype state, blockade, and conflict and emits a dated chronological
list of in-universe events. Pure derivation — same sector ⇒ same chronicle.

```bash
# Regenerate + chronicle.
cargo run --bin sectorforge -- history --project examples/m42_project

# Chronicle an existing sector.json into a directory.
cargo run --bin sectorforge -- history \
    --sector examples/m42_project/out/sector.json \
    --out out/
```

Writes `history.md` + `history.json`. Dates use `M{epoch}.{ddd}` notation
where `epoch` is scaled by event topo-rank (foundations land in the start
epoch, post-conflict reconquests in the end epoch); within an anchor the
chronicle is monotonic (foundation before annexation before reconquest).

### `sectorforge personae` (§3 NEW.md)

Deterministic dramatis personae overlay. Anchors a named character on
each system sovereign / orbital-controller / hidden-master slot and each
world presence at the configured dominance tier, drawing names + titles +
traits + agendas from per-faction-kind pools.

```bash
cargo run --bin sectorforge -- personae --project examples/m42_project --out out/
```

Writes `personae.md` + `personae.json`. Built-in pools cover the common
40k faction kinds (Imperial / Mechanicus / Ecclesiarchy / Inquisition /
RogueTrader / Chaos / Rebel / Necron / Tyranid / Ork / T'au / Aeldari /
Drukhari / Harlequin / Genestealer / Xenos); the agenda line is bound to
the actual competing claims on the anchor world so two personae for the
same faction in different places read differently.

### `sectorforge hooks` (§7 NEW.md)

Adventure / plot-hook generator. Scans worlds, systems, and routes for
combinations that imply runnable drama (contested claims with a force
occupier and a legitimate sovereign, hidden masters under GSC archetype
activity, Perilous routes, patrolled + pirated lanes, quarantine /
blockade / warzone states, Tyranid / Necron / Chaos pressure) and emits
ranked one-line hooks.

```bash
cargo run --bin sectorforge -- hooks --project examples/m42_project --out out/

# "Player edition": redact GM-only hooks derived from hidden-tier intel.
cargo run --bin sectorforge -- hooks --sector out/sector.json --player
```

Writes `hooks.md` + `hooks.json`. Hooks reference only real, present
entities; the GM-only flag respects the existing intel layer.

When the §12 economy derivation is enabled, the generator also emits
`StarvingWorld` hooks for every stranded world (anchored to the world,
naming the missing resource) and `LifelineLane` hooks for every
non-Perilous route that is the *only* import of a critical resource
(foodstuffs / promethium / manufactured) into a deficit system —
anchored to the route, ranked alongside the structural hooks.

### `sectorforge prose` (§6 NEW.md)

Narrative gazetteer generator: deterministic template grammar (not an
LLM) that emits a sector overview paragraph and a short prose entry per
system. Two tone presets:

```bash
# Florid in-universe gazetteer.
cargo run --bin sectorforge -- prose --project examples/m42_project --out out/

# Terse Administratum-dispatch tone.
cargo run --bin sectorforge -- prose --project examples/m42_project --dispatch
```

Writes `gazetteer.md` + `gazetteer.json`. Prose is strictly data-bound —
no fact appears that isn't in the JSON — and the variation between
adjacent systems is keyed by id so the gazetteer never reads
copy-pasted.

### `sectorforge relations` (§4 NEW.md)

Inter-faction diplomacy matrix. For every unordered pair of factions
present in the sector, derives a single canonical stance (`Allied`,
`Aligned`, `Neutral`, `Rival`, `Hostile`, `At War`) plus a short cause
text and a `tension` scalar (0..=100) computed from co-occurrence on
contested worlds and active warzones.

```bash
# Regenerate + diplomacy.
cargo run --bin sectorforge -- relations --project examples/m42_project --out out/

# Diplomacy for an existing sector.json.
cargo run --bin sectorforge -- relations --sector out/sector.json --json
```

Writes `relations.md` + `relations.json`. The matrix is also embedded in
`sector.json` under the `relations` field once the project ships a
[data/factions/relations.toml](examples/m42_project/data/factions/relations.toml)
(referenced via `inputs.relations` in `sectorforge.toml`).

The TOML file ships **kind rules** (faction kind × kind → stance),
**disposition rules** (level delta on top of the base stance), and
**pair overrides** (pin a specific `(faction_id, faction_id)` stance).
Built-in defaults — Imperial↔Chaos = At War, Mechanicus↔Imperial =
Aligned, Tyranid/Necron vs. anyone = At War, etc. — apply when the file
is silent. A small deterministic per-pair perturbation seeded from
`blake3("sectorforge:{seed}:relations:{a}:{b}")` breaks ties so two
identical kind+disposition pairs are not always identical.

Set `[relations].feed_conflict = true` to copy the flag onto the derived
matrix; `advance_sector` then biases per-world momentum/intensity by the
stance between the local attacker/defender (At War / Hostile pushes
toward the attacker, Allied / Aligned drifts toward peace). The bias is
applied *before* the existing tick logic and never overrides GM-edited
conflict state.

### `sectorforge regions` (§5 NEW.md)

Regional warp-phenomena overlay. Grows seeded blob regions over the hex
grid *before* route generation; the chosen condition modifies route
stability inside the footprint and tints PNG-export hexes for the
overlay:

* `WarpStorm` → forces routes crossing the footprint to `Perilous`.
* `Turbulence` → degrades by one stability tier.
* `CalmCorridor` → upgrades by one tier (cannot upgrade above
  `Hazardous` when another rule already forced `Perilous`).
* `Blackout` → marks the area for no covert / hidden routes.
* `Anomaly` → reweights world generation in the affected hex toward
  warp-phenomena / ancient-ruins / daemonic-corruption candidates
  (3× weight multiplier on the candidate pool draw). Regions are built
  *before* the system loop so the bias is applied at world-selection
  time, not as a post-hoc tag.

```bash
# Standalone overlay derivation (no full sector regen).
cargo run --bin sectorforge -- regions --project examples/m42_project --out out/

# Generate a full sector — regions auto-applied if enabled.
cargo run --bin sectorforge -- generate --project examples/m42_project
```

Writes `regions.md` + `regions.json`. The shipped
[data/routes/regions.toml](examples/m42_project/data/routes/regions.toml)
defaults to `enabled = false`; flip it on to grow regions for the project.
Regions are embedded on `sector.json` under the `regions` field and
rendered as a translucent tint underneath the existing hex grid in both
the PNG export and the on-screen GUI sector map. The Markdown sector map
prints region glyphs over empty hexes: `~` warp storm, `^` turbulence,
`=` calm corridor, `#` blackout, `*` anomaly.

`Blackout` regions also gate the hidden-route stage: webway, black-ship,
and smuggling-lane endpoints inside a blackout footprint are excluded so
no covert lane terminates there. Post-generation invariants check that
region hexes stay inside the grid, that no two regions overlap, and
that the route graph restricted to non-Perilous lanes stays connected
after region effects are applied — the `REGION_ISOLATES_SECTOR`
violation surfaces when a storm splits the sector into multiple
components.

### `sectorforge economy` (§12 NEW.md)

Trade & resource economy snapshot. Each world declares a signed
production/consumption vector over six categories (ore, promethium,
foodstuffs, manufactured goods, archeotech, recruits) keyed by
`world_type × tech_level × population`. Per-route trade volume is
derived from the endpoint surplus/deficit gradient × distance falloff ×
hazard tier × piracy/interdiction friction; per-system and sector-wide
balance sheets fall out for free.

```bash
# Regenerate + economy.
cargo run --bin sectorforge -- economy --project examples/m42_project --out out/

# Economy for an existing sector.json (built-in defaults apply).
cargo run --bin sectorforge -- economy --sector out/sector.json
```

Writes `economy.md`, `economy.json`, and `economy.csv` (per-world
vectors plus a `stranded` boolean for worlds with shortages no inbound
route can fix). The shipped
[data/worlds/economy.toml](examples/m42_project/data/worlds/economy.toml)
defaults to `enabled = false`; users can override the production matrix
per `world_type`, set per-`tech_level` multipliers, and set per-population
multipliers. With `feed_stability = true`, stranded-foodstuffs worlds
receive a bounded one-way nudge to
`stability.famine_or_resource_stress` (read-only; the conflict tick is
not perturbed).

When enabled, the economy snapshot also drives several downstream
surfaces:

* `sector.md` gains a dedicated **Economy** section (sector balance,
  stranded worlds, top trade lanes).
* The Route Planner annotates lifeline lanes — any non-Perilous route
  that is the *only* viable import of foodstuffs / promethium /
  manufactured into a deficit system is flagged Caution with an
  `only {resource} import into {sys}` note.
* The plot-hook generator (§7 NEW.md) emits `StarvingWorld` hooks for
  every stranded world and `LifelineLane` hooks for every critical
  supply route.
* The Trade heatmap mode (`HeatmapMode::TradeVolume`) sums incident
  route volumes per system.
* The diff report (§10) carries scalar `economy_balance_changes` plus
  newly / no-longer stranded world lists. The post-gen invariants
  enforce a `ECONOMY_ENABLED_NO_WORLDS` check (enabled but empty world
  list signals a misconfigured derivation).

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
    factions/relations.toml        # §4 NEW.md (optional)
    routes/route_rules.toml
    routes/regions.toml            # §5 NEW.md (optional)
    worlds/economy.toml            # §12 NEW.md (optional)
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
relations             = "data/factions/relations.toml"     # optional (§4 NEW.md)
regions               = "data/routes/regions.toml"         # optional (§5 NEW.md)
economy               = "data/worlds/economy.toml"         # optional (§12 NEW.md)

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
                                 #         tension (§4)         — sum of hostile/at-war pair tensions
                                 #         trade_volume (§12)   — sum of incident route volumes
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
   "manifest": { /* seed, digests, counts */ },
   "influence_field": { /* §9 NEXT: cells + bands */ },
   "power_projection": { /* §4 NEXT: faction → system → projection */ }
}
```

Each `GeneratedSystem` has an `id`, `name`, `coord`, `star`, list of
`worlds`, plus `primary_factions`, `tags`, `notes`, a `control`
summary (see "Faction control model" below), `orbital_assets`
([src/orbital_assets.rs](src/orbital_assets.rs) §2 NEXT), a
`blockade` snapshot, a `conflict` tick-state record
([src/conflict.rs](src/conflict.rs) §5 NEXT), an `intel`
fog-of-war record ([src/intel.rs](src/intel.rs) §7 NEXT), and an
`archetype` block of faction-specific narrative state
([src/archetypes.rs](src/archetypes.rs) §11 NEXT).

Each `GeneratedWorld` wraps a `WorldDto` view of `worlds::World` — variant
names are stable (e.g. `"HiveWorld"`) — and also carries `claims`
(per-faction legal/military/religious claims), a `control`
multi-winner snapshot, `regions`
([src/surface_region.rs](src/surface_region.rs) §1 NEXT: named
geographic regions with per-region dominant faction), and a per-world
`conflict` record.

Each `GeneratedRoute` now exposes additional `route_type` variants for
hidden lanes: `webway`, `black_ship`, `smuggling_lane` (§3 NEXT).
Hidden routes are added between systems where both have meaningful
faction presence of the appropriate kind, ignoring the warp-distance
cap. Each qualifying endpoint emits edges only to its `HIDDEN_K_NEAREST`
(currently 3) closest peers by hex distance (with system-id tie-break),
deduplicated across both endpoints. This caps an otherwise O(N²)
full-clique blow-up that produced thousands of edges in dense sectors;
the layer is still deterministic and consults no RNG.

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

Human-readable. Sections: title + seed, summary counts, ASCII sector map
(with `~^=#*` region glyphs over empty hexes inside §5 warp regions),
system index table, one block per system (coords, star, world table),
routes and factions tables, a **Diplomacy digest** (§4 — at-war /
hostile pair lists), a **Warp regions** section (§5 — region table when
the overlay is active), and an **Economy** section (§12 — sector
balance, stranded worlds, top trade lanes) when the economy derivation
is enabled.

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
  faction tint, subsector overlay, and a translucent warp-region tint
  (§5) for every hex covered by a `WarpRegion`. Click a hex to drill into
  the system. The bottom controls expose a **HEATMAP** dropdown that
  tints every system hex by a per-mode score: `CONTROL` (dominant-faction
  colour × control-score intensity), `MILITARY`, `TRADE`, `INDUSTRY`,
  `COVERT`, `FAITH`, `THREAT` (military × covert restricted to
  hostile/zealous), `INTEL` (low-visibility hexes glow), `TENSION`
  (§4 — sum of hostile/at-war pair tensions per system), or `TRADE VOL`
  (§12 — sum of incident route trade volumes). See
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
- **Diplomacy** (§4 NEW.md) — table view of `sector.relations.pairs`:
  every faction pair with its stance (colour-coded), tension scalar, and
  cause text. Backed by [src/gui/app/mod.rs](src/gui/app/mod.rs)
  `draw_relations_layout`.
- **Regions** (§5 NEW.md) — table view of `sector.regions`: id, name,
  kind, hex count, centre coord. Pairs with the in-map region tint.
- **Trade** (§12 NEW.md) — table view of `sector.economy`: sector
  resource balance, top trade lanes by volume, list of stranded worlds
  with shortages.
- **Dashboard** (§8 NEW.md) — analytics for the loaded sector. Faction-share
  bars coloured by the same per-faction palette the map uses, a Gini
  coefficient, contested-world / claim summary, route-graph connectivity
  callout (component count, diameter, articulation points, isolated systems),
  world / star / population / route distributions, per-subsector political
  variety, and a list of health flags. Backed by
  [src/analytics.rs](src/analytics.rs) and [src/gui/dashboard.rs](src/gui/dashboard.rs).
- **NEW…** (§9 NEW.md) — modal preset gallery. Lists every preset under
  `presets/`, lets you type a destination path + optional seed override and
  scaffold a fresh project tree from one. The new project is **not**
  auto-loaded; the gallery prints the next-step command. Backed by
  [src/presets.rs](src/presets.rs) and
  [src/gui/preset_gallery.rs](src/gui/preset_gallery.rs).

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
| `advance_sector(&mut sector)` | §5 NEXT — advance one conflict-simulation tick |
| `split_sector_save(&sector)` | §13 NEXT — extract IDs-only `SectorSave` from a sector |
| `merge_sector_save(&mut sector, save)` | §13 NEXT — re-apply runtime state to a fresh-from-catalog sector |
| `write_sector_save(path, &save)` / `load_sector_save(path)` | §13 NEXT — pretty-JSON save/load |
| `build_entity_world(&sector)` | §12 NEXT — flat ECS-style entity view (`EntityWorld`) |

Re-exported types: `AppConfig`, `SectorError`, `ProjectInput`, `InvariantReport`,
`InvariantViolation`, `GeneratedSector`, `GeneratedSystem`, `HexCoord`,
`Subsector`, `SubsectorConfig`, `SubsectorBuildError`, `ControlDenominator`,
`ConflictState`, `HYSTERESIS_TICKS`, `SectorSave`, `EntityWorld`,
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
| `RELATIONS_PAIR_UNKNOWN_FACTION` | `[[relations.pair_overrides]]` references a faction id that does not exist |
| `RELATIONS_KIND_RULE_EMPTY` | `[[relations.kind_rules]]` row has empty `a` or `b` kind |
| `REGIONS_COUNT_ZERO` | `regions.enabled = true` but `count = 0` |
| `REGIONS_COUNT_OVERFLOW` | `regions.count` exceeds half the grid cells |
| `REGIONS_MEAN_SIZE_ZERO` | `regions.mean_size = 0` |
| `REGIONS_CONDITION_BAD_WEIGHT` | A `regions.conditions[].weight` is non-finite or negative |
| `ECONOMY_TECH_MULTIPLIER_BAD` / `ECONOMY_POP_MULTIPLIER_BAD` | Multiplier is non-finite or negative |

Post-generation invariants ([src/invariants.rs](src/invariants.rs)):

| Code | Meaning |
|---|---|
| `REGION_HEX_OUT_OF_BOUNDS` | Region footprint hex falls outside the sector grid |
| `REGION_HEX_OVERLAP` | Two regions cover the same hex |
| `REGION_ISOLATES_SECTOR` | Region effects (storm / turbulence) leave the navigable route graph (non-Perilous lanes) split into ≥2 components |
| `ECONOMY_ENABLED_NO_WORLDS` | `economy.enabled = true` but no per-world entries were derived |

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
- [tests/analytics_and_presets.rs](tests/analytics_and_presets.rs) — §8/§9 NEW.md: analytics determinism + writers, preset scaffolding round-trip

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

**Add a new scenario preset.** Create `presets/<id>/preset.toml` with `title`,
`description`, and (optionally) `inherits = "<base>"` to reuse another
preset's data tree. Either supply a full project tree (`sectorforge.toml` +
`data/`) or set `inherits` and override just the files you want different.
Internal bundles prefixed with `_` (e.g. `_base`) are hidden from
`list-presets`. Verify with `cargo run --bin sectorforge -- new --out /tmp/t --preset <id>`.

**Use the analytics dashboard in CI.** `sectorforge analyze --project <DIR>
--strict` exits non-zero whenever any health flag fires. Combine with a tight
`[analyze]` block (e.g. `warn_faction_share = 0.40`) to gate merges on sector
quality. The JSON output (`analyze --json` or `analysis.json`) is stable
across runs, so a regression check is a diff away.

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
| [src/analytics.rs](src/analytics.rs) | §8 NEW.md analytics dashboard: faction balance + connectivity + flags |
| [src/presets.rs](src/presets.rs) | §9 NEW.md preset library + scaffolder (`new`, `list-presets`) |
| [src/search.rs](src/search.rs) | §2 NEW.md constraint-directed seed search (declarative wishes → deterministic seed enumeration) |
| [src/diff.rs](src/diff.rs) | §10 NEW.md model-aware sector diff (system/world/route/faction strata) and `diff_after_ticks` helper |
| [src/history.rs](src/history.rs) | §1 NEW.md deterministic chronicle: walks claims/dominance/archetype/conflict and emits dated `HistoryEvent`s with `M{epoch}.{ddd}` notation. Monotonic per anchor (foundation → annexation → reconquest). |
| [src/personae.rs](src/personae.rs) | §3 NEW.md deterministic dramatis personae: per-faction-kind name + title + trait + agenda pools anchored to system slots and world presences at a configurable dominance tier. |
| [src/hooks.rs](src/hooks.rs) | §7 NEW.md plot-hook generator: condition→template rules over the existing model (claims, hidden masters, archetype state, route hazard, blockades). Ranked by dramatic weight; player-edition redaction respects intel layer. |
| [src/prose.rs](src/prose.rs) | §6 NEW.md gazetteer prose: deterministic template grammar with seeded synonym rotation per system; gazetteer / dispatch tone presets. |
| [src/gui/dashboard.rs](src/gui/dashboard.rs) | §8 NEW.md GUI dashboard tab |
| [src/gui/preset_gallery.rs](src/gui/preset_gallery.rs) | §9 NEW.md GUI preset gallery modal |
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
| [src/hidden_routes.rs](src/hidden_routes.rs) | §3 NEXT: append `Webway` / `BlackShip` / `SmugglingLane` route variants between same-kind faction endpoints, ignoring the warp-distance cap. Each endpoint connects only to its `HIDDEN_K_NEAREST` closest peers (dedup'd) so the layer scales O(N) instead of O(N²) |
| [src/orbital_assets.rs](src/orbital_assets.rs) | §2 NEXT: discrete `OrbitalAsset` model (Station / Shipyard / DefensePlatform / BlockadeFleet) per system + `BlockadeReport` |
| [src/surface_region.rs](src/surface_region.rs) | §1 NEXT: per-world named `SurfaceRegion`s (Capital / Hive / Underhive / ForgeComplex / ShrineContinent / etc.) with per-region dominant faction |
| [src/conflict.rs](src/conflict.rs) | §5 NEXT: per-world + per-system `ConflictState` (momentum / intensity / mobilisation / attacker / defender / visible_controller) and a tick loop via `advance_sector`. Hysteresis (§11.3) lives in `advance_one` |
| [src/intel.rs](src/intel.rs) | §7 NEXT: fog-of-war `SystemIntel` keyed by observer faction (suspected presences, propaganda state, classified state, redaction helper) |
| [src/archetypes.rs](src/archetypes.rs) | §11 NEXT: eight faction archetype rules (Imperial governance stack / Necron phase / Tyranid front / Ork Waaagh! / Genestealer staged uprising / Tau sphere / Aeldari intermittent / Chaos corruption) populated into `GeneratedSystem.archetype` |
| [src/power_projection.rs](src/power_projection.rs) | §4 NEXT: per-faction route-graph BFS projection (`source_power × doctrine ÷ (1+hops²)`). Hidden routes are kind-gated. Exposed as `sector.power_projection` |
| [src/influence_field.rs](src/influence_field.rs) | §9 NEXT: continuous Voronoi-style cell assignment with `1/(1+d²)` falloff. Stored on `sector.influence_field` |
| [src/sector_save.rs](src/sector_save.rs) | §13 NEXT: `SectorSave` — IDs-only runtime state split from the static catalog half; `split` and `merge` for round-tripping |
| [src/world_ecs.rs](src/world_ecs.rs) | §12 NEXT: flat columnar `EntityWorld` adapter over `GeneratedSector` (System/World/Faction/Route entities) for callers that want an ECS-friendly shape without a `bevy_ecs` migration |
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
