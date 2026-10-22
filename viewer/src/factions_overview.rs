//! High-level faction overview.
//!
//! This view deliberately stays above subfaction/force detail: it shows sector-level
//! faction rollups and offers broad summary edits for names, kinds,
//! dispositions, and large presence sets.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Color32, RichText, Ui};

use sectorforge::ids::{FactionId, SystemId, WorldId};
use sectorforge::sector_model::{GeneratedFaction, GeneratedSector};

use super::palette::{self, draw_faction_chip, faction_style};

#[derive(Debug, Clone, Default)]
struct PresenceStats {
    systems: BTreeSet<SystemId>,
    worlds: BTreeSet<WorldId>,
}

impl PresenceStats {
    fn system_count(&self) -> usize {
        self.systems.len()
    }

    fn world_count(&self) -> usize {
        self.worlds.len()
    }
}

pub(crate) fn show_readonly(ui: &mut Ui, sector: &GeneratedSector) {
    show_header(ui, sector, false);
    ui.add_space(8.0);
    show_kind_summary(ui, sector);
    ui.add_space(10.0);
    show_table_header(ui, false);

    let observed = observed_presence(sector);
    let order = faction_order(sector);
    for i in order {
        show_readonly_row(
            ui,
            &sector.factions[i],
            observed.get(&sector.factions[i].id),
        );
    }
}

fn show_header(ui: &mut Ui, sector: &GeneratedSector, edit_mode: bool) {
    ui.label(
        RichText::new("FACTIONS")
            .color(palette::chrome_text())
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
        .color(palette::chrome_text_dim()),
    );
}

fn show_kind_summary(ui: &mut Ui, sector: &GeneratedSector) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &sector.factions {
        *counts.entry(f.kind.as_slug()).or_default() += 1;
    }
    if counts.is_empty() {
        ui.label(RichText::new("no factions in sector").color(palette::chrome_text_dim()));
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for (kind, count) in counts {
            ui.label(
                RichText::new(format!("{} {}", kind.to_uppercase(), count))
                    .color(palette::chrome_text_dim()),
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
    let style = faction_style(fac.kind.as_slug(), fac.id.as_str(), &fac.disposition);
    let observed = observed.cloned().unwrap_or_default();
    ui.horizontal(|ui| {
        draw_faction_chip(ui, style);
        fixed_text(ui, 180.0, &fac.name.to_uppercase(), palette::chrome_text());
        fixed_text(ui, 150.0, fac.id.as_str(), palette::chrome_text_dim());
        fixed_text(ui, 120.0, fac.kind.as_slug(), palette::chrome_text_dim());
        fixed_text(ui, 120.0, &fac.disposition, palette::chrome_text_dim());
        fixed_text(
            ui,
            98.0,
            &format!(
                "{}S {}W",
                fac.system_presence.len(),
                fac.world_presence.len()
            ),
            palette::chrome_text(),
        );
        fixed_text(
            ui,
            98.0,
            &format!("{}S {}W", observed.system_count(), observed.world_count()),
            palette::chrome_text_dim(),
        );
        fixed_text(
            ui,
            80.0,
            &format!("{:.0}", fac.power.total_projection()),
            palette::chrome_text_dim(),
        );
    });
    if !fac.subfactions.is_empty() {
        let force_count: usize = fac.subfactions.iter().map(|sf| sf.forces.len()).sum();
        ui.label(
            RichText::new(format!(
                "  {} subfactions / {} forces hidden",
                fac.subfactions.len(),
                force_count
            ))
            .color(palette::chrome_text_dim()),
        );
    }
}

fn faction_order(sector: &GeneratedSector) -> Vec<usize> {
    let mut order: Vec<usize> = (0..sector.factions.len()).collect();
    order.sort_by(|a, b| {
        let fa = &sector.factions[*a];
        let fb = &sector.factions[*b];
        fb.power
            .total_projection()
            .total_cmp(&fa.power.total_projection())
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
            }
        }
    }
    out
}

fn fixed(ui: &mut Ui, width: f32, text: &str) {
    fixed_text(ui, width, text, palette::chrome_text_dim());
}

fn fixed_text(ui: &mut Ui, width: f32, text: &str, color: Color32) {
    ui.add_sized(
        [width, 18.0],
        egui::Label::new(RichText::new(text).color(color)),
    );
}
