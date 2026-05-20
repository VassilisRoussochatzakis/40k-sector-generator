//! High-level faction overview/edit surface.
//!
//! This view deliberately stays above subfaction detail: it shows sector-level
//! faction rollups and offers broad summary edits for names, kinds,
//! dispositions, and large presence sets.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Color32, RichText, Sense, Stroke, Ui};

use crate::ids::{FactionId, SystemId, WorldId};
use crate::sector_model::{GeneratedFaction, GeneratedSector};

use super::editor::state::empty_faction;
use super::palette::{
    contrast_text, faction_style, FactionBorder, FactionStyle, PANEL_BG, TEXT, TEXT_DIM,
};

#[derive(Debug, Clone, Default)]
struct PresenceStats {
    systems: BTreeSet<SystemId>,
    worlds: BTreeSet<WorldId>,
    subfactions: BTreeMap<FactionId, PresenceStats>,
}

impl PresenceStats {
    fn system_count(&self) -> usize {
        self.systems.len()
    }

    fn world_count(&self) -> usize {
        self.worlds.len()
    }
}

pub fn show_readonly(ui: &mut Ui, sector: &GeneratedSector) {
    show_header(ui, sector, false);
    ui.add_space(8.0);
    show_kind_summary(ui, sector);
    ui.add_space(10.0);
    show_table_header(ui, false);

    let observed = observed_presence(sector);
    let mut order = faction_order(sector);
    for i in order.drain(..) {
        show_readonly_row(
            ui,
            &sector.factions[i],
            observed.get(&sector.factions[i].id),
        );
    }
}

pub fn show_editor(ui: &mut Ui, sector: &mut GeneratedSector) -> bool {
    show_header(ui, sector, true);
    ui.add_space(8.0);
    show_kind_summary(ui, sector);
    ui.add_space(10.0);

    let mut dirty = false;
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(RichText::new("+ ADD FACTION").monospace())
            .clicked()
        {
            let id = next_faction_id(sector);
            sector.factions.push(empty_faction(id));
            dirty = true;
        }
        if ui
            .button(RichText::new("REBUILD ALL FROM WORLD DATA").monospace())
            .on_hover_text("refresh summary presences from per-world faction records")
            .clicked()
        {
            rebuild_all_summaries_from_world_data(sector);
            dirty = true;
        }
        if ui
            .button(RichText::new("SORT BY NAME").monospace())
            .on_hover_text("sort sector faction list by id")
            .clicked()
        {
            sector.factions.sort_by(|a, b| a.id.cmp(&b.id));
            dirty = true;
        }
    });

    ui.add_space(10.0);
    show_table_header(ui, true);

    let observed = observed_presence(sector);
    let all_systems: Vec<SystemId> = sector.systems.iter().map(|s| s.id.clone()).collect();
    let all_worlds: Vec<WorldId> = sector
        .systems
        .iter()
        .flat_map(|s| s.worlds.iter().map(|w| w.id.clone()))
        .collect();
    let mut remove: Option<FactionId> = None;
    let mut order = faction_order(sector);

    for i in order.drain(..) {
        let fac_id = sector.factions[i].id.clone();
        let obs = observed.get(&fac_id).cloned().unwrap_or_default();
        let fac = &mut sector.factions[i];
        dirty |= show_edit_row(ui, fac, &obs, &all_systems, &all_worlds);
        if ui
            .button(RichText::new("DELETE FACTION").monospace())
            .on_hover_text("remove this faction and sector references to it")
            .clicked()
        {
            remove = Some(fac_id);
        }
        ui.separator();
    }

    if let Some(id) = remove {
        remove_faction_everywhere(sector, &id);
        dirty = true;
    }

    dirty
}

fn show_header(ui: &mut Ui, sector: &GeneratedSector, edit_mode: bool) {
    ui.label(
        RichText::new("FACTIONS")
            .color(TEXT)
            .monospace()
            .strong()
            .size(18.0),
    );
    let world_count: usize = sector.systems.iter().map(|s| s.worlds.len()).sum();
    ui.label(
        RichText::new(format!(
            "{} factions - {} systems - {} worlds{}",
            sector.factions.len(),
            sector.systems.len(),
            world_count,
            if edit_mode { " - edit mode" } else { "" }
        ))
        .color(TEXT_DIM)
        .monospace(),
    );
}

fn show_kind_summary(ui: &mut Ui, sector: &GeneratedSector) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &sector.factions {
        *counts.entry(f.kind.as_str()).or_default() += 1;
    }
    if counts.is_empty() {
        ui.label(
            RichText::new("no factions in sector")
                .color(TEXT_DIM)
                .monospace(),
        );
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for (kind, count) in counts {
            ui.label(
                RichText::new(format!("{} {}", kind.to_uppercase(), count))
                    .color(TEXT_DIM)
                    .monospace(),
            );
        }
    });
}

fn show_table_header(ui: &mut Ui, edit_mode: bool) {
    ui.horizontal(|ui| {
        fixed(ui, 28.0, "");
        fixed(ui, 180.0, "NAME");
        fixed(ui, 150.0, "ID");
        fixed(ui, 120.0, "KIND");
        fixed(ui, 120.0, "DISP");
        fixed(ui, 98.0, "SUMMARY");
        fixed(ui, 98.0, "WORLD DATA");
        fixed(ui, 80.0, "POWER");
        if edit_mode {
            fixed(ui, 160.0, "BULK");
        }
    });
    ui.separator();
}

fn show_readonly_row(ui: &mut Ui, fac: &GeneratedFaction, observed: Option<&PresenceStats>) {
    let style = faction_style(&fac.kind, fac.id.as_str(), &fac.disposition);
    let observed = observed.cloned().unwrap_or_default();
    ui.horizontal(|ui| {
        faction_chip(ui, style);
        fixed_text(ui, 180.0, &fac.name.to_uppercase(), TEXT);
        fixed_text(ui, 150.0, fac.id.as_str(), TEXT_DIM);
        fixed_text(ui, 120.0, &fac.kind, TEXT_DIM);
        fixed_text(ui, 120.0, &fac.disposition, TEXT_DIM);
        fixed_text(
            ui,
            98.0,
            &format!(
                "{}S {}W",
                fac.system_presence.len(),
                fac.world_presence.len()
            ),
            TEXT,
        );
        fixed_text(
            ui,
            98.0,
            &format!("{}S {}W", observed.system_count(), observed.world_count()),
            TEXT_DIM,
        );
        fixed_text(
            ui,
            80.0,
            &format!("{:.0}", fac.power.total_projection()),
            TEXT_DIM,
        );
    });
    if !fac.subfactions.is_empty() {
        ui.label(
            RichText::new(format!("  {} subfactions hidden", fac.subfactions.len()))
                .color(TEXT_DIM)
                .monospace(),
        );
    }
}

fn show_edit_row(
    ui: &mut Ui,
    fac: &mut GeneratedFaction,
    observed: &PresenceStats,
    all_systems: &[SystemId],
    all_worlds: &[WorldId],
) -> bool {
    let mut dirty = false;
    let style = faction_style(&fac.kind, fac.id.as_str(), &fac.disposition);

    ui.horizontal(|ui| {
        faction_chip(ui, style);
        fixed_text(ui, 180.0, &fac.name.to_uppercase(), TEXT);
        fixed_text(ui, 150.0, fac.id.as_str(), TEXT_DIM);
        fixed_text(ui, 120.0, &fac.kind, TEXT_DIM);
        fixed_text(ui, 120.0, &fac.disposition, TEXT_DIM);
        fixed_text(
            ui,
            98.0,
            &format!(
                "{}S {}W",
                fac.system_presence.len(),
                fac.world_presence.len()
            ),
            TEXT,
        );
        fixed_text(
            ui,
            98.0,
            &format!("{}S {}W", observed.system_count(), observed.world_count()),
            TEXT_DIM,
        );
        fixed_text(
            ui,
            80.0,
            &format!("{:.0}", fac.power.total_projection()),
            TEXT_DIM,
        );
    });

    ui.horizontal_wrapped(|ui| {
        field_label(ui, "NAME");
        dirty |= text_edit(ui, &mut fac.name, 180.0);
        field_label(ui, "KIND");
        dirty |= text_edit(ui, &mut fac.kind, 140.0);
        field_label(ui, "DISP");
        dirty |= text_edit(ui, &mut fac.disposition, 140.0);
    });

    ui.horizontal_wrapped(|ui| {
        if ui.button(RichText::new("ALL SYS").monospace()).clicked() {
            fac.system_presence = all_systems.to_vec();
            dirty = true;
        }
        if ui.button(RichText::new("NO SYS").monospace()).clicked() {
            fac.system_presence.clear();
            for sf in &mut fac.subfactions {
                sf.system_presence.clear();
            }
            dirty = true;
        }
        if ui.button(RichText::new("ALL WORLDS").monospace()).clicked() {
            fac.world_presence = all_worlds.to_vec();
            dirty = true;
        }
        if ui.button(RichText::new("NO WORLDS").monospace()).clicked() {
            fac.world_presence.clear();
            for sf in &mut fac.subfactions {
                sf.world_presence.clear();
            }
            dirty = true;
        }
        if ui
            .button(RichText::new("FROM WORLD DATA").monospace())
            .on_hover_text("replace summary presence counts using per-world faction records")
            .clicked()
        {
            fac.system_presence = observed.systems.iter().cloned().collect();
            fac.world_presence = observed.worlds.iter().cloned().collect();
            for sf in &mut fac.subfactions {
                if let Some(stats) = observed.subfactions.get(&sf.id) {
                    sf.system_presence = stats.systems.iter().cloned().collect();
                    sf.world_presence = stats.worlds.iter().cloned().collect();
                } else {
                    sf.system_presence.clear();
                    sf.world_presence.clear();
                }
            }
            dirty = true;
        }
        if !fac.subfactions.is_empty() {
            ui.label(
                RichText::new(format!("{} subfactions hidden", fac.subfactions.len()))
                    .color(TEXT_DIM)
                    .monospace(),
            );
        }
    });

    dirty
}

fn faction_order(sector: &GeneratedSector) -> Vec<usize> {
    let mut order: Vec<usize> = (0..sector.factions.len()).collect();
    order.sort_by(|a, b| {
        let fa = &sector.factions[*a];
        let fb = &sector.factions[*b];
        fb.power
            .total_projection()
            .partial_cmp(&fa.power.total_projection())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| fa.name.cmp(&fb.name))
            .then_with(|| fa.id.cmp(&fb.id))
    });
    order
}

fn observed_presence(sector: &GeneratedSector) -> BTreeMap<FactionId, PresenceStats> {
    let mut out: BTreeMap<FactionId, PresenceStats> = BTreeMap::new();
    for sys in &sector.systems {
        for world in &sys.worlds {
            for presence in &world.factions {
                let stats = out.entry(presence.faction_id.clone()).or_default();
                stats.systems.insert(sys.id.clone());
                stats.worlds.insert(world.id.clone());
                if let Some(sub_id) = &presence.subfaction_id {
                    let sub = stats.subfactions.entry(sub_id.clone()).or_default();
                    sub.systems.insert(sys.id.clone());
                    sub.worlds.insert(world.id.clone());
                }
            }
        }
    }
    out
}

fn rebuild_all_summaries_from_world_data(sector: &mut GeneratedSector) {
    let observed = observed_presence(sector);
    for fac in &mut sector.factions {
        if let Some(stats) = observed.get(&fac.id) {
            fac.system_presence = stats.systems.iter().cloned().collect();
            fac.world_presence = stats.worlds.iter().cloned().collect();
            for sf in &mut fac.subfactions {
                if let Some(sub) = stats.subfactions.get(&sf.id) {
                    sf.system_presence = sub.systems.iter().cloned().collect();
                    sf.world_presence = sub.worlds.iter().cloned().collect();
                } else {
                    sf.system_presence.clear();
                    sf.world_presence.clear();
                }
            }
        } else {
            fac.system_presence.clear();
            fac.world_presence.clear();
            for sf in &mut fac.subfactions {
                sf.system_presence.clear();
                sf.world_presence.clear();
            }
        }
    }
}

fn remove_faction_everywhere(sector: &mut GeneratedSector, id: &FactionId) {
    sector.factions.retain(|f| &f.id != id);
    sector.relations.pairs.retain(|p| &p.a != id && &p.b != id);
    sector.power_projection.by_faction.remove(id);

    for cell in &mut sector.influence_field.cells {
        cell.top.retain(|(fid, _)| fid != id);
        if cell.dominant.as_ref() == Some(id) {
            if let Some((fid, score)) = cell.top.first() {
                cell.dominant = Some(fid.clone());
                cell.score = *score;
            } else {
                cell.dominant = None;
                cell.score = 0;
            }
        }
    }
    sector
        .influence_field
        .bands
        .retain(|band| &band.faction_id != id);

    for route in &mut sector.routes {
        route.controls.retain(|control| &control.faction_id != id);
    }

    for sys in &mut sector.systems {
        sys.primary_factions.retain(|fid| fid != id);
        sys.control.top_factions.retain(|sf| &sf.faction_id != id);
        clear_option(&mut sys.control.dominant, id);
        clear_option(&mut sys.control.sovereign, id);
        clear_option(&mut sys.control.orbital_controller, id);
        clear_option(&mut sys.control.economic_hegemon, id);
        clear_option(&mut sys.control.hidden_master, id);
        sys.orbital_assets.retain(|asset| &asset.faction_id != id);
        clear_option(&mut sys.blockade.blockader, id);
        clear_option(&mut sys.blockade.besieged, id);
        clear_conflict(&mut sys.conflict, id);
        sys.archetype.imperial_co_sovereigns.retain(|fid| fid != id);

        for world in &mut sys.worlds {
            world.factions.retain(|presence| &presence.faction_id != id);
            world.claims.retain(|claim| &claim.faction_id != id);
            clear_option(&mut world.control.dominant, id);
            clear_option(&mut world.control.sovereign, id);
            clear_option(&mut world.control.occupier, id);
            clear_option(&mut world.control.economic_hegemon, id);
            clear_option(&mut world.control.popular_authority, id);
            clear_option(&mut world.control.hidden_master, id);
            clear_conflict(&mut world.conflict, id);
        }
    }
}

fn clear_conflict(conflict: &mut crate::conflict::ConflictState, id: &FactionId) {
    clear_option(&mut conflict.attacker, id);
    clear_option(&mut conflict.defender, id);
    clear_option(&mut conflict.visible_controller, id);
}

fn clear_option(slot: &mut Option<FactionId>, id: &FactionId) {
    if slot.as_ref() == Some(id) {
        *slot = None;
    }
}

fn next_faction_id(sector: &GeneratedSector) -> FactionId {
    let used: BTreeSet<&str> = sector.factions.iter().map(|f| f.id.as_str()).collect();
    for n in 1.. {
        let id = format!("faction_{n}");
        if !used.contains(id.as_str()) {
            return FactionId::new(id);
        }
    }
    unreachable!("unbounded faction id search exhausted");
}

fn fixed(ui: &mut Ui, width: f32, text: &str) {
    fixed_text(ui, width, text, TEXT_DIM);
}

fn fixed_text(ui: &mut Ui, width: f32, text: &str, color: Color32) {
    ui.add_sized(
        [width, 18.0],
        egui::Label::new(RichText::new(text).color(color).monospace()),
    );
}

fn field_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(TEXT_DIM).monospace());
}

fn text_edit(ui: &mut Ui, value: &mut String, width: f32) -> bool {
    ui.add_sized(
        [width, 22.0],
        egui::TextEdit::singleline(value).font(egui::FontId::monospace(12.0)),
    )
    .changed()
}

fn faction_chip(ui: &mut Ui, style: FactionStyle) {
    let size = egui::vec2(20.0, 20.0);
    let (rect, _resp) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    let bg = match style.border {
        FactionBorder::Dotted => egui::Color32::from_rgba_unmultiplied(
            style.fill.r(),
            style.fill.g(),
            style.fill.b(),
            150,
        ),
        _ => style.fill,
    };
    painter.rect_filled(rect, 3.0, bg);
    match style.border {
        FactionBorder::Clean => {
            painter.rect_stroke(rect, 3.0, Stroke::new(1.2, style.accent));
        }
        FactionBorder::Jagged => {
            painter.rect_stroke(rect, 3.0, Stroke::new(2.4, style.accent));
        }
        FactionBorder::Dotted => {
            painter.rect_stroke(rect, 3.0, Stroke::new(1.0, PANEL_BG));
        }
        FactionBorder::Thin => {
            painter.rect_stroke(rect, 3.0, Stroke::new(0.8, style.accent));
        }
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        style.glyph.to_string(),
        egui::FontId::monospace(13.0),
        contrast_text(style.fill),
    );
}
