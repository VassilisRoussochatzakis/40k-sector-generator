# CLAUDE.md

## Rules

- **Never read, modify, or reference anything in `old/`.**
- Obey all instructions in [INPUT.md](INPUT.md).
- When making non-trivial changes, update [GUIDE.md](GUIDE.md).
- Spec/requirement files live in [docs/](docs/) — `BUILDER_REQS.txt`, `IMPROVEMENT.txt`, `OPTIMIZE.txt`, `REFACTOR.txt`, `GUIBUILDER.txt`. Reference these by `§<tag>` rather than copying their content into commits or comments.

## Determinism invariants (do not violate)

- **Never iterate `FxMap`/`FxHashMap`/`FxSet`/`FxHashSet` for output.** Use `BTreeMap`/`BTreeSet`, or sort keys explicitly before emission. The Fx aliases in [src/lib.rs](src/lib.rs) are for internal lookup only.
- **All RNG draws go through [src/model/rng.rs](src/model/rng.rs)** (stage-keyed via `blake3`). Do not introduce `rand::thread_rng()` or seed from anything outside the stage RNG.
- **Output writers must be byte-stable.** After any change to rendering (`bitmap`, `svg_export`, `html_export`, `render`), run the golden tests:
  ```bash
  cargo test --test it -- golden
  ```
- **Mutations in the builder always go through the command bus.** Call `state.run(BuilderCommand::...)`. Never write directly to `BuilderState` fields from inside a panel — that breaks undo/redo (§R4).

## Workspace

| Crate | Path | Purpose |
|---|---|---|
| `sectorforge` (lib + bin) | [src/](src/) | Domain model, generation, analysis, exports, CLI |
| `sectorforge-builder` | [builder/](builder/) | Egui editor (writes) |
| `sectorforge-viewer` | [viewer/](viewer/) | Egui viewer (read-only) |
| `sectorforge-gui-core` | [gui-core/](gui-core/) | Shared egui widgets (`SectorView`, palette, info_panel) |
| integration tests | [tests/it/](tests/it/) | Single-binary integration suite |

Detailed file-by-file map: **[docs/MAP.md](docs/MAP.md)**. Don't load that file unless a task actually needs it — delegate the lookup to the `rust-explorer` subagent instead.

## Commands

```bash
cargo build                                  # build all targets
cargo test --workspace                       # all tests
cargo test --test it -- golden               # golden output tests (slower)
cargo test --test segmentum_tests -- --ignored  # full-m42 segmentum composition (slow; gated #[ignore])
cargo fmt --all
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo run --bin sectorforge -- --help        # CLI help
cargo run -p sectorforge-viewer -- --help    # Viewer help
cargo run -p sectorforge-builder -- --help   # Builder help
```

## Subagent routing

You can dispatch work to subagents using the Agent tool. Prefer this over doing everything in the main thread.

**Parallel dispatch** (multiple subagents at once) when:
- Exploration spans independent areas — e.g. "find all callers of `RouteStability` in `src/`, `builder/`, `viewer/` in parallel".
- The same mechanical change applies to many files — e.g. "add a `#[must_use]` attribute to every `pub fn` in `src/cli/`, one subagent per file".
- A research question has independent sub-questions that don't depend on each other's results.

**Sequential dispatch** when:
- Step B depends on the result of step A.
- The change crosses a workspace-member boundary that uses `pub use` from `sectorforge` (downstream crates may break).

**Background dispatch** (`Ctrl+B` or "run this in the background") when:
- The output isn't blocking the next thing you'll do — e.g. `cargo test --workspace`, full clippy sweeps, large greps.

**Stay in the main thread** when:
- The change is small and contained.
- The user is likely to want iteration after each step.

### Available custom subagents

Defined under `.claude/agents/`. Use these by name in a request, e.g. *"use the `rust-explorer` agent to find every call site of `apply_briefing`"*.

| Name | Use for |
|---|---|
| `rust-explorer` | Read-only codebase navigation. Returns `path:line` citations, never full files. Haiku — fast and cheap. |
| `panel-implementer` | Anything under `builder/src/builder/panels/`. Knows the `BuilderState` + `BuilderCommand` + derivations pattern. |
| `cli-implementer` | Anything under `src/cli/`. Knows the clap `Command` enum + per-runner pattern. |
| `test-runner` | Runs `cargo test` (filtered or full) and reports failures concisely. Doesn't fix anything. |
| `clippy-fixer` | Works through clippy warnings one lint category per pass. Never `#[allow]`s without permission. |

The built-in subagents `Explore` (read-only, Haiku, skips this file) and `Plan` (read-only, used in plan mode) are also available — `Explore` is the right choice for fast first-pass searches that shouldn't load this CLAUDE.md.

### Recipes

**Add a new builder panel.**
1. `rust-explorer`: find the closest existing panel by shape — list-with-detail, form, map overlay, etc.
2. `panel-implementer`: create `builder/src/builder/panels/<name>.rs`; add `pub mod <name>;` to `panels/mod.rs`; add the `BuilderTab` variant; wire `panels::<name>::show` in `app.rs`.
3. `panel-implementer`: add any new `BuilderCommand` variants in `builder/src/builder/command.rs`; if new derived data is needed, extend `builder/src/builder/state/derivations.rs`.
4. `test-runner`: `cargo test -p sectorforge-builder`.

**Add a new CLI subcommand.**
1. `rust-explorer`: read 2–3 existing runners similar in shape (e.g. `src/cli/analyze.rs`).
2. `cli-implementer`: create `src/cli/<name>.rs`; add the `Command::<Name>` variant in `src/cli/mod.rs`; wire dispatch.
3. `test-runner`: `cargo test --test it -- cli`.

**Bulk find/replace across the workspace.**
Dispatch four `rust-explorer` agents in parallel — one each for `src/`, `builder/src/`, `viewer/src/`, `gui-core/src/` — then merge results and apply the change with a single agent.

**Anything that changes a `pub` item in `src/lib.rs` or a re-exported type.**
Sequential, never parallel. Order: identify downstream callers (`rust-explorer`) → propose the change → apply in `src/` → run `cargo check --workspace` → fix downstream uses → tests.

**Run clippy cleanup.**
`clippy-fixer` agent. Confirm the lint category with the user before each pass — don't pick unilaterally.

### What goes in main context vs. a subagent

Subagents have isolated context windows. Their results return as a summary, not as the full evidence trail. So:

- **Big read-only investigations** → `rust-explorer` or `Explore`. The grep output stays in their context; only the answer comes back.
- **Verbose test/lint output** → `test-runner` or `clippy-fixer`. Same reasoning.
- **The actual code change** that needs review and possible iteration → main thread, so the user can see and tweak before committing.

A common mistake is to spawn `plan → code → test → review → commit` as five subagents. Don't. Context is lost between handoffs; a single agent doing all five in sequence is usually faster and produces better output. Use parallelism for **exploration** and for **applying the same template to many files**, not for splitting a single coherent change into phases.
