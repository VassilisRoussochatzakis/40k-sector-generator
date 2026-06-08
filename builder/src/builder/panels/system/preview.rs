//! SYSTEM tab — §CTX0 in-system map (Phase 0 of docs/CONTEXT_MENU.txt) and the
//! §T5 per-system bitmap preview.

use egui::{Color32, RichText, Ui};

use sectorforge::system_map::{render_system, SystemRenderOptions};
use sectorforge_gui_core::system_view::{SystemClick, SystemLayout, SystemSelection, SystemView};
use sectorforge_gui_core::{palette, ui_kit};

use crate::builder::state::SystemBitmapPreview;
use crate::builder::BuilderState;

use super::{SYSTEM_VIEW_SIDE_MAX, SYSTEM_VIEW_SIDE_MIN, SYS_STAR_GRID_ANCHOR};

// ── §CTX0 in-system map (Phase 0 of docs/CONTEXT_MENU.txt) ─────────────────

/// Embeds the shared [`SystemView`] widget under the SYSTEM tab so the in-system
/// map has a host before the context-menu work in Phase 6 lands. Click on a
/// planet → updates [`BuilderState::selected_world_id`]; click on the central
/// star → arms [`BuilderState::scroll_target`] so the Star section scrolls
/// into view on the same frame.
pub(crate) fn show_system_map_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "sys_in_map", "In-system map", true, |ui| {
        let panel_width = ui.available_width();
        ui.horizontal(|ui| {
            ui.label("Layout:");
            let mut horiz = matches!(state.system_view.layout, SystemLayout::Horizontal);
            if ui
                .selectable_label(horiz, "Horizontal")
                .on_hover_text("Star left, planets arrayed right in orbit order")
                .clicked()
            {
                horiz = true;
            }
            if ui
                .selectable_label(!horiz, "Orbital")
                .on_hover_text("Concentric orbit rings")
                .clicked()
            {
                horiz = false;
            }
            state.system_view.layout = if horiz {
                SystemLayout::Horizontal
            } else {
                SystemLayout::Orbital
            };
            ui.separator();
            ui.label("Size:");
            ui.add(
                egui::Slider::new(
                    &mut state.system_view.side,
                    SYSTEM_VIEW_SIDE_MIN..=SYSTEM_VIEW_SIDE_MAX,
                )
                .show_value(false),
            );
            if ui
                .button("Fit width")
                .on_hover_text("Resize preview to fill the panel width")
                .clicked()
            {
                state.system_view.side =
                    panel_width.clamp(SYSTEM_VIEW_SIDE_MIN, SYSTEM_VIEW_SIDE_MAX);
            }
        });
        let layout = state.system_view.layout;
        let side = state
            .system_view
            .side
            .clamp(SYSTEM_VIEW_SIDE_MIN, SYSTEM_VIEW_SIDE_MAX);
        // 3:1 aspect — preview spans the panel width but only a third as
        // tall so the in-system map doesn't dominate the SYSTEM tab.
        let height = (side / 3.0).max(SYSTEM_VIEW_SIDE_MIN / 3.0);
        let sys = &state.sector.systems[sys_idx];
        // §CTX1 Phase 7 — while the in-system right-click menu is open,
        // override the `selected` rendering so the SystemView highlights
        // the menu's contextual entity (star / world). Reuses the existing
        // SELECTION ring so we don't need to allocate a separate painter
        // overlay. Falls back to `selected_world_id` when no menu is open.
        let selected = crate::builder::panels::system_map::menu_selection_override(state, sys_idx)
            .unwrap_or_else(|| match state.selection.world_id.as_ref() {
                Some(wid) => sys
                    .worlds
                    .iter()
                    .find(|w| &w.id == wid)
                    .map(|w| SystemSelection::World(w.index))
                    .unwrap_or(SystemSelection::None),
                None => SystemSelection::None,
            });
        let (resp, click) = SystemView {
            system: sys,
            selected,
            side,
            height,
            layout,
        }
        .show(ui);
        if let Some(c) = click {
            handle_system_view_click(state, sys_idx, c);
        }
        // §CTX1 Phase 6 — secondary-click → open in-system menu. Resolver
        // + render live in `panels/system_map.rs`.
        if resp.secondary_clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                crate::builder::panels::system_map::arm_system_context_menu(
                    state,
                    sys_idx,
                    side,
                    height,
                    pos,
                    resp.rect.min,
                );
            }
        }
    });
    crate::builder::panels::system_map::show_system_context_menu(ui.ctx(), state);
    crate::builder::panels::system_map::show_world_rename_dialog(ui.ctx(), state);
}

/// Side-effect-free routing of a [`SystemClick`] to the corresponding builder
/// state mutation. Extracted so unit tests can exercise the wiring without
/// spinning up an egui context.
pub(super) fn handle_system_view_click(
    state: &mut BuilderState,
    sys_idx: usize,
    click: SystemClick,
) {
    match click {
        SystemClick::Star => {
            state.selection.scroll_target = Some(SYS_STAR_GRID_ANCHOR);
        }
        SystemClick::World(idx) => {
            let sys = &state.sector.systems[sys_idx];
            if let Some(w) = sys.worlds.iter().find(|w| w.index == idx) {
                state.selection.world_id = Some(w.id.clone());
            }
        }
        _ => {}
    }
}

// ── identity (S2 + S6) ──────────────────────────────────────────────────────

/// §35 T5 — per-system bitmap preview. Renders the focused system through the
/// same `system_map` PNG renderer the exporter uses (honouring the project's
/// §T1/§T2 map theme and the §EX3 `faction_fill` flag), uploads it once as an
/// egui texture, and caches it until the system / theme / faction_fill changes.
pub(super) fn show_bitmap_preview_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    /// Preview render resolution. Scale 1 ≈ 1080×720 px — large enough to read,
    /// cheap enough to re-upload on demand. The exporter's `system_scale`
    /// controls the on-disk resolution separately.
    const PREVIEW_SCALE: u32 = 1;

    ui_kit::collapsing_section(
        ui,
        "sys_bitmap_preview",
        "Image preview (§T5)",
        false,
        |ui| {
            let faction_fill = state.config.outputs.bitmap.faction_fill;
            let theme =
                sectorforge::map_theme::resolve_map_theme(&state.config.outputs.bitmap.theme)
                    .unwrap_or_else(|_| sectorforge::map_theme::MapTheme::gm_dark());
            let theme_name = theme.name.clone();

            let key = {
                let sys = &state.sector.systems[sys_idx];
                format!(
                    "{}|ff{}|{}|w{}|s{}",
                    sys.id,
                    u8::from(faction_fill),
                    theme_name,
                    sys.worlds.len(),
                    PREVIEW_SCALE
                )
            };

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("How this system will look when exported as an image.")
                        .small()
                        .color(palette::chrome_text_dim()),
                );
                if ui
                    .button("🔄 Refresh")
                    .on_hover_text("Re-render the preview from the current system and map theme")
                    .clicked()
                {
                    state.system_bitmap_preview = None;
                }
            });

            let stale = state
                .system_bitmap_preview
                .as_ref()
                .is_none_or(|p| p.key != key);
            if stale {
                let opts = SystemRenderOptions {
                    faction_fill,
                    theme,
                };
                let img = {
                    let sys = &state.sector.systems[sys_idx];
                    render_system(sys, &state.sector.factions, PREVIEW_SCALE, opts)
                };
                let size = [img.width() as usize, img.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                let texture = ui.ctx().load_texture(
                    "sys_bitmap_preview",
                    color,
                    egui::TextureOptions::LINEAR,
                );
                state.system_bitmap_preview = Some(SystemBitmapPreview { key, texture, size });
            }

            if let Some(p) = &state.system_bitmap_preview {
                let avail = ui.available_width().min(p.size[0] as f32);
                let sized = egui::load::SizedTexture::new(
                    p.texture.id(),
                    egui::vec2(p.size[0] as f32, p.size[1] as f32),
                );
                ui.add(egui::Image::new(sized).max_width(avail));
                ui.label(
                    RichText::new(format!(
                        "{}×{} px · theme {theme_name} · faction colours {}",
                        p.size[0],
                        p.size[1],
                        if faction_fill { "on" } else { "off" }
                    ))
                    .small()
                    .color(Color32::DARK_GRAY),
                );
            }
        },
    );
}
