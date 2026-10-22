# Bundled fonts (BEAUTY.md §5.5)

The builder/viewer custom typography is **always on**. The three faces below are
committed in this directory and `gui-core/src/fonts.rs` always embeds them with
`include_bytes!` (no feature flag). The gui-core map-snapshot golden suite never
calls `fonts::install`, so it still renders on stock egui fonts.

The three committed faces (swap one by dropping a new file with the same name):

| File          | Role                         | Used by                                                | Suggested OFL faces |
|---------------|------------------------------|--------------------------------------------------------|---------------------|
| `display.ttf` | Display / titles **only**    | `design::display_family()` → headings, info-panel titles | Cinzel · IM Fell English SC · Cormorant Garamond SC (gothic / inscriptional) |
| `body.ttf`    | Humanist body (primary sans) | the front of egui's `Proportional` family — every label | Inter · Source Sans 3 · Alegreya Sans |
| `mono.ttf`    | Monospace (tabular data)     | the front of egui's `Monospace` family — kv tables      | JetBrains Mono · IBM Plex Mono · Iosevka |

All suggestions are SIL Open Font License 1.1; any OFL face works — the code
references the filenames, not the face. A static (non-variable) `.ttf`/`.otf`
weight is simplest; rename it to the target filename above.

### Build

```bash
cargo run -p sectorforge-builder
cargo run -p sectorforge-viewer
```

Keep the OFL license text alongside the files (commit each face's license as
`<face>OFL.txt`, e.g. `displayOFL.txt`).
