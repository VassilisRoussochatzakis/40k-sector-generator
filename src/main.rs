//! sectorforge CLI entry point.

use std::io::Write;
use std::process::ExitCode;

use clap::Parser;

mod cli;

fn main() -> ExitCode {
    init_logging();
    let args = cli::Cli::parse();
    match cli::run(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            cli::exit_code::from_error(&e)
        }
    }
}

/// Finding #29 (observability): initialise the CLI logging facade once, before
/// any subcommand runs. Progress diagnostics (the `log_progress` family in
/// `cli::common`) flow through `log::info!`, so the defaults here must preserve
/// the pre-logging UX exactly:
///
/// * **Default level `info`** when `RUST_LOG` is unset — progress was always
///   visible by default, so it must stay visible. Setting `RUST_LOG` (e.g.
///   `RUST_LOG=warn` or `RUST_LOG=off`) now overrides this to control verbosity.
/// * **stderr sink** (env_logger's default) — progress never polluted stdout,
///   which carries machine-readable result/data emission (`println!`) only.
/// * **Bare format** — just the record's message, no timestamp and no module
///   target prefix, so an `info!` line is byte-for-byte the old `eprintln!`
///   text (e.g. `[sectorforge] sector: …`). This keeps CLI tests that assert on
///   stderr substrings valid.
fn init_logging() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| writeln!(buf, "{}", record.args()))
        .init();
}
