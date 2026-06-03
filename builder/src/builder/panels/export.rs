//! EXPORT tab (§N1 / §N2). Phase E §EX1..§EX8 — the export bundle editor, the
//! in-app face of `sectorforge generate --formats …` plus every per-overlay
//! writer the CLI exposes (`history`, `personae`, … `analyze`).
//!
//! §EX1  Per-format toggles (json / markdown / bitmap / svg / html) that edit
//!       the project's own `config.outputs.formats`, plus a single "Export
//!       bundle" that runs [`sectorforge::export_sector`] over the live sector.
//! §EX2  Manifest writer — `config.outputs.write_manifest` drives the
//!       `manifest.json` write inside `export_sector`; the toggle lives next to
//!       the format toggles.
//! §EX3  Bitmap settings — `sector_scale` / `system_scale` (1..=8),
//!       `render_systems`, `faction_fill`, `heatmap` mode picker, all editing
//!       `config.outputs.bitmap`.
//! §EX4  HTML settings — theme picker (dark | parchment | hololithic),
//!       `player_observer`, `player_min_confidence`, `size_warn_bytes`,
//!       `compact_json`, editing `config.outputs.html`.
//! §EX5  Standalone-system export (mirrors `generate-system`): coord + seed +
//!       index → write `<sys-id>.json` (+ optional `.md`) via
//!       [`sectorforge::generate_system_standalone`].
//! §EX6  "Render markdown" preview — live [`sectorforge::render_sector_markdown`]
//!       of the in-memory sector, mirroring `render-markdown`.
//! §EX7  Per-overlay export buttons — history, personae, hooks, prose,
//!       relations, regions, economy, interestingness, briefing, missions,
//!       sites, analyze. Each derives over the live sector (using the loaded
//!       catalog config, or its default) and writes `<name>.md` + `<name>.json`.
//! §EX8  "Export everything" runs the §EX1 bundle and every §EX7 overlay into
//!       the chosen folder in one click.
//!
//! Every knob in §EX1..§EX4 edits `config.outputs` directly so it round-trips
//! to `sectorforge.toml` on save and feeds `export_sector` unchanged; the
//! transient bits (output folder, standalone-system form, markdown preview)
//! live on [`crate::builder::export_run::ExportState`].

use camino::{Utf8Path, Utf8PathBuf};
use egui::{Color32, RichText, Ui};

use sectorforge::briefing::{self, AudiencePreset};
use sectorforge::config::{HtmlTheme, OutputFormat};
use sectorforge::heatmap::HeatmapMode;
use sectorforge::sector_model::HexCoord;
use sectorforge::SectorError;
use sectorforge_gui_core::ui_kit;

use crate::builder::export_run::ExportJobResult;
use crate::builder::{BuilderState, ModalKind};

const HTML_THEMES: &[HtmlTheme] = &[HtmlTheme::Dark, HtmlTheme::Parchment, HtmlTheme::Hololithic];

/// §EX1 toggleable bundle formats, in canonical emission order.
const FORMATS: &[(OutputFormat, &str)] = &[
    (OutputFormat::Json, "json"),
    (OutputFormat::Markdown, "markdown"),
    (OutputFormat::Bitmap, "bitmap (png)"),
    (OutputFormat::Svg, "svg"),
    (OutputFormat::Html, "html"),
];

/// §EX7 per-overlay writers, mirroring the CLI subcommands of the same name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    History,
    Personae,
    Hooks,
    Prose,
    Relations,
    Regions,
    Economy,
    Interestingness,
    Briefing,
    Missions,
    Sites,
    Analyze,
}

const OVERLAYS: &[Overlay] = &[
    Overlay::History,
    Overlay::Personae,
    Overlay::Hooks,
    Overlay::Prose,
    Overlay::Relations,
    Overlay::Regions,
    Overlay::Economy,
    Overlay::Interestingness,
    Overlay::Briefing,
    Overlay::Missions,
    Overlay::Sites,
    Overlay::Analyze,
];

impl Overlay {
    fn label(self) -> &'static str {
        match self {
            Overlay::History => "history",
            Overlay::Personae => "personae",
            Overlay::Hooks => "hooks",
            Overlay::Prose => "prose (gazetteer)",
            Overlay::Relations => "relations",
            Overlay::Regions => "regions",
            Overlay::Economy => "economy",
            Overlay::Interestingness => "interestingness",
            Overlay::Briefing => "briefing",
            Overlay::Missions => "missions",
            Overlay::Sites => "sites",
            Overlay::Analyze => "analyze",
        }
    }

    /// Base name(s) the writer drops into the output folder (for the success
    /// message). Each overlay writes `<base>.md` + `<base>.json`.
    fn artefact(self) -> &'static str {
        match self {
            Overlay::History => "history",
            Overlay::Personae => "personae",
            Overlay::Hooks => "hooks",
            Overlay::Prose => "gazetteer",
            Overlay::Relations => "relations",
            Overlay::Regions => "regions",
            Overlay::Economy => "economy",
            Overlay::Interestingness => "interestingness",
            Overlay::Briefing => "briefing-gm_full_truth",
            Overlay::Missions => "missions",
            Overlay::Sites => "sites",
            Overlay::Analyze => "analysis",
        }
    }
}

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Export");
    ui.label(
        RichText::new("§EX1..§EX8 — bundle formats, manifest, bitmap/HTML settings, per-overlay writers, standalone-system + markdown preview.")
            .small()
            .color(Color32::GRAY),
    );
    ui.separator();

    // Drain the async bundle-export worker (mirrors the RANDOM / SEARCH
    // runtimes) and surface its outcome as a completion modal / error.
    if let Some(result) = state.export.pump() {
        match result {
            ExportJobResult::Done(msg) => {
                state.export.error = None;
                state.modal = Some(ModalKind::Message(msg));
            }
            ExportJobResult::Failed(msg) => state.export.error = Some(msg),
        }
    }

    egui::ScrollArea::vertical()
        .id_salt("export_root_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_folder_row(ui, state);
            ui.separator();
            show_formats(ui, state);
            ui.separator();
            show_bitmap_settings(ui, state);
            ui.separator();
            show_html_settings(ui, state);
            ui.separator();
            show_overlays(ui, state);
            ui.separator();
            show_standalone_system(ui, state);
            ui.separator();
            show_markdown_preview(ui, state);

            if let Some(err) = state.export.error.as_ref() {
                ui.add_space(4.0);
                ui.colored_label(Color32::from_rgb(220, 120, 110), err);
            }
        });
}

// ── output folder + §EX1 bundle / §EX8 everything ──────────────────────────

fn show_folder_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Choose output folder…").clicked() {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Choose export output folder")
                .pick_folder()
            {
                if let Ok(path) = Utf8PathBuf::from_path_buf(folder) {
                    state.export.output_dir = Some(path);
                }
            }
        }
        let has_dir = state.export.output_dir.is_some();
        let busy = state.export.is_running();
        if ui
            .add_enabled(has_dir && !busy, egui::Button::new("Export bundle (§EX1)"))
            .on_hover_text("Write every enabled format (+ manifest) for the live sector.")
            .clicked()
        {
            run_bundle(state, ui.ctx());
        }
        if ui
            .add_enabled(
                has_dir && !busy,
                egui::Button::new(RichText::new("Export everything (§EX8)").strong()),
            )
            .on_hover_text("Run the bundle and every per-overlay writer in one pass.")
            .clicked()
        {
            run_everything(state, ui.ctx());
        }
    });
    let dir_label = state
        .export
        .output_dir
        .as_ref()
        .map(Utf8PathBuf::to_string)
        .unwrap_or_else(|| "(no output folder picked)".to_string());
    ui.colored_label(Color32::DARK_GRAY, dir_label);

    // Live progress for the off-thread bundle export (the big sector.json write).
    if state.export.is_running() {
        let frac = state.export.fraction();
        ui.add(egui::ProgressBar::new(frac).show_percentage());
        if let Some(status) = state.export.status_text() {
            ui.colored_label(Color32::GRAY, status);
        }
        if ui.button("■ Cancel").clicked() {
            state.export.cancel();
        }
    }
}

// ── §EX1 formats + §EX2 manifest ────────────────────────────────────────────

fn show_formats(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "ex_formats",
        "§EX1 / §EX2 — formats + manifest",
        true,
        |ui| {
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                let formats = &mut state.config.outputs.formats;
                for (fmt, label) in FORMATS {
                    let mut on = formats.contains(fmt);
                    if ui.checkbox(&mut on, *label).changed() {
                        if on {
                            if !formats.contains(fmt) {
                                formats.push(*fmt);
                            }
                        } else {
                            formats.retain(|f| f != fmt);
                        }
                        changed = true;
                    }
                }
            });
            changed |= ui
                .checkbox(
                    &mut state.config.outputs.write_manifest,
                    "write manifest.json (§EX2)",
                )
                .on_hover_text(
                    "Records the generator version + input-catalog digests next to the sector.",
                )
                .changed();
            changed |= ui
                .checkbox(&mut state.config.outputs.pretty_json, "pretty JSON")
                .changed();
            changed |= ui
                .checkbox(
                    &mut state.config.outputs.write_per_system_files,
                    "write per-system JSON files",
                )
                .changed();
            if changed {
                state.dirty = true;
            }
        },
    );
}

// ── §EX3 bitmap settings ────────────────────────────────────────────────────

fn show_bitmap_settings(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "ex_bitmap", "§EX3 — bitmap settings", false, |ui| {
        let mut changed = false;
        let bm = &mut state.config.outputs.bitmap;
        egui::Grid::new("export_bitmap_grid")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label("sector_scale")
                    .on_hover_text("Integer multiplier over the base sector map size (1..=8).");
                changed |= ui
                    .add(egui::DragValue::new(&mut bm.sector_scale).range(1..=8))
                    .changed();
                ui.end_row();

                ui.label("system_scale")
                    .on_hover_text("Integer multiplier for per-system maps (1..=8).");
                changed |= ui
                    .add(egui::DragValue::new(&mut bm.system_scale).range(1..=8))
                    .changed();
                ui.end_row();

                ui.label("render_systems");
                changed |= ui.checkbox(&mut bm.render_systems, "").changed();
                ui.end_row();

                ui.label("faction_fill");
                changed |= ui.checkbox(&mut bm.faction_fill, "").changed();
                ui.end_row();

                ui.label("heatmap");
                let prev = bm.heatmap;
                ui_kit::combo("export_bitmap_heatmap", bm.heatmap.label()).show_ui(ui, |ui| {
                    for mode in HeatmapMode::ALL {
                        ui.selectable_value(&mut bm.heatmap, *mode, mode.label());
                    }
                });
                changed |= bm.heatmap != prev;
                ui.end_row();
            });
        if changed {
            state.dirty = true;
        }
    });
}

// ── §EX4 HTML settings ──────────────────────────────────────────────────────

fn show_html_settings(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "ex_html", "§EX4 — HTML settings", false, |ui| {
        let mut changed = false;
        // Observer combo reads the faction roster, so collect ids first to
        // avoid borrowing `state.sector` while `state.config.html` is &mut.
        let factions: Vec<(String, String)> = state
            .sector
            .factions
            .iter()
            .map(|f| (f.id.to_string(), format!("{} ({})", f.name, f.id)))
            .collect();
        let html = &mut state.config.outputs.html;
        egui::Grid::new("export_html_grid")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label("theme");
                let prev = html.theme;
                ui_kit::combo("export_html_theme", html.theme.as_slug()).show_ui(ui, |ui| {
                    for t in HTML_THEMES {
                        ui.selectable_value(&mut html.theme, *t, t.as_slug());
                    }
                });
                changed |= html.theme != prev;
                ui.end_row();

                ui.label("player_observer")
                    .on_hover_text("Restrict the inlined sector to what this faction can see. (none) = full GM edition.");
                let prev = html.player_observer.clone();
                let selected = html
                    .player_observer
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string());
                ui_kit::combo("export_html_observer", selected).show_ui(ui, |ui| {
                    ui.selectable_value(&mut html.player_observer, None, "(none)");
                    for (id, label) in &factions {
                        ui.selectable_value(
                            &mut html.player_observer,
                            Some(id.clone()),
                            label,
                        );
                    }
                });
                changed |= html.player_observer != prev;
                ui.end_row();

                ui.label("player_min_confidence");
                changed |= ui
                    .add(egui::Slider::new(&mut html.player_min_confidence, 0..=100))
                    .changed();
                ui.end_row();

                ui.label("size_warn_bytes");
                changed |= ui
                    .add(egui::DragValue::new(&mut html.size_warn_bytes).speed(1024.0))
                    .changed();
                ui.end_row();

                ui.label("compact_json");
                changed |= ui.checkbox(&mut html.compact_json, "").changed();
                ui.end_row();
            });
        if changed {
            state.dirty = true;
        }
    });
}

// ── §EX7 per-overlay export ─────────────────────────────────────────────────

fn show_overlays(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "ex_overlays",
        "§EX7 — per-overlay export",
        true,
        |ui| {
            let has_dir = state.export.output_dir.is_some();
            if !has_dir {
                ui.colored_label(Color32::GRAY, "Pick an output folder first.");
            }
            let mut clicked: Option<Overlay> = None;
            ui.horizontal_wrapped(|ui| {
                for overlay in OVERLAYS {
                    if ui
                        .add_enabled(has_dir, egui::Button::new(overlay.label()))
                        .clicked()
                    {
                        clicked = Some(*overlay);
                    }
                }
            });
            if let Some(overlay) = clicked {
                run_overlay(state, overlay);
            }
        },
    );
}

// ── §EX5 standalone-system export ───────────────────────────────────────────

fn show_standalone_system(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "ex_standalone",
        "§EX5 — standalone-system export",
        false,
        |ui| {
            ui.colored_label(
            Color32::DARK_GRAY,
            "Mirrors `generate-system`: generates one system from the project on disk and writes <sys-id>.json.",
        );
            egui::Grid::new("export_sys_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.label("seed (blank = project seed)");
                    ui.text_edit_singleline(&mut state.export.sys_seed);
                    ui.end_row();

                    ui.label("coord q");
                    ui.add(egui::DragValue::new(&mut state.export.sys_coord_q));
                    ui.end_row();

                    ui.label("coord r");
                    ui.add(egui::DragValue::new(&mut state.export.sys_coord_r));
                    ui.end_row();

                    ui.label("index (>= 1)");
                    ui.add(egui::DragValue::new(&mut state.export.sys_index).range(1..=usize::MAX));
                    ui.end_row();

                    ui.label("also write .md");
                    ui.checkbox(&mut state.export.sys_markdown, "");
                    ui.end_row();
                });

            let has_project = state.project_path.is_some();
            let has_dir = state.export.output_dir.is_some();
            if !has_project {
                ui.colored_label(
                Color32::GRAY,
                "Open a project on disk first — standalone generation reads the project's catalogs.",
            );
            }
            if ui
                .add_enabled(
                    has_project && has_dir,
                    egui::Button::new("Generate + write system"),
                )
                .clicked()
            {
                run_standalone_system(state);
            }
        },
    );
}

// ── §EX6 markdown preview ───────────────────────────────────────────────────

fn show_markdown_preview(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "ex_md_preview",
        "§EX6 — render markdown preview",
        false,
        |ui| {
            ui.horizontal(|ui| {
                if ui.button("Refresh preview").clicked() {
                    state.export.md_preview =
                        Some(sectorforge::render_sector_markdown(&state.sector));
                }
                if state.export.md_preview.is_some() {
                    ui.colored_label(Color32::DARK_GRAY, "live render of the in-memory sector");
                }
            });
            let Some(md) = state.export.md_preview.as_ref() else {
                ui.colored_label(Color32::GRAY, "No preview yet — click \"Refresh preview\".");
                return;
            };
            egui::ScrollArea::both()
                .id_salt("export_md_preview_scroll")
                .max_height(360.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut md.as_str())
                            .desired_width(f32::INFINITY)
                            .desired_rows(20)
                            .font(egui::TextStyle::Monospace),
                    );
                });
        },
    );
}

// ── actions ─────────────────────────────────────────────────────────────────

fn run_bundle(state: &mut BuilderState, ctx: &egui::Context) {
    let Some(dir) = state.export.output_dir.clone() else {
        return;
    };
    // §V6 — refuse to export a sector carrying validation errors or invariant
    // violations (parity with the `sectorforge generate` refuse-on-error gate).
    if let Some(reason) = state.export_block_reason() {
        state.export.error = Some(reason);
        return;
    }
    let formats = formats_label(state);
    let summary = format!("Wrote bundle ({formats}) to {dir}");
    let sector = state.sector.share();
    let outputs = state.config.outputs.clone();
    state
        .export
        .spawn_bundle(ctx, sector, outputs, dir, summary);
}

/// Comma-joined slug list of the project's enabled bundle formats, for status
/// / completion messages.
fn formats_label(state: &BuilderState) -> String {
    state
        .config
        .outputs
        .formats
        .iter()
        .map(OutputFormat::as_slug)
        .collect::<Vec<_>>()
        .join(", ")
}

fn run_overlay(state: &mut BuilderState, overlay: Overlay) {
    let Some(dir) = state.export.output_dir.clone() else {
        return;
    };
    match write_overlay(state, dir.as_path(), overlay) {
        Ok(()) => {
            state.export.error = None;
            state.modal = Some(ModalKind::Message(format!(
                "Wrote {0}.md + {0}.json to {dir}",
                overlay.artefact()
            )));
        }
        Err(e) => {
            state.export.error = Some(format!("{} export failed: {e}", overlay.label()));
        }
    }
}

fn run_everything(state: &mut BuilderState, ctx: &egui::Context) {
    let Some(dir) = state.export.output_dir.clone() else {
        return;
    };
    // §V6 — same refuse-on-error gate as the §EX1 bundle: do not write a full
    // export (bundle + every overlay) for an invalid sector.
    if let Some(reason) = state.export_block_reason() {
        state.export.error = Some(reason);
        return;
    }
    // The per-overlay writers are small and read `state`, so run them inline on
    // the UI thread; only the heavy sector bundle goes off-thread (with live
    // progress) via `spawn_bundle` below.
    let mut first_error: Option<String> = None;
    let mut written = 0u32;
    for overlay in OVERLAYS {
        match write_overlay(state, dir.as_path(), *overlay) {
            Ok(()) => written += 1,
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(format!("{}: {e}", overlay.label()));
                }
            }
        }
    }

    let formats = formats_label(state);
    let overlays_note = match &first_error {
        None => format!("{written} overlay group(s)"),
        Some(err) => format!("{written} overlay group(s); first overlay failure — {err}"),
    };
    let summary = format!("Exported everything ({overlays_note} + bundle: {formats}) to {dir}");
    let sector = state.sector.share();
    let outputs = state.config.outputs.clone();
    state
        .export
        .spawn_bundle(ctx, sector, outputs, dir, summary);
}

/// Derive the requested overlay over the live sector (using the loaded catalog
/// config, or its default) and write its `.md` + `.json` into `dir`. Reads
/// `state` immutably so callers can mutate messaging fields afterwards.
fn write_overlay(
    state: &BuilderState,
    dir: &Utf8Path,
    overlay: Overlay,
) -> Result<(), SectorError> {
    let sector = &state.sector;
    let cats = &state.data_catalogs;
    match overlay {
        Overlay::History => {
            let mut cfg = cats
                .history
                .clone()
                .unwrap_or_else(|| state.config.history.clone());
            cfg.enabled = true;
            let report = sectorforge::derive_history_with(sector, &cfg);
            sectorforge::write_history(dir, &report, &cfg)
        }
        Overlay::Personae => {
            let cfg = cats.personae.clone().unwrap_or_default();
            let report = sectorforge::derive_personae_with(sector, &cfg);
            sectorforge::write_personae(dir, &report)
        }
        Overlay::Hooks => {
            let cfg = cats.hooks.clone().unwrap_or_default();
            let report = sectorforge::derive_hooks_with(sector, &cfg);
            sectorforge::write_hooks(dir, &report, &cfg)
        }
        Overlay::Prose => {
            let cfg = cats.prose.clone().unwrap_or_default();
            let report = sectorforge::derive_prose_with(sector, &cfg);
            sectorforge::write_prose(dir, &report)
        }
        Overlay::Relations => {
            let report = sectorforge::relations::RelationsReport {
                sector_id: sector.id.to_string(),
                seed: sector.seed.to_string(),
                matrix: (*sector.relations).clone(),
            };
            sectorforge::write_relations(dir, &report)
        }
        Overlay::Regions => sectorforge::write_regions(dir, &sector.id, &sector.regions),
        Overlay::Economy => sectorforge::write_economy(dir, &sector.id, &sector.economy),
        Overlay::Interestingness => {
            let cfg = sectorforge::interestingness::InterestingnessConfig::default();
            let report = sectorforge::derive_interestingness_with(sector, &cfg);
            sectorforge::write_interestingness(dir, &report)
        }
        Overlay::Briefing => {
            let profile = briefing::preset(AudiencePreset::GmFullTruth);
            let pack = sectorforge::apply_briefing(sector, &profile);
            sectorforge::write_briefing(dir, &pack, &profile)
        }
        Overlay::Missions => {
            let cfg = cats.missions.clone().unwrap_or_default();
            let report = sectorforge::derive_missions_with(sector, &cfg);
            sectorforge::write_missions(dir, &report, &cfg)
        }
        Overlay::Sites => {
            let cfg = cats.sites.clone().unwrap_or_default();
            let report = sectorforge::derive_sites_with(sector, &cfg);
            sectorforge::write_sites(dir, &report, &cfg)
        }
        Overlay::Analyze => {
            let report = sectorforge::analyze_sector_with(sector, &state.config.analyze);
            sectorforge::write_analysis(dir, &report)
        }
    }
}

fn run_standalone_system(state: &mut BuilderState) {
    let Some(project) = state.project_path.clone() else {
        state.export.error = Some("No project open on disk.".to_string());
        return;
    };
    let Some(dir) = state.export.output_dir.clone() else {
        return;
    };
    match generate_and_write_system(state, project.as_path(), dir.as_path()) {
        Ok(id) => {
            state.export.error = None;
            let suffix = if state.export.sys_markdown {
                format!("{id}.json + {id}.md")
            } else {
                format!("{id}.json")
            };
            state.modal = Some(ModalKind::Message(format!("Wrote {suffix} to {dir}")));
        }
        Err(e) => {
            state.export.error = Some(format!("Standalone-system export failed: {e}"));
        }
    }
}

/// Load the project on disk, generate one standalone system (CLI parity with
/// `generate-system`), and write `<id>.json` (+ optional `.md`) into `dir`.
/// Returns the generated system id for the success message.
fn generate_and_write_system(
    state: &BuilderState,
    project: &Utf8Path,
    dir: &Utf8Path,
) -> Result<String, SectorError> {
    let mut input = sectorforge::load_project(project)?;
    let seed = state.export.sys_seed.trim();
    if !seed.is_empty() {
        input.config.generation.seed = seed.to_string();
    }
    let coord = HexCoord {
        q: state.export.sys_coord_q,
        r: state.export.sys_coord_r,
    };
    let system = sectorforge::generate_system_standalone(input, state.export.sys_index, coord)?;
    let id = system.id.to_string();
    let json_path = dir.join(format!("{id}.json"));
    sectorforge::write_system_json(&json_path, &system)?;
    if state.export.sys_markdown {
        let md = sectorforge::render_system_markdown(&system);
        let md_path = dir.join(format!("{id}.md"));
        std::fs::write(&md_path, md).map_err(|e| SectorError::io(md_path.as_str(), e))?;
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn blank_state() -> BuilderState {
        BuilderState::new_blank("ex", "Export Test", "seed-ex", 6, 4)
    }

    #[test]
    fn overlay_list_has_twelve_distinct_entries() {
        assert_eq!(OVERLAYS.len(), 12);
        for (i, a) in OVERLAYS.iter().enumerate() {
            for b in &OVERLAYS[i + 1..] {
                assert_ne!(a.label(), b.label());
            }
        }
    }

    #[test]
    fn write_overlay_emits_md_and_json_for_every_overlay() {
        let state = blank_state();
        let tmp = tempdir().unwrap();
        let dir = Utf8Path::from_path(tmp.path()).unwrap();
        for overlay in OVERLAYS {
            write_overlay(&state, dir, *overlay).expect("overlay export should succeed");
            let base = overlay.artefact();
            assert!(
                dir.join(format!("{base}.json")).exists(),
                "{base}.json missing for {}",
                overlay.label()
            );
            assert!(
                dir.join(format!("{base}.md")).exists(),
                "{base}.md missing for {}",
                overlay.label()
            );
        }
    }

    #[test]
    fn bundle_writes_enabled_formats() {
        let mut state = blank_state();
        state.config.outputs.formats = vec![OutputFormat::Json, OutputFormat::Markdown];
        state.config.outputs.write_manifest = true;
        let tmp = tempdir().unwrap();
        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        state.export.output_dir = Some(dir.clone());
        let ctx = egui::Context::default();
        run_bundle(&mut state, &ctx);
        // Export now runs off-thread; block on the worker before asserting.
        let result = state
            .export
            .job
            .as_ref()
            .expect("bundle export job should be in flight")
            .receiver
            .recv()
            .expect("export worker should post a result");
        assert!(
            matches!(result, ExportJobResult::Done(_)),
            "expected a successful bundle export"
        );
        assert!(dir.join("sector.json").exists());
        assert!(dir.join("sector.md").exists());
        assert!(dir.join("manifest.json").exists());
    }
}
