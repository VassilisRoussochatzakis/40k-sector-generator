//! sectorforge CLI entry point.

use std::process::ExitCode;

use clap::Parser;

mod cli;

fn main() -> ExitCode {
    let args = cli::Cli::parse();
    match cli::run(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
