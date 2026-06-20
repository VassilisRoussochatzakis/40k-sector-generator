# sectorforge

`sectorforge` is a deterministic Warhammer 40,000 star-sector generator. It reads a project directory of typed TOML configuration files and turns it into a complete, reproducible sector — stars and worlds, the factions contesting them, the warp lanes between systems, the warp-storm regions painted across the map, an inter-faction diplomacy matrix, a trade-and-tithe economy, and a dated in-universe chronicle — then exports that sector as JSON, Markdown, a PNG hex map, an SVG map, and a self-contained interactive HTML page. Generation is a **pure function of the project input**: the same seed and configuration produce byte-identical output on every run, on every machine. Every random draw is keyed through a stage-keyed `blake3`-seeded `ChaCha8Rng`, every output writer is byte-stable, and the guarantees are locked down by committed golden tests.

The repository is a Rust workspace with four crates: the `sectorforge` generation engine and CLI (library + binary), a full-construction egui editor (`sectorforge-builder`), an egui viewer with limited in-place editing (`sectorforge-viewer`), and a shared egui widget library (`sectorforge-gui-core`).

## Table of contents

- [Highlights](#highlights)
- [What's in a generated sector](#whats-in-a-generated-sector)
- [Prerequisites](#prerequisites)
- [Install & build](#install--build)
- [Quick start](#quick-start)
- [The three apps](#the-three-apps)
- [CLI reference](#cli-reference)
- [Project & configuration format](#project--configuration-format)
- [The generation pipeline](#the-generation-pipeline)
- [Outputs & exports](#outputs--exports)
- [Examples & presets](#examples--presets)
- [Determinism guarantees](#determinism-guarantees)
- [Workspace layout](#workspace-layout)
- [Development](#development)
- [Documentation](#documentation)

## Highlights

- **Deterministic by construction.** Identical seed + config produce byte-identical `sector.json`, exports, and renders on every run. Proven non-degenerate by a companion golden that asserts different seeds yield different output.
- **Stage-keyed RNG.** Every random draw flows through `src/model/rng.rs`, seeding a `ChaCha8Rng` from `blake3("sectorforge:{root_seed}:{stage}:{discriminator}")`, so each generation stage gets an independent, reproducible stream and entropy ordering is independent of where a run stops.
- **A full domain model**, not just a map: systems, worlds, factions (a three-level hierarchy), warp/webway routes, warp-phenomena regions, derived subsectors, presence/control, claims, a diplomacy matrix, an economy snapshot, and a chronicle — all serialized into one canonical `sector.json`.
- **Five export formats** from one engine: canonical JSON, a Markdown report, a PNG hex map, an SVG hex map, and a single-file interactive HTML page (inlined JSON + CSS + vanilla-JS renderer, no external assets).
- **A rich CLI**: validate, generate, generate a single system, generate every project in a tree, fully-random synthesis from nothing but a size, deterministic constraint-directed seed search, multi-sector segmentum composition, and a family of read-only derivations (analysis, history, personae, hooks, prose, relations, regions, economy, interestingness, briefing, missions, sites, diff).
- **Two desktop apps**: a full-construction builder with a command bus + undo/redo, and a viewer for inspecting, analyzing, and lightly editing an already-built sector.
- **Square-sector invariant** enforced everywhere (`sector_width == sector_height`), so the grid geometry can never diverge by hand-edit, by loaded sector, or by any UI path.
- **Golden-test-locked byte stability** for every writer (JSON, PNG, SVG, HTML, heatmap, system map, segmentum), with self-blessing `blake3` pins that fail the build on any drift.

## What's in a generated sector

A generated sector is a single `GeneratedSector` — a named, seeded N×N hex-grid region that owns every entity below.

- **Systems** — each star system (or notable warp object) occupies one hex. A system carries its hex coordinate, a kind (`Star` / `SpecialLocation` / `BlackHole` / `WarpAnomaly` / `SpaceStation`), an optional resolved star with a spectral colour, the worlds orbiting it, tags/notes, and aggregated system-level control, stability, conflict, and intel state.
- **Worlds** — individual planets within a system (hive world, forge world, death world, …). Each holds its orbit and a bundle of physical/social attributes (world type, atmosphere, temperature, biosphere, population, tech level, government, notable features), plus the factions present on it, political claims, and a multi-winner control summary.
- **Factions** — the powers contesting the sector (Imperium, Chaos, the xenos races, criminal/merchant/rebel groups). Modelled as a three-level hierarchy (faction → subfaction → force), each with a typed faction kind (~34 known kinds plus an `Unknown` escape hatch), a disposition, the systems/worlds it occupies, and an aggregated multi-dimensional power profile.
- **Routes (warp lanes & webway)** — the navigable links between systems: Warp travel lanes plus the hidden Aeldari Webway. Each route connects two systems with a distance, a route type (`StableWarpLane` / `ChartedPassage` / `SecretPassage` / `Webway` / `BlackShip` / `SmugglingLane`), a stability rating (`Stable` … `Perilous`), and per-faction route control. The last three types are hidden lanes visible only to their owning faction.
- **Regions (warp-phenomena overlay)** — large-scale warp/territory phenomena painted across the map (warp storms, blessed zones, dead zones, …). Each region is a named blob of hex cells with a condition that biases nearby route stability and world generation and tints the map. This overlay is built *before* routes so routes react to it.
- **Subsectors** — a *derived* spatial partition (not a hand-authored entity): the sector's systems are grouped into rectangular tiles of ~12 systems each (`DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR`), and each tile classifies its routes as internal vs. border, resolves a controlling faction plus a capital system/world, and carries a deterministic per-tile summary. Subsector cluster borders are drawn on the PNG and SVG maps (when the active map theme's `show_subsector_borders` is on), feed the `analyze` dashboard's per-subsector variety report, and seed subsector chronicle events.
- **Presence & control** — who actually holds sway, and how openly. A faction's footprint on a world is recorded via an influence tier (`Hidden` / `Minor` / `Significant` / `Dominant`), nine presence dimensions (admin, military, orbital, economic, industrial, ideological, covert, logistics, legitimacy), and a derived dominance bucket; world- and system-level control summaries roll these up into named winners (dominant, sovereign, occupier, economic hegemon, hidden master).
- **Claims** — the legal/ideological basis on which a faction asserts ownership of a world (Imperial mandate, treaty right, religious or dynastic right, military occupation, ancient xenos domain, rebellion, …). Each claim ties a faction to a claim type (11 kinds) and a 0–100 strength; claims drive the territorial border outlines on the map and feed the strongest-claimant "sovereign" winner.
- **Relations (diplomacy matrix)** — how every pair of factions regards one another. Each unordered pair gets a public/secret attitude, directional per-side views, treaty status, numeric trust/fear/rivalry/economic/military/covert dimensions, and a derived tension scalar from worlds where both co-occur.
- **Economy** — the trade, tithe, and strategic-resource flow that ties worlds together. Derived read-only from the finished sector: each world declares a production/consumption vector, and each route carries a derived trade volume (endpoint surplus/deficit gradient × distance falloff × stability × route-control interference), with supply/tithe risk classifiers.
- **Chronicle (timeline)** — the in-universe history that explains how the sector reached its present state: a dated, narrative-source list of events emitted in `M{epoch}.{ddd}` calendar notation by walking every world's claims, dominance, archetype, and conflict state. The same sector always yields the same chronicle.
- **Generation manifest** — the provenance record: project id, generator name/version, seed and seed hash, optional constraint-generation fields (base seed, candidate index, constraints digest), input/settings digests, and the system/world/route counts. This underpins the determinism guarantee.

Sectors also carry two further map overlays — a continuous Voronoi-style **influence field** (per-cell faction claim bands) and a **power projection** of faction strength over the route graph.

## Prerequisites

You need **Rust** installed via [rustup](https://rustup.rs/). The minimum supported Rust version (MSRV) is **1.87** (Rust edition 2021).

- **macOS:** Install Xcode Command Line Tools first (`xcode-select --install`), then run the rustup installer.
- **Linux (Ubuntu/Debian):** install the X11 dev packages the GUI crates link against:
  ```bash
  sudo apt install pkg-config libx11-dev libxcb1-dev libxi-dev libxinerama-dev libxcursor-dev libxrandr-dev
  ```
- **Windows:** run the rustup installer; it includes a Rust-compatible MSVC build toolchain.

Verify your toolchain:

```bash
rustc --version   # any 1.87+ works
cargo --version
```

See [GUIDE.md §0](GUIDE.md#0-prerequisites) for the same notes in the context of the full user guide.

## Install & build

Clone the repository and build all targets:

```bash
cargo build
```

This compiles the whole workspace and produces three binaries:

- `sectorforge` — the generation engine + CLI (root crate).
- `sectorforge-builder` — the full-construction egui editor.
- `sectorforge-viewer` — the egui viewer with limited in-place editing.

(The root crate also ships an optional `dhat-profile` heap-profiling binary, gated behind the `dhat-heap` feature — not built by default.)

## Quick start

Examples live as **on-disk project directories** under `examples/`, not embedded in the binary, so `--project` paths are resolved relative to your current working directory. Run the commands below from the repository root.

**Validate the reference project:**

```bash
cargo run --bin sectorforge -- validate --project examples/m42_project
```

**Generate the reference sector** (writes into `examples/m42_project/out/`). The reference project emits validation warnings, so pass `--allow-warnings` to let generation proceed:

```bash
cargo run --bin sectorforge -- generate --project examples/m42_project --allow-warnings
```

This produces, among other files, `examples/m42_project/out/sector.json` (the canonical sector), `sector.md`, `sector.png`, and `sector.html` (the format set configured in that project's `[outputs].formats`).

**Launch the builder** (starts with a blank scratch sector; pass `--project` to open one):

```bash
cargo run -p sectorforge-builder
cargo run -p sectorforge-builder -- --project examples/m42_project
```

**Launch the viewer** on the generated sector:

```bash
cargo run -p sectorforge-viewer -- examples/m42_project/out/sector.json
cargo run -p sectorforge-viewer -- --project examples/m42_project
```

## The three apps

### CLI — `sectorforge`

The headless engine. It validates projects, runs the generation pipeline, exports the configured formats, and runs every post-generation derivation (analysis, history, economy, briefings, diffs, segmentum composition, seed search, …). This is the deterministic core; both desktop apps build on the same library. Full surface in the [CLI reference](#cli-reference).

### Builder — `sectorforge-builder`

A native egui desktop editor for **full sector construction** — building a complete sector from scratch or from an opened project, with multiple sectors open at once as switchable workspace tabs. Every document mutation flows through a **command bus**, giving full undo/redo (`Ctrl+Z` / `Ctrl+Y`). Its **27 panels** are grouped into clustered navigation: **BUILD** (project, map, subsectors, regions, routes), **ENTITIES** (systems, worlds, factions, sites), **POWER** (control, economy, relations), **LORE** (history/chronicle, personae, hooks, missions, prose, briefing), **ANALYZE** (analytics, interestingness, search, diff), **OUTPUT** (segmentum, iterative-gen, export), and **CHECK** (validation, invariants). It also offers procedural generation (the random / iterative-gen / segmentum tabs in the OUTPUT cluster), an in-app data-catalog editor (e.g. the `worlds.toml` generation tables), a `Ctrl+K` command palette, debounced auto-save, and a file watcher with a conflict resolver for external changes.

Launch with `cargo run -p sectorforge-builder`. Optional `--project <DIR>` opens an existing project on startup; otherwise it starts with a blank 8×8 "Small" scratch sector. The window opens at 1400×900.

### Viewer — `sectorforge-viewer`

A viewer **first**, with **limited** in-place editing — it is *not* read-only. It loads a `sector.json` (or a project's `out/sector.json`) and renders an interactive hex-grid sector map plus a drill-down system view, alongside read-only analytics tabs (dashboard, factions, relations, trade, regions, history), a read-only route planner (Safest / Shortest / Strategic pathfinding over the existing route graph), and a read-only segmentum overview for composed multi-sector bundles.

What it **can** edit is confined to an already-built sector and its generation inputs:

- live map edits behind an **EDIT MAP** toggle on the sector/system views (add/remove systems, routes, planets);
- a structured **EDIT-MAP** tab with MAP / ROUTES / FACTIONS / SETTINGS sub-tabs and per-system / per-world inspectors;
- a typed, grid-based **worlds.toml data editor** (DATA-RAW tab) that saves back to disk;
- with full project context (`sectorforge.toml` + `data/`), **RE-GENERATE / RE-ROLL** and **constraint search (Wishes)** that re-invoke the existing generation engine.

It can open/scaffold-new/save/save-as projects and export PNG / SVG / HTML / JSON bundle / per-system PNGs.

**The builder-vs-viewer boundary:** the viewer hand-edits an already-built sector's data and can re-run the pipeline on a loaded project, but it has **no command bus and no undo/redo**. Free-form, stage-by-stage construction — world-pool curation, region/influence/chronicle authoring, presence/claim/roster/relations/economy overrides — lives only in the builder. In short: *view, analyze, and lightly edit an already-generated sector in the viewer; build a sector from scratch in the builder.*

Launch options (window opens at 1400×900):

```bash
cargo run -p sectorforge-viewer -- path/to/sector.json   # open a specific sector file
cargo run -p sectorforge-viewer -- --project <dir>        # auto-resolve <dir>/out/sector.json etc.
cargo run -p sectorforge-viewer -- --segmentum path/to/segmentum.json   # open a composed bundle
cargo run -p sectorforge-viewer                           # start with no sector loaded
```

`--project <dir>` resolves the first existing of `<dir>`, `<dir>/out/sector.json`, `<dir>/sector.json`; when the file sits in an `out/` dir it also sets the editor's project root, enabling the worlds.toml editor and the Generation/Wishes tabs.

## CLI reference

Invoke any subcommand as:

```bash
cargo run --bin sectorforge -- <subcommand> [flags]
```

(or `sectorforge <subcommand> …` once installed). The CLI itself has no global flags beyond clap's built-in `-h`/`--help` and `-V`/`--version`; the options below are per-subcommand conventions that recur across many of them.

### Shared flag conventions

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory containing `sectorforge.toml`. Required by `validate`/`generate`/`generate-system`/`regions`/`search`; an optional **input mode** (regenerate-on-the-fly) for the derivation subcommands. |
| `--sector <PATH>` | A previously generated `sector.json`. Required by `validate-sector`/`render-markdown`; an optional **input mode** (load instead of regenerate) for the derivation subcommands. |
| `--seed <SEED>` | Override the seed from `sectorforge.toml`. **A string**, not an integer. Present on `generate`, `generate-system`, `random`, `new`. |
| `--out <DIR\|PATH>` | Output location — a directory for report/bundle subcommands, a file path for `generate-system`/`render-markdown` (which default to stdout when omitted). |
| `--json` | Emit the report as JSON to stdout instead of human-readable text / Markdown. |
| `--strict` | Treat warnings/health failures as errors and exit non-zero (exact meaning varies per subcommand — see below). |
| `--player` | Hide GM-only material in the derived output (`hooks`, `missions`, `sites`). |
| `--presets-dir <DIR>` | Source directory holding presets (default `presets`). Present on `new`, `list-presets`, `random`. |
| `--formats <LIST>` | Comma-separated output formats overriding `[outputs].formats`. Tokens: `json`, `markdown` (alias `md`), `png` (alias `bitmap`), `svg`, `html`. Present on `generate`, `random`. |
| `--light` | Drop render-only artifacts (html/png/svg/markdown), keeping only the machine-readable JSON. Present on `generate`, `random`. |
| `--exclude <LIST>` | Comma-separated formats to exclude from the effective set (`json` cannot be excluded). Present on `generate`, `random`. |
| `--allow-warnings` | Continue generation when validation produced warnings but not errors. Present on `generate`, `generate-all`. |

Exit codes are mapped per failure class (e.g. validation failure = 1, I/O / export failure = 74, config parse / invalid config = 78, world-data load / no candidates = 65, generation cancelled = 130, other = 70).

### Logging & output streams

Every subcommand initializes `env_logger` at default level `info`, so progress is visible out of the box. Set `RUST_LOG` to control verbosity (`RUST_LOG=off` to silence it, `RUST_LOG=warn` for warnings only, `RUST_LOG=debug` for detail). Diagnostics and progress are written to **stderr**; **stdout** carries only machine-readable output (`--json` payloads, generated Markdown/JSON sent to stdout), so piping or redirecting stdout never captures progress noise:

```bash
RUST_LOG=off cargo run --bin sectorforge -- analyze --sector out/sector.json --json > analysis.json
```

### `validate`

Validate a project directory without generating any output; reports workbook stats, errors, and warnings.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to validate (required). |
| `--json` | Emit the validation report as JSON to stdout. |
| `--strict` | Treat warnings as errors (non-zero exit). |

```bash
cargo run --bin sectorforge -- validate --project examples/m42_project --strict
```

### `generate`

Generate a full sector (all configured exports) from a project directory.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to generate (required). |
| `--seed <SEED>` | Override the seed from `sectorforge.toml`. |
| `--out <DIR>` | Override the output directory. |
| `--allow-warnings` | Continue if validation produced warnings (not errors). |
| `--heatmap <MODE>` | PNG heatmap mode: `off`, `control`, `military`, `trade`, `industrial`, `covert`, `faith`, `threat`, `intel`, `tension`, `trade_volume`, `food`, `tithe`, `supply`. |
| `--no-faction-fill` | Disable the per-system planet-map faction tint (the sector hex grid itself is never faction-tinted). |
| `--theme <THEME>` | PNG map theme: `gm_dark`, `print_mono`, `imperial_archive`, `navis_tactical`, `inquisition_redacted`, `subsector_political`. |
| `--constraints <PATH>` | Constraint file the generated sector must satisfy. |
| `--max-candidates <N>` | Maximum number of candidate seeds to evaluate against the constraints. |
| `--formats <LIST>` | Comma-separated output formats; overrides `[outputs].formats`. |
| `--light` | Keep only the JSON artifact. |
| `--exclude <LIST>` | Comma-separated formats to exclude (`json` cannot be excluded). |

```bash
cargo run --bin sectorforge -- generate --project examples/m42_project --allow-warnings --theme gm_dark --heatmap threat
```

### `generate-all`

Generate every project (each immediate subdirectory containing a `sectorforge.toml`) under a directory. One failure is reported but does not abort the rest; the command exits non-zero if any project failed.

| Flag | Meaning |
|---|---|
| `--dir <DIR>` | Directory whose immediate subdirectories are sector projects (default `examples`). |
| `--allow-warnings` | Continue a project if its validation produced warnings (not errors). |

```bash
cargo run --bin sectorforge -- generate-all --dir examples --allow-warnings
```

### `random`

Synthesise a fully-randomised, fully-complete sector from nothing but a size: materialises a fresh project, generates it, runs the five post-gen derivations, and exports the bundle plus reports.

| Flag | Meaning |
|---|---|
| `--size <SIZE>` | Sector size: `small` \| `medium` \| `large` \| `vast` \| `massive` \| `huge` (default `medium`; ignored when `--width`/`--height` given). |
| `--width <N>` | Explicit square grid side length (must equal `--height` if both given). |
| `--height <N>` | Explicit square grid side length (must equal `--width` if both given). |
| `--seed <SEED>` | Reproducibility seed; omit to mint a fresh one (echoed on success). |
| `--out <DIR>` | Project directory to create; must not exist (default `./random-<seed>`). |
| `--presets-dir <DIR>` | Source directory holding presets, must contain `--baseline` (default `presets`). |
| `--baseline <NAME>` | Baseline preset whose themed data tree seeds content; layout is still rolled from the seed (default `_full`). |
| `--formats <LIST>` | Comma-separated formats to export (default all five: `json`, `markdown`, `png`, `svg`, `html`). |
| `--light` | Keep only the JSON artifact. |
| `--exclude <LIST>` | Comma-separated formats to exclude (`json` cannot be excluded). |

```bash
cargo run --bin sectorforge -- random --size large --seed dawnbreak --out ./random-dawnbreak
```

### `generate-system`

Generate a single standalone star system from a project directory, emitting its JSON.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to draw the system from (required). |
| `--seed <SEED>` | Override the seed from `sectorforge.toml`. |
| `--index <N>` | 1-based system index (default 1). |
| `--coord-q <Q>` | Axial hex coordinate q (default 0). |
| `--coord-r <R>` | Axial hex coordinate r (default 0). |
| `--out <PATH>` | Output path for the system JSON (defaults to stdout). |
| `--markdown` | Also write a Markdown snippet alongside the JSON. |

```bash
cargo run --bin sectorforge -- generate-system --project examples/m42_project --index 3 --out system3.json --markdown
```

### `validate-sector`

Load a previously generated sector JSON and check post-generation invariants.

| Flag | Meaning |
|---|---|
| `--sector <PATH>` | Path to the generated `sector.json` (required). |
| `--json` | Emit the report as JSON to stdout. |

```bash
cargo run --bin sectorforge -- validate-sector --sector examples/m42_project/out/sector.json --json
```

### `render-markdown`

Render a Markdown overview from a previously generated sector JSON.

| Flag | Meaning |
|---|---|
| `--sector <PATH>` | Path to the generated `sector.json` (required). |
| `--out <PATH>` | Output path for the Markdown (defaults to stdout). |

```bash
cargo run --bin sectorforge -- render-markdown --sector examples/m42_project/out/sector.json --out overview.md
```

### `inspect-worlds`

Print statistics for a standalone world-data directory containing `worlds.toml`.

| Flag | Meaning |
|---|---|
| `--data-dir <DIR>` | World-data directory containing `worlds.toml` (required). |

```bash
cargo run --bin sectorforge -- inspect-worlds --data-dir presets/_full/data/worlds
```

### `analyze`

Read-only analytics dashboard for a generated sector; writes `analysis.md` + `analysis.json` to `--out`, or prints Markdown to stdout.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to regenerate and analyze (alternative to `--sector`). |
| `--sector <PATH>` | Existing `sector.json` to analyze (alternative to `--project`). |
| `--out <DIR>` | Directory to write `analysis.md` + `analysis.json` (omit to print Markdown to stdout). |
| `--json` | Emit only the JSON to stdout (overrides Markdown). |
| `--strict` | Exit with status 1 if any health flag fires (useful in CI). |

```bash
cargo run --bin sectorforge -- analyze --sector examples/m42_project/out/sector.json --out analysis/ --strict
```

### `new`

Scaffold a new project from a bundled preset by copying `presets/<preset>/` into `<out>`; the destination must not already exist.

| Flag | Meaning |
|---|---|
| `--out <DIR>` | Destination project directory to create (required). |
| `--preset <NAME>` | Preset name, matching a sub-directory of `presets/` (required). |
| `--seed <SEED>` | Override the preset's bundled seed. |
| `--presets-dir <DIR>` | Source directory holding presets (default `presets`). |

```bash
cargo run --bin sectorforge -- new --out my-sector --preset mercantile-crossroads --seed myseed
```

### `list-presets`

List the available presets found in `--presets-dir`.

| Flag | Meaning |
|---|---|
| `--presets-dir <DIR>` | Source directory holding presets (default `presets`). |

```bash
cargo run --bin sectorforge -- list-presets --presets-dir presets
```

### `search`

Deterministic constraint-directed seed search: reads a `wishes.toml` and enumerates seeds derived from the base seed until one satisfies all constraints or the budget is exhausted; writes `search.md` + `search.json` to `--out` or prints Markdown.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to search over (required). |
| `--wishes <PATH>` | Path to `wishes.toml` listing the constraints (required). |
| `--base-seed <SEED>` | Override the base seed from wishes/project (the n=0 candidate). |
| `--budget <N>` | Override the search budget (number of candidate seeds). |
| `--out <DIR>` | Directory to write `search.md` + `search.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |
| `--strict` | Exit 1 if no candidate satisfied the constraints. |

```bash
cargo run --bin sectorforge -- search --project examples/m42_project --wishes wishes.toml --budget 500 --strict
```

### `history`

Derive a deterministic chronicle of in-universe events from a sector; writes `history.md` + `history.json` to `--out` or prints Markdown.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to regenerate (alternative to `--sector`). |
| `--sector <PATH>` | Existing `sector.json` (alternative to `--project`). |
| `--out <DIR>` | Directory to write `history.md` + `history.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |

```bash
cargo run --bin sectorforge -- history --sector examples/m42_project/out/sector.json --out history/
```

### `personae`

Derive a deterministic dramatis personae overlay (named characters per faction presence).

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to regenerate (alternative to `--sector`). |
| `--sector <PATH>` | Existing `sector.json` (alternative to `--project`). |
| `--out <DIR>` | Directory to write `personae.md` + `personae.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |

```bash
cargo run --bin sectorforge -- personae --sector examples/m42_project/out/sector.json --json
```

### `hooks`

Derive adventure / plot hooks from sector state.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to regenerate (alternative to `--sector`). |
| `--sector <PATH>` | Existing `sector.json` (alternative to `--project`). |
| `--out <DIR>` | Directory to write `hooks.md` + `hooks.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |
| `--player` | Hide GM-only hooks (e.g. those derived from hidden-tier presences). |

```bash
cargo run --bin sectorforge -- hooks --sector examples/m42_project/out/sector.json --player --out hooks/
```

### `prose`

Derive a narrative gazetteer of deterministic template prose.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to regenerate (alternative to `--sector`). |
| `--sector <PATH>` | Existing `sector.json` (alternative to `--project`). |
| `--out <DIR>` | Directory to write `prose.md` + `prose.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |
| `--dispatch` | Use the terse Administratum-dispatch tone instead of the florid gazetteer voice. |

```bash
cargo run --bin sectorforge -- prose --sector examples/m42_project/out/sector.json --dispatch --out prose/
```

### `relations`

Derive the inter-faction diplomacy matrix for a sector.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to regenerate (alternative to `--sector`). |
| `--sector <PATH>` | Existing `sector.json` (alternative to `--project`). |
| `--out <DIR>` | Directory to write `relations.md` + `relations.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |

```bash
cargo run --bin sectorforge -- relations --sector examples/m42_project/out/sector.json --out relations/
```

### `regions`

Emit the regional warp-phenomena overlay for a project's grid (requires a project so the regions config can be read).

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory whose regions config and grid are used (required). |
| `--out <DIR>` | Directory to write `regions.md` + `regions.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |

```bash
cargo run --bin sectorforge -- regions --project examples/m42_project --out regions/
```

### `economy`

Derive a trade, tithe, and strategic-resource snapshot for a sector. Aliases: `analyze-economy`, `analyze-tithes`.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to regenerate (alternative to `--sector`). |
| `--sector <PATH>` | Existing `sector.json` (alternative to `--project`). |
| `--out <DIR>` | Directory to write `economy.md` + `economy.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |

```bash
cargo run --bin sectorforge -- economy --sector examples/m42_project/out/sector.json --out economy/
```

### `compose`

Compose a multi-sector segmentum from `segmentum.toml`: generates every listed child sector then runs a deterministic stitch stage emitting inter-sector warp links and a super-manifest.

| Flag | Meaning |
|---|---|
| `--segmentum <PATH>` | Path to `segmentum.toml` (required). |
| `--out <DIR>` | Output directory for `segmentum.md`, `segmentum.json`, `super_manifest.json`, and per-child sector subdirectories (required). |
| `--stitch-seed <SEED>` | Override the stitch seed from the segmentum file. |
| `--json` | Emit JSON to stdout instead of writing files. |

```bash
cargo run --bin sectorforge -- compose --segmentum examples/segmentum_example.toml --out segmentum-out/
```

### `interestingness`

Score a sector against a target interestingness profile and emit a scorecard.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to regenerate (alternative to `--sector`). |
| `--sector <PATH>` | Existing `sector.json` (alternative to `--project`). |
| `--out <DIR>` | Directory to write `interestingness.md` + `interestingness.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |
| `--profile <ID>` | Target profile: `political_sandbox` \| `grim_collapse` \| `mercantile` \| `villainous` \| `frontier` (default `political_sandbox`). |

```bash
cargo run --bin sectorforge -- interestingness --sector examples/m42_project/out/sector.json --profile grim_collapse
```

### `briefing`

Apply an audience-targeted redaction profile and write a redacted sector plus summary into `--out`.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to regenerate (alternative to `--sector`). |
| `--sector <PATH>` | Existing `sector.json` (alternative to `--project`). |
| `--out <DIR>` | Output directory for the redacted sector + summary (required). |
| `--preset <NAME>` | Built-in briefing profile: `gm` \| `navy` \| `inquisition` \| `trader` \| `governor` \| `public` (required). |
| `--observer <FACTION>` | Observer faction id (defaults to none for `gm` / `public`). |
| `--min-confidence <0-100>` | Override the preset's intel confidence cutoff. |

```bash
cargo run --bin sectorforge -- briefing --sector examples/m42_project/out/sector.json --preset inquisition --observer ordo_xenos --out briefing/
```

### `missions`

Derive deterministic mission / quest seeds from sector state.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to regenerate (alternative to `--sector`). |
| `--sector <PATH>` | Existing `sector.json` (alternative to `--project`). |
| `--out <DIR>` | Directory to write `missions.md` + `missions.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |
| `--player` | Hide GM-only missions. |

```bash
cargo run --bin sectorforge -- missions --sector examples/m42_project/out/sector.json --player --out missions/
```

### `sites`

Derive planetary points-of-interest per world.

| Flag | Meaning |
|---|---|
| `--project <DIR>` | Project directory to regenerate (alternative to `--sector`). |
| `--sector <PATH>` | Existing `sector.json` (alternative to `--project`). |
| `--out <DIR>` | Directory to write `sites.md` + `sites.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |
| `--player` | Hide sites whose public status differs from their actual status. |

```bash
cargo run --bin sectorforge -- sites --sector examples/m42_project/out/sector.json --player --out sites/
```

### `diff`

Deterministic sector diff in two modes: compare two saved sectors (`--before`/`--after`), or generate a project and diff before vs. after advancing N ticks (`--project`/`--ticks`).

| Flag | Meaning |
|---|---|
| `--before <PATH>` | Earlier `sector.json` (pairs with `--after`). |
| `--after <PATH>` | Later `sector.json` (pairs with `--before`). |
| `--project <DIR>` | Project directory to generate and advance (pairs with `--ticks`). |
| `--ticks <N>` | Number of ticks to advance the generated sector before diffing. |
| `--out <DIR>` | Directory to write `diff.md` + `diff.json`. |
| `--json` | Print JSON to stdout instead of Markdown. |
| `--skip-worlds` | Skip world-level detail in the report. |
| `--skip-routes` | Skip per-route detail in the report. |

```bash
cargo run --bin sectorforge -- diff --before out/sector.json --after out2/sector.json --out diff/
```

## Project & configuration format

A project is a directory holding a `sectorforge.toml` manifest plus the data TOML files it references (and, after generation, an `out/` directory of generated artifacts — *not* part of the input format). The reference layout is `examples/m42_project/`.

### `sectorforge.toml` (required)

The project manifest at the project root — the only file read by name. It is deserialized with `#[serde(deny_unknown_fields)]`, so **unknown keys are a hard error**. Its real top-level tables are:

| Table | Purpose |
|---|---|
| `[project]` | Identity: `id`, `title`, `description`, `version`. |
| `[inputs]` | Relative paths to the data TOMLs below. **`world_data_dir` is the only required input**; every other entry is optional and defaults when omitted. |
| `[generation]` | Core knobs: `seed`, `sector_width`, `sector_height` (must be **equal** — square-sector invariant), `system_count`, `min_worlds_per_system`, `max_worlds_per_system`, `allow_empty_hexes`, `world_feature_count`, plus subtables `[generation.placement]`, `[generation.world_selection]`, `[generation.routes]`, `[generation.relations]`. |
| `[analyze]` | Health-flag thresholds for the `analyze` dashboard. |
| `[search]` | Defaults for the seed-`search` budget / reporting. |
| `[diff]` | Defaults for the sector `diff`. |
| `[history]` | Fallback chronicle eras / event knobs, used only when `inputs.history` is unset. |
| `[outputs]` | `directory`, `formats` list, `pretty_json`, `write_per_system_files`, `write_manifest`, `write_diagnostics`, plus subtables `[outputs.bitmap]` and `[outputs.html]`. |
| `[map_theme]` | Optional top-level theme alias folded into `outputs.bitmap.theme` by the loader. |

### Data files

All of these live under the paths named in `[inputs]`. Each parses into a typed config and defaults sensibly when its `[inputs]` key is absent.

| File | `[inputs]` key | Required? | Contents |
|---|---|---|---|
| `worlds.toml` | `world_data_dir` (the directory) | **Yes** | The authoritative world-generation pool: an array-of-tables `[[generation]]` of weighted world templates (`star_colour`, `world_type`, `atmosphere`, `temperature`, `biosphere`, `population`, `tech_level`, `government`, `notable_feature`, `weight`), plus an optional structured `[features]` pool. Located by the constant filename `worlds.toml` inside the data dir. |
| `economy.toml` | `economy` | No | Trade & resource production/consumption tables, `[economy]` with per-world-type sub-tables `[economy.by_world_type.<WorldType>]`. |
| `factions.toml` | `factions` | No | The faction catalogue: an array-of-tables `[[factions]]`. |
| `relations.toml` | `relations` | No | Inter-faction diplomacy rules: `[relations]` with `[[relations.kind_rules]]` / `[[relations.disposition_rules]]` plus public/secret attitude overrides. |
| `route_rules.toml` | `route_rules` | No | Warp-route generation rules: `[routes]` with `[[routes.modifiers]]`. |
| `regions.toml` | `regions` | No | Regional warp-phenomena overlay catalogue + stage params: `[regions]` with `[[regions.conditions]]`. |
| `history.toml` | `history` | No | Generated-sector chronicle: `[history]` with `[[history.eras]]` and `[[history.event_rules]]`. Falls back to the inline `[history]` table in `sectorforge.toml` when unset. |
| `personae.toml` | `personae` | No | Dramatis personae: `[kinds.<group>]` pool tables plus `[[manual]]` hand-authored entries. |
| `sites.toml` | `sites` | No | Planetary sites / points-of-interest: `[[manual]]` hand-authored entries. |
| `system_names.toml` | `system_names` | No | Name-generation table for star systems (`[system_names]`: prefixes / suffixes / single_names). |
| `world_names.toml` | `world_names` | No | Name-generation tables for worlds/locations (`[location_names]`, `[world_names]`). |

The loader also recognises optional builder-only inputs `hooks` (`hooks.toml`), `missions` (`missions.toml`), and `prose` (`prose.toml`), each defaulting to its own config when absent; these are not present in `examples/m42_project`.

## The generation pipeline

`generate` runs a fixed, ordered sequence of stages. Each stage seeds a fresh `ChaCha8Rng` from its own stage key, so stages share no RNG stream and the ordering of entropy is independent of where a run stops. The stages, in order:

1. **World pool** — filter the loaded world-data rows into the in-memory candidate pool; apply authored-feature overrides. *(no RNG)*
2. **Placement** — Fisher–Yates hex placement of systems on the square grid plus a minimum-distance relaxation, producing the ordered list of hex coordinates.
3. **Regions (warp regions)** — build warp/storm regions and collect the anomaly hexes. Runs **before** worlds on purpose, so anomaly hexes can bias each system's candidate pool toward warp-phenomena / ancient-ruins.
4. **Systems (incl. worlds + naming)** — per placement, build each system and its worlds (planet types, populations, …), apply anomaly bias on anomaly hexes, and assign unique system/world names from the name catalog. Worlds and naming happen here — they are not separate top-level stages.
5. **Factions** — assign factions to worlds across all systems, then aggregate per-system presence into the sector-level faction list.
6. **Routes (public)** — select the public warp-lane network between systems (when routes are enabled), with initial stability/hazard classification. *(no RNG)*
7. **Region route effects** — apply warp-region effects to public routes (storm → Perilous, turbulence → one tier worse, calm corridor → one tier better up to the Perilous ceiling), preserving bridges. Idempotent. *(no RNG)*
8. **Hidden routes** — append hidden route layers (webway / black-ship / smuggling) onto the route list before control derivation. *(no RNG)*
9. **Stability rebalance** — when stability targets are configured, re-bucket public route stabilities to hit the target mix, guaranteeing a `Stable` backbone. *(no RNG)*
10. **Route controls** — derive per-route, per-faction control records from final route geometry and faction endpoint presence. *(no RNG)*
11. **System state** — per-system/world derived overlays: surface regions per world, world/system conflict state, orbital assets + blockade detection, and per-system fog-of-war intel. *(no RNG)*
12. **Manifest + sector assembly** — sort systems/routes/factions by id for stable serialization, build the generation manifest (counts + input/setting digests), and construct the sector. Pure and RNG-free, so it always runs (even on an early cutoff) to yield a valid renderable sector.
13. **Archetypes** — apply faction-kind archetype rules across the assembled sector. *(no RNG)*
14. **Power projection** — project faction power over the route graph (decays + doctrine) into the sector's power-projection field and fold the result back onto faction records. *(no RNG)*
15. **Influence field** — build the continuous Voronoi-style influence field (per-cell faction claim bands) over the sector. *(no RNG)*
16. **Relations** — derive the inter-faction relationship matrix once factions are finalized, filtered by `min_world_presence` before the pairwise loop.
17. **Economy** — derive the per-world/per-system economy snapshot, reading final route stability + control; optionally apply a stability nudge back onto the sector when `feed_stability` is set. *(no RNG)*
18. **Chronicle** — the final stage: derive the timeline of events referencing routes, regions, subsectors, claims, control, and present conflicts.

Post-generation invariant checking (`validate_sector`) and export run *after* generation returns, not inside the pipeline.

## Outputs & exports

Generation writes into a single output directory — `<project>/out/` by default, overridable with `generate --out <dir>`. The viewer resolves a sector by looking for `<project>/out/sector.json`.

### Generate-time formats

Selected by `[outputs].formats` in `sectorforge.toml` (token aliases: `md` = `markdown`, `bitmap` = `png`), overridable with `--formats`, reducible with `--light` (drops html/png/svg/markdown, keeps json) and `--exclude` (json cannot be excluded).

| Format | File | Token | Notes |
|---|---|---|---|
| **JSON (canonical sector)** | `out/sector.json` | `json` | The complete machine-readable sector — every system, world, faction, route, region, and overlay. The load-bearing artifact the viewer reads; **cannot be excluded**. Pretty vs. compact via `outputs.pretty_json`. |
| **Markdown report** | `out/sector.md` | `markdown` (`md`) | Human-readable sector report. Dropped by `--light`. |
| **PNG hex map** | `out/sector.png` | `png` (`bitmap`) | Raster hex map with legend — pointy-top odd-r grid, routes, systems, region overlay, and subsector cluster borders (when the theme's `show_subsector_borders` is on), with an embedded 5×7 bitmap font (no external font files). Resolution via `outputs.bitmap.sector_scale` (1–5). Modulated by `--heatmap`, `--no-faction-fill`, `--theme`. Dropped by `--light`. |
| **SVG hex map** | `out/sector.svg` | `svg` | Vector hex map — crisp at any zoom, with the same layout and subsector borders as the PNG. It shares the PNG's `RenderOptions` type, but the **CLI** SVG path always renders with the default theme: on the CLI the `--theme` / `--heatmap` / `--no-faction-fill` modulators take effect on the PNG only (the viewer's SVG export *does* honor them). Dropped by `--light`. |
| **Interactive HTML** | `out/sector.html` | `html` | One self-contained file: inlined sector JSON + inlined CSS + an inlined vanilla-JS hex renderer (pan/zoom, click-to-inspect, faction filter, heatmap toggle). No external assets — deterministic bytes. Supports a redacted player edition. Gating is by the `formats` list: dropped by `--light` or `--exclude html`. (The `[outputs.html]` table only carries theme + redaction options — it has no enable/disable toggle.) |

### Always-on / separately-gated artifacts

Not format tokens — gated by their own `outputs.*` booleans:

- **`out/manifest.json`** — the seed/digest/input audit chain, emitted whenever `outputs.write_manifest` is on (default true).
- **`out/validation_report.json`** — a small JSON stub recording that pre-generation validation passed, emitted alongside `manifest.json`.
- **`out/systems/<system_id>.json`** — one per system, written only when the `json` format is active *and* `outputs.write_per_system_files` is enabled.
- **`out/systems/<system_id>.png`** — one per system (a star with concentric orbit rings), written whenever PNG is enabled *and* `outputs.bitmap.render_systems` is on (default true). Scale via `outputs.bitmap.system_scale`.

### Derivation & composition outputs

These are *not* part of the `generate` format set — each is produced by running its subcommand, and lands in that command's own `--out` directory (or stdout):

- The read-only analysis subcommands each write a paired `<name>.md` + `<name>.json` (base names: `analysis`, `search`, `history`, `personae`, `hooks`, `prose`, `relations`, `regions`, `economy`, `interestingness`, `missions`, `sites`, `diff`), or print Markdown / `--json` JSON to stdout when `--out` is omitted.
- **`compose`** writes `segmentum.md`, `segmentum.json`, and `super_manifest.json` into `--out`, plus per-child sector subdirectories under `children/<child_id>/`.

## Examples & presets

Verified on disk. **Examples** (`examples/`) are ready-to-run project directories; **presets** (`presets/`) are templates the CLI scaffolds from (`new`) and `random` draws its themed data tree from. Run `validate`/`generate` against examples; scaffold a new project from a preset.

### Examples (`examples/`)

| Path | Description |
|---|---|
| `examples/m42_project` | The canonical reference sector: 10×10, 24 systems, `route_density 0.30`. The only project that exercises every input overlay (factions, relations, regions, economy, history, personae, sites) and all four configured output formats, and the only one carrying `analyze`/`search`/`diff`/`history`/`map_theme` config blocks. |
| `examples/big_test` | Large-scale stress test: 32×32 grid, 200 systems, `route_density 0.12`. Minimal outputs (json + markdown only) for scale/performance testing; `gm_dark` bitmap theme. |
| `examples/big_sparse_test` | Sparse frontier-scale variant of `big_test`: 32×32, 80 systems, `route_density 0.048`. Minimal json + markdown outputs. |
| `examples/segmentum_example.toml` | A standalone segmentum-composition manifest (not a project dir) for `compose`: stitches a 2×2 grid of four child sectors (alpha/beta/gamma/delta), each pointing at `m42_project`, with shared factions and deterministic stitch links keyed off `stitch_seed`. |

### Presets (`presets/`)

| Path | Description |
|---|---|
| `presets/m42-classic` | Balanced canonical Imperium sector — the default M42 setup, good for first-time users. 10×10, 24 systems, `route_density 0.30`. Fully fleshed (all overlays + SVG + interactive HTML). Inherits `_base`. |
| `presets/dead-sector` | Low-population, ruins-heavy void: sparse, high-hazard 10×10 with just 3 systems (1–3 worlds each), `route_density 0.12`. Necron/dead-world theme. (The `system_count = 3` is intentional — a near-empty void.) Inherits `_base`. |
| `presets/embattled-frontier` | 20×20 contested marchworld warzone: Imperium vs. Orks open war with a small Leagues of Votann presence. 56 systems, `route_density 0.20`, sparse high-hazard. Inherits `_base`. |
| `presets/mercantile-crossroads` | Dense, route-rich trade hub: 10×10, 30 systems (3–7 worlds each), very high `route_density 0.85` — Rogue Trader dynasties, chartist captains, xenos traders. Inherits `_base`. |

Two further preset directories are **internal** and not intended for direct use: `presets/_full` (the reference preset with every overlay and output enabled — the source of truth that `sectorforge random` copies) and `presets/_base` (the base bundle the user-facing presets inherit via `inherits = "_base"`).

## Determinism guarantees

Determinism is the central design constraint, not an afterthought.

- **Same inputs → same output.** Identical seed + config produce byte-identical `sector.json`, exports, and renders on every run. Locked by the `generate_same_seed_same_output` golden and proven non-degenerate by `generate_different_seed_different_output`.
- **Stage-keyed `blake3` RNG.** Every random draw uses a `ChaCha8Rng` seeded from `blake3("sectorforge:{root_seed}:{stage}:{discriminator}")` (`src/model/rng.rs`), giving each stage an independent, reproducible stream. `rand::thread_rng()` and ad-hoc seeding are forbidden. ChaCha8 is a portable, version-stable PRNG, and its output is pinned by a cross-version golden.
- **Byte-stable output writers.** Every export writer (`sector.json`, PNG, SVG, HTML, heatmap, system map, segmentum) renders byte-identical output from a fixed-seed sector, locked by committed `blake3` pins / full-file goldens that fail the build on any drift.
- **Deterministic key ordering.** All emitted collections are walked in a stable order — `BTreeMap` / `BTreeSet`, explicitly sorted keys, or fixed grid order — never by iterating an `FxHashMap` / `FxHashSet` (the Fx aliases are internal-lookup-only), so map ordering can never perturb output bytes.
- **Square-sector invariant.** `sector_width` must equal `sector_height` everywhere, enforced pre-generation by the `GEN_SECTOR_NOT_SQUARE` validation rule (catching hand-edited TOML, loaded sectors, and any UI path); every checked-in preset/example is N×N.
- **Golden tests.** Byte-stability is enforced by self-blessing `blake3` pins (`assert_blake3_golden`) and full-file goldens under `tests/goldens/`, which panic on mismatch. Run them with `cargo test --test it -- golden`; re-pin after an intentional render/export change via the per-writer env vars (e.g. `UPDATE_GOLDEN_PNG=1`, `UPDATE_GOLDEN_HTML=1`, `UPDATE_GOLDEN_JSON=1`, `UPDATE_GOLDEN_HEATMAP=1`). Live gui-core `sector_view` map renders are a separate snapshot suite refreshed with `UPDATE_MAP_SNAPSHOTS=1`.

## Workspace layout

All four crates share one version (0.1.0), edition (2021), and MSRV (1.87) via `[workspace.package]`.

| Crate | Path | Purpose |
|---|---|---|
| `sectorforge` (lib + bin) | `src/` | Domain model, sector generation engine, analysis, exports (PNG/SVG/HTML), and the `sectorforge` CLI binary. Also ships an optional `dhat-profile` heap-profiling bin behind the `dhat-heap` feature. |
| `sectorforge-builder` | `builder/` | Egui desktop editor (eframe) for full sector construction with write access. Depends on `sectorforge` + `sectorforge-gui-core`. |
| `sectorforge-viewer` | `viewer/` | Egui viewer (eframe) with limited in-place editing (map/faction/world edits, `worlds.toml` data editor, save/save-as). Depends on `sectorforge` + `sectorforge-gui-core`. |
| `sectorforge-gui-core` | `gui-core/` | Shared egui widget library (`SectorView`, palette, `info_panel`, and the rest of the GUI chrome) used by both the builder and viewer. Owns the raw paint primitives; `#![forbid(unsafe_code)]`. |

The builder and viewer both enable the `sectorforge-gui-core/bundled-fonts` feature by default, so the shipping apps build with bundled fonts on (the gui-core feature itself is opt-in/off by default). Integration tests live as a single-binary suite under `tests/it/`.

## Development

There is no task-runner (no `justfile` / `Makefile`); these are the canonical cargo invocations:

```bash
cargo build                                  # build all targets
cargo test --workspace                       # run the full test suite
cargo test --test it -- golden               # byte-stable golden output tests (slower)
cargo fmt --all                              # format all crates (tree is kept rustfmt-clean)
cargo check --workspace --all-targets        # fast type-check, no binaries
cargo clippy --workspace --all-targets -- -D warnings   # lint, warnings as errors
```

**Test layout.** Integration tests are a single binary at `tests/it.rs` that compiles the per-feature modules under `tests/it/` (so they are modules of one binary — hence `--test it`). Golden output tests (deterministic byte-stable PNG/SVG/HTML/JSON snapshots) run via the `golden` filter and gate all rendering changes. Property-based tests (proptest) cover invariants and route monotonicity, among others. A slow segmentum composition test is gated behind `#[ignore]`:

```bash
cargo test --test it segmentum -- --ignored
```

Several criterion benchmarks (`generation`, `briefing`, `seed_search`, `influence_field`, `render_png`) are defined as `[[bench]]` targets and are separate from the test suite.

**Fuzzing.** The `fuzz/` crate (cargo-fuzz + `libfuzzer-sys`) fuzzes the attacker-controlled parser surfaces. It is deliberately kept *outside* the workspace because `libfuzzer-sys` requires a nightly toolchain, so `--workspace` builds stay on stable. Six targets cover the config/data parsers: `config_parse`, `worlds_toml_parse`, `factions_toml_parse`, `sector_json_parse`, `presets_load`, `map_theme_parse_color`.

```bash
cargo install cargo-fuzz                      # one-time
cd fuzz && cargo +nightly fuzz run config_parse
```

## Documentation

- **[GUIDE.md](GUIDE.md)** — the full user guide (prerequisites, quick start, config reference, workflows).
- **[BUILDER.md](BUILDER.md)** — a procedural, step-by-step walkthrough of the builder UI, from launching the app to a small saved sector.
- **[docs/MAP.md](docs/MAP.md)** — the detailed file-by-file map of the codebase.
- **[CLAUDE.md](CLAUDE.md)** — repository working conventions, determinism/geometry invariants, and the subagent routing guide.
