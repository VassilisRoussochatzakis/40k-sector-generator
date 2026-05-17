//! Small egui helpers shared across editor panels.

use egui::{ComboBox, FontId, Response, RichText, Ui};

use crate::gui::palette::{TEXT, TEXT_DIM};

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn section(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(TEXT).font(mono(13.0)).strong());
}

pub fn dim(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(TEXT_DIM).font(mono(12.0)));
}

pub fn label(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(TEXT).font(mono(12.0)));
}

/// Dropdown over `&[&str]`; writes selected value into `current`. Returns true
/// if the value changed.
pub fn combo_str(ui: &mut Ui, id: &str, current: &mut String, options: &[&str]) -> bool {
    let mut changed = false;
    ComboBox::from_id_salt(id)
        .selected_text(RichText::new(current.as_str()).font(mono(12.0)))
        .show_ui(ui, |ui| {
            for opt in options {
                if ui
                    .selectable_label(current == opt, RichText::new(*opt).font(mono(12.0)))
                    .clicked()
                {
                    if current != opt {
                        *current = (*opt).to_string();
                        changed = true;
                    }
                }
            }
        });
    changed
}

/// Dropdown over `(value, label)` tuples. `current` holds the value string;
/// label is what user sees.
pub fn combo_kv(ui: &mut Ui, id: &str, current: &mut String, options: &[(&str, &str)]) -> bool {
    let mut changed = false;
    let shown: &str = options
        .iter()
        .find(|(v, _)| *v == current.as_str())
        .map(|(_, l)| *l)
        .unwrap_or(current.as_str());
    ComboBox::from_id_salt(id)
        .selected_text(RichText::new(shown).font(mono(12.0)))
        .show_ui(ui, |ui| {
            for (v, l) in options {
                if ui
                    .selectable_label(current == v, RichText::new(*l).font(mono(12.0)))
                    .clicked()
                {
                    if current != v {
                        *current = (*v).to_string();
                        changed = true;
                    }
                }
            }
        });
    changed
}

pub fn text_field(ui: &mut Ui, value: &mut String, hint: &str) -> Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(hint)
            .font(mono(12.0)),
    )
}
