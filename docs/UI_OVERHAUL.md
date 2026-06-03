# UI_OVERHAUL — Sectorforge GUI overhaul playbook

> **Audience: a future Claude Code instance executing this work.** This is an
> execution spec, not a pitch. It encodes the *current* state (with
> `path:line` evidence), the target design system, and step-by-step
> instructions precise enough to hand to a subagent. Reference sections as
> `§UO<n>` in commits and comments, the same way the repo references
> `§<tag>` from `docs/`.
>
> **Scope of the change:** *chrome and layout only* — type scale, section
> containers, dropdown sizing, spacing, separators, tab navigation. It must
> **not** touch the semantic map render or the export writers (see §UO8).
>
> **Status:** Phases 0–5 LANDED — theme type-scale/sizing, shared `ui_kit`,
> builder shell + clustered tabs, every builder panel, the viewer, and the P5
> empty-state foundation are all in, verified clippy-clean and golden-stable.
> Remaining: the screenshot-driven spacing/tooltip eyeball pass (§UO9), which
> needs the app running. Progress checklist in §UO12.

---

## §UO0 — TL;DR

The recent themeable-chrome work (`gui-core/src/theme.rs`, commits `3b992cc`,
`c84b101`) gave both apps a coherent color story through a single chokepoint:
`Theme::apply(ctx)`. That chokepoint is the lever for the rest of the overhaul.
Three things are still wrong and all three are fixable from a small number of
shared places:

1. **Text is small and flat.** `theme.rs` styles colors and spacing but never
   sets `style.text_styles`, so every default-styled `ui.label`/button/combo
   inherits egui's dense default type scale. → Fix once in the theme (§UO3.1,
   §UO5.1). Biggest single win, ~10 lines.
2. **Nothing groups related controls.** ~50 builder panels stack widgets under
   bare `CollapsingHeader` + thin `ui.separator()`; only ~16 `Frame`s exist in
   the whole builder. The eye can't find section boundaries. → Add a shared
   *section container* widget and migrate panels to it (§UO3.4, §UO5.2).
3. **Dropdowns are tiny and inconsistent.** 117 `ComboBox` call sites in the
   builder, almost none set width or font; the viewer is split between styled
   (`editor/ui_helpers.rs`) and unstyled combos. → A global combo width/height
   bump in the theme + one shared `combo()` helper (§UO3.3, §UO5.2).

Execution is phased (§UO6) so each phase compiles, passes tests, and is
independently shippable. Phase 0 alone (theme type scale + sizing) visibly
improves *every* screen for ~30 lines of change.

---

## §UO1 — Current-state audit (evidence)

### Architecture recap (where the levers are)

| Concern | Single source of truth | Read by |
|---|---|---|
| Chrome colors (panel/bg/text) | `gui-core/src/palette.rs` → `ChromeColors` + `chrome_bg/panel/text/text_dim()` (palette.rs:30, 69-92) | custom painters, info_panel, viewer chrome frames |
| egui `Style`/`Visuals` (widget look, rounding, shadow, spacing) | `gui-core/src/theme.rs` → `Theme::apply` → `build_visuals` (theme.rs:263) + `tune_spacing` (theme.rs:338) | **both apps, every widget** |
| Theme selection + push | `Theme::apply(ctx)` called from `builder/src/app.rs:43` and the viewer app | — |
| Good text helpers (private) | `gui-core/src/info_panel.rs` → `mono/title/section/body/dim/kv` (info_panel.rs:825-876) | only `info_panel.rs` |
| Good combo helpers (local) | `viewer/src/editor/ui_helpers.rs` → `mono`, `combo_str`, `combo_kv` (ui_helpers.rs:7) | only the viewer editor |

**Key fact:** `theme.rs::tune_spacing` (theme.rs:338-343) sets `item_spacing`,
`button_padding`, `menu_margin`, `window_margin` — but **never sets
`style.text_styles` and never touches `spacing.combo_width` /
`spacing.interact_size`.** That omission is the root cause of pain points #1
and #3.

### Pain point (a) — dropdowns small & inconsistent

- 117 `ComboBox` calls in `builder/src/`; representative defaults with no width
  or font: `builder/src/builder/panels/control.rs:211` (influence tier),
  `builder/src/builder/panels/history.rs:335` (event state — long enum names
  clipped), `builder/src/builder/panels/routes.rs:149` (system picker).
- Viewer is inconsistent: unstyled `.monospace()` combos at
  `viewer/src/app/sector_view.rs:272`, `viewer/src/app/export_ui.rs:86`,
  `viewer/src/app/planner_view.rs:420` — vs. the **correct** explicit-font
  pattern at `viewer/src/data_editor.rs:313` and
  `viewer/src/editor/ui_helpers.rs:7` (`.font(mono(12.0))`).

### Pain point (b) — no section boundaries

- ~50 panels use `CollapsingHeader` + `ui.separator()` and nothing else.
  Representative flat panels: `builder/src/builder/panels/briefing.rs:85-101`,
  `builder/src/builder/panels/system.rs:81-121` (15 collapsing headers, each
  opening to a flat widget stack), `builder/src/builder/panels/world.rs:60-92`
  (13 headers).
- Only ~16 `Frame`/`group` uses exist across the builder. The best existing
  examples to imitate: `builder/src/builder/panels/relations.rs:331,338,375,421`
  (rows wrapped in `Frame::group`) and `builder/src/builder/panels/control.rs:190`
  (presence rows). Even these box *rows*, not *sections*.
- `gui-core/src/info_panel.rs` is spacing-only: 30+ `ui.add_space(8.0)` calls,
  **zero** `ui.separator()` or `Frame`. Dense monospace blocks read as one wall.

### Pain point (c) — fonts small

- egui default type scale is untouched by the theme, so default-styled text is
  the dense egui default (~12.5px body, ~14px heading) everywhere outside
  `info_panel.rs`.
- `info_panel.rs` hardcodes its own scale (`mono(18)` title, `mono(13)` section,
  `mono(12)` body/kv — info_panel.rs:833-873). It's readable but isolated; no
  other panel benefits and it can't follow a theme-wide change.
- Micro-text everywhere: `.small()` buttons, gray `.monospace()` IDs with no
  explicit size (e.g. `control.rs:229`, `system.rs:165`, `history.rs:195`).

### Navigation pain (bonus, addressed in §UO6 P2)

- Builder top strip: **24 tabs** in a single `horizontal_wrapped` row of
  `selectable_label`s (`builder/src/builder/panels/nav.rs:47-52`). No grouping,
  wraps unpredictably, active tab only weakly distinguished.
- Viewer top bar (`viewer/src/app/layout.rs:34-126`) is better — it already uses
  a chrome `Frame` and `ui.separator()` between clusters — and is the model to
  copy.

---

## §UO2 — Design principles (the three laws)

1. **The theme is the only global lever.** Anything that should look consistent
   across every screen — type scale, control height, combo width, default
   spacing, rounding — is set **once** in `gui-core/src/theme.rs` so it lands in
   both apps and stays correct when the user switches presets. Never hardcode a
   chrome color or font size in a panel; read `palette::chrome_*` and the active
   `Style`.
2. **Three visual tiers, always.** (1) *App chrome* — tab strip, status bar,
   window frames. (2) *Section containers* — titled, framed, bordered boxes that
   group related controls (this tier is what's missing today). (3) *Fields* —
   aligned label + control rows inside a section. Every panel should read as
   tier-2 boxes containing tier-3 rows.
3. **Do not touch the map or the exports.** Faction/hazard/route colors, the
   custom painters (`sector_view`, `system_view`, `palette::draw_*`), the map
   canvas consts (`palette::BG`/`PANEL_BG`/`HEX_*`), `map_theme`, and the export
   writers (`bitmap`, `svg_export`, `html_export`, `render`) are **out of
   scope** and must stay byte-stable. The overhaul is chrome + panel layout
   only. See §UO8 for the guardrail and why.

---

## §UO3 — The design system (tokens)

All values below are *starting* values; tune during P0 with the app running.
They are deliberately a notch larger than egui defaults — that's the point.

### §UO3.1 Type scale (set in the theme via `style.text_styles`)

| `TextStyle` | Family | Size | Used for |
|---|---|---|---|
| `Heading` | Proportional | **20.0** | `ui.heading()` panel titles |
| `Body` | Proportional | **15.0** | default `ui.label`, most text |
| `Button` | Proportional | **15.0** | buttons, **combos**, **tabs** (huge readability win) |
| `Monospace` | Monospace | **14.0** | IDs, coords, data values |
| `Small` | Proportional | **12.0** | captions, hints, `.small()` |

Rationale: bumping `Button` is the highest-leverage single change — every tab in
the builder strip and every combo's selected-text is `Button`-styled, so they
all grow at once.

`info_panel.rs`'s private `mono(n)` scale should be re-pinned to read these
tokens after P0 (its 12/13/18 becomes 14/15/20) so the right-hand info panel
matches the rest of the app — but keep it monospace (it's tabular data).

### §UO3.2 Spacing & control sizing (set in the theme)

| Field | Current | Target | Effect |
|---|---|---|---|
| `spacing.item_spacing` | `(8, 5)` (theme.rs:339) | `(8, 7)` | a touch more vertical air between rows |
| `spacing.button_padding` | `(8, 4)` (theme.rs:340) | `(10, 6)` | bigger click targets |
| `spacing.interact_size.y` | egui default (~18) | **26.0** | taller buttons/combos/sliders |
| `spacing.combo_width` | egui default (~100) | **190.0** | combos stop clipping enum names |
| `spacing.window_margin` | `same(10)` (theme.rs:342) | keep | — |
| section inner margin | n/a | `10.0` | breathing room inside tier-2 boxes |

### §UO3.3 Dropdowns

- **Global:** the `combo_width` + `interact_size.y` + `Button` font bumps above
  fix ~80% of the "tiny" feel with zero call-site edits.
- **Shared helper:** add `ui_kit::combo(id, selected_text)` (§UO5.2) returning a
  pre-sized `egui::ComboBox`; migrate call sites opportunistically (P3/P4). It
  guarantees consistent width and the explicit font that
  `viewer/src/editor/ui_helpers.rs` already proves works.

### §UO3.4 Section containers (the missing tier)

Two new shared widgets (§UO5.2):

- `ui_kit::section(ui, title, |ui| { … })` — a titled, framed, bordered box
  (always open). For panels with a handful of sections.
- `ui_kit::collapsing_section(ui, id, title, default_open, |ui| { … })` — same
  frame, but the body collapses. Drop-in replacement for the bare
  `CollapsingHeader` pattern in dense panels (system.rs, world.rs).

Both use `egui::Frame::group(ui.style())`, which already paints
`faint_bg_color` + the themed border stroke, so they follow every preset for
free. They add an inner margin, a bold title row, and a hairline rule under it.

### §UO3.5 Color

**No new colors.** Use the active theme via `palette::chrome_text()`,
`chrome_text_dim()`, `chrome_panel()`, `chrome_bg()`, and the `Style`'s
`visuals.selection`/widget strokes for accents. The map's semantic palette is
off-limits (§UO8). If you think you need a new color, you're probably about to
violate Law 3.

---

## §UO4 — Architecture: where each change lives

```
gui-core/src/theme.rs        ← P0: add tune_typography(); extend tune_spacing()
gui-core/src/ui_kit.rs  (NEW)← P1: section/collapsing_section/field/combo + font helpers
gui-core/src/lib.rs          ← P1: `pub mod ui_kit;`
gui-core/src/info_panel.rs   ← P1: dogfood ui_kit (re-pin mono scale, optional frames)

builder/src/app.rs           ← P2: chrome frames on panels, bigger status bar
builder/src/builder/panels/nav.rs ← P2: grouped tab strip
builder/src/builder/panels/*.rs   ← P3: migrate to ui_kit (one panel per subagent)

viewer/src/app/*.rs          ← P4: sections on info/planner/export/dashboard
viewer/src/editor/ui_helpers.rs ← P4: re-export/forward to ui_kit::combo to dedupe
```

**Dependency direction:** `gui-core` is the shared crate both apps already
depend on (`sectorforge_gui_core::theme`, `::palette`). `ui_kit` belongs there.
Nothing in `gui-core` may depend on `BuilderState` (keep the `nav.rs`
precedent — gui-core/src/nav.rs:1 documents exactly this rule).

---

## §UO5 — Foundation code (copy-paste, then tune)

> These two pieces are load-bearing; get them exactly right and the rest is
> mechanical. Code targets the egui version already in the repo (the one where
> `Rounding::same`, `Frame::none()`, `Frame::group`, `.inner_margin(f32)`,
> `from_id_salt`, and `Shadow{offset,blur,spread,color}` all compile — see
> theme.rs and viewer/src/app/layout.rs for proof). If an API name differs at
> implementation time, fix the call, not the design.

### §UO5.1 — Theme type scale + sizing (`gui-core/src/theme.rs`)

In `Theme::apply`, after `tune_spacing(&mut style);`, add a typography pass and
extend spacing. New functions:

```rust
use egui::{FontFamily, FontId, TextStyle};

/// App-wide type scale. egui's default is dense; this is the single place that
/// enlarges every default-styled label/button/combo/tab across both apps.
/// §UO3.1.
fn tune_typography(style: &mut Style) {
    style.text_styles = [
        (TextStyle::Heading,   FontId::new(20.0, FontFamily::Proportional)),
        (TextStyle::Body,      FontId::new(15.0, FontFamily::Proportional)),
        (TextStyle::Button,    FontId::new(15.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(14.0, FontFamily::Monospace)),
        (TextStyle::Small,     FontId::new(12.0, FontFamily::Proportional)),
    ]
    .into(); // Style.text_styles is a BTreeMap<TextStyle, FontId>; arrays .into() it.
}
```

Extend `tune_spacing` (theme.rs:338) with the control-sizing tokens:

```rust
fn tune_spacing(style: &mut Style) {
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);   // was (8,5)
    style.spacing.button_padding = egui::vec2(10.0, 6.0); // was (8,4)
    style.spacing.menu_margin = Margin::same(6.0);
    style.spacing.window_margin = Margin::same(10.0);
    // §UO3.2 — new:
    style.spacing.interact_size.y = 26.0; // taller buttons/combos/sliders
    style.spacing.combo_width = 190.0;     // combos stop clipping enum names
}
```

Wire it in `Theme::apply` (theme.rs:84-89):

```rust
        let mut style = Style {
            visuals: build_visuals(&p),
            ..Style::default()
        };
        tune_spacing(&mut style);
        tune_typography(&mut style); // §UO5.1
        ctx.set_style(style);
```

**That is the entire Phase 0.** Build + run both apps; everything should be
visibly larger and roomier. The existing theme test
(`every_theme_applies_and_flips_chrome`, theme.rs:349) still passes — it only
asserts colors/dark-mode. Consider adding a tiny assertion that
`ctx.style().text_styles[&TextStyle::Body].size == 15.0` to lock the scale.

### §UO5.2 — Shared widget kit (`gui-core/src/ui_kit.rs`, NEW)

```rust
//! §UO — shared chrome widgets for the builder and viewer.
//!
//! Tier-2 section containers and tier-3 field rows (see docs/UI_OVERHAUL.md
//! §UO3.4). These read the active theme via `palette::chrome_*` and
//! `Frame::group`, so they follow every preset automatically. NO dependency on
//! BuilderState — same rule as `nav.rs`.

use egui::{Frame, Margin, Rounding, RichText, Ui, WidgetText};

use crate::palette;

/// A titled, framed, bordered section box (always open). Tier-2 container.
pub fn section<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::group(ui.style())
        .inner_margin(Margin::same(10.0))
        .rounding(Rounding::same(6.0))
        .show(ui, |ui| {
            ui.label(RichText::new(title).strong().color(palette::chrome_text()));
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(6.0);
            add(ui)
        })
        .inner
}

/// Same frame, but the body collapses. Drop-in for bare `CollapsingHeader`.
/// Returns the body's value when open.
pub fn collapsing_section<R>(
    ui: &mut Ui,
    id_source: impl std::hash::Hash,
    title: &str,
    default_open: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    Frame::group(ui.style())
        .inner_margin(Margin::same(8.0))
        .rounding(Rounding::same(6.0))
        .show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new(title).strong())
                .id_salt(id_source)
                .default_open(default_open)
                .show(ui, add)
                .body_returned
        })
        .inner
}

/// Aligned label-left / control-right row. Tier-3 field.
/// NOTE: verify `Label` sizing API against the repo's egui version; if
/// `.wrap()`/`.truncate()` differ, keep the fixed-width label, drop the modifier.
pub fn field(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let h = ui.spacing().interact_size.y;
        ui.add_sized(
            [150.0, h],
            egui::Label::new(
                RichText::new(label).color(palette::chrome_text_dim()),
            ),
        );
        add(ui);
    });
}

/// Pre-sized combo. Caller chains `.show_ui(ui, |ui| { … })`.
/// Width/height also come from theme spacing (§UO3.2); this pins a floor and a
/// consistent selected-text style.
pub fn combo(id: impl std::hash::Hash, selected: impl Into<WidgetText>) -> egui::ComboBox {
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .width(190.0)
}
```

Register in `gui-core/src/lib.rs` (add alongside the existing `pub mod`s at
lib.rs:3-13):

```rust
pub mod ui_kit;
```

Then **dogfood it in `info_panel.rs`** (P1): its private `mono/section` helpers
prove the pattern; either replace them with `ui_kit` calls or re-pin
`mono(13)`→`mono(15)` / `mono(12)`→`mono(14)` to match §UO3.1. Optionally wrap
each `section()` block in `ui_kit::section` for visible boundaries in the
right-hand panel.

---

## §UO6 — Phased execution plan

Each phase is independently shippable: it compiles, passes the §UO9 gate, and
improves the UI on its own. Do them in order; **don't** start P3 before P0/P1
land (panels depend on `ui_kit`).

### Phase 0 — Theme type scale + sizing  ⟶ biggest win, smallest diff
- **Files:** `gui-core/src/theme.rs` only.
- **Do:** §UO5.1 (`tune_typography`, extend `tune_spacing`, wire in `apply`).
- **Accept:** both apps build; text + controls visibly larger; golden tests
  unchanged (§UO9); theme test passes (optionally extended to assert Body size).
- **Effort:** ~30 lines.

### Phase 1 — Shared `ui_kit`
- **Files:** `gui-core/src/ui_kit.rs` (new), `gui-core/src/lib.rs`,
  `gui-core/src/info_panel.rs` (dogfood).
- **Do:** §UO5.2; migrate `info_panel.rs` to `ui_kit` (or re-pin its scale).
- **Accept:** `gui-core` builds; info panel shows framed sections / new scale;
  golden tests unchanged.

### Phase 2 — Builder shell
- **Files:** `builder/src/app.rs`, `builder/src/builder/panels/nav.rs`.
- **Do:**
  - Give the central panel and status bar a chrome `Frame`
    (`Frame::none().fill(palette::chrome_panel()).inner_margin(…)`), matching the
    viewer's top bar pattern at `viewer/src/app/layout.rs:36-40`.
  - **Group the 24-tab strip** (nav.rs:47) into labeled clusters with
    `ui.separator()` between them (the bigger `Button` font from P0 already makes
    tabs readable). Suggested grouping:

    | Cluster | Tabs |
    |---|---|
    | Build | Project · Map · Subsectors · Regions · Routes |
    | Entities | System · World · Factions · Sites |
    | Power | Control · Economy · Relations |
    | Lore | History · Personae · Hooks · Missions · Prose · Briefing |
    | Analyze | Analytics · Interestingness · Search · Diff |
    | Output | Segmentum · Export |

    Keep `horizontal_wrapped`; insert a `ui.separator()` between clusters.
    (Optional, larger: a left `SidePanel` vertical nav rail with collapsing
    category groups — defer unless the user asks.)
- **Accept:** builder builds; tabs grouped; panels sit on themed chrome; tests
  pass.

### Phase 3 — Builder panels migration (the long tail)
- **Files:** `builder/src/builder/panels/*.rs`, one panel per subagent dispatch.
- **Do per panel:** replace bare `CollapsingHeader::new(x).show(…)` →
  `ui_kit::collapsing_section(ui, id, x, default_open, …)`; wrap top-level
  sections in `ui_kit::section`; route `ComboBox::from_id_salt(…)` through
  `ui_kit::combo`; wrap label/control pairs in `ui_kit::field`. **Do not** change
  any `state.run(BuilderCommand::…)` dispatch (§UO8).
- **Order by impact** (most sections/combos first):
  1. `system.rs` (15 sections, 8 combos) · `world.rs` (13 sections, 8 combos)
  2. `control.rs` (10 combos) · `history.rs` (11 combos) · `routes.rs` (11 combos)
  3. `search.rs` (8 combos) · `economy.rs` · `factions.rs` · `relations.rs`
  4. everything else (`analytics`, `briefing`, `regions`, `subsectors`,
     `missions`, `sites`, `hooks`, `personae`, `prose`, `intel`,
     `interestingness`, `conflict`, `orbital`, `export`, …)
- **Accept (per panel):** panel builds; sections are framed; combos consistent;
  `cargo test -p sectorforge-builder` green; no mutation-path change.

### Phase 4 — Viewer
- **Files:** `viewer/src/app/{sector_view,system_view,planner_view,export_ui,…}.rs`,
  `viewer/src/dashboard.rs`, `viewer/src/factions_overview.rs`,
  `viewer/src/editor/ui_helpers.rs`.
- **Do:** wrap info-panel content and planner/export/dashboard sections in
  `ui_kit::section`; route the unstyled combos (sector_view.rs:272,
  export_ui.rs:86, planner_view.rs:420) through `ui_kit::combo`; forward
  `editor/ui_helpers.rs::combo_str/combo_kv` to `ui_kit::combo` to delete the
  duplication.
- **Accept:** viewer builds; combos consistent with builder; tests pass.

### Phase 5 — Polish pass
- Replace remaining bare `ui.separator()` section breaks with `ui_kit::section`
  where it reads better; consistent empty-states (`ui_kit` could gain a
  `placeholder(ui, "No systems yet")`); tooltips on icon buttons; a spacing
  audit with the app running. Driven by the user's eye — ask for a screenshot
  round.

---

## §UO7 — Per-area migration cheatsheet

**Before (typical builder panel):**
```rust
egui::CollapsingHeader::new("Star").default_open(true).show(ui, |ui| {
    ui.horizontal(|ui| { ui.label("Class:"); /* combo */ });
    egui::ComboBox::from_id_salt("star_class")
        .selected_text(cur).show_ui(ui, |ui| { /* … */ });
});
ui.separator();
```

**After:**
```rust
ui_kit::collapsing_section(ui, "sys_star", "Star", true, |ui| {
    ui_kit::field(ui, "Class", |ui| {
        ui_kit::combo("star_class", cur).show_ui(ui, |ui| { /* … */ });
    });
});
```

The mutation that follows the combo (the `state.run(BuilderCommand::…)`) is
copied verbatim — layout changes never touch the command bus.

---

## §UO8 — GUARDRAILS (read before editing)

These are hard constraints from `CLAUDE.md`. Violating one is a defect even if
it compiles.

1. **Don't touch the map render or exports.** Out of scope, must stay
   byte-stable:
   - painters: `gui-core/src/sector_view.rs`, `system_view.rs`,
     `palette::draw_route_*`, `draw_faction_chip_*`, `heatmap.rs`, `map_theme.rs`;
   - map canvas consts: `palette::BG`, `PANEL_BG`, `HEX_*`, `star_color`,
     faction/hazard/route colors;
   - export writers in `src/`: `bitmap`, `svg_export`, `html_export`, `render`.
   The theme deliberately separates *chrome* (themeable) from *semantic map
   colors* (fixed) — theme.rs:9-15 documents this. **Never reroute a map painter
   to read `chrome_*`.** If a golden test changes, you crossed this line — revert.
2. **Golden tests must stay green.** After *any* change run
   `cargo test --test it -- golden`. Chrome/font/layout changes cannot alter
   `bitmap`/`svg`/`html` output; if they do, you edited something semantic.
3. **Mutations go through the command bus (§R4).** This is a *presentational*
   refactor. Never replace `state.run(BuilderCommand::…)` with a direct
   `BuilderState` field write — it breaks undo/redo. Move the call verbatim; only
   the surrounding layout changes.
4. **Determinism for emitted collections (§ determinism invariants).** If a panel
   collects keys to display, iterate `BTreeMap`/`BTreeSet` or sort first — never
   iterate `FxHashMap`/`FxHashSet` for ordered output. (Mostly N/A for layout,
   but watch list panels.)
5. **`gui-core` stays free of `BuilderState`.** `ui_kit` takes `&mut Ui` and
   plain data only (the `nav.rs:1` precedent).
6. **No new dependencies, no new colors.** Everything is `egui` + the existing
   `palette`/`theme`.

---

## §UO9 — Verification protocol (run after every phase / every panel)

```bash
cargo fmt --all
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --test it -- golden        # MUST be unchanged (§UO8)
cargo test -p sectorforge-builder     # after builder phases
cargo test -p sectorforge-viewer      # after viewer phases
```

Visual check (the part CI can't do):
```bash
cargo run -p sectorforge-builder      # eyeball each migrated tab
cargo run -p sectorforge-viewer
```
Prefer the `run` / `verify` skills to launch and screenshot. **You cannot judge
the look from compile output alone** — after a phase, surface a screenshot or
ask the user to confirm before moving on. Cycle every theme preset once (the
picker is `Theme: <name>` in the top bar) to confirm sections/contrast hold in
Light as well as the darks.

---

## §UO10 — Subagent routing for this work

Per `CLAUDE.md`'s routing rules:

- **`rust-explorer`** — locate the exact `ComboBox`/`CollapsingHeader`/section
  sites in a panel before editing; find all callers of a `ui_helpers` combo you
  want to forward. Read-only, returns `path:line`.
- **`panel-implementer`** — the workhorse for Phase 3. **One panel per
  dispatch.** It knows the `BuilderState`/`BuilderCommand`/derivations pattern
  and won't bypass the command bus. Give it: the panel path, "migrate to
  `ui_kit::section`/`collapsing_section`/`field`/`combo` per `docs/UI_OVERHAUL.md`
  §UO7", and the §UO8 guardrails.
- **`test-runner`** — run the §UO9 gate after each panel/phase; reports, doesn't
  fix.
- **`clippy-fixer`** — only if a phase introduces lint debt; confirm the lint
  category with the user first.

**Parallelize** across *independent* panels (e.g. dispatch `system.rs` and
`routes.rs` migrations together — they don't share state). **Do not** split a
single panel's migration across agents, and **do not** parallelize P0/P1 (they're
shared foundations everything else waits on). Foundation code (theme `ui_kit`)
is reviewed in the **main thread** — it's load-bearing and the user will want to
tune the numbers.

---

## §UO11 — LLM execution rules (do / don't)

**Do**
- Land P0 first and *stop* — let the user see the global type-scale change before
  touching panels. It may be 80% of the felt improvement.
- Keep diffs reviewable: one panel per commit in P3.
- Tune the §UO3 numbers live with the app open; they're starting points.
- Re-read the active `Style` rather than hardcoding — if you find yourself typing
  a pixel font size in a panel, it belongs in the theme instead.
- Update `GUIDE.md` and the §UO12 checklist as phases complete (CLAUDE.md
  requires GUIDE.md updates for non-trivial changes).

**Don't**
- Don't rewrite a working panel wholesale "while you're in there." Migrate the
  layout primitives only; leave logic, derivations, and command dispatch alone.
- Don't invent colors or pull in a UI crate. egui + `palette`/`theme` only.
- Don't let a golden test change slip through "because it's just the UI" — that
  signals you touched a painter or writer (§UO8).
- Don't iterate `Fx*` maps to build a displayed list (§UO8.4).
- Don't claim a phase done without the §UO9 gate green *and* a visual check.

**Common traps**
- `style.text_styles` is a `BTreeMap`; assigning a fixed array via `.into()` is
  correct and stays deterministic. Don't `.insert()` into the default map and
  leave egui's small defaults for the styles you forgot.
- `Frame::group` already paints fill+border from the theme; don't also set an
  explicit `.fill()`/`.stroke()` or you'll fight the preset (esp. Light).
- `ComboBox::from_id_salt` (not the older `from_id_source`) is what this repo's
  egui uses — match the existing call sites.
- `interact_size.y` affects *all* interactive widgets; 26 is comfortable, 30+
  starts to look clumsy. Tune with the app open.

---

## §UO12 — Progress checklist

- [x] **P0** theme type scale + sizing (`theme.rs`) — `§UO5.1` ✅ landed; golden tests byte-stable, type-scale test added
- [x] **P1** `ui_kit` module + `lib.rs` wire + `info_panel` dogfood — `§UO5.2` ✅ landed; `gui-core/src/ui_kit.rs` (`section`/`collapsing_section`/`field`/`combo` + mono text helpers), headless smoke test, `info_panel` text helpers now delegate to it
- [x] **P2** builder shell: chrome frames + grouped tab strip — `§UO6 P2` ✅ landed; `app.rs` puts the tab strip + status bar on `chrome_panel()` frames and the central workspace on `chrome_bg()` so section boxes float; `nav.rs` groups all 26 tabs via `TAB_CLUSTERS` (BUILD · ENTITIES · POWER · LORE · ANALYZE · OUTPUT · CHECK) with a partition test.
- [x] **P3a** `system.rs`, `world.rs` ✅ (15 + 13 sections, 16 combos). Star section hand-wrapped in a matching `Frame::group` to keep `header_response.scroll_to_me`.
- [x] **P3b** `control.rs`, `history.rs`, `routes.rs` ✅ (21 sections, 32 combos).
- [x] **P3c** `search.rs`, `economy.rs`, `factions.rs`, `relations.rs` ✅ (combos everywhere; `economy`/`search`/`factions` sectioned; `relations` combos done, its grid already framed).
- [x] **P3d** remaining builder panels ✅ analytics, diff, export, segmentum, generation, intel, missions, sites, hooks, personae, orbital, conflict, project, surface_regions, validation, invariants, regions, subsectors, briefing, prose, interestingness, generate_random, worlds_editor. Zero raw `ComboBox::from_id_salt` left; 4 `CollapsingHeader`s intentionally kept (project_tree dir-node, system Star + intel observer = captured-response, diff per-row tree).
- [x] **P4** viewer: combo dedupe + sections ✅ all 9 viewer combos + `editor/ui_helpers::combo_str`/`combo_kv` forward to `ui_kit::combo`; `dashboard.rs` blocks wrapped in `ui_kit::section`; planner (framed SidePanel) + export (modal Window) already contained.
- [x] **P5** polish — *foundation landed*: `ui_kit::placeholder` empty-state helper added (theme-aware, replaces hardcoded `Color32::GRAY`) and applied to the whole-tab empty/no-selection prompts. ⏳ Remaining spacing/tooltip audit + broader empty-state rollout are the screenshot-driven pass (need the app running — see §UO9).
- [x] `GUIDE.md` updated (§8.0 documents Phases 2–5); `BUILDER.md` updated (clustered tab strip + framed sections).
- [x] golden tests byte-stable throughout (`cargo test --test it -- golden` green after every phase; full `cargo test --workspace` green). ⏳ *Every theme preset eyeballed* is the user's visual confirmation pass (can't be done headless).
```
