# BUILDER_FIX

Fix list from button audit of `sectorforge-builder`. Each item is self-contained: file, line, what's wrong, what to do. Work top-down, commit per section.

---

## LLM instructions (read first)

Persistent context for the agent picking this up.

- Repo root: `/Users/vassilis/Documents/40k-sector-generator`. Builder crate: `builder/`. Spec lives in `docs/BUILDER_REQS.txt`. Never touch `old/`.
- Tab router lives in [builder/src/builder/panels/nav.rs](builder/src/builder/panels/nav.rs). Stub panels delegate to [builder/src/builder/panels/placeholder.rs](builder/src/builder/panels/placeholder.rs).
- Every mutation that should be undoable goes through `BuilderCommand` via `state.run(cmd)`. Direct mutation of `state.sector.*` is acceptable only when already used by neighbouring code in the same panel (precedent matters).
- Validation/invariant refresh: call `state.mark_validation_dirty()` (and `state.dirty = true`) after any sector mutation that isn't already routed through a command (commands do it for you).
- After edits, run `cargo check` first (fast), then `cargo fmt`, then `cargo test -p sectorforge-builder` for the touched panel. Full `cargo test` only before commit.
- Do not refactor adjacent code while fixing. Do not add docs/comments unless the WHY is non-obvious.
- Do not spawn subagents. Each fix is one or two files — handle inline.
- Update `GUIDE.md` only if user-visible behaviour changes.
- Match existing panel idioms (frame groups, `egui::Id::new` salts, `persistent_singleline` helpers). Look at neighbouring panels before inventing a new shape.

---

## Fix 1 — duplicate regen buttons in SYSTEM tab (real bug)

**File**: [builder/src/builder/panels/system.rs:878-885](builder/src/builder/panels/system.rs#L878-L885)

Current:

```rust
ui.horizontal(|ui| {
    if ui.button("Regenerate this system").clicked() {
        run_regen(state, HexCoord { q, r }, index, &seed);
    }
    if ui.button("Regenerate at coord (replace)").clicked() {
        run_regen(state, HexCoord { q, r }, index, &seed);
    }
});
```

Both buttons call `run_regen` with the DragValue-mutated `q`/`r`. Second button is dead — same effect as first.

**Intent** (from button labels):
- "Regenerate this system" → regen in place at the system's existing coord. Ignores edited q/r.
- "Regenerate at coord (replace)" → regen at the edited q/r (move + replace).

**Fix**:

```rust
ui.horizontal(|ui| {
    if ui.button("Regenerate this system").clicked() {
        run_regen(state, sys.coord, index, &seed);
    }
    if (q, r) != (sys.coord.q, sys.coord.r)
        && ui.button("Regenerate at coord (replace)").clicked()
    {
        run_regen(state, HexCoord { q, r }, index, &seed);
    }
});
```

Two things to verify before committing:
1. `sys` may have been moved out of scope above line 878 — re-borrow via `let sys = &state.sector.systems[sys_idx];` inside the `horizontal` if needed. Check the borrow checker before adding clones.
2. `run_regen` calls `state.generate_system_here(coord, index, ...)`. Confirm in [builder/src/builder/state/generation_ops.rs](builder/src/builder/state/generation_ops.rs) that calling it with the existing coord is a no-op-friendly path (replace at same hex) and that calling it with a new coord that already holds another system triggers the §S6 collision dialog rather than silently clobbering. If `generate_system_here` doesn't check, gate the second button with the same occupant-check `apply_coord_move` uses (see system.rs:257-269).

Test: add a `#[cfg(test)]` case in system.rs that builds a 2-system sector, calls each button's handler equivalent, and asserts the in-place button preserves coord while the replace button moves the system.

---

## Fix 2 — PROJECT tab layout: open_project section bleeds into the toolbar row

**File**: [builder/src/builder/panels/project.rs:14-26](builder/src/builder/panels/project.rs#L14-L26)

Current:

```rust
ui.horizontal_wrapped(|ui| {
    if ui.button("New project…").clicked() {
        state.modal = Some(ModalKind::NewProject { ... });
    }
    let _ = open_project::show(ui, state);  // renders its own heading + label + 2 buttons inline
    save_project::show(ui, state);
});
```

`open_project::show` opens with `ui.heading("Open project")` + a `ui.label("Pick a directory…")` (see [open_project.rs:14-16](builder/src/builder/panels/open_project.rs#L14-L16)). Inside a horizontal toolbar that looks broken.

`open_project::show` is also used as a modal from [app.rs:142-144](builder/src/app.rs#L142-L144), where the heading is correct. Don't break that path.

**Fix options**:
- A (preferred — smaller blast radius): in `project.rs`, stop calling `open_project::show` inline. Replace with a plain `if ui.button("Open project…").clicked() { state.modal = Some(ModalKind::OpenProject { ... }); }` so the picker only appears as the modal that already exists. Confirm `ModalKind::OpenProject` exists in [state/types.rs](builder/src/builder/state/types.rs); the modal arm at [app.rs:141-144](builder/src/app.rs#L141-L144) already handles it. This matches how "New project…" works.
- B: split `open_project::show` into `show_inline_button` (button only) and `show_modal` (heading + body). Update both callers. More churn.

Go with A unless `ModalKind::OpenProject` carries state that the inline path needs to bypass — verify by reading the modal variant before editing.

---

## Fix 3 — SUBSECTORS "Recluster" button mislabel

**File**: [builder/src/builder/panels/subsectors.rs:141-148](builder/src/builder/panels/subsectors.rs#L141-L148)

The button writes `state.subsector_target_systems` and nulls `state.map_view_cache`. Actual k-means runs lazily on next MAP-tab tick via `refresh_map_cache`. Label "Recluster" implies immediate effect.

**Fix options**:
- A (minimal): rename to "Apply target & refresh". Update hover text. No behaviour change.
- B: actually run the cluster pass synchronously here so the SUBSECTORS list updates this frame. Requires calling `build_subsectors(&state.sector, SubsectorConfig { target_systems_per_subsector: …, ..default })` and pushing the result into wherever the panel reads from. Check whether the current frame already re-derives — if so, this is a no-op visually and A is the right call.

Default to A. Confirm by reading [subsectors/mod.rs](src/subsectors/mod.rs) and the panel's iteration pattern before doing B.

---

## Fix 4 — stub panels (no buttons, but feature-incomplete)

12 nav tabs still render only the placeholder. None of these is a button bug, but anyone testing the UI hits them. List with phase + reqs section:

| Tab | Phase | Reqs |
|---|---|---|
| analytics | E | §A1..§A4 |
| briefing | D | §BR1..§BR5 |
| diff | E | §DF1..§DF5 |
| export | E | §EX1..§EX8 |
| hooks | D | §HK1..§HK6 |
| interestingness | D | §INT1..§INT4 |
| missions | D | §M1..§M5 |
| personae | D | §PER1..§PER5 |
| prose | D | §PR1..§PR4 |
| search | E | §SR1..§SR5 |
| segmentum | E | §SG1..§SG5 |
| sites | D | §ST1..§ST4 |

Not in scope for this fix doc unless user asks. Tracked under §41 Outstanding panels in `docs/BUILDER_REQS.txt`.

---

## Clean panels (no action)

Audited and clean — leave alone:

- [app.rs](builder/src/app.rs), [nav.rs](builder/src/builder/panels/nav.rs)
- [new_project.rs](builder/src/builder/panels/new_project.rs), [save_project.rs](builder/src/builder/panels/save_project.rs), [preferences.rs](builder/src/builder/panels/preferences.rs), [conflict_resolver.rs](builder/src/builder/panels/conflict_resolver.rs)
- [map.rs](builder/src/builder/panels/map.rs) (toolbox + 3 dialogs)
- [system.rs](builder/src/builder/panels/system.rs) except Fix 1
- [world.rs](builder/src/builder/panels/world.rs), [orbital.rs](builder/src/builder/panels/orbital.rs)
- [factions.rs](builder/src/builder/panels/factions.rs), [control.rs](builder/src/builder/panels/control.rs), [relations.rs](builder/src/builder/panels/relations.rs)
- [regions.rs](builder/src/builder/panels/regions.rs), [surface_regions.rs](builder/src/builder/panels/surface_regions.rs), [subsectors.rs](builder/src/builder/panels/subsectors.rs) except Fix 3
- [routes.rs](builder/src/builder/panels/routes.rs)
- [economy.rs](builder/src/builder/panels/economy.rs), [history.rs](builder/src/builder/panels/history.rs), [conflict.rs](builder/src/builder/panels/conflict.rs)
- [intel.rs](builder/src/builder/panels/intel.rs), [validation.rs](builder/src/builder/panels/validation.rs), [invariants.rs](builder/src/builder/panels/invariants.rs), [generation.rs](builder/src/builder/panels/generation.rs)

---

## Suggested order

1. Fix 1 (real bug, smallest scope, no UI risk). Commit.
2. Fix 2 (UX, low risk, isolated to one file). Commit.
3. Fix 3 (rename only — pick option A). Commit.

Each commit message: `fix(builder): <one-line>`. Body only when the why isn't obvious from the diff.
