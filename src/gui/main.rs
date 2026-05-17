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
#[command(
    name = "sectorforge-gui",
    about = "Interactive viewer for a generated 40k sector"
)]
struct Cli {
    /// Path to a generated sector.json file.
    sector: Option<Utf8PathBuf>,
    /// Project directory (loads <dir>/out/sector.json).
    #[arg(long)]
    project: Option<Utf8PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let project_dir = resolve_project_dir(&cli);
    let path = resolve_sector_path(&cli);
    let (mut app, title) = match path {
        Some(p) => match sectorforge::load_sector_json(&p) {
            Ok(s) => {
                let t = format!("sectorforge — {}", s.id);
                (App::new(s), t)
            }
            Err(e) => {
                eprintln!("failed to load sector json '{}': {}", p, e);
                return ExitCode::from(2);
            }
        },
        None => {
            eprintln!("no sector.json found — launching editor with no sector loaded");
            (App::new_empty(), "sectorforge — editor".to_string())
        }
    };
    if let Some(dir) = project_dir {
        app = app.with_project_dir(dir.into_std_path_buf());
    }
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title(title),
        ..Default::default()
    };
    let res = eframe::run_native(
        "sectorforge",
        native_options,
        Box::new(move |_cc| Ok(Box::new(app))),
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

fn resolve_project_dir(cli: &Cli) -> Option<Utf8PathBuf> {
    if let Some(dir) = &cli.project {
        return Some(dir.clone());
    }
    let default = Utf8PathBuf::from("examples/m42_project");
    if default.join("sectorforge.toml").exists() {
        return Some(default);
    }
    None
}
