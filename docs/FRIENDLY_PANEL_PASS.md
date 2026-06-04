# FRIENDLY_PANEL_PASS

A reusable recipe for making a builder panel friendly and self-explanatory
without changing its data model or behaviour. Canonical example: the FACTIONS
panel (`builder/src/builder/panels/factions.rs`), commit `fcfa0fd`.

To roll it out, prompt: **"Follow docs/FRIENDLY_PANEL_PASS.md for `<PANEL>`."**

---

Apply the "friendly panel pass" to the `<PANEL>` tab
(`builder/src/builder/panels/<panel_file>.rs`).

Goal: make the panel friendly and self-explanatory without changing its data
model or behaviour. Read the canonical example first
(`builder/src/builder/panels/factions.rs`, commit `fcfa0fd`) and match its idiom.

Do these transforms wherever they apply:

1. **Plain-language labels + hover help.** Replace raw schema/field names used as
   visible labels (e.g. `default_disposition`, `legend_visible`) with human
   labels. Carry the underlying field name + a one-line explanation in a tooltip.
   Use the local `labeled(ui, label, help, add)` helper pattern from
   factions.rs (or `ui_kit::field`); put "(schema: <field>)" inside the help text.

2. **Kill dev-facing strings.** No source paths, module names, or fn names in UI
   text; no `{:?}`/debug dumps shown to the user; no internal phase/ticket talk.
   Where a control lists enum variants, show the `display_name()` and move the raw
   key to a hover tooltip.

3. **Themed empty-states.** Replace every `ui.colored_label(Color32::GRAY, …)`
   (and any bare grey "nothing here" text) with `ui_kit::placeholder(ui, …)`. Copy
   should say what's empty and what to do next (e.g. "none yet — pick from the
   list below").

4. **Toolbar affordances.** Give action buttons an icon + tooltip and consistent
   verbs: ➕ Add, ⧉ Duplicate, 🗑 Delete, 💾 Save. Give filter/search boxes
   `hint_text`.

5. **Friendly section titles.** Rename `ui_kit::collapsing_section` / `section`
   titles to human terms. KEEP the `id_source` constant unchanged so collapse
   state persists — change only the visible title string.

6. **Pick-from-existing over free-typing.** Where a field is an id/reference to
   another entity, offer a dropdown seeded from the existing values in the project
   (+ "(none)" + an in-popup "custom…" row) instead of a raw text box. Mirror
   factions.rs `id_combo` / `existing_group_ids`.

7. **Confirm destructive actions.** Route delete/clear/reset-all through a
   `ModalKind::Confirm*` variant rendered in `builder/src/app.rs` (reuse the
   panel's existing delete fn, made `pub(crate)`) — do this only for panels whose
   edits bypass the undo command bus. If the panel already mutates via
   `state.run(BuilderCommand::…)` (undoable), a confirm is optional; ask before
   adding.

   Two mechanisms exist. For a one-off, use a dedicated variant like
   `ConfirmDeleteFaction` (stable-id keyed). For anything else, prefer the generic
   carrier `ModalKind::ConfirmDestructive { title, body, action }`: add a variant
   to the data-only `ConfirmAction` enum (`state/types.rs`) naming your panel's
   `pub(crate)` delete/clear fn, and `app::apply_confirm_action` dispatches to it on
   Yes. The panel only *opens* the modal; the irreversible edit runs once,
   centrally, off the render path. Guidance:
   - Key the action by a **stable id** where one exists. A captured **list index**
     is acceptable when the entity has no id — the modal is the user's next
     interaction, so the index stays valid.
   - When the delete site has no `&mut BuilderState` in scope (a helper that takes
     only `&mut cfg` / `&mut file`), record the request in an out-param and open the
     modal in the caller once that borrow ends (see `worlds_editor::edit_rows` and
     `segmentum::sg_children_section`).
   - Rolled out to: snapshot delete (PROJECT), "Clear all overrides" (SUBSECTORS),
     child + warp-link delete (SEGMENTUM), the worlds.toml row delete (PROJECT →
     World data), and the manual-entry / rule deletes in HOOKS, MISSIONS, PERSONAE,
     SITES, RELATIONS.

8. **Visual cues.** Small swatches/badges/previews go through a gui-core helper
   (`gui-core/src/palette.rs`), e.g. `palette::draw_faction_swatch`. Builder panels
   MUST NOT call `Ui::painter`/`Ui::painter_at` — `builder/clippy.toml` forbids it;
   add the helper to gui-core and call it.

## Hard constraints (do not violate)

- Mutations stay on the command bus where the panel already uses it
  (`state.run(...)`); do not introduce new direct `BuilderState` field writes that
  break undo/redo (§R4).
- Output/iteration order: use `BTreeMap`/`BTreeSet` or sort keys; never iterate
  `FxMap`/`FxSet` for anything user-visible.
- Presentational only: do not change the data model, serialization, generation, or
  any domain invariant. No golden-output writer is involved, so golden tests don't
  apply.
- Reuse house widgets in `gui-core/src/ui_kit.rs` (`section`, `collapsing_section`,
  `field`, `placeholder`, `combo`, `columns_responsive`, `kv`, `mono_*`). Don't
  reinvent them.

## When done

- `cargo check -p sectorforge-builder`
- `cargo clippy -p sectorforge-builder -p sectorforge-gui-core --all-targets -- -D warnings`
- `cargo test -p sectorforge-builder -p sectorforge-gui-core`
- Update the BUILDER.md / GUIDE.md walkthrough section for this panel if one exists.
- Show me the before/after diff before committing; don't commit unless I ask.
