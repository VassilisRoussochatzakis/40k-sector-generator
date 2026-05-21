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
  sector.json                  # canonical machine-readable sector, including chronicle when enabled
  sector.md                    # human-readable summary, including Sector History when present
  validation_report.json       # pre-generation validation note
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

Progress is printed to stderr with `[sectorforge]` prefixes: load, validation,
world-pool build, system generation, route/control overlays, invariant check,
and export. Stdout keeps the final summary, so scripted callers can redirect
stderr if they only want artifacts or JSON.

| Flag | Meaning |
|---|---|
| `--seed <SEED>` | Override `[generation].seed` from `sectorforge.toml` |
| `--out <DIR>` | Override `[outputs].directory` |
| `--allow-warnings` | Continue past warnings (errors still block) |
| `--heatmap <MODE>` | Override `[outputs.bitmap].heatmap` for PNG exports |
| `--theme <NAME>` | Override `[outputs.bitmap.theme].name` for PNG exports |
| `--no-faction-fill` | Disable dominant-faction tinting on PNG maps |

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

### `sectorforge analyze` (§8 old/DONE.md)

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

### `sectorforge new --out <DIR> --preset <NAME>` (§9 old/DONE.md)

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

### `sectorforge search` (§2 old/DONE.md)

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

### `sectorforge diff` (§10 old/DONE.md)

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

### `sectorforge history` (§1 NEW2.md/DONE)

Deterministic sector chronicle generator. Generated sectors now embed a
typed `chronicle` block in `sector.json` after structural generation and
overlays complete. It walks claims, dominance, archetype state, blockades,
routes, subsectors, warp regions, and conflict, then emits dated / era-labelled
events with stable IDs, typed entity refs, participating factions,
consequences, weights, and short template prose. Pure derivation — same sector
and same history config ⇒ same chronicle.

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
Project config may define eras and rule-forced events inline:

```toml
[history]
enabled = true
epoch_start = 36
epoch_end = 42

[[history.eras]]
id = "age_of_compliance"
label = "Age of Compliance"
relative_start = -900
relative_end = -650
weight = 1.0
allowed_events = ["Founding", "Compliance", "Treaty"]

[[history.event_rules]]
when_system_state = "Warzone"
prefer_event = "War"
minimum_events = 1
```

You can also point `[inputs].history = "history.toml"` at a file containing
the same top-level `[history]` table; its digest is recorded in the manifest.
`sector.md` gains a **Sector History** chapter and local history snippets in
system/world sections. The GUI has a **HISTORY** tab: selecting an event
highlights affected systems/routes on the map, and selected worlds show all
chronicle events that reference them.

### `sectorforge personae` (§3 old/DONE.md)

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

### `sectorforge hooks` (§7 old/DONE.md)

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

### `sectorforge prose` (§6 old/DONE.md)

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

### `sectorforge relations` (§5 NEW2.md/DONE)

Inter-faction diplomacy matrix. For every unordered pair of factions
present in the sector, derives public and secret attitudes (`Allied`,
`Friendly`, `Transactional`, `Suspicious`, `Hostile`,
`Existential Enemy`), directional `a_to_b` / `b_to_a` views, treaty
status, relation dimensions (`trust`, `fear`, `rivalry`,
`ideological_distance`, `economic_dependency`, `military_pressure`,
`covert_activity`), a short cause text, and a `tension` scalar
(0..=100). The legacy `stance` field is still emitted for existing
callers and mirrors the secret/mechanical stance.

```bash
# Regenerate + diplomacy.
cargo run --bin sectorforge -- relations --project examples/m42_project --out out/

# Diplomacy for an existing sector.json.
cargo run --bin sectorforge -- relations --sector out/sector.json --json
```

Writes `relations.md` + `relations.json`. Generated sectors embed the
matrix in `sector.json` under `relations`; `[generation.relations]`
controls how many low-presence factions enter the O(n²) pair matrix.

The TOML file ships **kind rules** (faction kind × kind → stance),
**disposition rules** (level delta on top of the base stance), and
legacy **pair overrides** (pin a specific `(faction_id, faction_id)`
stance). NEW2 **relation overrides** under `[[relations.overrides]]`
can pin public/secret attitudes, treaty status, and selected numeric
dimensions while leaving the rest derived:

```toml
[[relations.overrides]]
a = "fac-house-vorn"
b = "fac-rogue-trader-cassian"
public_attitude = "friendly"
secret_attitude = "hostile"
treaty_status = "charter"
trust = 35
rivalry = 70
reason = "Succession debt and a disputed charter"
```

Directional fields (`a_public_attitude`, `b_secret_attitude`, etc.)
make the matrix asymmetric. Built-in defaults — Imperial↔Chaos =
Existential Enemy / At War, Mechanicus↔Imperial = Friendly / Aligned,
Tyranid/Necron vs. anyone = Existential Enemy / At War, etc. — apply
when the file is silent. A small deterministic per-pair perturbation
seeded from `blake3("sectorforge:{seed}:relations:{a}:{b}")` breaks
ties so two identical kind+disposition pairs are not always identical.
Additional dimensions are derived from world overlap, contested claims,
route-control competition, faction power, covert visibility, and active
warzones.

Set `[relations].feed_conflict = true` to copy the flag onto the derived
matrix; `advance_sector` then biases per-world momentum/intensity by the
secret stance between the local attacker/defender (At War / Hostile
pushes toward the attacker, Allied / Aligned drifts toward peace). The
bias is applied *before* the existing tick logic and never overrides
GM-edited conflict state. Briefing profiles can keep only public
relations via `show_secret_relations = false`.

### `sectorforge regions` (§5 old/DONE.md)

Regional warp-phenomena overlay. Grows seeded blob regions over the hex
grid *before* route generation; the chosen condition modifies route
stability inside the footprint and tints PNG-export hexes for the
overlay:

* `WarpStorm` → forces routes crossing the footprint to `Perilous`, except
  sole navigable bridge lanes are capped at `Hazardous`.
* `Turbulence` → degrades by one stability tier, with the same bridge cap
  when the downgrade would make the route `Perilous`.
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
The file must also be referenced from `[inputs].regions`; an unreferenced
`data/routes/regions.toml` is ignored and the loader uses disabled defaults.
Regions are embedded on `sector.json` under the `regions` field and
rendered as a translucent tint underneath the existing hex grid in the
PNG export, interactive HTML map, and on-screen GUI sector map. Each
region also gets a subdued center label chip using the region name. The
Markdown sector map prints region glyphs over empty hexes: `~` warp
storm, `^` turbulence, `=` calm corridor, `#` blackout, `*` anomaly.

`Blackout` regions also gate the hidden-route stage: webway, black-ship,
and smuggling-lane endpoints inside a blackout footprint are excluded so
no covert lane terminates there. Post-generation invariants check that
region hexes stay inside the grid, that no two regions overlap, and
that the route graph restricted to non-Perilous lanes stays connected
after region effects are applied — the `REGION_ISOLATES_SECTOR`
violation surfaces when a storm splits the sector into multiple
components.

### `sectorforge economy` (§12 old/DONE.md / §4 NEW2.md)

Trade & resource economy snapshot. Each world declares a signed
production/consumption vector over six categories (ore, promethium,
foodstuffs, manufactured goods, archeotech, recruits) keyed by
`world_type × tech_level × population`. Per-route trade volume is
derived from the endpoint surplus/deficit gradient × distance falloff ×
hazard tier × piracy/interdiction friction; per-system and sector-wide
balance sheets fall out for free. The same pass also derives strategic
output bands (`food`, `ore`, `manufacturing`, `arms`, `ships`,
`pilgrimage`, `psyker_tithe`, `manpower`, `knowledge`, `xenos_value`),
dependency edges, `tithe_status`, `supply_risk`, and
`strategic_priority`.

```bash
# Regenerate + economy.
cargo run --bin sectorforge -- economy --project examples/m42_project --out out/

# Economy for an existing sector.json (built-in defaults apply).
cargo run --bin sectorforge -- economy --sector out/sector.json
```

Writes `economy.md`, `economy.json`, and `economy.csv` (per-world
vectors, strategic output, tithe/supply status, plus a `stranded`
boolean for worlds with shortages no inbound route can fix). The shipped
[data/worlds/economy.toml](examples/m42_project/data/worlds/economy.toml)
defaults to `enabled = false`; users can override the production matrix
per `world_type`, set per-`tech_level` multipliers, and set per-population
multipliers. Top-level `[resources.world_type.*]` and
`[resources.notable_feature.*]` tables override/add strategic output
rules, including `trade_multiplier` and `supply_resilience`. With
`feed_stability = true`, stranded-foodstuffs worlds
receive a bounded one-way nudge to
`stability.famine_or_resource_stress` (read-only; the conflict tick is
not perturbed).

When enabled, the economy snapshot also drives several downstream
surfaces:

* `sector.md` gains a dedicated **Economy** section (sector balance,
  strategic output, tithe/supply stress, stranded worlds, top trade lanes).
* The Route Planner annotates lifeline lanes — any non-Perilous route
  that is the *only* viable import of foodstuffs / promethium /
  manufactured into a deficit system is flagged Caution with an
  `only {resource} import into {sys}` note.
* The plot-hook generator (§7 old/DONE.md) emits `StarvingWorld` hooks for
  every stranded world and `LifelineLane` hooks for every critical
  supply route.
* Economy heatmaps include `TradeVolume`, `FoodOutput`, `TitheStress`,
  and `SupplyVulnerability`.
* The diff report (§10) carries scalar `economy_balance_changes` plus
  newly / no-longer stranded world lists. The post-gen invariants
  enforce a `ECONOMY_ENABLED_NO_WORLDS` check (enabled but empty world
  list signals a misconfigured derivation).

### `sectorforge interestingness` (§18 NEW2.md)

Score a sector against a target campaign profile (political sandbox / grim
collapse / mercantile / villainous / frontier). Each profile defines target
bands for metrics — faction Gini, contested-world ratio, warzone count,
route-graph properties, asymmetric control worlds — and the report ranks the
sector's fit on a 0-100 scale with strengths / weaknesses lines.

```bash
cargo run --bin sectorforge -- interestingness \
    --sector examples/m42_project/out/sector.json \
    --profile political_sandbox \
    --out out/
```

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Generate from project and score the result |
| `--sector <PATH>` | Score an existing `sector.json` |
| `--out <DIR>` | Write `interestingness.md` + `interestingness.json` |
| `--json` | Emit JSON to stdout |
| `--profile <ID>` | One of `political_sandbox`/`grim_collapse`/`mercantile`/`villainous`/`frontier` |

### `sectorforge briefing` (§9 NEW2.md)

Audience-targeted redaction pack. Applies one of the built-in briefing
profiles — GM full truth, Imperial Navy captain, Inquisitorial cell, Rogue
Trader dynasty, local governor, public atlas — and writes a redacted clone of
the sector plus a Markdown digest. Profile rules: hidden-route stripping,
relations clearing or public-only relations (`show_secret_relations =
false`), claim hiding, archetype scrubbing, intel sub-records. Reuses the
same intel-confidence cutoff as the HTML player edition.

```bash
cargo run --bin sectorforge -- briefing \
    --sector examples/m42_project/out/sector.json \
    --out out/ \
    --preset navy
```

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Generate from project and apply the profile |
| `--sector <PATH>` | Apply the profile to an existing `sector.json` |
| `--out <DIR>` | Required — output dir for the redacted pack |
| `--preset <ID>` | `gm` / `navy` / `inquisition` / `trader` / `governor` / `public` |
| `--observer <FID>` | Optional observer faction id (drives presence-visibility filter) |
| `--min-confidence <N>` | Override the preset's intel confidence cutoff (0..=100) |

Writes `briefing-<profile_id>.md` + `briefing-<profile_id>.json`.

### `sectorforge missions` (§3 NEW2.md)

Deterministic mission / quest seeds. Scans contested worlds, hidden masters,
mismatched claims, perilous routes, and uncharted systems, and emits typed
mission seeds — Investigate / Escort / Sabotage / Diplomacy / Assassination /
Recovery / Defense / Exploration — each with patron, target, primary +
secondary location, public objective, hidden complication, reward, and "if
ignored" consequence.

```bash
cargo run --bin sectorforge -- missions \
    --sector examples/m42_project/out/sector.json \
    --out out/

# Player edition hides GM-only complications.
cargo run --bin sectorforge -- missions --sector out/sector.json --player
```

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Generate from project and derive missions |
| `--sector <PATH>` | Derive from an existing `sector.json` |
| `--out <DIR>` | Write `missions.md` + `missions.json` |
| `--json` | Emit JSON to stdout |
| `--player` | Hide GM-only missions (Hidden-tier presences) |

### `sectorforge sites` (§7 NEW2.md)

Per-world points-of-interest: governor's palace, cathedral spire,
manufactorum, underhive sump-city, void elevator, star-fort dockyard,
quarantine zone, xenos ruin, pilgrim necropolis, astropathic choir, Arbites
precinct, data-vault, disputed shrine, penal mine, black-market enclave, cult
safehouse, crashed voidship, agri granary, forge reactor, tomb complex, naval
anchorage. Sites derive from world type, notable features, surface regions,
and faction presences; each carries a controller, a `public_status` vs.
`actual_status` pair (so hidden cult cells read as `abandoned` to non-GMs),
and a one-line hook.

```bash
cargo run --bin sectorforge -- sites \
    --sector examples/m42_project/out/sector.json \
    --out out/

# Player edition hides sites whose public/actual status differ.
cargo run --bin sectorforge -- sites --sector out/sector.json --player
```

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Generate from project and derive sites |
| `--sector <PATH>` | Derive from an existing `sector.json` |
| `--out <DIR>` | Write `sites.md` + `sites.json` |
| `--json` | Emit JSON to stdout |
| `--player` | Hide sites with mismatched public/actual status |

### `sectorforge compose` (§14 NEW.md)

Compose a **segmentum** — several independently-generated child sectors
stitched together with deterministic inter-sector warp links. Each child
runs through the unchanged generation pipeline; a `stitch` stage seeded
from `blake3("sectorforge:{stitch_seed}:stitch:{a}:{b}")` then picks
border-system pairs across each adjacent super-grid edge. Same
`segmentum.toml` + same children + same `sectorforge` version ⇒ same
bytes.

```bash
cargo run --bin sectorforge -- compose \
    --segmentum path/to/segmentum.toml \
    --out out/segmentum
```

| Flag | Meaning |
|---|---|
| `--segmentum <FILE>` | Path to `segmentum.toml` |
| `--out <DIR>` | Output directory (created if missing) |
| `--stitch-seed <S>` | Override the file's `stitch_seed` |
| `--json` | Print the composed segmentum JSON to stdout instead of writing files |

Composition also reports progress to stderr. It logs each child load,
validation, sector-generation milestones, invariant check, child export, and
the final stitch stage. In `--json` mode the composed JSON still goes to stdout;
progress remains on stderr.

Output layout:

```
out/segmentum/
  segmentum.md
  segmentum.json
  super_manifest.json
  children/
    <child-id-1>/    # full per-child generate output (sector.json, csv/, png, ...)
    <child-id-2>/
```

Open the composed segmentum in the GUI with:

```bash
cargo sgui --segmentum out/segmentum/segmentum.json
```

The **SEGMENTUM** tab shows the scaled super-map, super-grid, aggregate
counts, child-sector roster, and inter-sector stitch links. Use **OPEN MAP**
/ **CHILD** to swap the active component sector without closing the GUI;
link endpoint buttons jump directly into the relevant child system view.

Example `segmentum.toml`:

```toml
[segmentum]
id           = "seg-pacificus"
title        = "Segmentum Pacificus"
stitch_seed  = "stitch-001"
columns      = 2
rows         = 1
faction_mode = "shared"     # "shared" | "independent"

[stitch]
max_links_per_pair = 2
border_depth       = 2
default_route_type = "charted_passage"
default_stability  = "unstable"

[[children]]
id      = "alpha"
project = "examples/m42_project"
column  = 0
row     = 0
seed    = "alpha-seed"   # optional override of [generation].seed

[[children]]
id      = "beta"
project = "examples/m42_project"
column  = 1
row     = 0
seed    = "beta-seed"
```

`faction_mode = "shared"` treats matching faction ids across children as
the same entity (rosters aggregate downstream); `"independent"` keeps
each child's roster isolated. The super-manifest digests every child's
canonical sector JSON so the audit chain extends cleanly from a sector
to a segmentum: same seeds + same digests ⇒ same composed bytes.

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
    factions/relations.toml        # §5 NEW2.md/DONE (optional)
    routes/route_rules.toml
    routes/regions.toml            # §5 old/DONE.md (optional)
    worlds/economy.toml            # §12 old/DONE.md (optional)
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
relations             = "data/factions/relations.toml"     # optional (§5 NEW2.md/DONE)
regions               = "data/routes/regions.toml"         # optional (§5 old/DONE.md)
economy               = "data/worlds/economy.toml"         # optional (§12 old/DONE.md)

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

[generation.relations]
# Minimum world presence required for a faction to appear in the diplomacy
# matrix. The full canonical faction catalogue is ~1000 entries; C(n,2)
# pairs scale quadratically, so a loose threshold blows up sector.json by
# tens of MB on large sectors.
#   1 (default) — every faction with any world presence anywhere
#   2           — drop incidental single-world cameos
#   3+          — only regionally relevant factions
min_world_presence         = 1

[outputs]
directory                  = "out"
formats                    = ["json", "markdown", "csv", "bitmap", "html"]
pretty_json                = true
write_per_system_files     = false         # true = also write duplicate systems/sys-NNNN.json files
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
# theme_file        = "data/map_themes/navis.toml" # optional; project-relative, digested into manifest.input_digests

[outputs.bitmap.theme]
name = "gm_dark"                 # gm_dark | print_mono | imperial_archive | navis_tactical | inquisition_redacted | subsector_political
# Optional inline overrides accept #RRGGBB or #RRGGBBAA:
# background = "#05070a"
# show_subsector_borders = true
# route_line_mode = "hazard_weighted"   # standard | hazard_weighted
# label_density = "all"                 # all | important_only | none
# legend = "full"                       # full | compact | hidden
# symbol_set = "standard"               # standard | tactical | redacted

# §11 NEW.md: self-contained interactive HTML map. Only honoured when
# `"html"` is listed in `formats` above. Output is byte-deterministic from
# the inlined sector — same seed + same theme ⇒ same bytes.
[outputs.html]
theme               = "dark"        # dark | parchment | hololithic
# When set, the inlined sector is redacted through `intel::redact_world_for_observer`
# so Hidden-tier presences below `player_min_confidence` (0..=100) and other
# observers' intel records are stripped before serialisation.
# player_observer   = "imperium"
player_min_confidence = 30
size_warn_bytes     = 8388608       # warn (stderr) above this; does not block
compact_json        = true          # pretty JSON would ~double the file size
```

The CLI accepts `--heatmap <mode>`, `--theme <name>`, and
`--no-faction-fill` on `generate` to override bitmap settings without editing
the TOML. `theme_file` is read through the project loader, so its BLAKE3 digest
appears in `manifest.input_digests`; inline theme overrides are already covered
by the `sectorforge.toml` digest.

Map themes are presentation-only. They do not alter generated sector data,
route topology, faction placement, or JSON/Markdown/CSV facts. The built-ins
are:

| Theme | Use |
|---|---|
| `gm_dark` | Default high-contrast screen map |
| `print_mono` | Black-and-white printable handout |
| `imperial_archive` | Parchment/gazetteer style |
| `navis_tactical` | Route-first naval chart; compact legend; important labels |
| `inquisition_redacted` | Classified red/black briefing style |
| `subsector_political` | Strong subsector borders and faction tinting |

Route visuals are also presentation-only. GUI sector maps use one canonical
`RoutePattern` per route type so the sector info-panel legend is an exact guide:
solid lines, dashed lanes, dotted lanes, bursts, dot clusters, and dense dot
trails.

Active route types are `StableWarpLane`, `ChartedPassage`, `SecretPassage`,
`Webway`, `BlackShip`, and `SmugglingLane`. GUI and PNG legends plus the route
editor dropdown use this same route-type set; route danger is represented by
`RouteStability` (`Stable`, `Unstable`, `Hazardous`, `Perilous`), not by a
separate route type.

For compatibility with the proposal syntax, a top-level `[map_theme]` table is
also accepted and merged into `[outputs.bitmap.theme]`.

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

Each entry is a **force** row. The generator rolls rows up into a
three-level hierarchy:

- **Faction** — highest level, e.g. `imperial` / Imperium, `chaos` / Chaos,
  `ork` / Orks.
- **Subfaction** — middle level, e.g. `imperial_guard`, `chaos_space_marine`.
- **Force** — specific catalogue row, e.g. Cadian 109th Regiment, Emperor's
  Children warband, named dynasty, sept, chapter, cult, or fleet.

Legacy files that only specify `kind` still work: `kind` becomes the
subfaction id, and built-in mappings derive the top faction
(`imperial_guard` -> `imperial`, `chaos_space_marine` -> `chaos`, etc.);
unknown custom kinds remain their own top-level faction.
Use optional `faction` / `faction_name` and `subfaction` /
`subfaction_name` fields when a row needs explicit hierarchy.
Preferred-* values must use the **variant name** form from `src/worlds.rs`
(e.g. `"HiveWorld"`, not `"Hive World"`). Validation warns on unknown values.

```toml
[[factions]]
faction = "imperial"                  # optional top-level override
faction_name = "Imperium"             # optional display name
subfaction = "imperial_guard"         # optional middle-level override
subfaction_name = "Imperial Guard"    # optional display name
id    = "cadian_109th"
name  = "Cadian 109th Regiment"
kind  = "imperial_guard"             # legacy classification / control profile
weight = 10.0
default_disposition = "lawful"
preferred_world_types        = ["HiveWorld", "BastionWorld"]
preferred_governments        = ["MilitaryGovernor", "MagistrateCouncil"]
preferred_notable_features   = ["AdministrativeHub", "PoliceState"]
```

Assignment algorithm: base weight × 1.5 for matching world type, × 1.4 for
matching government, × 1.3 per matching notable feature. Up to 3 factions
per world (capped by population density), selected at the subfaction/force
level and rolled up to the top-level `faction_id`. Per-world presence rows
emit `faction_id`, `subfaction_id` / `subfaction_name`, and `force_id` /
`force_name`, sorted by influence (Dominant > Significant > Minor > Hidden)
then catalog order.

`primary_factions` for a system is the top-3 by **influence-weighted score**
(spec §10.9): sum of `influence.weight()` over the top-level faction's presence on
that system's worlds (Dominant=3, Significant=2, Minor=1, Hidden=0.5). Ties
break by world-appearance count, then catalog order, then faction id.

The GUI **Factions → DESIGNER** mode can also author this file from scratch.
Pick an overall faction preset (Imperial, Mechanicus, Astartes, Chaos,
Xenos, custom, etc.), edit the `kind`, `id`, `name`, disposition, weight,
and preference lists, add rows to the designer roster, then **SAVE TOML...**
to write a normal `[[factions]]` catalog. **REPLACE FROM OUTPUT** converts
the currently loaded generated sector's forces back into editable faction
rows, using each generated subfaction id as the saved `kind`; because
generated output does not retain original catalog weights, the designer
derives a modest presence-based weight from each output force's system/world
footprint.

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

Default-valued state fields (`control`, `stability`, `blockade`,
`conflict`, `intel`, `archetype`) are omitted from the serialized
`sector.json` when their value equals the type default. They round-trip
back to defaults on load via `#[serde(default)]`. This keeps large
sectors compact (>5× shrink on a 200-system sector). Per-system `intel`
is also scoped to observer factions with at least one presence in the
system; rumor views for unrelated observers can be reconstructed on
demand from the raw system state.

`sector.json` is the canonical JSON artifact and already contains every
`GeneratedSystem`. Per-system JSON files under `systems/sys-NNNN.json`
duplicate those entries and are off by default. Enable
`[outputs].write_per_system_files = true` only when a downstream tool needs
standalone system files. Per-system bitmap PNGs are controlled separately by
`[outputs.bitmap].render_systems`. When per-system JSON is disabled, export
removes stale `systems/<current-system-id>.json` files but leaves PNGs and
other files untouched.

`GeneratedFaction` is the top-level faction rollup. Its `subfactions`
array contains middle-level `GeneratedSubfaction` rows, and each subfaction
can contain `forces` for the specific catalogue entries selected during
generation. Per-world `WorldFactionPresence` rows carry all three ids:
`faction_id`, optional `subfaction_id`, and optional `force_id` plus display
names.

The `relations` matrix is emitted only for factions with non-empty
`system_presence` or `world_presence`. The full canonical faction
catalogue (~1000 entries on the bundled data set) would generate
C(n,2) ≈ 500k pairs and >60 MB of JSON; filtering on actual sector
presence reduces this to the meaningful subset.

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
fields (factions, subfactions, forces, tags, features) are `;`-separated
within a single cell.

### `sector.html` (§11 NEW.md)

Self-contained, fully **offline** interactive map. Single file — no
external assets, no network calls — with the sector JSON inlined alongside
a small vanilla-JS canvas renderer (`src/html_export/renderer.js`):

- Pan (click-drag) and zoom (wheel).
- Click a system → side panel with worlds, primary factions, control.
- Heatmap toggle (off / control / worlds / presences / factions).
- Faction-fill tint toggle.
- Faction filter chips — click to hide systems dominated by that faction.
- Routes / labels visibility toggles.

Configured under `[outputs.html]`. The `player_observer` field runs the
existing `intel::redact_world_for_observer` over the sector before
inlining, so Hidden-tier presences below `player_min_confidence` and
non-observer intel records are stripped — yielding a shareable "player
edition" that's still byte-deterministic.

Output bytes depend only on the sector + theme + redaction settings; the
generator stamps no timestamps into the file. The exporter warns on
stderr when the resulting file exceeds `size_warn_bytes` (default 8 MiB)
but never blocks the write.

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
  faction tint, subsector overlay, deterministic route-pattern geometry,
  and a translucent warp-region tint plus subdued center labels
  (§5) for every `WarpRegion`. Click a hex to drill into the system. The
  bottom controls expose a **HEATMAP** dropdown that
  tints every system hex by a per-mode score: `CONTROL` (dominant-faction
  colour × control-score intensity), `MILITARY`, `TRADE`, `INDUSTRY`,
  `COVERT`, `FAITH`, `THREAT` (military × covert restricted to
  hostile/zealous), `INTEL` (low-visibility hexes glow), `TENSION`
  (§4 — sum of hostile/at-war pair tensions per system), or `TRADE VOL`
  (§12 — sum of incident route trade volumes). See
  [src/gui/heatmap.rs](src/gui/heatmap.rs).
  Heatmap cells are cached per loaded sector and mode, so toggling a
  non-`OFF` heatmap does not rescore every frame; the cache is invalidated
  when a new sector loads or live faction edits change map data.
  The sector info panel also caches its faction legend buckets for the same
  loaded-sector lifetime instead of rebuilding the rollup every repaint.
- **System** — per-system detail panel: worlds, coords, star type, tags,
  factions, neighboring systems.
- **Edit** — sector editor (rename systems, add/remove worlds, adjust tags
  and per-world factions). The **Factions** tab shows a deterministic colour
  + glyph chip per faction (derived from `kind`, `id`, `disposition` — see
  [src/gui/palette.rs](src/gui/palette.rs) `faction_style`) and lets you
  filter by kind/disposition, sort by total power, and pin favourites to
  the top.
- **Factions** — high-level sector faction view. It lists top-level factions,
  their palette chip, kind, disposition, power, sector summary presence, and
  observed per-world presence while hiding subfaction and force details. Toggle
  **EDIT MODE** for broad changes: rename factions, adjust kind/disposition,
  add/delete top-level factions, set all/none system or world summary
  presence, or rebuild summary presence from world records. Toggle
  **DESIGNER** for ground-up catalog work: choose an overall faction preset
  or custom kind, add/edit export rows with weights and world preferences,
  import the loaded output's generated forces with **REPLACE FROM OUTPUT**,
  and save the result as a `factions.toml`-compatible TOML file.
- **Data** — CSV data editor for `key.csv` / `generator.csv` from inside
  the app.
- **Planner** — route planner: pick `from` / `to` systems and pathfind over
  the existing route graph. Two metrics: `Safest` (Dijkstra with hazard
  weights — avoid `Unstable` / `Hazardous`; `Perilous` routes are impassable)
  or `Shortest` (BFS over hop count).
- **Diplomacy** (§5 NEW2.md/DONE) — table view of
  `sector.relations.pairs`: every faction pair with public/secret
  attitudes, treaty status, tension scalar, and cause text. Backed by
  [src/gui/app/mod.rs](src/gui/app/mod.rs)
  `draw_relations_layout`.
- **Regions** (§5 old/DONE.md) — table view of `sector.regions`: id, name,
  kind, hex count, centre coord. Pairs with the in-map region tint.
- **Trade** (§12 old/DONE.md) — table view of `sector.economy`: sector
  resource balance, top trade lanes by volume, list of stranded worlds
  with shortages.
- **Dashboard** (§8 old/DONE.md) — analytics for the loaded sector. Faction-share
  bars coloured by the same per-faction palette the map uses, a Gini
  coefficient, contested-world / claim summary, route-graph connectivity
  callout (component count, diameter, articulation points, isolated systems),
  world / star / population / route distributions, per-subsector political
  variety, and a list of health flags. Backed by
  [src/analytics.rs](src/analytics.rs) and [src/gui/dashboard.rs](src/gui/dashboard.rs).
- **NEW…** (§9 old/DONE.md) — modal preset gallery. Lists every preset under
  `presets/`, lets you type a destination path + optional seed override and
  scaffold a fresh project tree from one. The new project is **not**
  auto-loaded; the gallery prints the next-step command. Backed by
  [src/presets.rs](src/presets.rs) and
  [src/gui/preset_gallery.rs](src/gui/preset_gallery.rs).

The GUI also supports exporting bitmap PNGs at a configurable scale and theme:
sector overview, a single system map, or all per-system maps. The current
HEATMAP selection in the sector view is carried into the exported sector PNG.

### Launching the GUI

A `cargo sgui` alias is registered in [.cargo/config.toml](.cargo/config.toml):

```bash
# From a project directory (auto-loads out/sector.json if present)
cargo sgui --project examples/m42_project

# Direct path to a sector.json
cargo sgui examples/m42_project/out/sector.json

# Composed segmentum overview + child-sector switching
cargo sgui --segmentum out/segmentumTEST/segmentum.json

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
| `generate_sector_with_progress(input, cb)` | Same generation, emitting `SectorProgress` callback events |
| `generate_system_standalone(input, index, coord)` | Deterministic single-system generation, returns `GeneratedSystem` |
| `validate_sector(&sector)` | Post-generation invariant check (spec §11.11), returns `InvariantReport` |
| `compose_segmentum(&file, base_dir, out)` | Generate child sectors and compose a `Segmentum` |
| `compose_segmentum_with_progress(&file, base_dir, out, cb)` | Same segmentum composition, emitting `SegmentumProgress` events |
| `write_segmentum(dir, &segmentum)` | Write segmentum Markdown, JSON, and super-manifest |
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
`MapTheme`, `MapThemeConfig`, `LabelDensity`, `LegendStyle`,
`RouteLineMode`, `SymbolSet`, `ValidationIssue`, `ValidationReport`,
`HistoryConfig`, `SectorChronicle`, `HistoryEvent`, `SectorProgress`,
`Segmentum`, and `SegmentumProgress`.

### Typed identifiers

IDs are strongly typed via the newtypes in [src/ids.rs](src/ids.rs):

| Type | Wraps | Used for |
|---|---|---|
| `SystemId` | `String` | `GeneratedSystem.id`, route endpoints, system-keyed maps |
| `WorldId` | `String` | `GeneratedWorld.id`, world-keyed maps, stranded-world lists |
| `FactionId` | `String` | `GeneratedFaction.id`, presence rows, control summaries |
| `RouteId` | `String` | `GeneratedRoute.id`, hidden-route layers, route-keyed maps |

Each newtype is `#[serde(transparent)]` so on-disk JSON is unchanged from the
earlier String-based representation: existing `sector.json` files round-trip
without migration. The Rust API still gets compile-time separation — passing a
`FactionId` where a `SystemId` is expected is a type error.

Constructing IDs from string literals stays ergonomic via `From<&str>` and
`From<String>`:

```rust
use sectorforge::ids::{FactionId, SystemId, WorldId, RouteId};

let sys: SystemId = "sys-0001".into();
let wid = WorldId::new(format!("{sys}-w01"));
assert_eq!(sys, "sys-0001"); // PartialEq<&str>/<str>/<String> all defined
```

The newtypes also implement `Deref<Target = str>`, `AsRef<str>`, `Display`,
`Borrow<str>`, and `Ord` so they work with `BTreeMap<SystemId, _>`,
`HashSet<RouteId>`, `format!("{}", id)`, and `map.get(id.as_str())` without
ceremony.

For GUI text-edit fields, [src/gui/editor/ui_helpers.rs](src/gui/editor/ui_helpers.rs)
exposes `text_field_id`, `combo_str_id`, and `combo_kv_id` — generic wrappers
over the `&mut String` versions that round-trip the typed id through a
temporary `String` buffer so `egui::TextEdit` keeps working unchanged.

### Async / runtime model

`sectorforge` is **fully synchronous**. There is no `tokio` / `async-std`
dependency, no `async fn` in the public API, no spawned background tasks, no
shared mutable state behind locks. Generation, validation, export, and GUI
update all run on the calling thread; long operations are bounded by the size
of the project input. If you need to drive generation from an async runtime,
wrap calls in `tokio::task::spawn_blocking` (or your runtime's equivalent) at
the boundary — there is no internal `await` point to coordinate with.

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
| `RELATIONS_OVERRIDE_UNKNOWN_FACTION` | `[[relations.overrides]]` references a faction id that does not exist |
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
- [tests/analytics_and_presets.rs](tests/analytics_and_presets.rs) — §8/§9 old/DONE.md: analytics determinism + writers, preset scaffolding round-trip

Benchmarks (criterion):

```bash
cargo bench --bench generation            # full sample
cargo bench --bench generation -- --quick # ~10s smoke
```

Benches in [benches/generation.rs](benches/generation.rs) cover `generate_sector` at three sector sizes (8×10 / 16×20 / 24×30), `validate_project`, and `validate_sector_invariants`.

Generation builds a world candidate pool once per project load. That pool also
caches star-colour weight totals, so per-system star selection does not rescan
every workbook row.

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
| [src/generation.rs](src/generation.rs) | Placement, systems, worlds, factions, routes, and `SectorProgress` callback events. `build_system` is the unit reused by sector + standalone APIs |
| [src/sector_model.rs](src/sector_model.rs) | Output DTOs (`GeneratedSector` etc.) with `Serialize` + `Deserialize` |
| [src/control.rs](src/control.rs) | Faction presence → dimension scores, claims, multi-winner control summaries, and per-faction `PowerProfile` aggregation |
| [src/validation.rs](src/validation.rs) | All pre-generation checks |
| [src/invariants.rs](src/invariants.rs) | Spec §11.11 post-generation invariants |
| [src/render.rs](src/render.rs) | Pure Markdown rendering (sector + standalone system). Includes faction display buckets (§15) and per-world / per-system stability (§11.1) |
| [src/export.rs](src/export.rs) | JSON / Markdown / CSV / manifest writers + bundle export |
| [src/html_export.rs](src/html_export.rs) | §11 NEW.md self-contained interactive HTML map: inlines sector JSON + theme CSS + vanilla-JS canvas renderer; supports player-edition redaction via the intel layer. Byte-deterministic. |
| [src/map_theme.rs](src/map_theme.rs) | §13 NEW2.md bitmap map themes: built-in palettes, custom TOML theme parsing, color validation, label/legend/route/symbol style knobs |
| [src/bitmap/mod.rs](src/bitmap/mod.rs) | Sector PNG rendering (`image` crate); coordinates hex grid + routes + systems + themed legend |
| [src/bitmap/primitives.rs](src/bitmap/primitives.rs) | Pixel-level drawing primitives + embedded 5×7 font, shared with `system_map` |
| [src/system_map.rs](src/system_map.rs) | Per-system PNG rendering; honours `outputs.bitmap.faction_fill` plus bitmap map themes |
| [src/subsectors/mod.rs](src/subsectors/mod.rs) | Subsector clustering (k-means / Lloyd) + public API |
| [src/subsectors/summary.rs](src/subsectors/summary.rs) | Ownership resolution, faction-control tallies, capital selection |
| [src/analytics.rs](src/analytics.rs) | §8 old/DONE.md analytics dashboard: faction balance + connectivity + flags |
| [src/presets.rs](src/presets.rs) | §9 old/DONE.md preset library + scaffolder (`new`, `list-presets`) |
| [src/search.rs](src/search.rs) | §2 old/DONE.md constraint-directed seed search (declarative wishes → deterministic seed enumeration) |
| [src/diff.rs](src/diff.rs) | §10 old/DONE.md model-aware sector diff (system/world/route/faction strata) and `diff_after_ticks` helper |
| [src/history.rs](src/history.rs) | §1 NEW2.md/DONE deterministic `SectorChronicle`: typed dated / era-labelled history events with entity refs, consequences, route/subsector/region anchors, and `M{epoch}.{ddd}` notation. |
| [src/personae.rs](src/personae.rs) | §3 old/DONE.md deterministic dramatis personae: per-faction-kind name + title + trait + agenda pools anchored to system slots and world presences at a configurable dominance tier. |
| [src/hooks.rs](src/hooks.rs) | §7 old/DONE.md plot-hook generator: condition→template rules over the existing model (claims, hidden masters, archetype state, route hazard, blockades). Ranked by dramatic weight; player-edition redaction respects intel layer. |
| [src/prose.rs](src/prose.rs) | §6 old/DONE.md gazetteer prose: deterministic template grammar with seeded synonym rotation per system; gazetteer / dispatch tone presets. |
| [src/relations.rs](src/relations.rs) | §5 NEW2.md diplomacy matrix: public/secret faction attitudes, treaty status, directional views, trust/fear/rivalry/economic/military/covert dimensions, legacy stance compatibility, and relation Markdown/JSON report writer. |
| [src/segmentum.rs](src/segmentum.rs) | §14 NEW.md multi-sector composition: `segmentum.toml` loader, child-sector progress callbacks, deterministic stitch stage (`blake3("sectorforge:{stitch_seed}:stitch:{a}:{b}")`), inter-sector links, super-manifest, Markdown super-map. |
| [src/interestingness.rs](src/interestingness.rs) | §18 NEW2.md interestingness scorecard: weighted target-band fit over `[crate::analytics]` metrics, five built-in profiles (political_sandbox / grim_collapse / mercantile / villainous / frontier). |
| [src/briefing.rs](src/briefing.rs) | §9 NEW2.md briefing profiles: six audience presets (gm / navy / inquisition / trader / governor / public) that combine the existing intel redaction primitives with hidden-route, relations, claim, archetype, and orbital-asset stripping. |
| [src/missions.rs](src/missions.rs) | §3 NEW2.md mission seed generator: typed Investigate / Escort / Sabotage / Diplomacy / Assassination / Recovery / Defense / Exploration seeds keyed off contested worlds, hidden masters, mismatched claims, perilous routes, and uncharted systems. |
| [src/sites.rs](src/sites.rs) | §7 NEW2.md planetary points-of-interest: 21 site kinds (governor's palace, cathedral spire, manufactorum, underhive, cult safehouse, …) derived from world type / features / surface regions, with `public_status` vs. `actual_status` masking and one-line hooks. |
| [src/gui/dashboard.rs](src/gui/dashboard.rs) | §8 old/DONE.md GUI dashboard tab |
| [src/gui/preset_gallery.rs](src/gui/preset_gallery.rs) | §9 old/DONE.md GUI preset gallery modal |
| [src/config.rs](src/config.rs) | `sectorforge.toml` schema |
| [src/input.rs](src/input.rs) | Project loader (config + inputs + digests) |
| [src/names.rs](src/names.rs) | Name table types |
| [src/factions.rs](src/factions.rs) | Faction file types |
| [src/routes.rs](src/routes.rs) | Route-rules file types |
| [src/rng.rs](src/rng.rs) | Stage-based deterministic RNG |
| [src/taxonomy.rs](src/taxonomy.rs) | Variant-name ↔ enum bridge |
| [src/ids.rs](src/ids.rs) | Typed-id newtypes (`SystemId` / `WorldId` / `FactionId` / `RouteId`, `#[serde(transparent)]`) + canonical id-string constructors |
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
| [src/gui/factions_overview.rs](src/gui/factions_overview.rs) | High-level faction overview and broad edit-mode controls |
| [src/gui/data_editor.rs](src/gui/data_editor.rs) | CSV data editor UI |
| [src/gui/route_planner.rs](src/gui/route_planner.rs) | Route planner (Safest / Shortest) |
| [src/gui/info_panel.rs](src/gui/info_panel.rs) | Text formatting widgets |
| [src/gui/editor/](src/gui/editor/) | Sector/world editing UI (map, settings, factions, routes, worlds, systems) |
| [src/gui/palette.rs](src/gui/palette.rs) | Color palette for GUI; egui wrapper around [src/faction_style.rs](src/faction_style.rs) (`faction_style`, glyph + border) |
| [src/gui/heatmap.rs](src/gui/heatmap.rs) | egui wrapper around [src/heatmap.rs](src/heatmap.rs) — same scoring, returns `Color32` cells |
