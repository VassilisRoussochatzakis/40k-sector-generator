---
sweep_id: X01
scope: whole-workspace
reviewed_by: orchestrator
finding_counts: { critical: 0, high: 0, medium: 0, low: 0, nit: 0 }
overall: clean
---

# Cross-cutting Sweep: unsafe-audit

## Method

```
grep -rn "unsafe " --include="*.rs" -- src builder viewer gui-core
```

returns **zero matches**. Verified twice during Phase 0 recon. No `unsafe fn`, no `unsafe impl`, no `unsafe { ... }` block, no FFI surface (`extern "C"`), no `transmute`, no `MaybeUninit`, no `get_unchecked`/`unwrap_unchecked`, no `static mut`, no `Pin<&mut T>` manipulation, no manual `Send`/`Sync` impls.

## Findings

**No findings.** Workspace is 100% safe Rust.

## Recommendations (preventive, not findings)

1. Adopt `#![forbid(unsafe_code)]` at each crate root (`src/lib.rs`, `builder/src/lib.rs`, `viewer/src/lib.rs`, `gui-core/src/lib.rs`). This is a one-line, zero-runtime-cost guarantee that the property survives future edits. Categorized as a follow-up action (RUST_FIXES.md), not a finding.

2. If `cargo geiger` is ever installed, run it in CI to monitor the **dependency** unsafe surface (workspace deps include the eframe/winit/glutin/objc2 stack which is unavoidably unsafe-heavy at the OS-FFI seam). No action required today.

## Summary of suggested fixes

| id | severity | short | effort | risk |
|---|---|---|---|---|
| (preventive) | n/a | Add `#![forbid(unsafe_code)]` to all four crate roots | S | Low |
