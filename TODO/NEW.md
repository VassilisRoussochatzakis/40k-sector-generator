# NEW — Proposed Features for `sectorforge`

This document proposes new features for `sectorforge`, written as a companion to `OVERVIEW.md`. Each section is one self-contained proposal: what it is, why it fits the existing design, how it could work without breaking the reproducibility contract, what data/config surface it adds, how it surfaces in the CLI and GUI, the notable edge cases, and a rough effort/risk read.

The guiding constraint throughout: **every proposal must remain byte-deterministic** (same seed + same inputs + same version ⇒ same output), **offline**, **data-driven**, and **setting-agnostic at the data layer**, because those are the properties that make the current tool trustworthy. Where a feature would naturally want randomness, it derives its RNG from the existing `blake3("sectorforge:{seed}:{stage}:{discriminator}")` scheme as a new, isolated stage so it cannot perturb the output of older stages.

Proposals are ordered roughly by how central they are to the tool's stated purpose (building reusable campaign settings), not by implementation difficulty.

---

## 1. Deterministic sector history / chronicle generator

### What it is

Today a generated sector is a *snapshot*: it has claims that imply a past ("Chaos has been holding it by Military Occupation for forty years") but no actual narrated past. `advance_sector` evolves the situation *forward* one tick at a time, but there is no *backward* story explaining how the present configuration came to be. This feature adds a **history pass** that produces a deterministic, chronological backstory for the sector: a dated list of events that, read top to bottom, explains the current claims, control summaries, and conflict states.

### Why it fits

The tool already computes everything a history needs as a *consequence* — competing claims, dominance states, contested flags, archetype state (Chaos corruption stage, Genestealer uprising step, Necron awakening level, Tyranid front position). A history pass is essentially the inverse derivation: given that Chaos holds a world by Military Occupation while the Imperium retains an Imperial Mandate, *reconstruct a plausible sequence of events* that produces exactly that end-state. This is the single highest-value addition for the headline "campaign/fiction setting" use case, because GMs and writers consistently need the *why*, not just the *what*.

### How it works (determinism-safe)

A new pipeline stage, `history`, seeded from `blake3("sectorforge:{seed}:history:{sector|system|world}:{id}")`. The generator walks each world's claim list and faction power profile and emits events from a **deterministic event grammar** authored in TOML (similar in spirit to the existing route-modifier and faction-preference TOML). Example event templates: *foundation/colonisation*, *discovery by faction*, *first contact*, *annexation*, *secession/rebellion*, *invasion*, *purge*, *quarantine declared*, *cult exposure*, *trade charter granted*. Each template has preconditions expressed against existing model fields (e.g. a `RELIGIOUS_MANDATE` claim plus an Ecclesiarchy presence can emit a "consecration" event; a `REBELLION` claim plus high `fear`/`rebellion` stability can emit an "uprising" event).

Dating is synthetic but consistent: events are placed on a relative timeline (e.g. M41.215 → M42.001 style 40k notation, with the imperial-calendar format being just a name-pattern in config so it stays setting-agnostic). Ordering is topologically derived from claim dependencies (you cannot have a "reconquest" before a "loss"), then stably sorted, so the chronicle is reproducible.

Critically, **history is a pure derivation with no extra RNG draws affecting other stages** — exactly like the existing power-profile and stability derivations. It can be regenerated on demand from a stored `sector.json` (like subsectors), so it does not bloat the canonical JSON unless explicitly emitted.

### Data / config surface

- `history.toml` — event templates: id, narrative text pattern, preconditions over model fields, weight, calendar format string.
- `sectorforge.toml` — `[history]` block: enable, depth (how many events per world/system), calendar epoch and notation pattern, whether to emit into `sector.json` or only on demand.

### Surfacing

- CLI: `sectorforge history <project>` emits `history.md` (per-world and per-system chronicles plus a sector-wide "key events" digest) and optionally a `history` array in JSON.
- GUI: a **Timeline** tab on the system/world detail panels and a sector-wide history scroll; events are clickable and cross-link to the worlds/factions they reference.
- Markdown export gains an optional "History" section per system.

### Edge cases

Worlds with no claims and no notable features should still get at least a minimal foundation event so the chronicle is never empty. Contradictory claim sets (three competing claims) are a feature, not a bug — they should produce *parallel* event threads that visibly diverge, which is exactly the dramatic material a GM wants.

### Effort / risk

Medium-high effort (the event grammar and precondition engine are the bulk). Low risk to determinism if implemented as a derived, isolated stage. High narrative payoff.

---

## 2. Constraint-directed generation and seed search ("wishes")

### What it is

Right now the iteration loop is "re-roll seeds until the political situation feels interesting, then lock." That is a manual, eyeball-driven search. This feature lets the user *declare what they want* — e.g. "at least one Forge World under Mechanicus control, one Quarantined system, total Chaos control between 25% and 40%, and at least one three-way contested hive world" — and have the tool **find a seed that satisfies it**, or report that none was found within the search budget.

### Why it fits

The OVERVIEW explicitly frames the workflow as "iterate until happy, then lock." This feature automates the "until happy" part while keeping the "lock" guarantee fully intact: the output is still just a seed plus the unchanged project directory, so reproducibility and the manifest contract are untouched. It turns a vague, slow loop into a precise, fast one without changing the data model at all.

### How it works (determinism-safe)

Two layers:

1. **Constraint language.** A small predicate DSL evaluated against the *finished* sector model: counts, ratios, existence ("∃ a world where type = ForgeWorld ∧ dominant.kind = Mechanicus"), aggregate bounds on the per-faction sector power profile, connectivity properties of the route graph, presence of a given system political state. This reuses fields the model already exposes.
2. **Search.** A deterministic, ordered seed enumeration: derive candidate seeds from a base via `blake3("sectorforge:{base_seed}:search:{n}")`, generate each, evaluate constraints, stop at the first satisfying seed (or best-effort closest if none found, ranked by a violation score). Because the seed sequence is itself derived deterministically from the base, *the search itself is reproducible*: re-running the search with the same base and constraints returns the same winning seed.

Generation during search can stop early (it does not need the full simulation/intel layer to evaluate most structural constraints), keeping the search fast.

### Data / config surface

- `wishes.toml` (or a `[search]` block) — list of constraints, search budget (max seeds), base seed, an optional "soft" weighting for closest-match fallback.

### Surfacing

- CLI: `sectorforge search <project> --wishes wishes.toml` prints the winning seed, the constraint satisfaction report, and optionally writes the full sector for that seed.
- GUI: a **Wishes** panel where constraints are built with dropdowns (faction, world type, comparison, bound); a "Find a sector" button runs the search with a progress bar and shows the satisfying sector when found, plus a per-constraint pass/fail breakdown.

### Edge cases

Over-constrained wish sets that no seed in budget can satisfy must fail loudly with the closest near-miss and a per-constraint diagnostic ("Chaos control reached at most 18%, never the requested 25%"), so the user can relax the right constraint rather than guess. Constraints that are *structurally impossible* given the candidate pool (e.g. requiring a world type with zero weight in `generator.csv`) should be caught pre-search with a clear message, reusing the existing pre-generation validation machinery.

### Effort / risk

Medium effort. Very low determinism risk (it is a search over the existing deterministic generator, not a change to it). Extremely high quality-of-life value — arguably the feature most users would notice first.

---

## 3. Dramatis personae — deterministic named-character generator

### What it is

The political model is deep but **anonymous**. A world can be administered by the Imperium, economically run by a Rogue Trader dynasty, and infiltrated by a cult — but nobody has a *name*. This feature generates a deterministic cast of named individuals tied to the political structure: planetary governors, sector lords, Rogue Trader captains, inquisitors, cult magi, Ork warbosses, Tau ethereals, cardinals — one or more per significant faction presence, with a title, a few traits, and a one-line agenda.

### Why it fits

`OVERVIEW.md`'s "Out of scope" section says the tool ships *generic* example data on purpose so it can be reused — but that is about not shipping *canon* characters, not about not generating characters at all. A character generator that draws entirely from user-supplied name and trait pools stays perfectly within that philosophy: it generates *your* cast from *your* data, setting-agnostically. The hooks already exist — every Dominant/Significant presence and every per-system sovereign/occupier/hidden-master slot is a natural anchor for a person.

### How it works (determinism-safe)

A `personae` stage seeded from `blake3("sectorforge:{seed}:personae:{faction_id}:{world_or_system_id}")`. For each qualifying presence (configurable threshold — e.g. Dominant or Significant tier, or per-system sovereign/hidden-master slots), draw:

- A name from a new per-faction-kind name pool (TOML, mirroring the existing system/world name files: list, or prefix+suffix, or both).
- A title from a per-faction-kind title pool ("Planetary Governor", "Lord Militant", "Magos Dominus", "Cult Patriarch", "Rogue Trader", "Warboss").
- 1–3 traits from a trait pool, optionally biased by the world's notable features and the faction's disposition (a "police state" world biases the governor toward "Paranoid"/"Iron-Fisted"; a zealous faction biases toward "Fanatical").
- A one-line agenda template instantiated against the local political situation ("seeks to break the Rogue Trader's commercial charter", derived from the actual competing claims on that world).

No extra RNG affects other stages; personae are a derived overlay like subsectors and history.

### Data / config surface

- `personae.toml` — per-kind name pools, title pools, trait pools, agenda templates with preconditions over model fields.
- `sectorforge.toml` — `[personae]` block: which tiers/slots get a character, max characters per world/system, emit-to-JSON toggle.

### Surfacing

- CLI: characters appear in `history.md`/Markdown system blocks and (optionally) a `personae` array in JSON.
- GUI: each faction chip in the system/world detail panel expands to show its named representative; a sector-wide **Cast** tab lists everyone, filterable by faction/world, with the same deterministic colour+glyph treatment factions already get.

### Edge cases

Name-pool exhaustion across a large sector must be handled deterministically (compose via prefix+suffix fallback exactly as the existing name system does, then a numeral fallback) so two characters never silently collide on the same name without a stable disambiguator. Characters should be regenerable on demand and *not* baked into `sector.json` by default, to keep the canonical artifact lean and to let users re-roll just the cast without touching the structural layer.

### Effort / risk

Medium effort. Low determinism risk. High value for both GMs (NPCs to run) and writers (named figures to write about).

---

## 4. Inter-faction diplomacy / relationship layer

### What it is

Factions currently have a `kind`, a `disposition`, and per-world presences, but there is **no explicit model of how factions relate to each other**. Two Imperial factions and a Chaos faction on the same world are simply three presences; nothing encodes that the Imperials are nominally allied and both are at war with Chaos, or that two criminal factions are rivals. This feature adds a deterministic **faction relationship matrix**: for every ordered pair of factions, a stance (Allied, Aligned, Neutral, Rival, Hostile, At War) plus a short cause.

### Why it fits

The conflict layer already models attacker/defender and momentum per world/system, and the control summary already picks an `occupier` distinct from a `sovereign`. But "who is fighting whom and why" is currently *implied* by kinds rather than *stated*. An explicit relationship layer makes the conflict state legible and gives the existing `advance_sector` tick a principled input (a war between two factions should bias contested worlds where both are present), without adding any new randomness to the structural layer.

### How it works (determinism-safe)

A `relations` stage, seeded from `blake3("sectorforge:{seed}:relations:{faction_a}:{faction_b}")` with a canonical pair ordering for stability. Base stance is derived from `kind` × `kind` and `disposition` × `disposition` rules authored in TOML (Imperial↔Chaos ⇒ At War by default; Mechanicus↔Imperial ⇒ Aligned; two Criminal kinds ⇒ Rival unless dispositions both opportunistic ⇒ Neutral). A small deterministic perturbation per pair (from the seeded RNG) breaks ties so not every same-kind pair is identical, then the matrix is frozen and stably serialised. Optionally, *contested overlap* (how often the two factions co-occur on contested worlds) feeds a derived "tension" scalar — a pure derivation, no RNG.

### Data / config surface

- `relations.toml` — kind×kind and disposition×disposition base-stance rules, optional explicit per-pair overrides by faction id, cause-text templates.
- `sectorforge.toml` — `[relations]` toggle and whether the matrix feeds the conflict tick.

### Surfacing

- CLI/JSON: a `relations` matrix block; Markdown gains a "Factions at war" digest.
- GUI: a **Diplomacy** matrix view (factions on both axes, cells coloured by stance); clicking a cell explains the cause and lists the worlds/systems where that relationship is "live" (both present).
- Heatmap: a new **Tension** mode highlighting systems where hostile/at-war pairs co-occur — a natural sibling to the existing Threat heatmap.

### Edge cases

The matrix must stay coherent if a faction is added/removed in the GUI editor (recompute the affected row/column, leave the rest stable). Self-pairs are identity (Allied with self) and excluded from tension. The relationship layer should *bias* the conflict tick, never *override* explicit edited conflict state, so GM edits remain authoritative.

### Effort / risk

Medium effort. Low-to-medium risk (the conflict-tick coupling needs care to stay deterministic and to respect hysteresis). High value — it makes the political model self-explanatory.

---

## 5. Regional warp phenomena and large-scale map overlay

### What it is

The model has *per-route* hazard tiers (Unstable → Perilous) and *per-world* warp-phenomena notable features, but **no regional, large-scale warp feature** — the kind of thing 40k settings are built around (a warp storm region, a rift, a halo zone, a calm "safe lane" corridor). This feature adds deterministic **regions**: contiguous areas of the hex grid carrying a warp condition that modifies generation and routing inside their footprint.

### Why it fits

The influence-field feature already proves the tool is comfortable with continuous, hex-by-hex overlays under the discrete system placements. A warp-region overlay is the structural-hazard counterpart: instead of colouring hexes by faction influence, it colours regions by warp condition and *feeds that back into route weighting and world generation* (more warp-phenomena features inside a storm; routes crossing a storm boundary degraded or impassable). It directly enables the iconic "the only safe way into the sector is the Aurelian Corridor" story shape the tool is meant to support.

### How it works (determinism-safe)

A `regions` stage seeded from `blake3("sectorforge:{seed}:regions")`, run *before* route generation so routes can react to it. Region shapes are generated by a deterministic seeded process (e.g. seeded blob/Perlin-like growth from a small number of region centres, or polygon footprints) over the hex grid, each tagged with a condition from a TOML catalogue: `WarpStorm` (routes crossing become Perilous), `Turbulence` (one hazard tier worse), `CalmCorridor` (one tier better, ignores some distance falloff), `Blackout` (no covert/hidden routes generate inside), `Anomaly` (biases nearby world generation toward ancient-ruins / warp-phenomena candidates by reweighting the existing candidate pool, not by inventing new world types).

Because regions are computed before routes and worlds, the existing route-weighting and candidate-pool draws simply see modified weights — the determinism contract is preserved by ordering and by deriving region RNG from its own stage discriminator.

### Data / config surface

- `regions.toml` — condition catalogue: id, route effect, world-generation reweight rules, colour, label-name pool.
- `sectorforge.toml` — `[regions]`: enable, count or density, size distribution, placement mode (reusing the existing uniform/weighted/clustered vocabulary), whether region effects are advisory or hard.

### Surfacing

- JSON: a `regions` block (footprint hex lists + condition), and route records gain a "modified by region" provenance note.
- GUI: regions render as a translucent tinted overlay under the hex grid (composable with the existing subsector borders and influence field); the Route Planner accounts for region effects automatically.
- PNG/Markdown: regions drawn/listed; the ASCII map gains region glyphs.

### Edge cases

Region effects must compose predictably with per-route hazard tiers (define a clear precedence: region `WarpStorm` forcing Perilous overrides a naturally-Safe route; a `CalmCorridor` cannot upgrade a route already flagged Perilous by another rule — document the lattice). The `ensure_connected_graph` bridge pass must run *after* region effects so a storm cannot silently disconnect the sector without the bridge logic compensating; if a region genuinely isolates systems, that should be a reported warning, not a broken graph.

### Effort / risk

Medium-high effort (region geometry + the effect-composition lattice). Medium risk (ordering relative to routes and the connected-graph guarantee needs careful design). Very high setting value — this is a top-three "makes it feel like 40k" feature.

---

## 6. Narrative prose / gazetteer generator (template grammar, not an LLM)

### What it is

The Markdown export today is structured: tables, indices, bullet blocks. This feature adds an optional **in-universe prose layer** — a deterministic grammar that turns the structured model into readable narrative text: a sector gazetteer entry per system, an Imperial-dispatch-style summary, a faction briefing. Not an LLM (that would break determinism and the offline guarantee) but a seeded **template/Backus-style grammar** filled from the model.

### Why it fits

The OVERVIEW says the tool is "not a publishing tool" and the Markdown is for downstream consumption — this proposal stays inside that boundary by producing *raw narrative source text* (still Markdown), not laid-out pages. It is the natural complement to the history generator (§1): history gives events, prose gives voice. For the "writer" persona explicitly named in the use cases, this is the difference between a spreadsheet and a starting draft.

### How it works (determinism-safe)

A `prose` stage seeded from `blake3("sectorforge:{seed}:prose:{id}")`. A TOML/grammar file defines sentence templates with slots bound to model fields and conditional clauses ("`{system}` is a {state} system of {n} worlds, {if dominant} held by {dominant} {endif}…"). Variation comes from seeded selection among synonym lists and template alternatives — fully reproducible. Tone presets (terse Administratum report vs. florid gazetteer) are just different grammar files, keeping it setting-agnostic and user-owned.

### Data / config surface

- `prose.toml` — templates, synonym pools, tone presets, per-section toggles.
- `sectorforge.toml` — `[prose]`: enable, tone, which sections (sector overview / per-system / per-faction).

### Surfacing

- CLI: `sectorforge prose <project>` → `gazetteer.md`; also injected as opening paragraphs into the existing per-system Markdown blocks.
- GUI: a "Read as gazetteer" toggle on the system detail panel rendering the prose version above the structured tables.

### Edge cases

Templates must degrade gracefully when optional slots are empty (no "held by " with a blank). Repetition across many systems is the main quality risk — mitigate with seeded template/synonym rotation keyed by id so adjacent systems don't read identically. Keep the grammar strictly data-bound (no invented facts) so prose can never contradict the JSON.

### Effort / risk

Medium effort. Low determinism risk. High value for writers; moderate for GMs.

---

## 7. Adventure & plot-hook generator

### What it is

A derived layer that turns the political *tension already in the model* into concrete, runnable **adventure hooks**: short structured prompts of the form "situation → stakes → factions involved → possible complications," anchored to specific worlds/systems/routes.

### Why it fits

The model is, by design, dense with latent drama: contested worlds, three competing claims on one planet, hidden masters, blockaded systems, Genestealer uprising steps, recidivist underbellies, piracy on a tolled route. None of that is currently *surfaced as a prompt a GM can run*. This feature is pure derivation over fields that already exist — arguably the highest GM-utility-per-line-of-code feature in this whole document, because it monetises modelling work the tool already does.

### How it works (determinism-safe)

A `hooks` stage seeded from `blake3("sectorforge:{seed}:hooks:{anchor_id}")`. A TOML rulebook maps model conditions to hook templates: e.g. `contested == true ∧ hidden_master.kind == Genestealer` ⇒ "Counter-infiltration" hook; `claim ∈ {MILITARY_OCCUPATION} ∧ claim ∈ {IMPERIAL_MANDATE}` on the same world ⇒ "Reconquest / liberation" hook; a `Perilous` route between two populated systems ⇒ "Find the lost passage" hook; route with patrol + tolls + piracy ⇒ "Convoy escort" hook. Each fired rule instantiates a template with the real entities and a seeded pick among complication clauses. Hooks are ranked by a derived "dramatic weight" (contested-ness + number of competing claims + stability extremes) so the GM gets the juiciest ones first.

### Data / config surface

- `hooks.toml` — condition→template rules, complication pools, weighting.
- `sectorforge.toml` — `[hooks]`: enable, max hooks per anchor, sector-wide top-N digest size.

### Surfacing

- CLI/JSON: a `hooks` array; Markdown gains a "Plot hooks" section per system and a sector-wide "Top 10 hooks" digest.
- GUI: a **Hooks** tab; each hook links to the worlds/factions/routes it references; a "surprise me" button surfaces one high-weight hook at random (deterministically, from the seeded list).

### Edge cases

Avoid hook spam on dense sectors — cap per anchor and dedupe near-identical hooks deterministically. Hooks must reference only real, present entities (no "the cult on this world" if no cult presence exists). Respect the intel/fog-of-war layer if requested: a "player-facing" hook export should hide hooks that depend on Hidden-tier presences.

### Effort / risk

Low-medium effort (it is rule-matching over existing fields). Very low determinism risk. Very high GM value.

---

## 8. Sector analytics & balance dashboard

### What it is

A computed report (and GUI panel) that statistically characterises a *generated* sector: faction balance (Gini-style concentration of sector power), contested-world ratio, average claims per world, route-graph connectivity metrics (diameter, articulation points, isolated risk), world-type distribution vs. the configured candidate pool, subsector political variety. Essentially the output-side counterpart to the existing input-side `inspect-worlds`.

### Why it fits

The OVERVIEW already has an inspector for *input* world-data quality (`inspect-worlds`) but nothing equivalent for *output* sector quality. The iteration workflow ("re-roll until the political situation feels interesting") is currently eyeball-driven; analytics replace "feels interesting" with measurable signals, and pair perfectly with the seed-search feature (§2) by giving the user the vocabulary to express constraints. It is pure read-only derivation — zero determinism risk.

### How it works (determinism-safe)

A read-only `analyze` pass over a finished/loaded sector. No RNG at all; everything is a deterministic statistic over existing fields. Articulation-point and diameter computations run over the existing route graph. Distribution comparisons reuse the candidate-pool weights already loaded for validation.

### Data / config surface

None required (it reads the existing model). Optional `[analyze]` block to set thresholds for the "health flags" (e.g. flag if any single faction exceeds X% of sector power, or if connectivity has articulation points).

### Surfacing

- CLI: `sectorforge analyze <project>` → `analysis.md` + JSON; usable in CI to *fail a build* if a generated sector violates configured balance thresholds.
- GUI: a **Dashboard** tab with small charts (faction power bars, world-type histogram vs. configured weights, a connectivity callout listing systems whose loss would fragment the sector).

### Edge cases

Tiny sectors make some metrics degenerate (graph diameter of a 3-system sector) — report them but mark as low-confidence. Distribution-vs-pool comparison must account for consistency rules (e.g. "no two worlds in a system share a type") that intentionally skew the realised distribution away from the raw weights; explain the deviation rather than flag it as a fault.

### Effort / risk

Low effort, zero determinism risk, high decision-support value. A natural first feature to ship because it de-risks every other proposal by making sector quality measurable.

---

## 9. Scenario presets / template library

### What it is

A curated set of named configuration bundles that bias generation toward recognisable narrative shapes: *Embattled Frontier* (sparse, high-hazard routes, militarised factions), *Chaos Incursion* (a regional warp storm, high corruption, Chaos-favoured weights), *Tau Expansion Sphere*, *Dead Sector* (Necron-heavy, low population, ruins-biased candidate pool), *Mercantile Crossroads* (dense routes, Rogue-Trader-heavy, trade-hub-biased). Each preset is just a vetted `sectorforge.toml` + faction/route TOML overlay shipped with the tool.

### Why it fits

The tool's customisation surface is powerful but has a steep cold-start: a new user must understand grid size, placement mode, star-colour bias, route density, faction weights, and preferences before they get a sector they like. Presets give an *on-ramp* without compromising the data-driven philosophy — they are entirely expressed in the existing config format, not new code, and the user can open any preset and learn from it as a worked example. They also showcase features like regions (§5) and diplomacy (§4) by configuring them tastefully.

### How it works (determinism-safe)

No engine changes at all. Presets are config/data overlays applied on top of the bundled `examples/m42_project`. Determinism is automatically preserved because presets are *inputs*, captured by the existing BLAKE3 manifest digest.

### Data / config surface

A `presets/` directory of named overlays; a small CLI/GUI affordance to instantiate one into a new project directory.

### Surfacing

- CLI: `sectorforge new <project> --preset chaos-incursion` scaffolds a project from the preset.
- GUI: a "New from preset" gallery with a one-line description and a thumbnail PNG (generated deterministically from a default seed) per preset.

### Edge cases

Presets must be kept in sync with the schema across versions (a version bump that changes a config key should update or migrate presets); a tiny golden test per preset (generate-and-validate at a fixed seed) keeps them honest. Make explicit in docs that presets are *starting points*, not canon, to stay consistent with the "not a fluff database" stance.

### Effort / risk

Low effort, zero determinism risk. High onboarding value and a good showcase for the heavier features.

---

## 10. Sector diff & state-comparison tool

### What it is

A command and GUI view that **compares two sectors** — or two states of the same sector — and reports exactly what changed: added/removed systems and worlds, claim changes, control-summary flips (a world that went from Pacified to Contested), faction power deltas, route additions/removals. Primary uses: comparing two seeds side by side, and comparing a sector before vs. after one or more `advance_sector` ticks.

### Why it fits

Determinism and reproducibility are the tool's central value proposition, and the OVERVIEW already leans into version-controlling `sector.json`. A semantic diff is the missing companion: a raw `git diff` on a large JSON is unreadable, but a *model-aware* diff ("Subsector C: Imperium lost the sovereign slot on Hadrumetum III to Chaos; 2 routes degraded to Hazardous") is exactly what a GM needs between sessions and what a reviewer needs to sanity-check a re-roll. It also makes the `advance_sector` tick legible — right now you can advance the simulation but cannot easily *see* the consequence.

### How it works (determinism-safe)

Pure read-only structural comparison of two loaded sector models (or a sector + a `SectorSave` overlay before/after a tick). Stable IDs (`sys-0001`, `route-sys-0002-sys-0007`) make entity matching trivial and deterministic; the diff itself is a sorted, reproducible derivation.

### Data / config surface

None. Optional `[diff]` block to choose verbosity and which strata to include.

### Surfacing

- CLI: `sectorforge diff a/sector.json b/sector.json` → `diff.md` + machine-readable JSON; `sectorforge diff --ticks 5 <project>` auto-generates the before/after pair.
- GUI: a split or overlay map where changed hexes pulse; a changelog side panel grouped by subsector; clicking an entry flies the map to the affected system.

### Edge cases

Diffing across schema versions must detect the version mismatch (from the manifest) and either refuse or run in a clearly-labelled "best effort, schema changed" mode. Renames (same ID, changed name) must be reported as modifications, not as delete+add. ID scheme changes between versions are the main hazard — lean on the manifest's recorded generator version to gate this.

### Effort / risk

Low-medium effort, zero determinism risk. High value for between-session campaign play and for the time-evolution workflow the OVERVIEW already advertises.

---

## 11. Self-contained interactive HTML export

### What it is

A new export format: a single, dependency-free, **offline** `.html` file containing the full sector as an interactive map — pan/zoom, click a system for its detail panel, toggle the existing heatmap modes, filter by faction — with all data and rendering code inlined.

### Why it fits

The OVERVIEW lists PNG (static, not interactive) and JSON (interactive only if you build a viewer) but nothing in between. "Hand it to a downstream consumer (web map, VTT)" is named as a workflow, but currently the user has to *build* that web map. A self-contained HTML export is the lowest-friction way to share an explorable sector with a player or co-GM who does not run the desktop GUI, while staying true to the "not networked, a project directory is a folder you own" principle — the file is fully offline and contains no external calls.

### How it works (determinism-safe)

A new exporter that serialises the sector into an HTML template with the JSON inlined and a small vanilla-JS renderer (reusing the same hex/heatmap/colour logic the GUI and PNG exporter already implement). Deterministic because it is a pure transform of the deterministic model; the inlined JSON is byte-stable, so the HTML is too (modulo a fixed template).

### Data / config surface

`sectorforge.toml` `[export]` gains an `html` toggle, like the existing JSON/Markdown/CSV/PNG toggles. Optional theme (parchment / hololithic / dark) reusing the PNG renderer's palette.

### Surfacing

- CLI: part of the standard export toggle set.
- GUI: "Export → Interactive HTML" alongside the existing PNG export, honouring the current heatmap/faction-fill selection just as PNG export already does.

### Edge cases

Very large sectors produce large HTML files; gate per-system detail rendering behind lazy in-page panels and warn above a size threshold. Respect the intel/redaction layer: offer a "player edition" HTML that runs the existing redaction helper so Hidden-tier presences and GM-only hooks/personae are stripped before inlining.

### Effort / risk

Medium effort (a renderer reimplementation in JS, though it mirrors logic that already exists). Low determinism risk. High sharing/value, and it directly serves an already-stated downstream workflow.

---

## 12. Trade & resource economy layer

### What it is

The model has an *Economic* and *Industrial* power dimension per faction per world, but no actual **goods or trade flows**. This feature adds a deterministic light economy: each world produces/consumes a small set of resource categories (ore, promethium, foodstuffs, manufactured goods, archeotech, recruits/tithe) derived from its type and tech level, and routes carry derived trade volume based on the production/consumption gradient between their endpoints, modulated by the faction route-control records (tolls, piracy, interdiction) that already exist.

### Why it fits

The OVERVIEW already treats Industrial as "a first-class axis distinct from economic" — clearly the design *wants* economic texture but stops at abstract scores. A resource layer makes those scores *mean something*: a blockaded forge world's promethium shortfall becomes a concrete plot input and a concrete reason a route matters. It is a pure derivation over world type, tech level, population scale, and the existing route graph — no new RNG needed for the structural part, so determinism is free.

### How it works (determinism-safe)

An `economy` derivation: a TOML table maps (world type, tech level, population scale) → resource production/consumption vectors. Trade volume on a route = function of the endpoint surplus/deficit gradient, distance falloff (reuse route weighting), and the route's hazard tier and piracy/interdiction state from the existing route-control record. A per-system and per-sector balance sheet falls out (net importer/exporter, critical shortages). Optionally feeds the stability layer (a food-deficit world with no safe import route nudges the existing `famine` stability score) — strictly a derivation, no extra RNG.

### Data / config surface

- `economy.toml` — resource categories, production/consumption matrix keyed by existing taxonomy fields.
- `sectorforge.toml` — `[economy]` toggle and whether it feeds stability.

### Surfacing

- JSON: an `economy` block per world/route/system; CSV gains an `economy.csv`.
- GUI: a new **Trade** heatmap mode (route thickness ∝ volume) and a per-system balance-sheet section; the Route Planner can show "this lane carries the only food import for System X."
- Plot hooks (§7) can fire on economic conditions ("starving world, severed supply line").

### Edge cases

Disconnected systems (isolated by hazards/regions) must be reported as economically stranded rather than silently zeroed. The feedback into `stability.famine` must be bounded and one-directional to avoid an oscillation between economy and conflict ticks — keep it a read-only nudge consistent with the existing "pure derivation, no extra RNG" stability design.

### Effort / risk

Medium effort. Low determinism risk for the static derivation; the optional stability coupling needs the same hysteresis discipline the conflict tick already uses. High value for campaigns built on logistics/blockade stories.

---

## 13. External-tool export adapters (VTT / campaign-tracker bundles)

### What it is

First-party export *adapters* that emit sector data in the import formats of common downstream tools — e.g. a Foundry VTT scene/journal bundle, a generic campaign-tracker schema, a hex-map image set with a coordinate manifest — rather than leaving every integrator to write their own JSON transform.

### Why it fits

"Integrate into another tool … hand it to a downstream consumer (campaign tracker, web map, VTT loader)" is an explicitly stated workflow, but today the integration burden is entirely on the user. Shipping adapters as thin, deterministic transforms over the canonical `sector.json` keeps the core untouched (adapters are pure functions of the existing output) while massively lowering adoption friction. It also stays within scope: these are *export* transforms, not a networked or publishing feature.

### How it works (determinism-safe)

Each adapter is a deterministic pure transform from `sector.json` to a target schema, run as an opt-in export toggle. Because the input is byte-stable and the transform is pure, outputs are byte-stable too and covered by the same golden-test approach already used for the canonical output.

### Data / config surface

`sectorforge.toml` `[export.adapters]` listing which adapters to emit. Adapter-specific options (e.g. VTT grid size) in sub-tables.

### Surfacing

- CLI: adapters appear in the export toggle set; `sectorforge export --adapter foundry <project>`.
- GUI: an "Export for…" submenu.

### Edge cases

Third-party schemas drift between their own versions — pin each adapter to a stated target-tool version, surface it in the manifest, and golden-test each adapter independently so an upstream schema change is caught by CI rather than by a user's broken import. Keep adapters in a clearly separable module so a broken third-party format never blocks core releases.

### Effort / risk

Medium effort, scaling with the number of adapters. Low determinism risk. High adoption value; lower priority than the modelling features but a strong "ecosystem" play.

---

## 14. Multi-sector / segmentum composition

### What it is

A higher-scale mode that composes several generated sectors into a connected **super-region** (a segmentum-scale map): each child sector generated independently and reproducibly, then stitched with deterministic inter-sector warp links, an adjacency graph, and a top-level overview map and gazetteer.

### Why it fits

The OVERVIEW is explicit that a sector is the "mid-scale" unit and the tool deliberately stops there — so this is the natural *next* scale, and it composes beautifully with the determinism contract: a super-region is just a manifest of child seeds + a stitch seed, so the entire multi-sector artifact is reproducible from a handful of strings, exactly mirroring the single-sector guarantee. It serves long campaigns and fiction series that outgrow one sector without forcing the user to manage adjacency by hand.

### How it works (determinism-safe)

A `segmentum.toml` lists child sectors (each its own project + seed). A new `compose` command generates each child (reusing the existing deterministic pipeline untouched), then a `stitch` stage seeded from `blake3("sectorforge:{stitch_seed}:stitch:{sector_a}:{sector_b}")` adds inter-sector links between border systems under a configurable inter-sector distance/route policy. Child sectors are *not* regenerated by composition; their `sector.json` and manifests are reused and digested into a super-manifest, so the audit chain extends cleanly to the larger scale.

### Data / config surface

- `segmentum.toml` — child sector list, stitch seed, inter-sector link policy, super-grid layout.

### Surfacing

- CLI: `sectorforge compose segmentum.toml` → a super-map (PNG/Markdown/HTML), a super-manifest, and the unchanged child outputs.
- GUI: a top-level **Segmentum** view; clicking a sector zooms into the existing single-sector view; inter-sector links are first-class objects in the route planner ("can my characters reach the next sector?").

### Edge cases

Conflicting per-child config (different grid conventions, taxonomy versions) must be detected and reported before stitching, using the children's manifests. Cross-sector faction identity (is "the Imperium" in sector A the same entity as in sector B?) is a real modelling decision — make it explicit config (shared faction roster vs. independent rosters) rather than implicit, and have the analytics/diplomacy layers respect that choice.

### Effort / risk

High effort (a new scale tier, a stitch stage, a super-manifest, GUI navigation). Low-medium determinism risk because composition reuses the existing deterministic pipeline rather than modifying it. High value for the most ambitious users; reasonable to schedule after the modelling and tooling proposals above.

---

## 15. Scripting / plugin hooks for custom generation stages

### What it is

A sandboxed, embedded scripting interface (e.g. a Rhai/Lua-style engine compiled into the binary, fully offline) that lets advanced users register **custom derivation stages** — read the in-memory sector model after a chosen pipeline point and attach their own derived fields, tags, or summaries — without forking the tool.

### Why it fits

The OVERVIEW's whole "Customisation surface" section is about user control via data files, but every customisation today is *declarative weighting*. Power users will eventually want *logic* the TOML/CSV surface cannot express ("tag every world within 2 hops of a Forge World as 'Mechanicus client state'"). A sandboxed, deterministic scripting hook is the escape valve that keeps such users inside the tool and inside the reproducibility model, instead of post-processing JSON in an external script that escapes the manifest's audit chain.

### How it works (determinism-safe)

Scripts run as additional *derivation-only* stages (no RNG unless they request a seeded stream derived from `blake3("sectorforge:{seed}:script:{script_id}")`, mirroring the built-in stage scheme). The engine is deterministic, sandboxed (no I/O, no clock, no network), and each registered script's source file is digested into the manifest exactly like data files — so a sector produced with scripts is *still* fully auditable and reproducible: same seed + same inputs + same scripts + same version ⇒ same bytes.

### Data / config surface

- A `scripts/` directory; `sectorforge.toml` `[scripts]` lists enabled scripts and their insertion point in the pipeline (post-factions, post-routes, post-everything).

### Surfacing

- CLI: scripts run automatically as part of `generate`; a `--no-scripts` flag for debugging.
- GUI: a read-only "Scripts" panel showing which derived fields came from which script (provenance), and surfacing script errors in the existing warnings panel.

### Edge cases

This is the highest-risk proposal for the determinism contract — the engine *must* forbid all nondeterministic primitives (system time, OS randomness, environment, filesystem, iteration over unordered collections) at the sandbox boundary, not by convention. Script errors must fail generation loudly (consistent with "invalid output should not be possible"), never produce a partial sector. Scripts must be *derivation-only* by API design — they can add fields, not mutate core structural entities — to keep invariants and the validation pass meaningful.

### Effort / risk

High effort and the highest determinism risk of any proposal here (mitigable but only with a strict sandbox). Very high power-user value and a strong differentiator; best sequenced last, after the safer modelling and tooling features have hardened the pipeline.

---

## Suggested sequencing

Not part of any single feature, but a recommended order that front-loads value and de-risks the rest:

1. **Analytics dashboard (§8)** and **Scenario presets (§9)** — low effort, zero determinism risk, and they make every later feature easier to evaluate and demo.
2. **Constraint-directed seed search (§2)** and **Sector diff (§10)** — turn the existing iterate/lock and time-evolve workflows from manual into measurable.
3. **History (§1)**, **Plot hooks (§7)**, **Dramatis personae (§3)**, **Prose (§6)** — the narrative core; together they convert the structural model into runnable campaign material and are the headline value for the GM/writer personas.
4. **Diplomacy (§4)** and **Regional warp phenomena (§5)** — deeper modelling that makes the political picture self-explanatory and unmistakably 40k.
5. **Interactive HTML (§11)**, **Economy (§12)**, **Export adapters (§13)** — distribution and texture.
6. **Multi-sector composition (§14)** and **Scripting hooks (§15)** — the ambitious, higher-risk scale and extensibility tier, attempted only once the pipeline and its determinism guarantees are battle-hardened.

Every proposal above is designed to leave the core contract intact: same seed, same inputs, same version — same bytes.
