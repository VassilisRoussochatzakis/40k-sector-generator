# BEAUTY.md — instructions for beautifying the GUI to showcase quality

> **Audience: a future Claude session.** This is not user documentation. It is a
> self-contained brief you (the model) read when the user says *"beautify the
> UI"* / *"make this look stunning"* / *"work on BEAUTY.md"*. It assumes you
> arrive with **no prior context**. Read it top to bottom before writing any
> code. The goal is not "tidier" — it is *Dribbble-shot, README-hero,
> product-launch* beautiful: bespoke components a stranger would screenshot.
>
> **This work is meant to be run across multiple agents.** §3 is the execution
> model and is mandatory, not optional — dispatch recon and propagation to
> subagents; keep aesthetic judgment in the main thread.

---

## Status — what has landed, and where a future run picks up

This playbook was executed once (2026-06-04). The scaffolding and the first hero
are **done — do not redo them:**

- **Visual loop (§0):** the builder's dev capture mode exists —
  `cargo run -p sectorforge-builder -- --project <dir> --tab <TAB> --theme <THEME>
  --screenshot <png>` (plus `--screenshot-frames N`, `--select-faction <id>`). It
  renders a few frames, writes a PNG, and exits; **Read** that PNG to see the
  result. Use it every round — do not beautify blind.
- **Token foundation (§4, Phase B):** `gui-core/src/design.rs` (`§DESIGN`) holds
  the spacing / radius / elevation / motion / type / accent primitives; `theme.rs`
  + `ui_kit.rs` route through it. *Extend* it — never reintroduce literals.
- **First hero + recipe (§5, §6.2, Phase C):**
  `gui-core/src/card.rs::selectable_plate` (`§BEAUTY`) — an animated selectable
  plate — is landed and **propagated to the Factions / World / System / Routes
  rosters** (Phase D). Verified across Grimdark / Light / Slate.

**Second pass landed (2026-06-04).** Phase-D propagation plus three more heroes are
now **done — do not redo them:**

- **Phase-D propagation — COMPLETE.** `card::selectable_plate` now hosts *every*
  roster rail — Factions, World, System, Routes **plus** the formerly-pending Sites,
  Subsectors, Personae, Hooks, Missions, Search, Validation, Invariants — **and** the
  left nav-rail tab entries. Each rail keeps its meaning-carrying decoration inside
  the plate (severity dots, the search winner's green, per-row count badges, two-line
  title+sub-label rows wrapped in a `ui.vertical`). Verified across Grimdark / Light /
  Slate via the §0 capture loop.
- **Star-map hero (§6.1) — DONE.** Live-only void flourishes in
  `gui-core/src/sector_view.rs`: deterministic **star dust** (hashed positions, no
  shimmer), a radial **vignette** (8-point triangle fan), and a gilded **chart frame**
  (hairline border + brass corner brackets/rivets in the active accent). Gated on a
  dark map theme (`is_dark`) + a min canvas size. Decoupled from the golden exporters
  (separate `MapTheme`/`Canvas`/geometry — **export goldens stay green**); the gui-core
  *live-render* `map_snapshots` did move and were re-blessed (`UPDATE_MAP_SNAPSHOTS=1`;
  the dust is deterministic, so the new snapshots are stable).
- **Section plates (§6 #5) — DONE.** `ui_kit::section` / `collapsing_section` now carry
  an `elev_low` contact shadow, and `section` rules its title with a brass `gilt_rule`
  in the active accent — lifts every panel at once.
- **Nav-rail active indicator (§6 #3) — DONE** via the plate recipe above: the active
  tab now reads as a lit brass-barred plate instead of a flat fill.

**Start a future run here:**

1. **Typography (§5.5)** — still `default_fonts`; the single highest-value untouched
   primitive, and now the main blocker. Register a display face (titles only), a
   humanist body, and a mono for tables via `FontDefinitions` + `ctx.set_fonts`; add a
   font-family token to `design.rs`. **Decision needed first:** it requires shipping a
   licensed (OFL) font *binary* in-repo — choose the faces and get them committed, then
   wire them. It touches every pixel — do it before more component polish.
2. **Info panel (§6 #4)** (`gui-core/src/info_panel.rs`) — refine the kv tables:
   aligned columns, hairline row separators, restrained accents.
3. **Bespoke buttons & toggles (§6 #6)** and **dialogs / modals (§6 #7)** — a painted
   brass primary button (press/hover/glow), a custom sliding toggle, and elevated modal
   surfaces (scrim + `elev_high` + entrance motion).

The sections below are the full playbook; with the above landed, treat §4, §6.1, §6.2,
§6.3, and §6.5 as reference rather than to-do.

---

## 0. The one rule that matters most: close the visual feedback loop FIRST

You cannot see rendered pixels. Iterating on visuals **blind** produces
plausible-looking code that is actually ugly. **Subagents are even more blind —
they cannot run the GUI or judge a screenshot. So the visual loop lives in the
main thread; agents never own aesthetic decisions** (see §3). Before changing a
single component, establish a way to *look at the result*. In priority order:

1. **Drive the app yourself.** Use the `/run` skill (launches this project's
   egui app) and the `/verify` skill (confirms a change in the running app).
   Capture a screenshot each iteration. This is an egui/eframe app — it boots to
   a window; the skills know how to launch the builder (`cargo run -p
   sectorforge-builder`) and viewer (`cargo run -p sectorforge-viewer`).
2. **In-app screenshot — already wired (this is the loop that's actually used).**
   The builder has a dev-only capture mode:
   `cargo run -p sectorforge-builder -- --project <dir> --tab FACTIONS --theme
   Grimdark --screenshot /tmp/shot.png` renders a few frames, captures the window
   to a PNG, and exits (`--screenshot-frames N` tunes the settle delay;
   `--select-faction <id>` pre-selects a row). A session then **Reads** that PNG to
   see the result. Implementation: `builder/src/main.rs` wraps `BuilderApp`, sends
   `ctx.send_viewport_cmd(ViewportCommand::Screenshot)` — a **unit** variant in
   egui 0.29 (it gained a `UserData` argument only in 0.30; `UserData` does **not**
   exist here, so any `Screenshot(UserData::default())` snippet you remember is
   wrong for this version) — reads the image back via
   `egui::Event::Screenshot { image, .. }`, and encodes it with
   `gui_core::save_color_image_png`.
3. **Ask the user to paste screenshots.** Slowest, but always available. Tell
   them exactly which panel + theme + window size you need.

**If you have no way to see output, stop and say so.** Do not "beautify" blind.
A wrong guess compounds: you will build five components on top of a bad
foundation. One screenshot per round is the difference between showcase and
slop.

Per round: render → screenshot → critique honestly (name what's ugly: spacing,
contrast, weight, alignment, flatness) → adjust one axis → re-render.

---

## 1. What this app is (so your aesthetic is grounded, not generic)

**SectorForge** — a Warhammer 40,000 sector generator with a heavyweight egui
desktop **builder**, a lighter **viewer**, and shared **gui-core** widgets.

- **Framework:** `egui` + `eframe` **0.29**, **glow** (OpenGL) backend, `default_fonts` only. (Confirm versions in `*/Cargo.toml`; do not assume a newer egui API — see §7.)
- **Diegetic aesthetic — use it.** The default theme is **`Grimdark`**:
  *imperial amber/gold (`rgb(214,158,74)`) on near-black warm violet
  (`rgb(16,14,18)`)*. The subject is a **grimdark gothic star cartography**
  tool. That is a *gift* — the beauty target is not "generic clean SaaS," it is
  **Imperial cartographic instrument**: aged-parchment cards, brass/gilt edges,
  void-black star fields, gothic display type, hairline rules, riveted frames,
  candle-warm accent glow. Lean *all the way* into the fiction. Diegetic beauty
  reads as intentional; generic beauty reads as a template.
- **Two display modes matter.** Chrome is themeable (8 presets incl. one
  `Light`). Make hero components beautiful in **Grimdark first**, then verify
  they don't fall apart under `Light` and `Slate`.

---

## 2. The repo as it stands — build on this, do not reinvent it

Read these before designing. Exact ownership:

| File | Owns | For beauty work |
|---|---|---|
| `gui-core/src/theme.rs` | The **global style entry point**. `Theme::apply(ctx)` builds `Visuals` from a `Pal` (per-preset flat color set), then `tune_spacing` + `tune_typography`. 8 presets; default `Grimdark`. | This is where global radius/shadow/spacing/type defaults live today. Raise the *baseline* here; do per-component bespoke work elsewhere. **Single-writer file (§3).** |
| `gui-core/src/palette.rs` | Chrome colors (`set_chrome`, `chrome_text()`, `chrome_text_dim()`, `PANEL_BG`) **and** low-level custom painters (`rect_filled`/`rect_stroke` helpers, faction swatches, selection borders). | The painting primitives you'll extend. Reuse these helpers; add new bespoke painters here or in a new module. **Single-writer file (§3).** |
| `gui-core/src/ui_kit.rs` | **§UO chrome widget kit**: `section` / `collapsing_section` (framed boxes), `field` (label-left/control-right row), `combo`, `kv`, text helpers (`mono_title/section/body/dim`), responsive `columns_responsive`, `reading_column`. Type scale consts `TITLE/SECTION/BODY/DIM`. **Takes `&mut Ui` + plain data — never `BuilderState`.** | The container/layout vocabulary. Make these *prettier* and they lift every panel at once. Keep the "no `BuilderState`" rule. **Single-writer file (§3).** |
| `gui-core/src/sector_view.rs`, `system_view.rs`, `map_theme.rs`, `info_panel.rs`, `nav.rs`, `heatmap.rs` | The custom-painted map/system renderers + nav rail + info panel. | The hero surfaces. The star map is the single highest-impact thing to make beautiful. **But see the determinism caveat in §8.** |
| `gui-core/src/visual_tokens.rs` | **Map *semantic* tokens** — system glyphs, route line styles, region overlays. **Despite the name, NOT design tokens.** | Do not put spacing/radius/motion here. Leave it alone unless adding a map symbol. |
| `builder/src/builder/panels/*.rs` | ~50 builder panels (factions, world, system, routes, regions, economy, briefing, export…). Each `show()` takes `&mut BuilderState`. | Where most chrome lives. Beautify via the shared kit, not per-panel snowflakes. **Independent files → safe to fan out across agents (§3).** |
| `docs/UI_OVERHAUL.md` | The **§UO playbook** — the earlier "make panels coherent" pass (sections, fields, spacing). | **Read it** (or have an agent summarize it). BEAUTY.md is the *next tier up* (showcase, not merely coherent). Reference §UO; don't duplicate or contradict it. |

**Design-token module — LANDED.** `gui-core/src/design.rs` (`§DESIGN`) now holds
the named primitives: a 4 px **spacing** grid, one **radius** family
(`RADIUS_SM/MD/LG`), **elevation** presets (`elev_low/med/high`), **motion**
durations (`MOTION_FAST/BASE/SLOW`) + `ease_*` curves, the **type scale**
(`DISPLAY/TITLE/SECTION/BODY/DIM/CAPTION`, re-exported by `ui_kit` so there is one
source of truth), and a theme-derived **accent ramp** (`accent` / `lerp_color` /
`accent_bright` / `accent_glow`). `theme.rs` + `ui_kit.rs` route through it, the
scattered radius literals (`4.0`/`6.0`/`7.0`) are gone, and the first motion lives
in `card.rs`. **Extend this module** when a component needs a new primitive — do
not reintroduce literals, and do not recreate it. (The one §5.5 primitive still
missing: a registered display font — type is still `default_fonts`.)

---

## 3. Execution model — orchestrate this across multiple agents

This work is deliberately structured for **multi-agent execution**. The point is
*context economy and parallel breadth*: read-only recon and verbose
verification run in isolated subagent contexts and return only the conclusion,
so the main thread stays lean and focused on the thing only it can do — **look
at screenshots and judge beauty**.

### The non-negotiable division of labor

- **Main thread owns:** the visual feedback loop (§0), all aesthetic decisions,
  the token module, and the *first* hero component. Subagents are blind to
  pixels — they must never decide what looks good.
- **Subagents own:** read-only recon (mapping code, summarizing docs), verifying
  egui-0.29 API signatures, **mechanically propagating an already-approved recipe
  across many independent files**, and verbose verification (tests, clippy).

### Three hard rules (violating these corrupts the work)

1. **Shared files are single-writer.** `gui-core/src/design.rs`, `card.rs`,
   `theme.rs`, `ui_kit.rs`, `palette.rs` are touched by **exactly one writer at a time**
   (normally the main thread). Never dispatch two agents that both edit a shared
   module — they will clobber each other. Parallelism is **only** across
   *independent* files (one builder panel per agent).
2. **Design serially, propagate in parallel.** A single component's
   design (paint + motion + iterate on screenshots) is **coherent main-thread
   work** — do not split one component across a plan-agent → code-agent →
   test-agent pipeline; context is lost at every handoff (per CLAUDE.md). Fan out
   **only after** the recipe is extracted and approved, to apply that *same*
   recipe to many panels.
3. **Background the verification.** Run `test-runner` (goldens) and
   `clippy-fixer` as **background** dispatches while you keep working; don't
   block the visual loop on them.

### The agent roster for this work

| Job | Agent | Mode |
|---|---|---|
| Map a surface ("where is the faction card painted? what calls `draw_faction_swatch`?") | `rust-explorer` (Haiku, path:line) or `Explore`, or `cavecrew-investigator` for compressed output | **parallel**, read-only |
| Summarize `docs/UI_OVERHAUL.md` / verify egui-0.29 API via the `egui`/`eframe` skills | `Explore` / `general-purpose` | **parallel**, read-only |
| Sequence the overall plan | `Plan` | one-shot, read-only |
| Apply an approved recipe to a **builder panel** | `panel-implementer` (knows `BuilderState`/command pattern) | **parallel**, one panel per agent |
| Small bounded 1–2 file tweak | `cavecrew-builder` | single |
| Run golden + unit tests, report failures | `test-runner` | **background** |
| Work down clippy warnings | `clippy-fixer` | **background** |
| Review the resulting diff | `cavecrew-reviewer` | single |

> The hero **custom-paint widget and the token module live in `gui-core`
> (shared)** — that is **main-thread** work (single-writer, screenshot-iterated),
> *not* a `panel-implementer` job. `panel-implementer` is for **applying** the
> finished vocabulary inside the ~50 independent panel files.

### The pipeline (phases — agents per phase)

- **Phase A · Recon (parallel, read-only).** Fan out 4–5 agents at once: one each
  mapping `sector_view`/`map_theme`, the faction/swatch paint path, `nav`,
  `info_panel`; plus one summarizing `docs/UI_OVERHAUL.md`; plus one verifying
  the exact egui-0.29 signatures you'll use (`Rounding`, `epaint::Shadow`,
  `animate_bool_with_time`, `FontDefinitions`, `Mesh`, glow `PaintCallback`).
  Merge their path:line + API findings in the main thread.
- **Phase B · Tokens (serial, main thread).** Land `gui-core/src/design.rs` (§4),
  route `theme.rs`/`ui_kit.rs` through it, screenshot to confirm no regression.
  Single writer.
- **Phase C · Hero (serial, main thread).** Hand-paint **one** flagship component
  (§5/§6) with motion; iterate on screenshots until genuinely stunning; get user
  sign-off. Extract the reusable recipe (a card/plate fn, a hover curve, a shadow
  preset) into the shared kit.
- **Phase D · Propagation (parallel).** Now fan out `panel-implementer` agents,
  **one independent panel file each**, applying the approved recipe + tokens.
  Give every agent the *same* recipe spec so output is uniform. They do not
  invent visuals.
- **Phase E · Verify (background).** `test-runner` (incl. `cargo test --test it
  -- golden` if any shared/export code was touched — §8) and `clippy-fixer` run
  in the background; `cavecrew-reviewer` reviews the diff. Fix in the main
  thread.

### Two concrete dispatches

- *Recon fan-out:* "Dispatch in parallel — `rust-explorer`: (1) cite every paint
  call in `gui-core/src/sector_view.rs`; (2) trace the faction-card render in
  `builder/src/builder/panels/factions.rs` + swatch helpers in `palette.rs`; (3)
  map `nav.rs`. `Explore`: summarize `docs/UI_OVERHAUL.md §UO`. `general-purpose`:
  confirm 0.29 signatures for `Shadow`, `animate_bool_with_time`, `set_fonts` via
  the `egui` skill."
- *Recipe propagation:* "The approved card recipe is `design::card(ui, …)` +
  `design::ELEV_LOW` + 120ms eased hover. Dispatch `panel-implementer` in
  parallel, one per file, to replace the ad-hoc framed boxes in `factions.rs`,
  `world.rs`, `system.rs`, `routes.rs`, `regions.rs` with it. Do not change
  semantics or colors; visual swap only."

---

## 4. The token foundation — do this before any component (Phase B, serial)

> **Status: landed.** `gui-core/src/design.rs` already exposes everything below —
> extend it, don't recreate it. The spec is kept for reference and for the few
> primitives still worth adding (e.g. a registered display font, §5.5).

Beauty is *consistency in primitives*. Stunning UIs are stunning because every
radius, gap, shadow, and motion duration is drawn from a tiny disciplined set.
Create `gui-core/src/design.rs` exposing named constants/helpers:

- **Spacing scale** — `4, 8, 12, 16, 24, 32` (a 4px base grid). Every margin,
  gap, and pad references these, never a raw literal.
- **Radius scale** — e.g. `SM=4, MD=8, LG=12`. Pick *one* family and use it
  everywhere; mixed radii are the #1 tell of amateur UI. (egui 0.29 uses the
  `Rounding` type — note it was renamed `CornerRadius` in a *later* egui; we are
  on 0.29, so it is `Rounding`.)
- **Elevation / shadow recipes** — 2–3 named `epaint::Shadow` presets
  (`elev_low`, `elev_med`, `elev_high`) with deliberate `offset` / `blur` /
  `spread` / low-alpha `color`. Real depth = a soft large-blur ambient shadow +
  optional tight contact shadow. Avoid one harsh black drop shadow.
- **Motion** — durations (`FAST=0.08s`, `BASE=0.14s`, `SLOW=0.24s`) and an
  easing helper. egui animates via `ctx.animate_bool_with_time(id, on, dur)` →
  `f32` in `[0,1]` (it requests repaint while animating). Wrap it with an ease
  curve (e.g. ease-out-cubic) — linear interpolation looks mechanical.
- **Type scale** — display / title / section / body / caption sizes + intended
  weights. Consolidate with `ui_kit` consts.
- **Accent ramp** — derive hover/active/glow tints from the theme accent rather
  than hardcoding, so bespoke components recolor correctly across all 8 presets
  (read the active accent from `ui.visuals()` / the `Pal`).

Land this module, route `theme.rs` + `ui_kit.rs` through it, **screenshot to
confirm nothing regressed**, *then* start on components. This is a **single
writer** — do not parallelize it.

---

## 5. The beauty playbook — principles, each with the egui-0.29 technique

1. **One hero component to showcase quality, then propagate.** Do **not**
   "beautify the app" in one sweep — that yields uniform mediocrity. Pick a
   single hero (recommended: a **faction card**, or the **star-map frame**),
   iterate it to genuinely stunning with screenshots, *extract the patterns it
   taught you* (a card recipe, a shadow, a hover curve), then spread that
   vocabulary via Phase D agents (§3). Quality first, coverage second.

2. **Custom-paint the hero; don't restyle stock widgets.** The egui beauty
   ceiling is reached by dropping to the painter, not by tweaking
   `Visuals` on a default `Button`. Allocate space
   (`ui.allocate_exact_size` / `allocate_response`) and draw with
   `ui.painter()`: layered `Shape::rect_filled` / `rect_stroke`, hairline
   strokes, custom glyphs, `Shape::mesh` for gradients. Stock widgets are for
   the boring 80%; hand-paint the 20% that gets screenshotted. `Frame` (fill +
   stroke + rounding + shadow + inner_margin) is the workhorse container.

3. **Motion is half of "beautiful."** Static egui looks dead. Add, per
   interactive element: hover lift, press depress, focus ring fade, selection
   transition, and entrance for newly shown content. Drive every one off
   `animate_bool_with_time` / `animate_value_with_time` and **interpolate
   color/size/offset/shadow** with the eased `t`. Interpolate colors via
   `egui::Rgba` (linear space) then back to `Color32` — verify the exact
   helper against the egui skill. Subtle and fast (80–140ms) beats slow and
   showy.

4. **Depth, not flatness.** Use the elevation tokens. A card = base fill +
   `elev` shadow + 1px top-inner highlight stroke + hairline border. This single
   recipe ("two-tone edge": light hairline top, dark hairline bottom) is what
   makes a surface read as a *physical plate* instead of a colored rectangle.

5. **Typography is the cheapest 30% of beauty — and it's currently default.**
   No custom font is loaded. Register a display family via `FontDefinitions` +
   `ctx.set_fonts(...)` (`FontData::from_static`). For grimdark: a gothic /
   blackletter-adjacent *display* face for titles ONLY, a clean humanist sans
   for body, a mono for data/tables (the app already leans on `Monospace` for
   tabular panels). Set a real type scale + line-height + letter-spacing for
   caps headers. Never beautify without first fixing type — it touches every
   pixel.

6. **Gradients & texture via `Mesh`.** egui has no gradient-fill primitive. Build
   a `Mesh` with per-vertex `Color32` for vertical/radial fades (card sheen,
   vignette on the star field, gilded top-edge). A faint vignette + a few-percent
   noise/parchment overlay is the difference between "flat dark theme" and
   "aged Imperial chart." Keep it *subtle* — 3–8% alpha.

7. **Bespoke iconography.** Default glyph fonts look generic. Hand-draw small
   icons as `Shape` paths, or load an SVG set via `egui_extras`
   (`install_image_loaders`) + the `image`/`rfd` skills if you add assets. Icons
   should share the stroke weight and corner family of the token set.

8. **Spacing rhythm & alignment.** Snap every gap/margin to the 4px grid.
   Align labels into columns (the `field` helper already does this — extend it).
   Generous, *consistent* whitespace reads as premium; cramped or
   irregular whitespace reads as cheap. When unsure, add one spacing step.

9. **Diegetic color discipline.** Keep the palette tight: one void ground, one
   parchment surface, one brass accent, plus muted faction hues that already
   carry meaning. Beauty comes from *restraint + one confident accent*, not from
   many colors. The accent earns a soft glow (a low-alpha blurred underlay) on
   primary actions and the selected element — sparingly.

---

## 6. Concrete component targets, ranked by screenshot impact

Do them in roughly this order; each lists the file and what "showcase" means.
Pick **one** as the Phase C hero; the rest become Phase D propagation once the
recipe exists.

1. **The star-map frame & star field** (`gui-core/src/sector_view.rs`,
   `map_theme.rs`) — the hero. **Partly DONE:** the gilded corner-bracket frame,
   the void vignette, and the faint deterministic star dust have landed (live-only
   in `sector_view.rs`; export goldens green, live `map_snapshots` re-blessed).
   *Still open:* glowing route lines, per-system hover halos, and a smooth selection
   ring. **§8 confirmed:** the live view is fully decoupled from the deterministic
   exporters (separate `MapTheme`/`Canvas`/geometry) — flourishes there are safe.
2. **Faction card — DONE (the first hero).** The recipe landed as
   `gui-core/src/card.rs::selectable_plate` (`§BEAUTY`): a custom-painted,
   hover-/selection-animated plate (soft accent glow + brass selection bar +
   two-tone depth edge + hairline border), the accent read from the live theme so
   it works across all 8 presets. Applied to the Factions roster
   (`builder/src/builder/panels/factions.rs`) and propagated to the **World /
   System / Routes** rails (Phase D). The remaining list rails (Sites, Subsectors,
   Personae, Hooks, Missions, Search, Validation, Invariants) are straightforward
   propagation with the *same* recipe — call `card::selectable_plate`, keep any
   meaning-carrying glyph/swatch, don't reinvent the visual.
3. **Nav rail** (`gui-core/src/nav.rs`, `builder/.../panels/nav.rs`) — fixed-
   width §COLUMNS rail. **DONE:** the tab entries now route through
   `card::selectable_plate`, so the active tab is a lit brass-barred plate with a
   hover wash, consistent with every roster. *Optional refinement left:* a single
   bar that *slides* between tabs (vs. the per-row bar that grows in place today).
4. **Info panel** (`gui-core/src/info_panel.rs`) — tabular. Showcase = refined
   kv typography, aligned columns, hairline row separators, restrained accents.
5. **Section containers** (`ui_kit::section`) — lifts every panel at once.
   **DONE:** `section` / `collapsing_section` now carry an `elev_low` contact
   shadow, and `section` rules its title with a brass `gilt_rule` (active accent)
   in place of the flat `separator()`.
6. **Primary buttons & toggles** — bespoke painted variants (filled brass
   primary with press/hover/glow states; a custom sliding toggle) to replace the
   stock look on prominent actions.
7. **Dialogs / modals** (`builder/.../panels/*` confirm flows) — elevated
   surface, scrim behind, `elev_high` shadow, entrance fade/scale.

---

## 7. The egui-0.29 reality check — and the escape hatch

egui is immediate-mode and **not** CSS/SwiftUI. Know the ceiling so you don't
silently fake-and-fall-short. (Have a Phase-A agent confirm each signature.)

- **Verify every API against the version.** We are on **egui/eframe 0.29**.
  Names drift between releases (`Rounding`→`CornerRadius`, shadow fields,
  `id_salt` vs `id_source`). **Invoke the `egui` and `eframe` skills** to
  confirm exact 0.29 signatures before coding. Do not trust memory of a newer
  API. (Gotchas confirmed this pass, for reuse: `ViewportCommand::Screenshot` is a
  **unit** variant in 0.29 — `UserData` is 0.30+; `Color32` has **no** `lerp` — use
  `Color32::lerp_to_gamma`; `epaint::Shadow` fields are `Vec2`/`f32`, not `i8`;
  `Ui::allocate_ui_at_rect` is deprecated for `allocate_new_ui`; corner radius is
  still `Rounding`, not `CornerRadius`.)
- **What's native & easy:** `Frame` (fill/stroke/rounding/shadow/margin),
  painter shapes, `Mesh` gradients, `animate_*` motion, custom fonts, scrims,
  per-vertex color, clip rects.
- **What's hard / not native in 0.29:** true Gaussian **backdrop blur**
  (glassmorphism frost), rich multi-stop gradients beyond mesh fades, real drop
  shadows with blur on arbitrary shapes (egui `Shadow` is rect-oriented),
  advanced text (kerning control, SDF glow, gradient text). **Approximate**
  these with mesh + layered translucent shapes first.
- **The escape hatch — glow shader callback.** For effects egui can't do
  (animated nebula in the void field, real blur, bloom on route lines), this is
  the **glow** backend: render a custom GL pass via `egui::PaintCallback` +
  `egui_glow::CallbackFn`. Reach for this **only** when a flagship effect
  demands it and the cheaper mesh approximation looks wrong — it's powerful but
  costs portability and complexity. Confirm the 0.29 callback API via the skills
  first.

---

## 8. Safety rails — do not break these while beautifying

- **Determinism / golden output (critical).** The PNG/SVG/HTML exporters and the
  map renderer are **byte-stable and golden-tested**. Beautifying the *live egui
  chrome* (panels, cards, nav, buttons, dialogs, fonts, motion) does **not**
  touch them and is safe. But the live star-map view may **share rendering code
  with the exporters** (`bitmap`, `svg_export`, `html_export`, `render`,
  `map_theme`). If a map-beautification edit reaches shared/export code, you
  **must** keep golden tests green (run via a background `test-runner`):
  ```bash
  cargo test --test it -- golden
  ```
  When in doubt, isolate live-only flourishes (glow, vignette, hover halos)
  behind the live render path so exporter bytes don't move. Prefer adding a
  live-only painter over editing a shared one.
- **Don't recolor the *semantic* map palette.** Faction / hazard / route / region
  colors in `palette.rs` + `visual_tokens.rs` carry meaning and stay stable
  across themes (see the note atop `theme.rs`). Beautify *form, depth, motion,
  framing, glow* — not the meaning-bearing hues. **Tell every propagation agent
  this explicitly** so a fan-out doesn't recolor data.
- **Mutations go through the command bus.** In the builder, never write
  `BuilderState` fields directly from a panel — call `state.run(BuilderCommand::
  ...)`. Pure visual work usually touches no state, but if a flourish needs
  persisted data, route it through a command (§R4 / CLAUDE.md). `panel-implementer`
  knows this rule; restate it in the dispatch anyway.
- **Keep the kit `BuilderState`-free.** `ui_kit.rs` and `nav.rs` take `&mut Ui` +
  plain data. New shared beauty widgets follow the same rule so the viewer can
  use them too.
- **Never read or touch `old/`.** (CLAUDE.md.)
- **Respect the square-sector invariant** — irrelevant to chrome, but don't add
  any UI path that lets sector width/height diverge.
- **House-keeping after a non-trivial change:** `cargo fmt --all`,
  `cargo clippy --workspace --all-targets -- -D warnings` (background
  `clippy-fixer`), `cargo test -p sectorforge-builder` (+ `-p sectorforge-gui-core`,
  background `test-runner`), and update **GUIDE.md**. If you touched anything
  map/export-shared, also run the golden suite above.

---

## 9. Definition of done (per round, and overall)

A component is "showcase done" when, at the screenshot:

- Every gap/radius/shadow comes from a **token**, not a literal.
- It has **at least one animated state** (hover/press/select) with eased motion.
- It reads as having **depth** (edge highlight + shadow), not as a flat rect.
- **Typography** is intentional (display face for the title, real scale/weight).
- It is **coherent with the diegetic theme** and **survives `Light`/`Slate`**.
- You looked at it and would actually put it in a showcase. If you'd hesitate,
  it's not done — name the flaw and iterate.

After a fan-out (Phase D), spot-check **every** touched panel in a screenshot —
agents are blind, so a uniform recipe can still land wrong in an odd layout.

---

## 10. Anti-patterns — the things that keep egui ugly (and the orchestration)

- Beautifying **blind** (no screenshot). The cardinal sin (§0).
- **Letting a subagent make an aesthetic call** — they can't see; they only
  apply approved recipes (§3).
- **Two agents editing the same shared file** (`design.rs`/`theme.rs`/`ui_kit.rs`/
  `palette.rs`) — clobbered edits. Single-writer only (§3).
- **Splitting one component** into plan→code→test agents — context dies at each
  handoff. Coherent components stay in one place; parallelism is for *breadth*
  and *propagation*, not phases of one change (§3).
- Restyling stock widgets and calling it done — no bespoke painting, no depth.
- **Mixed radii / off-grid spacing** — instant amateur tell.
- A **global garish gradient** or many competing accent colors — restraint wins.
- **No motion** — or slow/janky motion (forgetting eased curves; not realizing
  `animate_*` already requests repaint).
- One **harsh black drop shadow** instead of a soft layered elevation.
- **Default fonts** left in place while polishing everything else.
- Doing a **whole-app sweep** before nailing a single hero — spreads mediocrity.
- Editing a **shared/export** renderer for a live-only flourish and breaking
  goldens.

---

## 11. Ready-to-paste kickoff prompt (fill the brackets)

> Read `BEAUTY.md` fully, including the §3 multi-agent execution model. We're
> making the **[star-map frame / nav rail / info panel]** showcase-beautiful,
> Grimdark theme first. Aesthetic reference: **[paste screenshot / link, or
> "Imperial cartographic instrument: parchment + brass + void-black"]**.
>
> 1. **Phase A (parallel agents):** dispatch `rust-explorer`/`Explore` to map the
>    target's paint path, summarize `docs/UI_OVERHAUL.md §UO`, and verify the
>    egui-0.29 APIs I'll need. Merge findings here.
> 2. **Phase B — already landed.** `gui-core/src/design.rs` (tokens) and
>    `gui-core/src/card.rs` (the `selectable_plate` recipe) exist, and
>    `theme.rs`/`ui_kit.rs` route through them. Don't rebuild them — *extend*
>    `design.rs` if your component needs a new primitive (e.g. a display-font
>    family, §5.5).
> 3. **Phase C (main thread):** hand-paint the **[component]** per §5 — custom
>    painter, eased hover + selection motion, elevation, a real display font.
>    Bespoke, not restyled stock. Screenshot every round via `/run`; critique
>    honestly; iterate until it matches the reference. Then extract the recipe.
> 4. **Phase D (parallel `panel-implementer`):** once I approve the recipe, fan
>    out one agent per panel to apply it — same spec to all, no invented visuals,
>    don't touch semantic colors.
> 5. **Phase E (background):** `test-runner` (incl. goldens if any shared/export
>    code moved) + `clippy-fixer`; keep §8 safety rails.
>
> Show me the screenshot after each iteration before moving on. Keep all
> aesthetic decisions in the main thread — agents do recon, propagation, and
> verification only.

---

*Companion docs: `docs/UI_OVERHAUL.md` (§UO — the coherence pass this builds on),
`CLAUDE.md` (determinism + command-bus + subagent-routing invariants), `GUIDE.md`
(project map).*
