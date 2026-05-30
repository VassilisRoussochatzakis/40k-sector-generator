//! sectorforge CLI: argument parsing + subcommand dispatch.

use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

mod analyze;
mod briefing;
mod common;
mod compose;
mod diff;
mod economy;
pub mod exit_code;
mod generate;
mod history;
mod hooks;
mod interestingness;
mod missions;
mod personae;
mod presets;
mod prose;
mod random;
mod regions;
mod relations;
mod search;
mod sites;
mod validate;

#[derive(Debug, Parser)]
#[command(
    name = "sectorforge",
    version,
    about = "Generate a Warhammer 40k star sector from data files"
)]
pub struct Cli {
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
        /// PNG map theme (§13). One of: gm_dark, print_mono,
        /// imperial_archive, navis_tactical, inquisition_redacted,
        /// subsector_political.
        #[arg(long)]
        theme: Option<String>,
        /// §15 NEW2.md: Constraint file to satisfy during generation.
        #[arg(long)]
        constraints: Option<Utf8PathBuf>,
        /// §15 NEW2.md: Maximum number of candidate seeds to evaluate.
        #[arg(long)]
        max_candidates: Option<u32>,
        /// Comma-separated output formats. Overrides
        /// `[outputs].formats` in `sectorforge.toml`. Tokens: `json`,
        /// `markdown`, `png` (alias for `bitmap`), `svg`, `html`.
        #[arg(long, value_delimiter = ',')]
        formats: Option<Vec<String>>,
    },
    /// Synthesise a fully-complete, fully-randomised sector from nothing but a
    /// size (RANDOM.md). Materialises a fresh project under `--out` with every
    /// overlay enabled, generates it, runs the five post-generation derivations
    /// (personae/sites/hooks/missions/prose), and exports the bundle + reports.
    Random {
        /// Sector size: small | medium | large | vast | massive | huge.
        /// Ignored when --width/--height are given. Defaults to medium.
        #[arg(long)]
        size: Option<String>,
        /// Explicit square grid side length (sectors must be square). When
        /// both --width and --height are given they must be equal.
        #[arg(long)]
        width: Option<u32>,
        /// Explicit square grid side length (sectors must be square). When
        /// both --width and --height are given they must be equal.
        #[arg(long)]
        height: Option<u32>,
        /// Reproducibility seed. Omit to mint a fresh one (echoed on success).
        #[arg(long)]
        seed: Option<String>,
        /// Project directory to create (must not exist). Defaults to
        /// `./random-<seed>`.
        #[arg(long)]
        out: Option<Utf8PathBuf>,
        /// Source directory holding presets (must contain `_full`). Defaults
        /// to `./presets`.
        #[arg(long, default_value = "presets")]
        presets_dir: Utf8PathBuf,
        /// Comma-separated output formats to export. Defaults to all five
        /// (json, markdown, png, svg, html).
        #[arg(long, value_delimiter = ',')]
        formats: Option<Vec<String>>,
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
    /// (containing `worlds.toml`).
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
    /// §1 NEW2.md: derive a deterministic chronicle of in-universe events from
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

pub fn run(cli: Cli) -> Result<ExitCode, sectorforge::SectorError> {
    match cli.command {
        Command::Validate {
            project,
            json,
            strict,
        } => validate::run_validate(&project, json, strict),
        Command::Generate {
            project,
            seed,
            out,
            allow_warnings,
            heatmap,
            no_faction_fill,
            theme,
            constraints,
            max_candidates,
            formats,
        } => generate::run_generate(
            project,
            seed,
            out,
            allow_warnings,
            heatmap,
            no_faction_fill,
            theme,
            constraints,
            max_candidates,
            formats,
        ),
        Command::Random {
            size,
            width,
            height,
            seed,
            out,
            presets_dir,
            formats,
        } => random::run_random(size, width, height, seed, out, presets_dir, formats),
        Command::GenerateSystem {
            project,
            seed,
            index,
            coord_q,
            coord_r,
            out,
            markdown,
        } => generate::run_generate_system(project, seed, index, coord_q, coord_r, out, markdown),
        Command::ValidateSector { sector, json } => validate::run_validate_sector(&sector, json),
        Command::RenderMarkdown { sector, out } => {
            validate::run_render_markdown(&sector, out.as_ref())
        }
        Command::InspectWorlds { data_dir } => validate::run_inspect_worlds(&data_dir),
        Command::Analyze {
            project,
            sector,
            out,
            json,
            strict,
        } => analyze::run_analyze(
            project.as_ref(),
            sector.as_ref(),
            out.as_ref(),
            json,
            strict,
        ),
        Command::New {
            out,
            preset,
            seed,
            presets_dir,
        } => presets::run_new(&out, &preset, seed.as_deref(), &presets_dir),
        Command::ListPresets { presets_dir } => presets::run_list_presets(&presets_dir),
        Command::Search {
            project,
            wishes,
            base_seed,
            budget,
            out,
            json,
            strict,
        } => search::run_search(
            &project,
            &wishes,
            base_seed,
            budget,
            out.as_ref(),
            json,
            strict,
        ),
        Command::History {
            project,
            sector,
            out,
            json,
        } => history::run_history(project.as_ref(), sector.as_ref(), out.as_ref(), json),
        Command::Personae {
            project,
            sector,
            out,
            json,
        } => personae::run_personae(project.as_ref(), sector.as_ref(), out.as_ref(), json),
        Command::Hooks {
            project,
            sector,
            out,
            json,
            player,
        } => hooks::run_hooks(
            project.as_ref(),
            sector.as_ref(),
            out.as_ref(),
            json,
            player,
        ),
        Command::Prose {
            project,
            sector,
            out,
            json,
            dispatch,
        } => prose::run_prose(
            project.as_ref(),
            sector.as_ref(),
            out.as_ref(),
            json,
            dispatch,
        ),
        Command::Relations {
            project,
            sector,
            out,
            json,
        } => relations::run_relations(project.as_ref(), sector.as_ref(), out.as_ref(), json),
        Command::Regions { project, out, json } => {
            regions::run_regions(&project, out.as_ref(), json)
        }
        Command::Economy {
            project,
            sector,
            out,
            json,
        } => economy::run_economy(project.as_ref(), sector.as_ref(), out.as_ref(), json),
        Command::Compose {
            segmentum,
            out,
            stitch_seed,
            json,
        } => compose::run_compose(&segmentum, &out, stitch_seed, json),
        Command::Interestingness {
            project,
            sector,
            out,
            json,
            profile,
        } => interestingness::run_interestingness(
            project.as_ref(),
            sector.as_ref(),
            out.as_ref(),
            json,
            profile,
        ),
        Command::Briefing {
            project,
            sector,
            out,
            preset,
            observer,
            min_confidence,
        } => briefing::run_briefing(
            project.as_ref(),
            sector.as_ref(),
            &out,
            preset,
            observer,
            min_confidence,
        ),
        Command::Missions {
            project,
            sector,
            out,
            json,
            player,
        } => missions::run_missions(
            project.as_ref(),
            sector.as_ref(),
            out.as_ref(),
            json,
            player,
        ),
        Command::Sites {
            project,
            sector,
            out,
            json,
            player,
        } => sites::run_sites(
            project.as_ref(),
            sector.as_ref(),
            out.as_ref(),
            json,
            player,
        ),
        Command::Diff {
            before,
            after,
            project,
            ticks,
            out,
            json,
            skip_worlds,
            skip_routes,
        } => diff::run_diff(diff::DiffArgs {
            before,
            after,
            project,
            ticks,
            out,
            json,
            skip_worlds,
            skip_routes,
        }),
    }
}
