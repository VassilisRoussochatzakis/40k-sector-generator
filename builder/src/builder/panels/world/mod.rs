//! WORLD tab (§N1 / §N2) — Phase B §W1..§W7 inspector.
//!
//! Covers every `GeneratedWorld` field via per-section editors backed by the
//! canonical `*::VARIANTS` arrays in [`sectorforge::worlds`] (§W2). Pinning
//! (§W3) is stored in [`BuilderState::pinned_worlds`]. Re-roll (§W4) calls
//! [`BuilderState::regenerate_world`]. Features picker (§W5) is a searchable
//! multi-select with weight preview against the candidate pool. Coupling
//! warnings (§W6) are inline non-blocking heuristics. Claims chip-row (§W7)
//! shows every entry in `world.claims` with quick deep-links to the faction.

use egui::{Color32, RichText, Ui};

use sectorforge::worlds::{
    Atmosphere, Biosphere, Government, Population, TechLevel, Temperature, WorldType,
};
use sectorforge_gui_core::card;
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;

use crate::builder::state::EntityRef;
use crate::builder::BuilderState;

mod claims;
mod environment;
mod factions;
mod features;
mod identity;
mod overlays;

use claims::show_claims_section;
use environment::{show_environment_section, show_society_section};
use factions::show_factions_section;
use features::{show_coupling_warnings, show_features_section};
use identity::{show_classification_section, show_identity_section, show_tags_notes_section};
use overlays::{
    show_chronicle_section, show_control_section, show_overlays_section, show_regen_section,
};

pub(crate) fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("World");
    ui.add_space(4.0);

    let total_worlds: usize = state.sector.systems.iter().map(|s| s.worlds.len()).sum();
    if total_worlds == 0 {
        ui_kit::placeholder(
            ui,
            "No worlds yet — open the System tab and add a world to one of your systems.",
        );
        return;
    }

    // §COLUMNS — master-detail: a persistent world roster on the left rail and
    // the RC-2 inspector filling the rest. Replaces the picker + 18-section
    // single-column stack that left ~1000 px of dead gutter at 1400 px wide.
    egui::SidePanel::left("world_roster")
        .resizable(true)
        .default_width(240.0)
        .width_range(180.0..=420.0)
        .show_inside(ui, |ui| show_world_roster(ui, state));

    egui::CentralPanel::default().show_inside(ui, |ui| show_world_inspector(ui, state));
}

// ── roster / inspector / header ─────────────────────────────────────────────

/// §COLUMNS — left-rail world roster (master pane), grouped by parent system.
/// Clicking a row selects the world; selection is pure view state, so it is set
/// directly (no command bus needed — only model edits route through `state.run`).
fn show_world_roster(ui: &mut Ui, state: &mut BuilderState) {
    ui.add_space(2.0);
    let current = state.selected_world_id.clone();
    let mut pick = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for sys in &state.sector.systems {
                if sys.worlds.is_empty() {
                    continue;
                }
                ui_kit::collapsing_section(
                    ui,
                    ("world_roster_sys", sys.id.as_str()),
                    &format!("{} ({})", sys.name, sys.worlds.len()),
                    true,
                    |ui| {
                        for w in &sys.worlds {
                            let sel = current.as_ref() == Some(&w.id);
                            // §BEAUTY: animated selectable plate (card::selectable_plate).
                            let (resp, _) =
                                card::selectable_plate(ui, ("world_row", &w.id), sel, |ui| {
                                    ui.label(
                                        RichText::new(w.name.to_string())
                                            .color(palette::chrome_text())
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(format!("({})", w.id))
                                            .color(palette::chrome_text_dim())
                                            .small(),
                                    );
                                });
                            if resp.clicked() {
                                pick = Some((sys.id.clone(), w.id.clone()));
                            }
                        }
                    },
                );
            }
        });
    if let Some((sid, wid)) = pick {
        state.selected_world_id = Some(wid);
        state.selected_system_id = Some(sid);
    }
}

/// §COLUMNS — right detail pane: a full-width header, then the §W1..§W7 sections
/// flowed across responsive columns (2 at 1400 px, collapsing to 1 when narrow).
/// The injected conflict / surface-region / intel sub-sections just become
/// columns like any other section.
fn show_world_inspector(ui: &mut Ui, state: &mut BuilderState) {
    let selected = state.selected_world_id.clone();
    let Some(wid) = selected else {
        ui_kit::placeholder(ui, "Select a world from the roster on the left.");
        return;
    };
    let Some((sys_idx, w_idx)) = state.find_world_indices(&wid) else {
        state.selected_world_id = None;
        return;
    };

    show_header(ui, state, sys_idx, w_idx);
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui_kit::columns_responsive(ui, 2, 460.0, |cols| {
                let n = cols.len();
                let mut next = 0usize;
                macro_rules! col {
                    () => {{
                        let c = &mut cols[next % n];
                        next += 1;
                        c
                    }};
                }
                show_identity_section(col!(), state, sys_idx, w_idx);
                show_classification_section(col!(), state, sys_idx, w_idx);
                show_environment_section(col!(), state, sys_idx, w_idx);
                show_society_section(col!(), state, sys_idx, w_idx);
                show_features_section(col!(), state, sys_idx, w_idx);
                show_coupling_warnings(col!(), state, sys_idx, w_idx);
                show_tags_notes_section(col!(), state, sys_idx, w_idx);
                show_factions_section(col!(), state, sys_idx, w_idx);
                show_claims_section(col!(), state, sys_idx, w_idx);
                show_control_section(col!(), state, sys_idx, w_idx);
                show_overlays_section(col!(), state, sys_idx, w_idx);
                crate::builder::panels::conflict::show_world_conflict_section(
                    col!(),
                    state,
                    sys_idx,
                    w_idx,
                );
                crate::builder::panels::surface_regions::show_surface_regions_section(
                    col!(),
                    state,
                    sys_idx,
                    w_idx,
                );
                crate::builder::panels::intel::show_world_intel_section(
                    col!(),
                    state,
                    sys_idx,
                    w_idx,
                );
                show_chronicle_section(col!(), state, sys_idx, w_idx);
                show_regen_section(col!(), state, sys_idx, w_idx);
                let _ = next; // final col!() bump is intentionally unread
            });
        });
}

fn show_header(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    let w = &state.sector.systems[sys_idx].worlds[w_idx];
    let sys_name = state.sector.systems[sys_idx].name.to_string();
    let sys_id = state.sector.systems[sys_idx].id.clone();
    let wid = w.id.clone();
    let name = w.name.to_string();
    let pinned = state.pinned_worlds.contains(&wid);
    ui.horizontal_wrapped(|ui| {
        ui.heading(name);
        ui.label(
            RichText::new(wid.to_string())
                .color(Color32::GRAY)
                .monospace(),
        );
        ui.colored_label(Color32::DARK_GRAY, format!("in {sys_name}"));
        if sectorforge_gui_core::entity_link(ui, "open system", true).clicked() {
            state.focus_entity(EntityRef::System(sys_id));
        }
        if pinned {
            ui.colored_label(palette::warning(), "PINNED");
        }
    });
}

// ── combo helper ───────────────────────────────────────────────────────────

trait EnumPicker: Sized + Clone + PartialEq + 'static {
    fn variants() -> &'static [Self];
    fn display(&self) -> &'static str;
}

impl EnumPicker for WorldType {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
}
impl EnumPicker for Atmosphere {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
}
impl EnumPicker for Temperature {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
}
impl EnumPicker for Biosphere {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
}
impl EnumPicker for Population {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
}
impl EnumPicker for TechLevel {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
}
impl EnumPicker for Government {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
}

fn combo_enum<E: EnumPicker>(ui: &mut Ui, salt: &str, target: &mut E) -> bool {
    let prev = target.clone();
    ui_kit::combo(salt, target.display()).show_ui(ui, |ui| {
        for v in E::variants() {
            ui.selectable_value(target, v.clone(), v.display());
        }
    });
    *target != prev
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::panels::world::features::coupling_warnings;
    use sectorforge::sector_model::{HexCoord, WorldDto};
    use sectorforge::worlds::NotableFeature;

    fn world_dto() -> WorldDto {
        WorldDto {
            star_colour: sectorforge::worlds::StarColour::Yellow,
            world_type: WorldType::AgriWorld,
            atmosphere: Atmosphere::Breathable,
            temperature: Temperature::Temperate,
            biosphere: Biosphere::Thriving,
            population: Population::DenselyPopulated,
            tech_level: TechLevel::Standard,
            government: Government::MilitaryGovernor,
            notable_features: Vec::new(),
        }
    }

    #[test]
    fn coupling_flags_dead_world_with_population() {
        let mut dto = world_dto();
        dto.world_type = WorldType::DeadWorld;
        let warns = coupling_warnings(&dto);
        assert!(warns.iter().any(|w| w.contains("DeadWorld")));
    }

    #[test]
    fn coupling_flags_uninhabited_with_government() {
        let mut dto = world_dto();
        dto.population = Population::Uninhabited;
        let warns = coupling_warnings(&dto);
        assert!(warns.iter().any(|w| w.contains("government")));
    }

    #[test]
    fn coupling_silent_on_normal_world() {
        let dto = world_dto();
        let warns = coupling_warnings(&dto);
        assert!(warns.is_empty(), "got unexpected warnings: {warns:?}");
    }

    #[test]
    fn pinned_world_refuses_regen() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let sid = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "S")
            .unwrap();
        let wid = state.sector.add_world_to_system(&sid, "W").unwrap();
        state.pinned_worlds.insert(wid.clone());
        let err = state.regenerate_world(&wid).unwrap_err();
        assert!(err.to_string().contains("pinned"));
    }

    #[test]
    fn enum_picker_variants_match_worlds_authoritative_set() {
        // §W2 audit: panel uses VARIANTS directly so it cannot drift.
        assert_eq!(WorldType::VARIANTS.len(), 24);
        assert_eq!(Atmosphere::VARIANTS.len(), 7);
        assert_eq!(Temperature::VARIANTS.len(), 5);
        assert_eq!(Biosphere::VARIANTS.len(), 6);
        assert_eq!(Population::VARIANTS.len(), 6);
        assert_eq!(TechLevel::VARIANTS.len(), 6);
        assert_eq!(Government::VARIANTS.len(), 30);
        assert!(NotableFeature::VARIANTS.len() >= 90);
    }
}
