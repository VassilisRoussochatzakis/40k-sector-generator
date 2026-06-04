# SPRUCE.md — Agentic UI Polish Runbook for `sectorforge`

> **Audience:** A Claude LLM agent with read/write access to this Rust + `egui` codebase and the ability to run `cargo`.
> **Mission:** Make the builder UI sleeker, more legible, and more "professional tool"-grade **without** breaking the multi-theme system, the data model, or any existing behavior. This is a *styling and composition* pass, not a feature change.
> **Output of your work:** Committed Rust changes that compile cleanly, plus a short `SPRUCE_CHANGELOG.md` summarizing what you touched.

This document is the source of truth. Follow the phases in order. Do not skip Phase 0. Build after every phase. If a step conflicts with reality in the codebase, prefer the codebase's actual structure and note the deviation in the changelog.

---

## 0. Operating principles (read before doing anything)

1. **Recon before edits.** You do not yet know where theming lives, which `egui` version is pinned, or how themes are switched. Phase 0 establishes all of that. Never write a style change before completing it.
2. **One source of truth for design tokens.** Every color, spacing value, radius, and font size must come from a central token module (Phase 2). If you find a hardcoded `Color32::from_rgb(...)` or a magic `8.0` spacing literal in a widget, that is a defect to migrate, not a pattern to copy.
3. **Theme-safe.** The user has multiple color themes (the screenshot shows "Theme: Grimdark"). Your tokens must be *parameterized by theme*, never baked to one palette. Structure: themes define a small set of **semantic roles**; widgets consume roles, never raw colors.
4. **Incremental + verifiable.** After each phase: `cargo check` (fast) then `cargo build`, fix warnings you introduced, and visually confirm. Never batch six phases then build once.
5. **Reversible.** Work on a branch (`git checkout -b spruce-ui` if git is present). Keep commits small and labeled per phase.
6. **Don't gold-plate.** Refined restraint beats maximalism for a data-dense builder tool. The goal is *clarity and hierarchy*, not decoration. Resist adding gradients, glows, or animation everywhere.
7. **Respect immediate-mode reality.** `egui` re-runs the UI every frame. "State" for hover/focus animation lives in `Context` via `animate_bool_with_time`, not in your structs. Don't invent a retained-mode animation system.

---

## 1. Diagnosis — what is actually wrong (so you fix causes, not symptoms)

From the current builder screen, these are the concrete defects to resolve. Keep this list; Phase 6 verifies against it.

| # | Defect | Root cause | Target |
|---|--------|-----------|--------|
| D1 | **Flat hierarchy** — title, section headers, body, and labels feel equally weighted | No type scale; one font family doing every job | A 5-step type scale with a display/body/mono split |
| D2 | **Surfaces blend together** — `Actions`/`Details` cards barely separate from the page | Card `Frame` fill ≈ panel fill; borders too low-contrast; no elevation | A 3-tier elevation system (base → surface → raised) with hairline borders + a soft shadow on raised cards |
| D3 | **All buttons look identical** — "New project" reads the same as "Save all" | No button-role system; every button uses default widget visuals | Primary / secondary / ghost / danger button helpers |
| D4 | **Data set in serif** — IDs (`new-sector`), seed (`seed-1`), size (`8×8`), and the entire status bar are hard to scan | Serif body font applied to tabular/telemetry data | Monospace for all IDs, seeds, counts, and status-bar telemetry |
| D5 | **Accent underused** — amber appears only on the active nav item and one selected button | Accent not wired into focus, primary actions, or active states | Deliberate accent usage: primary buttons, active nav (as an accent *bar*, not a full fill), focus rings, key numerics |
| D6 | **Heavy nav selection** — the solid gold fill on the active nav item is loud and dated | Selected state = full accent fill | Left accent bar (2–3 px) + subtle tinted background + accent-tinted text |
| D7 | **Semantic color misuse** — `validation: 0 err / 0 warn` is rendered in amber even when everything is clean | Status text uses brand/warn color unconditionally | Color by state: muted/neutral at 0, warn-amber only for warnings, error-red only for errors. Brand-amber and warn-amber must be visually distinct hues/levels |
| D8 | **Loose, uneven spacing** — large arbitrary gaps in the `Details` label/value rows; inconsistent padding between cards | No spacing scale; ad-hoc `add_space` calls | A 4 px base spacing scale; `Details` rebuilt as an aligned `egui::Grid` |
| D9 | **Inconsistent radii & padding** — buttons, cards, and nav items don't share a corner/padding language | Per-widget defaults | One radius scale (e.g. 4/8/12) and one padding scale, centrally applied |
| D10 | **No motion / state feedback** — hover and focus give little response | Default visuals, no transitions | Subtle hover lerp + focus ring + pressed feedback via `animate_bool_with_time` |
| D11 | **Dead space + weak rhythm** on the right/main columns | No max content width or column rhythm | Optional: constrain card column width and align the right rail to a consistent gutter |

---

## 2. Reconnaissance (Phase 0 — DO THIS FIRST)

Run these and record findings in `SPRUCE_CHANGELOG.md` under a `## Recon` heading. **Do not edit anything in this phase.**

### 2.1 Pin the egui version (critical — the API renamed things)

```bash
grep -RE '^(eframe|egui|egui_extras)\s*=' Cargo.toml
cargo tree -i egui 2>/dev/null | head -n 5
grep -E 'name = "egui"' -A1 Cargo.lock | head
```

**Why this matters — API drift you must handle:**

- **`Rounding` → `CornerRadius`.** Recent egui renamed the corner-radius type. On older egui use `egui::Rounding::same(8.0)`; on newer egui use `egui::CornerRadius::same(8)` (note: newer takes a `u8`, not `f32`). The same rename hit fields: `Visuals::window_rounding` → `window_corner_radius`, `WidgetVisuals::rounding` → `corner_radius`, `Frame::rounding` → `corner_radius`.
- **`Shadow` fields** changed shape over versions (`offset`/`blur`/`spread`/`color`; offset became an integer `[i8; 2]` and blur/spread became `u8`/`i8` in newer releases). Detect and match.
- **`Margin`** likewise moved from `f32` to integer-based fields in newer versions.
- **`Spacing` / `Style` / `Visuals`** field names are stable but additive — don't assume a field exists; check `cargo doc --open -p egui` or the version's changelog.

**Action:** Determine the exact pinned version, then create a single internal note in the token module (Phase 2) recording which API spelling you are using, e.g. `// egui 0.3x: CornerRadius(u8), Shadow{offset:[i8;2],blur:u8,...}`. Every snippet below is written in a **version-neutral** style; adapt the spelling to what you found here. **Do not guess — if `cargo check` complains about `Rounding` vs `CornerRadius`, that is your signal.**

### 2.2 Map the theming + style architecture

```bash
# Where is the current theme set?
grep -RIn --include='*.rs' -e 'set_visuals' -e 'set_style' -e 'set_fonts' -e 'Visuals' -e 'FontDefinitions' src
# How are themes represented? (the "Grimdark" enum/struct)
grep -RIn --include='*.rs' -e 'Grimdark' -e 'enum.*Theme' -e 'struct.*Theme' -e 'theme' src
# Where do panels/cards get drawn?
grep -RIn --include='*.rs' -e 'Frame::' -e 'SidePanel' -e 'CentralPanel' -e 'TopBottomPanel' -e 'CollapsingHeader' src
# Hardcoded colors and magic spacing to migrate later:
grep -RIn --include='*.rs' -e 'Color32::from_rgb' -e 'add_space(' -e 'Stroke::new' src | wc -l
```

Record: the theme type, where the active theme is stored, the function that pushes a theme into `egui::Context`, and the modules that render the nav, the cards, the buttons, the status bar, and the collapsibles. **Build a file map** (which file owns which region of the screenshot).

### 2.3 Establish a visual baseline

If the project runs, capture a "before" screenshot for the changelog. If you cannot run it (no display), note that and rely on `cargo check`/`build` plus careful reading. Either way:

```bash
cargo check
```

Confirm it compiles **before** you change anything, so any later breakage is attributable to your work.

---

## 3. The design language (Phase 1 — decide the tokens)

These are the target values. They are deliberately conservative for a dense tool. Treat numbers as defaults you may nudge ±2 to fit the existing layout — but keep the *ratios and roles*.

### 3.1 Semantic color roles (per theme)

Define **roles**, not colors. Each theme supplies a value for each role. Grimdark example values below (warm near-black base, parchment text, brass accent). Other themes reuse the same role names with different values.

| Role | Purpose | Grimdark example (sRGB) |
|------|---------|--------------------------|
| `bg_base` | App background, the darkest layer | `#0E0D0B` |
| `bg_surface` | Panels / nav / status bar | `#16140F` |
| `bg_raised` | Cards (`Actions`, `Details`, tree) | `#1D1A13` |
| `bg_overlay` | Menus, popups, tooltips | `#241F16` |
| `bg_hover` | Hovered interactive surface | `#2A2418` |
| `bg_active` | Pressed/selected surface tint | `#332B1A` |
| `border_subtle` | Hairline card/panel borders | `#2E2A20` |
| `border_strong` | Dividers, header rules | `#403828` |
| `text_primary` | Headings, key values | `#ECE5D6` |
| `text_secondary` | Body, descriptions | `#B7AE9C` |
| `text_muted` | Labels, hints, disabled-ish | `#7C7565` |
| `text_disabled` | Truly disabled controls | `#544F45` |
| `accent` | Brand / primary action / active | `#C8962F` (brass) |
| `accent_hover` | Hover state of accent | `#DEAA3E` |
| `accent_pressed` | Pressed accent | `#A87C24` |
| `on_accent` | Text/icon on an accent fill | `#1A1305` |
| `success` | OK / clean / 0-errors | `#7FB069` |
| `warning` | Warnings — **must differ from `accent`** | `#E0913A` (more orange than brass) |
| `danger` | Errors / destructive | `#D2603F` |
| `info` | Neutral notices | `#5E8CA8` |
| `focus_ring` | Keyboard/selection focus outline | `accent` @ ~70% alpha |
| `shadow` | Card drop shadow color | `#000000` @ ~45% alpha |

**Rules:**
- **`accent` ≠ `warning`.** This is defect D7. In Grimdark both are warm, so push `warning` toward orange and `accent` toward brass/gold, and rely on level/saturation to separate them. If they're indistinguishable at a glance, the status bar will lie to the user.
- Keep text contrast ≥ ~4.5:1 for `text_primary` on `bg_raised`. Don't let `text_secondary` drop below ~3:1.
- The three background tiers (`base` < `surface` < `raised`) must be *perceptibly* different — at least ~6–8 L\* steps apart — or D2 returns.

### 3.2 Spacing scale (4 px base)

Use only these. No arbitrary `add_space(7.0)`.

```
SPACE_2 = 2    SPACE_4 = 4    SPACE_8 = 8
SPACE_12 = 12  SPACE_16 = 16  SPACE_24 = 24  SPACE_32 = 32
```

- `item_spacing`: `(8, 6)` for general; `(8, 4)` inside dense grids like `Details`.
- `button_padding`: `(12, 6)`.
- Card inner margin: `16`. Card outer margin / gap between cards: `12`.
- Nav item padding: `(10, 6)`; nav group gap: `16`.

### 3.3 Radius scale

```
RADIUS_SM = 4   (buttons, chips, nav items)
RADIUS_MD = 8   (cards, panels, inputs)
RADIUS_LG = 12  (modals/popups only)
```
Pick one per element class and apply it everywhere via tokens (defect D9).

### 3.4 Elevation

- `bg_base`: no border, no shadow.
- `bg_surface` (panels): hairline `border_subtle`, no shadow.
- `bg_raised` (cards): hairline `border_subtle` **plus** a soft shadow (offset ~`(0, 2)`, blur ~`8`, spread `0`, color `shadow`). Keep it subtle — this is a tool, not a landing page.
- Popups/menus: stronger shadow (blur ~`16`).

### 3.5 Typography — the highest-impact change (defects D1 + D4)

Adopt a **three-role font system**. This single change does the most to make it look "designed."

| Text style | Role | Family | Size | Use |
|------------|------|--------|------|-----|
| `Heading` (display) | Display serif | the existing grimdark serif | 22 | Page title ("Project") |
| `subheading` (custom) | Display serif | same serif | 15 | Card section headers ("Actions", "Details") — consider small-caps/letter-spacing feel via uppercasing the string |
| `Body` | Body | a clean **proportional** UI face | 14 | Descriptions, nav labels, button text |
| `body_secondary` | Body | same proportional | 13 | Hints, secondary lines |
| `Monospace` | Mono | a monospace face | 13 | **All** IDs, seeds, sizes, counts, file paths, and the entire status bar |

- **Keep the serif for display only** (page titles + section headers). Serif there reinforces the grimdark identity. Using it for nav, buttons, and data is what makes the current UI feel heavy and slightly amateur — switch those to a crisp proportional UI font.
- **Monospace for telemetry** is the biggest "pro tool" upgrade: IDs like `new-sector`, `seed-1`, `8 × 8`, and the status line `0 sys · 0 wld · 0 rt · 0 fac` snap into alignment and stop competing with prose.
- Register custom fonts via `FontDefinitions` (see Phase 2.3). If the repo already bundles a serif, keep it; add one proportional + one mono. Prefer fonts already vendored in `assets/` to avoid new dependencies; if none exist, propose specific `.ttf` files in the changelog rather than silently adding crates.

---

## 4. Build the token + theme module (Phase 2)

Goal: a single module other code consumes. Adapt names to the repo's conventions. If a theme system already exists, **extend** it to expose these roles rather than replacing it.

### 4.1 Roles struct

```rust
// src/ui/theme.rs  (or wherever the existing theme lives)
use egui::Color32;

#[derive(Clone)]
pub struct Palette {
    pub bg_base: Color32,
    pub bg_surface: Color32,
    pub bg_raised: Color32,
    pub bg_overlay: Color32,
    pub bg_hover: Color32,
    pub bg_active: Color32,
    pub border_subtle: Color32,
    pub border_strong: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub text_disabled: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_pressed: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub info: Color32,
    pub focus_ring: Color32,
    pub shadow: Color32,
}

impl Palette {
    pub fn grimdark() -> Self {
        let rgb = |r, g, b| Color32::from_rgb(r, g, b);
        let rgba = |r, g, b, a| Color32::from_rgba_unmultiplied(r, g, b, a);
        Self {
            bg_base:       rgb(0x0E, 0x0D, 0x0B),
            bg_surface:    rgb(0x16, 0x14, 0x0F),
            bg_raised:     rgb(0x1D, 0x1A, 0x13),
            bg_overlay:    rgb(0x24, 0x1F, 0x16),
            bg_hover:      rgb(0x2A, 0x24, 0x18),
            bg_active:     rgb(0x33, 0x2B, 0x1A),
            border_subtle: rgb(0x2E, 0x2A, 0x20),
            border_strong: rgb(0x40, 0x38, 0x28),
            text_primary:  rgb(0xEC, 0xE5, 0xD6),
            text_secondary:rgb(0xB7, 0xAE, 0x9C),
            text_muted:    rgb(0x7C, 0x75, 0x65),
            text_disabled: rgb(0x54, 0x4F, 0x45),
            accent:        rgb(0xC8, 0x96, 0x2F),
            accent_hover:  rgb(0xDE, 0xAA, 0x3E),
            accent_pressed:rgb(0xA8, 0x7C, 0x24),
            on_accent:     rgb(0x1A, 0x13, 0x05),
            success:       rgb(0x7F, 0xB0, 0x69),
            warning:       rgb(0xE0, 0x91, 0x3A),
            danger:        rgb(0xD2, 0x60, 0x3F),
            info:          rgb(0x5E, 0x8C, 0xA8),
            focus_ring:    rgba(0xC8, 0x96, 0x2F, 0xB3),
            shadow:        rgba(0x00, 0x00, 0x00, 0x73),
        }
    }
    // Add one constructor per existing theme. Reuse the SAME role names.
}
```

Spacing/radius/typography tokens as plain consts:

```rust
pub mod space { pub const S2:f32=2.0; pub const S4:f32=4.0; pub const S8:f32=8.0;
    pub const S12:f32=12.0; pub const S16:f32=16.0; pub const S24:f32=24.0; pub const S32:f32=32.0; }
pub mod radius { pub const SM:f32=4.0; pub const MD:f32=8.0; pub const LG:f32=12.0; }
```

> ⚠️ If on a newer egui where `CornerRadius` takes `u8`, store radii as `u8` (`4`, `8`, `12`) instead and adjust call sites. This is the version decision from Phase 0.

### 4.2 Push a `Palette` into `egui::Visuals`

Map roles onto egui's widget-state machine. egui distinguishes **`noninteractive`**, **`inactive`**, **`hovered`**, **`active`**, and **`open`** widget visuals — wiring these correctly is what makes hover/press feedback work for free (defect D10).

```rust
pub fn apply_theme(ctx: &egui::Context, p: &Palette) {
    let mut v = egui::Visuals::dark();
    v.dark_mode = true;
    v.override_text_color = Some(p.text_secondary); // default body color
    v.panel_fill = p.bg_surface;
    v.window_fill = p.bg_overlay;
    v.extreme_bg_color = p.bg_base;       // text edit backgrounds, etc.
    v.faint_bg_color = p.bg_raised;       // zebra striping in grids
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.bg_active;
    v.selection.stroke = egui::Stroke::new(1.0, p.accent);

    // Hairline window/popup borders + radius (use corner_radius on newer egui).
    v.window_stroke = egui::Stroke::new(1.0, p.border_subtle);
    // v.window_corner_radius = radius::MD as u8;   // newer egui
    // v.window_rounding = egui::Rounding::same(radius::MD); // older egui

    let widget = |bg, weak_bg, border, fg| egui::style::WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: weak_bg,
        bg_stroke: egui::Stroke::new(1.0, border),
        fg_stroke: egui::Stroke::new(1.0, fg),
        // corner_radius: radius::SM as u8,  // newer egui
        // rounding: egui::Rounding::same(radius::SM), // older egui
        expansion: 0.0,
    };
    v.widgets.noninteractive = widget(p.bg_surface, p.bg_surface, p.border_subtle, p.text_secondary);
    v.widgets.inactive      = widget(p.bg_raised,  p.bg_raised,  p.border_subtle, p.text_primary);
    v.widgets.hovered       = widget(p.bg_hover,   p.bg_hover,   p.border_strong, p.text_primary);
    v.widgets.active        = widget(p.bg_active,  p.bg_active,  p.accent,        p.text_primary);
    v.widgets.open          = widget(p.bg_overlay, p.bg_overlay, p.border_strong, p.text_primary);

    ctx.set_visuals(v);
    apply_spacing(ctx);
}

fn apply_spacing(ctx: &egui::Context) {
    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing   = egui::vec2(space::S8, space::S8 * 0.75);
    s.spacing.button_padding = egui::vec2(space::S12, space::S8 * 0.75);
    s.spacing.menu_margin    = egui::Margin::same(space::S8 as _);
    s.spacing.indent         = space::S16;
    s.spacing.interact_size.y = 28.0; // comfortable hit target
    ctx.set_style(s);
}
```

> **Fill the `corner_radius`/`rounding` lines** per your Phase-0 version finding and delete the wrong one. Leaving both will not compile.

### 4.3 Register fonts

```rust
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Add bytes for vendored fonts. Prefer fonts already in assets/.
    fonts.font_data.insert("display".into(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/<serif>.ttf")).into());
    fonts.font_data.insert("ui".into(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/<proportional>.ttf")).into());
    fonts.font_data.insert("mono".into(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/<mono>.ttf")).into());

    // Proportional = UI body, with the serif available as a named family for headings.
    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "ui".into());
    fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "mono".into());
    fonts.families.insert(egui::FontFamily::Name("display".into()), vec!["display".into()]);

    ctx.set_fonts(fonts);
    install_text_styles(ctx);
}

fn install_text_styles(ctx: &egui::Context) {
    use egui::{FontFamily, FontId, TextStyle};
    let display = FontFamily::Name("display".into());
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading,  FontId::new(22.0, display.clone())),
        (TextStyle::Body,     FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button,   FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Small,    FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        // Custom: section subheaders rendered with the display serif.
        (TextStyle::Name("subheading".into()), FontId::new(15.0, display)),
    ].into();
    ctx.set_style(style);
}
```

Call `install_fonts(ctx)` once at startup, then `apply_theme(ctx, &palette)` whenever the theme changes (hook into the existing "Theme: Grimdark" switcher). **Verify `cargo check` here before continuing.**

---

## 5. Component refactors (Phase 3)

Migrate the regions of the screenshot one at a time. After each, build and eyeball it. Below, `p` is the active `&Palette` (thread it through, or fetch via a context extension / your app state).

### 5.1 Card / panel helper (fixes D2, D9)

Replace ad-hoc `Frame`s with one helper so every card shares elevation, radius, and padding.

```rust
pub fn card<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::default()
        .fill(p.bg_raised)
        .stroke(egui::Stroke::new(1.0, p.border_subtle))
        // .corner_radius(radius::MD as u8)        // newer egui
        // .rounding(egui::Rounding::same(radius::MD)) // older egui
        .inner_margin(egui::Margin::same(space::S16 as _))
        .outer_margin(egui::Margin::symmetric(0, (space::S12) as _))
        .shadow(egui::epaint::Shadow {
            offset: [0, 2],     // newer egui: [i8;2]; older: egui::vec2(0.0, 2.0)
            blur: 8,            // older egui: f32
            spread: 0,
            color: p.shadow,
        })
        .show(ui, |ui| add(ui))
        .inner
}
```

Section header inside a card (replaces the thin underline with a deliberate rule):

```rust
fn section_header(ui: &mut egui::Ui, p: &Palette, label: &str) {
    ui.label(
        egui::RichText::new(label.to_uppercase())
            .text_style(egui::TextStyle::Name("subheading".into()))
            .color(p.text_primary),
    );
    ui.add_space(space::S8);
    // 1px rule in border_strong, spanning the card width:
    let rect = ui.available_rect_before_wrap();
    let y = ui.cursor().top();
    ui.painter().hline(rect.x_range(), y, egui::Stroke::new(1.0, p.border_strong));
    ui.add_space(space::S12);
}
```

### 5.2 Button hierarchy (fixes D3, D5)

Provide four roles. This is what makes "New project…" read as the obvious primary action and "Save all" read as secondary.

```rust
pub enum BtnRole { Primary, Secondary, Ghost, Danger }

pub fn button(ui: &mut egui::Ui, p: &Palette, role: BtnRole, label: &str) -> egui::Response {
    let (fill, text, border) = match role {
        BtnRole::Primary   => (p.accent,    p.on_accent,    p.accent),
        BtnRole::Secondary => (p.bg_raised,  p.text_primary, p.border_strong),
        BtnRole::Ghost     => (egui::Color32::TRANSPARENT, p.text_secondary, egui::Color32::TRANSPARENT),
        BtnRole::Danger    => (p.bg_raised,  p.danger,       p.danger),
    };
    let btn = egui::Button::new(egui::RichText::new(label).color(text))
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, border));
        // .corner_radius(radius::SM as u8) / .rounding(...) per version
    let resp = ui.add(btn);
    // Hover lerp for non-primary; accent_hover for primary (see Phase 4 for the animated version).
    resp
}
```

Apply: `New project…` = **Primary**. `Open project…`, `Random sector…`, `Save as…`, `Save all` = **Secondary**. Disabled `Save`/`Save all` should use `text_disabled` and a flatter fill (use `ui.add_enabled(false, ..)` so egui dims them and blocks interaction — fixes the "can't tell it's disabled" problem). Reserve **Danger** for destructive actions (delete project, etc.) — text/border in `danger`, filling only on hover.

### 5.3 Left nav (fixes D6)

Replace the solid gold selection fill with an accent **bar + tint**.

```rust
fn nav_item(ui: &mut egui::Ui, p: &Palette, label: &str, selected: bool) -> egui::Response {
    let desired = egui::vec2(ui.available_width(), 30.0);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    let hovered = resp.hovered();
    let painter = ui.painter();

    let bg = if selected { p.bg_active } else if hovered { p.bg_hover } else { egui::Color32::TRANSPARENT };
    // painter.rect_filled(rect, radius::SM, bg);  // older: f32 radius; newer: u8 CornerRadius
    painter.rect_filled(rect, 4.0, bg);

    if selected {
        // 3px accent bar on the left edge.
        let bar = egui::Rect::from_min_size(rect.left_top(), egui::vec2(3.0, rect.height()));
        painter.rect_filled(bar, 0.0, p.accent);
    }
    let text_color = if selected { p.accent_hover } else if hovered { p.text_primary } else { p.text_secondary };
    painter.text(
        rect.left_center() + egui::vec2(space::S12, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.0),
        text_color,
    );
    resp
}
```

Group headers ("BUILD", "ENTITIES", "POWER", "LORE"): render in `text_muted`, `TextStyle::Small`, uppercased, with `letter-spacing` simulated by the small caps feel; add `SPACE_16` above each group. Keep the disclosure triangles but tint them `text_muted`.

### 5.4 Details block → aligned grid (fixes D4, D8)

```rust
fn details(ui: &mut egui::Ui, p: &Palette, rows: &[(&str, &str)]) {
    egui::Grid::new("project_details")
        .num_columns(2)
        .spacing(egui::vec2(space::S24, space::S8))
        .striped(false)
        .show(ui, |ui| {
            for (k, v) in rows {
                ui.label(egui::RichText::new(*k).color(p.text_muted).small());
                ui.label(egui::RichText::new(*v).color(p.text_primary).monospace()); // mono values!
                ui.end_row();
            }
        });
}
// rows = [("ID","new-sector"),("Title","New Sector"),("Seed","seed-1"),("Size","8 × 8"),("Folder","(unsaved)")]
```

Labels in `text_muted` small; values in **monospace** `text_primary`. This kills the giant uneven gaps and aligns everything to a column.

### 5.5 Status bar (fixes D4, D7)

Set it all in monospace, separate segments with a faint vertical rule, and **color by state**:

```rust
fn status_segment(ui: &mut egui::Ui, color: egui::Color32, text: &str) {
    ui.label(egui::RichText::new(text).monospace().color(color));
}

fn validation_color(p: &Palette, errors: u32, warns: u32) -> egui::Color32 {
    if errors > 0 { p.danger } else if warns > 0 { p.warning } else { p.text_muted }
}
```

So `validation: 0 err / 0 warn` renders in `text_muted` (or `success` if you want a positive "clean" signal), **not amber**. `project: (unsaved)` → `warning`; `clean` → `success`. Counters (`0 sys · 0 wld · …`, `cmd 0/0`, `cache: 0`, `deriv 0`) → `text_secondary` monospace. Add `bg_surface` fill + a top hairline `border_subtle` to the status `TopBottomPanel`.

### 5.6 Collapsibles (Snapshots, Recent projects, Files, World data, Generation)

Wrap each in the `card` frame OR give them a consistent `CollapsingHeader` style: header text in `text_secondary` body, triangle in `text_muted`, hovered header bg `bg_hover`. Keep them visually lighter than the primary cards so the hierarchy reads (primary content > collapsibles).

### 5.7 Top bar + tabs

The "Theme: Grimdark" chip and the "New Sector" tab: give the active tab an accent **underline** (2px) instead of a fill, matching the nav language. The `+` button → **Ghost** role. Keep the macOS traffic-light dots untouched (OS chrome).

---

## 6. Motion & micro-interactions (Phase 4 — restrained)

egui animates via `ctx.animate_bool_with_time(id, on, secs)` → returns an eased `f32` you lerp with. Add these **three** only; do not over-animate a tool.

1. **Hover lerp on buttons/nav.** Lerp fill between resting and hover color by the animated factor (~0.12s). Use `Color32` channel-wise lerp or `egui::ecolor::tint`/manual mix.
   ```rust
   let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
   let fill = lerp_color(rest_fill, hover_fill, t);
   ```
2. **Focus ring.** When a widget `has_focus()`, stroke a 2px `focus_ring` rounded rect just outside it. Critical for keyboard users and looks intentional.
3. **Pressed feedback.** On `resp.is_pointer_button_down_on()`, nudge `WidgetVisuals.active.expansion` to `-1.0` (a subtle inset) or darken to `accent_pressed`.

Helper:
```rust
fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgba_unmultiplied(m(a.r(),b.r()), m(a.g(),b.g()), m(a.b(),b.b()), m(a.a(),b.a()))
}
```

Avoid: animating layout/size of cards, sliding panels, anything that causes reflow every frame, or continuous animations that prevent egui from sleeping (watch CPU).

---

## 7. Keep the multi-theme system intact (Phase 5)

- Every new theme gets a `Palette::<name>()` constructor with the **same role set**. Do not add roles to one theme and not others.
- The theme switcher calls `apply_theme(ctx, &palette)` only. No widget should branch on `if theme == Grimdark`.
- If a theme is light, your role values flip (light `bg_*`, dark `text_*`) but **widget code does not change** — that's the proof the abstraction is right.
- Verify each existing theme still renders: switch through all of them and confirm contrast and that no color is hardcoded to grimdark brass.

---

## 8. Verification & acceptance (Phase 6)

Run and confirm each item. Record pass/fail in `SPRUCE_CHANGELOG.md`.

```bash
cargo fmt
cargo clippy --all-targets -- -W clippy::all
cargo build
cargo test            # if tests exist; do not regress them
```

**Acceptance checklist (maps to Section 1 defects):**

- [ ] D1 — Page title, section headers, body, and labels are now visibly different in size/weight/family.
- [ ] D2 — Cards clearly float above panels (distinct fill + hairline + subtle shadow). Three background tiers are distinguishable.
- [ ] D3 — "New project…" is unmistakably the primary action; secondaries are quieter; disabled buttons read as disabled.
- [ ] D4 — Every ID, seed, size, count, path, and the whole status bar is monospace and aligned.
- [ ] D5 — Accent appears on: primary buttons, active nav bar, focus rings, key numerics — and nowhere accidental.
- [ ] D6 — Active nav uses an accent bar + tint, not a solid fill.
- [ ] D7 — `validation: 0 err / 0 warn` is **not** amber; warning-amber and brand-amber are distinguishable.
- [ ] D8 — `Details` is a clean aligned grid; spacing uses only the 4px scale.
- [ ] D9 — Radii and padding are consistent across buttons, cards, nav, inputs.
- [ ] D10 — Hover, focus, and pressed states give visible feedback.
- [ ] D11 — No jarring dead zones; columns share a gutter rhythm.
- [ ] No hardcoded `Color32::from_rgb` remains in widget code (`grep` it — all colors come from `Palette`).
- [ ] All existing themes still render correctly; no widget branches on theme identity.
- [ ] `cargo build` clean; no new clippy warnings you introduced; tests pass.
- [ ] App still sleeps when idle (no runaway repaint from animation).

**Then write `SPRUCE_CHANGELOG.md`** with: recon findings (egui version + API spelling chosen), files touched per phase, before/after notes, and any deviations from this runbook with justification.

---

## 9. Appendix — egui pitfalls & quick reference

- **Version spelling is the #1 source of build breaks.** `Rounding`/`CornerRadius`, `rounding`/`corner_radius`, `Shadow` field types, `Margin` int-vs-float. Decide once in Phase 0; apply consistently.
- **Immediate mode:** never store hover/animation state in your structs; use `Context` (`animate_bool_with_time`, `animate_value_with_time`) keyed by a stable `Id`.
- **Set fonts before text styles**, and call both before the first frame uses them. Re-applying fonts every frame thrashes the atlas — do it once / on theme change only.
- **`override_text_color`** sets the global default; per-widget `RichText::color()` overrides it. Use the global for body, `RichText` for headings/values.
- **`weak_bg_fill` vs `bg_fill`:** buttons use `weak_bg_fill` when not "filled"; set both in `WidgetVisuals` or buttons may look unfilled.
- **Disabled controls:** prefer `ui.add_enabled(false, widget)` over manually graying — egui handles interaction-blocking + dimming.
- **Shadows cost fill-rate;** keep blur modest and only on raised cards/popups.
- **Contrast-check** every role pair you ship; the grimdark palette is intentionally low-key and easy to under-contrast.
- **Don't add font/crate dependencies silently.** If you need a font file, propose it in the changelog and prefer assets already vendored.

---

*End of runbook. Work the phases in order, build between each, and keep the changelog honest.*
