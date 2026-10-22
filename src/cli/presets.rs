//! `new` and `list-presets`.

use std::process::ExitCode;

use camino::Utf8PathBuf;

#[derive(Debug, clap::Args)]
pub(super) struct NewArgs {
    /// Destination project directory to create.
    #[arg(long)]
    pub out: Utf8PathBuf,
    /// Preset name (matches a sub-directory of `presets/`).
    #[arg(long)]
    pub preset: String,
    /// Override the preset's bundled seed.
    #[arg(long)]
    pub seed: Option<String>,
    /// Source directory holding presets. Defaults to `./presets`.
    #[arg(long, default_value = "presets")]
    pub presets_dir: Utf8PathBuf,
}

#[derive(Debug, clap::Args)]
pub(super) struct ListPresetsArgs {
    #[arg(long, default_value = "presets")]
    pub presets_dir: Utf8PathBuf,
}

pub(crate) fn run_new(args: NewArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let NewArgs {
        out,
        preset,
        seed,
        presets_dir,
    } = args;
    let out = &out;
    let preset = &preset;
    let seed = seed.as_deref();
    let presets_dir = &presets_dir;

    sectorforge::presets::scaffold(presets_dir, preset, out, seed).map(|_| {
        println!("Scaffolded project '{out}' from preset '{preset}'.");
        println!("Next:  cargo run --bin sectorforge -- generate --project {out}");
        ExitCode::SUCCESS
    })
}

pub(crate) fn run_list_presets(
    args: ListPresetsArgs,
) -> Result<ExitCode, sectorforge::SectorError> {
    let ListPresetsArgs { presets_dir } = args;
    let presets_dir = &presets_dir;

    let entries = sectorforge::presets::list(presets_dir)?;
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
