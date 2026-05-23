# NEW — Proposed Features for `sectorforge`

This document proposes new features for `sectorforge`, written as a companion to `OVERVIEW.md`. Each section is one self-contained proposal: what it is, why it fits the existing design, how it could work without breaking the reproducibility contract, what data/config surface it adds, how it surfaces in the CLI and GUI, the notable edge cases, and a rough effort/risk read.

The guiding constraint throughout: **every proposal must remain byte-deterministic** (same seed + same inputs + same version ⇒ same output), **offline**, **data-driven**, and **setting-agnostic at the data layer**, because those are the properties that make the current tool trustworthy. Where a feature would naturally want randomness, it derives its RNG from the existing `blake3("sectorforge:{seed}:{stage}:{discriminator}")` scheme as a new, isolated stage so it cannot perturb the output of older stages.

Completed proposals (§1–§11, §12, §14) have been moved to `old/DONE.md`. Section numbering below is preserved from the original document for stable cross-referencing.

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

## 15. Scripting / plugin hooks for custom generation stages

### What it is

A sandboxed, embedded scripting interface (e.g. a Rhai/Lua-style engine compiled into the binary, fully offline) that lets advanced users register **custom derivation stages** — read the in-memory sector model after a chosen pipeline point and attach their own derived fields, tags, or summaries — without forking the tool.

### Why it fits

The OVERVIEW's whole "Customisation surface" section is about user control via data files, but every customisation today is *declarative weighting*. Power users will eventually want *logic* the TOML surface cannot express ("tag every world within 2 hops of a Forge World as 'Mechanicus client state'"). A sandboxed, deterministic scripting hook is the escape valve that keeps such users inside the tool and inside the reproducibility model, instead of post-processing JSON in an external script that escapes the manifest's audit chain.

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

1. **Export adapters (§13)** — distribution and ecosystem reach over the now-mature model.
2. **Scripting hooks (§15)** — the ambitious, higher-risk extensibility tier, attempted only once the pipeline and its determinism guarantees are battle-hardened.

Every proposal above is designed to leave the core contract intact: same seed, same inputs, same version — same bytes.
