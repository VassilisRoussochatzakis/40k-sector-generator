# COLUMNS.md — Builder UI layout overhaul (instructions for the LLM)

> **Audience: you, Claude Code, working in this repo.** This is a playbook, not a
> spec for humans. It tells you *what is wrong with the builder layout*, *the
> egui patterns to fix it*, *the exact files to touch*, and *the invariants you
> must not break while doing it*. Read it top-to-bottom before changing layout
> code. When you act on it, work in the small phases in §9, run the tests in §10
> after each phase, and update [GUIDE.md](GUIDE.md) + tag commits with the right
> `§` marker (see [CLAUDE.md](CLAUDE.md)).

---

## 0. The one-paragraph problem

Every builder tab renders as a **single vertical column** inside
`ScrollArea::vertical().auto_shrink([false; 2])`, stacking titled
`ui_kit::section` boxes top-to-bottom. Each section frame stretches to the full
panel width, but its *contents* are narrow (a 190 px combo, a 150 px label + a
control). The default window is **1400×900** (`builder/src/main.rs:34`), so a
section that holds one dropdown paints ~340 px of widgets and ~1000 px of dead
space — then the next section starts a full row lower. `world.rs` stacks **18**
such sections (`builder/src/builder/panels/world.rs:46`). The result is a tall,
skinny ribbon of controls down the left edge with a vast empty gutter on the
right, and everything important is below the fold. **The fix is not to remove
the scroll area — it is to put multi-column / master-detail / width-bounded
content *inside* it.**

---

## 1. Diagnosis — the evidence in the tree

Confirm these before and after your changes (they are the metrics you are moving):

| Signal | Command | Today | Why it matters |
|---|---|---|---|
| Panels using a vertical one-column scroll | `grep -rc 'auto_shrink(\[false' builder/src/builder/panels` | ~26 | The default layout *is* the bug. |
| Panels using true side-by-side columns (`ui.columns`) | `grep -rln 'ui.columns(' builder/src/builder/panels` | **1** (`system.rs`) | Almost nothing splits horizontally. |
| Panels using master-detail (`SidePanel`) | `grep -rln 'SidePanel' builder/src/builder/panels` | **1** (`factions.rs`) | List-shaped tabs scroll instead of pinning a roster. |
| Panels that adapt to width (`available_width`) | `grep -rln 'available_width' builder/src/builder/panels` | **1** (`system.rs`) | No responsiveness; narrow windows crush, wide windows waste. |
| Any width cap on readable text | `grep -rn 'set_max_width\|max_width' builder/src/builder/panels` | **0** | Prose/briefing run edge-to-edge → unreadable line length. |
| Tabs | `BuilderTab::ALL.len()` (`state/tests.rs:164`) | **26** | The top strip (`nav.rs:88`) wraps to multiple rows at 1400 px. |

**Anti-pattern exemplar (study, then fix):**
`builder/src/builder/panels/world.rs:46-97` — `ScrollArea::vertical` wrapping 18
`show_*_section` calls separated by `ui.add_space(4.0)`. Each section is
`ui_kit::section` / `collapsing_section` (`gui-core/src/ui_kit.rs:44`), whose
`Frame::group` fills the available width.

**Good exemplars (copy these shapes):**
- Equal split: `builder/src/builder/panels/system.rs:79-119` —
  `ui.columns(2, |cols| { /* left: identity…  right: overlays… */ })`. Note it
  reborrows `state` on each `show_*_section(&mut cols[i], state, idx)` call.
- Master-detail: `builder/src/builder/panels/factions.rs:99-113` —
  `SidePanel::left(..).resizable(true).default_width(280.0).show_inside(ui, ..)`
  for the roster, `CentralPanel::default().show_inside(ui, ..)` for the
  inspector.

**Chrome shell (don't regress this — it is the §UO work):**
`builder/src/app.rs:53-87` — top `TopBottomPanel` (theme menu + workspace tabs +
`nav::show_top_bar`), bottom `TopBottomPanel` (`status::show`), and the
`CentralPanel` that calls `nav::show_active_panel`. Tabs are grouped into 7
labeled clusters in `nav.rs:34` (`TAB_CLUSTERS`).

---

## 2. Design targets

When you are done, each tab should satisfy as many of these as apply:

1. **No section paints more than ~40 % dead horizontal space** at 1400 px when
   its content is narrow. If a section's controls are < ~half the panel width,
   it must share its row with another section (columns) or sit in a fixed-width
   rail (master-detail) — not stretch alone.
2. **List-shaped tabs keep their list visible.** If a tab is "pick one of N,
   then edit it", the N must be a persistent left roster (master-detail), not a
   combo you reopen every time. The detail pane fills the rest.
3. **Form/inspector tabs flow into 2–3 columns** above a width threshold, and
   **collapse to 1 column** below it. Never a hard 2-column split that crushes a
   narrow window.
4. **Readable text is width-capped** (~720 px) and left-aligned — prose,
   briefing previews, long help/markdown.
5. **The window's vertical budget is respected.** Prefer wide over tall: a user
   should see more at once, scroll less.
6. **Resizable splitters persist.** Use `SidePanel`'s built-in resize (egui
   stores the width by panel `Id` automatically — no new state field).

---

## 3. The egui toolbox (this repo is egui **0.29**, no `egui_extras`)

Pick the smallest tool that fits. In rough order of preference:

### 3.1 `ui.columns(n, |cols| …)` — equal-width side-by-side
Best for inspectors whose sections are independent boxes (the `system.rs`
shape). `cols` is `&mut [egui::Ui]` of length `n`. Call your section fns against
`&mut cols[i]`, reborrowing `state` each call:

```rust
ui.columns(2, |cols| {
    show_identity_section(&mut cols[0], state, idx);
    show_overlays_section(&mut cols[1], state, idx);
});
```

**Borrow-checker note (this is the #1 thing you will trip on):** you *cannot*
build a `Vec<Box<dyn FnOnce(&mut Ui)>>` where each closure captures
`&mut state` — that's multiple simultaneous mutable borrows and will not
compile. The closures must run *sequentially* against the slice, each taking
`state` fresh. That is exactly what `system.rs` does and what the helper in §4
preserves. Selection of which section goes in which column is made at call
sites, inline.

### 3.2 `SidePanel::…::show_inside` + `CentralPanel::…::show_inside` — master-detail / asymmetric
Best for list+detail, or any "narrow fixed rail beside a wide pane". `columns`
can't do unequal widths; `SidePanel` can, and it's resizable for free:

```rust
egui::SidePanel::left("world_roster")        // Id MUST be unique per tab
    .resizable(true)
    .default_width(260.0)
    .width_range(180.0..=420.0)
    .show_inside(ui, |ui| show_roster(ui, state));
egui::CentralPanel::default()
    .show_inside(ui, |ui| show_inspector(ui, state));
```

Rules: unique `Id` string per tab (collisions silently mis-size). You can nest a
`ui.columns(2, …)` *inside* the `CentralPanel` to get roster + 2-column
inspector. This composes cleanly inside the app's outer `CentralPanel`.

### 3.3 `egui::Grid` — dense aligned key/value or matrices
Already used widely (`Grid::new(..).num_columns(2)`). Keep it for tight field
grids and for true matrices (diplomacy in `relations.rs`, control in
`control.rs`). Wrap a wide grid in `ScrollArea::horizontal` only when columns
are genuinely unbounded.

### 3.4 Width-bounded content — for readable text
No helper exists today. Add `ui_kit::reading_column` (§4) and use it for
`prose.rs`, `briefing.rs`, long help text, markdown previews. Pattern:

```rust
let w = ui.available_width().min(720.0);
ui.allocate_ui(egui::vec2(w, ui.available_height()), |ui| { ui.set_width(w); add(ui) });
```

### 3.5 Dashboard / card grid — for read-only metric panels
`analytics.rs`, `interestingness.rs`, `segmentum.rs`. Lay metric `ui_kit::section`
cards across responsive columns (§4 `columns_responsive`) so a 20-metric report
is a 3–4 column board, not a 20-row stack.

### 3.6 `egui_extras::TableBuilder` — *optional*, needs a new dep
Genuinely tabular tabs (worlds_editor, validation lists, mission/hook/site rows)
would benefit from real sortable/virtualized tables. **`egui_extras` is not a
dependency today.** Do **not** add it casually — propose it to the user first
(it is a workspace dep change). Until then, use `Grid` inside a scroll area.
When/if added, pin `egui_extras = "0.29"` to match `egui`.

---

## 4. Shared helpers to add to `gui-core/src/ui_kit.rs`

Add these next to the existing `section` / `field` / `combo` helpers. They take
`&mut Ui` + plain data only — **no `BuilderState`** (same rule the module header
states). Add a line for each to the headless `widgets_paint_headless` test at
the bottom of the file so they're smoke-covered.

### 4.1 `columns_responsive` — the core fix
```rust
/// Like [`egui::Ui::columns`] but chooses the column count from the available
/// width: up to `want` columns while each keeps ≥ `min_col_w`, otherwise fewer,
/// down to 1. The closure receives a slice of the chosen length — it MUST
/// handle `cols.len() == 1` (everything stacked on a narrow window). (§COLUMNS)
pub fn columns_responsive<R>(
    ui: &mut Ui,
    want: usize,
    min_col_w: f32,
    add: impl FnOnce(&mut [Ui]) -> R,
) -> R {
    let spacing = ui.spacing().item_spacing.x;
    let avail = ui.available_width();
    let fit = ((avail + spacing) / (min_col_w + spacing)).floor() as usize;
    let n = fit.clamp(1, want.max(1));
    ui.columns(n, add)
}
```

Call-site pattern for converting a stacked panel. A tiny local macro keeps the
sequential-reborrow rule (§3.1) painless and lets sections flow round-robin:

```rust
ui_kit::columns_responsive(ui, 2, 460.0, |cols| {
    let n = cols.len();
    let mut next = 0usize;
    macro_rules! col { () => {{ let c = &mut cols[next % n]; next += 1; c }} }
    show_identity_section(col!(), state, idx);
    show_classification_section(col!(), state, idx);
    show_environment_section(col!(), state, idx);
    show_society_section(col!(), state, idx);
    // …remaining sections…
});
```

Round-robin gives even column heights but splits logical groups. When grouping
matters more than balance (e.g. keep Identity + Classification together),
hand-assign like `system.rs` does instead of the macro. Choose per panel.

### 4.2 `reading_column` — width-cap readable text (§3.4)
```rust
/// Constrain `add` to at most `max_w` (default call: 720.0) and left-align it,
/// so prose / markdown / help text keep a readable line length on a wide
/// window instead of running edge-to-edge. (§COLUMNS)
pub fn reading_column<R>(ui: &mut Ui, max_w: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let w = ui.available_width().min(max_w);
    ui.allocate_ui(egui::vec2(w, 0.0), |ui| { ui.set_width(w); add(ui) }).inner
}
```

### 4.3 `master_detail` — optional convenience wrapper
Only add this if you find yourself repeating the `SidePanel` + `CentralPanel`
boilerplate more than ~3 times. Keep the `Id` a required parameter so callers
can't collide:

```rust
/// Resizable left list + filling detail pane. `id` MUST be unique per tab.
pub fn master_detail(
    ui: &mut Ui,
    id: &str,
    default_w: f32,
    list: impl FnOnce(&mut Ui),
    detail: impl FnOnce(&mut Ui),
) {
    egui::SidePanel::left(id).resizable(true).default_width(default_w)
        .show_inside(ui, list);
    egui::CentralPanel::default().show_inside(ui, detail);
}
```
Same borrow caveat: `list` runs fully before `detail`, so each may take
`&mut state` in turn — fine. Do **not** try to capture `&mut state` in both at
once.

### 4.4 `field` upgrade (optional)
`ui_kit::field` (`ui_kit.rs:85`) hard-codes a 150 px label column. In a narrow
master-detail column that can be too wide. Consider a `field_w(ui, label, w, add)`
variant taking the label width, so dense rails can use 110 px. Low priority.

---

## 5. Per-tab playbook

All 26 tabs, grouped by their `TAB_CLUSTERS` home (`nav.rs:34`), with the target
layout and the file to edit (`builder/src/builder/panels/<file>`). "MD" =
master-detail (§3.2); "RC-n" = `columns_responsive` to n columns (§4.1); "Cap" =
`reading_column` (§4.2); "Grid"/"Matrix" = aligned `egui::Grid`.

### BUILD
| Tab | File | Today | Target |
|---|---|---|---|
| Project | `project.rs` | buttons + tree, 1-col | RC-2: actions/metadata left, project tree + recent right. |
| Map | `map/mod.rs` | canvas + toolbox | Keep canvas central & filling; move tools to a left `SidePanel` rail, keep selection/inspector in a right `SidePanel` so the map never shrinks to a strip. |
| Subsectors | `subsectors.rs` | cluster list, 1-col | MD: cluster roster left, cluster detail (capital/colour/reassign) right. |
| Regions | `regions.rs` | table+paint+glyph+config | 3-pane: region table (left rail), paint/glyph map (center), config (right rail) via nested `SidePanel`s. |
| Routes | `routes.rs` | summary + 1-col list | MD: route roster left, route editor right. Summary stays full-width on top. |

### ENTITIES
| Tab | File | Today | Target |
|---|---|---|---|
| System | `system.rs` | picker + 2-col (good) | Promote the combo to an MD roster rail; keep the existing `ui.columns(2)` inspector but swap it for RC-2 so narrow windows collapse. The §CTX0 in-system map already reads `available_width` — keep it filling the inspector top. |
| World | `world.rs` | picker + **18-section stack** | **The flagship conversion.** MD roster left (reuse `selected_world_id`); inspector right as RC-2/RC-3 over the 18 sections. This single change removes most of the wasted space in the app. |
| Factions | `factions.rs` | MD (good) | Keep MD. Make the *inspector* RC-2 (identity/disposition left, borders/claims/relations right) — it currently stacks inside the central pane. |
| Sites | `sites.rs` | per-world editor, h-scroll | MD: world roster left, site table for that world right (Grid). |

### POWER
| Tab | File | Today | Target |
|---|---|---|---|
| Control | `control.rs` | presence/dominance/claims, 1-col | RC-2 for the editors; render presence/dominance as a Grid matrix where it's per-faction. |
| Economy | `economy.rs` | overrides+toml+heatmaps, 1-col | RC-2: override editor + toml on the left, heatmap/lifelines preview on the right (preview wants width). |
| Relations | `relations.rs` | diplomacy matrix, 1-col | Full-width **Matrix** Grid on top; per-pair override editor in a right `SidePanel` that opens on cell-select. |

### LORE
| Tab | File | Today | Target |
|---|---|---|---|
| History | `history.rs` | config+eras+events+timeline, h-scroll | RC-2: config/eras/rules left, event list + timeline right. Consider MD for the event list. |
| Personae | `personae.rs` | kind pool + per-anchor, h-scroll | MD: persona/anchor roster left, persona detail right. |
| Hooks | `hooks.rs` | ranked list, h-scroll | MD: hook list left, hook detail + anchor link right. Keep click-to-highlight. |
| Missions | `missions.rs` | mission list, h-scroll | MD: mission list left, mission detail right. |
| Prose | `prose.rs` | per-system + overview text | RC-2: system selector/list left, **Cap**-bounded text editor right. |
| Briefing | `briefing.rs` | md preview | RC-2: profile/config left, **Cap**-bounded rendered preview right. |

### ANALYZE
| Tab | File | Today | Target |
|---|---|---|---|
| Analytics | `analytics.rs` | report, 1-col | Dashboard: metric `section` cards across RC-3/RC-4 (§3.5). |
| Interestingness | `interestingness.rs` | score+config, 1-col | RC-2: config left, score-breakdown cards right (or dashboard). |
| Search | `search.rs` | query + results | MD: results list left, hit detail right. Query bar full-width on top. |
| Diff | `diff.rs` | compare A/B | RC-2 **side-by-side A vs B** — naturally columnar; align rows. |

### OUTPUT
| Tab | File | Today | Target |
|---|---|---|---|
| Segmentum | `segmentum.rs` | compose + grid | Dashboard grid of sector tiles (RC-n); compose controls in a top bar or left rail. |
| Export | `export.rs` | settings + preview | RC-2: bundle/format settings left, manifest + markdown/bitmap preview right (preview wants width + **Cap** for markdown). |

### CHECK
| Tab | File | Today | Target |
|---|---|---|---|
| Validation | `validation.rs` | error list + focus | MD: error/rule list left, error detail + "focus" deep-link right. |
| Invariants | `invariants.rs` | violation list | MD: same shape as Validation. |

> `intel.rs`, `orbital.rs`, `conflict.rs`, `surface_regions.rs` are **not tabs** —
> they export `show_*_section` fns embedded into the World/System inspectors.
> When you make World/System RC-2, these sections just become columns like any
> other; don't give them their own layout.

---

## 6. Chrome-level ideas (higher effort — touches `app.rs` / `nav.rs`, the §UO work)

Do these only after the per-tab work, and confirm with the user first since they
change the global shell:

1. **Vertical nav rail instead of the wrapping top strip.** 26 tabs + 7 cluster
   labels wrap to 2–3 rows at 1400 px (`nav.rs:88` uses `horizontal_wrapped`).
   A left `SidePanel` listing clusters as collapsible groups reclaims that
   vertical space and scales better. Trade-off: eats left width that
   master-detail tabs also want — so make the rail collapsible (icon-only when
   narrow) or let the active tab's own roster replace it.
2. **Persistent inspector pane.** For Map especially, keep the map central and
   pin the selected-entity inspector to a right `SidePanel` so selecting on the
   map doesn't navigate away to the World/System tab.
3. **Status bar density.** `status.rs` (bottom panel, `app.rs:69`) is a thin
   strip — fine, but it's a place to surface counts/health so panels don't each
   re-render a summary row.
4. **Remember per-tab splitter widths.** Free with `SidePanel` ids, but verify
   ids are stable across frames (don't derive them from selection).

---

## 7. Invariants you must not break (from CLAUDE.md)

These are hard constraints. Re-read them; layout work is easy to do in a way
that violates the command-bus rule.

- **Model edits go through the command bus.** `state.run(BuilderCommand::…)`.
  Never write a *model* field from a panel — it breaks undo/redo (§R4). Moving a
  widget into a column does **not** change this: the widget still calls the same
  command.
- **View state is exempt and may stay direct.** Selection (`selected_world_id`,
  `selected_system_id`, `selected_factions`), scroll targets, and splitter
  widths are ephemeral view state, written directly today (e.g.
  `world.rs:116`). Master-detail conversion is therefore *pure view code* — you
  do **not** need new commands to render a roster and set the selection. Prefer
  existing setter methods (`set_active_tab`) where they exist.
- **Determinism / output writers are unaffected by layout** — but still run the
  golden tests if you touch anything under `gui-core` shared with exports. The
  `bitmap`/`svg`/`html` writers are not in these panels; ui_kit is not in them.
- **`ui_kit` takes `&mut Ui` + plain data only** — no `BuilderState`. Keep new
  helpers (§4) that way so the viewer can use them too.
- **Don't touch `old/`.**

---

## 8. Hard do / don't (rules tuned for how you'll actually get this wrong)

- **DO gate every multi-column block on width.** Use `columns_responsive`, never
  a bare `ui.columns(2, …)` that can't collapse — that just moves the
  wasted-space problem to "crushed on a laptop". (`system.rs`'s hard 2-col is
  the one place to *upgrade*, not to copy verbatim.)
- **DON'T build `Vec<Box<dyn FnOnce>>` of section closures.** Borrow-check death
  (§3.1). Run sections sequentially against the column slice.
- **DO give every `SidePanel` a unique, stable string `Id`.** Two panels sharing
  `"left"` silently fight over one stored width.
- **DON'T remove `auto_shrink([false; 2])`.** You want the scroll area to fill;
  the fix is multi-column/bounded content *inside* it, not an unfilled area.
- **DO width-cap prose** with `reading_column`. Edge-to-edge body text at 1400 px
  is unreadable regardless of columns.
- **DON'T convert all 26 tabs in one commit.** One cluster per commit, tagged
  `§COLUMNS` (or fold into the existing `§UO` overhaul tag if the user prefers).
  Update [GUIDE.md](GUIDE.md) as you go.
- **DON'T add `egui_extras` without asking** (§3.6) — it's a workspace dep.
- **DO keep the §UO chrome** (`palette::chrome_*`, `ui_kit::section`) — build on
  it, don't replace it.

---

## 9. Suggested sequencing (do it in this order)

1. **Helpers first.** Add `columns_responsive` + `reading_column` to `ui_kit.rs`
   (§4), extend `widgets_paint_headless`, `cargo test -p sectorforge-gui-core`.
   Small, reviewable, unblocks everything.
2. **Convert World (`world.rs`) as the template.** It's the worst offender (18
   stacked sections) and the highest-value win. MD roster + RC-2 inspector. This
   becomes the reference all other ENTITIES/inspector tabs copy. Get the user's
   eyes on it before fanning out.
3. **Fan out by cluster**, copying the World shape: ENTITIES → POWER → LORE →
   ANALYZE → OUTPUT → CHECK → BUILD. One commit per cluster. Use the
   `panel-implementer` subagent per panel (it knows the
   `BuilderState`/`BuilderCommand` rules) — but keep the *first* of each shape
   (first MD, first dashboard) in the main thread so you can review the pattern.
4. **Chrome (§6) last**, only with user sign-off.

For the per-panel mechanical conversions in step 3, dispatch
`panel-implementer`; for "where is section X used / who calls Y" use
`rust-explorer`; for test runs use `test-runner`. Keep the first conversion of
each new shape in the main thread.

---

## 10. Verify after every phase

```bash
cargo test -p sectorforge-gui-core          # ui_kit helpers (headless paint)
cargo test -p sectorforge-builder           # panels still build + paint
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Panels have headless smoke tests via `egui::__run_test_ui` (see
`ui_kit.rs:159`); a layout that panics on an empty/narrow `Ui` fails there. If
you add a panel test, assert it paints at both a wide and a narrow
`available_width` so the `columns_responsive` collapse path is exercised.

**Manual acceptance (use the `/run` skill or `cargo run -p sectorforge-builder`):**
open the builder at the default 1400×900 *and* drag it narrow (~700 px). For each
converted tab check the §2 targets: no lone stretched section, lists stay
visible, columns collapse cleanly when narrow, prose is line-length-capped.

---

## 11. Self-check before you call it done

- [ ] `grep -rln 'ui.columns(\|columns_responsive\|SidePanel' builder/src/builder/panels` now covers the inspector/list tabs, not just `system.rs`/`factions.rs`.
- [ ] No converted panel paints a single full-width section that holds only a narrow control.
- [ ] Every multi-column block collapses to 1 column below its `min_col_w`.
- [ ] Prose/briefing/markdown are width-capped.
- [ ] No new direct **model** writes (command bus intact); only view state set directly.
- [ ] Every `SidePanel` has a unique `Id`.
- [ ] Tests + clippy + fmt green (§10).
- [ ] [GUIDE.md](GUIDE.md) updated; commits tagged `§COLUMNS`/`§UO`.
