// Consolidated integration-test entry point. All `tests/it/*.rs` files compile
// into this single test binary so the linker runs once instead of N times.

#[path = "it/shared.rs"]
mod shared;

#[path = "it/analytics_and_presets.rs"]
mod analytics_and_presets;
#[path = "it/cli_behavior.rs"]
mod cli_behavior;
#[path = "it/cli_gui_parity.rs"]
mod cli_gui_parity;
#[path = "it/cli_smoke.rs"]
mod cli_smoke;
#[path = "it/economy_tests.rs"]
mod economy_tests;
#[path = "it/export_byte_goldens.rs"]
mod export_byte_goldens;
#[path = "it/export_parity_tests.rs"]
mod export_parity_tests;
#[path = "it/golden_generation.rs"]
mod golden_generation;
#[path = "it/golden_png.rs"]
mod golden_png;
#[path = "it/hooks_tests.rs"]
mod hooks_tests;
#[path = "it/imports_test.rs"]
mod imports_test;
#[path = "it/invariants_proptest.rs"]
mod invariants_proptest;
#[path = "it/invariants_tests.rs"]
mod invariants_tests;
#[path = "it/iterative_gen_tests.rs"]
mod iterative_gen_tests;
#[path = "it/loading_tests.rs"]
mod loading_tests;
#[path = "it/personae_tests.rs"]
mod personae_tests;
#[path = "it/random_sector_tests.rs"]
mod random_sector_tests;
#[path = "it/relations_tests.rs"]
mod relations_tests;
#[path = "it/route_monotonicity.rs"]
mod route_monotonicity;
#[path = "it/search_and_diff.rs"]
mod search_and_diff;
#[path = "it/segmentum_tests.rs"]
mod segmentum_tests;
#[path = "it/svg_export_tests.rs"]
mod svg_export_tests;
#[path = "it/validation_tests.rs"]
mod validation_tests;
