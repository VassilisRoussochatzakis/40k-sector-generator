# Bundled fonts (BEAUTY.md §5.5)

The builder/viewer custom typography is **active by default**. The three faces
below are committed in this directory; `gui-core/src/fonts.rs` embeds them with
`include_bytes!`, gated behind the `bundled-fonts` Cargo feature, which both apps
turn on via their own `default` feature
(`default = ["sectorforge-gui-core/bundled-fonts"]` in `builder/Cargo.toml` and
`viewer/Cargo.toml`). Build either app with `--no-default-features` to fall back
to egui's `default_fonts`. `gui-core` keeps the feature off by default so its
map-snapshot golden suite still renders on stock egui fonts.

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
# default build = bundled faces
cargo run -p sectorforge-builder
cargo run -p sectorforge-viewer

# opt out → egui's default_fonts
cargo run -p sectorforge-builder --no-default-features
cargo run -p sectorforge-viewer  --no-default-features
```

Keep the OFL license text alongside the files (commit each face's `OFL.txt`).
