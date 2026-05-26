//! Persistent-buffer wrappers around `Ui::text_edit_singleline` /
//! `text_edit_multiline`.
//!
//! Reason: egui 0.29's `TextEdit` mutates the `&mut String` in place from key
//! events but stores no internal text. When a panel re-derives the buffer
//! every frame from sector state (e.g. `let mut buf = sys.name.to_string()`),
//! typed characters are lost between frames because the next frame overwrites
//! the buffer before any commit fires. These helpers persist the in-flight
//! buffer in `ui.data_mut` keyed by a caller-supplied [`egui::Id`] so typing
//! survives until a `lost_focus()` / explicit-commit path consumes it.
//!
//! Pattern:
//! ```ignore
//! let key = egui::Id::new(("foo_name_buf", id.as_str()));
//! let (mut buf, resp) = persistent_singleline(ui, key, &source);
//! if resp.lost_focus() && buf != source {
//!     // ...commit...
//!     persistent_text_clear(ui, key);
//! }
//! ```

use egui::{Id, Response, Ui};

/// Single-line variant. Returns the live buffer (already written back to temp)
/// and the widget [`Response`].
pub fn persistent_singleline(ui: &mut Ui, key: Id, source: &str) -> (String, Response) {
    let mut buf: String = ui
        .data_mut(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| source.to_string());
    let resp = ui.text_edit_singleline(&mut buf);
    ui.data_mut(|d| d.insert_temp(key, buf.clone()));
    (buf, resp)
}

/// Multi-line variant.
pub fn persistent_multiline(ui: &mut Ui, key: Id, source: &str) -> (String, Response) {
    let mut buf: String = ui
        .data_mut(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| source.to_string());
    let resp = ui.text_edit_multiline(&mut buf);
    ui.data_mut(|d| d.insert_temp(key, buf.clone()));
    (buf, resp)
}

/// Drop the stored buffer so the next frame reseeds from `source`. Call after
/// committing the edit back to canonical state.
pub fn persistent_text_clear(ui: &mut Ui, key: Id) {
    ui.data_mut(|d| d.remove::<String>(key));
}
