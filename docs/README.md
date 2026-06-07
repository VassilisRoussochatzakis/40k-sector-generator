# docs/ index

Reference and process documents for SectorForge. Spec/requirement files are
referenced from code and commits by `§<tag>` rather than by copying their
content. See [CLAUDE.md](../CLAUDE.md) for how these fit the workflow.

## Durable specs / references

Long-lived design specs, requirements, and reference material — kept current
as the code evolves.

| Doc | What it is |
|---|---|
| [MAP.md](MAP.md) | File-by-file map of the workspace source tree; externalized from CLAUDE.md so the top-level doc stays small. |
| [BUILDER_REQS.txt](BUILDER_REQS.txt) | Requirements for the full GUI sector builder (1:1 parity with the CLI/generator); target crate `sectorforge-builder`. |
| [GUIBUILDER.txt](GUIBUILDER.txt) | Specification for the interactive sector builder GUI — the move from scaffold-then-generate to a real-time design environment. |
| [CONTEXT_MENU.txt](CONTEXT_MENU.txt) | Build guide for the builder's right-click context menus on the sector hex map and per-system view. |
| [IMPROVEMENT.txt](IMPROVEMENT.txt) | Codebase improvement plan: whole-repo audit against REFACTOR.txt with cross-checked OPTIMIZE.txt items. |
| [OPTIMIZE.txt](OPTIMIZE.txt) | Catalogue of suggested correctness/performance changes with severity, evidence, and risk. |
| [REFACTOR.txt](REFACTOR.txt) | Reusable LLM prompt for reviewing, planning, and executing a careful refactor of a large Rust application. |
| [ADDING_A_WORLD_TYPE.md](ADDING_A_WORLD_TYPE.md) | Checklist for adding a new `WorldType` variant and the keyed lookups that depend on it. |
| [FRIENDLY_PANEL_PASS.md](FRIENDLY_PANEL_PASS.md) | Reusable recipe for making a builder panel friendly/self-explanatory without changing its data model. |
| [BEAUTY.md](BEAUTY.md) | Self-contained brief for a future session asked to beautify the GUI to showcase quality. |
| [UI_OVERHAUL.md](UI_OVERHAUL.md) | Execution playbook for the GUI overhaul: current state with `path:line` evidence, target design system, step-by-step instructions (`§UO<n>`). |

## Point-in-time / process docs

Snapshots of an audit, review, or work pass at a specific date. Useful as
history, but not kept up to date with later code.

| Doc | What it is |
|---|---|
| [IMPROVEMENT_REVIEW.md](IMPROVEMENT_REVIEW.md) | Agentic full-workspace code-quality review (2026-06-05); findings deduped into themes with per-area tables. |
| [TEST_GAPS.md](TEST_GAPS.md) | Test-gap audit (2026-06-07) from a 28-agent workflow; 88 confirmed zero-coverage gaps plus two real bugs found as a side effect. |
