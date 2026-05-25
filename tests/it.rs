// Consolidated integration-test entry point. All `tests/it/*.rs` files compile
// into this single test binary so the linker runs once instead of N times.

#[path = "it/analytics_and_presets.rs"]
mod analytics_and_presets;
#[path = "it/cli_gui_parity.rs"]
mod cli_gui_parity;
#[path = "it/golden_generation.rs"]
mod golden_generation;
#[path = "it/golden_png.rs"]
mod golden_png;
#[path = "it/imports_test.rs"]
mod imports_test;
#[path = "it/invariants_proptest.rs"]
mod invariants_proptest;
#[path = "it/invariants_tests.rs"]
mod invariants_tests;
#[path = "it/search_and_diff.rs"]
mod search_and_diff;
#[path = "it/segmentum_tests.rs"]
mod segmentum_tests;
#[path = "it/svg_export_tests.rs"]
mod svg_export_tests;
#[path = "it/validation_tests.rs"]
mod validation_tests;
