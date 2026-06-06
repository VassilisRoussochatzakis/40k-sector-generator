# SPRUCE_CHANGELOG.md

Agentic UI-polish pass per [SPRUCE.md](SPRUCE.md). Committed to `main` as `84ee826`
(`feat(gui): theme-aware status colors + role buttons (§SPRUCE)`). All changes are
**chrome only**; no map painter, export writer, or command-bus logic was touched.

---

## Recon (SPRUCE Phase 0)

**egui version.** Pinned to **egui / eframe 0.29.1** (`builder/Cargo.toml`,
`viewer/Cargo.toml`, `gui-core/Cargo.toml`). API spelling for this version, already
codified in [`gui-core/src/design.rs`](gui-core/src/design.rs) and reused unchanged:

- Corner radius is **`egui::Rounding`** with **`f32`** fields (`Rounding::same(8.0)`);
  the rename to `CornerRadius` (u8) is 0.31+. `Visuals::window_rounding`,
  `WidgetVisuals::rounding`, `Frame::rounding` all use `rounding` + `Rounding`.
- **`Shadow`** carries `offset: Vec2`, `blur: f32`, `spread: f32`, `color: Color32`
  (the integer `[i8;2]`/`u8` form is later).
- **`Margin`** is `f32`-based (`Margin::same(8.0)`).
- Color blending is `Color32::lerp_to_gamma` (no `Color32::lerp` in 0.29).

**Theming architecture (the critical finding).** The repository had already
completed two large UI passes before this runbook — `§UO` (UI_OVERHAUL.md) and
`§BEAUTY` (BEAUTY.md). So **most of SPRUCE was already implemented**:

| SPRUCE defect | Status before this pass | Where |
|---|---|---|
| D1 type scale (display/body/mono split) | **Done** | `design.rs` `DISPLAY/TITLE/SECTION/BODY/DIM/CAPTION` + bundled OFL fonts (`fonts.rs`) |
| D2 elevation / 3 bg tiers | **Done** | `design.rs` `elev_low/med/high`; `theme.rs` window/panel/faint tiers; `ui_kit::section` shadows |
| D3 button roles | **Partial** — brass `primary_button` + `toggle` existed and were wired into 14 panels, but no danger/ghost roles, and PROJECT "New project…" was a plain button |
| D4 monospace data/telemetry | **Missing** — status bar + PROJECT Details were proportional |
| D5 accent usage | **Done** | accent → hyperlink/selection/hover stroke + `accent_*` ramp in `design.rs` |
| D6 nav bar+tint (not solid fill) | **Done** | `card::selectable_plate` (accent glow wash + growing brass left bar) hosts the nav rail |
| D7 semantic status colors | **Missing** — no theme-aware success/warning/danger/info; ~80 ad-hoc `from_rgb` amber/red/green triples in ~25 panels; warm amber collided with the Grimdark brass accent |
| D8 Details grid / 4px spacing | **Done** | `ui_kit` fixed-column field rows + 4px `design` spacing scale |
| D9 radius/padding consistency | **Done** | one `RADIUS_*` family + spacing tokens, centrally applied |
| D10 motion | **Done** | `animate_bool_with_time` in `primary_button`/`toggle`/`selectable_plate`/`modal` |
| D11 column rhythm | **Done** | `ui_kit::columns_responsive` / `reading_column` |

`Theme` has 8 presets (`Grimdark` default + 6 dark + `Light`). Each expands a flat
`Pal` into `Visuals`; `Theme::apply` also pushes a process-wide `ChromeColors`
snapshot (`palette::set_chrome`) that the custom painters read without a `&Ui`.
**The `Pal` exposed `accent` only — no semantic status roles.** That absence is the
root of D7 and the focus of this pass.

`cargo check` was clean before any edit (baseline established).

---

## Changes by phase

### Phase A — the missing token family (gui-core)

- **`gui-core/src/palette.rs`** — added `StatusColors { success, warning, danger,
  info }` with `DARK` (SPRUCE §3.1 Grimdark values) and `LIGHT` (darkened for
  parchment; `danger` pushed vermilion to stay distinct from the crimson accent)
  constants, a `RwLock` snapshot mirroring `ChromeColors`, and accessors
  `success()` / `warning()` / `danger()` / `info()` + `validation_color(errors,
  warnings)` (muted at 0/0 → warning → danger).
- **`gui-core/src/theme.rs`** — `Theme::apply` now calls `palette::set_status(DARK |
  LIGHT)` keyed on the preset's `dark` flag. Extended the existing theme test to
  assert the set flips and that `warning() != accent` (the D7 invariant).
- **`gui-core/src/widgets.rs`** — added `danger_button` and `ghost_button`, both
  hand-painted + eased off `animate_bool_with_time` to match `primary_button`, and
  extended the headless paint test to cover them.

### Phase C — monospace telemetry (D4)

- **`builder/src/builder/panels/status.rs`** — full rewrite: every segment now
  monospace; health/`dirty`/`unsaved`/error colours routed through the new
  `palette` status set (was `Color32::GREEN`/`RED` + raw amber `from_rgb`).

### Phase D — role buttons wired (D3)

- **`builder/src/builder/panels/project.rs`** — "New project…" → `primary_button`;
  Details values (ID/Seed/Size/Folder/Version) → **monospace**; unsaved indicator +
  page subtitle + snapshots help → `palette::warning()` / `chrome_text_dim()`.
- **`builder/src/app.rs`** — both central confirm dialogs (`ConfirmDeleteFaction`,
  `ConfirmDestructive`): Delete → `danger_button`, Cancel → `ghost_button`, the
  "This can't be undone." line → `palette::danger()`.

### Phase B — chrome `from_rgb` migration (D7), dispatched across parallel agents

Migrated ~80 hardcoded chrome status literals in **22 panels** to `palette::*()`:

- *Diagnostics + consts* — `validation.rs` (`COLOUR_ERROR/WARNING/OK` consts deleted),
  `invariants.rs` (`SEVERITY_RED`), `segmentum.rs` (`WARN/ERR/OK` consts),
  `generation.rs`, `files.rs` (status line only — TOML syntax highlighter left as
  data-viz).
- *Lore badges* — `prose.rs`, `sites.rs`, `history.rs`, `hooks.rs`, `missions.rs`,
  `briefing.rs`, `personae.rs`, `intel.rs` (hand-written / default / GM-only / delete).
- *Editors + search/diff + reports* — `factions.rs`, `worlds_editor.rs`, `search.rs`,
  `diff.rs` (+added→success / −removed→danger / ~changed→warning), `analytics.rs`,
  `interestingness.rs`.
- *Residual `LIGHT_RED/GREEN/YELLOW`* — `economy.rs`, `control.rs`, `regions.rs`,
  `relations.rs`, `routes.rs`, `subsectors.rs`.
- *Final `from_rgb` sweep* — 20 more chrome sites: `economy` (re-derive + strategic
  tier), `control` (contested), `relations` (`tension_text`/`metric_text` tiers +
  defaults notice), `routes` (length/save/cluster advisories), `surface_regions`
  (population over-100), `system` + `world` (pinned / coupling-⚠ / "manual" badges),
  `regions` (worse-routes count), `export` (error).

**Left untouched (data-viz, by the §UO8 rule):** faction fills/accents,
route-stability, world-type, `ClaimType` chip tuples (`control.rs`/`world.rs`),
relation-attitude hues, heatmap + chart series.

### Docs

- **GUIDE.md** §8.0 — new "Semantic status colors + role buttons" subsection.
- **BUILDER.md** — intro bespoke-controls + confirm-dialog paragraphs, §0.3 + §12.1
  footer, §1.1 New project: monospace footer, state colouring, danger/ghost buttons.

---

## Acceptance checklist (SPRUCE §8)

- [x] **D1** — type scale (display/body/mono) in place (pre-existing §UO/§BEAUTY).
- [x] **D2** — cards float above panels; three bg tiers distinguishable (pre-existing).
- [x] **D3** — "New project…" is the brass primary; delete confirms use the danger
  button + ghost Cancel; secondaries stay quiet; disabled buttons read disabled.
- [x] **D4** — the whole status bar + the PROJECT Details data values are monospace
  and aligned. *(Deviation: per-entity inspectors elsewhere still show some IDs in
  proportional — see below.)*
- [x] **D5** — accent on primary buttons, active nav bar, plate selection, focus
  (pre-existing).
- [x] **D6** — active nav uses the brass bar + tint plate, not a solid fill (pre-existing).
- [x] **D7** — `validation: 0 err / 0 warn` is **muted, not amber**; `warning()` is a
  distinct orange from the brass accent (asserted in a theme test); ~80 chrome
  literals migrated to the theme-aware set.
- [x] **D8** — Details is an aligned field grid on the 4px scale (pre-existing layout;
  values now monospace).
- [x] **D9** — one radius/padding family, centrally applied (pre-existing).
- [x] **D10** — hover/press/selection feedback via `animate_bool_with_time`
  (pre-existing; the two new role buttons follow suit).
- [x] **D11** — responsive columns / reading-width (pre-existing).
- [x] **No hardcoded `from_rgb` in widget chrome** — all status chrome (including the
  power panels' `tension_text` / `metric_text` tiers, the pinned / coupling / "manual"
  badges, and the route advisories) now routes through `palette`. Only **data-viz**
  `from_rgb` remains by design (faction / `ClaimType` / `RelationAttitude` / syntax /
  chart) plus neutral `GRAY` / `DARK_GRAY` muted text.
- [x] **All existing themes still render; no widget branches on theme identity** —
  status colours are keyed only on dark/light, read via accessors.
- [x] **`cargo build`/`check` clean; no new clippy warnings; tests pass.**
- [x] **App still sleeps when idle** — no continuous animation added; the two new
  buttons animate only on hover/press like the existing ones.

---

## Deviations from the runbook (with justification)

1. **Most phases were already done.** SPRUCE Phase 1–6 describe building tokens,
   elevation, fonts, plates, motion, and columns that §UO/§BEAUTY already shipped.
   Per SPRUCE §0 ("prefer the codebase's actual structure"), this pass *extended*
   the existing system (a `StatusColors` sibling to `ChromeColors`; `danger`/`ghost`
   siblings to `primary_button`) rather than re-implementing it.
2. **Status colors are keyed dark/light, not per-preset** (SPRUCE §3.1 implies one
   value per role *per theme*). Semantic colours encode *meaning*, not brand, so a
   single dark set + a single light set keeps a red error red everywhere and — more
   importantly — guarantees the warning hue never coincides with a preset's own
   accent (the exact D7 failure mode). Two sets, not sixteen.
3. **Monospace scope is status bar + PROJECT Details**, not literally every ID in
   every inspector (SPRUCE D4 lists "all IDs"). The screenshot's defects (the status
   line and the Details block) are fixed; mono-ing every per-entity inspector field
   was judged gold-plating (SPRUCE §0.6).
4. **The power-panel `from_rgb` sweep was completed in a follow-up** (20 sites across
   economy / control / relations / routes / surface_regions / system / world / regions
   / export). `relations::tension_text`'s 40/15 tiers collapsed onto one `warning()`
   tier (the palette has no fourth warm step — clippy's `if_same_then_else` confirmed
   the collapse). Data-viz colours stayed untouched.
5. **Committed to `main`.** Landed as commit `84ee826`
   (`feat(gui): theme-aware status colors + role buttons (§SPRUCE)`).

---

## Verification

```
cargo check --workspace --all-targets   # clean
cargo clippy -p sectorforge-gui-core -p sectorforge-builder   # 0 warnings
cargo test -p sectorforge-gui-core       # 31 passed
cargo test -p sectorforge-builder        # 309 passed
cargo test --test it -- golden           # 13 passed (exports byte-stable — D7 is chrome-only)
```

The golden suite passing confirms the determinism invariant held: nothing in the
PNG/SVG/HTML export path changed.
