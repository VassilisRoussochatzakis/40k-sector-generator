//! MAP tab — §35 THEMES + HEATMAPS controls.
//!
//! §T1 builds a built-in theme picker over
//! [`sectorforge::map_theme::BUILTIN_THEME_NAMES`]. Selecting one rewrites
//! `config.outputs.bitmap.theme` (the same field the PNG/SVG/per-system
//! exporters consume) and — because [`super::interactions`] rebuilds a
//! `RenderMapTheme` from it every frame — retints the live MAP immediately.
//!
//! §T2 is a custom theme editor over every
//! [`sectorforge::map_theme::MapThemeConfig`] field (per-element colours,
//! route_line_mode, label_density, legend, symbol_set, factors). It saves /
//! loads `data/map_themes/<name>.toml`.
//!
//! §T3 is a heatmap selector covering every
//! [`sectorforge::heatmap::HeatmapMode`] plus the per-dimension
//! [`sectorforge::heatmap::StabilityDimension`] family. It drives
//! `map_heatmap_mode` / `map_stability_dim`.
//!
//! §T4 paints a route-style legend from the resolved theme's route colours +
//! line mode; it live-updates as the theme or any override changes.

use egui::{Color32, RichText, Ui};

use sectorforge::heatmap::{HeatmapMode, StabilityDimension};
use sectorforge::map_theme::{
    normalize_name, parse_map_theme_file, resolve_map_theme, LabelDensity, LegendStyle, MapTheme,
    MapThemeConfig, RouteLineMode, SymbolSet, BUILTIN_THEME_NAMES,
};
use sectorforge::sector_model::RouteStability;
use sectorforge_gui_core::ui_kit::{self, labeled};

use crate::builder::panels::system::pretty_slug;
use crate::builder::BuilderState;

/// Entry point: heatmap selector row (§T3) + the collapsing theme section
/// (§T1/§T2/§T4). Called from the MAP panel toolbar area.
pub(crate) fn show(ui: &mut Ui, state: &mut BuilderState) {
    show_heatmap_selector(ui, state);
    egui::CollapsingHeader::new(RichText::new("Map theme").strong())
        .id_salt("map_theme_section")
        .show(ui, |ui| {
            show_theme_picker(ui, state);
            ui.add_space(4.0);
            show_route_legend(ui, state);
            ui.add_space(4.0);
            show_custom_editor(ui, state);
        });
}

/// Human label for a built-in or custom theme name. Built-ins get hand-tuned
/// casing (the model exposes no `display_name()`); anything else falls back to
/// the shared [`pretty_slug`] humanizer (`panels::system`'s copy — previously
/// duplicated here as `humanize_slug`).
fn theme_display_name(name: &str) -> String {
    match name {
        "gm_dark" => "GM dark".to_string(),
        "print_mono" => "Print (mono)".to_string(),
        "imperial_archive" => "Imperial archive".to_string(),
        "navis_tactical" => "Navis tactical".to_string(),
        "inquisition_redacted" => "Inquisition (redacted)".to_string(),
        "subsector_political" => "Subsector political".to_string(),
        other => pretty_slug(other),
    }
}

// ── §T3 heatmap selector ─────────────────────────────────────────────────────

fn show_heatmap_selector(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Heatmap:");
        egui::ComboBox::from_id_salt("map_heatmap_full")
            .selected_text(current_heatmap_label(state))
            .show_ui(ui, |ui| {
                for mode in HeatmapMode::ALL {
                    let selected = state.map_view.stability_dim.is_none()
                        && state.map_view.heatmap_mode == *mode;
                    if ui.selectable_label(selected, mode.label()).clicked() {
                        state.map_view.heatmap_mode = *mode;
                        state.map_view.stability_dim = None;
                    }
                }
                ui.separator();
                ui_kit::placeholder(ui, "Stability — by dimension:");
                for dim in StabilityDimension::ALL {
                    let selected = state.map_view.stability_dim == Some(*dim);
                    if ui.selectable_label(selected, dim.label()).clicked() {
                        state.map_view.stability_dim = Some(*dim);
                        state.map_view.heatmap_mode = HeatmapMode::Off;
                    }
                }
            });
        ui.colored_label(
            Color32::DARK_GRAY,
            "A Control overlay takes over while it's active.",
        );
    });
}

fn current_heatmap_label(state: &BuilderState) -> String {
    match state.map_view.stability_dim {
        Some(dim) => format!("Stability: {}", dim.label()),
        None => state.map_view.heatmap_mode.label().to_string(),
    }
}

// ── §T1 built-in picker ──────────────────────────────────────────────────────

fn show_theme_picker(ui: &mut Ui, state: &mut BuilderState) {
    let current = state
        .config
        .outputs
        .bitmap
        .theme
        .name
        .clone()
        .unwrap_or_else(|| "gm_dark".to_string());
    let cur_norm = normalize_name(&current);
    let is_builtin = BUILTIN_THEME_NAMES.contains(&cur_norm.as_str());

    ui.horizontal_wrapped(|ui| {
        ui.label("Theme:");
        let selected_text = if is_builtin {
            theme_display_name(&cur_norm)
        } else {
            format!("{} (custom)", theme_display_name(&current))
        };
        egui::ComboBox::from_id_salt("map_theme_pick")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for name in BUILTIN_THEME_NAMES {
                    if ui
                        .selectable_label(is_builtin && cur_norm == *name, theme_display_name(name))
                        .on_hover_text(format!("schema: {name}"))
                        .clicked()
                    {
                        // Pick a clean builtin — drop any custom overrides.
                        state.config.outputs.bitmap.theme = MapThemeConfig::named(*name);
                        state.theme_panel.save_name = (*name).to_string();
                        mark_theme_dirty(state);
                    }
                }
            });
        ui.colored_label(
            Color32::DARK_GRAY,
            "Applies to the live map and PNG/SVG export.",
        );
    });
}

// ── §T4 route-style legend (live) ────────────────────────────────────────────

fn show_route_legend(ui: &mut Ui, state: &BuilderState) {
    let theme = resolve_or_default(&state.config.outputs.bitmap.theme);
    ui.label(
        RichText::new("Route colours")
            .small()
            .color(Color32::DARK_GRAY),
    );
    ui.horizontal_wrapped(|ui| {
        for (stab, name) in [
            (RouteStability::Stable, "Stable"),
            (RouteStability::Unstable, "Unstable"),
            (RouteStability::Hazardous, "Hazardous"),
            (RouteStability::Perilous, "Perilous"),
        ] {
            ui.colored_label(c32(route_px(&theme, stab)), format!("▬ {name}"));
            ui.add_space(6.0);
        }
    });
    ui.label(
        RichText::new(format!(
            "Line style: {} · Legend: {} · Symbols: {}",
            pretty_slug(theme.route_line_mode.as_slug()),
            pretty_slug(theme.legend.as_slug()),
            pretty_slug(theme.symbol_set.as_slug()),
        ))
        .small()
        .color(Color32::DARK_GRAY),
    );
}

// ── §T2 custom theme editor ──────────────────────────────────────────────────

fn show_custom_editor(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new(RichText::new("Custom theme editor (§T2)").strong())
        .id_salt("map_theme_custom")
        .show(ui, |ui| {
            let changed = {
                let cfg = &mut state.config.outputs.bitmap.theme;
                edit_theme_fields(ui, cfg)
            };
            if changed {
                mark_theme_dirty(state);
            }
            ui.separator();
            show_save_load_row(ui, state);
        });
}

/// Render every [`MapThemeConfig`] field. Pure over the config so the borrow of
/// `state` stays short. Returns whether anything changed.
fn edit_theme_fields(ui: &mut Ui, cfg: &mut MapThemeConfig) -> bool {
    let def = resolve_or_default(cfg);
    let mut changed = false;

    ui.label(
        RichText::new("Colours — pick a swatch to override the theme default.")
            .small()
            .color(Color32::DARK_GRAY),
    );
    // Each row: human label (left, with the schema field + note in its hover) →
    // swatch → reset. Replaces the old control-left / raw-field-name-right grid.
    let rows: [(&str, &str, &mut Option<String>, [u8; 3]); 18] = [
        (
            "Background",
            "Whole-canvas background fill (schema: background).",
            &mut cfg.background,
            px3(def.bg.0),
        ),
        (
            "Panel background",
            "Fill behind side panels and legends (schema: panel_background).",
            &mut cfg.panel_background,
            px3(def.panel_bg.0),
        ),
        (
            "Empty hex",
            "Fill of hexes with no system (schema: hex_empty).",
            &mut cfg.hex_empty,
            px3(def.hex_empty.0),
        ),
        (
            "Hex outline",
            "Line around each hex cell (schema: hex_outline).",
            &mut cfg.hex_outline,
            px3(def.hex_outline.0),
        ),
        (
            "Text",
            "Primary label text (schema: text).",
            &mut cfg.text,
            px3(def.text.0),
        ),
        (
            "Dim text",
            "Secondary / muted label text (schema: text_dim).",
            &mut cfg.text_dim,
            px3(def.text_dim.0),
        ),
        (
            "Subsector border",
            "Line dividing subsectors (schema: subsector_border).",
            &mut cfg.subsector_border,
            px3(def.subsector_border.0),
        ),
        (
            "Subsector label",
            "Subsector name text (schema: subsector_label).",
            &mut cfg.subsector_label,
            px3(def.subsector_label.0),
        ),
        (
            "Subsector label background",
            "Plate behind subsector names (schema: subsector_label_background).",
            &mut cfg.subsector_label_background,
            px3(def.subsector_label_bg.0),
        ),
        (
            "Capital marker",
            "Fill of the sector-capital marker (schema: capital_marker).",
            &mut cfg.capital_marker,
            px3(def.capital_marker.0),
        ),
        (
            "Capital outline",
            "Outline of the sector-capital marker (schema: capital_outline).",
            &mut cfg.capital_outline,
            px3(def.capital_outline.0),
        ),
        (
            "Route — stable",
            "Colour of stable routes (schema: route_stable).",
            &mut cfg.route_stable,
            px3(def.route_stable.0),
        ),
        (
            "Route — unstable",
            "Colour of unstable routes (schema: route_unstable).",
            &mut cfg.route_unstable,
            px3(def.route_unstable.0),
        ),
        (
            "Route — hazardous",
            "Colour of hazardous routes (schema: route_hazardous).",
            &mut cfg.route_hazardous,
            px3(def.route_hazardous.0),
        ),
        (
            "Route — perilous",
            "Colour of perilous routes (schema: route_perilous).",
            &mut cfg.route_perilous,
            px3(def.route_perilous.0),
        ),
        (
            "Route type marks",
            "Colour of route-type indicators (schema: route_type).",
            &mut cfg.route_type,
            px3(def.route_type.0),
        ),
        (
            "Neutral control route",
            "Route colour where no faction holds control (schema: route_control_neutral).",
            &mut cfg.route_control_neutral,
            px3(def.route_control_neutral.0),
        ),
        (
            "Orbit ring",
            "Colour of orbital rings around systems (schema: orbit_ring).",
            &mut cfg.orbit_ring,
            px3(def.orbit_ring.0),
        ),
    ];
    for (label, help, slot, default_rgb) in rows {
        changed |= colour_row(ui, label, help, slot, default_rgb);
    }

    ui.add_space(4.0);
    ui.label(RichText::new("Rendering").small().color(Color32::DARK_GRAY));
    changed |= line_mode_row(ui, &mut cfg.route_line_mode, def.route_line_mode);
    changed |= label_density_row(ui, &mut cfg.label_density, def.label_density);
    changed |= legend_row(ui, &mut cfg.legend, def.legend);
    changed |= symbol_set_row(ui, &mut cfg.symbol_set, def.symbol_set);
    changed |= bool_row(
        ui,
        "Show subsector borders",
        "Draw the lines between subsectors (schema: show_subsector_borders).",
        &mut cfg.show_subsector_borders,
        def.show_subsector_borders,
    );

    ui.add_space(4.0);
    ui.label(RichText::new("Factors").small().color(Color32::DARK_GRAY));
    changed |= factor_row(
        ui,
        "Faction tint strength",
        "How strongly faction colour tints held hexes (schema: faction_tint_strength).",
        &mut cfg.faction_tint_strength,
        def.faction_tint_strength,
        0.0,
        1.0,
    );
    changed |= factor_row(
        ui,
        "Heatmap tint — floor",
        "Minimum heatmap tint applied to the lowest values (schema: heatmap_tint_min).",
        &mut cfg.heatmap_tint_min,
        def.heatmap_tint_min,
        0.0,
        1.0,
    );
    changed |= factor_row(
        ui,
        "Heatmap tint — range",
        "Spread of heatmap tint from low to high values (schema: heatmap_tint_range).",
        &mut cfg.heatmap_tint_range,
        def.heatmap_tint_range,
        0.0,
        1.0,
    );
    changed |= factor_row(
        ui,
        "Route thickness",
        "Line width multiplier for routes (schema: route_thickness).",
        &mut cfg.route_thickness,
        def.route_thickness,
        0.4,
        3.0,
    );
    changed |= factor_row(
        ui,
        "Region tint strength",
        "How strongly region colour tints its hexes (schema: region_tint_strength).",
        &mut cfg.region_tint_strength,
        def.region_tint_strength,
        0.0,
        1.0,
    );

    changed
}

fn show_save_load_row(ui: &mut Ui, state: &mut BuilderState) {
    if state.theme_panel.save_name.is_empty() {
        state.theme_panel.save_name = state
            .config
            .outputs
            .bitmap
            .theme
            .name
            .clone()
            .unwrap_or_else(|| "custom".to_string());
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("data/map_themes/");
        ui.add(
            egui::TextEdit::singleline(&mut state.theme_panel.save_name)
                .hint_text("theme name")
                .desired_width(140.0),
        );
        ui.label(".toml");
        let name_ok = !state.theme_panel.save_name.trim().is_empty();
        let has_project = state.project_path.is_some();
        if ui
            .add_enabled(name_ok && has_project, egui::Button::new("💾  Save theme"))
            .on_hover_text("Write the current theme to a .toml file under the project")
            .clicked()
        {
            save_theme_to_disk(state);
        }
        if ui
            .add_enabled(name_ok && has_project, egui::Button::new("📂  Load"))
            .on_hover_text("Replace the current theme with the named .toml file")
            .clicked()
        {
            load_theme_from_disk(state);
        }
        if !has_project {
            ui.colored_label(Color32::DARK_GRAY, "(open a project to save)");
        }
    });
    if let Some(msg) = &state.theme_panel.status {
        let col = if msg.starts_with('✓') {
            Color32::from_rgb(120, 200, 130)
        } else {
            Color32::from_rgb(230, 110, 110)
        };
        ui.colored_label(col, msg);
    }
}

// ── disk round-trip ──────────────────────────────────────────────────────────

fn save_theme_to_disk(state: &mut BuilderState) {
    let Some(root) = state.project_path.clone() else {
        return;
    };
    let name = state.theme_panel.save_name.trim().to_string();
    let mut cfg = state.config.outputs.bitmap.theme.clone();
    if cfg.name.is_none() {
        cfg.name = Some(name.clone());
    }
    let file = sectorforge::map_theme::MapThemeFile { map_theme: cfg };
    let text = match toml::to_string_pretty(&file) {
        Ok(t) => t,
        Err(e) => {
            state.theme_panel.status = Some(format!("✗ Could not serialise theme: {e}"));
            return;
        }
    };
    let dir = root.join("data").join("map_themes");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        state.theme_panel.status = Some(format!("✗ Could not create folder: {e}"));
        return;
    }
    let path = dir.join(format!("{name}.toml"));
    match crate::builder::project_io::atomic_write(&path, text.as_bytes()) {
        Ok(()) => {
            state.theme_panel.status = Some(format!("✓ Saved data/map_themes/{name}.toml"));
        }
        Err(e) => state.theme_panel.status = Some(format!("✗ Could not write file: {e}")),
    }
}

fn load_theme_from_disk(state: &mut BuilderState) {
    let Some(root) = state.project_path.clone() else {
        return;
    };
    let name = state.theme_panel.save_name.trim().to_string();
    let path = root
        .join("data")
        .join("map_themes")
        .join(format!("{name}.toml"));
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            state.theme_panel.status = Some(format!("✗ Could not read file: {e}"));
            return;
        }
    };
    match parse_map_theme_file(&text) {
        Ok(cfg) => {
            state.config.outputs.bitmap.theme = cfg;
            mark_theme_dirty(state);
            state.theme_panel.status = Some(format!("✓ Loaded data/map_themes/{name}.toml"));
        }
        Err(e) => state.theme_panel.status = Some(format!("✗ Could not parse theme: {e}")),
    }
}

// ── row helpers ──────────────────────────────────────────────────────────────

fn colour_row(
    ui: &mut Ui,
    label: &str,
    help: &str,
    slot: &mut Option<String>,
    default_rgb: [u8; 3],
) -> bool {
    let mut changed = false;
    labeled(ui, label, help, |ui| {
        let mut rgb = slot
            .as_deref()
            .and_then(|s| Color32::from_hex(s).ok())
            .map(|c| [c.r(), c.g(), c.b()])
            .unwrap_or(default_rgb);
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            *slot = Some(to_hex(rgb));
            changed = true;
        }
        if slot.is_some() {
            if ui
                .small_button("↺")
                .on_hover_text("Reset this colour to the theme default")
                .clicked()
            {
                *slot = None;
                changed = true;
            }
        } else {
            ui.label(RichText::new("default").small().color(Color32::DARK_GRAY));
        }
    });
    changed
}

fn line_mode_row(ui: &mut Ui, slot: &mut Option<RouteLineMode>, def: RouteLineMode) -> bool {
    enum_combo(
        ui,
        "Route line style",
        "How route lines are drawn (schema: route_line_mode).",
        "route_line_mode",
        slot,
        def,
        &[RouteLineMode::Standard, RouteLineMode::HazardWeighted],
        |v| v.as_slug(),
    )
}

fn label_density_row(ui: &mut Ui, slot: &mut Option<LabelDensity>, def: LabelDensity) -> bool {
    enum_combo(
        ui,
        "Label density",
        "Which system labels to print (schema: label_density).",
        "label_density",
        slot,
        def,
        &[
            LabelDensity::All,
            LabelDensity::ImportantOnly,
            LabelDensity::None,
        ],
        |v| v.as_slug(),
    )
}

fn legend_row(ui: &mut Ui, slot: &mut Option<LegendStyle>, def: LegendStyle) -> bool {
    enum_combo(
        ui,
        "Legend style",
        "Size and visibility of the map legend (schema: legend).",
        "legend",
        slot,
        def,
        &[LegendStyle::Full, LegendStyle::Compact, LegendStyle::Hidden],
        |v| v.as_slug(),
    )
}

fn symbol_set_row(ui: &mut Ui, slot: &mut Option<SymbolSet>, def: SymbolSet) -> bool {
    enum_combo(
        ui,
        "Symbol set",
        "Glyph family used for system markers (schema: symbol_set).",
        "symbol_set",
        slot,
        def,
        &[
            SymbolSet::Standard,
            SymbolSet::Tactical,
            SymbolSet::Redacted,
        ],
        |v| v.as_slug(),
    )
}

/// Generic `Option<E>` combo with a leading "Default (X)" entry that clears the
/// override. Each entry shows a humanized label with the raw slug in a hover.
/// `E: Copy + PartialEq`.
#[allow(clippy::too_many_arguments)]
fn enum_combo<E: Copy + PartialEq>(
    ui: &mut Ui,
    label: &str,
    help: &str,
    id_salt: &str,
    slot: &mut Option<E>,
    def: E,
    variants: &[E],
    slug: impl Fn(E) -> &'static str,
) -> bool {
    let mut changed = false;
    labeled(ui, label, help, |ui| {
        let current = slot.unwrap_or(def);
        let text = if slot.is_some() {
            pretty_slug(slug(current))
        } else {
            format!("Default ({})", pretty_slug(slug(def)))
        };
        egui::ComboBox::from_id_salt(format!("theme_enum_{id_salt}"))
            .selected_text(text)
            .show_ui(ui, |ui| {
                let default_label = format!("Default ({})", pretty_slug(slug(def)));
                if ui
                    .selectable_label(slot.is_none(), default_label)
                    .on_hover_text("Inherit this value from the theme")
                    .clicked()
                    && slot.is_some()
                {
                    *slot = None;
                    changed = true;
                }
                for v in variants {
                    if ui
                        .selectable_label(*slot == Some(*v), pretty_slug(slug(*v)))
                        .on_hover_text(format!("schema: {}", slug(*v)))
                        .clicked()
                    {
                        *slot = Some(*v);
                        changed = true;
                    }
                }
            });
    });
    changed
}

fn bool_row(ui: &mut Ui, label: &str, help: &str, slot: &mut Option<bool>, def: bool) -> bool {
    let mut changed = false;
    labeled(ui, label, help, |ui| {
        let mut v = slot.unwrap_or(def);
        if ui.checkbox(&mut v, "").changed() {
            *slot = Some(v);
            changed = true;
        }
        if slot.is_some() {
            if ui
                .small_button("↺")
                .on_hover_text("Reset to the theme default")
                .clicked()
            {
                *slot = None;
                changed = true;
            }
        } else {
            ui.label(RichText::new("default").small().color(Color32::DARK_GRAY));
        }
    });
    changed
}

#[allow(clippy::too_many_arguments)]
fn factor_row(
    ui: &mut Ui,
    label: &str,
    help: &str,
    slot: &mut Option<f32>,
    def: f32,
    min: f32,
    max: f32,
) -> bool {
    let mut changed = false;
    labeled(ui, label, help, |ui| {
        let mut v = slot.unwrap_or(def);
        if ui
            .add(egui::DragValue::new(&mut v).speed(0.01).range(min..=max))
            .changed()
        {
            *slot = Some(v);
            changed = true;
        }
        if slot.is_some() {
            if ui
                .small_button("↺")
                .on_hover_text("Reset to the theme default")
                .clicked()
            {
                *slot = None;
                changed = true;
            }
        } else {
            ui.label(RichText::new("default").small().color(Color32::DARK_GRAY));
        }
    });
    changed
}

// ── shared utilities ─────────────────────────────────────────────────────────

fn mark_theme_dirty(state: &mut BuilderState) {
    state.dirty = true;
    state.dirty_files.insert("sectorforge.toml".to_string());
    state.mark_validation_dirty();
}

/// Resolve the config to a concrete [`MapTheme`], falling back to gm_dark when a
/// custom colour fails to parse so the editor never panics mid-edit.
fn resolve_or_default(cfg: &MapThemeConfig) -> MapTheme {
    resolve_map_theme(cfg).unwrap_or_else(|_| MapTheme::gm_dark())
}

fn route_px(theme: &MapTheme, stab: RouteStability) -> [u8; 4] {
    match stab {
        RouteStability::Stable => theme.route_stable.0,
        RouteStability::Unstable => theme.route_unstable.0,
        RouteStability::Hazardous => theme.route_hazardous.0,
        RouteStability::Perilous => theme.route_perilous.0,
        _ => theme.route_unstable.0,
    }
}

fn px3([r, g, b, _a]: [u8; 4]) -> [u8; 3] {
    [r, g, b]
}

fn c32([r, g, b, a]: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn to_hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}
