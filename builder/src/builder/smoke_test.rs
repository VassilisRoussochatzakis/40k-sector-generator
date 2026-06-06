//! Headless GUI smoke test (T6 / §40).
//!
//! Drives the core builder tabs through one off-screen egui frame each, with no
//! GPU and no display server, then asserts a session save succeeds. This is a
//! crash-only test: egui tessellates into shapes in software (texture uploads
//! are deferred to the absent renderer), so a clean `ctx.run` over each panel
//! proves the paint path doesn't panic on a freshly-constructed state. It is
//! deterministic and fast (<1s) so it stays CI-safe.

use crate::builder::panels::nav::show_active_panel;
use crate::builder::session::save_session;
use crate::builder::state::{BuilderState, BuilderTab};

/// Render one headless egui frame whose central panel paints the active tab.
fn paint_one_frame(state: &mut BuilderState) {
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            show_active_panel(ui, state);
        });
    });
}

#[test]
fn builder_smoke_paints_core_tabs_and_saves() {
    // Small square sector (§ square-geometry invariant: width == height).
    let mut state = BuilderState::new_blank("smoke", "Smoke Sector", "smoke-seed", 30, 30);

    // §40 / T6: every core tab must survive a headless paint without panicking.
    for tab in [
        BuilderTab::Project,
        BuilderTab::Map,
        BuilderTab::System,
        BuilderTab::World,
    ] {
        // Transient view state — selecting the active tab is not a document
        // mutation, so it is written directly (never lands in sector.json).
        state.active_tab = tab;
        paint_one_frame(&mut state);
    }

    // A save must succeed without error or panic. Use a unique temp path so
    // parallel test runs never collide; tear it down afterwards.
    let path = std::env::temp_dir().join(format!(
        "sectorforge-builder-smoke-{}.sgforge",
        std::process::id()
    ));
    let result = save_session(&path, &state);
    assert!(result.is_ok(), "save_session failed: {result:?}");

    // Sanity: the on-disk envelope is non-empty.
    let written = std::fs::read(&path).expect("read back saved session");
    assert!(!written.is_empty(), "saved session file is empty");

    let _ = std::fs::remove_file(&path);
}
