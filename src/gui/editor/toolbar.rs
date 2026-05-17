//! Top toolbar for the editor: file ops + tab switcher.

use egui::{RichText, Ui};

use crate::gui::palette::{TEXT, TEXT_DIM};

use super::state::{Dialog, EditorState, Tab};
use super::ui_helpers::mono;

pub fn editor_toolbar(ui: &mut Ui, state: &mut EditorState) {
    ui.horizontal(|ui| {
        if ui
            .button(RichText::new("NEW SECTOR").font(mono(12.0)))
            .clicked()
        {
            state.dialog = Dialog::NewSector {
                name: String::new(),
                title: String::new(),
                seed: "manual".to_string(),
                width: 8,
                height: 10,
            };
        }
        if ui
            .button(RichText::new("OPEN").font(mono(12.0)))
            .clicked()
        {
            let projects = super::file_ops::list_projects();
            state.dialog = Dialog::OpenProject {
                selected: projects.first().cloned(),
                projects,
            };
        }
        let can_save = state.sector.is_some();
        if ui
            .add_enabled(can_save, egui::Button::new(RichText::new("SAVE AS").font(mono(12.0))))
            .clicked()
        {
            let default = state
                .sector
                .as_ref()
                .map(|s| s.id.clone())
                .unwrap_or_default();
            state.dialog = Dialog::SaveAs {
                name: default,
                error: None,
            };
        }

        ui.separator();

        for (tab, label) in [
            (Tab::Map, "MAP"),
            (Tab::Routes, "ROUTES"),
            (Tab::Factions, "FACTIONS"),
            (Tab::Settings, "SETTINGS"),
        ] {
            if ui
                .selectable_label(state.tab == tab, RichText::new(label).font(mono(12.0)).color(TEXT))
                .clicked()
            {
                state.tab = tab;
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(sec) = &state.sector {
                let dirty_marker = if state.dirty { " *" } else { "" };
                ui.label(
                    RichText::new(format!(
                        "{}{}  [{} sys {} routes {} factions]",
                        sec.id.to_uppercase(),
                        dirty_marker,
                        sec.systems.len(),
                        sec.routes.len(),
                        sec.factions.len(),
                    ))
                    .color(TEXT_DIM)
                    .font(mono(12.0)),
                );
            } else {
                ui.label(
                    RichText::new("NO SECTOR LOADED")
                        .color(TEXT_DIM)
                        .font(mono(12.0)),
                );
            }
        });
    });
}
