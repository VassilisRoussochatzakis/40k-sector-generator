//! CONTROL tab (§N1 / §N2). Phase C §C1..§C8 presence/dominance/control-state
//! editor + Phase B §CL1..§CL4 claims editor.
//!
//! §C1  per-world presence rows: 10 dim sliders + influence tier + intel.
//! §C2  add / remove presence rows.
//! §C3  dominance state ComboBox per (world, faction); manual lock toggle.
//! §C4  per-system `control_state` ComboBox.
//! §C5  per-system `primary_factions` list with derive + manual lock.
//! §C6  per-faction `PowerProfile` preview, refreshed live from
//!      `aggregate_faction_power`.
//! §C7  toggle to project per-system power onto the MAP via the SectorView
//!      heatmap channel.
//! §C8  toggle to render the continuous influence-field on the MAP.
//!
//! §CL1 chip-row per world (faction_id, ClaimType, strength) + remove.
//! §CL2 add-claim picker. Multiple claims per faction allowed.
//! §CL3 Contested auto-flag — N>1 distinct claimants on a world.
//! §CL4 Bulk convert: every claim of kind X by faction Y becomes kind Z.

use std::collections::BTreeSet;

use egui::{Color32, RichText, Ui};

use sectorforge::control::{aggregate_faction_power, derive_system_control, derive_world_control};
use sectorforge::ids::{FactionId, SystemId, WorldId};
use sectorforge::sector_model::{
    ClaimType, DominanceState, FactionInfluence, PresenceDimensions, SystemState,
    WorldFactionPresence,
};

use sectorforge_gui_core::palette;
use sectorforge_gui_core::sector_view::SectorMapCache;
use sectorforge_gui_core::ui_kit::{self, labeled};

use crate::builder::command::BuilderCommand;
use crate::builder::state::{BuilderTab, ControlOverlay, EntityRef, ModalKind};
use crate::builder::BuilderState;

mod claims;

const CLAIM_TYPES: &[ClaimType] = &[
    ClaimType::LegalSovereignty,
    ClaimType::ImperialMandate,
    ClaimType::TreatyRight,
    ClaimType::ReligiousMandate,
    ClaimType::DynasticRight,
    ClaimType::CommercialCharter,
    ClaimType::MilitaryOccupation,
    ClaimType::AncientDomain,
    ClaimType::HuntingGround,
    ClaimType::CovertWrit,
    ClaimType::Rebellion,
];

const INFLUENCE_TIERS: &[FactionInfluence] = &[
    FactionInfluence::Dominant,
    FactionInfluence::Significant,
    FactionInfluence::Minor,
    FactionInfluence::Hidden,
];

const DOMINANCE_STATES: &[DominanceState] = &[
    DominanceState::Rumored,
    DominanceState::Presence,
    DominanceState::Influence,
    DominanceState::Contested,
    DominanceState::Controlled,
    DominanceState::Stronghold,
];

/// §C6 power-projection table headers: the compact column heading kept for the
/// dense grid, paired with a plain-language hover note so the abbreviations are
/// self-explanatory.
const POWER_COLUMNS: &[(&str, &str)] = &[
    ("faction", "Faction id"),
    ("admin", "Administrative projection"),
    ("mil", "Military projection"),
    ("naval", "Naval projection (orbital + military reach)"),
    ("econ", "Economic / trade projection"),
    ("ind", "Industrial / forge projection"),
    ("ideo", "Ideological / faith projection"),
    ("covert", "Covert / intelligence projection"),
    ("logi", "Logistical / supply projection"),
    ("legit", "Legitimacy / recognised authority"),
    ("total", "Total weighted projection"),
];

/// Plain-language name for a [`FactionInfluence`] tier. The raw slug stays
/// reachable via the dropdown row tooltip.
fn influence_label(t: FactionInfluence) -> &'static str {
    match t {
        FactionInfluence::Dominant => "Dominant",
        FactionInfluence::Significant => "Significant",
        FactionInfluence::Minor => "Minor",
        FactionInfluence::Hidden => "Hidden",
        _ => "Influence",
    }
}

/// One-line explanation of an influence tier for the dropdown row tooltip.
fn influence_help(t: FactionInfluence) -> &'static str {
    match t {
        FactionInfluence::Dominant => "Strongest force here (schema: dominant).",
        FactionInfluence::Significant => "A major player, not the top (schema: significant).",
        FactionInfluence::Minor => "A small foothold (schema: minor).",
        FactionInfluence::Hidden => "Concealed / covert presence (schema: hidden).",
        _ => "How strong this faction is here (schema: influence).",
    }
}

/// Plain-language name for a [`DominanceState`].
fn dominance_label(d: DominanceState) -> &'static str {
    match d {
        DominanceState::Rumored => "Rumored",
        DominanceState::Presence => "Presence",
        DominanceState::Influence => "Influence",
        DominanceState::Contested => "Contested",
        DominanceState::Controlled => "Controlled",
        DominanceState::Stronghold => "Stronghold",
        _ => "Dominance",
    }
}

/// Plain-language name for a [`ClaimType`].
fn claim_label(k: ClaimType) -> &'static str {
    match k {
        ClaimType::LegalSovereignty => "Legal sovereignty",
        ClaimType::ImperialMandate => "Imperial mandate",
        ClaimType::TreatyRight => "Treaty right",
        ClaimType::ReligiousMandate => "Religious mandate",
        ClaimType::DynasticRight => "Dynastic right",
        ClaimType::CommercialCharter => "Commercial charter",
        ClaimType::MilitaryOccupation => "Military occupation",
        ClaimType::AncientDomain => "Ancient domain",
        ClaimType::HuntingGround => "Hunting ground",
        ClaimType::CovertWrit => "Covert writ",
        ClaimType::Rebellion => "Rebellion",
        _ => "Claim",
    }
}

/// Human label for one of the 10 presence dimensions, plus a one-line note for
/// the row tooltip. Friendlier replacement for the bare slug grid labels.
fn dimension_label(field: &str) -> (&'static str, &'static str) {
    match field {
        "admin" => (
            "Administrative",
            "Bureaucratic / governing footprint (schema: admin).",
        ),
        "military" => (
            "Military",
            "Ground forces and garrison strength (schema: military).",
        ),
        "orbital" => (
            "Orbital",
            "Void / orbital control above the world (schema: orbital).",
        ),
        "economic" => ("Economic", "Trade and commercial reach (schema: economic)."),
        "industrial" => (
            "Industrial",
            "Forge / manufactory output (schema: industrial).",
        ),
        "ideological" => (
            "Ideological",
            "Faith / cult / popular authority (schema: ideological).",
        ),
        "covert" => (
            "Covert",
            "Hidden intelligence and infiltration (schema: covert).",
        ),
        "logistics" => (
            "Logistics",
            "Supply-chain and resupply reach (schema: logistics).",
        ),
        "legitimacy" => (
            "Legitimacy",
            "Recognised right to rule here (schema: legitimacy).",
        ),
        "visibility" => (
            "Visibility",
            "How obvious this presence is to the player (schema: visibility).",
        ),
        _ => ("Dimension", "Presence dimension (0..=100)."),
    }
}

pub(crate) fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Control");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "Who holds each world and system — presence, dominance, control state, and claims.",
    );
    ui.separator();

    // §COLUMNS — the MAP-overlay picker stays full-width on top (it is a single
    // wrapping row of toggles), then the §C1..§C6 / §CL editor sections flow
    // round-robin through `columns_responsive` (2 columns at 1400 px, collapsing
    // to 1 when narrow) instead of stacking down the left edge.
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_overlay_toggles(ui, state);
            ui.separator();

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
                show_world_presence_editor(col!(), state);
                show_system_control_editor(col!(), state);
                show_power_profile_preview(col!(), state);
                show_contested_summary(col!(), state);
                show_bulk_convert(col!(), state);
                claims::show_world_list(col!(), state);
                let _ = next; // final col!() bump is intentionally unread
            });
        });
}

// ── C7 + C8 overlay toggles ──────────────────────────────────────────────

fn show_overlay_toggles(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "ctrl_map_overlays", "Map overlay", true, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Tint the map by:");
            for mode in [
                ControlOverlay::None,
                ControlOverlay::PowerProjection,
                ControlOverlay::InfluenceField,
                ControlOverlay::Administrative,
                ControlOverlay::Military,
                ControlOverlay::Orbital,
                ControlOverlay::Naval,
                ControlOverlay::Mercantile,
                ControlOverlay::Industrial,
                ControlOverlay::Logistical,
                ControlOverlay::Informational,
                ControlOverlay::Religious,
                ControlOverlay::Sympathetic,
            ] {
                let selected = state.map_view.control_overlay == mode;
                if ui
                    .selectable_label(selected, mode.label())
                    .on_hover_text(overlay_help(mode))
                    .clicked()
                {
                    state.map_view.control_overlay = mode;
                }
            }
            if ui
                .small_button("🗺 Show on map")
                .on_hover_text("Jump to the MAP tab to see the selected overlay")
                .clicked()
            {
                state.focus_entity(EntityRef::Tab(BuilderTab::Map));
            }
        });
        ui.colored_label(
            Color32::DARK_GRAY,
            "Recolours the map by the chosen measure: who projects the most reach, whose influence-field dominates a cell, or which faction leads on a single presence axis.",
        );
    });
}

/// One-line hover note per overlay mode — plain language, no source paths.
fn overlay_help(mode: ControlOverlay) -> &'static str {
    match mode {
        ControlOverlay::None => "No tint — show the normal map.",
        ControlOverlay::PowerProjection => {
            "Tint each system by who can project the most reach into it."
        }
        ControlOverlay::InfluenceField => {
            "Tint each cell by the faction whose influence-field dominates it."
        }
        ControlOverlay::Administrative => "Tint by the top administrative presence per system.",
        ControlOverlay::Military => "Tint by the top military presence per system.",
        ControlOverlay::Orbital => "Tint by the top orbital presence per system.",
        ControlOverlay::Naval => "Tint by combined military + orbital reach per system.",
        ControlOverlay::Mercantile => "Tint by the top economic / trade presence per system.",
        ControlOverlay::Industrial => "Tint by the top industrial / forge presence per system.",
        ControlOverlay::Logistical => "Tint by the top logistics / supply presence per system.",
        ControlOverlay::Informational => "Tint by the top covert / intel presence per system.",
        ControlOverlay::Religious => "Tint by the top ideological / faith presence per system.",
        ControlOverlay::Sympathetic => {
            "Tint by the strongest sympathetic / legitimacy presence per system."
        }
    }
}

// ── C1 + C2 + C3 per-world presence editor ───────────────────────────────

fn show_world_presence_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "ctrl_world_presence", "World presence", true, |ui| {
        let world_id = state.selection.world_id.clone();
        let Some(world_id) = world_id else {
            ui_kit::placeholder(
                ui,
                "No world selected. Pick a world in the WORLD tab to edit which factions are present here.",
            );
            return;
        };
        let Some((sys_idx, w_idx)) = state.find_world_indices(&world_id) else {
            ui_kit::placeholder(
                ui,
                "The selected world is no longer in the sector. Pick another in the WORLD tab.",
            );
            return;
        };

        let header = {
            let w = &state.sector.systems[sys_idx].worlds[w_idx];
            format!("{}  ({})", w.name, w.id)
        };
        ui.label(RichText::new(header).strong());

        // Snapshot for stable iteration; edits applied back below.
        let presences = state.sector.systems[sys_idx].worlds[w_idx].factions.clone();
        let factions: Vec<(FactionId, String)> = state
            .sector
            .factions
            .iter()
            .map(|f| (f.id.clone(), f.name.to_string()))
            .collect();

        let mut remove_at: Option<usize> = None;
        let mut edits: Vec<(usize, WorldFactionPresence)> = Vec::new();
        let mut lock_toggles: Vec<(FactionId, bool)> = Vec::new();

        egui::ScrollArea::vertical()
            .id_salt(("c1_scroll", world_id.as_str()))
            .max_height(360.0)
            .show(ui, |ui| {
                for (i, p) in presences.iter().enumerate() {
                    let mut edit = p.clone();
                    let mut row_remove = false;
                    let mut locked = state
                        .dominance_locked
                        .contains(&(world_id.clone(), p.faction_id.clone()));
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let label = factions
                                .iter()
                                .find(|(fid, _)| fid == &p.faction_id)
                                .map(|(_, n)| n.clone())
                                .unwrap_or_else(|| p.faction_id.to_string());
                            ui.label(RichText::new(label).strong());
                            ui.label(
                                RichText::new(p.faction_id.as_str())
                                    .color(Color32::DARK_GRAY)
                                    .monospace(),
                            )
                            .on_hover_text("Faction id (schema: faction_id).");
                            if ui
                                .small_button("🗑 Remove")
                                .on_hover_text("Remove this faction's presence from the world")
                                .clicked()
                            {
                                row_remove = true;
                            }
                        });

                        // §C1 influence tier + intel + dimensions.
                        ui.horizontal(|ui| {
                            ui.label("Influence:")
                                .on_hover_text("How strong this faction is here (schema: influence).");
                            ui_kit::combo(
                                ("c1_tier", p.faction_id.as_str()),
                                influence_label(edit.influence),
                            )
                            .show_ui(ui, |ui| {
                                for t in INFLUENCE_TIERS {
                                    ui.selectable_value(
                                        &mut edit.influence,
                                        *t,
                                        influence_label(*t),
                                    )
                                    .on_hover_text(influence_help(*t));
                                }
                            });
                            ui.label("Intel:").on_hover_text(
                                "How confident the player's intel is, 0..=100 (schema: intel_confidence).",
                            );
                            ui.add(egui::DragValue::new(&mut edit.intel_confidence).range(0..=100));
                        });

                        ui.label(
                            RichText::new("Presence by dimension (0–100)")
                                .color(Color32::DARK_GRAY),
                        );
                        egui::Grid::new(("c1_dim", p.faction_id.as_str()))
                            .num_columns(2)
                            .spacing([8.0, 2.0])
                            .show(ui, |ui| {
                                dim_slider(ui, "admin", &mut edit.dimensions.admin);
                                ui.end_row();
                                dim_slider(ui, "military", &mut edit.dimensions.military);
                                ui.end_row();
                                dim_slider(ui, "orbital", &mut edit.dimensions.orbital);
                                ui.end_row();
                                dim_slider(ui, "economic", &mut edit.dimensions.economic);
                                ui.end_row();
                                dim_slider(ui, "industrial", &mut edit.dimensions.industrial);
                                ui.end_row();
                                dim_slider(ui, "ideological", &mut edit.dimensions.ideological);
                                ui.end_row();
                                dim_slider(ui, "covert", &mut edit.dimensions.covert);
                                ui.end_row();
                                dim_slider(ui, "logistics", &mut edit.dimensions.logistics);
                                ui.end_row();
                                dim_slider(ui, "legitimacy", &mut edit.dimensions.legitimacy);
                                ui.end_row();
                                dim_slider(ui, "visibility", &mut edit.dimensions.visibility);
                                ui.end_row();
                            });

                        // §C3 dominance picker + manual lock.
                        ui.horizontal(|ui| {
                            ui.label("Dominance:").on_hover_text(
                                "How firmly this faction holds the world (schema: dominance). Auto-derived from the dimensions unless you lock it.",
                            );
                            let score = edit.dimensions.local_control_score();
                            let derived = DominanceState::from_score(score);
                            if !locked {
                                edit.dominance = derived;
                            }
                            ui_kit::combo(
                                ("c3_dom", p.faction_id.as_str()),
                                dominance_label(edit.dominance),
                            )
                            .show_ui(ui, |ui| {
                                for d in DOMINANCE_STATES {
                                    if ui
                                        .selectable_label(
                                            edit.dominance == *d,
                                            dominance_label(*d),
                                        )
                                        .on_hover_text(format!("schema: {}", d.as_slug()))
                                        .clicked()
                                    {
                                        edit.dominance = *d;
                                        locked = true;
                                    }
                                }
                            });
                            let was_locked = locked;
                            ui.checkbox(&mut locked, "Lock")
                                .on_hover_text("Keep this dominance fixed instead of auto-deriving it");
                            if was_locked != locked {
                                lock_toggles.push((p.faction_id.clone(), locked));
                            }
                            ui.colored_label(
                                Color32::DARK_GRAY,
                                format!("auto = {} (score {:.1})", dominance_label(derived), score),
                            );
                        });
                    });
                    if row_remove {
                        remove_at = Some(i);
                    } else if presence_changed(p, &edit) {
                        edits.push((i, edit));
                    }
                    ui.add_space(2.0);
                }
            });

        // Apply lock toggles to side-table first.
        for (faction_id, locked) in lock_toggles {
            if locked {
                state
                    .dominance_locked
                    .insert((world_id.clone(), faction_id));
            } else {
                state
                    .dominance_locked
                    .remove(&(world_id.clone(), faction_id));
            }
        }
        // §R4 (CTL-1): per-presence edits + remove route through EditWorld so
        // dimension/tier/intel/dominance tweaks and row removal are undoable.
        // The transient `dominance_locked` side-table is written directly
        // above/below (it is UI lock state, not sector model).
        if !edits.is_empty() || remove_at.is_some() {
            let id = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
            for (i, p) in edits {
                if i < draft.factions.len() {
                    draft.factions[i] = p;
                }
            }
            if let Some(i) = remove_at {
                if i < draft.factions.len() {
                    let removed = draft.factions.remove(i);
                    state
                        .dominance_locked
                        .remove(&(world_id.clone(), removed.faction_id));
                }
            }
            if let Err(e) = state.run(BuilderCommand::EditWorld {
                world: id,
                before: None,
                after: Box::new(draft),
            }) {
                state.feedback.modal = Some(ModalKind::Message(format!("Edit failed: {e}")));
            }
        }

        // §C2 add-presence row.
        ui.separator();
        show_add_presence_row(ui, state, &world_id, sys_idx, w_idx, &factions);
    });
}

/// Compare only the panel-editable subset of a [`WorldFactionPresence`].
/// `WorldFactionPresence` doesn't derive `PartialEq` (it carries `Arc<str>`
/// fields), so we limit the dirty-check to what §C1 / §C3 can actually edit.
fn presence_changed(a: &WorldFactionPresence, b: &WorldFactionPresence) -> bool {
    a.influence != b.influence
        || a.intel_confidence != b.intel_confidence
        || a.dominance != b.dominance
        || a.dimensions != b.dimensions
}

fn dim_slider(ui: &mut Ui, field: &str, value: &mut f32) {
    let (label, help) = dimension_label(field);
    ui.label(label).on_hover_text(help);
    ui.add(egui::Slider::new(value, 0.0..=100.0).clamping(egui::SliderClamping::Always));
}

fn show_add_presence_row(
    ui: &mut Ui,
    state: &mut BuilderState,
    world_id: &WorldId,
    sys_idx: usize,
    w_idx: usize,
    factions: &[(FactionId, String)],
) {
    // §E5: shared candidate computation (factions not already present here); the
    // picker widgets below stay CONTROL-specific (no dominance combo, influence
    // tooltips, faction-id hover) — intentional divergence from the WORLD row.
    let candidates = match super::presence_widgets::presence_candidates(
        &state.sector.systems[sys_idx].worlds[w_idx],
        factions,
    ) {
        super::presence_widgets::PresenceCandidates::NoFactions => {
            ui_kit::placeholder(
                ui,
                "No factions in this sector yet. Add some in the FACTIONS tab to assign presence here.",
            );
            return;
        }
        super::presence_widgets::PresenceCandidates::AllPresent => {
            ui_kit::placeholder(
                ui,
                "Every faction already has a presence row on this world.",
            );
            return;
        }
        super::presence_widgets::PresenceCandidates::Available(c) => c,
    };

    let buf_id = egui::Id::new(("c2_add_buf", world_id.as_str()));
    #[derive(Clone)]
    struct Buf {
        faction: FactionId,
        tier: FactionInfluence,
    }
    let default = Buf {
        faction: candidates[0].0.clone(),
        tier: FactionInfluence::Minor,
    };
    let mut buf: Buf = ui.data_mut(|d| d.get_temp::<Buf>(buf_id).unwrap_or(default));

    ui.horizontal(|ui| {
        ui.label("Add faction:");
        let label = candidates
            .iter()
            .find(|(fid, _)| fid == &buf.faction)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| candidates[0].1.clone());
        if !candidates.iter().any(|(fid, _)| fid == &buf.faction) {
            buf.faction = candidates[0].0.clone();
        }
        ui_kit::combo(("c2_fac", world_id.as_str()), label).show_ui(ui, |ui| {
            for (fid, n) in &candidates {
                if ui
                    .selectable_label(&buf.faction == fid, n)
                    .on_hover_text(format!("id: {fid}"))
                    .clicked()
                {
                    buf.faction = (*fid).clone();
                }
            }
        });
        ui_kit::combo(("c2_tier", world_id.as_str()), influence_label(buf.tier)).show_ui(
            ui,
            |ui| {
                for t in INFLUENCE_TIERS {
                    ui.selectable_value(&mut buf.tier, *t, influence_label(*t))
                        .on_hover_text(influence_help(*t));
                }
            },
        );
        if ui
            .button("➕ Add presence")
            .on_hover_text("Add this faction's presence row to the world")
            .clicked()
        {
            // §R4 (CTL-2): add-presence routes through EditWorld so the new row
            // is undoable.
            let id = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            if let Err(e) = state.edit_world(id, |w| {
                w.factions.push(WorldFactionPresence {
                    faction_id: buf.faction.clone(),
                    subfaction_id: None,
                    subfaction_name: None,
                    force_id: None,
                    force_name: None,
                    influence: buf.tier,
                    relationship_to_government: "neutral".into(),
                    dimensions: PresenceDimensions::default(),
                    dominance: DominanceState::default(),
                    intel_confidence: 100,
                });
            }) {
                state.feedback.modal = Some(ModalKind::Message(format!("Edit failed: {e}")));
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(buf_id, buf));
}

// ── C4 + C5 per-system control editor ────────────────────────────────────

fn show_system_control_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "ctrl_system_control", "System control", true, |ui| {
        let system_id = state.selection.system_id.clone();
        let Some(system_id) = system_id else {
            ui_kit::placeholder(
                ui,
                "No system selected. Pick a system in the SYSTEM tab to set its control state and leading factions.",
            );
            return;
        };
        let Some(sys_idx) = state.sector.systems.iter().position(|s| s.id == system_id) else {
            ui_kit::placeholder(
                ui,
                "The selected system is no longer in the sector. Pick another in the SYSTEM tab.",
            );
            return;
        };
        let header = {
            let s = &state.sector.systems[sys_idx];
            format!("{}  ({})", s.name, s.id)
        };
        ui.label(RichText::new(header).strong());

        // §C4 control_state picker.
        let mut new_state: Option<Option<SystemState>> = None;
        {
            let current = state.sector.systems[sys_idx].control.state;
            labeled(
                ui,
                "Control state",
                "Overall security situation of the system (schema: control.state). Leave unset to let the map derive it.",
                |ui| {
                    ui_kit::combo(
                        ("c4_state", system_id.as_str()),
                        match current {
                            Some(s) => super::system::system_state_label(s),
                            None => "(none)",
                        },
                    )
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(current.is_none(), "(none)").clicked() {
                            new_state = Some(None);
                        }
                        for s in super::SYSTEM_STATES {
                            if ui
                                .selectable_label(
                                    current == Some(*s),
                                    super::system::system_state_label(*s),
                                )
                                .on_hover_text(format!("schema: {}", s.as_slug()))
                                .clicked()
                            {
                                new_state = Some(Some(*s));
                            }
                        }
                    });
                },
            );
        }
        if let Some(ns) = new_state {
            // §R4 (CTL-3): §C4 control_state picker routes through EditSystem
            // so the control-state change is undoable. The system's worlds
            // ride through the clone unchanged.
            let id = state.sector.systems[sys_idx].id.clone();
            if let Err(e) = state.edit_system(id, |sys| sys.control.state = ns) {
                state.feedback.modal = Some(ModalKind::Message(format!("Edit failed: {e}")));
            }
        }

        // §C5 primary_factions: auto-derive top-3 unless locked.
        ui.add_space(4.0);
        let mut locked = state.primary_factions_locked.contains(&system_id);
        let derived: Vec<FactionId> = {
            let summary = derive_system_control(&state.sector.systems[sys_idx]);
            summary
                .top_factions
                .iter()
                .take(3)
                .map(|s| s.faction_id.clone())
                .collect()
        };
        // §R4 / §C5 (IMPROVEMENT_REVIEW E2): the auto-derived top-3 is denormalized
        // document state mirroring presence. Like the passive LD4 chronicle refresh,
        // this reconcile stays OFF the undo bus — dispatching a command here would
        // inject an undo entry on mere tab navigation and fight undo (undo → re-derive
        // from unchanged presence → re-dispatch). Write only on real change and mark
        // the project dirty so the reconciled value saves. The "↺ Re-derive" button
        // below routes through EditSystem; when locked, the manual override is kept.
        if !locked && state.sector.systems[sys_idx].primary_factions != derived {
            state.sector.systems[sys_idx].primary_factions = derived.clone();
            state.dirty = true;
        }
        let factions = &state.sector.systems[sys_idx].primary_factions.clone();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Leading factions").strong())
                .on_hover_text(
                    "The top factions ranked in this system (schema: primary_factions). Auto-derived from presence unless you override it.",
                );
            let was_locked = locked;
            ui.checkbox(&mut locked, "Override")
                .on_hover_text("Keep this list fixed instead of auto-deriving the top 3");
            if was_locked != locked {
                if locked {
                    state.primary_factions_locked.insert(system_id.clone());
                } else {
                    state.primary_factions_locked.remove(&system_id);
                }
                state.dirty = true;
            }
            if ui
                .small_button("↺ Re-derive")
                .on_hover_text("Clear the override and rank the top factions from presence again")
                .clicked()
            {
                // §R4 (CTL-3): Recompute writes derived primary_factions via
                // EditSystem so the override-clear is undoable. The transient
                // `primary_factions_locked` table is UI lock state (direct).
                state.primary_factions_locked.remove(&system_id);
                let id = state.sector.systems[sys_idx].id.clone();
                if let Err(e) = state.edit_system(id, |sys| sys.primary_factions = derived.clone())
                {
                    state.feedback.modal = Some(ModalKind::Message(format!("Edit failed: {e}")));
                }
            }
        });
        if factions.is_empty() {
            ui_kit::placeholder(
                ui,
                "No factions ranked here yet. Add presence on this system's worlds to populate it.",
            );
        } else {
            ui.horizontal_wrapped(|ui| {
                for (rank, fid) in factions.iter().enumerate() {
                    ui.label(
                        RichText::new(format!("{}. {}", rank + 1, fid))
                            .monospace()
                            .color(Color32::LIGHT_BLUE),
                    )
                    .on_hover_text(format!("id: {fid}"));
                }
            });
        }

        // Read-only system summary echo.
        ui.add_space(4.0);
        let summary = derive_system_control(&state.sector.systems[sys_idx]);
        let state_text = summary
            .state
            .map(|s| super::system::system_state_label(s).to_string())
            .unwrap_or_else(|| "—".to_string());
        ui.colored_label(
            Color32::DARK_GRAY,
            format!(
                "Derived: state {}  ·  dominant {}  ·  sovereign {}  ·  orbital {}  ·  economic {}  ·  hidden {}",
                state_text,
                summary.dominant.as_ref().map(|f| f.as_str()).unwrap_or("-"),
                summary
                    .sovereign
                    .as_ref()
                    .map(|f| f.as_str())
                    .unwrap_or("-"),
                summary
                    .orbital_controller
                    .as_ref()
                    .map(|f| f.as_str())
                    .unwrap_or("-"),
                summary
                    .economic_hegemon
                    .as_ref()
                    .map(|f| f.as_str())
                    .unwrap_or("-"),
                summary
                    .hidden_master
                    .as_ref()
                    .map(|f| f.as_str())
                    .unwrap_or("-"),
            ),
        )
        .on_hover_text("Live read-only summary derived from the worlds' presence rows.");

        // Echo derived world-control for the selected world to make the
        // §C3 dominance loop legible from the system view too.
        if let Some(wid) = state.selection.world_id.clone() {
            if let Some((wsi, wwi)) = state.find_world_indices(&wid) {
                if wsi == sys_idx {
                    let w = &state.sector.systems[wsi].worlds[wwi];
                    let s = derive_world_control(w);
                    ui.colored_label(
                        Color32::DARK_GRAY,
                        format!(
                            "World {}: dominant {} · sovereign {} · occupier {} · score {:.1} · contested {}",
                            w.name,
                            s.dominant.as_ref().map(|f| f.as_str()).unwrap_or("-"),
                            s.sovereign.as_ref().map(|f| f.as_str()).unwrap_or("-"),
                            s.occupier.as_ref().map(|f| f.as_str()).unwrap_or("-"),
                            s.control_score,
                            s.contested
                        ),
                    )
                    .on_hover_text("Live read-only summary derived from this world's presence rows.");
                }
            }
        }
    });
}

// ── C6 PowerProfile preview ──────────────────────────────────────────────

fn show_power_profile_preview(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "ctrl_power_profile",
        "Power projection preview (§C6)",
        false,
        |ui| {
            let power = aggregate_faction_power(&state.sector.systems);
            if power.is_empty() {
                ui_kit::placeholder(
                    ui,
                    "No presence rows yet. Assign faction presence on worlds above to see projected power here.",
                );
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("c6_scroll")
                .max_height(220.0)
                .show(ui, |ui| {
                    egui::Grid::new("c6_grid")
                        .num_columns(10)
                        .striped(true)
                        .show(ui, |ui| {
                            for (h, tip) in POWER_COLUMNS {
                                ui.label(RichText::new(*h).strong().monospace())
                                    .on_hover_text(*tip);
                            }
                            ui.end_row();
                            for (fid, p) in &power {
                                ui.label(RichText::new(fid.as_str()).monospace())
                                    .on_hover_text(format!("id: {fid}"));
                                for v in [
                                    p.administrative,
                                    p.military,
                                    p.naval,
                                    p.economic,
                                    p.industrial,
                                    p.ideological,
                                    p.covert,
                                    p.logistical,
                                    p.legitimacy,
                                ] {
                                    ui.label(
                                        RichText::new(format!("{v:5.1}"))
                                            .monospace()
                                            .color(power_color(v)),
                                    );
                                }
                                ui.label(
                                    RichText::new(format!("{:5.1}", p.total_projection()))
                                        .strong()
                                        .monospace(),
                                );
                                ui.end_row();
                            }
                        });
                });
            if ui
                .button("↺ Apply to faction totals")
                .on_hover_text(
                    "Write these projected numbers back onto each faction's stored power totals",
                )
                .clicked()
            {
                // §R4 (IMPROVEMENT_REVIEW E1): route the bulk power overwrite through
                // the command bus so it is undoable. `before` is captured on apply;
                // the bus rails handle dirty + validation/derivation invalidation.
                let after = aggregate_faction_power(&state.sector.systems);
                if let Err(e) = state.run(BuilderCommand::ApplyFactionPower {
                    before: Vec::new(),
                    after,
                }) {
                    state.feedback.modal = Some(ModalKind::Message(format!("Apply failed: {e}")));
                }
            }
        },
    );
}

fn power_color(v: f32) -> Color32 {
    if v >= 30.0 {
        palette::success()
    } else if v >= 10.0 {
        palette::warning()
    } else if v >= 1.0 {
        Color32::LIGHT_GRAY
    } else {
        Color32::DARK_GRAY
    }
}

// ── CL3 contested summary ─────────────────────────────────────────────────

fn contested_worlds(state: &BuilderState) -> Vec<(usize, usize, BTreeSet<FactionId>)> {
    let mut out = Vec::new();
    for (si, sys) in state.sector.systems.iter().enumerate() {
        for (wi, w) in sys.worlds.iter().enumerate() {
            let distinct: BTreeSet<FactionId> =
                w.claims.iter().map(|c| c.faction_id.clone()).collect();
            if distinct.len() > 1 {
                out.push((si, wi, distinct));
            }
        }
    }
    out
}

fn show_contested_summary(ui: &mut Ui, state: &mut BuilderState) {
    let contested = contested_worlds(state);
    ui_kit::collapsing_section(
        ui,
        "ctrl_contested",
        &format!("Contested worlds ({})", contested.len()),
        false,
        |ui| {
            if contested.is_empty() {
                ui_kit::placeholder(
                    ui,
                    "No contested worlds. A world becomes contested once two or more different factions claim it.",
                );
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("cl3_scroll")
                .max_height(140.0)
                .show(ui, |ui| {
                    for (si, wi, distinct) in contested {
                        let w = &state.sector.systems[si].worlds[wi];
                        let label = format!(
                            "{}  ({} distinct claimants, {} claims)",
                            w.name,
                            distinct.len(),
                            w.claims.len()
                        );
                        let wid = w.id.clone();
                        let sys_id = state.sector.systems[si].id.clone();
                        ui.horizontal(|ui| {
                            if ui
                                .small_button("🌐 Open world")
                                .on_hover_text("Jump to this world in the WORLD tab")
                                .clicked()
                            {
                                state.focus_entity(EntityRef::World {
                                    system: sys_id.clone(),
                                    world: wid,
                                });
                            }
                            ui.label(RichText::new(label).color(palette::warning()));
                        });
                    }
                });
        },
    );
}

// ── CL4 bulk convert ──────────────────────────────────────────────────────

fn show_bulk_convert(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "ctrl_bulk_convert",
        "Bulk convert claims",
        false,
        |ui| {
            let factions: Vec<(FactionId, String)> = state
                .sector
                .factions
                .iter()
                .map(|f| (f.id.clone(), f.name.to_string()))
                .collect();
            if factions.is_empty() {
                ui_kit::placeholder(
                    ui,
                    "No factions in this sector yet. Add some in the FACTIONS tab to convert their claims.",
                );
                return;
            }
            let id = egui::Id::new("cl4_bulk_buf");
            #[derive(Clone)]
            struct Buf {
                faction: FactionId,
                from: ClaimType,
                to: ClaimType,
            }
            let default = Buf {
                faction: factions[0].0.clone(),
                from: ClaimType::LegalSovereignty,
                to: ClaimType::LegalSovereignty,
            };
            let mut buf: Buf = ui.data_mut(|d| d.get_temp::<Buf>(id).unwrap_or(default));

            ui_kit::placeholder(
                ui,
                "Rewrite every claim of one kind held by a faction into another kind, across all worlds, in one step.",
            );
            ui.horizontal(|ui| {
                ui.label("Faction:");
                let label = factions
                    .iter()
                    .find(|(fid, _)| fid == &buf.faction)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_else(|| "(none)".into());
                ui_kit::combo("cl4_faction", label).show_ui(ui, |ui| {
                    for (fid, n) in &factions {
                        if ui
                            .selectable_label(&buf.faction == fid, n)
                            .on_hover_text(format!("id: {fid}"))
                            .clicked()
                        {
                            buf.faction = fid.clone();
                        }
                    }
                });
                ui.label("From:");
                ui_kit::combo("cl4_from", claim_label(buf.from)).show_ui(ui, |ui| {
                    for k in CLAIM_TYPES {
                        ui.selectable_value(&mut buf.from, *k, claim_label(*k))
                            .on_hover_text(format!("schema: {}", k.as_slug()));
                    }
                });
                ui.label("Into:");
                ui_kit::combo("cl4_to", claim_label(buf.to)).show_ui(ui, |ui| {
                    for k in CLAIM_TYPES {
                        ui.selectable_value(&mut buf.to, *k, claim_label(*k))
                            .on_hover_text(format!("schema: {}", k.as_slug()));
                    }
                });
            });

            let matches = count_bulk_matches(state, &buf.faction, buf.from);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{matches} matching claim(s)")).color(Color32::DARK_GRAY),
                );
                let same = buf.from == buf.to;
                let disabled = matches == 0 || same;
                let btn = egui::Button::new(format!("🔁 Convert {matches} claim(s)"));
                if ui
                    .add_enabled(!disabled, btn)
                    .on_hover_text("Rewrite every matching claim to the new kind (one undo step)")
                    .clicked()
                {
                    // §R4 (CTL-4): one undo step across all converted worlds;
                    // apply_bulk_convert dispatches BulkEditWorlds (sets dirty +
                    // re-runs invariants), so no manual dirty/validation here.
                    let _ = apply_bulk_convert(state, &buf.faction, buf.from, buf.to);
                }
                if same {
                    ui.colored_label(Color32::DARK_GRAY, "From and Into match — nothing to do");
                }
            });

            ui.data_mut(|d| d.insert_temp(id, buf));
        },
    );
}

fn count_bulk_matches(state: &BuilderState, faction: &FactionId, from: ClaimType) -> usize {
    let mut n = 0usize;
    for sys in &state.sector.systems {
        for w in &sys.worlds {
            for c in &w.claims {
                if &c.faction_id == faction && c.claim_type == from {
                    n += 1;
                }
            }
        }
    }
    n
}

/// §R4 (CTL-4 / §CL4): rewrite every matching claim across all worlds in a
/// single undo step via [`BuilderCommand::BulkEditWorlds`]. Read-clones each
/// affected world, mutates the clone's `claims`, and collects the
/// `(world_id, draft)` pairs; the live sector is only touched through
/// `state.run`. Returns the number of claims converted (0 if none).
fn apply_bulk_convert(
    state: &mut BuilderState,
    faction: &FactionId,
    from: ClaimType,
    to: ClaimType,
) -> usize {
    let mut n = 0usize;
    let mut after: Vec<(WorldId, sectorforge::sector_model::GeneratedWorld)> = Vec::new();
    for sys in &state.sector.systems {
        for w in &sys.worlds {
            let mut changed = 0usize;
            for c in &w.claims {
                if &c.faction_id == faction && c.claim_type == from {
                    changed += 1;
                }
            }
            if changed > 0 {
                let mut draft = w.clone();
                for c in &mut draft.claims {
                    if &c.faction_id == faction && c.claim_type == from {
                        c.claim_type = to;
                    }
                }
                after.push((w.id.clone(), draft));
                n += changed;
            }
        }
    }
    if !after.is_empty() {
        if let Err(e) = state.run(BuilderCommand::BulkEditWorlds {
            before: Vec::new(),
            after,
        }) {
            state.feedback.modal = Some(ModalKind::Message(format!("Bulk convert failed: {e}")));
        }
    }
    n
}

// ── Public helpers used by the MAP overlay (§C7 / §C8) ───────────────────

/// §C7 / §C8: build the per-system overlay cell map the MAP panel feeds into
/// [`sectorforge_gui_core::sector_view::SectorView::heatmap`] when
/// [`crate::builder::state::ControlOverlay`] is on. The returned map is
/// keyed by system id; [`None`] when the overlay is `None`.
#[must_use]
pub(crate) fn build_overlay_cells(
    sector: &sectorforge::sector_model::GeneratedSector,
    factions: &[sectorforge::sector_model::GeneratedFaction],
    overlay: ControlOverlay,
    cache: Option<&SectorMapCache>,
) -> Option<std::collections::HashMap<SystemId, sectorforge_gui_core::heatmap::HeatCell>> {
    use sectorforge_gui_core::heatmap::HeatCell;
    match overlay {
        ControlOverlay::None => None,
        ControlOverlay::PowerProjection => {
            let map = sectorforge::power_projection::project_sector(sector);
            let max = map
                .by_faction
                .values()
                .flat_map(|m| m.values().copied())
                .fold(0.0_f32, f32::max)
                .max(1.0);
            let mut out: std::collections::HashMap<SystemId, HeatCell> =
                std::collections::HashMap::new();
            for sys in &sector.systems {
                let best = sectorforge::power_projection::system_top_reach(&map, sys.id.as_str());
                if let Some((fid, v)) = best {
                    let style = cache
                        .and_then(|mc| mc.faction_style(fid.as_str()).copied())
                        .unwrap_or_else(|| {
                            sectorforge_gui_core::palette::faction_style_by_id(
                                factions,
                                fid.as_str(),
                            )
                        });
                    out.insert(
                        sys.id.clone(),
                        HeatCell {
                            color: style.fill,
                            intensity: (v / max).clamp(0.0, 1.0),
                        },
                    );
                }
            }
            Some(out)
        }
        ControlOverlay::InfluenceField => {
            let field = sectorforge::influence_field::build(sector);
            let width = field.width as i32;
            let mut out: std::collections::HashMap<SystemId, HeatCell> =
                std::collections::HashMap::new();
            for sys in &sector.systems {
                let q = sys.coord.q;
                let r = sys.coord.r;
                if q < 0 || r < 0 || q >= width || r >= field.height as i32 {
                    continue;
                }
                let idx = (r as usize) * (field.width as usize) + q as usize;
                let Some(cell) = field.cells.get(idx) else {
                    continue;
                };
                let Some(fid) = cell.dominant.as_ref() else {
                    continue;
                };
                let style = cache
                    .and_then(|mc| mc.faction_style(fid.as_str()).copied())
                    .unwrap_or_else(|| {
                        sectorforge_gui_core::palette::faction_style_by_id(factions, fid.as_str())
                    });
                out.insert(
                    sys.id.clone(),
                    HeatCell {
                        color: style.fill,
                        intensity: (cell.score as f32 / 100.0).clamp(0.0, 1.0),
                    },
                );
            }
            Some(out)
        }
        ControlOverlay::Administrative
        | ControlOverlay::Military
        | ControlOverlay::Orbital
        | ControlOverlay::Naval
        | ControlOverlay::Mercantile
        | ControlOverlay::Industrial
        | ControlOverlay::Logistical
        | ControlOverlay::Informational
        | ControlOverlay::Religious
        | ControlOverlay::Sympathetic => {
            Some(build_dimension_overlay(sector, factions, overlay, cache))
        }
    }
}

/// Compute per-system top-faction tints for the 10 PresenceDimensions overlays.
/// Aggregates each world's presence rows by faction, sums the chosen axis, and
/// picks the highest scorer in each system. Intensity scales against the
/// sector-wide max so empty / low cells fade out and stronghold cells saturate.
fn build_dimension_overlay(
    sector: &sectorforge::sector_model::GeneratedSector,
    factions: &[sectorforge::sector_model::GeneratedFaction],
    overlay: ControlOverlay,
    cache: Option<&SectorMapCache>,
) -> std::collections::HashMap<SystemId, sectorforge_gui_core::heatmap::HeatCell> {
    use sectorforge::sector_model::PresenceDimensions;
    use sectorforge_gui_core::heatmap::HeatCell;
    let axis = |d: &PresenceDimensions| -> f32 {
        match overlay {
            ControlOverlay::Administrative => d.admin,
            ControlOverlay::Military => d.military,
            ControlOverlay::Orbital => d.orbital,
            ControlOverlay::Naval => 0.5 * (d.military + d.orbital),
            ControlOverlay::Mercantile => d.economic,
            ControlOverlay::Industrial => d.industrial,
            ControlOverlay::Logistical => d.logistics,
            ControlOverlay::Informational => d.covert,
            ControlOverlay::Religious => d.ideological,
            ControlOverlay::Sympathetic => d.legitimacy,
            _ => 0.0,
        }
    };
    let mut per_sys: Vec<(SystemId, std::collections::BTreeMap<FactionId, f32>)> = Vec::new();
    let mut global_max = 1.0_f32;
    for sys in &sector.systems {
        let mut by_fac: std::collections::BTreeMap<FactionId, f32> =
            std::collections::BTreeMap::new();
        for w in &sys.worlds {
            for p in &w.factions {
                let v = axis(&p.dimensions);
                if v > 0.0 {
                    *by_fac.entry(p.faction_id.clone()).or_insert(0.0) += v;
                }
            }
        }
        if let Some(&m) = by_fac.values().max_by(|a, b| a.total_cmp(b)) {
            global_max = global_max.max(m);
        }
        per_sys.push((sys.id.clone(), by_fac));
    }
    let mut out: std::collections::HashMap<SystemId, HeatCell> = std::collections::HashMap::new();
    for (sid, by_fac) in per_sys {
        let Some((fid, score)) = by_fac
            .iter()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(f, s)| (f.clone(), *s))
        else {
            continue;
        };
        if score <= 0.0 {
            continue;
        }
        let style = cache
            .and_then(|mc| mc.faction_style(fid.as_str()).copied())
            .unwrap_or_else(|| {
                sectorforge_gui_core::palette::faction_style_by_id(factions, fid.as_str())
            });
        out.insert(
            sid,
            HeatCell {
                color: style.fill,
                intensity: (score / global_max).clamp(0.0, 1.0),
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::{FactionClaim, GeneratedSector, HexCoord};

    fn faction_id(s: &str) -> FactionId {
        FactionId::from(s)
    }

    fn empty() -> GeneratedSector {
        GeneratedSector::empty("t", "T", "seed", 8, 8)
    }

    fn install_world_with_claims(s: &mut GeneratedSector, claims: Vec<FactionClaim>) {
        let sys_id = s.add_system(HexCoord { q: 0, r: 0 }, "Sys").unwrap();
        let _ = s.add_world_to_system(&sys_id, "World").unwrap();
        if let Some(sys) = s.systems.first_mut() {
            if let Some(w) = sys.worlds.first_mut() {
                w.claims = claims;
            }
        }
    }

    #[test]
    fn cl3_contested_when_distinct_claimants_gt_1() {
        let fa = faction_id("alpha");
        let fb = faction_id("beta");
        let mut s = empty();
        install_world_with_claims(
            &mut s,
            vec![
                FactionClaim {
                    faction_id: fa.clone(),
                    claim_type: ClaimType::LegalSovereignty,
                    strength: 50,
                },
                FactionClaim {
                    faction_id: fb,
                    claim_type: ClaimType::Rebellion,
                    strength: 30,
                },
                FactionClaim {
                    faction_id: fa,
                    claim_type: ClaimType::MilitaryOccupation,
                    strength: 60,
                },
            ],
        );
        let mut contested = 0usize;
        for sys in &s.systems {
            for w in &sys.worlds {
                let distinct: BTreeSet<FactionId> =
                    w.claims.iter().map(|c| c.faction_id.clone()).collect();
                if distinct.len() > 1 {
                    contested += 1;
                }
            }
        }
        assert_eq!(contested, 1);
    }

    #[test]
    fn cl4_bulk_match_count_predicate() {
        let fa = faction_id("alpha");
        let mut s = empty();
        install_world_with_claims(
            &mut s,
            vec![
                FactionClaim {
                    faction_id: fa.clone(),
                    claim_type: ClaimType::LegalSovereignty,
                    strength: 50,
                },
                FactionClaim {
                    faction_id: fa.clone(),
                    claim_type: ClaimType::LegalSovereignty,
                    strength: 70,
                },
                FactionClaim {
                    faction_id: fa.clone(),
                    claim_type: ClaimType::Rebellion,
                    strength: 30,
                },
            ],
        );
        let mut matches = 0usize;
        for sys in &s.systems {
            for w in &sys.worlds {
                for c in &w.claims {
                    if c.faction_id == fa && c.claim_type == ClaimType::LegalSovereignty {
                        matches += 1;
                    }
                }
            }
        }
        assert_eq!(matches, 2);
    }

    // ── §C7 / §C8 overlay helpers ──────────────────────────────────────

    #[test]
    fn build_overlay_returns_none_for_off() {
        let s = empty();
        assert!(build_overlay_cells(&s, &s.factions, ControlOverlay::None, None).is_none());
    }

    #[test]
    fn build_overlay_power_projection_keys_systems_with_power() {
        use sectorforge::sector_model::{GeneratedFaction, PowerProfile};
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        s.add_route(
            &a,
            &b,
            sectorforge::sector_model::RouteType::StableWarpLane,
            sectorforge::sector_model::RouteStability::Stable,
        )
        .unwrap();
        s.factions.push(GeneratedFaction {
            id: faction_id("alpha"),
            name: "Alpha".into(),
            kind: "imperial".into(),
            disposition: "lawful".into(),
            subfactions: Vec::new(),
            system_presence: vec![a.clone()],
            world_presence: vec![],
            power: PowerProfile {
                military: 50.0,
                ..Default::default()
            },
        });
        let cells = build_overlay_cells(&s, &s.factions, ControlOverlay::PowerProjection, None)
            .expect("PowerProjection always returns a map");
        assert!(cells.contains_key(&a));
        assert!(cells.contains_key(&b), "BFS reaches one-hop neighbour");
    }

    #[test]
    fn build_overlay_influence_field_handles_empty_sector() {
        let s = empty();
        let cells = build_overlay_cells(&s, &s.factions, ControlOverlay::InfluenceField, None)
            .expect("InfluenceField always returns a map");
        assert!(cells.is_empty(), "empty sector → empty influence map");
    }
}
