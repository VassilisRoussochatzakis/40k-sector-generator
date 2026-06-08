//! sectorforge-viewer — viewer for a previously generated sector JSON.
//!
//! Usage:
//!   sectorforge-viewer <path/to/sector.json>
//!   sectorforge-viewer --project <project-dir>       # auto-loads out/sector.json
//!   sectorforge-viewer --segmentum <path/to/segmentum.json>
//!   (no args)                                        # launches editor with no sector loaded

use std::process::ExitCode;

use camino::{Utf8Path, Utf8PathBuf};
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
    /// Project dir, its out/ dir, or a sector.json file (first that exists).
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
    let path = match resolve_sector_path(&cli) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    // When --project is used, the editor's project root is the parent of the
    // `out/` dir holding sector.json — not the (dir / out-dir / file) path the
    // user typed. Matches the auto-detect in app/lifecycle.rs.
    let project_dir = if cli.project.is_some() {
        path.as_deref().and_then(infer_project_dir)
    } else {
        None
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
/// - `Ok(Some(p))` — path to load. The positional arg is passed through as-is
///   (a missing file then surfaces as a `load_sector_json` error). `--project`
///   accepts the project dir, its `out/` dir, or the `sector.json` file itself
///   and resolves to the first candidate that exists.
/// - `Err(msg)` — `--project` was given but none of its candidates exist; `msg`
///   lists every path tried so the misuse is visible instead of silently
///   falling back to an empty editor.
fn resolve_sector_path(cli: &Cli) -> Result<Option<Utf8PathBuf>, String> {
    if let Some(p) = &cli.sector {
        return Ok(Some(p.clone()));
    }
    if let Some(dir) = &cli.project {
        // Accept any of: the sector.json file itself, a project dir
        // (`<dir>/out/sector.json`), or its out dir (`<dir>/sector.json`).
        // First existing file wins.
        let candidates = [
            dir.clone(),
            dir.join("out").join("sector.json"),
            dir.join("sector.json"),
        ];
        if let Some(hit) = candidates.iter().find(|c| c.is_file()) {
            return Ok(Some(hit.clone()));
        }
        let tried = candidates
            .iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join("\n  ");
        return Err(format!(
            "--project '{dir}': no sector.json found. tried:\n  {tried}"
        ));
    }
    Ok(None)
}

/// Infer the editor's project root from a resolved sector.json path: when the
/// file sits in an `out/` dir (`<root>/out/sector.json`), the root is its
/// grandparent; otherwise there is no inferable project root. Mirrors the
/// auto-detect in app/lifecycle.rs so all three `--project` input forms
/// (project dir, out dir, file) collapse to the same root.
fn infer_project_dir(sector: &Utf8Path) -> Option<Utf8PathBuf> {
    let parent = sector.parent()?;
    if parent.file_name() == Some("out") {
        Some(parent.parent()?.to_path_buf())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(sector: Option<&str>, project: Option<&str>) -> Cli {
        Cli {
            sector: sector.map(Utf8PathBuf::from),
            project: project.map(Utf8PathBuf::from),
            segmentum: None,
        }
    }

    fn touch(p: &std::path::Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "{}").unwrap();
    }

    fn utf8(p: std::path::PathBuf) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(p).unwrap()
    }

    #[test]
    fn no_load_arg_resolves_to_none() {
        assert_eq!(resolve_sector_path(&cli(None, None)), Ok(None));
    }

    #[test]
    fn positional_passes_through_unchecked() {
        // A missing positional file is fine here — the caller surfaces it as a
        // load error, so resolution just forwards it verbatim.
        let got = resolve_sector_path(&cli(Some("/nope/sector.json"), None));
        assert_eq!(got, Ok(Some(Utf8PathBuf::from("/nope/sector.json"))));
    }

    #[test]
    fn project_accepts_project_dir() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("out").join("sector.json"));
        let dir = utf8(tmp.path().to_path_buf());
        assert_eq!(
            resolve_sector_path(&cli(None, Some(dir.as_str()))),
            Ok(Some(dir.join("out").join("sector.json")))
        );
    }

    #[test]
    fn project_accepts_out_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let out = utf8(tmp.path().join("out"));
        touch(out.join("sector.json").as_std_path());
        assert_eq!(
            resolve_sector_path(&cli(None, Some(out.as_str()))),
            Ok(Some(out.join("sector.json")))
        );
    }

    #[test]
    fn project_accepts_sector_json_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = utf8(tmp.path().join("out").join("sector.json"));
        touch(file.as_std_path());
        assert_eq!(
            resolve_sector_path(&cli(None, Some(file.as_str()))),
            Ok(Some(file))
        );
    }

    #[test]
    fn project_prefers_out_sector_over_bare_sector() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("out").join("sector.json"));
        touch(&tmp.path().join("sector.json"));
        let dir = utf8(tmp.path().to_path_buf());
        // Generator layout wins: <dir>/out/sector.json, not <dir>/sector.json.
        assert_eq!(
            resolve_sector_path(&cli(None, Some(dir.as_str()))),
            Ok(Some(dir.join("out").join("sector.json")))
        );
    }

    #[test]
    fn project_with_no_sector_errors_and_lists_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = utf8(tmp.path().to_path_buf());
        let err = resolve_sector_path(&cli(None, Some(dir.as_str()))).unwrap_err();
        assert!(err.contains("no sector.json found"), "{err}");
        assert!(err.contains("out/sector.json"), "{err}");
    }

    #[test]
    fn infer_project_dir_from_out_layout() {
        // All three input forms resolve to <root>/out/sector.json, so the
        // project root is always the grandparent.
        let p = Utf8PathBuf::from("/x/proj/out/sector.json");
        assert_eq!(infer_project_dir(&p), Some(Utf8PathBuf::from("/x/proj")));
    }

    #[test]
    fn infer_project_dir_none_outside_out_layout() {
        let p = Utf8PathBuf::from("/x/proj/sector.json");
        assert_eq!(infer_project_dir(&p), None);
    }
}
