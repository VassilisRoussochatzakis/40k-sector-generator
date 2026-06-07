# sectorforge

A deterministic Warhammer 40,000 star-sector generator. It reads a project
directory of typed TOML configuration files and produces a reproducible sector
as JSON, Markdown, and bitmap images — same inputs always yield the same output.
The workspace bundles the generation engine and CLI together with a full-featured
graphical builder for constructing sectors and a viewer for inspecting and making
limited in-place edits.

## Prerequisites

You need **Rust**, installed via [rustup](https://rustup.rs/) (any 1.70+ toolchain
works; the project's MSRV is 1.87). Platform notes:

- **macOS:** install the Xcode Command Line Tools first (`xcode-select --install`),
  then run the rustup installer.
- **Linux (Ubuntu/Debian):**
  `sudo apt install pkg-config libx11-dev libxcb1-dev libxi-dev libxinerama-dev libxcursor-dev libxrandr-dev`
- **Windows:** run the rustup installer; it includes a Rust-compatible MSVC build
  toolchain.

See [GUIDE.md](GUIDE.md) §0 for the full prerequisites walkthrough.

## Quick start

From the repository root:

```bash
# Build all binaries (CLI + builder + viewer)
cargo build

# CLI help
cargo run --bin sectorforge -- --help

# Launch the interactive sector builder
cargo run -p sectorforge-builder

# Launch the viewer/editor
cargo run -p sectorforge-viewer
```

Example projects live under `examples/` as plain on-disk directories (they are
**not** embedded in the binaries). Their `--project` paths are CWD-relative, so
run these from the repository root with `--project examples/<name>`:

```bash
# Validate and generate the reference M42 example project
cargo run --bin sectorforge -- validate --project examples/m42_project
cargo run --bin sectorforge -- generate --project examples/m42_project --allow-warnings
```

## Workspace members

| Crate | Purpose |
|---|---|
| `sectorforge` | Domain model, generation, analysis, exports, and CLI (lib + binary). |
| `sectorforge-builder` | Egui editor for full sector construction. |
| `sectorforge-viewer` | Egui viewer with limited in-place editing (map/faction/world edits, `worlds.toml` data editor, save/save-as). |
| `sectorforge-gui-core` | Shared egui widgets (`SectorView`, palette, info panel). |

## Documentation

- [GUIDE.md](GUIDE.md) — full user guide (prerequisites, CLI, config, exports).
- [BUILDER.md](BUILDER.md) — step-by-step walkthrough of the builder UI.
- [docs/MAP.md](docs/MAP.md) — file-by-file map of the codebase.
- [CLAUDE.md](CLAUDE.md) — contributor / agent working conventions.
