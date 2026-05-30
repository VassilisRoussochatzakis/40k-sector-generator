# RANDOM.md — "Random Sector" generation (size-only → fully complete sector)

**Goal.** Given *nothing but a size*, produce a fully populated `GeneratedSector`
**with every feature on**, and open it for viewing/editing in the builder (also as a
CLI subcommand). No project directory, no hand-authored `sectorforge.toml`, no data
files. Pick a size, get a complete sector.

**Hard requirement (this revision):** *no gotchas, no silent defaults.* The path must
synthesize an **entirely new, fully-randomized `sectorforge.toml`** in which every
section is present and every overlay is **explicitly enabled** — including regions
and economy — and it must run the post-generation derivations that `generate` does
*not* do. Anything that ships off-by-default must be turned on by us, not left to a
default.

> **Status: ✅ IMPLEMENTED.** This document began as the design/scope; the
> feature now exists end-to-end. Per-section status markers are inline below;
> the file-by-file checklist is §11.
>
> - **Engine** — `src/gen/random_sector.rs` (`SectorSize`, `mint_seed`,
>   `build_random_config`, `generate_random_sector`, `RandomReport`). ✅
> - **CLI** — `sectorforge random` (`src/cli/random.rs`). ✅
> - **Builder** — **Random sector…** wizard (`ModalKind::GenerateRandom`,
>   `builder/src/builder/panels/generate_random.rs`). ✅
> - **Preset** — `presets/_full/` (every overlay enabled + the new
>   `hooks.toml` / `missions.toml` / `prose.toml`). ✅
> - **Tests** — `tests/it/random_sector_tests.rs` (completeness, determinism,
>   CLI, `_full` preset) + module unit tests. ✅
> - **Docs** — [GUIDE.md](GUIDE.md) (`sectorforge random` + source map) and
>   [BUILDER.md](BUILDER.md) §1.5. ✅
> - **Deferred (Phase 3, optional — not done):** `include_dir!`-embedded
>   self-contained binary; `--fixed-shape`; an explicit density knob.

---

## 1. TL;DR

The generation engine **already produces most of a complete sector end-to-end.** The
orchestrator `generation::generate_with_progress_and_cancel`
(`src/gen/generation/mod.rs:246`) runs ~15 stages and emits systems, worlds,
factions, routes, control, conflict, intel, orbital assets, archetypes,
power_projection, influence_field, relations, economy, chronicle, and regions.

But "fully complete with no gotchas" is **not** just "call generate." Three concrete
realities force the design:

1. **`GenerationConfig` has no `Default`** (`src/loading/config.rs:92`): `seed`,
   `sector_width`, `sector_height`, `system_count`, `min_worlds_per_system`,
   `max_worlds_per_system` are required. A size-only path must fill the whole config.

2. **Two overlays are OFF by default *and the bundled data files actively disable
   them*.** `RegionsConfig.enabled` (`src/gen/regions.rs:154`) and
   `EconomyConfig.enabled` (`src/analysis/economy.rs:72`) default to `false`, and
   `presets/_base/data/regions.toml` / `economy.toml` **literally set `enabled = false`**.
   So scaffolding `_base` and merely *referencing* those files still yields no regions
   and no economy. They must be written/patched to `enabled = true`.

3. **Five features are not produced by `generate` at all.** `personae`, `sites`,
   `hooks`, `missions`, `prose` are **post-generation derivations**, invoked
   separately (`derive_personae_with`, `derive_sites_with`, `derive_hooks_with`,
   `derive_missions_with`, `derive_prose_with`; the builder calls
   `BuilderState::recompute_*`). A truly complete sector must run these *after*
   generate.

So the feature is: **(a) synthesize a complete, randomized config + overlay data
files with everything enabled, (b) generate, (c) run the five post-gen derivations,
(d) install into the builder.** Plus, optionally, **(e) bake the "everything" wiring
into a checked-in preset.**

---

## 2. What "a fully complete sector" contains — and where each piece comes from

### 2.1 Emitted by `generate` (lives in `GeneratedSector`)

Root type: `GeneratedSector` (`src/model/sector_model/mod.rs:16`).

**Always present (required):** `id`, `title`, `seed`, `generator_name/version`,
`width`, `height`, `systems` (each: coord, kind, star, worlds, surface regions,
control, stability, conflict, intel, orbital assets), `routes` (type, stability,
per-faction controls), `factions` (presence + `PowerProfile`), `manifest`.

Per-world detail (`GeneratedWorld` `:155`, `WorldDto` `:201`): star colour, world
type, atmosphere, temperature, biosphere, population, tech level, government, notable
features, surface regions, tags/notes.

**Overlays — gating verified in `src/gen/generation/mod.rs`:**

| Overlay | Field | Gate | Default | Callsite |
|---|---|---|---|---|
| Relations | `relations` | none — always runs | populated | `:705` `relations::derive_with_threshold` |
| Economy | `economy` | `economy.enabled` | **OFF** ⚠ | `:721` `economy::derive_with` |
| Chronicle | `chronicle` | `history.enabled` | ON (default true) | `:737` `history::derive_with_progress` |
| Regions | `regions` | `regions.enabled` | **OFF** ⚠ | regions stage |
| Surface regions / conflict | per world/system | always | populated | `:568`,`:569`,`:574` |
| Orbital assets / blockade | per system | always | populated | `:571` |
| Intel | per system | always | populated | `:582` |
| Route controls | per route | always | populated | `:543` |
| Power projection, influence field, archetypes | sector | always | populated | overlay block |

### 2.2 NOT emitted by `generate` — post-gen derivations (separate calls)

These are **not** in `generate`'s stage list. Each takes a finished `GeneratedSector`
+ its config and returns a report. They are written as separate output files / shown
in their own builder panels.

| Feature | lib entry (crate root re-export) | Report | Builder | Config |
|---|---|---|---|---|
| Personae | `sectorforge::derive_personae_with(&sector, &cfg)` | `PersonaeReport` | `recompute_personae` | `PersonaeConfig` `src/analysis/personae.rs:33` |
| Sites | `sectorforge::derive_sites_with(&sector, &cfg)` | `SitesReport` | `recompute_sites` | `SitesConfig` `src/gen/sites.rs:28` |
| Hooks | `sectorforge::derive_hooks_with(&sector, &cfg)` | `HooksReport` | `recompute_hooks` | `HooksConfig` `src/analysis/hooks.rs:30` |
| Missions | `sectorforge::derive_missions_with(&sector, &cfg)` | `MissionsReport` | `recompute_missions` | `MissionsConfig` `src/analysis/missions.rs:33` |
| Prose | `sectorforge::derive_prose_with(&sector, &cfg)` | `ProseReport` | `recompute_prose` | `ProseConfig` `src/analysis/prose.rs:27` |

None of these five has an `enabled` flag — they always derive when called. The
"gotcha" is that they are simply *not called* by `generate`. (Module-level
equivalents: `personae::derive_with`, `sites::derive_with`, etc.)

**Save format:** `GeneratedSector` → `sector.json` (`src/export/writers.rs:135`).
The five reports export as their own files via `export_sector` / the CLI subcommands.

---

## 3. What already exists

- **End-to-end generator** (`src/gen/generation/mod.rs`), re-exported as
  `generate_sector` / `generate_sector_with_progress` / `generate_with_progress_and_cancel`.
- **RNG** (`src/model/rng.rs:8-17`): blake3 stage-keyed `ChaCha8Rng`. The root seed is
  the only entropy source — same seed + same inputs ⇒ byte-identical output. Stages:
  `"placement"`, `"system"`, `"world"`, `"factions"`, `"routes"`, `"regions"`, `"sites"`.
- **Bundled data on disk** in `presets/` (not embedded in the binary):
  `presets/_base/data` has worlds (~196 rows / 1011 lines), factions (995 / ~9951
  lines), names, route_rules — plus regions/economy/relations files that ship
  **disabled**. `presets/embattled-frontier/data` adds populated history/personae/sites.
  **No preset has hooks/missions/prose files.**
- **Scaffolder** `presets::scaffold` (`src/loading/presets.rs:111`), `scaffold_to_dir`
  (`:291`), `default_presets_dir` (`:266`), `rewrite_seed` (`:197`).
- **Builder installs**: `BuilderState::new_blank` (`builder/src/builder/state/mod.rs:627`),
  `open_project` (`builder/src/builder/project_io.rs:403`), `new_project` (`:137`),
  `apply_preview` direct-assign (`builder/src/builder/state/generation_ops.rs:181`),
  CLI `builder --project <dir>` (`builder/src/main.rs:8-31`). The builder already runs
  `recompute_*` for the five post-gen features on load / on button.
- **CLI**: `Command` enum (`src/cli/mod.rs:40`); `Generate` (`:54`, needs `--project`),
  `New` (`:158`, scaffolds only). The post-gen features each have a subcommand
  (`src/cli/{personae,sites,hooks,missions,prose}.rs`) that does
  `generate_sector(input)` → `derive_*_with`. No size-only random command exists.

**Reference config** `presets/embattled-frontier/sectorforge.toml` is the closest
existing "most features on" config — it wires relations/regions/economy/history/
personae/sites and sets `[analyze] [search] [diff] [history] [map_theme]`. It does
**not** wire hooks/missions/prose (no files exist), and it still relies on its
overlay data files setting `enabled = true` (it does, unlike `_base`).

---

## 4. Strategies

Two ways to get "everything wired," not mutually exclusive:

**Strategy S — Synthesize a fresh project from scratch (primary).**
Build a complete `AppConfig` + overlay configs in memory, serialize them to a fresh
project dir (config + every data file, all enabled), reuse the bundled *content*
catalogs (worlds/factions/names — these are lore, not knobs), then load + generate +
derive. This is the literal "generate an entirely new sector .toml" the request asks
for, and it owns every field, so there are no defaults to trip over.

**Strategy P — A `_full` "everything" preset (durable, recommended companion).**
Check in one preset whose `sectorforge.toml` references all 14 input keys and whose
overlay data files are all `enabled = true`, including new hooks/missions/prose files.
The random generator can then scaffold `_full` and only roll the seed + size + a few
structural knobs. Easier to inspect/diff in git; guarantees "the preset has
EVERYTHING."

Recommended: implement **S** as the engine, and ship **P** as the data bundle S reuses
for content catalogs (so there is exactly one place that holds the full feature set).
§5 specs S; §6 specs the `_full` preset.

---

## 5. Strategy S — synthesize an entirely new, fully-random `sectorforge.toml`

### 5.1 Determinism model for "entirely random"

"Entirely random" must not mean "varies only by seed." We roll the **structural
config knobs** too (placement mode, density, worlds-per-system, region count, …) so
sectors differ in shape, not just contents. Determinism is preserved by deriving every
rolled value from the **minted root seed** via a dedicated RNG stage:

```
mint root_seed (the ONLY nondeterministic step; e.g. blake3(time_ns ++ os_random) → hex)
let cfg_rng = rng::stage_rng(&root_seed, "config", "");   // new stage key, no collision
roll all config knobs from cfg_rng
generation stages keep using their own stage keys ("placement", "system", …)
```

Re-running with the same `root_seed` reproduces the identical config *and* sector,
byte-for-byte. The minted seed is echoed in output + `manifest.seed` so any random
sector is reproducible afterward. This does not violate the determinism invariant:
the only RNG is the stage RNG; the mint is pre-generation entropy selection.

### 5.2 Sizing policy (the one user input)

`SectorSize` enum → grid dims; everything else is rolled. Anchored to `_base` (8×10 =
80 cells, 24 systems ⇒ density ≈ 0.30).

```
SectorSize   width × height   cells   system_count
Small         6 × 8            48      round(d·48)
Medium        8 × 10           80      round(d·80)   (~ _base)
Large        12 × 14          168      round(d·168)
Huge         16 × 20          320      round(d·320)
Custom{w,h}   w × h            w·h     round(d·w·h)
```

with rolled `d = density ∈ [0.25, 0.40]`, `system_count` clamped `>= 4` and
`<= cells`. The size enum is a convenience; a raw `width × height` is also accepted.

### 5.3 Rolled fields (no field omitted, none left to a default)

Every value below is written explicitly into the generated TOML, rolled from
`cfg_rng` unless marked fixed:

| Section / field | Value |
|---|---|
| `[generation] system_count` | `round(d·cells)`, clamp `[4, cells]` |
| `min_worlds_per_system` | roll `1..=2` |
| `max_worlds_per_system` | roll `4..=7` (≥ min) |
| `world_feature_count` | roll `3..=5` |
| `allow_empty_hexes` | `true` (keeps `system_count` valid) |
| `strict_world_rows` | `true` |
| `subsector_width/height` | factor of grid (e.g. ⌈w/4⌉, ⌈h/4⌉) |
| `[placement] mode` | roll {`uniform_grid`,`weighted_grid`,`clustered`} |
| `cluster_bias` | roll `0.3..=0.7` if clustered, else `0.0` |
| `minimum_system_distance` | `1` |
| `[world_selection] same_star_colour_bias` | roll `1.0..=1.4` |
| other world_selection bools | rolled |
| `[routes] enabled` | `true` |
| `max_route_distance` | roll `3..=6` |
| `route_density` | roll `0.15..=0.40` |
| `ensure_connected_graph` | `true` |
| `[generation.relations] min_world_presence` | `1` (or `2` for Huge, §5.6) |
| `[regions] enabled` | **`true`** + `count` ∝ grid (clamp ≤ ½ cells), `mean_size` roll `4..=8`, `apply_to_routes=true`, full `conditions` list |
| `[economy] enabled` | **`true`** + `feed_stability` rolled |
| `[relations] feed_conflict` | rolled |
| `[history] enabled` | **`true`** + epoch window + event caps rolled |
| `[map_theme] name` | roll from available theme names |
| `[outputs] formats` | `["json","markdown","bitmap","svg","html"]` (everything) |
| `[analyze] [search] [diff]` | sensible explicit values (these only affect reports/CLI, not the sector) |

### 5.4 The generated `sectorforge.toml` (complete shape)

Build `AppConfig` in code (it derives `Serialize`) and `toml::to_string_pretty` it, so
the document is guaranteed valid and exhaustive. Equivalent shape:

```toml
[project]
id = "random-<seed8>"
title = "Random Sector <seed8>"
description = "Procedurally generated random sector."
version = "0.1.0"

[inputs]                                   # every key wired
world_data_dir = "data/worlds"
system_names   = "data/names/system_names.toml"
world_names    = "data/names/world_names.toml"
factions       = "data/factions/factions.toml"
route_rules    = "data/routes/route_rules.toml"
relations      = "data/factions/relations.toml"
regions        = "data/routes/regions.toml"
economy        = "data/worlds/economy.toml"
history        = "data/history.toml"
personae       = "data/personae.toml"
sites          = "data/sites.toml"
hooks          = "data/hooks.toml"          # NEW file (see §6)
missions       = "data/missions.toml"       # NEW file
prose          = "data/prose.toml"          # NEW file

[generation]                               # §5.2/§5.3 — all fields explicit
seed = "<minted>"
sector_width = <W>
sector_height = <H>
subsector_width = <sw>
subsector_height = <sh>
system_count = <rolled>
min_worlds_per_system = <rolled>
max_worlds_per_system = <rolled>
allow_empty_hexes = true
world_feature_count = <rolled>
strict_world_rows = true
[generation.placement]      # mode/cluster_bias/min_dist
[generation.world_selection]
[generation.routes]
[generation.relations]

[history]                                  # enabled = true + caps
[analyze]
[search]
[diff]
[outputs]                                  # formats = all five
[outputs.bitmap]
[outputs.html]
[map_theme]
```

### 5.5 Overlay data files written alongside (all `enabled = true`)

The config above references files. The generator writes them so nothing is disabled:

- `data/routes/regions.toml` — `[regions] enabled = true`, rolled `count`/`mean_size`,
  `apply_to_routes = true`, and the full `[[regions.conditions]]` pool (warp_storm,
  turbulence, calm_corridor, blackout, anomaly). **Crucially not the `_base` copy,
  which sets `enabled = false`.**
- `data/worlds/economy.toml` — `[economy] enabled = true`, `feed_stability`, plus the
  production/resource tables (copy from `_base`/embattled-frontier content, since those
  are domain data, then force `enabled = true`).
- `data/factions/relations.toml` — `[relations]` with rolled `feed_conflict` (relations
  derive regardless; this only biases conflict).
- `data/history.toml` — eras + event_rules (copy the populated embattled-frontier file;
  `[history].enabled = true` lives in the main config too).
- `data/personae.toml`, `data/sites.toml` — knobs + optional `[[manual]]` seeds
  (copy embattled-frontier's populated files).
- `data/hooks.toml`, `data/missions.toml`, `data/prose.toml` — **new** files with
  explicit knobs (these have no existing copy anywhere — §6).
- Content catalogs reused verbatim from the bundle: `worlds.toml`, `factions.toml`,
  `system_names.toml`, `world_names.toml`, `route_rules.toml`. (We do not invent 196
  world rows / 995 factions from nothing — that data *is* the content; "random" means
  random composition, not invented lore.)

These overlay configs all derive `Serialize`, so the generator constructs e.g.
`RegionsConfig { enabled: true, count, mean_size, apply_to_routes: true, conditions }`
and `toml::to_string`s it — no string templating, no chance of a stale `enabled = false`.

### 5.6 Pipeline (Strategy S, end to end)

```rust
// crate `sectorforge`, new module e.g. src/gen/random_sector.rs
pub enum SectorSize { Small, Medium, Large, Huge, Custom { width: u32, height: u32 } }

pub struct RandomReport {                 // everything a "complete" sector needs
    pub sector: GeneratedSector,
    pub personae: PersonaeReport,
    pub sites: SitesReport,
    pub hooks: HooksReport,
    pub missions: MissionsReport,
    pub prose: ProseReport,
    pub input: ProjectInput,              // builder needs catalogs + config
    pub seed: String,                     // the minted seed, for reproducibility
}

pub fn generate_random_sector(
    size: SectorSize,
    seed: Option<String>,        // None => mint
    bundle_dir: &Utf8Path,       // the full content/data bundle (the _full preset)
    dest: &Utf8Path,             // fresh project dir (TempDir for headless)
) -> Result<RandomReport, SectorError>;
```

Steps:
1. `seed = seed.unwrap_or_else(mint_seed)`; `cfg_rng = stage_rng(&seed, "config", "")`.
2. Resolve `(W, H)` from `size`; roll all knobs (§5.3).
3. Materialize `dest`: copy content catalogs from `bundle_dir`; write the rolled
   `AppConfig` → `dest/sectorforge.toml`; write every overlay data file with
   `enabled = true` (§5.5).
4. `let input = load_project(dest)?;`  → `validate_project(&input)?`
   (`src/cli/generate.rs:107` shows the call shape).
5. `let sector = generate_sector(input.clone())?;` → `validate_sector(&sector)?`.
6. Post-gen derivations (the §2.2 five), using configs from `input.catalogs`:
   `derive_personae_with`, `derive_sites_with`, `derive_hooks_with`,
   `derive_missions_with`, `derive_prose_with`.
7. Return `RandomReport`.

Headless (CLI) uses a `tempfile::TempDir` for `dest` and writes outputs elsewhere; the
builder uses a real `dest` so the project is immediately savable + editable.

---

## 6. Strategy P — the `_full` "everything" preset

Per the request, a preset must wire **every** feature. Current coverage (from preset
inventory):

| Feature | _base | embattled-frontier | needed in `_full` |
|---|---|---|---|
| worlds / names / factions / route_rules | ✅ | ✅ | copy from `_base` |
| relations | file present, **disabled** | ✅ enabled | enabled |
| regions | file present, **`enabled=false`** | ✅ enabled | **set enabled=true** |
| economy | file present, **`enabled=false`** | ✅ enabled | **set enabled=true** |
| history | — | ✅ | copy (enabled) |
| personae | — | ✅ | copy |
| sites | — | ✅ | copy |
| hooks | — | — | **create** `hooks.toml` |
| missions | — | — | **create** `missions.toml` |
| prose | — | — | **create** `prose.toml` |

Build `presets/_full/` (hidden like `_base` — names starting `_` are skipped by
`presets::list`, `src/loading/presets.rs:69`, but `scaffold` accepts them by id):

1. `sectorforge.toml` referencing all 14 input keys (the §5.4 shape), `[regions]`/
   `[economy]`/`[history]` enabled, `formats` = all five.
2. `data/` = `_base` content catalogs + populated regions/economy/relations (with
   `enabled = true`) + history/personae/sites (from embattled-frontier) + new
   `hooks.toml`, `missions.toml`, `prose.toml`.

New files to author (no existing copy):

| File | Config struct | Top-level knobs |
|---|---|---|
| `data/hooks.toml` | `HooksConfig` (`src/analysis/hooks.rs:30`) | `max_per_anchor`, `top_n_digest`, `hide_hidden_hooks`, `[[manual]]` |
| `data/missions.toml` | `MissionsConfig` (`src/analysis/missions.rs:33`) | `max_per_anchor`, `top_n_digest`, `player_edition`, `[[manual]]` |
| `data/prose.toml` | `ProseConfig` (`src/analysis/prose.rs:27`) | `tone`, `include_overview`, `include_per_system`, `[prose.overrides]` |

`_full` serves double duty: it is the `bundle_dir` Strategy S copies content + overlay
data from (single source of truth for "everything"), and it is directly scaffoldable
(`sectorforge new --preset _full` once unhidden, or a builder button) for a fixed-data
random sector.

> A lighter alternative to authoring `_full` by hand: have Strategy S write the whole
> bundle itself on first run (it already constructs every overlay config). Then `_full`
> is optional. Recommended to still check in `_full` for git-visibility and so the CLI
> `new`/builder wizard can offer it.

---

## 7. Work breakdown

### 7.1 Core library (`sectorforge`) — ✅ DONE
- `src/gen/random_sector.rs` *(new)*: `SectorSize`, `RandomReport`, `mint_seed`, the
  knob-roll policy (§5.3), the `AppConfig`/overlay-config builders, and
  `generate_random_sector` (§5.6).
- ~~Generalize `presets::rewrite_seed` (`src/loading/presets.rs:197`) →
  `patch_generation_fields`~~ **→ Skipped:** the engine builds the whole
  `AppConfig` from structs and `toml::to_string_pretty`s it, so there is no
  template to patch. (The only line-patch left is `regions.toml`'s `count` /
  `mean_size`, scaled to the grid — see `patch_regions_toml`.)
- `src/gen/mod.rs`: `pub mod random_sector;`  ·  `src/lib.rs`: `pub use gen::random_sector;`
  (sequential — touches a re-exported surface).

### 7.2 CLI (`src/cli/`) — ✅ DONE
- `Command::Random { size | width/height, seed, out, formats }` (`src/cli/mod.rs:40`),
  runner `src/cli/random.rs`. Reuse `common.rs` helpers + `export_sector`
  (`src/cli/generate.rs:157`); also write the five post-gen reports like the existing
  `personae`/`sites`/… runners do.

```
sectorforge random --size medium [--seed S] [--out DIR]
sectorforge random --width 12 --height 14 [--seed S] [--out DIR]
```

### 7.3 Builder (`sectorforge-builder`) — ✅ DONE
- `ModalKind::GenerateRandom { size, custom_w, custom_h, seed }`
  (`builder/src/builder/state/types.rs:23`).
- `builder/src/builder/panels/generate_random.rs` *(new)*: size dropdown + optional
  custom dims + optional seed + destination picker. On confirm: call
  `generate_random_sector(...)` to `dest`, then **`open_project(dest)`** so the result
  arrives fully wired (sector + config + catalogs + project_path), and the builder's
  `recompute_*` populate the personae/sites/hooks/missions/prose panels. `*state =
  new_state` (same pattern as `new_project.rs:100`).
- Button next to "New project…" in `builder/src/builder/panels/project.rs:14`; wire
  modal dispatch in `app.rs`.
- **Undo:** whole-sector install is a "new document" — like `apply_preview` /
  `new_project`, it legitimately bypasses the command bus and clears the log (§R4
  carve-out). Do not add a whole-sector `BuilderCommand`.

### 7.4 Data (`presets/_full`) — ✅ DONE
- Author the preset tree per §6, including the three new feature files.

---

## 8. Determinism & invariants checklist
- ✅ Single entropy point: `mint_seed`. Everything else (config roll + generation)
  derives from it → reproducible, byte-identical on re-run with the same seed.
- ✅ Config roll uses a new `"config"` RNG stage (`rng::stage_rng`) — no collision with
  generation stages; no new RNG source introduced.
- ⚠ **Golden tests**: a pure-addition feature shouldn't touch writers, but if any
  render path changes run `cargo test --test it -- golden`.
- ⚠ **No `FxMap` iteration for output**: the roll/config code emits scalars; if it ever
  iterates catalogs, sort keys first.
- ✅ Builder whole-sector install documented as a command-bus bypass.

---

## 9. Testing
- **Sizing**: `SectorSize` → dims/`system_count` respects validation bounds
  (`src/validate/validation.rs:68,76,86,167,484`).
- **Completeness (the no-gotcha guard)** — for a fixed seed at each size assert:
  `sector.regions` non-empty, `sector.economy.enabled` && economy maps non-empty,
  `sector.chronicle` non-empty, `relations`/`influence_field`/`power_projection`
  populated, every system has worlds within `[min,max]`, route graph connected,
  **and** all five post-gen reports (`personae/sites/hooks/missions/prose`) non-empty.
- **Determinism**: same `(size, seed)` ⇒ byte-identical `sector.json` + identical
  generated `sectorforge.toml`. Add under `tests/it/`.
- **CLI**: `sectorforge random --size small --seed t --out <tmp>` exits 0, writes
  `sector.json` + all requested formats + the five reports.
- **Preset**: `_full` loads (`load_project`) and validates; every `[inputs]` file
  exists; regions/economy/history `enabled = true`.
- **Builder smoke** (`cargo test -p sectorforge-builder`): confirm path yields a
  `BuilderState` with non-empty systems, set `project_path`, and populated
  recompute_* caches.
- **Perf guard**: Huge with a fixed seed (gate `#[ignore]` if slow, like the segmentum
  test) — relations/economy scale with placed factions (§5.6).

---

## 10. Open questions

> **Resolved during implementation:**
> 1. `_full` is checked in **and** Strategy S reads its data tree (the
>    recommended both-of path).
> 2. Structural knobs **are rolled** (placement / density / worlds / regions /
>    routes / map theme). `--fixed-shape` is deferred to Phase 3.
> 3. The builder result is **project-backed** — written to the chosen folder
>    and immediately editable / savable.
> 4. **Huge** auto-raises `min_world_presence` to `2` to bound relations /
>    economy output; every other size uses `1`.
> 5. A self-contained `include_dir!` binary stays **out of scope** (Phase 3).
1. **Author `_full` by hand vs let Strategy S synthesize the whole bundle at runtime?**
   Recommended: check in `_full` (git-visible, single source of truth) *and* have S
   read content + overlay data from it.
2. **Roll structural knobs (placement/density/worlds), or fix them?** This doc rolls
   them (true "entirely random" shape). Could expose a `--fixed-shape` to pin them.
3. **Builder result project-backed (default) vs in-memory temp + forced Save-As?**
   Recommended project-backed (immediately editable/savable).
4. **Faction breadth on Huge**: auto-raise `min_world_presence` to bound relations/
   economy output?
5. **Self-contained binary?** Strategy S still reads content catalogs from disk. A
   later pass could `include_dir!` the `_full` bundle and build `ProjectInput` in
   memory (no temp dir). Out of scope here.

---

## 11. File-by-file change list

> **Status: ✅ every row below is implemented.** The one deviation: the §7.1
> `rewrite_seed` → `patch_generation_fields` generalisation was intentionally
> skipped (the engine builds the config from structs, so there is no template
> to patch). A small `patch_regions_toml` scales the regions overlay to the
> grid instead.

| File | Change |
|---|---|
| `src/gen/random_sector.rs` *(new)* | `SectorSize`, knob-roll, config+overlay builders, `generate_random_sector`, `mint_seed`, post-gen derivations |
| `src/gen/mod.rs` / `src/lib.rs` | module + re-export (sequential) |
| `src/loading/presets.rs:197` | optional: generalize `rewrite_seed` → `patch_generation_fields` |
| `src/cli/mod.rs:40` | `Command::Random` + dispatch (~`:419`) |
| `src/cli/random.rs` *(new)* | runner; reuse `common.rs` + `export_sector`; write the five reports |
| `builder/src/builder/state/types.rs:23` | `ModalKind::GenerateRandom` |
| `builder/src/builder/panels/generate_random.rs` *(new)* | form → core call → `open_project` |
| `builder/src/builder/panels/mod.rs` | `pub mod generate_random;` |
| `builder/src/builder/panels/project.rs:14` | "Random sector…" button |
| `builder/src/app.rs` | modal dispatch |
| `presets/_full/sectorforge.toml` *(new)* | all 14 inputs; regions/economy/history enabled; all formats |
| `presets/_full/data/**` *(new)* | content catalogs + enabled overlays + new `hooks.toml`, `missions.toml`, `prose.toml` |
| `tests/it/…` | completeness + determinism + CLI + preset tests |

Per CLAUDE.md routing: CLI → `cli-implementer`; builder panel → `panel-implementer`;
call-site lookups → `rust-explorer`; keep the `src/lib.rs` re-export change sequential.

---

## 12. Effort & phasing
- **Phase 1 — `_full` preset + Strategy S core + CLI — ✅ DONE:** authored
  `_full` (incl. the three new files), built `random_sector.rs` and
  `sectorforge random`. Fully testable headless.
- **Phase 2 — builder entry — ✅ DONE:** modal + panel + button reusing
  `open_project`.
- **Phase 3 — ➖ optional, not done:** embed `_full` for a self-contained
  binary; `--fixed-shape`; explicit density knob.

The generation, validation, derivations, data, and builder-install machinery all
exist. This feature is **config synthesis + a randomization policy + flipping every
overlay on + running the five post-gen derivations** — plus one new preset that holds
every feature. No new generation logic.
