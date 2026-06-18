//! TF-T-4: per-subcommand smoke coverage. Confirms every CLI subcommand is
//! reachable from the binary and that `--help` prints + exits cleanly. Catches
//! the regression class where a clap variant exists but its `run_*` glue was
//! dropped or its argument schema panics on parse.
//!
//! These are not behavioural tests — they only prove the subcommand survives
//! the dispatch tier. Behaviour is verified by the dedicated tests
//! (`cli_gui_parity`, `golden_generation`, etc.).

use std::process::Command;

use crate::shared::bin;

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
        let out = Command::new(bin())
            .args([sub, "--help"])
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn `sectorforge {sub} --help`: {e}"));
        assert!(
            out.status.success(),
            "`sectorforge {sub} --help` exited non-zero ({:?}); stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr),
        );
        // Help should mention either "Usage:" or the subcommand name itself.
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("Usage:") || stdout.contains(sub),
            "`sectorforge {sub} --help` produced unexpected output: {stdout}"
        );
    }
}

#[test]
fn unknown_subcommand_is_rejected() {
    let out = Command::new(bin())
        .args(["definitely-not-a-real-subcommand"])
        .output()
        .expect("spawn bin");
    assert!(
        !out.status.success(),
        "unknown subcommand should exit non-zero"
    );
}

#[test]
fn top_level_help_lists_every_subcommand() {
    let out = Command::new(bin())
        .arg("--help")
        .output()
        .expect("spawn bin");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for sub in SUBCOMMANDS {
        assert!(
            stdout.contains(sub),
            "top-level --help is missing subcommand `{sub}`"
        );
    }
}
