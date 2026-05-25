# OVERVIEW

`sectorforge` is a deterministic procedural generator for star sectors in the style of Warhammer 40,000. It does not run a game — it builds the *setting* a game, campaign, or piece of fiction is set against. You feed it a project directory of data files (world catalogues, name lists, faction definitions, route rules) plus a single configuration file, and it produces a fully populated sector: dozens of star systems sprawled across a hex grid, each with its own worlds, factions, governments, populations, warp routes, and political situation. Re-running with the same seed gives you back the exact same sector, byte for byte; changing the seed gives you a brand-new sector with the same data foundations.

This document is a qualitative tour of what the application *does* and *produces*, written for someone evaluating whether the tool fits their use case. It does not describe how the code is organised, how to build it, or how to call its APIs — those questions are answered in `GUIDE.md`.

---

## 1. What the application is for

The headline use case is **building reusable, deterministic Warhammer 40k sector settings**. A sector is the canonical "mid-scale" unit of the 40k galaxy: bigger than a single planet, smaller than a segmentum. Most published 40k campaigns, novels, and games take place inside one. Sectorforge is built to spit them out at will and in arbitrary numbers, so a GM, writer, or game designer can:

- Generate an original sector for a new campaign in seconds.
- Re-roll until the political situation feels interesting, then lock the seed and never lose that sector.
- Tweak a single faction's preferred world types, regenerate, and see how the balance of power shifts across the map.
- Author and edit individual worlds, factions, and routes after generation through a graphical editor.
- Export the result as machine-readable JSON, human-readable Markdown briefing documents, or printable PNG bitmap maps — pick the format that fits your downstream workflow.

It is intentionally setting-agnostic at the data layer: the world taxonomy and faction model are designed for 40k, but the catalogues are typed TOML files you fully control, so it can be reskinned to other space-opera universes by replacing the data.

The app ships as three front ends — a command-line tool (`sectorforge`) for scripted, headless, batch workflows, a desktop viewer/editor (`sectorforge-viewer`) for exploring generated sectors, and a desktop builder (`sectorforge-builder`) for constructing projects interactively. All consume the same project directories and emit the same outputs, so you can flip between them in the same project.

---

## 2. The shape of a generated sector

A generated sector is more than a list of planets. It is a layered model with five major strata, each derived deterministically from the seed and the input data.

### 2.1 The hex grid

The sector lives on a rectangular axial hex grid whose width and height you choose. Within that grid, the generator places a configurable number of star **systems**, leaving the rest as empty void hexes. Placement is itself configurable: a uniform spread, a weighted scatter, or a "clustered" mode that pulls systems toward the centre, simulating a more populated sector core surrounded by frontier emptiness. A minimum-distance constraint prevents systems from being placed on top of each other.

### 2.2 Stars and worlds

Every system gets a star, with a colour drawn from the project's catalogue. Each system then receives a randomised count of worlds (between configurable minimum and maximum). Every world is drawn from a weighted candidate pool authored in `worlds.toml` — meaning the *probability distribution* of world types in your sector is entirely under your control. The world taxonomy is rich and matches what 40k readers expect:

- **World types** — hive worlds, forge worlds, agri worlds, death worlds, shrine worlds, feral worlds, civilised worlds, mining worlds, paradise worlds, fortress worlds, knight worlds, and many more.
- **Atmosphere, temperature, biosphere** — independent axes capturing whether a world is breathable, lethal, frozen, blistering, lush, irradiated, etc.
- **Population scale and tech level** — from teeming hive-billions down to feral tribes, from medieval to advanced.
- **Government** — military governor, magistrate council, theocracy, oligarchy, dynastic, planetary lord, etc.
- **Notable features** — flavour modifiers like trade hub, administrative capital, warp phenomena, police state, cult activity, daemonic incursion, xenos infestation, recidivist underbelly, ancient ruins, and so on. Each world carries several.

The generator also enforces consistency rules you set, like "all worlds in a system share the primary star's colour" or "no two worlds in the same system share a world type."

### 2.3 Names

Both systems and worlds are named from project-supplied pools, with two construction modes (single curated names, or prefix+suffix composition) and a roman-numeral fallback that produces forms like *"Hadrumetum III"*. If you supply both styles, the generator coin-flips between them so the sector reads as varied rather than formulaic.

### 2.4 Factions

Faction placement is the political layer of the generator and is where the design invests heavily. Factions are defined per-project in a TOML file, each with a `kind` (imperial, chaos, xenos, criminal, mechanicus, ecclesiarchy, rogue trader, cult, etc.), a `disposition` (lawful, zealous, hostile, opportunistic…), a base weight, and a set of *preferences* — preferred world types, preferred governments, preferred notable features. Each preference category multiplies the faction's odds of taking a foothold on worlds that match it.

Faction assignment is *multi-presence* and *multi-winner*. A world is not "owned" by one faction. Up to three factions can be present on each world simultaneously, each with their own **influence tier** (Dominant, Significant, Minor, Hidden) and their own **dominance state** (Rumored, Presence, Influence, Contested, Controlled, Stronghold). This means the same world can be, e.g., legally administered by the Imperium, economically dominated by a Rogue Trader dynasty, and quietly infiltrated by a Genestealer cult — all three appearing in the world's faction list, each with their own confidence rating.

On top of presence, the generator derives a ten-dimensional **power profile** for every faction on every world:

- **Administrative** — the bureaucratic, civilian-governance footprint.
- **Military** — ground forces, garrisons, militias.
- **Orbital** — control over the space around the world (defence platforms, picket ships).
- **Naval** — fleet projection between systems.
- **Economic** — trade, taxation, commercial capture.
- **Industrial** — manufacturing, forge output, productive capacity (treated as a first-class axis distinct from economic).
- **Ideological** — faith, doctrine, propaganda reach.
- **Covert** — espionage, infiltration, hidden cell networks.
- **Logistics** — supply lines, transport capacity.
- **Legitimacy** — the public-facing claim to rule.

Each dimension is a 0–100 score derived from world traits, faction kind, and the faction's stated dispositions, with no extra RNG draws — so the political picture is fully reproducible from the seed.

### 2.5 Control and claims

From the per-presence dimensions, the generator computes **multi-winner control summaries** at every level of the hierarchy:

- **Per world** — a `dominant` faction (overall winner), plus separate winners for `sovereign` (the legitimate ruler), `occupier` (boots on the ground), `economic_hegemon` (who runs the markets), `popular_authority` (who the population actually listens to), and `hidden_master` (who runs things from the shadows). Worlds can be flagged `contested` when the top two are close.
- **Per system** — an aggregated political state: Pacified, Fragmented, Blockaded, Warzone, Infiltrated, Quarantined, or Uncharted, plus its dominant faction, sovereign, orbital controller, economic hegemon, and hidden master, and a top-N ranking of all factions present.
- **Per faction** — a sector-wide power profile aggregated from all of that faction's presences, with a single weighted total projection score for fast comparisons.

Parallel to the control summary, every world carries a list of typed **claims** — Legal Sovereignty, Imperial Mandate, Religious Mandate, Dynastic Right, Commercial Charter, Military Occupation, Ancient Domain, Hunting Ground, Covert Writ, Rebellion, Treaty Right. These let you express situations like *"the Imperium claims this world by Imperial Mandate, but Chaos has been holding it by Military Occupation for forty years, and a noble house is still asserting an Ancient Domain"* — three competing claims on the same dirt.

### 2.6 Routes

Once systems exist, the generator wires them together with **warp routes**. Routes are bounded by a configurable maximum warp distance and a route density. Their weights factor in distance falloff, plus per-route bonuses for trade hubs and populated worlds, and penalties for warp phenomena. You can also author **custom route modifiers** in TOML — e.g. "any route touching a Forge World is 1.5× more likely to exist" — to bias the layout toward stories you want to tell. A `Perilous` hazard tier marks routes as fully impassable; lesser hazards (Unstable, Hazardous, Dangerous) just degrade route safety.

If `ensure_connected_graph` is on, the generator adds bridge routes so every system is reachable from every other, even if the natural weighting wouldn't have produced a connected graph. Each route also has an undirected, deterministically-named ID (`route-sys-0002-sys-0007`) so it can be referenced by other tools.

Routes carry a per-faction **route control** record: for each route, for each faction with skin in the game, the model reports patrol presence, toll authority, interdiction status, piracy levels, secrecy, and confidence. This produces situations like *"the Imperial Navy patrols this lane, Rogue Trader X collects tolls, and pirate clan Y has infested it"* — all on the same edge, all visible at once.

### 2.7 Subsectors

After a sector is generated, an additional pass groups its systems into **subsectors** by k-means clustering with greedy farthest-first seeding and Lloyd refinement over hex distance. Each subsector gets:

- A spreadsheet-style label (A, B, … AA, AB, …) assigned row-major over capital coordinates.
- A name derived from its capital system (e.g. *"Subsector Aurelia"*).
- The list of every hex it covers — including empty ones — driving subsector borders on the map.
- Internal routes (both endpoints inside) vs. border routes (cross-subsector) classified separately.
- Adjacency: neighbouring and connected subsectors.
- A political summary: world-type counts, faction control in basis points (1/10000 fractions), dominant factions, the controlling faction (if there is a clear one), the chosen capital system and capital world, and per-faction control tiers (`absolute`, `clear`, `plurality`, `contested`, `presence`, `trace`).

Cluster count is driven by a target systems-per-subsector value (default 12). Subsectors are derived on demand from a stored sector rather than baked into the JSON, which means you can re-cluster the same sector with different parameters without regenerating it.

---

## 3. The simulation layer (NEXT-tier features)

Beyond the static political snapshot, the application carries an additional simulation-style layer of features. These produce *narrative* state — the kind of thing you'd want to write down on a GM's notebook — rather than just structural facts about the sector.

- **Surface regions.** Every world is subdivided into named geographic regions: capitals, hive blocks, underhives, forge complexes, shrine continents, agri-belts, hunting grounds, and so on. Each region has its own dominant faction, so political control inside a single world can vary by geography — *"the capital is loyalist, but the southern hive is in open rebellion."*
- **Orbital assets.** Each system carries a discrete inventory of orbital structures: stations, shipyards, defence platforms, blockade fleets. These determine who actually controls *space* around the system as distinct from the surface, and feed a per-system blockade report (is the system being blockaded? by whom?).
- **Hidden routes.** Alongside warp routes, the generator adds covert lanes that ignore the warp-distance cap: **Webway** branches between Aeldari-flavoured factions, **Black Ship** lanes between Imperial inquisitorial powers, and **Smuggling Lanes** between criminal endpoints. They are gated by faction kind so they only appear where they would canonically make sense.
- **Conflict state.** Each world and each system carries a per-tick conflict record: momentum, intensity, mobilisation level, attacker, defender, and the currently visible controller. A single tick function (`advance_sector`) progresses the simulation, applying hysteresis so political states do not flip back and forth on the boundary.
- **Intel and fog-of-war.** Per-system **intel records** are keyed by observer faction: what does faction X *believe* is happening on system Y? This includes suspected (rather than actual) presences, propaganda state, classification, and a redaction helper that turns the ground truth into the version each faction sees. Useful for asymmetric-information scenarios.
- **Faction archetypes.** Each faction is annotated with archetype-specific narrative state. The eight archetypes are: **Imperial** (governance stack, sector tithe burden), **Necron** (phase / awakening level), **Tyranid** (hive-fleet front position), **Ork** (Waaagh! momentum), **Genestealer** (staged-uprising step), **Tau** (sphere of expansion), **Aeldari** (intermittent presence pattern), **Chaos** (corruption stages). Each one produces a domain-specific state block on every system, ready to be read into a campaign log.
- **Power projection.** A per-faction graph-walk projects each faction's source power along the route network — source × doctrine ÷ (1 + hops²), with hidden routes gated by faction kind. This produces a sector-wide "shadow map" of where each faction's reach extends beyond its actual presences.
- **Influence field.** A continuous Voronoi-style cell assignment with 1/(1+d²) falloff colours every hex of the sector, including empty voids, with the influence of the nearest factions. The result is a smooth political "wallpaper" underneath the discrete system placements.
- **Stability state.** Each world and each system carries a derived stability snapshot: public order, corruption, fear, rebellion, xenos threat, warp instability, famine. These are pure derivations from the existing data — no extra RNG — so they remain reproducible.
- **Sector save.** Runtime state (conflict ticks, intel, claims-as-evolved, propaganda) can be split off from the static catalog half into a small IDs-only `SectorSave` JSON. This makes it cheap to re-apply a campaign's accumulated state on top of a freshly regenerated sector — i.e. you can change the data files, re-roll the structural layer, and overlay your campaign progress onto the new world.
- **Entity-world view.** An optional flat, columnar view of the entire sector (systems, worlds, factions, routes as separate entity tables) is available for callers that want an ECS-friendly shape without a full ECS migration.

---

## 4. The graphical front end

`sectorforge-viewer` is a desktop application (native on macOS / Windows / Linux with a graphical display; not headless) that loads any generated sector and lets you explore and edit it interactively.

`sectorforge-builder` is a separate desktop application for building and saving sector projects. It owns the builder workspace, project tree, generation controls, live preview, undo/redo command bus, and project save path; the viewer can then open the same directory with `sectorforge-viewer --project <dir>`.

### 4.1 Sector view

The top-level view is the hex map. You can zoom and pan, and every system hex is coloured by its primary star colour, tinted by its dominant faction (deterministic colour + glyph per faction, derived from `kind`, `id`, and `disposition`), and overlaid with subsector borders. Clicking a hex drills into the system detail view.

A **heatmap** control on the sector view lets you reshade every hex by one of several derived scores, instead of by star colour:

- **Control** — dominant-faction colour at intensity proportional to control score.
- **Military** — military power per system.
- **Trade** — commercial / economic activity.
- **Industry** — industrial / forge output.
- **Covert** — hidden / espionage activity.
- **Faith** — ecclesiastical / ideological presence.
- **Threat** — military × covert restricted to hostile/zealous factions.
- **Intel** — low-visibility systems glow, surfacing fog-of-war.

The current heatmap selection carries through to PNG export, so you can save sector posters in any of these modes.

### 4.2 System view

Clicking a system pops up a detail panel: worlds, coordinates, star type, system tags, all factions with their influence tiers, the system's `control` state (Pacified, Warzone, etc.), neighbouring systems, the system's orbital assets and any blockade report, its conflict tick state, its archetype block, and its full list of routes with their per-faction route-control records.

### 4.3 Edit mode

A full sector editor lets you:

- Rename systems and worlds.
- Add and remove worlds, change their world type, government, atmosphere, tags, and per-world faction presence.
- Add and remove factions on individual worlds, adjust influence and dominance.
- Edit routes — add, remove, change hazard tier.
- Manage the project's faction roster from a dedicated **Factions** tab, with deterministic colour + glyph chips, filtering by kind and disposition, sorting by total power, and the ability to pin favourites to the top.
- Save modified data back out to the project files.

### 4.4 Data editor

The typed `worlds.toml` editor is built into the application. You can author and tweak the candidate pool entirely inside the GUI — variant dropdowns and live weight controls — with validation feedback in the editor, without ever opening a text editor.

### 4.5 Route planner

A dedicated **Planner** view lets you pick a `from` and `to` system and find a route between them across the existing warp lanes, with two metrics:

- **Safest** — Dijkstra-weighted by hazard tier (avoids Unstable / Hazardous / Dangerous).
- **Shortest** — BFS over hop count.

`Perilous` routes are always treated as impassable. This is a quick way to answer "can my characters get from A to B, and how risky is it?" without leaving the application.

### 4.6 Export from the GUI

The GUI can export PNG maps at any integer scale (1× through 8×): the full sector overview, a single system, or the entire batch of per-system maps. The current heatmap and faction-fill toggles carry into the export, so you can stage the picture you want and then save it as a finished image.

---

## 5. Output formats and downstream workflows

Every generated sector can be exported in several flavours, chosen via the project's TOML or overridden on the command line.

- **JSON.** The canonical machine-readable export. A single top-level `sector.json` plus optional one-JSON-per-system files in `systems/`. This is the format you would integrate into another tool, a campaign-tracker web app, a virtual tabletop, etc. The schema includes all of the strata above: systems, worlds, factions, routes, subsectors, control summaries, claims, orbital assets, conflict, intel, archetypes, power projection, influence field.
- **Markdown.** Human-readable, intended for a GM's binder or a campaign wiki. Includes a sector summary, an ASCII map, a system index table, one block per system (coords, star, world table, factions, notes), and full routes and factions tables. There's also a per-system Markdown render available — useful for spinning up a one-off briefing document for a single star.
- **Bitmap PNG.** Pixel-rendered maps. The sector PNG includes the hex grid, all systems, all routes (with per-route midpoint glyphs for route control), faction tinting per hex, subsector borders, and an embedded legend. Per-system PNGs render the worlds inside one system, each haloed in its dominant faction's colour. A 5×7 embedded font draws labels at any scale without external font assets. PNG scale is integer 1× through 8×.
- **HTML.** Self-contained interactive sector map: one file with the sector JSON inlined alongside a vanilla-JS canvas renderer. Pan, zoom, click systems, toggle heatmaps, swap themes. No network calls, no external assets.
- **Manifest.** A `manifest.json` lists the seed, seed hash, generator name and version, settings digest, BLAKE3 digest of every input file, and final counts (systems, worlds, routes). This is what makes the output *auditable*: anyone with the manifest can verify they have the exact same input files you ran the generator against.

The output formats are independent toggles, so you can run an integration pipeline that emits only JSON, an author workflow that emits only Markdown, or a print-shop workflow that emits only PNGs.

---

## 6. Validation and invariants

Sectorforge takes the position that *invalid output should not be possible*. To enforce that, validation runs at two points.

**Pre-generation validation** runs over the project config and the world data before any generation happens. It catches things like an empty grid, a system count larger than the grid, world-count ranges that are inverted, world-data files that produced zero usable candidates, duplicate faction IDs, faction weights that aren't finite and positive, route weights that aren't positive, and empty name pools. Errors block generation. Warnings are visible in the report and only block if you've asked for strict mode.

**Post-generation invariants** run after a sector has been built (or whenever you re-load one from disk via `validate-sector`). These check that:

- All system IDs and world IDs are unique and stably formatted.
- All system coordinates are inside the declared grid bounds and unique.
- Every route endpoint refers to a real system.
- Every route's stored distance matches the actual hex distance between its endpoints.
- The undirected route graph has no duplicate edges.
- The faction summary's references are coherent with the per-world faction lists.
- World tag namespaces are present and well-formed.
- The manifest's declared counts match the actual counts.

If any invariant fails, the sector is not written. The whole point is that any file labelled `sector.json` is by construction internally consistent.

There is also a dedicated **inspector** command (`inspect-worlds`) that diagnoses a world-data directory in isolation — useful when you're authoring a `worlds.toml` and want to know how many usable rows you have, what the top-weighted star colours / world types / notable features look like, and which rows were excluded.

---

## 7. Determinism and reproducibility

The generator is fully deterministic. Same seed plus same inputs plus same version equals byte-identical output. This is enforced architecturally:

- The seed is a user-controlled string, not a number — so it can be a meaningful campaign slug like `"hadrumetum-2026"`.
- Each generation stage derives its RNG from `blake3("sectorforge:{seed}:{stage}:{discriminator}")`, so stages are independent of each other and adding a new stage doesn't disturb the output of older ones.
- All maps that hit serialisation use ordered key types so output order is stable.
- Every ID — systems, worlds, routes — is a stable, sorted string (`sys-0001`, `sys-0007-w03`, `route-sys-0002-sys-0007`).
- The manifest records a BLAKE3 digest of every input file, so you can verify the inputs are unchanged before re-running.

A golden-output test asserts byte equality across two independent runs of the bundled example with identical seed, and property-based tests fuzz the invariants and the determinism guarantee across random seeds, sector sizes, and world-count ranges.

Practical consequences:

- A finished sector can be checked into version control as `sector.json` and any reviewer can regenerate it from scratch and confirm match.
- Sharing a sector with another GM is just sharing the seed and the project directory.
- Re-rolling means changing the seed, not regenerating with the same one.

---

## 8. Customisation surface

Everything that drives the sector is user-editable.

- **The world taxonomy is yours.** The set of legal star colours, world types, atmospheres, biospheres, governments, and notable features comes from the enum set in [src/worlds.rs](src/worlds.rs). Edit the source list to prune the canonical 40k taxonomy down to whatever subset you want, or extend it.
- **The candidate pool is yours.** `worlds.toml` is the weighted population the generator draws from. Adding a `[[generation]]` table adds a new candidate world template. Increasing a row's weight makes that template more common in generated sectors. Omitting a field makes it unweighted.
- **The names are yours.** Two name TOML files, one for systems and one for worlds, each accept either a list of complete names, or prefix and suffix pools to compose from, or both.
- **The factions are yours.** A factions TOML file lists every faction that can appear, its kind, its dispositional flavour, its weight, and the world types / governments / notable features it prefers.
- **The routes are yours.** A routes TOML file declares the route weighting policy, including arbitrary modifiers (`when notable_feature = "TradeHub", multiplier = 2.0`).
- **The generation policy is yours.** A single `sectorforge.toml` controls grid size, system count, world counts per system, placement mode, star-colour bias, route density, route distance cap, name fallback patterns, and bitmap output options.

The same project directory format is consumed identically by the CLI and the GUI, so you can author data in your editor of choice or directly in the GUI's built-in data editor.

---

## 9. Workflows the application supports

A few representative end-to-end flows the tool is built for:

- **One-shot sector for a new campaign.** Start from the bundled `examples/m42_project`, change the seed to your campaign slug, optionally tweak faction weights, run `generate`, open the Markdown briefing or the PNG poster. Done.
- **Iterate until happy, then lock.** Re-run `generate` with different seeds until the political situation reads well, then commit `sectorforge.toml` + `seed` + the data files to version control. Anyone with that checkout can regenerate the same sector.
- **Single-system on demand.** Use `generate-system` to spit out one isolated star system at chosen coordinates with chosen seed and index. The output is a standalone JSON (and optionally Markdown) file. Useful for one-off NPC systems or scratchpad work that doesn't justify a full sector.
- **Author the world catalogue.** Open the GUI's Data editor, build up a custom world taxonomy and weighted candidate pool, validate as you go via the warnings panel, and regenerate without ever leaving the application.
- **Re-cluster subsectors.** Generate a sector once, then re-cluster it with different `target_systems_per_subsector` values — the subsector layer is derived on demand and not baked into the sector JSON.
- **Integrate into another tool.** Treat `sectorforge` as a content pipeline: emit `sector.json` from CI, hand it to a downstream consumer (campaign tracker, web map, VTT loader), and use the BLAKE3 manifest to detect input changes.
- **Asymmetric information.** Use the per-faction intel records to ask "what does faction X see of this sector?" and produce filtered outputs that hide presences below a confidence threshold — handy for running campaigns where players know less than the GM.
- **Time-evolve a sector.** Use the `advance_sector` tick + the IDs-only `SectorSave` split/merge to evolve the political situation between sessions while keeping the static catalog regenerable.

---

## 10. Out of scope

It is worth being explicit about what the application is *not*:

- It is not a game. There are no turns, no players, no win condition, no rules engine. The "simulation layer" is a state model, not a game system.
- It is not a tactical map generator. It builds sectors and systems, not planetary surfaces, dungeons, or battlefields.
- It is not a fluff database. It does not ship with canonical 40k named characters, organisations, or events — the bundled example data is generic by design so the tool can be reused. You author your own setting on top.
- It is not networked. There is no server, no multiplayer editing, no cloud sync. A project directory is a folder you own.
- It is not a publishing tool. The Markdown / PNG output is meant for downstream consumption (a wiki, a printout, a VTT), not as a final-form rulebook layout.

Within those limits, what it *does* aim for is a single application that takes a couple of seconds to produce a politically dense, fully-cross-referenced, byte-reproducible Warhammer-40k-style star sector with the depth a campaign or piece of fiction can be built on top of — and a graphical editor that lets you make that sector yours after the fact.
