---
unit_id: U014
crate: sectorforge
paths:
  - src/export/bitmap/mod.rs
  - src/export/bitmap/legend.rs
  - src/export/bitmap/primitives.rs
  - src/export/bitmap/labels.rs
  - src/export/bitmap/routes.rs
  - src/export/bitmap/grid.rs
  - src/export/bitmap/regions.rs
  - src/export/bitmap/systems.rs
  - src/export/bitmap/colors.rs
  - src/export/bitmap/geom.rs
  - src/export/bitmap/canvas.rs
  - src/export/bitmap/tests.rs
  - src/export/svg_export/mod.rs
  - src/export/svg_export/legend.rs
  - src/export/svg_export/primitives.rs
  - src/export/svg_export/labels.rs
  - src/export/svg_export/routes.rs
  - src/export/svg_export/grid.rs
  - src/export/svg_export/regions.rs
  - src/export/svg_export/systems.rs
  - src/export/svg_export/colors.rs
  - src/export/svg_export/geom.rs
  - src/export/svg_export/canvas.rs
  - src/export/svg_export/tests.rs
loc_reviewed: 3913
reviewed_by: agent
health_score: 4
finding_counts: { critical: 0, high: 2, medium: 6, low: 8, nit: 6 }
top_risks:
  - "Bitmap renderer panics on extreme `scale` values via integer overflow / image-buf alloc (F-014-001)"
  - "SVG XML escaping silently corrupts output when user strings contain control chars (F-014-002)"
  - "Per-system rebuild of `HashSet<sys_cells>` inside subsector label loop — O(S*N) work, both backends (F-014-003)"
---

# Review: src/export — bitmap + svg_export

## Summary

Two parallel rasteriser backends behind a shared `Canvas` trait (`render_core`). The
bitmap path is a custom Bresenham/scanline rasteriser with an embedded 5×7 font; the
SVG path emits hand-written XML. The split is clean, helpers live in `colors`/`geom`
shims onto `render_core`, and determinism-sensitive ordering is driven by `Vec` walks
over `sector.systems` / `regions` / `subsectors` — `HashMap`s here are lookup tables
keyed by SystemId or `(q,r)`, never iterated for output. The hot-path issues are
concentrated in (a) `as i32`/`as u32` casts with no overflow guard at high `scale`,
(b) per-element `String` allocations in the SVG writer (`color_hex`), and (c) a
single subsector-label hoist that's currently O(subsectors × systems). XML escaping
covers the five canonical entities but not the control-char subset XML 1.0 forbids
in content/attribute values.

## Findings

### F-014-001 — [HIGH] [Panics & failure surface] Integer overflow / runaway alloc on high `scale`
- **Location:** `src/export/bitmap/geom.rs:20-37`, `src/export/bitmap/mod.rs:161-168`
- **Category:** Panics & failure surface / Resource
- **Confidence:** Med-High
- **Blast radius:** CLI callers passing large `--scale`; benches; any future "poster"
  preset.
- **Problem:** `Geom::new` does `let s = scale.max(1) as i32`. A caller-supplied
  `scale` of e.g. `u32::MAX / 2` silently casts to a negative `i32` (wrap), and
  every dependent `i32` field (`margin = 28 * s`, `legend_width`, `line_h`,
  `legend_pad`) wraps under `i32::checked_mul`. Downstream
  `total_w as u32` / `total_h as u32` in `mod.rs:168` then passes garbage to
  `RgbaImage::from_pixel(w, h, ...)`, which will either allocate many GB and
  abort, or wrap-overflow `w*h*4` inside the `image` crate.
- **Why it matters:** A single CLI typo (`--scale 100000`) or an integration with
  external config can panic the renderer with a hostile-looking error from
  `image`, not a typed `SectorError`. CLI is documented in CLAUDE.md as
  user-facing.
- **Evidence:** No `checked_mul`/`saturating_mul`/`TryFrom` anywhere in `Geom::new`
  or `render()`; `scale: u32` is the only filter.
- **Suggested fix:** Clamp `scale` to a documented maximum (e.g. 16) in
  `Geom::new`, return a typed `SectorError::InvalidScale` from `render`, and use
  `i32::try_from(scale).map_err(...)?` plus `checked_mul` on the products.
  Minimum: `let s = scale.clamp(1, MAX_SCALE) as i32;` plus a `pub const
  MAX_SCALE: u32 = 16;`.
- **Effort:** S
- **Risk of fix:** Low (clamp is byte-stable for in-range callers).

### F-014-002 — [HIGH] [Correctness / output stability] XML escaping skips control characters
- **Location:** `src/export/svg_export/primitives.rs:142-153`
- **Category:** Correctness / Security-adjacent (output validity)
- **Confidence:** High
- **Blast radius:** Every text element with user-derived content (sector ID, seed,
  faction names, region names, system names, theme name).
- **Problem:** `escape_xml_into` handles `< > & " '` but not the XML 1.0
  restricted-char set (`U+0000..U+001F` except TAB/LF/CR, plus `U+007F` and
  surrogates). If any upstream content contains e.g. a stray `\x00`, the
  resulting SVG fails XML parsing — golden tests pass today only because the
  test inputs are well-behaved. Names/seeds in this codebase come from
  generators that *should* stay ASCII, but the contract isn't enforced
  upstream.
- **Why it matters:** Soft data corruption (file is wedged, parser dies with a
  misleading "not well-formed" error far from the cause), and the same code is
  used to format both element text and attribute strings (`format!("SECTOR:
  {sector.id}")` goes through `text()` which calls `escape_xml_into`, but
  `color_hex` / `dasharray` / `text-anchor` are not escaped because they're
  static).
- **Evidence:** Read of `escape_xml_into` — no control-char branch.
- **Suggested fix:**
  ```rust
  fn escape_xml_into(out: &mut String, body: &str) {
      for c in body.chars() {
          match c {
              '<' => out.push_str("&lt;"),
              '>' => out.push_str("&gt;"),
              '&' => out.push_str("&amp;"),
              '"' => out.push_str("&quot;"),
              '\'' => out.push_str("&apos;"),
              // XML 1.0 forbids U+0000..U+001F except TAB(09)/LF(0A)/CR(0D).
              '\t' | '\n' | '\r' => out.push(c),
              c if (c as u32) < 0x20 || c == '\u{7f}' => out.push('\u{fffd}'),
              _ => out.push(c),
          }
      }
  }
  ```
  Add a proptest in `svg_export/tests.rs` that any roundtrip through
  `render_sector_svg` parses with `quick-xml` or `roxmltree`.
- **Effort:** S
- **Risk of fix:** Low (current well-behaved inputs unaffected — golden bytes
  unchanged).

### F-014-003 — [MEDIUM] [Performance] `sys_cells` HashSet rebuilt per subsector
- **Location:** `src/export/bitmap/labels.rs:162-168` and
  `src/export/svg_export/labels.rs:162-167`
- **Category:** Performance / Hot path — per-export, scales with subsectors
- **Confidence:** High
- **Blast radius:** Sector export with many subsectors (default sector has ~16);
  not GUI hot path but called on every export.
- **Problem:** Inside `for sub in subsectors { ... }` the code rebuilds
  `let sys_cells: HashSet<(i32, i32)> = sector.systems.iter().map(|sys|
  (sys.coord.q, sys.coord.r)).collect();` — purely a function of `sector`,
  not `sub`. This is O(S × N) hash inserts where it should be O(S + N).
- **Why it matters:** Cheap to fix and called per export; on a 32×32 sector with
  20 subsectors this rebuilds ~3200 hash slots that never change.
- **Evidence:** Direct read.
- **Suggested fix:** Hoist before the loop:
  ```rust
  let sys_cells: HashSet<(i32, i32)> = sector
      .systems
      .iter()
      .map(|sys| (sys.coord.q, sys.coord.r))
      .collect();
  for sub in subsectors { ... }
  ```
- **Effort:** XS
- **Risk of fix:** None — same bytes (set used only in `.contains()`).

### F-014-004 — [MEDIUM] [Performance] `color_hex` allocates a fresh `String` per SVG primitive
- **Location:** `src/export/svg_export/primitives.rs:8-10`
- **Category:** Performance / Hot path — per element
- **Confidence:** High
- **Blast radius:** Every `<rect>` / `<circle>` / `<polygon>` / `<line>` / `<text>`
  emission. For a default sector this is on the order of `width × height × 2 +
  routes + systems × 3` heap allocations of a 7-byte string, all immediately
  formatted into the master buffer and dropped.
- **Problem:** `format!("#{:02x}{:02x}{:02x}", ...)` allocates. Every caller
  passes the returned `String` into a `write!(s, ..., f = color_hex(...))`
  invocation, so the temporary string just round-trips through the heap.
- **Why it matters:** Pure allocator churn in a hot loop, and the eventual
  benchmark in §3.6 of the rubric calls this out specifically.
- **Evidence:** `color_hex` returns owned `String`; every primitive calls it 1-2x.
- **Suggested fix:** Write directly into the output buffer; the format spec is
  fixed-length so no temporary is needed:
  ```rust
  // Replace color_hex with an inline write.
  fn write_color(s: &mut String, c: Rgba<u8>) {
      let _ = write!(s, "#{:02x}{:02x}{:02x}", c.0[0], c.0[1], c.0[2]);
  }
  // Then in rect():
  s.push_str(r#"<rect x=""#);
  let _ = write!(s, "{x:.2}");
  s.push_str(r#"" ... fill=""#);
  write_color(s, fill);
  ```
  Or, simpler, replace `color_hex` with an `Display`-implementing adapter
  newtype so existing `{f}` formatters keep working without the alloc.
- **Effort:** S
- **Risk of fix:** None — output bytes identical, golden tests confirm.

### F-014-005 — [MEDIUM] [Performance] `HashMap::new()` (SipHash) for per-export lookup tables
- **Location:** `src/export/bitmap/grid.rs:28,63,117`,
  `src/export/svg_export/grid.rs:27,62`, `src/export/svg_export/mod.rs:86`
- **Category:** Performance / Per-export
- **Confidence:** Med
- **Blast radius:** Each export builds three small `HashMap`s. SipHash is
  comparatively slow vs `FxHashMap`, and keys are tiny (`(i32,i32)` or a
  fixed-size `SystemId`). With `with_capacity` we'd also save the rehash
  pass.
- **Problem:** No `with_capacity`; using stdlib `HashMap` (SipHash) when the
  workspace already exposes `FxHashMap` aliases in `src/lib.rs` for exactly
  this case. CLAUDE.md note: Fx is for *lookup* only — these are lookup
  tables, so Fx is correct.
- **Why it matters:** Determinism is unaffected (no iteration for output);
  this is a pure win. Per-export cost is small individually but rasterisation
  benches will see it.
- **Suggested fix:** Replace `HashMap::new()` with
  `crate::FxHashMap::default()` (or `FxHashMap::with_capacity_and_hasher(n,
  Default::default())` where `n = sector.systems.len()` /
  `sector.regions.iter().map(|r| r.hexes.len()).sum()`). Update parameter
  types on `compute_*` accordingly. Verify with `cargo test --test it --
  golden` — should be a no-op for output.
- **Effort:** S
- **Risk of fix:** Low (only affects iteration order, which isn't observed
  externally).

### F-014-006 — [MEDIUM] [Performance] `String::with_capacity(64 KiB)` undersized for non-trivial sectors
- **Location:** `src/export/svg_export/mod.rs:70`
- **Category:** Performance / Per-export
- **Confidence:** Med
- **Blast radius:** Every SVG export of a default-size or larger sector.
- **Problem:** A 32×32 sector with ~150 systems and routes commonly produces
  several hundred KB of SVG (the per-element line is ~150-250 bytes). The
  initial 64 KiB capacity guarantees several `String` reallocations during
  emission.
- **Why it matters:** Each realloc copies the entire accumulated buffer.
- **Suggested fix:** Either pre-size from sector dimensions
  (`sector.width * sector.height * 256 + sector.systems.len() * 400`), or
  pick a saner default like 256 KiB. Verify with a `criterion` bench in
  `benches/` if available.
- **Effort:** XS
- **Risk of fix:** None.

### F-014-007 — [MEDIUM] [Idiomatic Rust / Performance] `to_uppercase()` allocates per faction row, per render
- **Location:** `src/export/bitmap/legend.rs:92,124,305,313,413,419`,
  `src/export/svg_export/legend.rs:83,119,304,312,419,425`,
  `src/export/bitmap/labels.rs:110,143`,
  `src/export/svg_export/labels.rs:117,144`,
  `src/export/bitmap/regions.rs:75`, `src/export/svg_export/regions.rs:27`
- **Category:** Performance / Per-element
- **Confidence:** Med
- **Blast radius:** Per-render; `to_ascii_uppercase` would avoid the
  Unicode-aware path entirely.
- **Problem:** `name.to_uppercase()` walks the string with full Unicode
  tables; `to_ascii_uppercase` is a byte-loop. For ASCII inputs (which the
  generator produces) the output is identical.
- **Why it matters:** Bytewise-identical output, materially cheaper. The
  bitmap font itself can only render ASCII (see `glyph()` returning `_` for
  non-ASCII).
- **Suggested fix:** s/`to_uppercase()`/`to_ascii_uppercase()`/ at every
  cited site. Keep the SVG sites consistent with bitmap.
- **Effort:** XS
- **Risk of fix:** None for ASCII inputs. If a non-ASCII name slips in, the
  bitmap font already drops it; SVG would render it but in original case —
  arguably more correct. Confirm golden bytes (will only diverge if a test
  fixture contains non-ASCII names).

### F-014-008 — [MEDIUM] [Panics & failure surface] `sector.height.saturating_sub(1)` swallows underflow but other geometry math doesn't
- **Location:** `src/export/bitmap/geom.rs:54-60`,
  `src/export/svg_export/geom.rs:21-24`
- **Category:** Panics / Correctness
- **Confidence:** Low-Med
- **Blast radius:** Niche — `sector.width = 0` or `height = 0`.
- **Problem:** `map_bounds` uses `saturating_sub(1)` for `height` (good) but
  `sector.width` is multiplied directly, and an empty sector produces a
  positive bound only because of `margin*2 + 0`. More importantly, the
  resulting `as i32` cast can produce 0 or negative if `margin*2` is small
  relative to `0`. Not unsound but unclear: `width = 0` returns a 56-pixel-wide
  image with no content, no error.
- **Why it matters:** Zero-sized sector silently produces an image-only legend.
  Probably intended; document it.
- **Suggested fix:** Add a `debug_assert!(sector.width > 0 && sector.height > 0)`
  at the top of `map_bounds`, or return a typed error before reaching the
  rasteriser if the sector is empty.
- **Effort:** XS
- **Risk of fix:** None.

### F-014-009 — [LOW] [Idiomatic Rust] Awkward two-step `bot_owned` shadowing
- **Location:** `src/export/bitmap/labels.rs:137-145`
- **Category:** Style / Readability
- **Confidence:** High
- **Blast radius:** Readability only.
- **Problem:** The current idiom
  ```rust
  let bot_owned: String;
  let bot: &str = {
      let raw = s.name.strip_prefix("Subsector ").unwrap_or_else(|| s.name.as_ref());
      bot_owned = raw.to_ascii_uppercase();
      bot_owned.as_str()
  };
  ```
  exists only because the original `raw` borrow would have made `bot` keep
  borrowing from `s.name`. After the rewrite to `to_ascii_uppercase` (which
  always returns an owned `String`), the two-step dance is unnecessary.
- **Suggested fix:**
  ```rust
  let bot_owned = s
      .name
      .strip_prefix("Subsector ")
      .unwrap_or(s.name.as_ref())
      .to_ascii_uppercase();
  let bot: &str = &bot_owned;
  ```
- **Effort:** XS
- **Risk of fix:** None.

### F-014-010 — [LOW] [Panics] `.expect("non-empty")` in subsector-label fallback relies on caller invariant
- **Location:** `src/export/bitmap/labels.rs:247`,
  `src/export/svg_export/labels.rs:240`
- **Category:** Panics / Documentation
- **Confidence:** High
- **Blast radius:** Internal — the `continue` guard at the top of the loop
  already filters empty subsectors.
- **Problem:** `min_by_key(..)` on `s.hex_cells.iter()` — if the upstream
  guard ever changes, this panics. The `.expect` message is unhelpful.
- **Suggested fix:** Either `unreachable!("subsector.hex_cells must be non-empty;
  guarded at line {N}")` for clarity, or restructure with `let Some(...) =
  ... else { continue }` so the invariant is local.
- **Effort:** XS
- **Risk of fix:** None.

### F-014-011 — [LOW] [Idiomatic Rust] `pts.iter().map(|p| p.1).min()/.max()` walks twice in `fill_polygon`
- **Location:** `src/export/bitmap/primitives.rs:211-220`
- **Category:** Performance / Idiom
- **Confidence:** High
- **Blast radius:** Per-polygon (modest — region tints, hex grid not via this
  path).
- **Problem:** Two separate iterator chains walk `pts` to find `ymin` and
  `ymax`, plus the two `.expect()` calls duplicate the non-empty guard.
- **Suggested fix:**
  ```rust
  let (ymin, ymax) = pts
      .iter()
      .map(|p| p.1)
      .fold((i32::MAX, i32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
  ```
  or simply `pts.iter().minmax_by_key(...)` if `itertools` is in tree. The
  early `is_empty()` guard then guarantees the fold initialiser isn't observed.
- **Effort:** XS
- **Risk of fix:** None.

### F-014-012 — [LOW] [Performance] `fill_polygon` re-collects `xs` per scanline; the inner intersection scan does the same work for every `y` in `ymin..=ymax`
- **Location:** `src/export/bitmap/primitives.rs:207-240`
- **Category:** Performance / Hot path for hex grid drawing
- **Confidence:** Med
- **Blast radius:** Hex grid rasterisation — called per hex.
- **Problem:** For each scanline, we re-iterate every edge. With convex
  polygons (hexes), an edge-table / active-edge-list cuts this to O(edges +
  scanlines) instead of O(edges × scanlines). For 6-vert hexes the speedup
  is small; for arbitrary polygons it matters.
- **Why it matters:** The hot caller is `draw_hex_grid` over every cell.
  At default scale and 32×32 grids this is ~6k scanlines × 6 edges each.
- **Suggested fix:** Specialise hex fill to a Bresenham-style edge walk if
  this shows up in benches. Cheaper interim: keep `xs` allocated outside
  the loop (already done via `xs.clear()`) but inline the edge-y bounds
  check `if ay > y && by > y` to skip non-crossing edges earlier.
- **Effort:** M (full edge-table) / XS (inline early-out)
- **Risk of fix:** Low; output bytes unchanged.

### F-014-013 — [LOW] [Idiomatic Rust] `compute_region_tints` duplicated verbatim across both backends
- **Location:** `src/export/bitmap/grid.rs:113-135`,
  `src/export/svg_export/grid.rs:57-80`
- **Category:** Maintainability
- **Confidence:** High
- **Blast radius:** Two-place edits any time a `RegionConditionKind` is added —
  bitmap renders the new tint, SVG silently doesn't (or vice versa).
- **Problem:** The base RGB table per `RegionConditionKind` is the same in
  both files. Belongs in `render_core::colors` or a new
  `render_core::regions::base_tint(kind)` function.
- **Suggested fix:** Move the `match region.kind { ... }` table into
  `crate::export::render_core::colors::region_base_rgba(kind: RegionConditionKind) ->
  Rgba<u8>`. Both backends call it; the `tint_against` and `out.insert` loops
  stay backend-side.
- **Effort:** XS
- **Risk of fix:** None — pure refactor, golden bytes unchanged.

### F-014-014 — [LOW] [Idiomatic Rust] `obstacles.iter().chain(placed.iter())` repeated linear scan inside `try_place`
- **Location:** `src/export/bitmap/labels.rs:216-220`,
  `src/export/svg_export/labels.rs:211-215`
- **Category:** Performance / Subsector label placement
- **Confidence:** Med
- **Blast radius:** Quadratic in `(systems + subsectors) * candidates`.
- **Problem:** Each `try_place` call scans every obstacle linearly. For
  sectors with hundreds of systems this dominates subsector label placement
  time.
- **Suggested fix:** Either keep this acceptable (subsector counts are
  typically < 20), or build an interval/grid index over `obstacles.y0..y1`
  if it shows up in benches. Acceptable as-is — flagging for visibility.
- **Effort:** M
- **Risk of fix:** Low.

### F-014-015 — [LOW] [Error handling] `let _ = write!(..)` discards `fmt::Error` from `String` writers
- **Location:** `src/export/svg_export/primitives.rs:25,32,51,58,80,82,89,110,118,132`,
  `src/export/svg_export/mod.rs:71`
- **Category:** Error handling / Style
- **Confidence:** High
- **Blast radius:** None at runtime — `<String as fmt::Write>::write_str` is
  infallible.
- **Problem:** `let _ = write!(s, ...);` is correct (the `Result` is always
  `Ok`), but the pattern leaks across a large surface and obscures intent.
- **Suggested fix:** Define a tiny macro `macro_rules! w { ($s:expr, $($t:tt)*)
  => { let _ = write!($s, $($t)*); } }` and use `w!(s, "<rect x=\"{x}\"...")`,
  or just `s.write_fmt(format_args!(...)).unwrap()` once via a helper. Pure
  style.
- **Effort:** XS
- **Risk of fix:** None.

### F-014-016 — [LOW] [Idiomatic Rust / API] `BitmapCanvas::polygon` allocates `Vec<(i32,i32)>` per polygon
- **Location:** `src/export/bitmap/canvas.rs:82-103`
- **Category:** Performance / Per-polygon
- **Confidence:** Med
- **Blast radius:** Per hex on the grid.
- **Problem:** `let ipts: Vec<(i32, i32)> = pts.iter().map(...).collect();`
  allocates per call. With 32×32 hexes this is ~1000 short-lived allocs
  per render.
- **Suggested fix:** Either keep a scratch `Vec` on `BitmapCanvas` (clear()
  + extend()), or change `fill_polygon` to take an iterator + accept f32
  vertices directly with `.round() as i32` inside the scan-fill loop.
- **Effort:** S
- **Risk of fix:** Low — same rounding pipeline, bytes unchanged.

### F-014-017 — [NIT] [Idiomatic Rust] `match opts.route_view_mode` duplicated between SVG and bitmap legend bodies
- **Location:** `src/export/bitmap/legend.rs:138-185`,
  `src/export/svg_export/legend.rs:135-176`
- **Category:** Maintainability
- **Confidence:** Med
- **Blast radius:** Adding a new `RouteViewMode` variant requires four edits.
- **Suggested fix:** Hoist iteration to a helper that yields `(pattern,
  label)` tuples to a backend-supplied closure.
- **Effort:** S
- **Risk of fix:** Low.

### F-014-018 — [NIT] [Documentation] No `# Errors` on `write_sector_png_to` / `write_sector_png_to_with`
- **Location:** `src/export/bitmap/mod.rs:98-117`
- **Category:** Documentation
- **Confidence:** High
- **Suggested fix:** Add `# Errors` doc blocks identical in shape to the SVG
  module's `write_sector_svg_to` blocks (which already document `SectorError::Io`).
- **Effort:** XS
- **Risk of fix:** None.

### F-014-019 — [NIT] [Idiomatic Rust] `#[allow(unused_imports)]` over a `pub(crate) use` block in `bitmap/mod.rs`
- **Location:** `src/export/bitmap/mod.rs:39-43`
- **Category:** Hygiene
- **Confidence:** Med
- **Problem:** The blanket `#[allow]` masks unused re-exports if the
  consumer (`system_map`) drops a primitive.
- **Suggested fix:** Either remove the `#[allow]` and let clippy drive
  pruning, or split into the actually-used vs. re-exported-for-consumers
  groups so dead re-exports surface.
- **Effort:** XS
- **Risk of fix:** Low.

### F-014-020 — [NIT] [Style] Magic numbers in legend layout (`4 * g.scale`, `8 * g.scale`, `30 * g.scale`)
- **Location:** `src/export/bitmap/legend.rs:142-208` and through the file
- **Category:** Documentation / Maintainability
- **Confidence:** High
- **Problem:** Layout offsets are embedded as repeated `K * g.scale`
  expressions; mirrored in SVG legend as `30.0 / 38.0 / 22.0`. Hard to
  audit when adjusting spacing.
- **Suggested fix:** Add a small `LegendLayout` const struct (or `const`
  block) with named fields (`SWATCH_OFFSET`, `STROKE_OFFSET`, `STAB_LINE_LEN`,
  `STAB_LABEL_X`, `GAP_AFTER_SECTION`). Share between backends via
  `render_core::legend_layout`.
- **Effort:** S
- **Risk of fix:** None.

### F-014-021 — [NIT] [Testing] Inline tests are smoke-only
- **Location:** `src/export/bitmap/tests.rs`, `src/export/svg_export/tests.rs`
- **Category:** Testing & verification
- **Confidence:** High
- **Problem:** The inline tests confirm "does not panic" and "scaled output is
  larger" but not, e.g., that `render` is determinism-stable across hashing
  seed perturbations, or that the SVG parses with an XML parser. Golden
  coverage lives in `tests/it/` (U022's scope), so this is a NIT here.
- **Suggested fix:** Add (a) a `proptest!` smoke run with random small
  sector dimensions to catch panic regressions in `map_bounds`/glyph code,
  and (b) a `roxmltree::Document::parse` assertion on the SVG output.
- **Effort:** S
- **Risk of fix:** None.

## Rubric coverage

- **3.1 Panics & failure surface:** F-014-001, F-014-008, F-014-010.
  `unwrap_unchecked`/`get_unchecked` absent. `put_pixel`/`fill_row`/`fill_rect`
  all clamp bounds before slicing — no OOB. `fill_circle`/`draw_ring` guard
  negative radius. `draw_line`/`draw_line_thick` are pure integer Bresenham
  with no overflow path inside.
- **3.2 unsafe & soundness:** No unsafe in this slice. No findings.
- **3.3 Ownership/borrowing/clones:** F-014-007 (`to_uppercase` allocations),
  F-014-009 (awkward shadow), F-014-016 (per-polygon Vec). Otherwise the code
  is borrow-friendly — `&MapTheme`, `&Geom`, `&RenderOptions` everywhere.
- **3.4 Error handling:** F-014-015. `save_png_fast` correctly maps
  `io::Error` -> `SectorError::export` and `image::ImageError` -> `SectorError::export`.
  `write_sector_svg_to_with` does the same for `fs::write`. No swallowed errors.
- **3.5 Concurrency & async:** N/A — no threading in render path.
- **3.6 Performance:** F-014-003, F-014-004, F-014-005, F-014-006, F-014-007,
  F-014-011, F-014-012, F-014-014, F-014-016. Encoder is `CompressionType::Fast`
  + `FilterType::NoFilter` for PNG (already tuned for export speed; documented in
  `mod.rs:55`). `fill_rect` already uses `copy_within` to broadcast the first
  row — good. `fill_row` uses `chunks_exact_mut(4)` — good.
- **3.7 Idiomatic Rust & API design:** F-014-007, F-014-009, F-014-013, F-014-017,
  F-014-019. Naming RFC-430 compliant. `#[must_use]` present on
  `render_sector_svg` and `render_sector_image`.
- **3.8 Dependencies & Cargo hygiene:** No unused imports observed in this
  slice (clippy would have flagged). `image` features look right for what
  the encoder does. No findings.
- **3.9 Memory & resource management:** PNG encoder takes a
  `BufWriter<File>` — `save_png_fast` correctly drops the writer at end of
  scope, file closes deterministically. `encode_png_bytes` writes to a
  `Vec<u8>` sized to `as_raw().len() / 4` — undersized (PNG output may
  exceed `raw_len / 4` for some images), but only a perf hint; encoder
  grows the buffer correctly. The big in-memory `RgbaImage` (`total_w *
  total_h * 4` bytes) is held until PNG encode finishes; with high `scale`
  this can be GBs (overlap with F-014-001). No streaming option — but the
  encoder is one-shot so chunked output would require a different `image`
  API. Acceptable trade-off given the use case.
- **3.10 Testing & verification:** F-014-021. Inline tests are minimal.
- **3.11 Documentation & maintainability:** F-014-018 (missing # Errors),
  F-014-020 (magic numbers). Module-level docstrings are present and
  high-quality across all files.

## Determinism check

Walked every `HashMap` / `HashSet` use:

- `compute_system_tints`, `compute_region_tints`, `compute_heatmap` return
  `HashMap`s that are consumed by `render_core::grid::draw_hex_grid`, which
  iterates `sector.systems` / per-cell. The map is lookup-only — no
  iteration drives output.
- `labels::draw_subsector_labels` builds `HashSet<(i32,i32)>` for
  `cells.contains(...)` and `sys_cells.contains(...)` — lookup-only.
- `obstacles: Vec<Rect>`, `placed: Vec<Rect>`, `cands: Vec<...>` are
  ordered `Vec`s; sort keys are deterministic (squared distance, then
  candidate order which derives from `sub.hex_cells` order — owned by
  `Subsector`).

No determinism violations found in this slice. Recommend the codebase add
a `#![deny(clippy::iter_over_hash_type)]` lint at workspace level (clippy
4.x has this) to keep the invariant enforced — note for the X-cutting unit.

## Summary of suggested fixes

- F-014-001 — HIGH — Clamp `scale` and validate map dims before alloc — S / Low
- F-014-002 — HIGH — Escape XML control chars in `escape_xml_into` — S / Low
- F-014-003 — MEDIUM — Hoist `sys_cells` HashSet out of subsector loop (both backends) — XS / None
- F-014-004 — MEDIUM — Inline `color_hex` write into output buffer (no temp String) — S / None
- F-014-005 — MEDIUM — Switch lookup `HashMap`s to `FxHashMap` with capacity — S / Low
- F-014-006 — MEDIUM — Pre-size SVG output `String` from sector dimensions — XS / None
- F-014-007 — MEDIUM — Replace `to_uppercase()` with `to_ascii_uppercase()` — XS / None
- F-014-008 — MEDIUM — Assert/error on zero-dim sectors in `map_bounds` — XS / None
- F-014-009 — LOW — Collapse two-step `bot_owned` shadow in bitmap labels — XS / None
- F-014-010 — LOW — Make `.expect("non-empty")` an `unreachable!` with reason — XS / None
- F-014-011 — LOW — Single-pass `(ymin, ymax)` fold in `fill_polygon` — XS / None
- F-014-012 — LOW — Edge-list / early-out optimisation in `fill_polygon` — M / Low
- F-014-013 — LOW — Move `RegionConditionKind` tint table into `render_core` — XS / None
- F-014-014 — LOW — Index obstacles spatially if subsector counts grow — M / Low
- F-014-015 — LOW — Wrap repeated `let _ = write!(...)` in a `w!` macro — XS / None
- F-014-016 — LOW — Scratch `Vec` for `BitmapCanvas::polygon` int conversion — S / Low
- F-014-017 — NIT — Hoist `RouteViewMode` legend iteration into helper — S / Low
- F-014-018 — NIT — Add `# Errors` docs to `write_sector_png_to*` — XS / None
- F-014-019 — NIT — Remove blanket `#[allow(unused_imports)]` in bitmap/mod.rs — XS / Low
- F-014-020 — NIT — Named constants for legend layout offsets — S / None
- F-014-021 — NIT — Property/XML-parser-validated tests in inline suites — S / None
