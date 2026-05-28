//! PERSONAE tab (§N1 / §N2) — Phase D §PER1..§PER5.
//!
//! §PER1  Per-faction-kind persona-pool editor. Loads defaults from
//!        `src/personae.rs` (built-in [`KindPools`]); per-project overrides
//!        live under `[inputs].personae` -> `data/personae.toml`. Each kind
//!        exposes editable name prefixes / roots / suffixes / single names /
//!        titles / traits.
//! §PER2  Per-anchor persona editor. Lists the derived personae (system
//!        sovereign / orbital controller / economic hegemon / hidden master /
//!        per-world presences) and lets users add/remove `[[manual]]`
//!        entries with name, title, traits, agenda. Manual entries survive
//!        regenerate because [`sectorforge::personae::derive_with`] appends
//!        `cfg.manual` last.
//! §PER3  "Auto-derive" button calls [`BuilderState::recompute_personae`]
//!        which runs `personae::derive_with(&sector, &cfg)`. Auto-recompute-
//!        on-edit toggle mirrors the History/Relations panels.
//! §PER4  Dominance-tier setting (`min_world_dominance`) controls which
//!        per-world presence rows anchor a persona. Per-anchor caps live
//!        alongside it.
//! §PER5  Agenda text bound to competing claims on the anchor world. The
//!        derivation source — `kind`, anchor, rival claim if any — appears
//!        as a tooltip beside each row's agenda field.
//!
//! The panel never edits derived `personae_report` rows directly. All
//! mutations land in [`BuilderState::data_catalogs::personae`] and the
//! recompute pass rewrites the published overlay.

use egui::{Color32, RichText, Ui};

use sectorforge::personae::{
    DominanceTier, KindPools, Persona, PersonaAnchor, PersonaeConfig, SystemSlot,
};

use crate::builder::state::EntityRef;
use crate::builder::BuilderState;

const DEFAULT_PERSONAE_PATH: &str = "data/personae.toml";

/// Built-in faction kinds the editor lists by default. Users can add custom
/// kinds via the "add kind" button — anything not in this list still derives
/// fall-through pools from [`sectorforge::personae`].
const BUILTIN_KINDS: &[&str] = &[
    "imperial",
    "mechanicus",
    "ecclesiarchy",
    "inquisition",
    "rogue_trader",
    "chaos",
    "rebel",
    "necron",
    "tyranid",
    "ork",
    "tau",
    "aeldari",
    "drukhari",
    "harlequin",
    "genestealer",
    "xenos",
];

const DOMINANCE_TIERS: &[DominanceTier] = &[
    DominanceTier::Presence,
    DominanceTier::Influence,
    DominanceTier::Contested,
    DominanceTier::Controlled,
    DominanceTier::Stronghold,
];

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Personae");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "§PER1..§PER5 — kind pool editor, per-anchor personae, dominance tier, agenda derivation.",
    );
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_header_actions(ui, state);
            ui.separator();
            show_dominance_section(ui, state);
            ui.separator();
            show_persona_table(ui, state);
            ui.separator();
            show_manual_editor(ui, state);
            ui.separator();
            show_kind_pools_section(ui, state);
            ui.separator();
            show_save_row(ui, state);
        });
}

// ── §PER3 header actions ────────────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Auto-derive personae").clicked() {
            ensure_personae_catalog(state);
            state.recompute_personae();
        }
        ui.checkbox(&mut state.personae_auto_recompute, "auto-recompute on edit");
        let total = state
            .personae_report
            .as_ref()
            .map(|r| r.personae.len())
            .unwrap_or(0);
        let manual = state
            .data_catalogs
            .personae
            .as_ref()
            .map(|c| c.manual.len())
            .unwrap_or(0);
        ui.label(format!("personae: {total}  (manual: {manual})"));
        if state.data_catalogs.personae.is_none() {
            ui.colored_label(
                Color32::from_rgb(220, 170, 80),
                "no personae.toml loaded (defaults apply)",
            );
        }
    });
}

// ── §PER4 dominance tier + per-anchor caps ──────────────────────────────────

fn show_dominance_section(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§PER4 — dominance tier + per-anchor caps").strong());
    ensure_personae_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.personae.as_mut() else {
        return;
    };
    let mut changed = false;
    egui::Grid::new("per4_grid").num_columns(2).show(ui, |ui| {
        ui.label("min_world_dominance");
        egui::ComboBox::from_id_salt("per4_dom")
            .selected_text(format!("{}", cfg.min_world_dominance))
            .show_ui(ui, |ui| {
                for tier in DOMINANCE_TIERS {
                    if ui
                        .selectable_value(&mut cfg.min_world_dominance, *tier, format!("{tier}"))
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        ui.end_row();
        ui.label("max_per_world");
        changed |= ui
            .add(egui::DragValue::new(&mut cfg.max_per_world).range(0..=64))
            .changed();
        ui.end_row();
        ui.label("max_per_system");
        changed |= ui
            .add(egui::DragValue::new(&mut cfg.max_per_system).range(0..=64))
            .changed();
        ui.end_row();
    });
    ui.colored_label(
        Color32::DARK_GRAY,
        "Higher tier ⇒ fewer worlds anchor personae. Per-system cap counts sovereign/orbital/economic/hidden slots.",
    );
    if changed {
        on_catalog_edited(state);
    }
}

// ── §PER2 + §PER5 persona table ─────────────────────────────────────────────

fn show_persona_table(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§PER2 / §PER5 — derived personae").strong());
    let Some(report) = state.personae_report.clone() else {
        ui.colored_label(
            Color32::GRAY,
            "No personae yet. Click \"Auto-derive personae\" above.",
        );
        return;
    };
    if report.personae.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "Empty roster — lower the dominance tier or generate a sector with faction presence.",
        );
        return;
    }
    let selected = state.selected_persona_id.clone();
    egui::ScrollArea::horizontal()
        .id_salt("per_grid_scroll")
        .show(ui, |ui| {
            egui::Grid::new("per_grid")
                .striped(true)
                .min_col_width(80.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Faction").strong());
                    ui.label(RichText::new("Kind").strong());
                    ui.label(RichText::new("Anchor").strong());
                    ui.label(RichText::new("Name").strong());
                    ui.label(RichText::new("Title").strong());
                    ui.label(RichText::new("Traits").strong());
                    ui.label(RichText::new("Agenda").strong());
                    ui.label(RichText::new("").strong());
                    ui.end_row();

                    for p in &report.personae {
                        let is_selected = selected.as_deref() == Some(p.id.as_str());
                        if ui
                            .selectable_label(is_selected, p.faction_id.to_string())
                            .clicked()
                        {
                            state.selected_persona_id = Some(p.id.to_string());
                            state.focus_entity(EntityRef::Faction(p.faction_id.clone()));
                        }
                        ui.label(if p.faction_kind.is_empty() {
                            RichText::new("—").color(Color32::DARK_GRAY)
                        } else {
                            RichText::new(p.faction_kind.clone())
                        });
                        show_anchor_link(ui, state, p);
                        ui.label(RichText::new(p.name.clone()).strong());
                        ui.label(p.title.clone());
                        ui.label(if p.traits.is_empty() {
                            RichText::new("—").color(Color32::DARK_GRAY)
                        } else {
                            RichText::new(p.traits.join(", "))
                        });
                        ui.label(p.agenda.clone()).on_hover_text(format!(
                            "Source: kind = {}\nfaction = {}\nanchor = {}",
                            if p.faction_kind.is_empty() {
                                "(unknown)"
                            } else {
                                p.faction_kind.as_str()
                            },
                            p.faction_id,
                            anchor_label(&p.anchor),
                        ));
                        if ui.button("edit").clicked() {
                            state.selected_persona_id = Some(p.id.to_string());
                            state.personae_edit_target = Some(p.id.to_string());
                        }
                        ui.end_row();
                    }
                });
        });
}

fn show_anchor_link(ui: &mut Ui, state: &mut BuilderState, p: &Persona) {
    match &p.anchor {
        PersonaAnchor::System { system_id, slot } => {
            if ui
                .link(format!("{system_id} · {}", slot_label(*slot)))
                .clicked()
            {
                state.focus_entity(EntityRef::System(system_id.clone()));
            }
        }
        PersonaAnchor::World {
            system_id,
            world_id,
        } => {
            if ui.link(format!("{system_id}/{world_id}")).clicked() {
                state.focus_entity(EntityRef::World {
                    system: system_id.clone(),
                    world: world_id.clone(),
                });
            }
        }
        _ => {}
    }
}

fn slot_label(slot: SystemSlot) -> &'static str {
    match slot {
        SystemSlot::Sovereign => "sovereign",
        SystemSlot::OrbitalController => "orbital",
        SystemSlot::EconomicHegemon => "economic",
        SystemSlot::HiddenMaster => "hidden",
        _ => "unknown",
    }
}

fn anchor_label(a: &PersonaAnchor) -> String {
    match a {
        PersonaAnchor::System { system_id, slot } => {
            format!("system {system_id} ({})", slot_label(*slot))
        }
        PersonaAnchor::World {
            system_id,
            world_id,
        } => format!("world {system_id}/{world_id}"),
        _ => "unknown".into(),
    }
}

// ── §PER2 manual entry editor ───────────────────────────────────────────────

fn show_manual_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§PER2 — manual personae").strong());
    ensure_personae_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.personae.as_mut() else {
        return;
    };
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ manual persona").clicked() {
            cfg.manual.push(blank_manual_persona(cfg.manual.len()));
            changed = true;
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            "Manual entries are appended after derivation and survive regenerate.",
        );
    });
    if cfg.manual.is_empty() {
        ui.colored_label(Color32::GRAY, "No manual personae yet.");
    } else {
        egui::Grid::new("per_manual_grid")
            .num_columns(7)
            .striped(true)
            .show(ui, |ui| {
                ui.label(RichText::new("id").strong());
                ui.label(RichText::new("faction").strong());
                ui.label(RichText::new("kind").strong());
                ui.label(RichText::new("name").strong());
                ui.label(RichText::new("title").strong());
                ui.label(RichText::new("traits (comma)").strong());
                ui.label(RichText::new("agenda").strong());
                ui.end_row();

                for (idx, p) in cfg.manual.iter_mut().enumerate() {
                    let mut id_buf = p.id.to_string();
                    if ui.text_edit_singleline(&mut id_buf).changed() {
                        p.id = id_buf.into();
                        changed = true;
                    }
                    let mut fac = p.faction_id.to_string();
                    if ui.text_edit_singleline(&mut fac).changed() {
                        p.faction_id = sectorforge::ids::FactionId::new(fac.as_str());
                        changed = true;
                    }
                    changed |= ui.text_edit_singleline(&mut p.faction_kind).changed();
                    changed |= ui.text_edit_singleline(&mut p.name).changed();
                    changed |= ui.text_edit_singleline(&mut p.title).changed();
                    let mut traits_csv = p.traits.join(", ");
                    if ui.text_edit_singleline(&mut traits_csv).changed() {
                        p.traits = traits_csv
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        changed = true;
                    }
                    changed |= ui.text_edit_singleline(&mut p.agenda).changed();
                    if ui.button("✕").clicked() {
                        remove_idx = Some(idx);
                    }
                    ui.end_row();
                }
            });
    }
    if let Some(idx) = remove_idx {
        cfg.manual.remove(idx);
        changed = true;
    }
    if changed {
        on_catalog_edited(state);
    }
}

fn blank_manual_persona(seq: usize) -> Persona {
    Persona {
        id: format!("persona-manual-{seq:04}").into(),
        faction_id: sectorforge::ids::FactionId::new(""),
        faction_kind: String::new(),
        anchor: PersonaAnchor::System {
            system_id: sectorforge::ids::SystemId::new(""),
            slot: SystemSlot::Sovereign,
        },
        name: String::new(),
        title: String::new(),
        traits: Vec::new(),
        agenda: String::new(),
    }
}

// ── §PER1 kind pool editor ──────────────────────────────────────────────────

fn show_kind_pools_section(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§PER1 — per-faction-kind pools").strong());
    ensure_personae_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.personae.as_mut() else {
        return;
    };
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(
            Color32::DARK_GRAY,
            "Empty pool fields fall back to built-in defaults from src/personae.rs.",
        );
    });

    // Render one collapsing header per built-in kind plus any custom kinds
    // the user has authored.
    let mut kinds: Vec<String> = BUILTIN_KINDS.iter().map(|s| (*s).to_string()).collect();
    for key in cfg.kinds.keys() {
        if !kinds.contains(key) {
            kinds.push(key.clone());
        }
    }

    for kind in &kinds {
        let header = egui::CollapsingHeader::new(RichText::new(kind).strong())
            .id_salt(format!("per_kind_{kind}"))
            .default_open(false);
        header.show(ui, |ui| {
            let pools = cfg.kinds.entry(kind.clone()).or_default();
            changed |= pool_editor(ui, kind, pools);
            if ui
                .button(RichText::new("Reset to defaults").color(Color32::from_rgb(220, 170, 80)))
                .clicked()
            {
                cfg.kinds.remove(kind);
                changed = true;
            }
        });
    }

    ui.horizontal_wrapped(|ui| {
        ui.label("custom kind id:");
        let mut new_kind = String::new();
        let resp = ui.text_edit_singleline(&mut new_kind);
        if resp.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && !new_kind.trim().is_empty()
        {
            cfg.kinds
                .entry(new_kind.trim().to_string())
                .or_insert_with(KindPools::default);
            changed = true;
        }
    });

    if changed {
        on_catalog_edited(state);
    }
}

fn pool_editor(ui: &mut Ui, kind: &str, pools: &mut KindPools) -> bool {
    let mut changed = false;
    egui::Grid::new(format!("per_pool_{kind}"))
        .num_columns(2)
        .show(ui, |ui| {
            changed |= csv_row(ui, "name prefixes", &mut pools.name_prefixes);
            changed |= csv_row(ui, "name roots", &mut pools.name_roots);
            changed |= csv_row(ui, "name suffixes", &mut pools.name_suffixes);
            changed |= csv_row(ui, "single names", &mut pools.single_names);
            changed |= csv_row(ui, "titles", &mut pools.titles);
            changed |= csv_row(ui, "traits", &mut pools.traits);
        });
    changed
}

fn csv_row(ui: &mut Ui, label: &str, values: &mut Vec<String>) -> bool {
    ui.label(label);
    let mut csv = values.join(", ");
    let resp = ui.text_edit_multiline(&mut csv);
    let changed = resp.changed();
    if changed {
        *values = csv
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    ui.end_row();
    changed
}

// ── save row ────────────────────────────────────────────────────────────────

fn show_save_row(ui: &mut Ui, state: &mut BuilderState) {
    let has_catalog = state.data_catalogs.personae.is_some();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(has_catalog, egui::Button::new("Save personae.toml"))
            .clicked()
        {
            if state.config.inputs.personae.is_none() {
                state.config.inputs.personae = Some(DEFAULT_PERSONAE_PATH.into());
            }
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Save personae.toml failed: {e}"
                )));
            }
        }
        let path_label = state
            .config
            .inputs
            .personae
            .clone()
            .unwrap_or_else(|| format!("(unset; will write to {DEFAULT_PERSONAE_PATH})"));
        ui.colored_label(Color32::DARK_GRAY, path_label);
    });
}

// ── shared helpers ──────────────────────────────────────────────────────────

fn ensure_personae_catalog(state: &mut BuilderState) {
    if state.data_catalogs.personae.is_none() {
        state.data_catalogs.personae = Some(PersonaeConfig::default());
    }
    if state.config.inputs.personae.is_none() {
        state.config.inputs.personae = Some(DEFAULT_PERSONAE_PATH.into());
    }
}

fn ensure_personae_catalog_if_needed(state: &mut BuilderState) {
    if state.data_catalogs.personae.is_none() {
        state.data_catalogs.personae = Some(PersonaeConfig::default());
    }
}

fn on_catalog_edited(state: &mut BuilderState) {
    state.dirty = true;
    if let Some(rel) = state.config.inputs.personae.clone() {
        state.dirty_files.insert(rel);
    } else {
        state.dirty_files.insert(DEFAULT_PERSONAE_PATH.into());
    }
    state.mark_validation_dirty();
    if state.personae_auto_recompute {
        state.recompute_personae();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::personae::{DominanceTier, PersonaeConfig};

    #[test]
    fn ensure_catalog_seeds_defaults_and_path() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        assert!(state.data_catalogs.personae.is_none());
        ensure_personae_catalog(&mut state);
        assert!(state.data_catalogs.personae.is_some());
        assert_eq!(
            state.config.inputs.personae.as_deref(),
            Some(DEFAULT_PERSONAE_PATH)
        );
    }

    #[test]
    fn recompute_personae_publishes_report() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.personae = Some(PersonaeConfig {
            min_world_dominance: DominanceTier::Presence,
            ..Default::default()
        });
        state.recompute_personae();
        assert!(state.personae_report.is_some());
    }
}
