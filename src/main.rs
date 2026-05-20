//! sectorforge CLI entry point.

use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use serde::Serialize;

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
        /// industrial, covert, faith, threat, intel, tension, trade_volume,
        /// food, tithe, supply. Overrides the project's `outputs.bitmap.heatmap`.
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
    /// §2 NEW.md: deterministic constraint-directed seed search.
    /// Loads `<project>`, reads a `wishes.toml` listing constraints, and
    /// enumerates seeds derived from the base seed until one satisfies all
    /// constraints (or the budget is exhausted). Writes `search.md` +
    /// `search.json` to `--out` when given, otherwise prints the Markdown.
    Search {
        #[arg(long)]
        project: Utf8PathBuf,
        #[arg(long)]
        wishes: Utf8PathBuf,
        /// Override the base seed from wishes/project (n=0 candidate).
        #[arg(long)]
        base_seed: Option<String>,
        /// Override the search budget.
        #[arg(long)]
        budget: Option<u32>,
        /// Write search.md + search.json into this directory.
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        /// Print JSON to stdout instead of Markdown.
        #[arg(long)]
        json: bool,
        /// Exit 1 if no candidate satisfied the constraints.
        #[arg(long)]
        strict: bool,
    },
    /// §1 NEW.md: derive a deterministic chronicle of in-universe events from
    /// a generated sector. Accepts either `--project <dir>` (regenerates) or
    /// `--sector <path>` (loads an existing sector.json). Writes
    /// `history.md` + `history.json` to `--out`, or prints Markdown to stdout.
    History {
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        sector: Option<Utf8PathBuf>,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// §3 NEW.md: derive a deterministic dramatis personae overlay (named
    /// characters per faction presence). Accepts `--project` or `--sector`.
    Personae {
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        sector: Option<Utf8PathBuf>,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// §7 NEW.md: derive adventure / plot hooks. Accepts `--project` or
    /// `--sector`. `--player` hides GM-only hooks (hidden-presence derived).
    Hooks {
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        sector: Option<Utf8PathBuf>,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        #[arg(long)]
        json: bool,
        /// Hide GM-only hooks (e.g. those derived from hidden-tier presences).
        #[arg(long)]
        player: bool,
    },
    /// §6 NEW.md: derive a narrative gazetteer (deterministic template
    /// prose). Accepts `--project` or `--sector`. `--dispatch` switches the
    /// tone preset.
    Prose {
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        sector: Option<Utf8PathBuf>,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        #[arg(long)]
        json: bool,
        /// Use the terse Administratum-dispatch tone instead of the
        /// florid gazetteer voice.
        #[arg(long)]
        dispatch: bool,
    },
    /// §5 NEW2.md: derive the inter-faction diplomacy matrix.
    /// Accepts `--project` or `--sector`.
    Relations {
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        sector: Option<Utf8PathBuf>,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// §5 NEW.md: emit the regional warp-phenomena overlay for a project's
    /// grid. Requires a project so the regions config can be read.
    Regions {
        #[arg(long)]
        project: Utf8PathBuf,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// §12 NEW.md / §4 NEW2.md: derive trade, tithe, and strategic-resource snapshot.
    /// Accepts `--project` or `--sector`.
    #[command(alias = "analyze-economy", alias = "analyze-tithes")]
    Economy {
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        sector: Option<Utf8PathBuf>,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// §14 NEW.md: compose a segmentum (multi-sector) from `segmentum.toml`.
    /// Loads + generates every listed child sector (reusing the existing
    /// deterministic pipeline), then runs a deterministic stitch stage to
    /// emit inter-sector warp links and a super-manifest.
    Compose {
        /// Path to `segmentum.toml`.
        #[arg(long)]
        segmentum: Utf8PathBuf,
        /// Output directory for `segmentum.md`, `segmentum.json`,
        /// `super_manifest.json`, and per-child sector subdirectories.
        #[arg(long)]
        out: Utf8PathBuf,
        /// Override the stitch seed from the segmentum file.
        #[arg(long)]
        stitch_seed: Option<String>,
        /// Emit JSON to stdout instead of writing files.
        #[arg(long)]
        json: bool,
    },
    /// §18 NEW2.md: interestingness scorecard for a sector against a target
    /// profile (political sandbox / grim collapse / mercantile / villainous /
    /// frontier). Accepts `--project` or `--sector`.
    Interestingness {
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        sector: Option<Utf8PathBuf>,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        #[arg(long)]
        json: bool,
        /// Target profile id. One of:
        /// `political_sandbox` | `grim_collapse` | `mercantile` |
        /// `villainous` | `frontier`. Default: `political_sandbox`.
        #[arg(long)]
        profile: Option<String>,
    },
    /// §9 NEW2.md: audience-targeted redaction pack. Applies a built-in
    /// briefing profile (gm / navy / inquisition / trader / governor /
    /// public) and writes a redacted sector + summary into `--out`.
    Briefing {
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        sector: Option<Utf8PathBuf>,
        #[arg(long)]
        out: Utf8PathBuf,
        /// Built-in preset to apply.
        #[arg(long)]
        preset: String,
        /// Observer faction id (defaults to none for GM / public).
        #[arg(long)]
        observer: Option<String>,
        /// Override the preset's intel confidence cutoff (0..=100).
        #[arg(long)]
        min_confidence: Option<u8>,
    },
    /// §3 NEW2.md: deterministic mission / quest seeds derived from sector
    /// state. Accepts `--project` or `--sector`. `--player` hides GM-only
    /// missions.
    Missions {
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        sector: Option<Utf8PathBuf>,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        player: bool,
    },
    /// §7 NEW2.md: planetary points-of-interest per world. Accepts
    /// `--project` or `--sector`. `--player` hides sites whose public status
    /// differs from actual status.
    Sites {
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        sector: Option<Utf8PathBuf>,
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        player: bool,
    },
    /// §10 NEW.md: deterministic sector diff. Two modes:
    ///
    /// * `--before <a.json> --after <b.json>`: compare two saved sectors.
    /// * `--project <dir> --ticks <N>`: generate the sector, advance N
    ///   ticks, diff before vs. after.
    Diff {
        #[arg(long)]
        before: Option<Utf8PathBuf>,
        #[arg(long)]
        after: Option<Utf8PathBuf>,
        #[arg(long)]
        project: Option<Utf8PathBuf>,
        #[arg(long)]
        ticks: Option<u32>,
        /// Write diff.md + diff.json into this directory.
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        /// Print JSON to stdout instead of Markdown.
        #[arg(long)]
        json: bool,
        /// Skip world-level detail in the report.
        #[arg(long)]
        skip_worlds: bool,
        /// Skip per-route detail.
        #[arg(long)]
        skip_routes: bool,
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
                print_json(&report)?;
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
            let json = to_json_pretty(&system)?;
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
                print_json(&report)?;
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
        Command::Search {
            project,
            wishes,
            base_seed,
            budget,
            out,
            json,
            strict,
        } => run_search(project, wishes, base_seed, budget, out, json, strict),
        Command::History {
            project,
            sector,
            out,
            json,
        } => run_history(project, sector, out, json),
        Command::Personae {
            project,
            sector,
            out,
            json,
        } => run_personae(project, sector, out, json),
        Command::Hooks {
            project,
            sector,
            out,
            json,
            player,
        } => run_hooks(project, sector, out, json, player),
        Command::Prose {
            project,
            sector,
            out,
            json,
            dispatch,
        } => run_prose(project, sector, out, json, dispatch),
        Command::Relations {
            project,
            sector,
            out,
            json,
        } => run_relations(project, sector, out, json),
        Command::Regions { project, out, json } => run_regions(project, out, json),
        Command::Economy {
            project,
            sector,
            out,
            json,
        } => run_economy(project, sector, out, json),
        Command::Compose {
            segmentum,
            out,
            stitch_seed,
            json,
        } => run_compose(segmentum, out, stitch_seed, json),
        Command::Interestingness {
            project,
            sector,
            out,
            json,
            profile,
        } => run_interestingness(project, sector, out, json, profile),
        Command::Briefing {
            project,
            sector,
            out,
            preset,
            observer,
            min_confidence,
        } => run_briefing(project, sector, out, preset, observer, min_confidence),
        Command::Missions {
            project,
            sector,
            out,
            json,
            player,
        } => run_missions(project, sector, out, json, player),
        Command::Sites {
            project,
            sector,
            out,
            json,
            player,
        } => run_sites(project, sector, out, json, player),
        Command::Diff {
            before,
            after,
            project,
            ticks,
            out,
            json,
            skip_worlds,
            skip_routes,
        } => run_diff(
            before,
            after,
            project,
            ticks,
            out,
            json,
            skip_worlds,
            skip_routes,
        ),
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
        print_json(&analysis)?;
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

#[allow(clippy::too_many_arguments)]
fn run_search(
    project: Utf8PathBuf,
    wishes: Utf8PathBuf,
    base_seed: Option<String>,
    budget: Option<u32>,
    out: Option<Utf8PathBuf>,
    json: bool,
    strict: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let input = sectorforge::load_project(&project)?;
    let mut wishes_file = sectorforge::search::load_wishes(&wishes)?;
    if let Some(b) = base_seed {
        wishes_file.search.base_seed = Some(b);
    }
    if let Some(n) = budget {
        wishes_file.search.budget = n;
    }
    let outcome = sectorforge::run_seed_search(&input, &wishes_file)?;
    if !outcome.preflight_errors.is_empty() {
        for e in &outcome.preflight_errors {
            eprintln!("preflight: {e}");
        }
        return Ok(ExitCode::from(1));
    }
    if let Some(dir) = &out {
        sectorforge::write_search_outcome(dir, &outcome)?;
        println!("Wrote {dir}/search.md and {dir}/search.json");
    } else if json {
        print_json(&outcome)?;
    } else {
        let md = sectorforge::search::render_outcome_markdown(&outcome);
        print!("{md}");
    }
    if strict && outcome.winning.is_none() {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

fn load_or_regenerate(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
) -> Result<sectorforge::GeneratedSector, sectorforge::SectorError> {
    match (project, sector) {
        (Some(project), None) => {
            let input = sectorforge::load_project(&project)?;
            sectorforge::generate_sector(input)
        }
        (None, Some(sector)) => sectorforge::load_sector_json(&sector),
        (Some(_), Some(_)) | (None, None) => Err(sectorforge::SectorError::InvalidConfig(
            "pass exactly one of --project <dir> or --sector <path>".into(),
        )),
    }
}

fn run_history(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
    out: Option<Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project, sector)?;
    let cfg = sectorforge::history::HistoryConfig::default();
    let report = sectorforge::derive_history_with(&sec, &cfg);
    if let Some(dir) = &out {
        sectorforge::write_history(dir, &report, &cfg)?;
        println!("Wrote {dir}/history.md and {dir}/history.json");
    } else if json {
        print_json(&report)?;
    } else {
        let md = sectorforge::history::render_markdown(&report, &cfg);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_personae(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
    out: Option<Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project, sector)?;
    let report = sectorforge::derive_personae(&sec);
    if let Some(dir) = &out {
        sectorforge::write_personae(dir, &report)?;
        println!("Wrote {dir}/personae.md and {dir}/personae.json");
    } else if json {
        print_json(&report)?;
    } else {
        let md = sectorforge::personae::render_markdown(&report);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_hooks(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
    out: Option<Utf8PathBuf>,
    json: bool,
    player: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project, sector)?;
    let cfg = sectorforge::hooks::HooksConfig {
        hide_hidden_hooks: player,
        ..Default::default()
    };
    let report = sectorforge::derive_hooks_with(&sec, &cfg);
    if let Some(dir) = &out {
        sectorforge::write_hooks(dir, &report, &cfg)?;
        println!("Wrote {dir}/hooks.md and {dir}/hooks.json");
    } else if json {
        print_json(&report)?;
    } else {
        let md = sectorforge::hooks::render_markdown(&report, &cfg);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_relations(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
    out: Option<Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    // When using --project the per-project relations.toml is honoured;
    // --sector falls back to the built-in defaults.
    let (sec, cfg) = match (&project, &sector) {
        (Some(p), None) => {
            let input = sectorforge::load_project(p)?;
            let cfg = input.relations.clone();
            (sectorforge::generate_sector(input)?, cfg)
        }
        (None, Some(s)) => (
            sectorforge::load_sector_json(s)?,
            sectorforge::relations::RelationsConfig::default(),
        ),
        _ => {
            return Err(sectorforge::SectorError::InvalidConfig(
                "pass exactly one of --project <dir> or --sector <path>".into(),
            ));
        }
    };
    let matrix = sectorforge::derive_relations_with(&sec, &cfg);
    let report = sectorforge::relations::RelationsReport {
        sector_id: sec.id.clone(),
        seed: sec.seed.clone(),
        matrix,
    };
    if let Some(dir) = &out {
        sectorforge::write_relations(dir, &report)?;
        println!("Wrote {dir}/relations.md and {dir}/relations.json");
    } else if json {
        print_json(&report)?;
    } else {
        let md = sectorforge::relations::render_markdown(&report);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_regions(
    project: Utf8PathBuf,
    out: Option<Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let input = sectorforge::load_project(&project)?;
    let cfg = input.regions.clone();
    let regs = sectorforge::build_regions(
        &input.config.generation.seed,
        input.config.generation.sector_width,
        input.config.generation.sector_height,
        &cfg,
    );
    if let Some(dir) = &out {
        sectorforge::write_regions(dir, &input.config.project.id, &regs)?;
        println!("Wrote {dir}/regions.md and {dir}/regions.json");
    } else if json {
        print_json(&regs)?;
    } else {
        let md = sectorforge::regions::render_markdown(&input.config.project.id, &regs);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_economy(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
    out: Option<Utf8PathBuf>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let (sec, cfg) = match (&project, &sector) {
        (Some(p), None) => {
            let input = sectorforge::load_project(p)?;
            let cfg = sectorforge::economy::EconomyConfig {
                enabled: true,
                ..input.economy.clone()
            };
            (sectorforge::generate_sector(input)?, cfg)
        }
        (None, Some(s)) => {
            let cfg = sectorforge::economy::EconomyConfig {
                enabled: true,
                ..Default::default()
            };
            (sectorforge::load_sector_json(s)?, cfg)
        }
        _ => {
            return Err(sectorforge::SectorError::InvalidConfig(
                "pass exactly one of --project <dir> or --sector <path>".into(),
            ));
        }
    };
    let report = sectorforge::derive_economy_with(&sec, &cfg);
    if let Some(dir) = &out {
        sectorforge::write_economy(dir, &sec.id, &report)?;
        println!("Wrote {dir}/economy.md, {dir}/economy.json, and {dir}/economy.csv");
    } else if json {
        print_json(&report)?;
    } else {
        let md = sectorforge::economy::render_markdown(&sec.id, &report);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_prose(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
    out: Option<Utf8PathBuf>,
    json: bool,
    dispatch: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project, sector)?;
    let cfg = sectorforge::prose::ProseConfig {
        tone: if dispatch {
            sectorforge::prose::ProseTone::Dispatch
        } else {
            sectorforge::prose::ProseTone::Gazetteer
        },
        ..Default::default()
    };
    let report = sectorforge::derive_prose_with(&sec, &cfg);
    if let Some(dir) = &out {
        sectorforge::write_prose(dir, &report)?;
        println!("Wrote {dir}/gazetteer.md and {dir}/gazetteer.json");
    } else if json {
        print_json(&report)?;
    } else {
        let md = sectorforge::prose::render_markdown(&report);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_compose(
    segmentum_path: Utf8PathBuf,
    out: Utf8PathBuf,
    stitch_seed: Option<String>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let mut file = sectorforge::load_segmentum_file(&segmentum_path)?;
    if let Some(s) = stitch_seed {
        file.segmentum.stitch_seed = s;
    }
    let base_dir = segmentum_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| Utf8PathBuf::from("."));
    let seg = sectorforge::compose_segmentum(&file, &base_dir, &out)?;
    if json {
        print_json(&seg)?;
    } else {
        sectorforge::write_segmentum(&out, &seg)?;
        println!(
            "Composed segmentum '{}' — {} children, {} inter-sector links, {} systems",
            seg.id,
            seg.children.len(),
            seg.inter_sector_links.len(),
            seg.manifest.system_count
        );
        println!("Output written to: {out}");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_interestingness(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
    out: Option<Utf8PathBuf>,
    json: bool,
    profile: Option<String>,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project, sector)?;
    let mut cfg = sectorforge::interestingness::InterestingnessConfig::default();
    if let Some(p) = profile.as_deref() {
        cfg.profile = parse_interestingness_profile(p)?;
    }
    let report = sectorforge::derive_interestingness_with(&sec, &cfg);
    if let Some(dir) = &out {
        sectorforge::write_interestingness(dir, &report)?;
        println!("Wrote {dir}/interestingness.md and {dir}/interestingness.json");
    } else if json {
        print_json(&report)?;
    } else {
        print!("{}", sectorforge::interestingness::render_markdown(&report));
    }
    Ok(ExitCode::SUCCESS)
}

fn parse_interestingness_profile(
    s: &str,
) -> Result<sectorforge::interestingness::ProfileId, sectorforge::SectorError> {
    use sectorforge::interestingness::ProfileId;
    match s.to_ascii_lowercase().as_str() {
        "political_sandbox" | "sandbox" => Ok(ProfileId::PoliticalSandbox),
        "grim_collapse" | "collapse" | "grim" => Ok(ProfileId::GrimCollapse),
        "mercantile" | "trade" => Ok(ProfileId::Mercantile),
        "villainous" | "villain" => Ok(ProfileId::Villainous),
        "frontier" => Ok(ProfileId::Frontier),
        other => Err(sectorforge::SectorError::InvalidConfig(format!(
            "unknown interestingness profile '{other}' (expected political_sandbox|grim_collapse|mercantile|villainous|frontier)"
        ))),
    }
}

fn run_briefing(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
    out: Utf8PathBuf,
    preset: String,
    observer: Option<String>,
    min_confidence: Option<u8>,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project, sector)?;
    let audience = sectorforge::briefing::parse_preset(&preset).ok_or_else(|| {
        sectorforge::SectorError::InvalidConfig(format!(
            "unknown briefing preset '{preset}' (expected gm|navy|inquisition|trader|governor|public)"
        ))
    })?;
    let mut profile = sectorforge::briefing::preset(audience);
    if let Some(obs) = observer {
        profile.observer_faction = Some(obs);
    }
    if let Some(m) = min_confidence {
        profile.minimum_intel_confidence = m;
    }
    let pack = sectorforge::apply_briefing(&sec, &profile);
    sectorforge::write_briefing(&out, &pack, &profile)?;
    println!(
        "Wrote {out}/briefing-{}.md and {out}/briefing-{}.json",
        profile.id, profile.id
    );
    Ok(ExitCode::SUCCESS)
}

fn run_missions(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
    out: Option<Utf8PathBuf>,
    json: bool,
    player: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project, sector)?;
    let cfg = sectorforge::missions::MissionsConfig {
        player_edition: player,
        ..Default::default()
    };
    let report = sectorforge::derive_missions_with(&sec, &cfg);
    if let Some(dir) = &out {
        sectorforge::write_missions(dir, &report, &cfg)?;
        println!("Wrote {dir}/missions.md and {dir}/missions.json");
    } else if json {
        print_json(&report)?;
    } else {
        print!("{}", sectorforge::missions::render_markdown(&report, &cfg));
    }
    Ok(ExitCode::SUCCESS)
}

fn run_sites(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
    out: Option<Utf8PathBuf>,
    json: bool,
    player: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let sec = load_or_regenerate(project, sector)?;
    let cfg = sectorforge::sites::SitesConfig {
        player_edition: player,
        ..Default::default()
    };
    let report = sectorforge::derive_sites_with(&sec, &cfg);
    if let Some(dir) = &out {
        sectorforge::write_sites(dir, &report, &cfg)?;
        println!("Wrote {dir}/sites.md and {dir}/sites.json");
    } else if json {
        print_json(&report)?;
    } else {
        print!("{}", sectorforge::sites::render_markdown(&report, &cfg));
    }
    Ok(ExitCode::SUCCESS)
}

#[allow(clippy::too_many_arguments)]
fn run_diff(
    before: Option<Utf8PathBuf>,
    after: Option<Utf8PathBuf>,
    project: Option<Utf8PathBuf>,
    ticks: Option<u32>,
    out: Option<Utf8PathBuf>,
    json: bool,
    skip_worlds: bool,
    skip_routes: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    let cfg = sectorforge::diff::DiffConfig {
        skip_worlds,
        skip_routes,
        ..Default::default()
    };
    let diff = match (before, after, project, ticks) {
        (Some(a), Some(b), None, None) => {
            let sa = sectorforge::load_sector_json(&a)?;
            let sb = sectorforge::load_sector_json(&b)?;
            sectorforge::diff_sectors_with(&sa, &sb, &cfg)
        }
        (None, None, Some(proj), Some(n)) => {
            let input = sectorforge::load_project(&proj)?;
            let (d, _, _) = sectorforge::diff::diff_after_ticks(input, n, &cfg)?;
            d
        }
        _ => {
            return Err(sectorforge::SectorError::InvalidConfig(
                "pass either --before <a.json> --after <b.json> or --project <dir> --ticks <N>"
                    .into(),
            ));
        }
    };
    if let Some(dir) = &out {
        sectorforge::write_diff(dir, &diff)?;
        println!("Wrote {dir}/diff.md and {dir}/diff.json");
    } else if json {
        print_json(&diff)?;
    } else {
        let md = sectorforge::render_diff_markdown(&diff);
        print!("{md}");
    }
    Ok(ExitCode::SUCCESS)
}

fn print_json<T: Serialize>(value: &T) -> Result<(), sectorforge::SectorError> {
    let text = to_json_pretty(value)?;
    println!("{text}");
    Ok(())
}

fn to_json_pretty<T: Serialize>(value: &T) -> Result<String, sectorforge::SectorError> {
    serde_json::to_string_pretty(value).map_err(|e| sectorforge::SectorError::ExportFailed {
        path: "<stdout>".to_string(),
        message: e.to_string(),
    })
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
        "tension" => Ok(HeatmapMode::Tension),
        "trade_volume" | "trade-volume" | "tradevol" => Ok(HeatmapMode::TradeVolume),
        "food" | "food_output" | "food-output" => Ok(HeatmapMode::FoodOutput),
        "tithe" | "tithe_stress" | "tithe-stress" => Ok(HeatmapMode::TitheStress),
        "supply" | "supply_vulnerability" | "supply-vulnerability" => {
            Ok(HeatmapMode::SupplyVulnerability)
        }
        other => Err(sectorforge::SectorError::InvalidConfig(format!(
            "unknown heatmap mode '{other}' (expected off|control|military|trade|industrial|covert|faith|threat|intel|tension|trade_volume|food|tithe|supply)"
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
