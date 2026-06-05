use std::process::ExitCode;

use camino::Utf8PathBuf;
use clap::Parser;
use sectorforge::ids::FactionId;
use sectorforge_builder::builder::open_project;
use sectorforge_builder::builder::state::BuilderTab;
use sectorforge_builder::BuilderApp;
use sectorforge_gui_core::theme::Theme;

#[derive(Debug, Parser)]
#[command(
    name = "sectorforge-builder",
    about = "Interactive builder for 40k sectors"
)]
struct Cli {
    /// Project directory to open. If omitted, launches with a blank workspace.
    #[arg(long)]
    project: Option<Utf8PathBuf>,

    /// Dev tooling: render a few frames, capture the window to this PNG, then
    /// exit. Backs the autonomous visual-feedback loop (BEAUTY.md §0) — a way to
    /// *look at* a chrome change instead of beautifying blind. Implies a
    /// one-shot, non-interactive run.
    #[arg(long, value_name = "PNG")]
    screenshot: Option<Utf8PathBuf>,

    /// Frames to render before capturing, so layout + the first derivations
    /// settle. Only meaningful together with `--screenshot`.
    #[arg(long, default_value_t = 12)]
    screenshot_frames: u32,

    /// Dev tooling: open straight to a named tab (e.g. `FACTIONS`, `MAP`).
    /// Case-insensitive; matches the tab strip labels. Requires `--project`.
    #[arg(long, value_name = "TAB")]
    tab: Option<String>,

    /// Dev tooling: pre-select a faction by id (so a capture shows the selected
    /// state). Requires `--project`.
    #[arg(long, value_name = "FACTION_ID")]
    select_faction: Option<String>,

    /// Dev tooling: start in a named chrome theme (e.g. `Grimdark`, `Light`,
    /// `Slate`). Case-insensitive; matches the `Theme:` menu labels.
    #[arg(long, value_name = "THEME")]
    theme: Option<String>,
}

/// Resolve a `--tab` value against the tab-strip labels (case-insensitive).
fn parse_tab(s: &str) -> Option<BuilderTab> {
    let up = s.trim().to_ascii_uppercase();
    BuilderTab::ALL.iter().copied().find(|t| t.label() == up)
}

/// Resolve a `--theme` value against the theme labels (case-insensitive).
fn parse_theme(s: &str) -> Option<Theme> {
    let s = s.trim();
    Theme::ALL
        .into_iter()
        .find(|t| t.label().eq_ignore_ascii_case(s))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut app = if let Some(dir) = cli.project {
        match open_project(&dir) {
            Ok(mut state) => {
                // Dev capture conveniences (no-ops in normal interactive use).
                if let Some(tab) = cli.tab.as_deref().and_then(parse_tab) {
                    state.set_active_tab(tab);
                }
                if let Some(fid) = cli.select_faction.as_deref() {
                    state.select_faction(Some(FactionId::new(fid)));
                }
                BuilderApp::with_initial_state(state)
            }
            Err(e) => {
                eprintln!("failed to open project '{dir}': {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        BuilderApp::new()
    };

    if let Some(theme) = cli.theme.as_deref().and_then(parse_theme) {
        app.set_theme(theme);
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1400.0, 900.0])
        .with_title("sectorforge - builder");
    if let Some(icon) = sectorforge_gui_core::app_icon::load_app_icon() {
        viewport = viewport.with_icon(icon);
    }
    let opts = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let screenshot = cli.screenshot;
    let screenshot_frames = cli.screenshot_frames;

    let run = eframe::run_native(
        "sectorforge-builder",
        opts,
        Box::new(move |cc| {
            // §BEAUTY §5.5: register the bundled display/body/mono faces before
            // the first frame applies the theme. No-op unless `bundled-fonts` is on.
            sectorforge_gui_core::fonts::install(&cc.egui_ctx);
            let app: Box<dyn eframe::App> = if let Some(path) = screenshot {
                Box::new(ScreenshotApp::new(app, path, screenshot_frames))
            } else {
                Box::new(app)
            };
            Ok(app)
        }),
    );

    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("builder error: {e}");
            ExitCode::from(1)
        }
    }
}

/// Dev-only one-shot capture wrapper (see the `--screenshot` flag). It delegates
/// every frame to the real [`BuilderApp`]; once the UI has settled it sends an
/// egui `Screenshot` viewport command, writes the returned framebuffer to a PNG
/// via [`sectorforge_gui_core::save_color_image_png`], and closes the window.
/// Lives here rather than inside `BuilderApp` so the shipping app carries none
/// of the capture machinery.
///
/// egui 0.29 specifics: `ViewportCommand::Screenshot` is a *unit* variant (it
/// gained a `UserData` argument only in 0.30), and the captured image arrives
/// the following frame as `Event::Screenshot { image, .. }`.
struct ScreenshotApp {
    inner: BuilderApp,
    path: Utf8PathBuf,
    capture_at: u32,
    frame: u32,
    requested: bool,
}

impl ScreenshotApp {
    fn new(inner: BuilderApp, path: Utf8PathBuf, capture_at: u32) -> Self {
        Self {
            inner,
            path,
            capture_at,
            frame: 0,
            requested: false,
        }
    }
}

impl eframe::App for ScreenshotApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::update(&mut self.inner, ctx, frame);

        self.frame += 1;
        // BuilderApp only repaints on input; drive a steady cadence so we reach
        // `capture_at` and then observe the screenshot event on the next frame.
        ctx.request_repaint();

        // Fail-safe: never spin forever if the backend can't satisfy a capture
        // (e.g. a headless host with no GL surface) — close after a grace window.
        if self.requested && self.frame > self.capture_at + 180 {
            eprintln!("screenshot: no capture event arrived; giving up");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if !self.requested && self.frame >= self.capture_at {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            self.requested = true;
            return;
        }

        if self.requested {
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                match sectorforge_gui_core::save_color_image_png(self.path.as_std_path(), &image) {
                    Ok(()) => eprintln!(
                        "screenshot written: {} ({}x{})",
                        self.path, image.size[0], image.size[1]
                    ),
                    Err(e) => eprintln!("screenshot save failed: {e}"),
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
