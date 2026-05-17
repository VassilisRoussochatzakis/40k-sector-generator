//! sectorforge CLI entry point.

use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

use sectorforge::validation::Severity;

#[derive(Debug, Parser)]
#[command(name = "sectorforge", version, about = "Generate a Warhammer 40k star sector from data files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate a project directory without generating output.
    Validate {
        #[arg(long)]
        project: Utf8PathBuf,
        /// Emit validation report as JSON to stdout.
        #[arg(long)]
        json: bool,
        /// Treat warnings as errors.
        #[arg(long)]
        strict: bool,
    },
    /// Generate a full sector from a project directory.
    Generate {
        #[arg(long)]
        project: Utf8PathBuf,
        /// Override seed from sectorforge.toml.
        #[arg(long)]
        seed: Option<String>,
        /// Override output directory.
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        /// Continue if validation produced warnings (but not errors).
        #[arg(long)]
        allow_warnings: bool,
    },
    /// Print workbook statistics for a standalone .xlsx file.
    InspectWorlds {
        #[arg(long)]
        workbook: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, sectorforge::SectorError> {
    match cli.command {
        Command::Validate {
            project,
            json,
            strict,
        } => {
            let input = sectorforge::load_project(&project)?;
            let report = sectorforge::validate_project(&input)?;
            if json {
                let text = serde_json::to_string_pretty(&report).unwrap();
                println!("{}", text);
            } else {
                print_validation_report(&report);
            }
            let has_blocking = !report.ok || (strict && !report.warnings.is_empty());
            Ok(if has_blocking { ExitCode::from(1) } else { ExitCode::SUCCESS })
        }
        Command::Generate {
            project,
            seed,
            out,
            allow_warnings,
        } => {
            let mut input = sectorforge::load_project(&project)?;
            if let Some(s) = seed {
                input.config.generation.seed = s;
            }
            let report = sectorforge::validate_project(&input)?;
            if !report.ok {
                print_validation_report(&report);
                return Ok(ExitCode::from(1));
            }
            if !allow_warnings && !report.warnings.is_empty() {
                eprintln!(
                    "validation produced {} warning(s); rerun with --allow-warnings or fix them",
                    report.warnings.len()
                );
                print_validation_report(&report);
                return Ok(ExitCode::from(1));
            }

            let output_dir = out
                .clone()
                .unwrap_or_else(|| project.join(&input.config.outputs.directory));
            let output_cfg = input.config.outputs.clone();
            let sector = sectorforge::generate_sector(input)?;
            sectorforge::export_sector(&sector, &output_cfg, &output_dir)?;

            println!(
                "Generated sector '{}' (seed: {}) — {} systems, {} worlds, {} routes",
                sector.id,
                sector.seed,
                sector.systems.len(),
                sector.manifest.world_count,
                sector.routes.len()
            );
            println!("Output written to: {}", output_dir);
            Ok(ExitCode::SUCCESS)
        }
        Command::InspectWorlds { workbook } => {
            let stats = sectorforge::inspect_world_workbook(&workbook)?;
            print_workbook_stats(&stats);
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn print_validation_report(report: &sectorforge::ValidationReport) {
    println!("Validation: {}", if report.ok { "OK" } else { "FAILED" });
    println!("  Workbook rows:        {}", report.world_workbook.row_count);
    println!(
        "  Usable candidates:    {}",
        report.world_workbook.usable_candidate_count
    );
    println!(
        "  Excluded rows:        {}",
        report.world_workbook.excluded_row_count
    );
    println!("  Errors:               {}", report.errors.len());
    println!("  Warnings:             {}", report.warnings.len());
    for issue in &report.errors {
        let path = issue.path.as_deref().unwrap_or("-");
        println!(
            "  [{}] {}: {} ({})",
            severity_tag(issue.severity),
            issue.code,
            issue.message,
            path
        );
    }
    for issue in &report.warnings {
        let path = issue.path.as_deref().unwrap_or("-");
        println!(
            "  [{}] {}: {} ({})",
            severity_tag(issue.severity),
            issue.code,
            issue.message,
            path
        );
    }
}

fn severity_tag(s: Severity) -> &'static str {
    match s {
        Severity::Error => "ERROR",
        Severity::Warning => "WARN",
        Severity::Info => "INFO",
    }
}

fn print_workbook_stats(stats: &sectorforge::world_pool::WorkbookStats) {
    println!("World workbook: {}", stats.workbook_path);
    println!("Key tables:");
    for (name, n) in &stats.key_table_counts {
        println!("  {:<18} {}", name, n);
    }
    println!("Generator rows:    {}", stats.generator_rows);
    println!("Usable candidates: {}", stats.usable_candidates);
    println!("Excluded rows:     {}", stats.excluded_rows);
    println!("\nTop star colours by total weight:");
    for (n, w) in &stats.top_star_colours {
        println!("  {:<14} {:.2}", n, w);
    }
    println!("\nTop world types by total weight:");
    for (n, w) in &stats.top_world_types {
        println!("  {:<22} {:.2}", n, w);
    }
    println!("\nTop notable features by row count:");
    for (n, c) in &stats.top_features {
        println!("  {:<28} {}", n, c);
    }
}
