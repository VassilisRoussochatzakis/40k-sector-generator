# Bundled fonts (BEAUTY.md §5.5)

The builder/viewer custom typography is **wired but dormant**. The code
(`gui-core/src/fonts.rs`) embeds three faces with `include_bytes!`, gated behind
the `bundled-fonts` Cargo feature so the default build stays on egui's
`default_fonts` and never references a missing binary.

To activate it, drop **three OFL-licensed font files** in this directory with
these exact names, then build with the feature on:

| File          | Role                         | Used by                                                | Suggested OFL faces |
|---------------|------------------------------|--------------------------------------------------------|---------------------|
| `display.ttf` | Display / titles **only**    | `design::display_family()` → headings, info-panel titles | Cinzel · IM Fell English SC · Cormorant Garamond SC (gothic / inscriptional) |
| `body.ttf`    | Humanist body (primary sans) | the front of egui's `Proportional` family — every label | Inter · Source Sans 3 · Alegreya Sans |
| `mono.ttf`    | Monospace (tabular data)     | the front of egui's `Monospace` family — kv tables      | JetBrains Mono · IBM Plex Mono · Iosevka |

All suggestions are SIL Open Font License 1.1; any OFL face works — the code
references the filenames, not the face. A static (non-variable) `.ttf`/`.otf`
weight is simplest; rename it to the target filename above.

### Enable

```bash
# build / run with the bundled faces
cargo run -p sectorforge-builder --features sectorforge-gui-core/bundled-fonts
cargo run -p sectorforge-viewer  --features sectorforge-gui-core/bundled-fonts
```

To make it the default, add `bundled-fonts` to a `default = [...]` feature in
both `builder/Cargo.toml` and `viewer/Cargo.toml` (or to `gui-core`'s own
`default`). Keep the OFL license text alongside the files (commit each face's
`OFL.txt`).

> **Do not** build `--all-features` until the three files exist — `include_bytes!`
> will fail to compile without them. That is the intended contract: the feature
> means "the binaries are present."
