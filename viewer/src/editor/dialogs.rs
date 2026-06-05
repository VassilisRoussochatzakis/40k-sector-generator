//! Modal dialogs for the editor. Each dialog reads/mutates state in place,
//! returns a side-effect when confirmed.

use egui::{Context, RichText};

use super::state::{Dialog, EditorState};
use super::ui_helpers::{combo_str, label, section, text_field};

pub(crate) fn draw_dialog(ctx: &Context, state: &mut EditorState) {
    // §BEAUTY §6.7: dim + inert the page behind any open dialog. Called every
    // frame so the scrim animates out on close.
    let _ = sectorforge_gui_core::modal::scrim(ctx, !matches!(state.dialog, Dialog::None));
    let dialog = std::mem::replace(&mut state.dialog, Dialog::None);
    match dialog {
        Dialog::None => {}
        Dialog::Message(msg) => {
            let mut open = true;
            egui::Window::new(RichText::new("INFO"))
                // Foreground: sit above the Order::Middle scrim — see modal::scrim.
                .order(egui::Order::Foreground)
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    label(ui, &msg);
                    if ui.button(RichText::new("OK")).clicked() {
                        // closes via open=false next frame
                    }
                });
            if open {
                state.dialog = Dialog::Message(msg);
            }
        }
        Dialog::OpenProject {
            mut projects,
            mut selected,
        } => {
            let mut keep = true;
            let mut load: Option<String> = None;
            let mut cancel = false;
            egui::Window::new(RichText::new("OPEN PROJECT"))
                // Foreground: sit above the Order::Middle scrim — see modal::scrim.
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    if projects.is_empty() {
                        label(ui, "no projects with out/sector.json under examples/");
                    } else {
                        section(ui, "PROJECT");
                        let current = selected.clone().unwrap_or_default();
                        let mut current_s = current;
                        let opts: Vec<&str> = projects.iter().map(|s| s.as_str()).collect();
                        if combo_str(ui, "open_project_combo", &mut current_s, &opts) {
                            selected = Some(current_s);
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                selected.is_some(),
                                egui::Button::new(RichText::new("LOAD")),
                            )
                            .clicked()
                        {
                            load = selected.clone();
                        }
                        if ui.button(RichText::new("REFRESH")).clicked() {
                            projects = super::file_ops::list_projects();
                        }
                        if ui.button(RichText::new("CANCEL")).clicked() {
                            cancel = true;
                        }
                    });
                });
            if let Some(name) = load {
                match super::file_ops::load_project_sector(&name) {
                    Ok((sec, input, path)) => state.set_sector(sec, input, Some(path)),
                    Err(e) => state.dialog = Dialog::Message(e.to_string()),
                }
                keep = false;
            }
            if cancel {
                keep = false;
            }
            if keep && matches!(state.dialog, Dialog::None) {
                state.dialog = Dialog::OpenProject { projects, selected };
            }
        }
        Dialog::NewSector {
            mut name,
            mut title,
            mut seed,
            mut width,
            mut height,
            mut irregular_dimensions,
        } => {
            let mut create = false;
            let mut cancel = false;
            egui::Window::new(RichText::new("NEW SECTOR"))
                // Foreground: sit above the Order::Middle scrim — see modal::scrim.
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    section(ui, "ID (folder name)");
                    text_field(ui, &mut name, "my-sector");
                    section(ui, "TITLE");
                    text_field(ui, &mut title, "My Sector");
                    section(ui, "SEED");
                    text_field(ui, &mut seed, "seed");

                    ui.add_space(8.0);
                    ui.checkbox(&mut irregular_dimensions, "Irregular dimensions");

                    if irregular_dimensions {
                        ui.colored_label(
                            crate::palette::warning(),
                            "⚠ abnormal dimensions can cause problems in segmenta or joining sectors",
                        );
                    }

                    ui.horizontal(|ui| {
                        label(ui, "WIDTH");
                        let w_res = ui.add(egui::DragValue::new(&mut width).range(1..=64));
                        label(ui, "HEIGHT");
                        let h_res = ui.add(egui::DragValue::new(&mut height).range(1..=64));

                        if !irregular_dimensions {
                            if w_res.changed() {
                                height = width;
                            } else if h_res.changed() {
                                width = height;
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !name.trim().is_empty(),
                                egui::Button::new(RichText::new("CREATE")),
                            )
                            .clicked()
                        {
                            create = true;
                        }
                        if ui
                            .button(RichText::new("CANCEL"))
                            .clicked()
                        {
                            cancel = true;
                        }
                    });
                });
            if create {
                let display_title = if title.trim().is_empty() {
                    name.clone()
                } else {
                    title
                };
                let sector = sectorforge::sector_model::empty_sector(
                    name.trim(),
                    display_title.trim(),
                    seed.trim(),
                    width,
                    height,
                );
                state.set_sector(sector, None, None);
                state.mark_dirty();
            } else if !cancel {
                state.dialog = Dialog::NewSector {
                    name,
                    title,
                    seed,
                    width,
                    height,
                    irregular_dimensions,
                };
            }
        }
        Dialog::SaveAs { mut name, error } => {
            let mut save = false;
            let mut cancel = false;
            egui::Window::new(RichText::new("SAVE AS"))
                // Foreground: sit above the Order::Middle scrim — see modal::scrim.
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    section(ui, "PROJECT NAME");
                    text_field(ui, &mut name, "my-sector");
                    label(ui, "→ examples/<name>/out/sector.json");
                    if let Some(e) = error.as_ref() {
                        ui.colored_label(crate::palette::danger(), e);
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !name.trim().is_empty(),
                                egui::Button::new(RichText::new("SAVE")),
                            )
                            .clicked()
                        {
                            save = true;
                        }
                        if ui.button(RichText::new("CANCEL")).clicked() {
                            cancel = true;
                        }
                    });
                });
            if save {
                if let Some(sector) = state.sector.as_mut() {
                    refresh_manifest_counts(sector, name.trim());
                    let sec_snapshot = sector.clone();
                    match super::file_ops::save_project_sector(name.trim(), &sec_snapshot) {
                        Ok(path) => {
                            state.loaded_from = Some(path);
                            state.dirty = false;
                            state.dialog = Dialog::Message(format!("saved {}", name.trim()));
                        }
                        Err(e) => {
                            state.dialog = Dialog::SaveAs {
                                name: name.to_string(),
                                error: Some(e.to_string()),
                            };
                        }
                    }
                } else {
                    state.dialog = Dialog::Message("no sector to save".into());
                }
            } else if !cancel {
                state.dialog = Dialog::SaveAs { name, error };
            }
        }
        Dialog::PlaceSystem {
            coord,
            mut name,
            mut kind,
            mut has_star,
        } => {
            let mut create = false;
            let mut cancel = false;
            egui::Window::new(RichText::new("ADD SYSTEM"))
                // Foreground: sit above the Order::Middle scrim — see modal::scrim.
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    label(ui, &format!("AT Q{:+} R{:+}", coord.q, coord.r));
                    section(ui, "NAME");
                    text_field(ui, &mut name, "system name");

                    ui.add_space(6.0);
                    ui.checkbox(&mut has_star, "HAS STAR");

                    section(ui, "KIND");
                    let mut kind_s = format!("{kind:?}");
                    let kinds = [
                        "Star",
                        "SpecialLocation",
                        "BlackHole",
                        "WarpAnomaly",
                        "SpaceStation",
                    ];
                    if combo_str(ui, "place_sys_kind", &mut kind_s, &kinds) {
                        kind = match kind_s.as_str() {
                            "Star" => sectorforge::sector_model::SystemKind::Star,
                            "SpecialLocation" => {
                                sectorforge::sector_model::SystemKind::SpecialLocation
                            }
                            "BlackHole" => sectorforge::sector_model::SystemKind::BlackHole,
                            "WarpAnomaly" => sectorforge::sector_model::SystemKind::WarpAnomaly,
                            "SpaceStation" => sectorforge::sector_model::SystemKind::SpaceStation,
                            _ => sectorforge::sector_model::SystemKind::Star,
                        };
                        if kind == sectorforge::sector_model::SystemKind::Star {
                            has_star = true;
                        }
                    }

                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !name.trim().is_empty(),
                                egui::Button::new(RichText::new("CREATE")),
                            )
                            .clicked()
                        {
                            create = true;
                        }
                        if ui.button(RichText::new("CANCEL")).clicked() {
                            cancel = true;
                        }
                    });
                });
            if create {
                let index = state.next_system_index();
                let id = sectorforge::ids::system_id(index);
                let star = if has_star {
                    Some(sectorforge::sector_model::GeneratedStar {
                        colour_code: "W".into(),
                        colour_name: "white".into(),
                        spectral_type: Some("W".into()),
                        source_row_index: None,
                    })
                } else {
                    None
                };
                let sys = sectorforge::sector_model::empty_system(
                    id.clone(),
                    index,
                    name.trim().to_string(),
                    coord,
                    kind,
                    star,
                );
                if let Some(sec) = state.sector.as_mut() {
                    sec.systems.push(sys);
                    state.mark_dirty();
                    state.selection = super::state::Selection::System(id);
                }
            } else if !cancel {
                state.dialog = Dialog::PlaceSystem {
                    coord,
                    name,
                    kind,
                    has_star,
                };
            }
        }
    }
}

fn refresh_manifest_counts(sector: &mut sectorforge::sector_model::GeneratedSector, name: &str) {
    sector.id = name.into();
    sector.manifest.project_id = name.into();
    sector.manifest.system_count = sector.systems.len();
    sector.manifest.world_count = sector.systems.iter().map(|s| s.worlds.len()).sum();
    sector.manifest.route_count = sector.routes.len();
}
