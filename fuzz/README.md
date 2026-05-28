# sectorforge-fuzz

TF-T-10 cargo-fuzz scaffold. Targets the public parser surfaces that take
attacker-controlled bytes (config TOML, worlds TOML, preset metadata, map
theme TOML).

## Why this crate is outside the workspace

`libfuzzer-sys` requires nightly. Including it in the root `[workspace]`
would break stable `cargo build --workspace`. The root `Cargo.toml` lists
only `viewer`, `gui-core`, `builder` as workspace members, so this crate is
opt-in.

## Usage

```bash
cargo install cargo-fuzz  # one-time
cd fuzz
cargo +nightly fuzz run config_parse
cargo +nightly fuzz run worlds_toml_parse
cargo +nightly fuzz run presets_load
cargo +nightly fuzz run map_theme_parse_color
```

Corpus, artifacts, and coverage outputs live under `fuzz/corpus/`,
`fuzz/artifacts/`, `fuzz/coverage/` (all gitignored).

## Adding a target

1. Create `fuzz_targets/<name>.rs` with a `fuzz_target!(|data: &[u8]| { ... })`
   body that calls the parser you want to harden.
2. Add a matching `[[bin]]` entry to `Cargo.toml`.
3. `cargo +nightly fuzz run <name>`.

Each target should drive a single parser entry point so reproducer minimisation
stays useful.
