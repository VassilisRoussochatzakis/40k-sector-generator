---
sweep_id: X06
scope: cross-cutting (Cargo manifests, lockfile, features, lints)
paths:
  - Cargo.toml
  - builder/Cargo.toml
  - viewer/Cargo.toml
  - gui-core/Cargo.toml
  - Cargo.lock
  - builder/clippy.toml
  - viewer/clippy.toml
loc_reviewed: 250
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 0, medium: 6, low: 4, nit: 3 }
top_risks:
  - "No [workspace.dependencies] table — every shared crate is declared up to 4 times with copy-pasted version+feature strings, easy to drift (F-X06-001)"
  - "viewer declares image and tempfile but uses neither (F-X06-002, F-X06-003)"
  - "gui-core declares eframe but only uses egui — eframe is dead weight in the lib (F-X06-004)"
  - "No [workspace.lints], and clippy disallowed_* policy only set on builder/viewer (F-X06-008, F-X06-009)"
  - "RUSTSEC-2024-0436 (unmaintained `paste` via metal→wgpu-hal→eframe) — not actionable from workspace (F-X06-011)"
---

# Cross-cutting Sweep: Dependencies & Cargo Hygiene (X06)

## Method

1. Read all four `Cargo.toml` files and both `clippy.toml` files.
2. `cargo audit` (RustSec advisory DB, 1098 advisories loaded; 423 crate-deps scanned).
3. Manual unused-dep check: for each direct dependency declared in a crate's `[dependencies]` / `[dev-dependencies]`, grep that crate's `src/` and `tests/` for either `use <crate>` or `<crate>::` (string-literal matches discounted). `cargo +nightly udeps` was not run — nightly toolchain not installed; `cargo-udeps`/`cargo-machete` not in `~/.cargo/bin`.
4. Cross-referenced `Cargo.lock` for version duplicates against `reviews/_baseline/dup-deps.txt`.
5. Reviewed `reviews/_baseline/inventory.md` for declared features and profiles.

`cargo audit` summary:

```
Scanning Cargo.lock for vulnerabilities (423 crate dependencies)
Crate:     paste
Version:   1.0.15
Warning:   unmaintained
ID:        RUSTSEC-2024-0436
Dependency tree:
paste 1.0.15
└── metal 0.29.0
    └── wgpu-hal 22.0.0
        └── wgpu-core 22.1.0
            └── wgpu 22.1.0
                └── egui-wgpu 0.29.1
                    └── eframe 0.29.1
```

No CVE / security advisories. One unmaintained-crate warning, transitive through `eframe`.

## Findings

### F-X06-001 — [MEDIUM] [Maintainability] No `[workspace.dependencies]` — shared crates declared 3–4 times with copy-pasted versions/features

- **Location:** `Cargo.toml:6-7` (no `[workspace.dependencies]`); `builder/Cargo.toml:9-17`; `viewer/Cargo.toml:7-19`; `gui-core/Cargo.toml:7-10`
- **Category:** Dependencies / Cargo hygiene
- **Confidence:** High
- **Blast radius:** Future drift — bumping `eframe` to 0.30 requires editing three files in lockstep; missing one yields a confusing transitive-version split.
- **Problem:** The root `Cargo.toml` defines `[workspace]` (members only) but **no `[workspace.dependencies]` table**. As a result the same dependency lines are repeated:
  - `eframe = { version="0.29", default-features=false, features=["default_fonts","glow","wayland","x11"] }` × 3 (gui-core, builder, viewer)
  - `egui = "0.29"` × 3
  - `image = { version="0.25", default-features=false, features=["png"] }` × 3 (root, gui-core, viewer)
  - `camino = { version="1", features=["serde1"] }` × 3 (root, builder, viewer)
  - `clap = { version="4", features=["derive"] }` × 3 (root, builder, viewer)
  - `serde = { version="1", features=["derive","rc"] }` × 4 (root, builder, viewer, indirectly gui-core)
  - `serde_json = "1"` × 3
  - `toml = "0.8"` × 3
  - `thiserror = "1"` × 3
  - `rfd = "0.17"` × 2
  - `rand = "0.8"` × 2
  - `tempfile = "3"` × 3 (dev-deps)
- **Why it matters:** (a) Drift — easy for one crate to advance to `eframe = "0.30"` while others stay on `"0.29"`, fragmenting the build. (b) Audit churn — every clippy/security upgrade is N edits instead of 1. (c) Cargo since 1.64 supports `crate.workspace = true` exactly for this.
- **Evidence:** Reads of all four manifests.
- **Suggested fix:** Hoist into `[workspace.dependencies]`. Concrete plan at end of report under **Dep-Hoisting Plan**.
- **Effort:** S (~30 min mechanical)
- **Risk of fix:** Low — Cargo resolves to identical versions; lockfile should not change.

---

### F-X06-002 — [MEDIUM] [Dead code] `viewer` declares `image = "0.25"` but never uses it

- **Location:** `viewer/Cargo.toml:19`
- **Category:** Dependencies / unused direct dep
- **Confidence:** High
- **Blast radius:** Build time only; viewer already pulls `image` transitively via `eframe`, so removing it does not change `Cargo.lock`.
- **Problem:** `grep -rn "use image\|image::" viewer/src` returns **zero matches**. The crate is in the manifest but not consumed.
- **Why it matters:** Misleading — anyone reading `viewer/Cargo.toml` thinks viewer does PNG work; it does not. (Viewer routes PNG through `sectorforge::export::bitmap`, which owns the `image` dep.)
- **Evidence:** `grep -rn "use image\|image::" viewer/src` → empty. Only matches for the substring `image` in viewer are string literals like `"PilgrimageSite"`.
- **Suggested fix:** Delete the line.
  ```toml
  # viewer/Cargo.toml — remove:
  - image = { version = "0.25", default-features = false, features = ["png"] }
  ```
- **Effort:** S
- **Risk of fix:** Low — `cargo check -p sectorforge-viewer` will validate.

---

### F-X06-003 — [LOW] [Dead code] `viewer` declares `tempfile = "3"` dev-dep but no test in viewer uses it

- **Location:** `viewer/Cargo.toml:21-22`
- **Category:** Dependencies / unused dev-dep
- **Confidence:** High
- **Blast radius:** Dev-only — test build time.
- **Problem:** `viewer/` has no `tests/` directory, and `grep -rn "tempfile\|TempDir" viewer` matches only the `Cargo.toml` declaration. No `#[cfg(test)]` inline test in `viewer/src` uses it either.
- **Evidence:** `ls viewer/tests` → ENOENT; `grep -rn tempfile viewer/src` → empty.
- **Suggested fix:** Delete the dev-dep line. If the workspace later adds an integration test for viewer that needs scratch dirs, restore via the hoisted `workspace.dev-dependencies` (see plan below).
  ```toml
  # viewer/Cargo.toml — remove [dev-dependencies] tempfile = "3"
  ```
- **Effort:** S
- **Risk of fix:** Low.

---

### F-X06-004 — [MEDIUM] [Dead code] `gui-core` declares `eframe` but only uses `egui`

- **Location:** `gui-core/Cargo.toml:8`
- **Category:** Dependencies / unused direct dep
- **Confidence:** High
- **Blast radius:** None at link time (eframe is still pulled by builder/viewer); manifest only.
- **Problem:** `grep -rn "eframe" gui-core/src tests` → no matches. The crate uses `egui::IconData`, `egui::Painter`, `egui::Shape`, etc., but never references `eframe::*`. Likely declared originally to share feature-flag selection across crates.
- **Why it matters:** A library crate (gui-core) should only declare what it actually `use`s. The over-declaration tightly couples gui-core's release to eframe's release cadence even though gui-core compiles fine against bare `egui`.
- **Evidence:** `grep -rn "eframe" gui-core` → only matches `gui-core/Cargo.toml:8`.
- **Suggested fix:** Drop `eframe` from `gui-core/Cargo.toml`. The feature unification still happens at the workspace level because builder/viewer both declare the same `eframe` features.
  ```toml
  # gui-core/Cargo.toml — remove:
  - eframe = { version = "0.29", default-features = false, features = ["default_fonts", "glow", "wayland", "x11"] }
  ```
- **Effort:** S
- **Risk of fix:** Low — if a future gui-core change actually needs `eframe::Frame`, restore via workspace dep.

---

### F-X06-005 — [LOW] [Determinism / Hygiene] viewer uses `rand::random::<f64>()` for "randomize seed" UI, but root invariant says all RNG goes through `src/model/rng.rs`

- **Location:** `viewer/src/editor/generation_panel.rs:30`, `viewer/src/editor/generation_panel.rs:259`
- **Category:** Dependencies / project invariant
- **Confidence:** Medium — UI-only use, not part of byte-stable output, so probably intentional. But CLAUDE.md says *"Do not introduce `rand::thread_rng()` or seed from anything outside the stage RNG."*
- **Blast radius:** None on golden output. Only affects the seed-string the user sees in the UI when clicking "randomize".
- **Problem:** `rand::random::<f64>()` internally calls `thread_rng()`. The CLAUDE.md invariant disallows this. The use is in a UI handler ("randomize seed" button) so it is genuinely non-deterministic by design — but the invariant doesn't carve out an exception.
- **Evidence:**
  ```
  viewer/src/editor/generation_panel.rs:30:  input.config.generation.seed = f64::to_string(&rand::random::<f64>());
  viewer/src/editor/generation_panel.rs:259: input.config.generation.seed = f64::to_string(&rand::random::<f64>());
  ```
- **Suggested fix:** Either (a) carve out an explicit UI-exception in CLAUDE.md and add a `// invariant-exception: UI seed picker, not part of byte-stable output` comment at both call sites, or (b) re-derive the seed via `blake3::Hasher::new().update(&std::time::SystemTime::now()...)` so the dependency on `rand` from viewer can be deleted entirely. Option (b) lets viewer drop `rand = "0.8"` from its manifest.
- **Effort:** S
- **Risk of fix:** Low. Whichever option chosen, must be documented.

Related: if (b) is chosen, viewer can also drop `rand = "0.8"` from `Cargo.toml:13`.

---

### F-X06-006 — [MEDIUM] [Maintainability] No `MSRV` (`rust-version`) declared in any manifest

- **Location:** all four `[package]` stanzas
- **Category:** Dependencies / pinning
- **Confidence:** High
- **Blast radius:** Future — a contributor on rustc 1.80 may suddenly find the workspace broken when a dep silently raises its MSRV.
- **Problem:** Inventory already flags this. No `rust-version = "..."` line anywhere. Workspace currently builds on rustc 1.95 (per baseline build).
- **Why it matters:** Without MSRV declared, Cargo can pick newer minor versions of deps that require newer rustc, and there is no compile-time check.
- **Suggested fix:** Add to `[workspace.package]` (Cargo 1.64+):
  ```toml
  [workspace.package]
  rust-version = "1.85"   # or whatever you actually support; pick conservatively
  edition = "2021"
  ```
  then in each member crate replace `edition = "2021"` with `edition.workspace = true` and add `rust-version.workspace = true`.
- **Effort:** S
- **Risk of fix:** Low.

---

### F-X06-007 — [LOW] [Maintainability] Caret ranges are extremely loose (`"1"`, `"0.8"`, `"4"`)

- **Location:** all four manifests
- **Category:** Dependencies / version policy
- **Confidence:** Medium
- **Blast radius:** Bin crates — `Cargo.lock` is committed so reproducibility is preserved. But a fresh `cargo update` may silently jump from `serde 1.0.228` to `serde 1.0.999`. Acceptable for an app; less ideal once gui-core becomes a published library.
- **Problem:** `serde = "1"`, `clap = "4"`, `toml = "0.8"`, `thiserror = "1"`, etc. — the loosest possible semver compatibility. `Cargo.lock` pins effective versions today.
- **Suggested fix:** Either (a) leave as is and document it as an explicit choice in `docs/MAP.md`, or (b) tighten to minor-precise (`serde = "1.0"`, `clap = "4.5"`, `toml = "0.8"`). For published library (`gui-core`), prefer (b). The plan below uses minor-precise versions.
- **Effort:** S
- **Risk of fix:** Low.

---

### F-X06-008 — [MEDIUM] [Lint hygiene] `[lints.clippy]` block only declared on builder and viewer; gui-core and root are silent

- **Location:** `builder/Cargo.toml:22-24`, `viewer/Cargo.toml:24-26`, vs. `Cargo.toml`, `gui-core/Cargo.toml` (no lints)
- **Category:** Dependencies / Cargo hygiene
- **Confidence:** High
- **Blast radius:** Inconsistent enforcement of the project invariant *"raw paint primitives live in sectorforge-gui-core"*. If someone uses `egui::Painter` directly in root `src/` or in gui-core itself, clippy won't error — but the same code in builder/viewer would.
- **Problem:** The `disallowed_types`/`disallowed_methods` policy in `builder/clippy.toml` and `viewer/clippy.toml` (which are byte-identical, see F-X06-010) is meant to keep raw `egui::Painter` use confined to gui-core. The corresponding `[lints.clippy] disallowed_types = "deny" / disallowed_methods = "deny"` activation is **only** in builder/viewer manifests. Root + gui-core have no `[lints.clippy]` stanza at all.
- **Why it matters:** Gui-core is the canonical owner of paint primitives, so the lint shouldn't fire *there*. Root is a non-GUI crate where the policy is irrelevant. But the inconsistency means **no one** enforces "no `clippy::dbg_macro`", "no `clippy::todo`", "no `clippy::print_stderr`" workspace-wide.
- **Evidence:** Reads of all four manifests; baseline clippy showed 542 warnings on builder lib alone (many of which a workspace `[lints]` block would surface earlier).
- **Suggested fix:** Add a `[workspace.lints]` block in root `Cargo.toml`, then have each crate opt in via `[lints] workspace = true`. See plan below. Pair with a workspace-level `clippy.toml` (single source of truth) — see F-X06-010.
- **Effort:** M (need to triage which warning groups become `warn`/`deny`)
- **Risk of fix:** Medium — turning warnings into denies may break the build on existing warnings; staged rollout recommended.

---

### F-X06-009 — [MEDIUM] [Lint hygiene] No `[workspace.lints]` table — workspace lint policy is not centralized

- **Location:** `Cargo.toml` (root) — no `[workspace.lints]`
- **Category:** Dependencies / Cargo hygiene
- **Confidence:** High
- **Blast radius:** Same as F-X06-008.
- **Problem:** Independent finding from F-X06-008: even before adding *new* lints, the existing `disallowed_types`/`disallowed_methods = "deny"` policy should live in one place, not be duplicated across two manifests with two identical `clippy.toml` files.
- **Suggested fix:** Move the policy to `[workspace.lints.clippy]` and opt-in per-crate via `[lints] workspace = true`. See plan.
- **Effort:** S
- **Risk of fix:** Low.

---

### F-X06-010 — [NIT] [Duplication] `builder/clippy.toml` and `viewer/clippy.toml` are byte-identical

- **Location:** `builder/clippy.toml:1-11`, `viewer/clippy.toml:1-11`
- **Category:** Dependencies / Cargo hygiene
- **Confidence:** High
- **Problem:** Both files declare exactly the same `disallowed-types` / `disallowed-methods` set. Drift risk: adding a rule in one and forgetting the other.
- **Suggested fix:** Move to a single root `clippy.toml`. (Cargo's per-package `clippy.toml` lookup walks up to the workspace root, so a single root file applies to every member crate. The `[lints]` activation per crate can still differ if needed.)
- **Effort:** S
- **Risk of fix:** Low.

---

### F-X06-011 — [LOW] [Security advisory] RUSTSEC-2024-0436 — unmaintained `paste 1.0.15` (transitive through eframe → egui-wgpu → wgpu → wgpu-hal → metal)

- **Location:** transitive; not a direct dep. Path: `paste 1.0.15 ← metal 0.29.0 ← wgpu-hal 22.0.0 ← wgpu-core 22.1.0 ← wgpu 22.1.0 ← egui-wgpu 0.29.1 ← eframe 0.29.1`
- **Category:** Dependencies / RustSec advisory
- **Confidence:** High (from `cargo audit`)
- **Blast radius:** None today — `paste` is a proc-macro crate (compile-time only); the advisory is "unmaintained", not a CVE. No runtime exposure.
- **Why it matters:** Tracks the eframe/wgpu version. eframe 0.30/0.31+ may upgrade `wgpu` to a release where `metal` no longer needs `paste`. Worth watching, not actionable from workspace.
- **Suggested fix:** Add a documented `cargo-deny` (or `.cargo/audit.toml`) ignore line so CI doesn't flake on this:
  ```toml
  # .cargo/audit.toml
  [advisories]
  ignore = ["RUSTSEC-2024-0436"]   # paste, transitive through eframe; reassess when eframe ≥ 0.30
  ```
- **Effort:** S
- **Risk of fix:** Low (only adds noise suppression, doesn't change the build).

---

### F-X06-012 — [LOW] [Dep version split] Workspace pins `thiserror = "1"`, but a dep (wgpu) already pulls `thiserror 2.0.18` — bumping the workspace would deduplicate

- **Location:** root `Cargo.toml:28`, `builder/Cargo.toml:17`, `viewer/Cargo.toml:11`
- **Category:** Dependencies / version unification
- **Confidence:** High
- **Blast radius:** Build time only — `thiserror` is a proc-macro crate; both major versions are in the dep graph today (`thiserror 1.0.69` × 8+ deps, `thiserror 2.0.18` × 4 deps). Workspace bumping to `"2"` would drop the 1.x copy from sectorforge's own contribution to the lockfile (others may still pull it transitively).
- **Why it matters:** Minor build-time savings; consolidates upgrade path; macro APIs are essentially identical between 1.x and 2.x.
- **Suggested fix:** Bump workspace `thiserror = "1"` → `thiserror = "2"`. Verify with `cargo check --workspace --all-targets`.
- **Effort:** S
- **Risk of fix:** Low — `thiserror` 2 is a near-drop-in (one breaking change: bare `#[from]` on a unit variant). Project's `#[derive(Error)]` uses all look standard.

---

### F-X06-013 — [NIT] [Heavy crate for trivial need] No misuse detected

- **Location:** N/A
- **Category:** Dependencies / right-sizing
- **Confidence:** High
- **Problem:** Specifically checked for `regex` (none used in workspace), `serde_yaml` (none), `chrono` (none), `tokio` (none), `reqwest` (none). All declared crates have a clear, justified use in source. The `image` crate is correctly stripped to `default-features=false, features=["png"]`. Same for `eframe` (only `default_fonts`+backends). `criterion` is stripped to `default-features=false, features=["cargo_bench_support"]`.
- **Evidence:** Per-crate grep audit above; manifest read.
- **Suggested fix:** None. Keep current minimal feature selection.

---

### F-X06-014 — [NIT] [Cargo audit tooling] No CI gate for `cargo audit`

- **Location:** N/A (no CI config inspected; no `.cargo/audit.toml`; no `deny.toml`).
- **Category:** Dependencies / CI hygiene
- **Confidence:** Medium
- **Problem:** RustSec/CVE checks run only when someone manually runs `cargo audit`. The only finding today is RUSTSEC-2024-0436 (unmaintained, F-X06-011), but a future CVE on a direct dep would go unnoticed.
- **Suggested fix:** Add `cargo audit` (and optionally `cargo deny check advisories,bans,licenses`) to CI. Schema:
  ```yaml
  # .github/workflows/audit.yml
  - run: cargo install --locked cargo-audit
  - run: cargo audit --deny warnings
  ```
- **Effort:** S
- **Risk of fix:** Low — informational gate.

---

## Categories with no findings

- **Workspace-fixable transitive duplicates beyond what dup-deps.txt shows:** None. All duplicates in `reviews/_baseline/dup-deps.txt` (`bitflags 1↔2`, `core-foundation 0.9↔0.10`, `objc2 0.5↔0.6`, `objc2-app-kit 0.2↔0.3`, `objc2-foundation 0.2↔0.3`, `block2 0.5↔0.6`, `core-graphics`, `getrandom 0.2↔0.3↔0.4`, `rand 0.8↔0.9`, `rand_chacha 0.3↔0.9`, `rand_core 0.6↔0.9`) are forced by the eframe/winit/objc2/wgpu stack on one side and the workspace's direct deps (`rand 0.8`) or proptest's transitive (`rand 0.9`) on the other. Workspace cannot eliminate these without major upstream bumps. The `rand 0.8 ↔ 0.9` split is the only one where workspace action *could* help, by bumping the workspace's direct `rand = "0.8"` to `"0.9"` — but that's a breaking API change in `rand` (`SeedableRng`, `Rng::gen`) and risks the determinism invariant of `src/model/rng.rs`. **Recommend leaving as is.** See top of file: `src/model/rng.rs` uses `rand_chacha::ChaCha*Rng` directly and the project intentionally pinned `rand 0.8` for reproducibility (`rand 0.9` changed default PRNG plumbing).
- **License-compatibility issues:** None spotted. All direct deps are MIT or MIT/Apache-2.0 dual. No GPL/AGPL transitives observed in `Cargo.lock`. (Not a full SPDX audit; recommend `cargo deny check licenses` for that.)
- **`cargo geiger`:** Not run (per task instructions). Workspace `unsafe` = 0 per `unsafe-audit.review.md`.
- **Unsafe surface in deps:** Unavoidably non-trivial via eframe/winit/glutin/objc2/wgpu (OS-FFI seam). Not actionable.
- **`dhat` gating:** Correct. `[features] dhat-heap = ["dep:dhat"]` and `[[bin]] dhat-profile` with `required-features = ["dhat-heap"]`. Zero cost on default builds. Verified.

---

## Summary of suggested fixes

| id | severity | short | effort | risk |
|---|---|---|---|---|
| F-X06-001 | MEDIUM | Hoist shared deps into `[workspace.dependencies]` (12+ duplicated lines collapse to 1 each) | S | Low |
| F-X06-002 | MEDIUM | Remove unused `image = "0.25"` from `viewer/Cargo.toml` | S | Low |
| F-X06-003 | LOW | Remove unused `tempfile` dev-dep from `viewer/Cargo.toml` | S | Low |
| F-X06-004 | MEDIUM | Remove unused `eframe` from `gui-core/Cargo.toml` | S | Low |
| F-X06-005 | LOW | viewer's `rand::random` in seed UI violates CLAUDE.md RNG invariant — document exception or replace with blake3-of-time | S | Low |
| F-X06-006 | MEDIUM | Declare `rust-version` MSRV in `[workspace.package]` | S | Low |
| F-X06-007 | LOW | Tighten caret ranges or document looseness | S | Low |
| F-X06-008 | MEDIUM | Centralize lint policy: `[workspace.lints]` + `[lints] workspace = true` per crate | M | Medium |
| F-X06-009 | MEDIUM | Hoist `disallowed_*=deny` into `[workspace.lints.clippy]` | S | Low |
| F-X06-010 | NIT | Collapse `builder/clippy.toml` + `viewer/clippy.toml` into one root `clippy.toml` | S | Low |
| F-X06-011 | LOW | Document RUSTSEC-2024-0436 (paste, transitive, unmaintained) ignore in `.cargo/audit.toml` | S | Low |
| F-X06-012 | LOW | Bump workspace `thiserror = "1"` → `"2"` | S | Low |
| F-X06-013 | NIT | No-op — right-sizing audit clean | - | - |
| F-X06-014 | NIT | Add `cargo audit` to CI | S | Low |

---

## Dep-Hoisting Plan (concrete diff)

This is the suggested target state for the root `Cargo.toml`. After applying, each member crate's `[dependencies]` becomes ~5 lines of `<name>.workspace = true`.

### Root `Cargo.toml` — add these sections

```toml
[workspace]
members = ["viewer", "gui-core", "builder"]
resolver = "2"   # confirm: already implicit in edition 2021 for workspace root with a [package], but be explicit

[workspace.package]
edition      = "2021"
version      = "0.1.0"
rust-version = "1.85"      # F-X06-006 — pick conservatively, validate with cargo check

[workspace.dependencies]
# Domain
sectorforge          = { package = "sector-generator", path = "." }
sectorforge-gui-core = { path = "gui-core" }

# CLI / config / serialization (used by 3+ crates)
clap        = { version = "4.5", features = ["derive"] }
serde       = { version = "1.0", features = ["derive", "rc"] }
serde_json  = "1.0"
toml        = "0.8"
thiserror   = "2"           # F-X06-012 — bump major
camino      = { version = "1.1", features = ["serde1"] }

# Hashing / RNG (root crate only, but hoisted for single source of truth)
blake3       = "1.5"
rand         = "0.8"        # KEEP 0.8 — determinism: see src/model/rng.rs
rand_chacha  = "0.3"        # KEEP 0.3 paired with rand 0.8
rustc-hash   = "2"
rayon        = "1.10"

# Imaging
image = { version = "0.25", default-features = false, features = ["png"] }

# GUI stack
eframe = { version = "0.29", default-features = false, features = ["default_fonts", "glow", "wayland", "x11"] }
egui   = "0.29"
rfd    = "0.17"

# Optional / dev
dhat       = { version = "0.3", optional = true }
tempfile   = "3"
proptest   = "1"
criterion  = { version = "0.5", default-features = false, features = ["cargo_bench_support"] }

[workspace.lints.clippy]                    # F-X06-008, F-X06-009
disallowed_types   = "deny"
disallowed_methods = "deny"
# (Optionally:)
# dbg_macro       = "warn"
# todo            = "warn"
# unimplemented   = "warn"

# Root package becomes:
[package]
name          = "sector-generator"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true

[lib]
name = "sectorforge"
path = "src/lib.rs"

[[bin]]
name = "sectorforge"
path = "src/main.rs"

[[bin]]
name = "dhat-profile"
path = "src/bin/dhat_profile.rs"
required-features = ["dhat-heap"]

[dependencies]
clap.workspace        = true
serde.workspace       = true
serde_json.workspace  = true
toml.workspace        = true
thiserror.workspace   = true
rand.workspace        = true
rand_chacha.workspace = true
blake3.workspace      = true
camino.workspace      = true
image.workspace       = true
rustc-hash.workspace  = true
rayon.workspace       = true
dhat = { workspace = true, optional = true }   # optional flag stays per-crate

[features]
default     = []
dhat-heap   = ["dep:dhat"]

[dev-dependencies]
tempfile.workspace  = true
proptest.workspace  = true
criterion.workspace = true

[[bench]]
name    = "generation"
harness = false

[lints]
workspace = true

# profiles (dev, release, bench, profiling) unchanged
```

### `builder/Cargo.toml` — replaces lines 1-29 with

```toml
[package]
name                   = "sectorforge-builder"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true

[dependencies]
sectorforge.workspace          = true
sectorforge-gui-core.workspace = true
eframe.workspace               = true
egui.workspace                 = true
rfd.workspace                  = true
serde.workspace                = true
serde_json.workspace           = true
toml.workspace                 = true
camino.workspace               = true
clap.workspace                 = true
thiserror.workspace            = true

[dev-dependencies]
tempfile.workspace = true

[lints]
workspace = true

[[bin]]
name = "sectorforge-builder"
path = "src/main.rs"
```

### `viewer/Cargo.toml` — replaces lines 1-31 with

```toml
[package]
name                   = "sectorforge-viewer"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true

[dependencies]
sectorforge.workspace          = true
sectorforge-gui-core.workspace = true
serde.workspace                = true
serde_json.workspace           = true
toml.workspace                 = true
thiserror.workspace            = true
rand.workspace                 = true     # see F-X06-005 — consider removing
clap.workspace                 = true
camino.workspace               = true
eframe.workspace               = true
egui.workspace                 = true
rfd.workspace                  = true
# image removed: F-X06-002
# tempfile dev-dep removed: F-X06-003

[lints]
workspace = true

[[bin]]
name = "sectorforge-viewer"
path = "src/main.rs"
```

### `gui-core/Cargo.toml` — replaces lines 1-13 with

```toml
[package]
name                   = "sectorforge-gui-core"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true

[dependencies]
sectorforge.workspace = true
# eframe removed: F-X06-004
egui.workspace        = true
image.workspace       = true

[dev-dependencies]
blake3.workspace = true

[lints]
workspace = true
```

### Delete (after hoisting)

- `builder/clippy.toml` — superseded by root `clippy.toml` (which should be created and contain the previous bodies, since `disallowed-types`/`disallowed-methods` paths live in `clippy.toml`, not `Cargo.toml`).
- `viewer/clippy.toml` — same.

### Net effect

- 4 manifests shrink from a combined ~145 lines to ~100 lines.
- One canonical `version = "0.29"` for the eframe/egui stack — bumping is a one-line change.
- Lint policy lives in exactly one place.
- Removes 3 confirmed-unused declarations (viewer/image, viewer/tempfile, gui-core/eframe).
- Cargo.lock is expected to change only in: (a) `thiserror` upgrade (F-X06-012, optional), (b) drop of the workspace's *direct* `image` and `tempfile` lines for viewer (transitive copies remain through `eframe` and `proptest`, so lockfile entries don't actually disappear), (c) drop of `gui-core`'s direct `eframe` declaration (still pulled by builder+viewer; lockfile unchanged).
