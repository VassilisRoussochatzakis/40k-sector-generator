# CLAUDE.md

## Working style: delegate first

**The default unit of work is a subagent, not the main thread.** Main-thread context is the scarcest resource here — every grep, test log, and file read in it crowds out the reasoning for the change under review. Push that consumption into subagents whose context is disposable. Anything touching more than one file or crate, or needing more than a couple of files read to understand, should be subagent work; "I'll just do it inline" is the exception that needs justifying.

Three principles:

1. **Decompose by independence, not by phase.** Split a task into the largest set of genuinely independent pieces and dispatch them in parallel. Do *not* relay one coherent change through a `plan → code → test → review` chain of subagents — context dies at each handoff; one agent carries one change end to end. Fan out the independent; keep the coherent whole.
2. **Brief rigorously.** A subagent is only as good as its brief — see the checklist under [Subagent routing](#subagent-routing). Vague briefs force re-work, which costs more context than doing it right.
3. **Verify, then trust.** Subagents return summaries, not their evidence. Cross-check anything load-bearing with a second agent — call sites exhaustive, refactor compiles, tests actually ran.

Keep in the main thread only the actual code change under review, so the user can see and tweak it before committing.

## Rules

- **Never read, modify, or reference anything in `old/`.**
- Obey all instructions in [INPUT.md](INPUT.md).
- When making non-trivial changes, update [GUIDE.md](GUIDE.md).
- Spec/requirement files live in [docs/](docs/) — `BUILDER_REQS.txt`, `IMPROVEMENT.txt`, `OPTIMIZE.txt`, `REFACTOR.txt`, `GUIBUILDER.txt`. Reference these by `§<tag>`, don't copy their content into commits or comments. **Don't read them in the main thread for a lookup — dispatch a `rust-explorer` or `Explore` agent to pull the relevant `§<tag>`.**

## Determinism invariants (do not violate)

Pass these verbatim into the brief of any subagent whose work touches the relevant area — they won't inherit them otherwise.

- **Never iterate `FxMap`/`FxHashMap`/`FxSet`/`FxHashSet` for output.** Use `BTreeMap`/`BTreeSet`, or sort keys explicitly before emission. The Fx aliases in [src/lib.rs](src/lib.rs) are for internal lookup only.
- **All RNG draws go through [src/model/rng.rs](src/model/rng.rs)** (stage-keyed via `blake3`). Do not introduce `rand::thread_rng()` or seed from anything outside the stage RNG.
- **Output writers must be byte-stable.** After any change to rendering (`bitmap`, `svg_export`, `html_export`, `render`), run the golden tests via the `test-runner` agent: `cargo test --test it -- golden`.
- **Mutations in the builder always go through the command bus.** Call `state.run(BuilderCommand::...)`. Never write directly to `BuilderState` fields from inside a panel — that breaks undo/redo (§R4). _Carve-out:_ **transient, non-undoable UI state** is exempt and may be written directly — selection (`selected_*_id`), drag/rect-select scratch, scroll/context-menu/modal fields, nav-rail collapse, etc. These are view state, not document state; they never land in `sector.json` and have nothing to undo. Anything that mutates `state.sector` / `state.data_catalogs` / chronicle / presence / claim / roster / relations- or economy-overrides is document state and **must** go through a `BuilderCommand`. (`#[cfg(test)]` code may bypass the bus to construct fixtures.)

## Sector geometry invariant (do not violate)

- **Sectors must be square** — `sector_width == sector_height` everywhere; enforced by `GEN_SECTOR_NOT_SQUARE` in [src/validate/validation.rs](src/validate/validation.rs). Grid-dimension fields lock the two equal, the `random` `SectorSize` presets are all `N×N` (`Custom { dim }` carries one side), and every checked-in `sectorforge.toml` is square. Don't add a non-square config, `SectorSize`, or any UI path that lets width and height diverge. (Exception: `tests/it/invariants_proptest.rs` feeds non-square dims to stress generation via the post-gen `validate_sector`, which doesn't run this pre-gen rule.)

## Workspace

| Crate | Path | Purpose |
|---|---|---|
| `sectorforge` (lib + bin) | [src/](src/) | Domain model, generation, analysis, exports, CLI |
| `sectorforge-builder` | [builder/](builder/) | Egui editor — full sector construction (writes) |
| `sectorforge-viewer` | [viewer/](viewer/) | Egui viewer with **limited** in-place editing (map/faction/world edits, `worlds.toml` data editor, save/save-as) — not read-only; full construction lives in the builder |
| `sectorforge-gui-core` | [gui-core/](gui-core/) | Shared egui widgets (`SectorView`, palette, info_panel) |
| integration tests | [tests/it/](tests/it/) | Single-binary integration suite |

File-by-file map: **[docs/MAP.md](docs/MAP.md)** — large; **never load it into the main thread.** Delegate lookups to `rust-explorer`, which returns only `path:line` citations.

**Dependency convention.** Any crate shared by ≥2 members is pinned once in the root `[workspace.dependencies]`; members reference it with `name.workspace = true` (crate-specific features inline: `name = { workspace = true, features = [...] }`). Don't re-pin a version in a member manifest. Lint *levels* live in `[workspace.lints]` (members opt in with `[lints] workspace = true`); the disallowed-type/method *path lists* stay in per-crate `clippy.toml` (builder/ + viewer/) — that paint-primitive ban is crate-scoped, and `gui-core` owns the raw paint primitives so it deliberately has no such file.

## Commands

Run the slower ones *through* a subagent (`test-runner` for tests, `clippy-fixer` for clippy) so verbose output never lands in the main thread.

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

Dispatch via the Agent tool. Reaching for the main thread to do what a subagent could is the mistake to avoid.

**Fan out** whenever pieces don't depend on each other's results — independent exploration (one agent per tree), the same mechanical edit across many files (one agent per file), independent sub-questions. Prefer many narrow agents over few broad ones; narrow briefs verify cleanly and compose. If steps 2–4 don't read step 1's output, they're parallel, not sequential.

**Go sequential** only on a real data dependency: step B needs A's result, or a change to a `pub`/re-exported item in `sectorforge` may break downstream crates (apply upstream, `cargo check --workspace`, then touch downstream). Sequential still means a chain of subagents, not main-thread work.

**Background** (`Ctrl+B`) long non-blocking jobs — `cargo test --workspace`, full clippy, large greps — so they're done by the time you need them.

**Brief rigorously.** Every dispatch includes: exact scope (files/symbols + explicit out-of-bounds, e.g. "don't touch `old/`"); the invariants in force (copy them in); the output shape (`path:line` / diff / pass-fail with failing test names); a definition of done.

**Verify load-bearing claims.** Cross-check a "find all call sites" result with a second search before acting on it. Follow edits with `cargo check --workspace`; run tests via `test-runner`. "I made the change" ≠ "it compiles."

### Custom subagents (`.claude/agents/`)

| Name | Use for |
|---|---|
| `rust-explorer` | Read-only navigation. Returns `path:line`, never full files. Haiku — fast and cheap; dispatch liberally and in parallel. |
| `panel-implementer` | Anything under `builder/src/builder/panels/`. Knows the `BuilderState` + `BuilderCommand` + derivations pattern. |
| `cli-implementer` | Anything under `src/cli/`. Knows the clap `Command` enum + per-runner pattern. |
| `test-runner` | Runs `cargo test` (filtered or full), reports failures concisely. Doesn't fix anything. |
| `clippy-fixer` | Works through clippy one lint category per pass. Never `#[allow]`s without permission. |

Built-in `Explore` (read-only, Haiku, skips this file) is the right choice for fast, contextless reconnaissance — fire off several at once. `Plan` is for plan mode.

### Recipes

**Add a builder panel.**
1. `rust-explorer` (×2 parallel): find the closest existing panel by shape (list-with-detail / form / map overlay); enumerate `BuilderTab` variants + `panels/mod.rs` registration sites.
2. `panel-implementer`: create `panels/<name>.rs`; add `pub mod <name>;`; add the `BuilderTab` variant; wire `panels::<name>::show` in `app.rs`.
3. `panel-implementer`: add new `BuilderCommand` variants in `command.rs`; extend `state/derivations.rs` if new derived data is needed.
4. `test-runner`: `cargo test -p sectorforge-builder` — confirm the new code path actually ran.

**Add a CLI subcommand.**
1. `rust-explorer`: read 2–3 similar runners (e.g. `src/cli/analyze.rs`) in one pass.
2. `cli-implementer`: create `src/cli/<name>.rs`; add `Command::<Name>` in `src/cli/mod.rs`; wire dispatch.
3. `test-runner`: `cargo test --test it -- cli`.

**Bulk find/replace across the workspace.**
Four `rust-explorer` agents in parallel (`src/`, `builder/src/`, `viewer/src/`, `gui-core/src/`), merge, apply with one agent, then a fifth `rust-explorer` (or `cargo check`) to confirm nothing was missed.

**Change a `pub`/re-exported item in `src/lib.rs`.**
Sequential, never parallel: enumerate downstream callers (`rust-explorer` ×4 trees, cross-checked) → apply in `src/` → `cargo check --workspace` → fix downstream → tests.

**Clippy cleanup.**
`clippy-fixer` agent. Confirm the lint category with the user before each pass.
