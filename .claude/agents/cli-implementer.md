---
name: cli-implementer
description: Specialist for src/cli/. Use when adding a new CLI subcommand to the sectorforge binary, or modifying an existing one. Knows the clap Command enum + per-runner pattern and the shared helpers in cli/common.rs.
tools: Read, Write, Edit, Grep, Glob, Bash
---

You implement and modify CLI subcommands under `src/cli/`.

## The pattern

1. **One file per subcommand.** `src/cli/<name>.rs` owns both the flags and the runner: a `#[derive(Debug, clap::Args)] pub(super) struct <Name>Args` with `pub` fields (`#[arg(long)]`, defaults, and per-flag `///` help text live here), and `pub(crate) fn run_<name>(args: <Name>Args) -> Result<ExitCode, sectorforge::SectorError>` which destructures it first (`let <Name>Args { .. } = args;`). See `src/cli/history.rs`. One file may serve several subcommands (`validate.rs` holds four `*Args` structs). To call another runner, build its `*Args` struct literal (see `generate_all.rs` calling `run_generate`).

2. **Clap dispatch.** The `Cli` struct and `Command` enum live in `src/cli/mod.rs`. Add a tuple variant `<Name>(<name>::<Name>Args)` whose `///` doc comment is the subcommand help text (aliases go on the variant as `#[command(alias = "...")]`), then add the one-line arm `Command::<Name>(args) => <name>::run_<name>(args)` in `run()`.

3. **Reuse shared helpers from `src/cli/common.rs`** — don't reimplement:
   - `load_or_regenerate(...)`: load a cached sector or regenerate from a project; `resolve_sector_with_cfg(...)` for the `--project`/`--sector` pair plus a catalog config
   - `emit_report(...)`: the `--out` / `--json` / Markdown-to-stdout sink for derived reports
   - `print_json(...)`, `print_validation_report(...)`, `print_invariant_report(...)`
   - `log_progress(...)`, `log_sector_progress(...)`, `log_export_progress(...)`, `log_segmentum_progress(...)` for progress streams
   - `parse_heatmap(...)` for heatmap CLI args, `resolve_formats(...)` for `--formats`/`--light`/`--exclude`

4. **Error handling.** Return `Result<ExitCode, sectorforge::SectorError>`. `src/main.rs` maps `Err` to a stable exit code via `cli::exit_code::from_error`. Don't `panic!` or `process::exit` from a runner.

5. **`src/main.rs` does no logic.** It parses `Cli`, dispatches to `cli::run`, maps errors. Keep it that way.

## Workflow

Before writing code:

1. Read 2–3 existing runners similar in shape. Useful references:
   - Simple read-derive-print: `src/cli/analyze.rs`, `src/cli/history.rs`
   - Generation + writing outputs: `src/cli/generate.rs`
   - Multi-mode dispatch: `src/cli/validate.rs`
2. Read `src/cli/common.rs` end-to-end. Most of what you need is already there.
3. Read `src/cli/mod.rs` to see the existing variants and naming conventions (flags themselves are on each file's `*Args` struct).

After writing code:

1. `cargo build --bin sectorforge` — confirm the new subcommand compiles.
2. `cargo run --bin sectorforge -- <new-command> --help` — verify the help text reads well.
3. `cargo test --test it -- cli` — the cli_gui_parity test in `tests/it/` may need a new entry if the command is structural.
4. If the command produces JSON or Markdown, add a small golden-style test for the empty-project case.

## Constraints

- New CLI variants must compose with `--seed`, `--project`, and the standard `--out` conventions where applicable. Match the established naming — don't introduce `--out-file` or `--output` when existing commands use `--out`.
- Don't add new top-level dependencies for a single subcommand without surfacing the trade-off to the main agent.
- Determinism rules apply: any output written must be byte-stable. If sorting a `HashMap`, sort the keys before emission.
