//! High-level faction overview/edit surface.
//!
//! This view deliberately stays above subfaction/force detail: it shows sector-level
//! faction rollups and offers broad summary edits for names, kinds,
//! dispositions, and large presence sets.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum FactionDesignerError {
    #[error("validation error: {field} - {message}")]
    Validation { field: String, message: String },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config error: {0}")]
    Config(String),
}

use egui::{Color32, DragValue, RichText, Ui};
use rfd::FileDialog;

use sectorforge::factions::{FactionDef, FactionsFile};
use sectorforge::ids::{FactionId, SystemId, WorldId};
use sectorforge::sector_model::{GeneratedFaction, GeneratedSector};

use super::palette::{self, draw_faction_chip, faction_style};

#[derive(Debug, Clone, Default)]
struct PresenceStats {
    systems: BTreeSet<SystemId>,
    worlds: BTreeSet<WorldId>,
    subfactions: BTreeMap<FactionId, PresenceStats>,
    forces: BTreeMap<FactionId, PresenceStats>,
}

impl PresenceStats {
    fn system_count(&self) -> usize {
        self.systems.len()
    }

    fn world_count(&self) -> usize {
        self.worlds.len()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FactionDesignerState {
    rows: Vec<DesignerFactionRow>,
    selected_preset: usize,
    new_id: String,
    new_name: String,
    new_kind: String,
    new_disposition: String,
    new_weight: f64,
    new_world_types: String,
    new_governments: String,
    new_features: String,
    status: String,
}

impl Default for FactionDesignerState {
    fn default() -> Self {
        let p = OVERALL_PRESETS[0];
        Self {
            rows: Vec::new(),
            selected_preset: 0,
            new_id: p.kind.to_string(),
            new_name: p.name.to_string(),
            new_kind: p.kind.to_string(),
            new_disposition: p.disposition.to_string(),
            new_weight: p.weight,
            new_world_types: join_static_list(p.world_types),
            new_governments: join_static_list(p.governments),
            new_features: join_static_list(p.features),
            status: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct DesignerFactionRow {
    id: String,
    name: String,
    kind: String,
    disposition: String,
    weight: f64,
    world_types: String,
    governments: String,
    features: String,
}

#[derive(Debug, Clone, Copy)]
struct OverallPreset {
    label: &'static str,
    kind: &'static str,
    name: &'static str,
    disposition: &'static str,
    weight: f64,
    world_types: &'static [&'static str],
    governments: &'static [&'static str],
    features: &'static [&'static str],
}

const OVERALL_PRESETS: &[OverallPreset] = &[
    OverallPreset {
        label: "Imperial",
        kind: "imperial",
        name: "Imperial Administration",
        disposition: "lawful",
        weight: 10.0,
        world_types: &["HiveWorld", "BastionWorld", "IndustrialWorld"],
        governments: &[
            "MilitaryGovernor",
            "MagistrateCouncil",
            "EcclesiarchicalAppointee",
        ],
        features: &["AdministrativeHub", "PoliceState", "ImportantShrine"],
    },
    OverallPreset {
        label: "Mechanicus",
        kind: "mechanicus",
        name: "Adeptus Mechanicus",
        disposition: "insular",
        weight: 7.0,
        world_types: &["ForgeWorld", "ResearchStation", "IndustrialWorld"],
        governments: &["MechanicusForgeLord", "ExploratorAuthority", "Hereteks"],
        features: &["ForbiddenTech", "GreatWork", "HeavyIndustry"],
    },
    OverallPreset {
        label: "Ecclesiarchy",
        kind: "ecclesiarchy",
        name: "Adeptus Ministorum",
        disposition: "zealous",
        weight: 5.0,
        world_types: &["ShrineWorld", "FeudalWorld", "ImperialWorld"],
        governments: &["EcclesiarchicalAppointee", "LocalReligiousAuthorities"],
        features: &["ImportantShrine", "PilgrimageSite", "Missionaries"],
    },
    OverallPreset {
        label: "Inquisition",
        kind: "inquisition",
        name: "Inquisition Cell",
        disposition: "secretive",
        weight: 2.0,
        world_types: &["HiveWorld", "ShrineWorld", "DeathWorld"],
        governments: &[],
        features: &["InquisitionOutpost", "WitchHunt", "SecretMasters"],
    },
    OverallPreset {
        label: "Astartes",
        kind: "adeptus_astartes",
        name: "Space Marine Chapter",
        disposition: "lawful",
        weight: 3.0,
        world_types: &["BastionWorld", "DeathWorld", "FeudalWorld"],
        governments: &["MilitaryGovernor", "WarriorAristocracy"],
        features: &["VastFortresses", "ScholaProgenium", "WarZone"],
    },
    OverallPreset {
        label: "Guard",
        kind: "imperial_guard",
        name: "Astra Militarum Command",
        disposition: "lawful",
        weight: 4.0,
        world_types: &["BastionWorld", "HiveWorld", "ImperialWorld"],
        governments: &["MilitaryGovernor", "MagistrateCouncil"],
        features: &["VastFortresses", "MartialLaw", "NavalOutpost"],
    },
    OverallPreset {
        label: "Merchant",
        kind: "merchant",
        name: "Merchant Combine",
        disposition: "neutral",
        weight: 4.0,
        world_types: &["HiveWorld", "PleasureWorld", "Orbital"],
        governments: &["GuildsCombine", "Megacorporations", "RogueTraderDynasty"],
        features: &["TradeHub", "Freeport", "SoleSuppliers"],
    },
    OverallPreset {
        label: "Criminal",
        kind: "criminal",
        name: "Underworld Cartel",
        disposition: "hostile",
        weight: 3.0,
        world_types: &["HiveWorld", "PenalWorld", "Orbital"],
        governments: &["InfractionistGang", "CorruptAristocrats", "Warlords"],
        features: &["PowerfulCriminals", "SecretMasters", "TheSilentTrade"],
    },
    OverallPreset {
        label: "Rebel",
        kind: "rebel",
        name: "Secessionist Front",
        disposition: "hostile",
        weight: 3.0,
        world_types: &["ImperialWorld", "IndustrialWorld", "FrontierWorld"],
        governments: &["RevolutionaryJunta", "Communards", "Demagogue"],
        features: &["PopularUprising", "Separatists", "CivilWar"],
    },
    OverallPreset {
        label: "Chaos Marines",
        kind: "chaos_space_marine",
        name: "Chaos Space Marine Warband",
        disposition: "hostile",
        weight: 3.0,
        world_types: &["DeadWorld", "WarpLostWorld", "DeathWorld"],
        governments: &["ChaosCult", "Hereteks", "Warlords"],
        features: &["DaemonicCorruption", "ChaosCultists", "WarZone"],
    },
    OverallPreset {
        label: "Daemon",
        kind: "daemon",
        name: "Daemonic Incursion",
        disposition: "hostile",
        weight: 2.0,
        world_types: &["WarpLostWorld", "DeadWorld", "ShrineWorld"],
        governments: &["ChaosCult", "HereticalImperialCult"],
        features: &["DaemonicCorruption", "WarpPhenomena", "ImpendingDoom"],
    },
    OverallPreset {
        label: "Genestealer Cult",
        kind: "genestealer_cult",
        name: "Genestealer Cult",
        disposition: "secretive",
        weight: 3.0,
        world_types: &["HiveWorld", "IndustrialWorld", "ExtractiveColony"],
        governments: &["PuppetGovernment", "ShadowyPsykerCabal", "Demagogue"],
        features: &["XenosInfiltrators", "SecretMasters", "MutantHordes"],
    },
    OverallPreset {
        label: "Aeldari",
        kind: "aeldari",
        name: "Aeldari Warhost",
        disposition: "secretive",
        weight: 2.0,
        world_types: &["Worldship", "XenosWorld", "DeadWorld"],
        governments: &["XenosOverlords", "TraditionalOligarchy"],
        features: &["XenoRuins", "AncientTombs", "SecretMasters"],
    },
    OverallPreset {
        label: "Drukhari",
        kind: "drukhari",
        name: "Drukhari Kabal",
        disposition: "hostile",
        weight: 2.0,
        world_types: &["Orbital", "HiveWorld", "FrontierWorld"],
        governments: &["InfractionistGang", "Warlords", "XenosOverlords"],
        features: &["TheSilentTrade", "MassPanic", "PowerfulCriminals"],
    },
    OverallPreset {
        label: "Ork",
        kind: "ork",
        name: "Ork Waaagh!",
        disposition: "hostile",
        weight: 4.0,
        world_types: &["DeathWorld", "FrontierWorld", "XenosWorld"],
        governments: &["Warlords", "ClansTribes", "WarriorAristocracy"],
        features: &["HostileXenos", "WarZone", "DangerousWildlife"],
    },
    OverallPreset {
        label: "Tau",
        kind: "tau",
        name: "T'au Expedition",
        disposition: "neutral",
        weight: 3.0,
        world_types: &["FrontierWorld", "AgriWorld", "IndustrialWorld"],
        governments: &["PuppetGovernment", "Megacorporations", "XenosOverlords"],
        features: &["FriendlyXenos", "ForeignControl", "LocalTech"],
    },
    OverallPreset {
        label: "Necron",
        kind: "necron",
        name: "Necron Dynasty",
        disposition: "hostile",
        weight: 2.0,
        world_types: &["TombWorld", "DeadWorld", "PlanetaryMonument"],
        governments: &["None", "XenosOverlords"],
        features: &["AncientTombs", "SealedMenace", "ArchaeotechRuins"],
    },
    OverallPreset {
        label: "Tyranid",
        kind: "tyranid",
        name: "Tyranid Splinter Fleet",
        disposition: "hostile",
        weight: 2.0,
        world_types: &["DeathWorld", "XenosWorld", "AgriWorld"],
        governments: &["None", "XenosOverlords"],
        features: &["HostileXenos", "MutantHordes", "ImpendingDoom"],
    },
    OverallPreset {
        label: "Votann",
        kind: "leagues_of_votann",
        name: "Kin Hold",
        disposition: "neutral",
        weight: 2.0,
        world_types: &["ExtractiveColony", "Asteroid", "IndustrialWorld"],
        governments: &["GuildsCombine", "TraditionalOligarchy"],
        features: &["HeavyMining", "LocalTech", "SoleSuppliers"],
    },
    OverallPreset {
        label: "Minor Xenos",
        kind: "minor_xenos",
        name: "Minor Xenos Enclave",
        disposition: "hostile",
        weight: 2.0,
        world_types: &["XenosWorld", "FrontierWorld", "DeadWorld"],
        governments: &["XenosOverlords", "ClansTribes"],
        features: &["PrimitiveXenos", "HostileXenos", "XenoRuins"],
    },
    OverallPreset {
        label: "Custom",
        kind: "custom_faction",
        name: "Custom Faction",
        disposition: "neutral",
        weight: 1.0,
        world_types: &[],
        governments: &[],
        features: &[],
    },
];

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

pub fn show_editor(ui: &mut Ui, sector: &mut GeneratedSector) -> bool {
    show_header(ui, sector, true);
    ui.add_space(8.0);
    show_kind_summary(ui, sector);
    ui.add_space(10.0);

    let mut dirty = false;
    ui.horizontal_wrapped(|ui| {
        if ui.button(RichText::new("+ ADD FACTION")).clicked() {
            let id = next_faction_id(sector);
            sector
                .factions
                .push(crate::editor::state::empty_faction(&id));
            dirty = true;
        }
        if ui
            .button(RichText::new("REBUILD ALL FROM WORLD DATA"))
            .on_hover_text("refresh summary presences from per-world faction records")
            .clicked()
        {
            rebuild_all_summaries_from_world_data(sector);
            dirty = true;
        }
        if ui
            .button(RichText::new("SORT BY NAME"))
            .on_hover_text("sort sector faction list by id")
            .clicked()
        {
            sector.factions.sort_unstable_by(|a, b| a.id.cmp(&b.id));
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
    let order = faction_order(sector);

    for i in order {
        let fac_id = sector.factions[i].id.clone();
        let obs = observed.get(&fac_id).cloned().unwrap_or_default();
        let fac = &mut sector.factions[i];
        dirty |= show_edit_row(ui, fac, &obs, &all_systems, &all_worlds);
        if ui
            .button(RichText::new("DELETE FACTION"))
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

pub(crate) fn show_designer(
    ui: &mut Ui,
    sector: &GeneratedSector,
    state: &mut FactionDesignerState,
    project_dir: Option<&Path>,
) {
    ui.label(
        RichText::new("FACTION DESIGNER")
            .color(palette::chrome_text())
            .strong()
            .size(18.0),
    );
    ui.label(
        RichText::new(format!(
            "{} designer rows - {} output factions available",
            state.rows.len(),
            sector.factions.len()
        ))
        .color(palette::chrome_text_dim()),
    );
    ui.add_space(8.0);

    ui.horizontal_wrapped(|ui| {
        if ui.button(RichText::new("START SCRATCH")).clicked() {
            state.rows.clear();
            state.status = "designer rows cleared".into();
        }
        if ui
            .button(RichText::new("REPLACE FROM OUTPUT"))
            .on_hover_text("build designer rows from loaded sector forces")
            .clicked()
        {
            state.rows = designer_rows_from_sector_output(sector);
            state.status = format!("loaded {} rows from output", state.rows.len());
        }
        if ui
            .button(RichText::new("APPEND FROM OUTPUT"))
            .on_hover_text("append generated forces not already in the designer rows")
            .clicked()
        {
            let added = append_output_rows(state, sector);
            state.status = format!("appended {added} output rows");
        }
        if ui
            .button(RichText::new("SAVE TOML..."))
            .on_hover_text("save designer rows as data/factions/factions.toml schema")
            .clicked()
        {
            match choose_and_save_designer_toml(state, sector, project_dir) {
                Ok(Some(path)) => state.status = format!("saved {}", path),
                Ok(None) => {}
                Err(e) => state.status = format!("save failed: {e}"),
            }
        }
    });
    if !state.status.is_empty() {
        ui.label(RichText::new(&state.status).color(egui::Color32::from_rgb(235, 200, 90)));
    }

    ui.add_space(10.0);
    show_designer_builder(ui, state);
    ui.add_space(10.0);
    show_designer_rows(ui, state);
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
        *counts.entry(f.kind.as_ref()).or_default() += 1;
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
    let style = faction_style(&fac.kind, fac.id.as_str(), &fac.disposition);
    let observed = observed.cloned().unwrap_or_default();
    ui.horizontal(|ui| {
        draw_faction_chip(ui, style);
        fixed_text(ui, 180.0, &fac.name.to_uppercase(), palette::chrome_text());
        fixed_text(ui, 150.0, fac.id.as_str(), palette::chrome_text_dim());
        fixed_text(ui, 120.0, &fac.kind, palette::chrome_text_dim());
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
        draw_faction_chip(ui, style);
        fixed_text(ui, 180.0, &fac.name.to_uppercase(), palette::chrome_text());
        fixed_text(ui, 150.0, fac.id.as_str(), palette::chrome_text_dim());
        fixed_text(ui, 120.0, &fac.kind, palette::chrome_text_dim());
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

    ui.horizontal_wrapped(|ui| {
        field_label(ui, "NAME");
        dirty |= text_edit(ui, &mut fac.name, 180.0);
        field_label(ui, "KIND");
        dirty |= text_edit(ui, &mut fac.kind, 140.0);
        field_label(ui, "DISP");
        dirty |= text_edit(ui, &mut fac.disposition, 140.0);
    });

    ui.horizontal_wrapped(|ui| {
        if ui.button(RichText::new("ALL SYS")).clicked() {
            fac.system_presence = all_systems.to_vec();
            dirty = true;
        }
        if ui.button(RichText::new("NO SYS")).clicked() {
            fac.system_presence.clear();
            for sf in &mut fac.subfactions {
                sf.system_presence.clear();
                for force in &mut sf.forces {
                    force.system_presence.clear();
                }
            }
            dirty = true;
        }
        if ui.button(RichText::new("ALL WORLDS")).clicked() {
            fac.world_presence = all_worlds.to_vec();
            dirty = true;
        }
        if ui.button(RichText::new("NO WORLDS")).clicked() {
            fac.world_presence.clear();
            for sf in &mut fac.subfactions {
                sf.world_presence.clear();
                for force in &mut sf.forces {
                    force.world_presence.clear();
                }
            }
            dirty = true;
        }
        if ui
            .button(RichText::new("FROM WORLD DATA"))
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
                for force in &mut sf.forces {
                    if let Some(stats) = observed
                        .subfactions
                        .get(&sf.id)
                        .and_then(|sub| sub.forces.get(&force.id))
                    {
                        force.system_presence = stats.systems.iter().cloned().collect();
                        force.world_presence = stats.worlds.iter().cloned().collect();
                    } else {
                        force.system_presence.clear();
                        force.world_presence.clear();
                    }
                }
            }
            dirty = true;
        }
        if !fac.subfactions.is_empty() {
            let force_count: usize = fac.subfactions.iter().map(|sf| sf.forces.len()).sum();
            ui.label(
                RichText::new(format!(
                    "{} subfactions / {} forces hidden",
                    fac.subfactions.len(),
                    force_count
                ))
                .color(palette::chrome_text_dim()),
            );
        }
    });

    dirty
}

fn show_designer_builder(ui: &mut Ui, state: &mut FactionDesignerState) {
    ui.label(
        RichText::new("ADD OVERALL / SUBFACTION")
            .color(palette::chrome_text())
            .strong(),
    );
    let selected = state.selected_preset.min(OVERALL_PRESETS.len() - 1);
    let mut picked = selected;
    ui.horizontal_wrapped(|ui| {
        field_label(ui, "OVERALL");
        crate::ui_kit::combo(
            "faction_designer_overall",
            RichText::new(OVERALL_PRESETS[selected].label),
        )
        .show_ui(ui, |ui| {
            for (i, p) in OVERALL_PRESETS.iter().enumerate() {
                if ui
                    .selectable_label(i == selected, RichText::new(p.label))
                    .clicked()
                {
                    picked = i;
                }
            }
        });
        if picked != state.selected_preset {
            state.selected_preset = picked;
            apply_preset_to_builder(state, OVERALL_PRESETS[picked]);
        }
        field_label(ui, "KIND");
        let _ = text_edit(ui, &mut state.new_kind, 150.0);
        field_label(ui, "ID");
        let _ = text_edit(ui, &mut state.new_id, 170.0);
        if ui.button(RichText::new("ID FROM NAME")).clicked() {
            state.new_id = slug_id(&state.new_name);
        }
    });

    ui.horizontal_wrapped(|ui| {
        field_label(ui, "NAME");
        let _ = text_edit(ui, &mut state.new_name, 240.0);
        field_label(ui, "DISP");
        let _ = text_edit(ui, &mut state.new_disposition, 120.0);
        field_label(ui, "WEIGHT");
        ui.add_sized(
            [80.0, 22.0],
            DragValue::new(&mut state.new_weight)
                .speed(0.25)
                .range(0.1..=100.0),
        );
        if ui.button(RichText::new("+ ADD ROW")).clicked() {
            match add_builder_row(state) {
                Ok(()) => {
                    let added = state
                        .rows
                        .last()
                        .map(|row| row.id.as_str())
                        .unwrap_or_default();
                    state.status = format!("added {added}");
                }
                Err(e) => state.status = e.to_string(),
            }
        }
    });

    ui.horizontal_wrapped(|ui| {
        field_label(ui, "WORLD TYPES");
        let _ = text_edit(ui, &mut state.new_world_types, 280.0);
        field_label(ui, "GOVS");
        let _ = text_edit(ui, &mut state.new_governments, 260.0);
        field_label(ui, "FEATURES");
        let _ = text_edit(ui, &mut state.new_features, 320.0);
    });
}

fn show_designer_rows(ui: &mut Ui, state: &mut FactionDesignerState) {
    ui.horizontal(|ui| {
        fixed(ui, 28.0, "");
        fixed(ui, 40.0, "#");
        fixed(ui, 170.0, "ID");
        fixed(ui, 220.0, "NAME");
        fixed(ui, 150.0, "KIND");
        fixed(ui, 120.0, "DISP");
        fixed(ui, 80.0, "WEIGHT");
    });
    ui.separator();

    if state.rows.is_empty() {
        ui.label(RichText::new("designer roster empty").color(palette::chrome_text_dim()));
        return;
    }

    let mut remove: Option<usize> = None;
    for (i, row) in state.rows.iter_mut().enumerate() {
        let style = faction_style(&row.kind, &row.id, &row.disposition);
        ui.horizontal(|ui| {
            draw_faction_chip(ui, style);
            fixed_text(ui, 40.0, &(i + 1).to_string(), palette::chrome_text_dim());
            let _ = text_edit(ui, &mut row.id, 170.0);
            let _ = text_edit(ui, &mut row.name, 220.0);
            let _ = text_edit(ui, &mut row.kind, 150.0);
            let _ = text_edit(ui, &mut row.disposition, 120.0);
            ui.add_sized(
                [80.0, 22.0],
                DragValue::new(&mut row.weight)
                    .speed(0.25)
                    .range(0.1..=100.0),
            );
            if ui.button(RichText::new("REMOVE")).clicked() {
                remove = Some(i);
            }
        });
        ui.horizontal_wrapped(|ui| {
            field_label(ui, "WORLD TYPES");
            let _ = text_edit(ui, &mut row.world_types, 260.0);
            field_label(ui, "GOVS");
            let _ = text_edit(ui, &mut row.governments, 240.0);
            field_label(ui, "FEATURES");
            let _ = text_edit(ui, &mut row.features, 300.0);
        });
        ui.add_space(4.0);
    }

    if let Some(i) = remove {
        let removed = state.rows.remove(i);
        state.status = format!("removed {}", removed.id);
    }
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
                    if let Some(force_id) = &presence.force_id {
                        let force = sub.forces.entry(force_id.clone()).or_default();
                        force.systems.insert(sys.id.clone());
                        force.worlds.insert(world.id.clone());
                    }
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
                for force in &mut sf.forces {
                    if let Some(sub) = stats.subfactions.get(&sf.id) {
                        if let Some(force_stats) = sub.forces.get(&force.id) {
                            force.system_presence = force_stats.systems.iter().cloned().collect();
                            force.world_presence = force_stats.worlds.iter().cloned().collect();
                            continue;
                        }
                    }
                    force.system_presence.clear();
                    force.world_presence.clear();
                }
            }
        } else {
            fac.system_presence.clear();
            fac.world_presence.clear();
            for sf in &mut fac.subfactions {
                sf.system_presence.clear();
                sf.world_presence.clear();
                for force in &mut sf.forces {
                    force.system_presence.clear();
                    force.world_presence.clear();
                }
            }
        }
    }
}

fn remove_faction_everywhere(sector: &mut GeneratedSector, id: &FactionId) {
    sector.factions.retain(|f| &f.id != id);
    let relations = std::sync::Arc::make_mut(&mut sector.relations);
    relations.pairs.retain(|p| &p.a != id && &p.b != id);
    let power_projection = std::sync::Arc::make_mut(&mut sector.power_projection);
    power_projection.by_faction.remove(id);

    let influence_field = std::sync::Arc::make_mut(&mut sector.influence_field);
    for cell in &mut influence_field.cells {
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
    influence_field.bands.retain(|band| &band.faction_id != id);

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

fn clear_conflict(conflict: &mut sectorforge::conflict::ConflictState, id: &FactionId) {
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

fn designer_rows_from_sector_output(sector: &GeneratedSector) -> Vec<DesignerFactionRow> {
    let mut rows = Vec::new();
    for fac in &sector.factions {
        if fac.subfactions.is_empty() {
            rows.push(DesignerFactionRow {
                id: fac.id.as_str().to_string(),
                name: fac.name.to_string(),
                kind: fac.kind.to_string(),
                disposition: fac.disposition.to_string(),
                weight: output_weight(fac.system_presence.len(), fac.world_presence.len()),
                world_types: String::new(),
                governments: String::new(),
                features: String::new(),
            });
        } else {
            for sf in &fac.subfactions {
                if sf.forces.is_empty() {
                    rows.push(DesignerFactionRow {
                        id: sf.id.as_str().to_string(),
                        name: sf.name.to_string(),
                        kind: sf.id.as_str().to_string(),
                        disposition: if sf.disposition.is_empty() {
                            fac.disposition.to_string()
                        } else {
                            sf.disposition.to_string()
                        },
                        weight: output_weight(sf.system_presence.len(), sf.world_presence.len()),
                        world_types: String::new(),
                        governments: String::new(),
                        features: String::new(),
                    });
                } else {
                    for force in &sf.forces {
                        rows.push(DesignerFactionRow {
                            id: force.id.as_str().to_string(),
                            name: force.name.to_string(),
                            kind: sf.id.as_str().to_string(),
                            disposition: if force.disposition.is_empty() {
                                sf.disposition.to_string()
                            } else {
                                force.disposition.to_string()
                            },
                            weight: output_weight(
                                force.system_presence.len(),
                                force.world_presence.len(),
                            ),
                            world_types: String::new(),
                            governments: String::new(),
                            features: String::new(),
                        });
                    }
                }
            }
        }
    }
    rows
}

fn append_output_rows(state: &mut FactionDesignerState, sector: &GeneratedSector) -> usize {
    let mut used: BTreeSet<String> = state
        .rows
        .iter()
        .map(|row| row.id.trim().to_string())
        .collect();
    let mut added = 0;
    for row in designer_rows_from_sector_output(sector) {
        if used.insert(row.id.clone()) {
            state.rows.push(row);
            added += 1;
        }
    }
    added
}

fn apply_preset_to_builder(state: &mut FactionDesignerState, p: OverallPreset) {
    state.new_id = p.kind.to_string();
    state.new_name = p.name.to_string();
    state.new_kind = p.kind.to_string();
    state.new_disposition = p.disposition.to_string();
    state.new_weight = p.weight;
    state.new_world_types = join_static_list(p.world_types);
    state.new_governments = join_static_list(p.governments);
    state.new_features = join_static_list(p.features);
}

fn add_builder_row(state: &mut FactionDesignerState) -> Result<(), FactionDesignerError> {
    let name = non_empty_trim(&state.new_name, "name")?;
    let kind = non_empty_trim(&state.new_kind, "kind")?;
    let mut id = state.new_id.trim().to_string();
    if id.is_empty() {
        id = slug_id(&name);
    }
    if id.is_empty() {
        return Err(FactionDesignerError::Validation {
            field: "id".to_string(),
            message: "cannot be empty (auto-slug from name failed)".to_string(),
        });
    }
    validate_weight(state.new_weight, "new row")?;
    state.rows.push(DesignerFactionRow {
        id,
        name,
        kind,
        disposition: state.new_disposition.trim().to_string(),
        weight: round_weight(state.new_weight),
        world_types: clean_list_text(&state.new_world_types),
        governments: clean_list_text(&state.new_governments),
        features: clean_list_text(&state.new_features),
    });
    Ok(())
}

fn choose_and_save_designer_toml(
    state: &FactionDesignerState,
    sector: &GeneratedSector,
    project_dir: Option<&Path>,
) -> Result<Option<String>, FactionDesignerError> {
    let file = designer_rows_to_factions_file(&state.rows)?;
    let default_name = format!("{}-factions.toml", sector.id);
    let mut dialog = FileDialog::new()
        .add_filter("TOML", &["toml"])
        .set_file_name(&default_name);
    if let Some(dir) = default_faction_dir(project_dir) {
        dialog = dialog.set_directory(dir);
    }
    let Some(path) = dialog.save_file() else {
        return Ok(None);
    };
    let text = toml::to_string_pretty(&file)
        .map_err(|e| FactionDesignerError::Config(format!("encode toml: {e}")))?;
    fs::write(&path, text)?;
    Ok(Some(path.display().to_string()))
}

fn designer_rows_to_factions_file(
    rows: &[DesignerFactionRow],
) -> Result<FactionsFile, FactionDesignerError> {
    if rows.is_empty() {
        return Err(FactionDesignerError::Config(
            "designer has no rows".to_string(),
        ));
    }
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut factions = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let id = non_empty_trim(&row.id, &format!("row {} id", i + 1))?;
        let name = non_empty_trim(&row.name, &format!("row {} name", i + 1))?;
        let kind = non_empty_trim(&row.kind, &format!("row {} kind", i + 1))?;
        if !used.insert(id.clone()) {
            return Err(FactionDesignerError::Validation {
                field: format!("row {} id", i + 1),
                message: format!("duplicate id '{id}'"),
            });
        }
        validate_weight(row.weight, &format!("row {}", i + 1))?;
        factions.push(FactionDef {
            faction: None,
            faction_name: None,
            subfaction: None,
            subfaction_name: None,
            id: FactionId::new(id),
            name,
            kind,
            weight: round_weight(row.weight),
            default_disposition: row.disposition.trim().to_string(),
            preferred_world_types: parse_list(&row.world_types),
            preferred_governments: parse_list(&row.governments),
            preferred_notable_features: parse_list(&row.features),
            style_fill: None,
            style_accent: None,
            style_glyph: None,
            style_border: None,
            legend_visible: None,
        });
    }
    Ok(FactionsFile { factions })
}

fn default_faction_dir(project_dir: Option<&Path>) -> Option<PathBuf> {
    let root = project_dir?;
    let faction_dir = root.join("data").join("factions");
    if faction_dir.is_dir() {
        Some(faction_dir)
    } else {
        Some(root.to_path_buf())
    }
}

fn non_empty_trim(value: &str, field: &str) -> Result<String, FactionDesignerError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(FactionDesignerError::Validation {
            field: field.to_string(),
            message: "cannot be empty".to_string(),
        })
    } else {
        Ok(trimmed.to_string())
    }
}

fn validate_weight(weight: f64, field: &str) -> Result<(), FactionDesignerError> {
    if !weight.is_finite() || weight <= 0.0 {
        Err(FactionDesignerError::Validation {
            field: field.to_string(),
            message: "weight must be finite and > 0".to_string(),
        })
    } else {
        Ok(())
    }
}

fn output_weight(system_count: usize, world_count: usize) -> f64 {
    let raw = 1.0 + system_count as f64 * 0.35 + world_count as f64 * 0.15;
    round_weight(raw.clamp(1.0, 10.0))
}

fn round_weight(weight: f64) -> f64 {
    (weight * 10.0).round() / 10.0
}

fn parse_list(text: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert((*s).to_string()))
        .map(str::to_string)
        .collect()
}

fn clean_list_text(text: &str) -> String {
    parse_list(text).join(", ")
}

fn join_static_list(values: &[&str]) -> String {
    values.join(", ")
}

fn slug_id(value: &str) -> String {
    let mut out = String::new();
    let mut pending_sep = false;
    for c in value.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            pending_sep = false;
        } else {
            pending_sep = true;
        }
    }
    out.trim_matches('_').to_string()
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

fn field_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(palette::chrome_text_dim()));
}

fn text_edit<T>(ui: &mut Ui, value: &mut T, width: f32) -> bool
where
    T: AsRef<str> + From<String>,
{
    let mut buf = value.as_ref().to_string();
    let changed = ui
        .add_sized(
            [width, 22.0],
            egui::TextEdit::singleline(&mut buf).font(egui::FontId::proportional(12.0)),
        )
        .changed();
    if changed {
        *value = T::from(buf);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn designer_rows_export_as_factions_file() {
        let rows = vec![DesignerFactionRow {
            id: "test_cell".into(),
            name: "Test Cell".into(),
            kind: "inquisition".into(),
            disposition: "secretive".into(),
            weight: 2.25,
            world_types: "HiveWorld, ShrineWorld, HiveWorld".into(),
            governments: String::new(),
            features: "InquisitionOutpost".into(),
        }];

        let file = designer_rows_to_factions_file(&rows).expect("valid designer rows");
        assert_eq!(file.factions.len(), 1);
        assert_eq!(file.factions[0].id.as_str(), "test_cell");
        assert_eq!(file.factions[0].kind, "inquisition");
        assert_eq!(file.factions[0].weight, 2.3);
        assert_eq!(
            file.factions[0].preferred_world_types,
            vec!["HiveWorld".to_string(), "ShrineWorld".to_string()]
        );

        let text = toml::to_string_pretty(&file).expect("toml encode");
        assert!(text.contains("[[factions]]"));
        assert!(text.contains("default_disposition = \"secretive\""));
    }

    #[test]
    fn designer_rows_reject_duplicate_ids() {
        let row = DesignerFactionRow {
            id: "dup".into(),
            name: "Duplicate".into(),
            kind: "imperial".into(),
            disposition: "lawful".into(),
            weight: 1.0,
            world_types: String::new(),
            governments: String::new(),
            features: String::new(),
        };
        let err = designer_rows_to_factions_file(&[row.clone(), row]).expect_err("duplicate id");
        assert!(err.to_string().contains("duplicate id"));
    }
}
