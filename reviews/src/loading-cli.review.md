---
unit_id: U005
crate: sectorforge
paths:
  - src/loading/mod.rs
  - src/loading/config.rs
  - src/loading/input.rs
  - src/loading/presets.rs
  - src/loading/sector_save.rs
  - src/cli/mod.rs
  - src/cli/common.rs
  - src/cli/validate.rs
  - src/cli/generate.rs
  - src/cli/analyze.rs
  - src/cli/presets.rs
  - src/cli/search.rs
  - src/cli/compose.rs
  - src/cli/diff.rs
  - src/cli/briefing.rs
  - src/cli/economy.rs
  - src/cli/history.rs
  - src/cli/hooks.rs
  - src/cli/interestingness.rs
  - src/cli/missions.rs
  - src/cli/personae.rs
  - src/cli/prose.rs
  - src/cli/regions.rs
  - src/cli/relations.rs
  - src/cli/sites.rs
loc_reviewed: 3379
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 4, medium: 9, low: 8, nit: 6 }
top_risks:
  - "rewrite_seed does not escape user-supplied seed → invalid/injected TOML (F-005-001)"
  - "load_project returns SectorError::WorldDataLoad with stringified WorldError → loses typed source (F-005-002)"
  - "No path normalization in read_relative — relative paths can escape project root and digest keys can collide (F-005-003)"
  - "Generate runner mixes Ok(ExitCode::from(1)) with Err(SectorError) → inconsistent exit-code surface (F-005-004)"
---

# Review: src/loading + src/cli

## Summary

The loading/CLI surface is structurally clean: each subcommand has a one-file
runner, errors propagate via `Result<ExitCode, SectorError>`, and the loader
uses `camino::Utf8Path` end-to-end (so §3.8 is largely satisfied). However the
edges are not as careful as the core. The most concrete defects:

1. **Untrusted-input handling** is loose — `rewrite_seed` does not escape the
   user seed (CRITICAL-leaning but unlikely-to-be-malicious → HIGH), TOML
   parses do not enforce a max-size guard, and `read_relative` lets relative
   paths escape the project root.
2. **Error-model drift** — the CLI uniformly returns `Result<_, SectorError>`,
   but several runners (`generate`) early-return `Ok(ExitCode::from(1))` while
   peer runners would have returned `Err(SectorError::…)` for the same kind of
   failure. The CLI exits 1 for some logical failures and 2 for others
   (because `main.rs` only does `ExitCode::from(2)` on `Err`), which is
   surprising.
3. **Determinism contract** — `system_count`, `min_worlds_per_system`,
   `max_worlds_per_system`, `world_feature_count` and `min_world_presence` are
   typed `usize` in the config. `usize` differs across 32/64-bit targets;
   this is a portability risk for byte-stable artefacts (manifests / saves
   round-trip `system_count: usize`).
4. **Almost no inline tests** in `src/loading/` (only `presets::rewrite_seed`
   has any coverage) and **zero proptest** coverage despite the loaders being
   TOML-parser-driven (§3.10).

Nothing reaches CRITICAL — no `unwrap_unchecked`, no `unsafe`, no clearly
reachable panic. But there are HIGH findings worth fixing before the next
release-tagged build.

## Findings

### F-005-001 — [HIGH] [Correctness] `rewrite_seed` does not escape the user-supplied seed
- **Location:** `src/loading/presets.rs:197-219` (called via `scaffold` at
  `src/loading/presets.rs:154-161`; entry point is the `new` subcommand,
  `src/cli/mod.rs:156-170` → `src/cli/presets.rs:7-18`).
- **Category:** Correctness / Injection
- **Confidence:** High
- **Blast radius:** Every `sectorforge new --preset … --seed …` invocation.
- **Problem:** The replacement line is built as `format!("seed = \"{new_seed}\"\n")`.
  If `new_seed` contains a literal `"`, a backslash, or a newline, the
  resulting `sectorforge.toml` is either malformed (parse failure on next
  `generate`) or, worse, silently emits two TOML keys / starts a new section
  the user didn't ask for. The seed is a free-form CLI string so this is
  reachable on realistic input (e.g. a copy-pasted seed containing `"`).
- **Evidence:** Read of the function body.
- **Suggested fix:** Round-trip through the `toml` crate or at minimum reject /
  escape the seed:
  ```rust
  fn quote_toml_basic_string(s: &str) -> String {
      let mut out = String::with_capacity(s.len() + 2);
      out.push('"');
      for ch in s.chars() {
          match ch {
              '"'  => out.push_str("\\\""),
              '\\' => out.push_str("\\\\"),
              '\n' => out.push_str("\\n"),
              '\r' => out.push_str("\\r"),
              '\t' => out.push_str("\\t"),
              c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
              c => out.push(c),
          }
      }
      out.push('"');
      out
  }
  // …then:
  out.push_str(indent);
  out.push_str("seed = ");
  out.push_str(&quote_toml_basic_string(new_seed));
  out.push('\n');
  ```
  Better still, reject seeds containing control chars at the CLI boundary —
  the rest of the system treats the seed as opaque ASCII anyway.
- **Effort:** S
- **Risk of fix:** Low

### F-005-002 — [HIGH] [Error handling] `load_project` collapses `WorldError` into a stringified `WorldDataLoad`
- **Location:** `src/loading/input.rs:83-88`
- **Category:** Error handling / typed source loss
- **Confidence:** High
- **Blast radius:** Every `sectorforge generate` / `validate` / GUI loader.
- **Problem:** `WorldError` is stringified into `SectorError::WorldDataLoad { message }`.
  The `#[source]` chain is broken — the original I/O error is no longer
  available to `anyhow`-style printers or to GUI code that wants to surface
  file + line. This is the only place in the loader that throws away a typed
  source (sibling cases use `SectorError::io(path, e)` which preserves the
  `std::io::Error`).
- **Evidence:** Read of `errors.rs:5-39` (no `#[source]` on `WorldDataLoad`).
- **Suggested fix:** Add a typed source:
  ```rust
  #[error("failed to load world data at {path}")]
  WorldDataLoad {
      path: String,
      #[source]
      source: crate::worlds::WorldError,
  },
  ```
  and adjust the call site to `WorldDataLoad { path: data_dir.to_string(), source: e }`.
- **Effort:** S
- **Risk of fix:** Low (touches one variant + one call site).

### F-005-003 — [HIGH] [Security/Determinism] `read_relative` does not normalise or constrain `rel`
- **Location:** `src/loading/input.rs:228-237`
- **Category:** Path handling / Determinism
- **Confidence:** High
- **Blast radius:** Every input file referenced from `sectorforge.toml`.
- **Problem:** `rel` is joined directly onto `root` (so `../../etc/passwd`
  resolves to whatever the OS allows) AND it is used verbatim as the digest
  key:
  ```rust
  let abs = root.join(rel);
  let text = fs::read_to_string(&abs)…;
  digests.insert(rel.to_string(), blake3_of(&text));
  ```
  Two consequences:
  1. **Determinism leak.** `digests` (which ends up in the manifest, §3
     determinism guarantee) is keyed by the raw user string. Two equivalent
     spellings (`./factions.toml` vs `factions.toml`, or two paths that resolve
     to the same canonical file on a case-insensitive FS) yield different
     manifest entries. Manifest comparison across machines becomes lossy.
  2. **Sandbox escape.** A malicious or careless `sectorforge.toml` can read
     files outside the project root. The CLI is not run as a privileged user,
     but the loader is also called from the GUI builder where the project
     directory may come from a file dialog and the user does not expect
     loading the project to be able to read e.g. `~/.ssh/id_rsa`.
- **Evidence:** Read of `input.rs:228-237` and `presets.rs:155-160`.
- **Suggested fix:**
  ```rust
  fn read_relative(root: &Utf8Path, rel: &str, digests: &mut BTreeMap<…>) -> Result<…> {
      // Reject absolute paths and any `..` component.
      let p = Utf8Path::new(rel);
      if p.is_absolute() || p.components().any(|c| matches!(c, Utf8Component::ParentDir)) {
          return Err(SectorError::InvalidConfig(format!(
              "input path '{rel}' must be a relative path within the project root"
          )));
      }
      // Normalise key for stable digests.
      let normalised: Utf8PathBuf = p.components().filter(|c| !matches!(c, Utf8Component::CurDir)).collect();
      let abs = root.join(&normalised);
      let text = fs::read_to_string(&abs).map_err(|e| SectorError::io(abs.as_str(), e))?;
      digests.insert(normalised.to_string(), blake3_of(&text));
      Ok(text)
  }
  ```
- **Effort:** S
- **Risk of fix:** Low — but **golden tests must be re-baselined** if any
  bundled preset uses a `./` prefix today (none observed in the repo).

### F-005-004 — [HIGH] [Error model] `generate` short-circuits validation/invariant failures as `Ok(ExitCode::from(1))` instead of `Err`
- **Location:** `src/cli/generate.rs:43-48`, `:67-79`, `:108-120`, `:135-145`;
  contrast with `src/cli/validate.rs:23-28` (which uses `Ok(ExitCode::from(1))`
  too — that's fine because the *report* is the success path).
- **Category:** Error model / Exit-code coherence
- **Confidence:** Medium-High
- **Blast radius:** Every CI/script that calls `sectorforge generate`.
- **Problem:** Three different failure surfaces co-exist for the same logical
  failure kind:
  - `Err(SectorError::InvalidConfig(...))` → main exits **2**.
  - Bespoke `eprintln!` + `Ok(ExitCode::from(1))` for "preflight failed", "no
    candidate", "validation failed", "invariants failed" → exits **1**.
  - `Err(SectorError::ValidationFailed { … })` from `validate_project` itself
    (a typed variant exists at `errors.rs:19-23`!) is never used by this
    runner — it raw-prints to stderr instead.
  Result: scripts can't reliably distinguish "bad CLI args" (`2`) from "bad
  project file" (`1` for invariants, `2` for InvalidConfig, ?) and the typed
  `ValidationFailed` is dead code in the CLI path.
- **Evidence:** Read of `main.rs:9-17` and `generate.rs:43-145`.
- **Suggested fix:** Pick one convention and document it. Recommendation:
  - Reserve `ExitCode::from(2)` for "unhandled internal error" only.
  - Use `ExitCode::from(1)` for "ran to completion but produced a failing
    report" (validation/invariants/search-empty/diff-nonzero).
  - Return real `Err(SectorError::ValidationFailed { … })` from runners that
    fail because of project content, and have `main.rs` map them to exit 1:
    ```rust
    fn main() -> ExitCode {
        match cli::run(cli::Cli::parse()) {
            Ok(code) => code,
            Err(SectorError::ValidationFailed { .. } | SectorError::InvalidConfig(_)) => {
                eprintln!("…");
                ExitCode::from(1)
            }
            Err(e) => { eprintln!("error: {e}"); ExitCode::from(2) }
        }
    }
    ```
  At minimum, document the exit-code policy in `src/cli/mod.rs` doc comment.
- **Effort:** M
- **Risk of fix:** Medium (visible behaviour change for scripts).

### F-005-005 — [MEDIUM] [Determinism] `usize` config fields are not portable across 32/64-bit targets
- **Location:** `src/loading/config.rs:101-103` (`system_count`,
  `min_worlds_per_system`, `max_worlds_per_system`), `:107`
  (`world_feature_count`), `:252` (`min_world_presence`).
- **Category:** Determinism / API
- **Confidence:** Medium (no 32-bit CI today, but the project ships a
  byte-stable manifest).
- **Blast radius:** Round-tripping a project across architectures.
- **Problem:** `serde` accepts arbitrarily large TOML integers into `usize`,
  but the *range* differs between targets. A project authored with
  `system_count = 8_000_000_000` parses on a 64-bit host and silently fails
  on a 32-bit host. More importantly, `system_count` appears in the manifest
  (`src/model/sector_model/mod.rs:831`), which is part of the determinism
  guarantee — its JSON representation is the same across architectures, but
  the *validation* surface is not.
- **Evidence:** Read of struct fields + `mutation.rs:68`.
- **Suggested fix:** Pick a `u32` for grid-shaped counts and `u64` only where
  truly needed; validate at parse time:
  ```rust
  pub system_count: u32,
  pub min_worlds_per_system: u8,
  pub max_worlds_per_system: u8,
  ```
  and convert at use site via `usize::try_from`. This also tightens the
  contract — `min_worlds_per_system: 0` is presumably invalid, and a u8
  documents the "should be small" intent.
- **Effort:** M (downstream usize→u32 conversions)
- **Risk of fix:** Medium

### F-005-006 — [MEDIUM] [Robustness] Unbounded TOML file reads
- **Location:** All `fs::read_to_string` calls in `src/loading/input.rs`
  (`:57`, `:97`, `:106`, `:113`, `:123`, `:132`, `:136`, `:145`, `:154`,
  `:163`, `:172`, `:179`, `:186`, `:193`, `:200`, `:234`).
- **Category:** Robustness / DoS surface
- **Confidence:** Medium
- **Blast radius:** GUI launcher loading a user-picked directory.
- **Problem:** `fs::read_to_string` will happily slurp a multi-GB file into
  memory and OOM the process. For the CLI this is a self-inflicted wound;
  for the GUI builder (which opens a project via file dialog) a stray
  symlink to `/dev/zero` or a multi-GB log file at the input path will
  hang/crash the application without diagnostics.
- **Evidence:** No size guard anywhere in `loading/`.
- **Suggested fix:** Add a sanity cap (e.g. 16 MiB per input file — the
  largest legitimate input observed in `presets/` is the worlds workbook at
  ~250 KiB):
  ```rust
  fn read_capped(path: &Utf8Path, cap: u64) -> Result<String, SectorError> {
      let md = fs::metadata(path).map_err(|e| SectorError::io(path.as_str(), e))?;
      if md.len() > cap {
          return Err(SectorError::InvalidConfig(format!(
              "{path}: file too large ({} bytes; max {cap})", md.len()
          )));
      }
      fs::read_to_string(path).map_err(|e| SectorError::io(path.as_str(), e))
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-005-007 — [MEDIUM] [Error handling] Silent `.ok()` drops the read failure for the constraints digest
- **Location:** `src/cli/generate.rs:60-66`
- **Category:** Error handling / Silent failure
- **Confidence:** High
- **Blast radius:** Manifests where the constraints file was deleted/moved
  between `load_wishes` and the digest step.
- **Problem:**
  ```rust
  let constraints_text = std::fs::read_to_string(&c_path).ok();
  if let Some(text) = constraints_text {
      input.config.generation.search_constraints_digest = Some(format!(…));
  }
  ```
  If the read fails, the manifest silently omits the constraints digest —
  the artefact says "we ran search" but no longer records *which*
  constraints were used. That is exactly the determinism evidence the
  manifest exists to record.
- **Evidence:** Direct read.
- **Suggested fix:**
  ```rust
  let text = std::fs::read_to_string(&c_path)
      .map_err(|e| sectorforge::SectorError::io(c_path.as_str(), e))?;
  input.config.generation.search_constraints_digest = Some(format!(
      "blake3:{}",
      sectorforge::rng::hex(blake3::hash(text.as_bytes()).as_bytes())
  ));
  ```
  The file was just read successfully a few lines earlier via
  `sectorforge::search::load_wishes(&c_path)`; either reuse that text (best)
  or fail loudly here.
- **Effort:** S
- **Risk of fix:** Low

### F-005-008 — [MEDIUM] [Robustness] `merge` accepts catalog/save mismatch on `generator_version`
- **Location:** `src/loading/sector_save.rs:106-118`
- **Category:** Robustness / Forward-compat
- **Confidence:** High
- **Blast radius:** Long-running simulations loaded after a generator upgrade.
- **Problem:** `merge()` checks `sector_id` and `seed` but **not**
  `generator_version`. The whole point of the IDs-only save format (per the
  module docs) is to round-trip runtime state across regenerations of a
  catalog that hasn't changed — but the loader has no way to detect that the
  catalog *has* changed because the generator version moved. The result is a
  silently-corrupted state.
- **Evidence:** Module-level doc-comment vs. function body.
- **Suggested fix:** Either add a third check (with a configurable "force"
  override) or fold `generator_version` into the catalog digest the save is
  validated against. A minimal first step:
  ```rust
  if sector.generator_version.as_ref() != save.generator_version.as_str() {
      return Err(MergeError::GeneratorVersionMismatch {
          catalog: sector.generator_version.to_string(),
          save:    save.generator_version,
      });
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-005-009 — [MEDIUM] [Concurrency/IO] `scaffold` is not atomic — partial trees on failure
- **Location:** `src/loading/presets.rs:135-162`
- **Category:** Robustness
- **Confidence:** High
- **Blast radius:** Failed `sectorforge new` invocations.
- **Problem:** The function `create_dir_all(dest)` immediately, then
  `copy_dir_recursive` (which can fail mid-tree), then `rewrite_seed`
  (which can fail mid-write). On any of those failures the destination is
  left half-populated. The "destination must not already exist" check
  becomes useless for retry — the user has to manually `rm -rf` before
  trying again.
- **Evidence:** Read of the function body.
- **Suggested fix:** Stage into a sibling temp dir, then rename atomically:
  ```rust
  use tempfile::Builder;
  let parent = dest.parent().unwrap_or_else(|| Utf8Path::new("."));
  let staging = Builder::new()
      .prefix(".sectorforge-scaffold-")
      .tempdir_in(parent.as_std_path())
      .map_err(|e| SectorError::io(parent.as_str(), e))?;
  // … do all work into `staging.path()` …
  fs::rename(staging.into_path(), dest.as_std_path())
      .map_err(|e| SectorError::io(dest.as_str(), e))?;
  ```
- **Effort:** M
- **Risk of fix:** Low–Medium

### F-005-010 — [MEDIUM] [Concurrency/Determinism] `default_presets_dir` consults the live filesystem and `current_exe`
- **Location:** `src/loading/presets.rs:265-280`
- **Category:** Determinism / Testability
- **Confidence:** High
- **Blast radius:** GUI / tests where CWD and exe path differ.
- **Problem:** The function makes filesystem decisions (`is_dir()`,
  `std::env::current_exe()`) and silently falls back to `./presets`. There
  is no way to pin it for tests or to surface "I couldn't find presets" as
  an error. The CLI side already exposes `--presets-dir` (good) so this
  helper is only used by `scaffold_to_dir`, but that helper is the one
  called from the GUI builder per the comment.
- **Evidence:** Read of function body + `scaffold_to_dir` caller comment.
- **Suggested fix:** Return `Option<Utf8PathBuf>`; let callers decide what
  to do when no presets directory can be found, and let tests inject a
  path via env var (e.g. `SECTORFORGE_PRESETS_DIR`).
- **Effort:** S
- **Risk of fix:** Low (single caller in this unit; GUI caller is U015/U016).

### F-005-011 — [MEDIUM] [Path handling] `InspectWorlds` takes `--data-dir: String`, not `Utf8PathBuf`
- **Location:** `src/cli/mod.rs:131-135`, `src/cli/validate.rs:62-66`.
- **Category:** §3.8 path handling / consistency
- **Confidence:** High
- **Blast radius:** Cosmetic but breaks the project's "all paths through
  `camino`" rule (per CLAUDE.md §3.8 guidance).
- **Problem:** Every other subcommand takes `Utf8PathBuf`; this one alone
  takes `String`, then passes it to `sectorforge::inspect_world_workbook`
  which is itself typed as `&str` (`src/lib.rs:909`). The user gets no
  UTF-8 validation, no `~` handling, and no consistency with help output.
- **Evidence:** Read of the two declarations.
- **Suggested fix:** Change both signatures to `Utf8PathBuf`/`&Utf8Path`.
  This also reduces a `String → &str` round-trip.
- **Effort:** S
- **Risk of fix:** Low (touches one lib function + caller).

### F-005-012 — [MEDIUM] [Ownership] `Utf8PathBuf` arguments taken by `&Utf8PathBuf` reference instead of `&Utf8Path`
- **Location:** `src/cli/validate.rs:11-15` (and 31, 49), `src/cli/regions.rs:9-13`,
  `src/cli/search.rs:9-17`, `src/cli/presets.rs:7-12`,
  `src/cli/compose.rs:9-13`, every other runner that takes
  `&Utf8PathBuf`/`Option<&Utf8PathBuf>`.
- **Category:** §3.3 ownership / API design
- **Confidence:** High
- **Blast radius:** Cosmetic / clippy.
- **Problem:** `&Utf8PathBuf` is the `&String`-equivalent anti-pattern.
  `&Utf8Path` is the correct borrow and is what every downstream function
  actually wants.
- **Evidence:** All runner signatures.
- **Suggested fix:**
  ```rust
  pub fn run_validate(project: &Utf8Path, json: bool, strict: bool) -> Result<…>
  ```
  Adjust call sites in `cli::run` to `project.as_path()` /
  `out.as_deref()`. Single mechanical pass — would be a good `clippy-fixer`
  agent run with the `ptr_arg` lint enabled.
- **Effort:** S
- **Risk of fix:** Low

### F-005-013 — [MEDIUM] [Testing] No proptest / fuzz coverage for the TOML loaders
- **Location:** Whole of `src/loading/`.
- **Category:** §3.10 testing
- **Confidence:** High
- **Blast radius:** Robustness against malformed/adversarial input.
- **Problem:** The brief flags TOML parsers as proptest candidates. The only
  test in `src/loading/` is `presets::rewrite_seed_*` (three asserts).
  `load_project`, `merge`, `OutputFormat::parse_token`, `parse_heatmap`,
  `parse_interestingness_profile`, `rewrite_seed` (against malicious input
  per F-005-001) all merit property tests.
- **Evidence:** Direct read; no `#[cfg(test)]` modules in `config.rs`,
  `input.rs`, or `sector_save.rs` beyond the round-trip smoke test in
  `sector_save.rs:222-238`.
- **Suggested fix:** Add a `proptest!` module covering:
  - `OutputFormat::parse_token` is total (never panics on arbitrary `&str`).
  - `parse_heatmap` ditto.
  - `rewrite_seed` round-trips: for any `text` that contains exactly one
    `[generation]` section with one `seed = "…"`, parsing the output via
    `toml::from_str::<toml::Value>` always succeeds for any `new_seed: String`.
  Optional: cargo-fuzz target `loader_fuzz_target` that feeds bytes to
  `toml::from_str::<AppConfig>` and asserts no panic.
- **Effort:** M
- **Risk of fix:** None (tests only).

### F-005-014 — [LOW] [Performance/Allocation] Per-call `format!` allocs in `blake3_of_bytes`
- **Location:** `src/loading/input.rs:243-246`
- **Category:** §3.6 perf
- **Confidence:** Medium
- **Blast radius:** Startup — once per input file.
- **Problem:** `format!("blake3:{}", …)` allocates a fresh `String` per file.
  Not in a hot loop, so this is a NIT-borderline LOW. Worth flagging only
  because the same pattern appears in
  `src/cli/generate.rs:62-65` *inside* the search constraint path and the
  search loop in `src/analysis/search.rs` may iterate.
- **Evidence:** Read.
- **Suggested fix:** A `digest_to_string(prefix: &str, h: &Hash) -> String`
  helper using `String::with_capacity(prefix.len() + 64 + 1)` and `write!`.
- **Effort:** S
- **Risk of fix:** None

### F-005-015 — [LOW] [Idiomatic] `print_validation_report` / `print_invariant_report` write to stdout
- **Location:** `src/cli/common.rs:22-49`, `:69-81`
- **Category:** §3.7 CLI ergonomics
- **Confidence:** Medium
- **Blast radius:** Pipelines that `… | jq` the JSON output.
- **Problem:** These print to **stdout**, but they are diagnostics, and the
  caller frequently also emits JSON to stdout (e.g. `validate --json`). When
  the user runs without `--json` it's fine; when they pass `--json` we only
  call `print_json`, so this is currently OK. But the `generate` runner mixes
  human progress (`log_progress` → stderr) with calling
  `print_validation_report` (stdout) on failure
  (`src/cli/generate.rs:110`, `:118`) — that's a stdout/stderr split bug:
  failure noise shows up on stdout where machine-readable output is
  expected.
- **Evidence:** Read of `generate.rs:108-120`.
- **Suggested fix:** Route diagnostics through `eprintln!` consistently.
  Either make `print_validation_report` write to `&mut impl Write` or add a
  `print_validation_report_stderr` sibling, and call the stderr variant from
  `generate.rs` failure paths.
- **Effort:** S
- **Risk of fix:** Low (golden text tests pass — they don't grep stdout).

### F-005-016 — [LOW] [Idiomatic] `print_json` `<stdout>` path is meaningless in error message
- **Location:** `src/cli/common.rs:15-20`
- **Category:** §3.7 error reporting
- **Confidence:** High
- **Blast radius:** Error UX only.
- **Problem:** If `serde_json::to_string_pretty` fails (e.g. non-UTF-8 in a
  field — practically impossible here but defensive), the user sees `failed
  to export <stdout>: …`. The error path label is a lie.
- **Suggested fix:** Use a real "json serialisation" variant or just inline
  the message: `SectorError::InvalidConfig(format!("failed to serialise to JSON: {e}"))`.
  Or, since this is a serialise error not a config error, add a
  `SectorError::Serde { message }` variant.
- **Effort:** S
- **Risk of fix:** Low

### F-005-017 — [LOW] [Idiomatic] `OutputFormat::parse_token` is one of three nearly-identical token-table parsers
- **Location:** `src/loading/config.rs:353-362`, `src/cli/common.rs:114-139`
  (`parse_heatmap`), `src/cli/interestingness.rs:33-47`
  (`parse_interestingness_profile`), `src/cli/briefing.rs:18-22`
  (delegates to `sectorforge::briefing::parse_preset` so this one is fine).
- **Category:** §3.7 idiomatic
- **Confidence:** Medium
- **Blast radius:** Maintainability.
- **Problem:** Three identical match-on-lowercase-string patterns, each with
  bespoke error message. `clap`'s `ValueEnum` derive gives this for free.
- **Suggested fix:** Define each parsed CLI choice as
  ```rust
  #[derive(clap::ValueEnum, Clone, Copy)]
  enum HeatmapCli { Off, Control, Military, … }
  ```
  and use it directly in the `#[arg]` attribute; map to the domain type via
  `impl From<HeatmapCli> for HeatmapMode`. Eliminates the three custom
  parsers, gets `--help` to enumerate values, and gets bash completion.
- **Effort:** M
- **Risk of fix:** Low (output schema unchanged).

### F-005-018 — [LOW] [API] `--data-dir` for `inspect-worlds` is a positional-vs-flag inconsistency
- **Location:** `src/cli/mod.rs:131-135`
- **Category:** §3.7 CLI ergonomics
- **Confidence:** Medium
- **Problem:** Every other subcommand that takes a single primary path uses
  `--project <dir>` or `--sector <path>`. `inspect-worlds` uses
  `--data-dir <path>`, which is fine, but it could (and IMO should) accept
  the path positionally given that it's the only required argument:
  `sectorforge inspect-worlds <dir>`.
- **Suggested fix:** Add `#[arg(long)]` → drop in favour of positional:
  ```rust
  InspectWorlds {
      data_dir: Utf8PathBuf,
  },
  ```
- **Effort:** S
- **Risk of fix:** Medium (CLI surface change — bump minor version).

### F-005-019 — [LOW] [Idiomatic] `parse_token` does not surface invalid tokens
- **Location:** `src/loading/config.rs:353-362`
- **Category:** §3.7 / error reporting
- **Confidence:** High
- **Problem:** Returns `Option<Self>`, so the caller has to reconstruct an
  "unknown format X" error. `src/cli/generate.rs:96-100` does so manually.
- **Suggested fix:** Return `Result<Self, SectorError>` so the call site is
  a single `?`. Or make this an `impl FromStr` so clap can wire it directly.
- **Effort:** S
- **Risk of fix:** Low

### F-005-020 — [LOW] [Idiomatic] `merge_name_tables` overwrites instead of merging `location_names`
- **Location:** `src/loading/input.rs:248-268`, esp. line 258
- **Category:** §3.7 idiomatic / surprise
- **Confidence:** Medium
- **Problem:** Every other field is "skip if source-empty", but
  `target.location_names = source.location_names;` unconditionally
  overwrites — including with an empty value. If a project lists both a
  `system_names.toml` and a `world_names.toml` and the latter omits
  `location_names`, the system-names location entries are clobbered.
- **Evidence:** Read of body.
- **Suggested fix:**
  ```rust
  if !source.location_names.is_empty() {
      target.location_names = source.location_names;
  }
  ```
  (Provide an `is_empty()` on the type if it doesn't have one.)
- **Effort:** S
- **Risk of fix:** Low — but **golden tests likely change**; verify on the
  bundled presets.

### F-005-021 — [LOW] [Documentation] Many CLI runners lack `# Errors` / `# Examples`
- **Location:** All of `src/cli/*.rs` (except they're `pub(crate)` so
  rustdoc lint may not fire).
- **Category:** §3.11 documentation
- **Confidence:** Medium
- **Problem:** None of the runners document the exact error cases or the
  exit-code contract; only the subcommand variants in `mod.rs` carry doc
  comments and those are end-user-facing not API-facing. Given F-005-004
  (exit-code drift), a `# Exit codes` section on `pub fn run` would help.
- **Suggested fix:** Add doc comment to `pub fn run(cli: Cli) -> Result<ExitCode, SectorError>`
  in `src/cli/mod.rs:411` listing the exit-code policy.
- **Effort:** S
- **Risk of fix:** None

### F-005-022 — [NIT] [Style] `Utf8PathBuf::from("presets")` literal duplicated in `default_value`
- **Location:** `src/cli/mod.rs:168`, `:173`
- **Category:** §3.11
- **Problem:** `"presets"` is a magic literal in two `default_value` attrs and
  in `default_presets_dir()` at `src/loading/presets.rs:266`. A single
  `pub const DEFAULT_PRESETS_DIR: &str = "presets";` would unify.
- **Suggested fix:** Hoist the constant.
- **Effort:** S
- **Risk of fix:** None

### F-005-023 — [NIT] [Idiomatic] `should_log_progress` calls `usize::is_multiple_of` (1.87 feature)
- **Location:** `src/cli/common.rs:474`
- **Category:** §3.11
- **Problem:** `is_multiple_of` is stable since 1.87 (June 2025). If the
  project pins an MSRV below that, this won't compile. No rust-version is
  declared in workspace Cargo.toml (verify in cross-cut). Not a defect per
  se, just worth noting.
- **Suggested fix:** Confirm MSRV or replace with `current % stride == 0`.
- **Effort:** S
- **Risk of fix:** None

### F-005-024 — [NIT] [Idiomatic] `Cli::run` is a 200-line `match` arm by arm
- **Location:** `src/cli/mod.rs:411-625`
- **Category:** §3.11 maintainability
- **Problem:** Adding a new subcommand requires editing the enum, the doc
  comment, the dispatch arm, and the runner module. The dispatch arm is
  pure data shuffling. A trait + table would shrink it, but the cost is a
  level of indirection for very little gain — the boilerplate is local.
  Flagging only because future U005 changes will keep growing this file.
- **Suggested fix:** Leave as-is unless the subcommand count grows past
  ~30. Alternative: macro that takes the enum variants and emits the match.
- **Effort:** L
- **Risk of fix:** Medium

### F-005-025 — [NIT] [Idiomatic] `print!` vs `println!` inconsistency between Markdown emitters
- **Location:** `src/cli/analyze.rs:41`, `src/cli/diff.rs:51`,
  `src/cli/history.rs:40`, `src/cli/sites.rs:40`, etc.
- **Category:** §3.7
- **Problem:** Some Markdown emitters use `print!("{md}")` assuming
  `render_*_markdown` ends with `\n`. Others use `println!`. Verify the
  trailing-newline contract on the renderers; if it isn't enforced, the
  user sees `% ` continuation prompts in zsh on alternating commands.
- **Suggested fix:** Document the trailing-newline contract on each
  `render_*_markdown` and pick `print!` uniformly.
- **Effort:** S
- **Risk of fix:** None

### F-005-026 — [NIT] [Documentation] `compose` mod uses `unwrap_or_else(|| Utf8PathBuf::from("."))` for parent dir
- **Location:** `src/cli/compose.rs:20-23`
- **Category:** §3.11
- **Problem:** Falls back silently to `"."` if `--segmentum` has no parent
  (i.e. the user passed a bare filename in CWD). That's the right behaviour
  but undocumented; future contributor may "fix" it to error.
- **Suggested fix:** Add a one-line comment.
- **Effort:** S
- **Risk of fix:** None

### F-005-027 — [NIT] [Determinism — CLAUDE.md invariant note] No `FxMap` iterations in this unit
- **Location:** None.
- **Category:** Determinism invariant
- **Problem:** Searched all 25 files; no `FxMap`/`FxHashMap`/`FxSet`/
  `FxHashSet` iterations in `src/loading/` or `src/cli/`. `input_digests` is
  a `BTreeMap`, `SectorSave.systems` is a `BTreeMap`. Good.
- **Suggested fix:** None.

## Per-§ rubric coverage

- **3.1 Panics** — no `unwrap`/`expect` on parsed data observed.
  `tempfile::TempDir::new().expect("tempdir")` at `presets.rs:305` is in a
  test (fine). `unwrap()` at `presets.rs:334-360` is also test-only. The
  `unwrap_or_else(|| Utf8PathBuf::from("."))` at `compose.rs:22` is
  infallible (the literal is valid UTF-8). No reachable panics.
- **3.2 unsafe & soundness** — no `unsafe` blocks. Confirmed.
- **3.3 Ownership/borrows/clones** — F-005-012 (`&Utf8PathBuf`). The
  cloning in `load_project` is unavoidable (you have to clone before parsing
  out the rest of the borrow chain). The `input.config.outputs.clone()` at
  `generate.rs:128` is fine — needed because `input` is consumed by
  `generate_sector_with_progress`.
- **3.4 Error handling** — F-005-002, F-005-004, F-005-007, F-005-016.
- **3.5 Concurrency / async** — none used in this unit. N/A.
- **3.6 Performance** — F-005-014. Loading is a startup cost so allocation
  pressure is not interesting.
- **3.7 Idiomatic / API** — F-005-011, F-005-012, F-005-017, F-005-018,
  F-005-019, F-005-022, F-005-025.
- **3.8 Dependencies / Cargo** — No unused imports observed. Direct deps
  used: `camino`, `serde`, `serde_json`, `toml`, `clap`, `blake3`,
  `tempfile` (test-only), `thiserror`. All justified.
- **3.9 Memory & resources** — F-005-006 (unbounded read), F-005-009 (no
  atomic scaffold). No `Arc`/`Rc` cycles; no growing caches.
- **3.10 Testing** — F-005-013. The `sector_save` round-trip test is good
  but minimal. `presets::rewrite_seed_*` are insufficient against
  F-005-001.
- **3.11 Documentation** — F-005-021, F-005-026.

## Summary of suggested fixes

- F-005-001 — HIGH — Escape user seed in `rewrite_seed` — S/Low
- F-005-002 — HIGH — Add `#[source]` to `WorldDataLoad` — S/Low
- F-005-003 — HIGH — Reject `..` / absolute / normalise key in `read_relative` — S/Low
- F-005-004 — HIGH — Pick one exit-code convention for `generate` failures — M/Medium
- F-005-005 — MEDIUM — Use `u32`/`u8` instead of `usize` for portability — M/Medium
- F-005-006 — MEDIUM — Cap `fs::read_to_string` size in loaders — S/Low
- F-005-007 — MEDIUM — Stop swallowing constraints-file read failure — S/Low
- F-005-008 — MEDIUM — Validate `generator_version` in `SectorSave::merge` — S/Low
- F-005-009 — MEDIUM — Make `scaffold` atomic via temp-then-rename — M/Low-Medium
- F-005-010 — MEDIUM — Make `default_presets_dir` return `Option` — S/Low
- F-005-011 — MEDIUM — `inspect-worlds --data-dir` to `Utf8PathBuf` — S/Low
- F-005-012 — MEDIUM — `&Utf8PathBuf` → `&Utf8Path` in runner signatures — S/Low
- F-005-013 — MEDIUM — Proptest + cargo-fuzz target for TOML loaders — M/None
- F-005-014 — LOW — Share a digest-format helper — S/None
- F-005-015 — LOW — Route diagnostics in `generate` failure paths to stderr — S/Low
- F-005-016 — LOW — Drop "<stdout>" lie from `print_json` error — S/Low
- F-005-017 — LOW — Replace bespoke parsers with clap `ValueEnum` — M/Low
- F-005-018 — LOW — Make `inspect-worlds` accept positional path — S/Medium
- F-005-019 — LOW — `OutputFormat::parse_token` should return `Result` / `FromStr` — S/Low
- F-005-020 — LOW — Don't clobber `location_names` with empty source — S/Low
- F-005-021 — LOW — Document exit-code policy on `cli::run` — S/None
- F-005-022 — NIT — Hoist `"presets"` magic literal — S/None
- F-005-023 — NIT — Confirm MSRV covers `is_multiple_of` — S/None
- F-005-024 — NIT — `Cli::run` 200-line match (acceptable for now) — L/Medium
- F-005-025 — NIT — `print!` vs `println!` Markdown trailing-newline contract — S/None
- F-005-026 — NIT — Comment the `compose` parent fallback — S/None
- F-005-027 — NIT — No `FxMap` iteration in this unit — verified — None
