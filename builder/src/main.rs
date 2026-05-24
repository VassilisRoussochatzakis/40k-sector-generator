use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::Parser;
use sectorforge_builder::builder::open_project;
use sectorforge_builder::BuilderApp;

#[derive(Debug, Parser)]
#[command(
    name = "sectorforge-builder",
    about = "Interactive builder for 40k sectors"
)]
struct Cli {
    /// Project directory to open. If omitted, launches with a blank workspace.
    #[arg(long)]
    project: Option<Utf8PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let app = if let Some(dir) = cli.project {
        match open_project(&dir) {
            Ok(state) => BuilderApp::with_initial_state(state),
            Err(e) => {
                eprintln!("failed to open project '{dir}': {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        BuilderApp::new()
    };

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("sectorforge - builder"),
        ..Default::default()
    };

    match eframe::run_native(
        "sectorforge-builder",
        opts,
        Box::new(move |_cc| Ok(Box::new(app))),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gui error: {e}");
            ExitCode::from(1)
        }
    }
}
