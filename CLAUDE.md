# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

This is an empty Rust project (40k Sector Generator for Warhammer 40k). No `Cargo.toml` or source files exist yet — the `.gitignore` confirms Cargo/rustfmt/mutation testing (`cargo-mutants`) are part of the toolchain.

Ignore the old directory, don't ever read it or go into it.

Update GUIDE.md after every change.

## Commands

```bash
# Initialize the project (if not already done)
cargo init

# Build
cargo build

# Run tests
cargo test

# Format code
cargo fmt

# Check
cargo check

# Mutation testing (if cargo-mutants is installed)
cargo mutants
```
