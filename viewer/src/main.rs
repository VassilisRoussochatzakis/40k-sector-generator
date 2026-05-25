//! sectorforge-viewer — viewer for a previously generated sector JSON.
//!
//! Usage:
//!   sectorforge-viewer <path/to/sector.json>
//!   sectorforge-viewer --project <project-dir>       # auto-loads out/sector.json
//!   sectorforge-viewer --segmentum <path/to/segmentum.json>
//!   (no args)                                        # launches editor with no sector loaded

use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::Parser;

use sectorforge_viewer::App;

#[derive(Debug, Parser)]
#[command(
    name = "sectorforge-viewer",
    about = "Interactive viewer for a generated 40k sector"
)]
struct Cli {
    /// Path to a generated sector.json file.
    #[arg(conflicts_with = "segmentum")]
    sector: Option<Utf8PathBuf>,
    /// Project directory (loads <dir>/out/sector.json).
    #[arg(long, conflicts_with = "segmentum")]
    project: Option<Utf8PathBuf>,
    /// Path to a composed segmentum.json file.
    #[arg(long)]
    segmentum: Option<Utf8PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let project_dir = if cli.segmentum.is_some() {
        None
    } else {
        resolve_project_dir(&cli)
    };
    let path = resolve_sector_path(&cli);
    let (mut app, title) = if let Some(p) = &cli.segmentum {
        match sectorforge_viewer::segmentum_view::load_segmentum_bundle(p) {
            Ok(bundle) => {
                let t = format!("sectorforge — {}", bundle.segmentum.id);
                (App::new_segmentum(bundle), t)
            }
            Err(e) => {
                eprintln!("failed to load segmentum json '{p}': {e}");
                return ExitCode::from(2);
            }
        }
    } else if let Some(p) = path {
        match sectorforge::load_sector_json(&p) {
            Ok(s) => {
                let t = format!("sectorforge — {}", s.id);
                (App::new_with_source(s, p.clone().into_std_path_buf()), t)
            }
            Err(e) => {
                eprintln!("failed to load sector json '{p}': {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        eprintln!("no sector.json found — launching editor with no sector loaded");
        (App::new_empty(), "sectorforge — editor".to_string())
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
            eprintln!("viewer error: {e}");
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
    None
}

fn resolve_project_dir(cli: &Cli) -> Option<Utf8PathBuf> {
    if let Some(dir) = &cli.project {
        return Some(dir.clone());
    }
    None
}
