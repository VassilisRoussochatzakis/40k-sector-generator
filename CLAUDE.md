# CLAUDE.md

## Working style: delegate first, delegate broadly

**The default unit of work is a subagent, not the main thread.** Before doing anything yourself, ask: *can a subagent do this?* If yes, dispatch it. The main thread's context is the scarcest resource in this repo — every grep, every test log, every file you read in the main thread crowds out the reasoning you actually need for the change under review. Push that consumption into subagents whose context is disposable.

This is not a stylistic preference; it is how work here is expected to be done. A task that touches more than one file, spans more than one crate, or requires reading more than a couple of files to understand should almost always be broken into subagent units. Treat "I'll just do it inline" as the exception that needs justifying, not the norm.

Three principles govern *how* you use them:

1. **Decompose aggressively.** Split a task into the largest set of genuinely independent pieces and dispatch them in parallel. Independent exploration, independent mechanical edits, independent research questions — all of these should fan out, not run serially in your head.
2. **Be rigorous, not lazy, with each subagent.** A subagent is only as good as its brief. Give it a precise scope, the exact files or symbols in play, the invariants it must respect (see below), and the exact form of answer you want back (`path:line` citations, a diff, a pass/fail). Vague briefs produce vague results and force re-work — which costs more context than doing it right the first time.
3. **Verify, then trust.** Subagents return summaries, not their full evidence trail. For anything load-bearing, have a second agent cross-check the first — confirm call sites are exhaustive, confirm a refactor compiles, confirm tests actually ran. Thoroughness means *closing the loop*, not assuming the first answer was complete.

The detailed routing table, dispatch heuristics, and recipes are in **[Subagent routing](#subagent-routing)** below. Read that section as the operational core of this file.

## Rules

- **Never read, modify, or reference anything in `old/`.**
- Obey all instructions in [INPUT.md](INPUT.md).
- When making non-trivial changes, update [GUIDE.md](GUIDE.md).
- Spec/requirement files live in [docs/](docs/) — `BUILDER_REQS.txt`, `IMPROVEMENT.txt`, `OPTIMIZE.txt`, `REFACTOR.txt`, `GUIBUILDER.txt`. Reference these by `§<tag>` rather than copying their content into commits or comments. **Don't read these in the main thread to answer a lookup — dispatch a `rust-explorer` or `Explore` agent to pull the relevant `§<tag>` and report back.**

## Determinism invariants (do not violate)

These invariants are non-negotiable and must be passed verbatim into the brief of any subagent whose work touches the relevant area. A subagent that doesn't know about them will cheerfully break them.

- **Never iterate `FxMap`/`FxHashMap`/`FxSet`/`FxHashSet` for output.** Use `BTreeMap`/`BTreeSet`, or sort keys explicitly before emission. The Fx aliases in [src/lib.rs](src/lib.rs) are for internal lookup only.
- **All RNG draws go through [src/model/rng.rs](src/model/rng.rs)** (stage-keyed via `blake3`). Do not introduce `rand::thread_rng()` or seed from anything outside the stage RNG.
- **Output writers must be byte-stable.** After any change to rendering (`bitmap`, `svg_export`, `html_export`, `render`), run the golden tests — dispatch the `test-runner` agent for this rather than running it inline:
  ```bash
  cargo test --test it -- golden
  ```
- **Mutations in the builder always go through the command bus.** Call `state.run(BuilderCommand::...)`. Never write directly to `BuilderState` fields from inside a panel — that breaks undo/redo (§R4). _Carve-out:_ **transient, non-undoable UI state** is exempt and may be written directly — selection (`selected_*_id`), drag/rect-select scratch, scroll/context-menu/modal fields, nav-rail collapse, etc. These are view state, not document state; they never land in `sector.json` and have nothing to undo. Anything that mutates `state.sector` / `state.data_catalogs` / chronicle / presence / claim / roster / relations- or economy-overrides is document state and **must** go through a `BuilderCommand`. (`#[cfg(test)]` code may bypass the bus to construct fixtures.)

## Sector geometry invariant (do not violate)

- **Sectors must be square** — `sector_width == sector_height` everywhere. Enforced by the `GEN_SECTOR_NOT_SQUARE` validation rule in [src/validate/validation.rs](src/validate/validation.rs). The builder/viewer grid-dimension fields lock the two equal; the `random` `SectorSize` presets are all `N × N` (`Custom { dim }` carries one side length); every checked-in `sectorforge.toml` (under `presets/` and `examples/`) is square. Do not add a non-square config, a non-square `SectorSize`, or any UI path that lets width and height diverge. (Exception: `tests/it/invariants_proptest.rs` deliberately feeds non-square dims to stress the generation engine via `validate_sector`, which is post-gen and does not run this pre-gen rule.)

## Workspace

| Crate | Path | Purpose |
|---|---|---|
| `sectorforge` (lib + bin) | [src/](src/) | Domain model, generation, analysis, exports, CLI |
| `sectorforge-builder` | [builder/](builder/) | Egui editor — full sector construction (writes) |
| `sectorforge-viewer` | [viewer/](viewer/) | Egui viewer with **limited** in-place editing (map/faction/world edits, `worlds.toml` data editor, save/save-as) — not read-only; full construction lives in the builder |
| `sectorforge-gui-core` | [gui-core/](gui-core/) | Shared egui widgets (`SectorView`, palette, info_panel) |
| integration tests | [tests/it/](tests/it/) | Single-binary integration suite |

Detailed file-by-file map: **[docs/MAP.md](docs/MAP.md)**. **Never load that file into the main thread** — it is large and will burn context you can't get back. Delegate every lookup against it to the `rust-explorer` subagent, which can read it, answer the specific question, and return only `path:line` citations.

**Dependency convention.** Any crate shared by ≥2 members is pinned once in the root `[workspace.dependencies]`; members reference it with `name.workspace = true` (add crate-specific features inline: `name = { workspace = true, features = [...] }`). Don't re-pin a version in a member manifest — bump it in the root block. Lint *levels* live in `[workspace.lints]` (members opt in with `[lints] workspace = true`); the disallowed-type/method *path lists* stay in the per-crate `clippy.toml` (builder/ + viewer/) because that paint-primitive ban is crate-scoped — `gui-core` owns the raw paint primitives, so it deliberately has no such file.

## Commands

Prefer running the slower of these *through* a subagent (`test-runner` for tests, `clippy-fixer` for clippy) so the verbose output never lands in the main thread.

```bash
cargo build                                  # build all targets
cargo test --workspace                       # all tests
cargo test --test it -- golden               # golden output tests (slower)
cargo test --test it segmentum -- --ignored  # full-m42 segmentum composition (slow; gated #[ignore])
cargo fmt --all
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo run --bin sectorforge -- --help        # CLI help
cargo run -p sectorforge-viewer -- --help    # Viewer help
cargo run -p sectorforge-builder -- --help   # Builder help
```

## Subagent routing

Dispatch work to subagents using the Agent tool. **This is the primary way work gets done here — reaching for the main thread to do something a subagent could do is the mistake to avoid.** When in doubt, delegate.

### When to fan out (parallel dispatch)

Be greedy about parallelism. Any time a task contains pieces that don't depend on each other's results, those pieces should run as simultaneous subagents rather than one after another. Concretely:

- **Exploration spanning independent areas** — e.g. "find all callers of `RouteStability` in `src/`, `builder/`, `viewer/` in parallel" → four agents, one per tree, at once.
- **The same mechanical change across many files** — e.g. "add a `#[must_use]` attribute to every `pub fn` in `src/cli/`" → one subagent per file, all at once.
- **A research question with independent sub-questions** — split it into the sub-questions and dispatch one agent each.
- **Default posture:** if you've written a numbered list of steps and steps 2, 3, and 4 don't read step 1's output, they should not be sequential. Fan them out.

Err toward *more* parallel agents with *narrower* scopes. A dozen tightly-scoped agents that each answer one precise question is better than three broad ones that each return a sprawling, half-relevant dump — the narrow briefs are easier to verify and their results compose cleanly.

### When to go sequential

Only serialize when there is a real data dependency:

- Step B genuinely needs step A's result.
- The change crosses a workspace-member boundary that uses `pub use` from `sectorforge` (downstream crates may break, so you must apply upstream and re-check before touching downstream).

Sequential does **not** mean "in the main thread." A dependent chain is still a chain of subagents — you're just feeding one's output into the next's brief.

### Background dispatch

(`Ctrl+B` or "run this in the background") when the output isn't blocking your next move — `cargo test --workspace`, full clippy sweeps, large greps. Kick these off early so they're done by the time you need them.

### Briefing a subagent rigorously

A subagent only sees what you tell it. Every dispatch should include:

- **Exact scope** — the files, directories, or symbols in play, and explicit out-of-bounds areas (e.g. "do not touch `old/`").
- **The invariants in force** — copy the relevant determinism / geometry / command-bus rule into the brief. Don't assume the agent inherited this file's context.
- **The required output shape** — `path:line` citations, a unified diff, a pass/fail with the failing test names, etc. Pinning the format is what makes results composable across agents.
- **A definition of done** — what "complete and correct" means, so the agent doesn't stop at "probably fine."

### Verify the loop is closed

Subagents return a summary, not their evidence. For anything that matters:

- **Cross-check exhaustiveness.** After a "find all call sites" agent returns, a second agent (or a different search strategy) should confirm nothing was missed before you act on the list.
- **Confirm it actually built/ran.** "I made the change" is not "the change compiles." Follow edits with a `cargo check --workspace` via an agent, and tests via `test-runner`.
- **Don't accept a refactor on faith.** If a subagent reports a non-trivial change succeeded, verify the claim against `cargo check` / golden tests before moving on.

Thoroughness here is the whole point: the reason to delegate is to be able to afford *more* verification, not less.

### Available custom subagents

Defined under `.claude/agents/`. Use these by name in a request, e.g. *"use the `rust-explorer` agent to find every call site of `apply_briefing`"*.

| Name | Use for |
|---|---|
| `rust-explorer` | Read-only codebase navigation. Returns `path:line` citations, never full files. Haiku — fast and cheap, so dispatch it liberally and in parallel. |
| `panel-implementer` | Anything under `builder/src/builder/panels/`. Knows the `BuilderState` + `BuilderCommand` + derivations pattern. |
| `cli-implementer` | Anything under `src/cli/`. Knows the clap `Command` enum + per-runner pattern. |
| `test-runner` | Runs `cargo test` (filtered or full) and reports failures concisely. Doesn't fix anything. |
| `clippy-fixer` | Works through clippy warnings one lint category per pass. Never `#[allow]`s without permission. |

The built-in subagents `Explore` (read-only, Haiku, skips this file) and `Plan` (read-only, used in plan mode) are also available — `Explore` is the right choice for fast first-pass searches that shouldn't load this CLAUDE.md. Because it's cheap and contextless, prefer it for broad reconnaissance and fire off several at once.

### Recipes

**Add a new builder panel.**
1. `rust-explorer`: find the closest existing panel by shape — list-with-detail, form, map overlay, etc. (Dispatch a second `rust-explorer` in parallel to enumerate the existing `BuilderTab` variants and `panels/mod.rs` registration sites, so you have the full wiring picture before editing.)
2. `panel-implementer`: create `builder/src/builder/panels/<name>.rs`; add `pub mod <name>;` to `panels/mod.rs`; add the `BuilderTab` variant; wire `panels::<name>::show` in `app.rs`.
3. `panel-implementer`: add any new `BuilderCommand` variants in `builder/src/builder/command.rs`; if new derived data is needed, extend `builder/src/builder/state/derivations.rs`.
4. `test-runner`: `cargo test -p sectorforge-builder` — and verify the run actually executed the new code path, not just that the suite was green before.

**Add a new CLI subcommand.**
1. `rust-explorer`: read 2–3 existing runners similar in shape (e.g. `src/cli/analyze.rs`) — one agent can survey several in a single pass.
2. `cli-implementer`: create `src/cli/<name>.rs`; add the `Command::<Name>` variant in `src/cli/mod.rs`; wire dispatch.
3. `test-runner`: `cargo test --test it -- cli`.

**Bulk find/replace across the workspace.**
Dispatch four `rust-explorer` agents in parallel — one each for `src/`, `builder/src/`, `viewer/src/`, `gui-core/src/` — merge their results, then apply the change with a single agent. Follow with a fifth `rust-explorer` (or a `cargo check` via `test-runner`) to confirm no call site was missed.

**Anything that changes a `pub` item in `src/lib.rs` or a re-exported type.**
Sequential, never parallel. Order: identify downstream callers (`rust-explorer`, fanned out across all four trees) → propose the change → apply in `src/` → run `cargo check --workspace` → fix downstream uses → tests. Treat the call-site enumeration as load-bearing and cross-check it before touching anything.

**Run clippy cleanup.**
`clippy-fixer` agent. Confirm the lint category with the user before each pass — don't pick unilaterally.

### What goes in main context vs. a subagent

Subagents have isolated context windows. Their results return as a summary, not the full evidence trail. So:

- **Big read-only investigations** → `rust-explorer` or `Explore`. The grep output stays in their context; only the answer comes back. Default everything in this bucket to a subagent.
- **Verbose test/lint output** → `test-runner` or `clippy-fixer`. Same reasoning — never let a wall of cargo output into the main thread.
- **The actual code change** that needs review and possible iteration → main thread, so the user can see and tweak before committing.

**The one thing to get right: decompose by independence, not by phase.** Parallelism and delegation are for splitting work into pieces that don't depend on each other — independent exploration, the same template applied across many files, independent sub-questions. They are *not* for chopping a single coherent change into a relay of `plan → code → test → review → commit` subagents; context is lost at every handoff, so a single agent carrying one coherent change through to the end produces better, more reviewable output than five agents passing it down a line. Delegate aggressively across independent work; keep each indivisible change whole. That distinction — fan out the independent, never fragment the coherent — is what "rigorous use of subagents" means here.
