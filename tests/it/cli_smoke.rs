//! TF-T-4: per-subcommand smoke coverage. Confirms every CLI subcommand is
//! reachable from the binary and that `--help` prints + exits cleanly. Catches
//! the regression class where a clap variant exists but its `run_*` glue was
//! dropped or its argument schema panics on parse.
//!
//! These are not behavioural tests — they only prove the subcommand survives
//! the dispatch tier. Behaviour is verified by the dedicated tests
//! (`cli_gui_parity`, `golden_generation`, etc.).

use crate::shared::{run_cli, run_cli_ok};

const SUBCOMMANDS: &[&str] = &[
    "validate",
    "generate",
    "generate-system",
    "random",
    "validate-sector",
    "render-markdown",
    "inspect-worlds",
    "analyze",
    "new",
    "list-presets",
    "search",
    "history",
    "personae",
    "hooks",
    "prose",
    "relations",
    "regions",
    "economy",
    "compose",
    "interestingness",
    "briefing",
    "missions",
    "sites",
    "diff",
];

#[test]
fn every_subcommand_responds_to_help() {
    for sub in SUBCOMMANDS {
        let stdout = run_cli_ok(&[sub, "--help"]);
        // Help should mention either "Usage:" or the subcommand name itself.
        assert!(
            stdout.contains("Usage:") || stdout.contains(sub),
            "`sectorforge {sub} --help` produced unexpected output: {stdout}"
        );
    }
}

#[test]
fn unknown_subcommand_is_rejected() {
    let (code, _, _) = run_cli(&["definitely-not-a-real-subcommand"]);
    assert_ne!(code, Some(0), "unknown subcommand should exit non-zero");
}

#[test]
fn top_level_help_lists_every_subcommand() {
    let stdout = run_cli_ok(&["--help"]);
    for sub in SUBCOMMANDS {
        assert!(
            stdout.contains(sub),
            "top-level --help is missing subcommand `{sub}`"
        );
    }
}
