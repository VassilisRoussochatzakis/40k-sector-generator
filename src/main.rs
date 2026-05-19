//! sectorforge CLI entry point.

use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

use sectorforge::sector_model::HexCoord;
use sectorforge::validation::Severity;

#[derive(Debug, Parser)]
#[command(
    name = "sectorforge",
    version,
    about = "Generate a Warhammer 40k star sector from data files"
)]
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
        /// PNG heatmap mode (§10). One of: off, control, military, trade,
        /// industrial, covert, faith, threat, intel. Overrides the project's
        /// `outputs.bitmap.heatmap` setting.
        #[arg(long)]
        heatmap: Option<String>,
        /// Disable per-system faction tint in the PNG export (§8).
        #[arg(long)]
        no_faction_fill: bool,
    },
    /// Generate a single standalone system from a project directory.
    GenerateSystem {
        #[arg(long)]
        project: Utf8PathBuf,
        /// Override seed from sectorforge.toml.
        #[arg(long)]
        seed: Option<String>,
        /// 1-based system index.
        #[arg(long, default_value_t = 1)]
        index: usize,
        /// Axial hex coord q (defaults to 0).
        #[arg(long, default_value_t = 0)]
        coord_q: i32,
        /// Axial hex coord r (defaults to 0).
        #[arg(long, default_value_t = 0)]
        coord_r: i32,
        /// Output path for the system JSON (defaults to stdout).
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        /// Also write a Markdown snippet alongside the JSON.
        #[arg(long)]
        markdown: bool,
    },
    /// Load a previously generated sector JSON and check post-generation
    /// invariants (spec §11.11).
    ValidateSector {
        #[arg(long)]
        sector: Utf8PathBuf,
        /// Emit report as JSON to stdout.
        #[arg(long)]
        json: bool,
    },
    /// Render a Markdown overview from a previously generated sector JSON.
    RenderMarkdown {
        #[arg(long)]
        sector: Utf8PathBuf,
        /// Output path for the Markdown (defaults to stdout).
        #[arg(long)]
        out: Option<Utf8PathBuf>,
    },
    /// Print statistics for a standalone world-data directory
    /// (containing key.csv + generator.csv).
    InspectWorlds {
        #[arg(long)]
        data_dir: String,
    },
    /// §8 NEW.md: read-only analytics dashboard for a generated sector.
    /// Accepts either `--project <dir>` (regenerates) or `--sector <path>`
    /// (loads an existing sector.json). Writes `analysis.md` + `analysis.json`
    /// to `--out`, or prints Markdown to stdout when `--out` is omitted.
    Analyze {
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        sector: Option<Utf8PathBuf>,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        /// Emit only the JSON to stdout (overrides Markdown).
        #[arg(long)]
        json: bool,
        /// Exit with status 1 if any health flag fires (useful in CI).
        #[arg(long)]
        strict: bool,
    },
    /// §9 NEW.md: scaffold a new project from a bundled preset.
    /// Copies `presets/<preset>/` into `<out>` and writes a header comment to
    /// `sectorforge.toml`. The destination must not already exist.
    New {
        /// Destination project directory to create.
        #[arg(long)]
        out: Utf8PathBuf,
        /// Preset name (matches a sub-directory of `presets/`).
        #[arg(long)]
        preset: String,
        /// Override the preset's bundled seed.
        #[arg(long)]
        seed: Option<String>,
        /// Source directory holding presets. Defaults to `./presets`.
        #[arg(long, default_value = "presets")]
        presets_dir: Utf8PathBuf,
    },
    /// §9 NEW.md: list available presets in `--presets-dir` (default `presets`).
    ListPresets {
        #[arg(long, default_value = "presets")]
        presets_dir: Utf8PathBuf,
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
                println!("{text}");
            } else {
                print_validation_report(&report);
            }
            let has_blocking = !report.ok || (strict && !report.warnings.is_empty());
            Ok(if has_blocking {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Generate {
            project,
            seed,
            out,
            allow_warnings,
            heatmap,
            no_faction_fill,
        } => {
            let mut input = sectorforge::load_project(&project)?;
            if let Some(s) = seed {
                input.config.generation.seed = s;
            }
            if let Some(h) = heatmap.as_deref() {
                input.config.outputs.bitmap.heatmap = parse_heatmap(h)?;
            }
            if no_faction_fill {
                input.config.outputs.bitmap.faction_fill = false;
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

            let output_dir = out.unwrap_or_else(|| project.join(&input.config.outputs.directory));
            let output_cfg = input.config.outputs.clone();
            let sector = sectorforge::generate_sector(input)?;

            // Spec §11.11: check invariants before writing.
            let inv = sectorforge::validate_sector(&sector);
            if !inv.ok {
                eprintln!(
                    "post-generation invariants failed ({} violation(s)):",
                    inv.violations.len()
                );
                for v in &inv.violations {
                    eprintln!("  {} {}", v.code, v.message);
                }
                return Ok(ExitCode::from(1));
            }

            sectorforge::export_sector(&sector, &output_cfg, &output_dir)?;

            println!(
                "Generated sector '{}' (seed: {}) — {} systems, {} worlds, {} routes",
                sector.id,
                sector.seed,
                sector.systems.len(),
                sector.manifest.world_count,
                sector.routes.len()
            );
            println!("Output written to: {output_dir}");
            Ok(ExitCode::SUCCESS)
        }
        Command::GenerateSystem {
            project,
            seed,
            index,
            coord_q,
            coord_r,
            out,
            markdown,
        } => {
            let mut input = sectorforge::load_project(&project)?;
            if let Some(s) = seed {
                input.config.generation.seed = s;
            }
            let coord = HexCoord {
                q: coord_q,
                r: coord_r,
            };
            let system = sectorforge::generate_system_standalone(input, index, coord)?;
            let json = serde_json::to_string_pretty(&system).map_err(|e| {
                sectorforge::SectorError::ExportFailed {
                    path: "<stdout>".to_string(),
                    message: e.to_string(),
                }
            })?;
            match &out {
                Some(p) => sectorforge::write_system_json(p, &system)?,
                None => println!("{json}"),
            }
            if markdown {
                let md = sectorforge::render_system_markdown(&system);
                match &out {
                    Some(p) => {
                        let md_path = p.with_extension("md");
                        std::fs::write(&md_path, md)
                            .map_err(|e| sectorforge::SectorError::io(md_path.as_str(), e))?;
                    }
                    None => println!("\n{md}"),
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::ValidateSector { sector, json } => {
            let sec = sectorforge::load_sector_json(&sector)?;
            let report = sectorforge::validate_sector(&sec);
            if json {
                let text = serde_json::to_string_pretty(&report).unwrap();
                println!("{text}");
            } else {
                print_invariant_report(&report);
            }
            Ok(if report.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        Command::RenderMarkdown { sector, out } => {
            let sec = sectorforge::load_sector_json(&sector)?;
            let md = sectorforge::render_sector_markdown(&sec);
            match out {
                Some(p) => sectorforge::write_sector_markdown(&p, &sec)?,
                None => print!("{md}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::InspectWorlds { data_dir } => {
            let stats = sectorforge::inspect_world_workbook(&data_dir)?;
            print_workbook_stats(&stats);
            Ok(ExitCode::SUCCESS)
        }
        Command::Analyze {
            project,
            sector,
            out,
            json,
            strict,
        } => run_analyze(project, sector, out, json, strict),
        Command::New {
            out,
            preset,
            seed,
            presets_dir,
        } => {
            sectorforge::presets::scaffold(&presets_dir, &preset, &out, seed.as_deref()).map(|_| {
                println!("Scaffolded project '{out}' from preset '{preset}'.");
                println!("Next:  cargo run --bin sectorforge -- generate --project {out}");
                ExitCode::SUCCESS
            })
        }
        Command::ListPresets { presets_dir } => {
            let entries = sectorforge::presets::list(&presets_dir)?;
            if entries.is_empty() {
                println!("No presets found in {presets_dir}");
            } else {
                println!("Available presets ({}):", entries.len());
                for p in entries {
                    println!("  {:<24} {}", p.id, p.description);
                }
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn run_analyze(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
    out: Option<Utf8PathBuf>,
    json: bool,
    strict: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, cfg) = match (project, sector) {
        (Some(project), None) => {
            let input = sectorforge::load_project(&project)?;
            let cfg = input.config.analyze.clone();
            let sec = sectorforge::generate_sector(input)?;
            (sec, cfg)
        }
        (None, Some(sector)) => {
            let sec = sectorforge::load_sector_json(&sector)?;
            (sec, sectorforge::analytics::AnalyzeConfig::default())
        }
        (Some(_), Some(_)) | (None, None) => {
            return Err(sectorforge::SectorError::InvalidConfig(
                "pass exactly one of --project <dir> or --sector <path>".into(),
            ));
        }
    };
    let analysis = sectorforge::analyze_sector_with(&sec, &cfg);
    if let Some(dir) = &out {
        sectorforge::write_analysis(dir, &analysis)?;
        println!("Wrote {dir}/analysis.md and {dir}/analysis.json");
    } else if json {
        let text = serde_json::to_string_pretty(&analysis).unwrap();
        println!("{text}");
    } else {
        let md = sectorforge::render_analysis_markdown(&analysis);
        print!("{md}");
    }
    let has_flags = !analysis.health_flags.is_empty();
    if strict && has_flags {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

fn print_validation_report(report: &sectorforge::ValidationReport) {
    println!("Validation: {}", if report.ok { "OK" } else { "FAILED" });
    println!(
        "  Workbook rows:        {}",
        report.world_workbook.row_count
    );
    println!(
        "  Usable candidates:    {}",
        report.world_workbook.usable_candidate_count
    );
    println!(
        "  Excluded rows:        {}",
        report.world_workbook.excluded_row_count
    );
    if !report.world_workbook.exclusion_reasons.is_empty() {
        for (reason, count) in &report.world_workbook.exclusion_reasons {
            println!("    - {reason}: {count}");
        }
    }
    println!("  Errors:               {}", report.errors.len());
    println!("  Warnings:             {}", report.warnings.len());
    for issue in &report.errors {
        print_issue(issue);
    }
    for issue in &report.warnings {
        print_issue(issue);
    }
}

fn print_issue(issue: &sectorforge::ValidationIssue) {
    match &issue.path {
        Some(path) => println!(
            "  [{}] {}: {} ({})",
            severity_tag(issue.severity),
            issue.code,
            issue.message,
            path
        ),
        None => println!(
            "  [{}] {}: {}",
            severity_tag(issue.severity),
            issue.code,
            issue.message
        ),
    }
}

fn print_invariant_report(report: &sectorforge::InvariantReport) {
    println!(
        "Sector invariants: {}",
        if report.ok { "OK" } else { "FAILED" }
    );
    println!("  Violations: {}", report.violations.len());
    for v in &report.violations {
        match &v.path {
            Some(path) => println!("  [{}] {} ({})", v.code, v.message, path),
            None => println!("  [{}] {}", v.code, v.message),
        }
    }
}

fn parse_heatmap(s: &str) -> Result<sectorforge::heatmap::HeatmapMode, sectorforge::SectorError> {
    use sectorforge::heatmap::HeatmapMode;
    match s.to_ascii_lowercase().as_str() {
        "off" => Ok(HeatmapMode::Off),
        "control" => Ok(HeatmapMode::Control),
        "military" => Ok(HeatmapMode::Military),
        "trade" => Ok(HeatmapMode::Trade),
        "industrial" | "industry" => Ok(HeatmapMode::Industrial),
        "covert" => Ok(HeatmapMode::Covert),
        "faith" => Ok(HeatmapMode::Faith),
        "threat" => Ok(HeatmapMode::Threat),
        "intel" => Ok(HeatmapMode::Intel),
        other => Err(sectorforge::SectorError::InvalidConfig(format!(
            "unknown heatmap mode '{other}' (expected off|control|military|trade|industrial|covert|faith|threat|intel)"
        ))),
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
    println!("World data dir: {}", stats.data_dir);
    println!("Key tables:");
    for (name, n) in &stats.key_table_counts {
        println!("  {name:<18} {n}");
    }
    println!("Generator rows:    {}", stats.generator_rows);
    println!("Usable candidates: {}", stats.usable_candidates);
    println!("Excluded rows:     {}", stats.excluded_rows);
    println!("\nTop star colours by total weight:");
    for (n, w) in &stats.top_star_colours {
        println!("  {n:<14} {w:.2}");
    }
    println!("\nTop world types by total weight:");
    for (n, w) in &stats.top_world_types {
        println!("  {n:<22} {w:.2}");
    }
    println!("\nTop notable features by row count:");
    for (n, c) in &stats.top_features {
        println!("  {n:<28} {c}");
    }
}
