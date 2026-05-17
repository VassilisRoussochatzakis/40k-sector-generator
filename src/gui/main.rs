//! sectorforge-gui — viewer for a previously generated sector JSON.
//!
//! Usage:
//!   sectorforge-gui <path/to/sector.json>
//!   sectorforge-gui --project <project-dir>          # auto-loads out/sector.json
//!   (no args)                                        # tries examples/m42_project/out/sector.json

use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::Parser;

use sectorforge::gui::App;

#[derive(Debug, Parser)]
#[command(name = "sectorforge-gui", about = "Interactive viewer for a generated 40k sector")]
struct Cli {
    /// Path to a generated sector.json file.
    sector: Option<Utf8PathBuf>,
    /// Project directory (loads <dir>/out/sector.json).
    #[arg(long)]
    project: Option<Utf8PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let path = resolve_sector_path(&cli);
    let path = match path {
        Some(p) => p,
        None => {
            eprintln!(
                "no sector.json found. pass a path, or --project <dir>, or run from a directory \
                 where examples/m42_project/out/sector.json exists."
            );
            return ExitCode::from(2);
        }
    };
    let sector = match sectorforge::load_sector_json(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to load sector json '{}': {}", path, e);
            return ExitCode::from(2);
        }
    };
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title(format!("sectorforge — {}", sector.id)),
        ..Default::default()
    };
    let res = eframe::run_native(
        "sectorforge",
        native_options,
        Box::new(move |_cc| Ok(Box::new(App::new(sector)))),
    );
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gui error: {e}");
            ExitCode::from(1)
        }
    }
}

fn resolve_sector_path(cli: &Cli) -> Option<Utf8PathBuf> {
    if let Some(p) = &cli.sector {
        return Some(p.clone());
    }
    if let Some(dir) = &cli.project {
        let p = dir.join("out").join("sector.json");
        if p.exists() {
            return Some(p);
        }
    }
    let default = Utf8PathBuf::from("examples/m42_project/out/sector.json");
    if default.exists() {
        return Some(default);
    }
    None
}
