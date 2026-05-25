//! `new` and `list-presets`.

use std::process::ExitCode;

use camino::Utf8PathBuf;

pub fn run_new(
    out: &Utf8PathBuf,
    preset: &str,
    seed: Option<&str>,
    presets_dir: &Utf8PathBuf,
) -> Result<ExitCode, sectorforge::SectorError> {
    sectorforge::presets::scaffold(presets_dir, preset, out, seed).map(|_| {
        println!("Scaffolded project '{out}' from preset '{preset}'.");
        println!("Next:  cargo run --bin sectorforge -- generate --project {out}");
        ExitCode::SUCCESS
    })
}

pub fn run_list_presets(presets_dir: &Utf8PathBuf) -> Result<ExitCode, sectorforge::SectorError> {
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
