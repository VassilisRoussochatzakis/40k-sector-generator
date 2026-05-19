# NEW — Proposed Features for `sectorforge`

This document proposes new features for `sectorforge`, written as a companion to `OVERVIEW.md`. Each section is one self-contained proposal: what it is, why it fits the existing design, how it could work without breaking the reproducibility contract, what data/config surface it adds, how it surfaces in the CLI and GUI, the notable edge cases, and a rough effort/risk read.

The guiding constraint throughout: **every proposal must remain byte-deterministic** (same seed + same inputs + same version ⇒ same output), **offline**, **data-driven**, and **setting-agnostic at the data layer**, because those are the properties that make the current tool trustworthy. Where a feature would naturally want randomness, it derives its RNG from the existing `blake3("sectorforge:{seed}:{stage}:{discriminator}")` scheme as a new, isolated stage so it cannot perturb the output of older stages.

Completed proposals (§1–§10, §12) have been moved to `old/DONE.md`. Section numbering below is preserved from the original document for stable cross-referencing.

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

For the remaining proposals:

1. **Interactive HTML (§11)** and **Export adapters (§13)** — distribution and ecosystem reach over the now-mature model.
2. **Multi-sector composition (§14)** and **Scripting hooks (§15)** — the ambitious, higher-risk scale and extensibility tier, attempted only once the pipeline and its determinism guarantees are battle-hardened.

Every proposal above is designed to leave the core contract intact: same seed, same inputs, same version — same bytes.
