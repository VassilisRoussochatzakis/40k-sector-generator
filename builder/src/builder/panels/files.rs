//! §PF2 / §PF4 / §PF5: raw-TOML editor surface on the PROJECT tab.
//!
//! The §P4 project tree sets [`BuilderState::selected_file`] when the user
//! clicks a file. This panel turns that selection into an editable buffer: any
//! `.toml` file under the project root opens as an "editor tab" (§PF2). Each
//! tab keeps its own live buffer + dirty flag (§PF4) and validates on every
//! keystroke by parsing the text as a TOML document. Per-tab **Save** writes
//! exactly the user's bytes back via the atomic `tmp + rename` writer (§PF6),
//! while **Save all** (driven from the PROJECT button row, see
//! [`crate::builder::panels::project`]) flushes every dirty buffer and then
//! runs the full `save_project` pass (§PF5).
//!
//! Saving a config file also re-parses it into the in-memory catalog via
//! [`crate::builder::project_io::reload_catalog`] so the typed panels (FACTIONS,
//! REGIONS, the §PF3 world-data editor, …) stay in lock-step with hand edits.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, RichText, TextStyle};

use sectorforge_gui_core::{palette, ui_kit};

use crate::builder::project_io;
use crate::builder::state::{validate_toml, OpenTomlBuffer};
use crate::builder::{BuilderState, ModalKind};

/// True when `rel` names a TOML file the editor is willing to open.
fn is_toml(rel: &str) -> bool {
    rel.rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
        == Some("toml")
}

/// Leaf file name for a project-relative path (everything after the last `/`).
fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    if state.project_path.is_none() {
        ui_kit::placeholder(ui, "Open a project to edit its TOML files.");
        return;
    }

    ui.colored_label(
        palette::chrome_text_dim(),
        "Raw TOML editor. Click any .toml in the Project tree to open it as a tab; \
         edits are checked as you type and Save writes the file safely.",
    );
    ui.add_space(2.0);

    // §PF2: a fresh tree click opens (or re-focuses) the file as an editor tab.
    ensure_selected_open(state);

    if state.toml_editor.open.is_empty() {
        ui_kit::placeholder(
            ui,
            "No file open — click a .toml in the Project tree above to edit it.",
        );
        return;
    }

    tab_strip(ui, state);
    ui.separator();
    editor_body(ui, state);
}

/// §PF2: when [`BuilderState::selected_file`] points at a `.toml` that is not
/// yet open, read it from disk and install it as the active tab.
fn ensure_selected_open(state: &mut BuilderState) {
    let Some(sel) = state.selected_file.clone() else {
        return;
    };
    let rel = sel.as_str().to_string();
    if !is_toml(&rel) {
        return;
    }
    if state.toml_editor.open.contains_key(&rel) {
        state.toml_editor.active = Some(rel);
        return;
    }
    let Some(root) = state.project_path.clone() else {
        return;
    };
    let abs = root.join(&rel);
    if let Ok(text) = std::fs::read_to_string(abs.as_std_path()) {
        state
            .toml_editor
            .open
            .insert(rel.clone(), OpenTomlBuffer::new(text));
        state.toml_editor.active = Some(rel);
    }
}

/// §PF2 / §PF4: the strip of open-file tabs, each with a dirty `●` marker, an
/// invalid-TOML tint, and a close button.
fn tab_strip(ui: &mut egui::Ui, state: &mut BuilderState) {
    let active = state.toml_editor.active.clone();
    let rels: Vec<String> = state.toml_editor.open.keys().cloned().collect();
    let mut new_active: Option<String> = None;
    let mut to_close: Option<String> = None;

    ui.horizontal_wrapped(|ui| {
        for rel in &rels {
            let buf = &state.toml_editor.open[rel];
            let dirty = buf.is_dirty();
            let invalid = buf.error.is_some();
            let name = file_name(rel);
            let label = if dirty {
                format!("● {name}")
            } else {
                name.to_string()
            };
            let mut text = RichText::new(label).monospace();
            if invalid {
                text = text.color(palette::danger());
            } else if dirty {
                text = text.color(palette::warning());
            }
            let selected = active.as_deref() == Some(rel.as_str());
            if ui
                .selectable_label(selected, text)
                .on_hover_text(rel.as_str())
                .clicked()
            {
                new_active = Some(rel.clone());
            }
            if ui.small_button("×").on_hover_text("Close tab").clicked() {
                to_close = Some(rel.clone());
            }
        }
    });

    if let Some(rel) = new_active {
        state.toml_editor.active = Some(rel.clone());
        state.selected_file = Some(rel.into());
    }
    if let Some(rel) = to_close {
        close_tab(state, &rel);
    }
}

/// Drop a tab. Picks a neighbour as the new active tab and clears the tree
/// selection when it pointed at the closed file (so it does not reopen).
fn close_tab(state: &mut BuilderState, rel: &str) {
    state.toml_editor.open.remove(rel);
    if state.toml_editor.active.as_deref() == Some(rel) {
        state.toml_editor.active = state.toml_editor.open.keys().next().cloned();
    }
    if state.selected_file.as_deref().map(|p| p.as_str()) == Some(rel) {
        state.selected_file = state.toml_editor.active.clone().map(Into::into);
    }
}

/// §PF2 / §PF4 / §PF5: header (path + Save / Revert + validity pip), the
/// validation message, and the syntax-highlighted multiline editor.
fn editor_body(ui: &mut egui::Ui, state: &mut BuilderState) {
    let Some(active) = state.toml_editor.active.clone() else {
        return;
    };
    let (dirty, error_msg) = {
        let b = &state.toml_editor.open[&active];
        (b.is_dirty(), b.error.clone())
    };

    let mut do_save = false;
    let mut do_revert = false;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(&active)
                .monospace()
                .color(palette::chrome_text_dim()),
        );
        if ui
            .add_enabled(dirty, egui::Button::new("💾  Save"))
            .on_hover_text("Save this file and refresh the editors that read it")
            .clicked()
        {
            do_save = true;
        }
        if ui
            .add_enabled(dirty, egui::Button::new("↩  Revert"))
            .on_hover_text("Discard edits and restore the file as it is on disk")
            .clicked()
        {
            do_revert = true;
        }
        match &error_msg {
            None => ui.colored_label(palette::success(), "✓ valid TOML"),
            Some(_) => ui.colored_label(palette::danger(), "✗ parse error"),
        };
    });
    if let Some(msg) = &error_msg {
        ui.colored_label(palette::danger(), RichText::new(msg).monospace());
    }

    let style = ui.style().clone();
    let mut layouter = move |ui: &egui::Ui, text: &str, wrap_width: f32| {
        let mut job = highlight(text, &style);
        job.wrap.max_width = wrap_width;
        ui.fonts(|f| f.layout_job(job))
    };

    // §COLUMNS: capped height so the editor, when nested inside the PROJECT
    // tab's outer page scroll, is a finite box that scrolls internally rather
    // than expanding to infinite height and overlapping the sections below.
    let resp = egui::ScrollArea::vertical()
        .id_salt(("toml_editor_scroll", active.as_str()))
        .auto_shrink([false, false])
        .max_height(420.0)
        .show(ui, |ui| {
            let b = state
                .toml_editor
                .open
                .get_mut(&active)
                .expect("active tab present");
            ui.add(
                egui::TextEdit::multiline(&mut b.buffer)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(22)
                    .layouter(&mut layouter),
            )
        })
        .inner;

    if resp.changed() {
        if let Some(b) = state.toml_editor.open.get_mut(&active) {
            b.revalidate();
        }
        state.dirty_files.insert(active.clone());
    }
    if do_revert {
        if let Some(b) = state.toml_editor.open.get_mut(&active) {
            b.buffer = b.original.clone();
            b.revalidate();
        }
        state.dirty_files.remove(&active);
    }
    if do_save {
        if let Err(e) = save_one(state, &active) {
            state.modal = Some(ModalKind::Message(format!("Save failed: {e}")));
        }
    }
}

/// §PF5 / §PF6: write one open buffer back to disk atomically, then reflect it
/// into the in-memory catalog. Refuses to write a file that does not parse so a
/// broken config never lands on disk.
pub fn save_one(state: &mut BuilderState, rel: &str) -> Result<(), String> {
    let root = state
        .project_path
        .clone()
        .ok_or_else(|| "no project open".to_string())?;
    let text = state
        .toml_editor
        .open
        .get(rel)
        .map(|b| b.buffer.clone())
        .ok_or_else(|| format!("{rel} is not open"))?;
    if let Some(msg) = validate_toml(&text) {
        return Err(format!("invalid TOML — fix before saving: {msg}"));
    }

    let abs = root.join(rel);
    project_io::atomic_write(&abs, text.as_bytes()).map_err(|e| e.to_string())?;

    // Keep the typed catalogs in sync with the hand edit. A reload failure is
    // non-fatal (the bytes are already safely on disk) but is surfaced in the
    // status bar so a mismatch never passes silently.
    if let Err(e) = project_io::reload_catalog(state, rel, &text) {
        state.last_catalog_error = Some(e.to_string());
    }
    if let Some(b) = state.toml_editor.open.get_mut(rel) {
        b.mark_saved();
        b.revalidate();
    }
    state.dirty_files.remove(rel);
    project_io::refresh_watcher_baseline(state);
    Ok(())
}

/// §PF5: "Save all". Flushes every dirty editor buffer (validating each), then
/// runs the full `save_project` pass so catalog-derived files and the sector
/// JSON are persisted too. Finally re-reads each still-open buffer so the
/// editor mirrors the canonical post-save bytes.
pub fn save_all(state: &mut BuilderState) -> Result<(), String> {
    let dirty: Vec<String> = state
        .toml_editor
        .open
        .iter()
        .filter(|(_, b)| b.is_dirty())
        .map(|(k, _)| k.clone())
        .collect();
    for rel in &dirty {
        save_one(state, rel)?;
    }
    project_io::save_project(state).map_err(|e| e.to_string())?;
    resync_open_buffers(state);
    Ok(())
}

/// Re-read every open buffer from disk and reset its dirty/validation state.
/// Used after [`save_all`] because `save_project` re-serialises the catalog
/// files (formatting may differ from the user's hand layout).
fn resync_open_buffers(state: &mut BuilderState) {
    let Some(root) = state.project_path.clone() else {
        return;
    };
    let rels: Vec<String> = state.toml_editor.open.keys().cloned().collect();
    for rel in rels {
        let abs = root.join(&rel);
        if let Ok(text) = std::fs::read_to_string(abs.as_std_path()) {
            if let Some(b) = state.toml_editor.open.get_mut(&rel) {
                *b = OpenTomlBuffer::new(text);
            }
        }
        state.dirty_files.remove(&rel);
    }
}

// ── Syntax highlighting ──────────────────────────────────────────────────────

/// Build a [`LayoutJob`] that tints a TOML document: comments, `[section]`
/// headers, `key =` names, and quoted strings each get their own colour. The
/// tokenizer is line-based — cheap and good enough for a config editor.
fn highlight(text: &str, style: &egui::Style) -> LayoutJob {
    let font = TextStyle::Monospace.resolve(style);
    let normal = style.visuals.text_color();
    let palette = Palette {
        comment: Color32::from_rgb(106, 153, 85),
        header: Color32::from_rgb(86, 156, 214),
        key: Color32::from_rgb(156, 220, 254),
        string: Color32::from_rgb(206, 145, 120),
        punct: Color32::from_gray(150),
        normal,
    };

    let mut job = LayoutJob::default();
    for line in text.split_inclusive('\n') {
        highlight_line(&mut job, line, &font, &palette);
    }
    job
}

struct Palette {
    comment: Color32,
    header: Color32,
    key: Color32,
    string: Color32,
    punct: Color32,
    normal: Color32,
}

fn highlight_line(job: &mut LayoutJob, raw: &str, font: &FontId, p: &Palette) {
    // Split off a trailing newline so it is always emitted as plain text.
    let (line, newline) = match raw.strip_suffix('\n') {
        Some(body) => (body, "\n"),
        None => (raw, ""),
    };

    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    seg(job, indent, p.normal, font);

    if trimmed.starts_with('#') {
        seg(job, trimmed, p.comment, font);
    } else if trimmed.starts_with('[') {
        // `[section]` or `[[array]]` header. Colour the bracketed span; emit
        // any trailing inline comment in comment colour.
        match trimmed.find('#') {
            Some(h) => {
                seg(job, &trimmed[..h], p.header, font);
                seg(job, &trimmed[h..], p.comment, font);
            }
            None => seg(job, trimmed, p.header, font),
        }
    } else if let Some(eq) = trimmed.find('=') {
        seg(job, &trimmed[..eq], p.key, font);
        seg(job, "=", p.punct, font);
        highlight_value(job, &trimmed[eq + 1..], font, p);
    } else {
        seg(job, trimmed, p.normal, font);
    }

    seg(job, newline, p.normal, font);
}

/// Colour the value half of a `key = value` line. Quoted strings get the string
/// colour; an unquoted trailing `# comment` gets the comment colour.
fn highlight_value(job: &mut LayoutJob, value: &str, font: &FontId, p: &Palette) {
    let has_quote = value.contains('"') || value.contains('\'');
    if !has_quote {
        if let Some(h) = value.find('#') {
            seg(job, &value[..h], p.normal, font);
            seg(job, &value[h..], p.comment, font);
            return;
        }
        seg(job, value, p.normal, font);
        return;
    }
    // String-bearing value: tint the whole thing as a string. Good enough for a
    // line-based highlighter without a full TOML lexer.
    seg(job, value, p.string, font);
}

fn seg(job: &mut LayoutJob, text: &str, color: Color32, font: &FontId) {
    if text.is_empty() {
        return;
    }
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font.clone(),
            color,
            ..Default::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_toml_matches_extension_case_insensitively() {
        assert!(is_toml("data/factions/factions.toml"));
        assert!(is_toml("FOO.TOML"));
        assert!(!is_toml("out/sector.json"));
        assert!(!is_toml("README"));
    }

    #[test]
    fn file_name_takes_leaf() {
        assert_eq!(file_name("data/worlds/worlds.toml"), "worlds.toml");
        assert_eq!(file_name("sectorforge.toml"), "sectorforge.toml");
    }

    #[test]
    fn highlight_emits_full_text() {
        let src = "# comment\n[section]\nkey = \"value\"\nn = 3 # trailing\n";
        let job = highlight(src, &egui::Style::default());
        // The concatenated sections must reproduce the input byte-for-byte.
        assert_eq!(job.text, src);
    }
}
