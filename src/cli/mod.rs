//! sectorforge CLI: argument parsing + subcommand dispatch.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod analyze;
mod briefing;
mod common;
mod compose;
mod diff;
mod economy;
pub mod exit_code;
mod generate;
mod generate_all;
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
    Validate(validate::ValidateArgs),
    /// Generate a full sector from a project directory.
    Generate(generate::GenerateArgs),
    /// Generate every example project under a directory (default `examples/`),
    /// overwriting each one's own `out/` folder. A project is any immediate
    /// subdirectory containing a `sectorforge.toml`; each is generated with its
    /// own configured seed and formats. One project's failure is reported but
    /// does not abort the rest, and exits non-zero if any failed.
    GenerateAll(generate_all::GenerateAllArgs),
    /// Synthesise a fully-complete, fully-randomised sector from nothing but a
    /// size (RANDOM.md). Materialises a fresh project under `--out` with every
    /// overlay enabled, generates it, runs the five post-generation derivations
    /// (personae/sites/hooks/missions/prose), and exports the bundle + reports.
    Random(random::RandomArgs),
    /// Generate a single standalone system from a project directory.
    GenerateSystem(generate::GenerateSystemArgs),
    /// Load a previously generated sector JSON and check post-generation
    /// invariants (spec §11.11).
    ValidateSector(validate::ValidateSectorArgs),
    /// Render a Markdown overview from a previously generated sector JSON.
    RenderMarkdown(validate::RenderMarkdownArgs),
    /// Print statistics for a standalone world-data directory
    /// (containing `worlds.toml`).
    InspectWorlds(validate::InspectWorldsArgs),
    /// §8 NEW.md: read-only analytics dashboard for a generated sector.
    /// Accepts either `--project <dir>` (regenerates) or `--sector <path>`
    /// (loads an existing sector.json). Writes `analysis.md` + `analysis.json`
    /// to `--out`, or prints Markdown to stdout when `--out` is omitted.
    Analyze(analyze::AnalyzeArgs),
    /// §9 NEW.md: scaffold a new project from a bundled preset.
    /// Copies `presets/<preset>/` into `<out>` and writes a header comment to
    /// `sectorforge.toml`. The destination must not already exist.
    New(presets::NewArgs),
    /// §9 NEW.md: list available presets in `--presets-dir` (default `presets`).
    ListPresets(presets::ListPresetsArgs),
    /// §2 NEW.md: deterministic constraint-directed seed search.
    /// Loads `<project>`, reads a `wishes.toml` listing constraints, and
    /// enumerates seeds derived from the base seed until one satisfies all
    /// constraints (or the budget is exhausted). Writes `search.md` +
    /// `search.json` to `--out` when given, otherwise prints the Markdown.
    Search(search::SearchArgs),
    /// §1 NEW2.md: derive a deterministic chronicle of in-universe events from
    /// a generated sector. Accepts either `--project <dir>` (regenerates) or
    /// `--sector <path>` (loads an existing sector.json). Writes
    /// `history.md` + `history.json` to `--out`, or prints Markdown to stdout.
    History(history::HistoryArgs),
    /// §3 NEW.md: derive a deterministic dramatis personae overlay (named
    /// characters per faction presence). Accepts `--project` or `--sector`.
    Personae(personae::PersonaeArgs),
    /// §7 NEW.md: derive adventure / plot hooks. Accepts `--project` or
    /// `--sector`. `--player` hides GM-only hooks (hidden-presence derived).
    Hooks(hooks::HooksArgs),
    /// §6 NEW.md: derive a narrative gazetteer (deterministic template
    /// prose). Accepts `--project` or `--sector`. `--dispatch` switches the
    /// tone preset.
    Prose(prose::ProseArgs),
    /// §5 NEW2.md: derive the inter-faction diplomacy matrix.
    /// Accepts `--project` or `--sector`.
    Relations(relations::RelationsArgs),
    /// §5 NEW.md: emit the regional warp-phenomena overlay for a project's
    /// grid. Requires a project so the regions config can be read.
    Regions(regions::RegionsArgs),
    /// §12 NEW.md / §4 NEW2.md: derive trade, tithe, and strategic-resource snapshot.
    /// Accepts `--project` or `--sector`.
    #[command(alias = "analyze-economy", alias = "analyze-tithes")]
    Economy(economy::EconomyArgs),
    /// §14 NEW.md: compose a segmentum (multi-sector) from `segmentum.toml`.
    /// Loads + generates every listed child sector (reusing the existing
    /// deterministic pipeline), then runs a deterministic stitch stage to
    /// emit inter-sector warp links and a super-manifest.
    Compose(compose::ComposeArgs),
    /// §18 NEW2.md: interestingness scorecard for a sector against a target
    /// profile (political sandbox / grim collapse / mercantile / villainous /
    /// frontier). Accepts `--project` or `--sector`.
    Interestingness(interestingness::InterestingnessArgs),
    /// §9 NEW2.md: audience-targeted redaction pack. Applies a built-in
    /// briefing profile (gm / navy / inquisition / trader / governor /
    /// public) and writes a redacted sector + summary into `--out`.
    Briefing(briefing::BriefingArgs),
    /// §3 NEW2.md: deterministic mission / quest seeds derived from sector
    /// state. Accepts `--project` or `--sector`. `--player` hides GM-only
    /// missions.
    Missions(missions::MissionsArgs),
    /// §7 NEW2.md: planetary points-of-interest per world. Accepts
    /// `--project` or `--sector`. `--player` hides sites whose public status
    /// differs from actual status.
    Sites(sites::SitesArgs),
    /// §10 NEW.md: deterministic sector diff. Two modes:
    ///
    /// * `--before <a.json> --after <b.json>`: compare two saved sectors.
    /// * `--project <dir> --ticks <N>`: generate the sector, advance N
    ///   ticks, diff before vs. after.
    Diff(diff::DiffArgs),
}

pub fn run(cli: Cli) -> Result<ExitCode, sectorforge::SectorError> {
    match cli.command {
        Command::Validate(args) => validate::run_validate(args),
        Command::Generate(args) => generate::run_generate(args),
        Command::GenerateAll(args) => generate_all::run_generate_all(args),
        Command::Random(args) => random::run_random(args),
        Command::GenerateSystem(args) => generate::run_generate_system(args),
        Command::ValidateSector(args) => validate::run_validate_sector(args),
        Command::RenderMarkdown(args) => validate::run_render_markdown(args),
        Command::InspectWorlds(args) => validate::run_inspect_worlds(args),
        Command::Analyze(args) => analyze::run_analyze(args),
        Command::New(args) => presets::run_new(args),
        Command::ListPresets(args) => presets::run_list_presets(args),
        Command::Search(args) => search::run_search(args),
        Command::History(args) => history::run_history(args),
        Command::Personae(args) => personae::run_personae(args),
        Command::Hooks(args) => hooks::run_hooks(args),
        Command::Prose(args) => prose::run_prose(args),
        Command::Relations(args) => relations::run_relations(args),
        Command::Regions(args) => regions::run_regions(args),
        Command::Economy(args) => economy::run_economy(args),
        Command::Compose(args) => compose::run_compose(args),
        Command::Interestingness(args) => interestingness::run_interestingness(args),
        Command::Briefing(args) => briefing::run_briefing(args),
        Command::Missions(args) => missions::run_missions(args),
        Command::Sites(args) => sites::run_sites(args),
        Command::Diff(args) => diff::run_diff(args),
    }
}
