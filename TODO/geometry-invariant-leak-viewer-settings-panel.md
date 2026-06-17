# TODO: Square-geometry invariant breakable in viewer Settings panel

**Severity:** High (violates a non-negotiable invariant in a shipping write path)
**Found:** 2026-06-17, while code-verifying the disposition of the review/findings files.
**Status:** OPEN.

## Problem

The "sectors must be square" invariant (`sector_width == sector_height`, enforced pre-gen
by `GEN_SECTOR_NOT_SQUARE` in `src/validate/validation.rs`) is breakable through a live
viewer UI path.

`viewer/src/editor/settings_panel.rs:41-63` renders WIDTH and HEIGHT as **two independent
`DragValue` fields with no mirroring**:

```rust
ui.horizontal(|ui| {
    label(ui, "WIDTH");
    if ui.add(egui::DragValue::new(&mut sector.width).range(1..=64)).changed() {
        dirty = true;
    }
    label(ui, "HEIGHT");
    if ui.add(egui::DragValue::new(&mut sector.height).range(1..=64)).changed() {
        dirty = true;
    }
});

if sector.width != sector.height {
    ui.colored_label(
        crate::palette::warning(),
        "⚠ IRREGULAR DIMENSIONS: Joining sectors in a Segmentum usually requires square (H=W) sectors.",
    );
}
```

When the two diverge, the only guard is a **cosmetic warning label** (L58-63). The change
still sets `dirty = true`, which flows the non-square sector into `sector.json` on
auto-save. This is a post-load editor write path, so the pre-gen `GEN_SECTOR_NOT_SQUARE`
rule never runs against it.

## Why the prior audit missed it

`REVIEW_FINDINGS_2026-06-16.md` §2 theme 3 reports geometry as **"HELD fully"** on the
grounds that the viewer "Irregular-dims checkbox" was removed. The checkbox is indeed gone,
but these two unmirrored `DragValue`s are the **same divergence escape hatch under a
different control** — they were not caught. The audit claim is inaccurate; treat geometry
as **partially** held until this is fixed.

(This is the lone OPEN item from `REVIEW_FINDINGS.md` (2026-06-06), finding T17-6, which was
otherwise 19/20 implemented and has been archived to `old/`.)

## Fix

Mirror WIDTH ↔ HEIGHT unconditionally, exactly as the viewer's own New-Sector dialog
already does at `viewer/src/editor/dialogs.rs:114-128`:

```rust
ui.horizontal(|ui| {
    label(ui, "WIDTH");
    let w_res = ui.add(egui::DragValue::new(&mut sector.width).range(1..=64));
    label(ui, "HEIGHT");
    let h_res = ui.add(egui::DragValue::new(&mut sector.height).range(1..=64));

    // Geometry invariant: sectors must be square. Mirror width <-> height
    // unconditionally so the viewer cannot construct a non-square sector.
    if w_res.changed() {
        sector.height = sector.width;
        dirty = true;
    } else if h_res.changed() {
        sector.width = sector.height;
        dirty = true;
    }
});
```

After mirroring, the `if sector.width != sector.height` warning block (L58-63) becomes dead
and should be removed.

## Done means

- Editing WIDTH or HEIGHT in the Settings panel keeps `sector.width == sector.height`; the
  panel cannot produce a non-square `sector.json`.
- `rg "IRREGULAR DIMENSIONS" viewer/src/` returns no matches.
- A viewer test asserts a Settings-panel dimension edit leaves width == height (closes the
  same coverage class the New-Sector dialog mirror has).

## Verification commands

```bash
cargo test -p sectorforge-viewer
cargo test --test it -- golden
```
