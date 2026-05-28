---
sweep_id: X03
scope: whole-workspace
reviewed_by: agent
finding_counts: { critical: 0, high: 4, medium: 5, low: 3, nit: 2 }
overall: needs-work
top_themes:
  - "Duplicate `MutationError` enum in src/ (one is dead code, shadows the other)"
  - "Builder/viewer silently swallow rich typed errors at load boundaries (TOML parse, subsector build, generation)"
  - "CLI collapses every variant of `SectorError` to exit code 2 — no validation/IO/usage differentiation"
  - "Stringly-typed error payloads (`ConfigParse { message: String }`, `BuilderError::ParseFailed.message`) drop structured source info"
  - "No `#[non_exhaustive]` on any public error enum"
---

# Cross-cutting Sweep: error-model

## Method

```
grep -rEn "thiserror|pub enum [A-Za-z]*Error|impl Display for|impl Error for" --include='*.rs' -- src builder viewer gui-core
grep -rEn "anyhow|eyre|Box<dyn"                                              # zero hits
grep -rEn "let _ =|\.ok\(\)|\.unwrap_or_default\(\)"                         # 309 / 13 / 89
grep -rEn "if let Ok\(.*\) ="                                                # 36 sites
grep -rEn "#\[from\]|#\[source\]"                                            # coverage check
```

Workspace uses **no** `anyhow`/`eyre`/`Box<dyn Error>`. All error types are
`thiserror`-derived enums. Library + bin (`sectorforge`), builder, and viewer
each define their own `*Error` enum and convert at the crate seam — the
intended layering is sound. The problems are in execution: missing variants,
`message: String` flattening, and silent swallowing of typed errors at load
boundaries in the GUI crates.

## Enumeration of error types (13 total)

| Defn | Crate | Variants | `#[from]`/`#[source]` | `#[non_exhaustive]` | Notes |
|---|---|---|---|---|---|
| `SectorError`            | src/model/errors.rs:5   | 9 | only `Io` has `#[source]` | no | catch-all for the library |
| `MutationError` (model)  | src/model/errors.rs:43  | 4 | none | no | **dead code** — never imported anywhere |
| `MutationError` (sector) | src/model/sector_model/mutation.rs:22 | 10 | none | no | the real one; shadows the dead one |
| `WorldError`             | src/worlds.rs:10        | 2 | `Io` | no | |
| `WorldsTomlError`        | src/worlds_toml.rs:56   | 5 | `Io`, `World` | no | `Parse`/`Emit` are `String` not `#[from]` |
| `MergeError`             | src/loading/sector_save.rs:146 | 2 | none | no | |
| `MapThemeError`          | src/export/map_theme.rs:11 | 4 | none | no | |
| `SubsectorBuildError`    | src/export/subsectors/mod.rs:136 | 5 | none | no | swallowed in 7 GUI sites (see F-X03-003) |
| `BuilderError`           | builder/src/builder/errors.rs:6 | 8 | `IoFailed`, `Mutation`, `Serde` | no | `ParseFailed { file, message }` flattens TOML errors |
| `DataEditorError`        | viewer/src/data_editor.rs:19 | 3 | `Io`, `Toml` | no | |
| `PresetGalleryError`     | viewer/src/preset_gallery.rs:16 | 1 | none | no | single string variant — trivial |
| `FactionDesignerError`   | viewer/src/factions_overview.rs:14 | 3 | `Io` | no | |
| `EditorFileError`        | viewer/src/editor/file_ops.rs:9 | 3 | `Io`, `Json` | no | |

## Findings

### F-X03-001 — [HIGH] [Correctness/Maintainability] Duplicate `MutationError` enum; the one re-exported as `sectorforge::errors::MutationError` is dead code

- **Location:** `src/model/errors.rs:41-52` (the dead one) and `src/model/sector_model/mutation.rs:22-43` (the real one). `src/lib.rs:73` re-exports `model::errors`, so `sectorforge::errors::MutationError` resolves to the dead one.
- **Category:** Error handling / API design
- **Confidence:** High
- **Blast radius:** Public-API confusion. A downstream caller importing `sectorforge::errors::MutationError` (because that's the natural place to find it) will get a 4-variant `Serialize`-able enum that no code ever returns. The 10-variant enum it would actually want is at `sectorforge::sector_model::mutation::MutationError`.
- **Problem:** Two unrelated enums share the name. The one in `model/errors.rs` defines `NotFound`/`Collision`/`InvalidCoord`/`InvalidState`, derives `Serialize`/`Deserialize`, and is **never imported by any file in `src/`, `builder/`, `viewer/`, `gui-core/`, or `tests/`** (verified by grep). The one in `sector_model::mutation` is the one used by every mutation API and by `BuilderError::Mutation(#[from] MutationError)` at `builder/src/builder/errors.rs:20`.
- **Why it matters:** Future contributors will look at `sectorforge::errors::*` (advertised via `pub use model::errors;`) and either use the wrong type or duplicate variants. The dead enum also includes `Serialize`/`Deserialize` derives — if someone later writes a JSON dump that "needs a serializable MutationError" they'll silently get the wrong four cases.
- **Suggested fix:** Delete the dead enum at `src/model/errors.rs:41-52` (and its `serde::{Deserialize, Serialize}` import at line 1 if it was only for that enum). If the serializable shape is actually wanted later, derive `Serialize` on the real `sector_model::mutation::MutationError` instead. Update the module-level doc comment if any.
- **Effort:** S
- **Risk of fix:** Low — verified zero external imports.

### F-X03-002 — [HIGH] [Error handling] `builder/src/builder/project_io.rs::reload_catalog` swallows every TOML parse error at the file-watcher boundary

- **Location:** `builder/src/builder/project_io.rs:832-924` (15 sequential `if let Ok(cfg) = toml::from_str::<...>(text) { ... }` blocks).
- **Category:** Error handling / silent data loss
- **Confidence:** High
- **Blast radius:** Every on-disk edit to a project catalog (factions, regions, route_rules, relations, economy, history, personae, hooks, sites, prose, missions, names, sectorforge.toml, worlds.toml). When a user edits any of those by hand and saves a typo, the file-watcher re-reads the text, the `toml::from_str` returns `Err`, the `if let Ok` arm doesn't fire, and the **in-memory catalog is silently left at the previous good value** — the GUI shows stale state with no indication that the disk file is broken.
- **Problem:** The first path (`open_project`) has rich `BuilderError::ParseFailed { file, message }` reporting and a test (`open_project_surfaces_toml_parse_errors_with_line` at line 1032) that asserts line/col info reaches the user. The reload path lost that property entirely. This is a coherence break: the *load* boundary uses the typed error well; the *reload* boundary throws it away.
- **Why it matters:** Determinism + author trust. Silent revert-to-old-state is exactly the failure mode that drove the §P2 line-number reporting requirement.
- **Suggested fix:** Change `reload_catalog` to `Result<(), BuilderError>` (or push into a `state.modal = Some(ModalKind::Message(...))` like every other panel does on command failure). Each arm becomes:
  ```rust
  if rel == worlds_rel {
      let cfg = sectorforge::worlds_toml::WorldsConfig::from_str(text)
          .map_err(|e| BuilderError::ParseFailed {
              file: rel.to_string(),
              message: e.to_string(),
          })?;
      state.data_catalogs.worlds = Some(cfg);
      return Ok(());
  }
  ```
  The 15 arms are mechanical; a helper `fn parse_or_keep<T: DeserializeOwned>(rel: &str, text: &str) -> Result<T, BuilderError>` would dedupe them.
- **Effort:** M (15 sites, repetitive)
- **Risk of fix:** Low — adds error surface, doesn't remove behavior; existing callers in `panels/conflict_resolver.rs` already handle `BuilderError`-style results.

### F-X03-003 — [HIGH] [Error handling] `build_subsectors` errors are silently discarded in seven GUI/analytics sites via `.unwrap_or_default()` / `let Ok ... else { return }` / `match Err(_) => Vec::new()`

- **Locations:**
  - `viewer/src/app/lifecycle.rs:23` — `build_subsectors(...).unwrap_or_default()`
  - `viewer/src/app/sector_view.rs:667-668` — same
  - `viewer/src/app/mod.rs:193-197` — same
  - `builder/src/builder/panels/subsectors.rs:117-124` — same (`current_subsectors`)
  - `builder/src/builder/panels/subsectors.rs:535-542` — same
  - `builder/src/builder/panels/map/cache.rs:26-33` — same
  - `src/analysis/analytics.rs:517-520` — `match Err(_) => Vec::new()`
  - `src/analysis/history/subsectors.rs:55-60` — `let Ok(subsectors) = ... else { return; }`
- **Category:** Error handling / silent failure
- **Confidence:** High
- **Blast radius:** Every place that depends on a derived subsector partition — the sector map, the subsector panel, the analytics dashboard, the chronicle's per-subsector events. On `SubsectorBuildError::DuplicateSystemId` or `CoordinateOutOfBounds`, the user sees an empty subsector list with no diagnosis.
- **Problem:** `SubsectorBuildError` carries actionable detail (which system, which coord, what dimension mismatch — see `src/export/subsectors/mod.rs:136-154`). All seven call sites throw it away. This is **inconsistent with the rest of `src/`** which propagates typed errors all the way to `SectorError`.
- **Why it matters:** Bad input that should yield "system X at (q,r) is outside sector (W×H)" instead yields a blank map. Determinism is preserved (same garbage every run) but author feedback isn't.
- **Suggested fix:** Either:
  1. **Preferred:** add a `Subsectors(#[from] SubsectorBuildError)` variant to `BuilderError` (and a parallel surface in the viewer — its `app` types have no top-level error enum yet; this is part of the larger viewer-error-surface gap noted in F-X03-006). Push to the status modal on failure.
  2. **Minimum:** at every site, `match build_subsectors(...) { Ok(v) => v, Err(e) => { eprintln!("subsectors: {e}"); Vec::new() } }`. Strictly worse than (1) but recoverable without architectural changes.
- **Effort:** M
- **Risk of fix:** Low — every call site already accepts an empty `Vec` as the fallback shape.

### F-X03-004 — [HIGH] [API design] CLI collapses every error to exit code 2; no distinction between validation/IO/usage/internal errors

- **Location:** `src/main.rs:9-18`.
- **Category:** Error handling / CLI contract
- **Confidence:** High
- **Blast radius:** Every shell/CI consumer. `sectorforge validate --strict` legitimately needs to distinguish "validation reported errors" (exit 1, normal) from "config file unreadable" (exit 2, broken setup) from "internal logic bug" (exit 70). Today they're all `ExitCode::from(2)`.
- **Problem:** `match cli::run(args) { Ok(code) => code, Err(e) => { eprintln!("error: {e}"); ExitCode::from(2) } }`. `SectorError` already encodes the distinction: `ValidationFailed { ... }` should be exit 1, `Io` should be 74 (sysexits `EX_IOERR`), `ConfigParse`/`InvalidConfig` should be 78 (`EX_CONFIG`), `GenerationCancelled` should be 130 (signal-like), everything else 70 (`EX_SOFTWARE`). Subcommands that *do* know the distinction (e.g. `cli/generate.rs:78`) manually `Ok(ExitCode::from(1))` — but anything that bubbles via `?` collapses.
- **Why it matters:** §3.4 rubric "broken public API contract" — the CLI's exit code is a contract. Note the inconsistency: `cli/generate.rs:47`, `:78`; `cli/search.rs` and friends all return `Ok(ExitCode::from(1))` for their domain failure mode, but a `SectorError` from underneath comes out as 2. Three different exit-code policies coexist.
- **Suggested fix:** Add a small mapping function in `src/main.rs`:
  ```rust
  fn exit_for(e: &SectorError) -> ExitCode {
      use SectorError::*;
      match e {
          ValidationFailed { .. } => ExitCode::from(1),
          Io { .. }               => ExitCode::from(74),
          ConfigParse { .. } | InvalidConfig(_) | WorldDataLoad { .. } => ExitCode::from(78),
          GenerationCancelled     => ExitCode::from(130),
          _                       => ExitCode::from(70),
      }
  }
  ```
  Mark `SectorError` `#[non_exhaustive]` at the same time (see F-X03-005) so the `_` arm is intentional.
- **Effort:** S
- **Risk of fix:** Low.

### F-X03-005 — [MEDIUM] [API design] No public error enum is `#[non_exhaustive]`; every variant addition is a breaking change

- **Location:** All 13 enums enumerated above (none carry the attribute; verified by `grep -rEn "non_exhaustive" --include='*.rs' -- src builder viewer gui-core` → 0 hits).
- **Category:** API design
- **Confidence:** High
- **Blast radius:** Re-exported types: `SectorError`, `WorldsTomlError`, `MapThemeError`, `SubsectorBuildError`, `MergeError`, `WorldError`, `MutationError` (sector_model). The lib has no semver discipline today, but the public surface is large and downstream callers in builder/viewer do `match e { SectorError::ConfigParse { ... } => ... }` (see `builder/src/builder/project_io.rs:699-714`).
- **Problem:** Adding a new variant to any of these enums silently breaks every exhaustive `match` in `builder` and `viewer`. The rubric §3.4 calls this out specifically as a HIGH-floor issue; I'm scoring MEDIUM only because the workspace is a closed system (the only downstream callers are crates in the same repo and they'll fail to compile rather than silently misbehave).
- **Suggested fix:** Add `#[non_exhaustive]` to every public `*Error` enum. Where `match` sites exist (e.g. `map_load_err` in `project_io.rs:699`), they already have an `other => ...` arm and need no change. Where they don't, the compiler will force the addition.
- **Effort:** S
- **Risk of fix:** Low — adds compile-time noise, no runtime impact.

### F-X03-006 — [MEDIUM] [Error handling] `BuilderError::ParseFailed { file, message: String }` flattens typed source errors; same anti-pattern repeated 12+ times

- **Locations:** Variant defn `builder/src/builder/errors.rs:13-14`; call sites at
  `project_io.rs:140-143, 147-152, 175-178, 209-216, 224-226, 234-236, 267-270, 461-464, 476-479, 684-688, 699-714, 717-722`,
  `session.rs:252-256`, `state/generation_ops.rs:145-148, 250-253`.
- **Category:** Error handling / context loss
- **Confidence:** High
- **Blast radius:** Every project-load and project-save path in the builder. The typed TOML error (with span + line/col) is `.to_string()`-ed into a free-form `message` field. Downstream code can only display the string, not pretty-print spans or extract the line for an editor-side highlight.
- **Problem:** The enum already has `IoFailed(#[from] std::io::Error)` and `Serde(#[from] serde_json::Error)` — the pattern works. But TOML serialize/deserialize errors are squashed into `ParseFailed.message` instead of getting their own `Toml(#[from] toml::de::Error)` / `TomlSer(#[from] toml::ser::Error)` variants. Note `map_load_err` (line 699) goes further and squashes an already-typed `SectorError` into `ParseFailed.message`, **losing the `path` and `source: io::Error` distinction `SectorError::Io` was carrying.** This is the load-boundary information loss the brief asked about.
- **Why it matters:** Once flattened to `String`, the builder UI can never offer "jump to line in editor" for a malformed TOML — the line number is buried in unstructured text. Tests in this file already assert the message *contains* "line" — fragile.
- **Suggested fix:** Extend `BuilderError`:
  ```rust
  #[non_exhaustive]
  pub enum BuilderError {
      // ...existing...
      #[error("TOML parse error in {file}: {source}")]
      Toml { file: String, #[source] source: toml::de::Error },
      #[error("TOML emit error for {file}: {source}")]
      TomlSer { file: String, #[source] source: toml::ser::Error },
      #[error(transparent)]
      Sector(#[from] sectorforge::errors::SectorError),
  }
  ```
  Replace the 12+ `.map_err(|e| BuilderError::ParseFailed { file: ..., message: e.to_string() })` sites with `.map_err(|e| BuilderError::Toml { file: ..., source: e })`. `map_load_err` becomes the one-liner `BuilderError::Sector(err)`.
- **Effort:** M
- **Risk of fix:** Low — `ParseFailed` can stay for the cases that actually are stringly-typed validation errors (e.g. `project_io.rs:140-143`).

### F-X03-007 — [MEDIUM] [Error handling] `SectorError::ConfigParse`/`WorldDataLoad`/`ExportFailed` use `message: String` instead of `#[source]`; loses TOML/JSON span info

- **Locations:** `src/model/errors.rs:13-23, 34-35` (definitions); construction sites at `src/lib.rs:367, 383, 398, 413, 426, 441, 455`, `src/loading/input.rs:60, 71`, `src/loading/presets.rs:96`, `src/analysis/relations.rs:1331`, `src/analysis/economy.rs:784`, `src/analysis/briefing.rs:502`, `src/analysis/search.rs:411` (≈14 sites).
- **Category:** Error handling / context loss
- **Confidence:** High
- **Blast radius:** Every CLI-side report of a malformed config file. The CLI prints `"error: failed to parse config at <path>: <message>"` where `<message>` is `toml_error.to_string()` — line/col is preserved by happy accident of TOML's `Display`, but the structured source is gone. A consumer that wanted to render TOML errors with diagnostic spans (e.g. a future LSP integration) has no path.
- **Problem:** Same shape as F-X03-006 but at the library level. The enum has `Io { #[source] source: io::Error }` showing the right pattern — but `ConfigParse` and `WorldDataLoad` use `message: String` instead of a `#[source] source: ConfigParseSource` (could be its own enum wrapping `toml::de::Error`, `serde_json::Error`).
- **Suggested fix:** Either:
  1. Add `Toml(#[from] toml::de::Error)` + `Json(#[from] serde_json::Error)` variants and let `?` carry them.
  2. Or keep the path context but use a `Box<dyn Error + Send + Sync>` source: `ConfigParse { path: String, #[source] source: Box<dyn Error + Send + Sync> }`.
  Option 1 is cleaner for known sources; option 2 is more compatible with the existing `path` carrier shape. Either way, callers stop doing `format!("invalid sector json: {e}")`.
- **Effort:** M (≈14 call sites)
- **Risk of fix:** Medium — touches the library's most public error enum. Pair with F-X03-005 (`#[non_exhaustive]`) and do both in one breaking-change window.

### F-X03-008 — [MEDIUM] [Error handling] `viewer/src/editor/wishes_panel.rs:120, 131` silently swallows `generate()` errors on user button click

- **Location:** `viewer/src/editor/wishes_panel.rs:117-136`.
- **Category:** Silent failure at user interaction boundary
- **Confidence:** High
- **Blast radius:** The "Apply" and "Preview" buttons in the wishes panel. `sectorforge::generation::generate(input)` returns `Result<GeneratedSector, SectorError>` — covering NoWorldCandidates, WeightedSelectionFailed, InvalidConfig. The viewer drops the error and leaves `state.sector` at the old value with no indication.
- **Problem:** ```rust
  if let Ok(sec) = sectorforge::generation::generate(input.clone()) {
      state.sector = Some(sec);
      state.mark_dirty();
  }
  ```
  Twice. The user clicks "Apply candidate seed", the generator rejects the wishes-derived seed (e.g. because the constraint pool degenerated under that seed), and visually nothing happens.
- **Suggested fix:** Add a status string on the panel state (`state.status: String`), and on `Err(e) => { state.status = format!("generate failed: {e}"); }`. The panel already shows free-text "best near-miss" output (`cli/generate.rs:72`), so the surface is established elsewhere; carry it through here too.
- **Effort:** S
- **Risk of fix:** Low.

### F-X03-009 — [MEDIUM] [Error handling] `viewer/src/editor/file_ops.rs:65-68` silently discards `load_project` errors when loading the project alongside a sector

- **Location:** `viewer/src/editor/file_ops.rs:62-69`.
- **Category:** Silent failure
- **Confidence:** High
- **Blast radius:** Project-aware viewer features (wishes panel, constraint preview, faction designer presets). When a user opens a project whose `sectorforge.toml` is malformed, the sector loads fine but `input` is silently `None`, and downstream features that check `state.project_input.is_some()` silently degrade.
- **Problem:** ```rust
  if let Ok(utf8_root) = camino::Utf8PathBuf::from_path_buf(project_root) {
      if let Ok(pi) = sectorforge::input::load_project(&utf8_root) {
          input = Some(pi);
      }
  }
  ```
  Two nested `if let Ok` discarding (a) UTF-8 path conversion error, (b) `SectorError` from the loader. Mirror pattern at `viewer/src/app/lifecycle.rs:58-62` and `viewer/src/app/sector_view.rs:672-674` and `viewer/src/app/lifecycle.rs:200-202`.
- **Suggested fix:** Bubble the error to the caller's return type (`(sector, Option<input>, source, Option<load_warning>)`) or store it in the editor state and display in the existing status bar. The simplest delta is to push the error string into `EditorFileError::Config` and let the caller surface it.
- **Effort:** S
- **Risk of fix:** Low.

### F-X03-010 — [LOW] [Error handling] `builder/src/builder/preferences.rs:43` silently discards parse errors loading user prefs

- **Location:** `builder/src/builder/preferences.rs:36-44`.
- **Category:** Silent failure (intentional per doc comment)
- **Confidence:** High
- **Blast radius:** User preferences file. If a user hand-edits `~/.config/sectorforge/prefs.toml` with a typo, they get default prefs on next launch with no indication that their edits were ignored.
- **Problem:** The doc comment explicitly says "intentionally swallows … so a hand-edited file with a typo cannot wedge the GUI." Fair — but the right shape is **degrade-with-notice**, not silent revert. The current code doesn't even log to stderr.
- **Suggested fix:** Keep the `unwrap_or_default()` shape, but add a stderr line on parse failure:
  ```rust
  match toml::from_str(&text) {
      Ok(p) => p,
      Err(e) => {
          eprintln!("preferences: failed to parse {} ({e}); using defaults", path);
          Self::default()
      }
  }
  ```
  No GUI surface needed; the file is preferences-only.
- **Effort:** S
- **Risk of fix:** Low.

### F-X03-011 — [LOW] [Error handling] `src/cli/generate.rs:60` silent `.ok()` on constraint-file read

- **Location:** `src/cli/generate.rs:60`.
- **Category:** Silent failure at CLI boundary
- **Confidence:** High
- **Problem:** `let constraints_text = std::fs::read_to_string(&c_path).ok();`. The path was already loaded successfully a few lines up (`search::load_wishes(&c_path)?` at line 34), so the `.ok()` is almost certainly fine — but if the file changes between the two reads, the digest field silently goes to `None` and the manifest loses the audit trail.
- **Suggested fix:** Either re-use the bytes already loaded by `load_wishes` (better — read once, hash once, parse once), or fail loudly: `let constraints_text = std::fs::read_to_string(&c_path).map_err(|e| SectorError::io(c_path.as_str(), e))?;`.
- **Effort:** S
- **Risk of fix:** Low.

### F-X03-012 — [LOW] [API design] `PresetGalleryError` is a single-variant enum with one `String` payload

- **Location:** `viewer/src/preset_gallery.rs:15-19`.
- **Problem:** `enum PresetGalleryError { Load(String) }` adds no information over a `String` and forces a `.to_string()` flattening at construction (`presets::list(&dir).map_err(|e| PresetGalleryError::Load(e.to_string()))`, line 56). The underlying `presets::list` returns `SectorError` — wrapping its `Display` into a string and re-wrapping that in a Newtype loses both `#[source]` and structured matching.
- **Suggested fix:** Replace with `#[error(transparent)] enum PresetGalleryError { Load(#[from] sectorforge::SectorError) }`, or just use `SectorError` directly. If the viewer prefers to keep a viewer-local error type for matching purposes, give it `Load(#[from] sectorforge::SectorError)` rather than `Load(String)`.
- **Effort:** S
- **Risk of fix:** Low.

### F-X03-013 — [NIT] [Style] `WorldsTomlError::Parse(String)` / `Emit(String)` should `#[from]` `toml::de::Error` / `toml::ser::Error`

- **Location:** `src/worlds_toml.rs:59-62, 108, 113`.
- **Problem:** Same shape as F-X03-007 but localized. `toml::from_str(text).map_err(|e| WorldsTomlError::Parse(e.to_string()))` could be `toml::from_str(text)?` if `Parse(#[from] toml::de::Error)`.
- **Suggested fix:** Replace `Parse(String)` with `Parse(#[from] toml::de::Error)`, ditto Emit. Eliminates two `.map_err(|e| ...e.to_string())` closures.
- **Effort:** S
- **Risk of fix:** Low.

### F-X03-014 — [NIT] [Style] `src/validate/diff.rs` has ~80 `let _ = writeln!(s, ...)` lines

- **Location:** `src/validate/diff.rs:808-1010` approximately.
- **Problem:** `writeln!` on a `String` is infallible — the `let _ =` is acknowledging clippy. Cleaner is `use std::fmt::Write as _;` at top of file (already imported elsewhere) and dropping the `let _ =`, since the `Write` impl on `String` never errors.
- **Suggested fix:** Bulk replace `let _ = writeln!(s, ` → `writeln!(s, ` (then `unwrap` will be required, or use `_ = writeln!(...);` for terseness). Stylistic only.
- **Effort:** S
- **Risk of fix:** Low.

## Cross-cutting observations (not findings)

1. **Layering is right in principle.** Each crate has its own `*Error` and converts at the seam — no `Box<dyn Error>`, no `anyhow`, no `Result<T, String>`. This is the correct shape; the findings above are about execution gaps, not strategy.

2. **`?` context loss is largely absent in `src/`.** Almost every `?` in the library is preceded by a `.map_err(|e| SectorError::io(path, e))` or equivalent that attaches a path. This is exemplary. Where it breaks down is in the GUI crates and at the cross-crate seam (F-X03-002, F-X03-006).

3. **The codebase has no `panic!`/`todo!`/`unimplemented!`/`unreachable!` in library code** (verified by grep — 0 hits). This is good and rare; worth preserving with a `#![deny(clippy::panic, clippy::todo, clippy::unimplemented, clippy::unreachable)]` at the crate roots if it isn't already.

4. **No async, no shared mutable state, no `Arc<Mutex<...>>` patterns.** Concurrency-related error categories from §3.5 are all N/A. Errors are synchronous, single-threaded, and propagated via `Result`.

## Summary of suggested fixes

| id | severity | short | effort | risk |
|---|---|---|---|---|
| F-X03-001 | HIGH | Delete dead duplicate `MutationError` in `src/model/errors.rs:41-52` | S | Low |
| F-X03-002 | HIGH | Stop swallowing TOML parse errors in `project_io::reload_catalog` (15 sites) | M | Low |
| F-X03-003 | HIGH | Propagate `SubsectorBuildError` instead of `.unwrap_or_default()` (8 sites) | M | Low |
| F-X03-004 | HIGH | Map `SectorError` variants to differentiated CLI exit codes in `src/main.rs` | S | Low |
| F-X03-005 | MEDIUM | Add `#[non_exhaustive]` to all 13 public `*Error` enums | S | Low |
| F-X03-006 | MEDIUM | Replace `BuilderError::ParseFailed.message: String` with typed `Toml`/`TomlSer`/`Sector` variants | M | Low |
| F-X03-007 | MEDIUM | Replace `SectorError::ConfigParse.message: String` with `#[source]`/`#[from]` typed source | M | Medium |
| F-X03-008 | MEDIUM | Surface `generate()` errors in `wishes_panel.rs:120, 131` instead of `if let Ok` | S | Low |
| F-X03-009 | MEDIUM | Surface `load_project` errors in `viewer/editor/file_ops.rs` and `app/lifecycle.rs` | S | Low |
| F-X03-010 | LOW | Log to stderr when `preferences.rs` swallows a parse error | S | Low |
| F-X03-011 | LOW | Don't silently `.ok()` the constraint-file read in `cli/generate.rs:60` | S | Low |
| F-X03-012 | LOW | `PresetGalleryError::Load(String)` → `Load(#[from] SectorError)` | S | Low |
| F-X03-013 | NIT | `WorldsTomlError::Parse/Emit` should `#[from]` the toml errors | S | Low |
| F-X03-014 | NIT | `src/validate/diff.rs` — drop the 80× `let _ = writeln!(...)` boilerplate | S | Low |
