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
    // Install the crash-note panic hook before any other work: under release
    // `panic = "abort"` a panic is an otherwise-silent hard crash, so this drops
    // a breadcrumb to the OS temp dir first. See `gui-core/src/diagnostics.rs`.
    sectorforge_gui_core::diagnostics::install_panic_hook("viewer");
    let cli = Cli::parse();
    let project_dir = if cli.segmentum.is_some() {
        None
    } else {
        resolve_project_dir(&cli)
    };
    let path = match resolve_sector_path(&cli) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
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
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1400.0, 900.0])
        .with_title(title);
    if let Some(icon) = sectorforge_gui_core::app_icon::load_app_icon() {
        viewport = viewport.with_icon(icon);
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let res = eframe::run_native(
        "sectorforge",
        native_options,
        Box::new(move |cc| {
            // §BEAUTY §5.5: register bundled faces before the first theme apply.
            // No-op unless the `bundled-fonts` feature is on.
            sectorforge_gui_core::fonts::install(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    );
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("viewer error: {e}");
            ExitCode::from(1)
        }
    }
}

/// Resolve the sector.json to load at startup.
///
/// - `Ok(None)` — no load argument given; the caller launches an empty editor.
/// - `Ok(Some(p))` — path to load (positional path is passed through as-is; a
///   missing file then surfaces as a `load_sector_json` error).
/// - `Err(msg)` — a `--project` directory was given but holds no
///   `out/sector.json`; `msg` names the path we tried so the misuse is visible
///   instead of silently falling back to an empty editor.
fn resolve_sector_path(cli: &Cli) -> Result<Option<Utf8PathBuf>, String> {
    if let Some(p) = &cli.sector {
        return Ok(Some(p.clone()));
    }
    if let Some(dir) = &cli.project {
        let p = dir.join("out").join("sector.json");
        if p.exists() {
            return Ok(Some(p));
        }
        let mut msg = format!("--project '{dir}': no sector.json found at '{p}'");
        // Common mistake: passing the sector.json file itself to --project,
        // which then resolves to '<file>/out/sector.json'.
        if dir.extension() == Some("json") {
            msg.push_str(&format!(
                "\nhint: --project takes a directory; to load that file directly, pass it positionally: sectorforge-viewer '{dir}'"
            ));
        }
        return Err(msg);
    }
    Ok(None)
}

fn resolve_project_dir(cli: &Cli) -> Option<Utf8PathBuf> {
    if let Some(dir) = &cli.project {
        return Some(dir.clone());
    }
    None
}
