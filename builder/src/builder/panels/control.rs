//! CONTROL tab (§N1 / §N2). Phase B §CL1..§CL4 claims editor.
//!
//! The §C1..§C8 presence / dominance / control-state surface lands with Phase C
//! (§11). CL1..CL4 ship here first because claims live directly on
//! [`GeneratedWorld::claims`] and can be edited in isolation.
//!
//! CL1  chip-row per world (faction_id, ClaimType, strength) + remove.
//! CL2  add-claim picker. Multiple claims per faction allowed.
//! CL3  Contested auto-flag — N>1 distinct claimants on a world. Surfaced as
//!      a badge on the world row and an aggregate header at the top.
//! CL4  Bulk convert: every claim of kind X by faction Y becomes kind Z. Runs
//!      across every world in the sector in one pass.

use std::collections::BTreeSet;

use egui::{Color32, RichText, Ui};

use sectorforge::ids::{FactionId, WorldId};
use sectorforge::sector_model::{ClaimType, FactionClaim};

use crate::builder::state::BuilderTab;
use crate::builder::BuilderState;

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

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Control");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "§CL1..§CL4 claims editor. Presence / dominance / control-state (§C1..§C8) land with Phase C.",
    );
    ui.separator();

    show_contested_summary(ui, state);
    ui.separator();
    show_bulk_convert(ui, state);
    ui.separator();
    show_world_list(ui, state);
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
    egui::CollapsingHeader::new(format!("§CL3 — Contested ({})", contested.len()))
        .default_open(true)
        .show(ui, |ui| {
            if contested.is_empty() {
                ui.colored_label(
                    Color32::GRAY,
                    "no contested worlds — every world has ≤ 1 distinct claimant.",
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
                        ui.horizontal(|ui| {
                            if ui.small_button("→ WORLD").clicked() {
                                state.selected_world_id = Some(wid);
                                state.active_tab = BuilderTab::World;
                            }
                            ui.label(RichText::new(label).color(Color32::from_rgb(220, 160, 80)));
                        });
                    }
                });
        });
}

// ── CL4 bulk convert ──────────────────────────────────────────────────────

fn show_bulk_convert(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("§CL4 — Bulk convert claims")
        .default_open(false)
        .show(ui, |ui| {
            let factions: Vec<(FactionId, String)> = state
                .sector
                .factions
                .iter()
                .map(|f| (f.id.clone(), f.name.to_string()))
                .collect();
            if factions.is_empty() {
                ui.colored_label(Color32::GRAY, "no factions in this sector.");
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

            ui.horizontal(|ui| {
                ui.label("faction Y:");
                let label = factions
                    .iter()
                    .find(|(fid, _)| fid == &buf.faction)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_else(|| "(none)".into());
                egui::ComboBox::from_id_salt("cl4_faction")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for (fid, n) in &factions {
                            if ui.selectable_label(&buf.faction == fid, n).clicked() {
                                buf.faction = fid.clone();
                            }
                        }
                    });
                ui.label("claim X:");
                egui::ComboBox::from_id_salt("cl4_from")
                    .selected_text(format!("{:?}", buf.from))
                    .show_ui(ui, |ui| {
                        for k in CLAIM_TYPES {
                            ui.selectable_value(&mut buf.from, *k, format!("{k:?}"));
                        }
                    });
                ui.label("→ Z:");
                egui::ComboBox::from_id_salt("cl4_to")
                    .selected_text(format!("{:?}", buf.to))
                    .show_ui(ui, |ui| {
                        for k in CLAIM_TYPES {
                            ui.selectable_value(&mut buf.to, *k, format!("{k:?}"));
                        }
                    });
            });

            let matches = count_bulk_matches(state, &buf.faction, buf.from);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("matches: {matches}"))
                        .color(Color32::DARK_GRAY)
                        .monospace(),
                );
                let same = buf.from == buf.to;
                let disabled = matches == 0 || same;
                let btn = egui::Button::new(format!("Convert {matches} claims"));
                if ui.add_enabled(!disabled, btn).clicked() {
                    let n = apply_bulk_convert(state, &buf.faction, buf.from, buf.to);
                    if n > 0 {
                        state.dirty = true;
                        state.mark_validation_dirty();
                    }
                }
                if same {
                    ui.colored_label(Color32::GRAY, "X = Z — nothing to do");
                }
            });

            ui.data_mut(|d| d.insert_temp(id, buf));
        });
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

fn apply_bulk_convert(
    state: &mut BuilderState,
    faction: &FactionId,
    from: ClaimType,
    to: ClaimType,
) -> usize {
    let mut n = 0usize;
    for sys in &mut state.sector.systems {
        for w in &mut sys.worlds {
            for c in &mut w.claims {
                if &c.faction_id == faction && c.claim_type == from {
                    c.claim_type = to;
                    n += 1;
                }
            }
        }
    }
    n
}

// ── CL1 + CL2 per-world chip-row ─────────────────────────────────────────

fn show_world_list(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        ui.label("filter:");
        let id = egui::Id::new("cl_world_filter");
        let mut filter: String = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
        let r = ui.add(egui::TextEdit::singleline(&mut filter).desired_width(180.0));
        if r.changed() {
            ui.data_mut(|d| d.insert_temp(id, filter.clone()));
        }
        let only_contested_id = egui::Id::new("cl_only_contested");
        let mut only_contested: bool =
            ui.data_mut(|d| d.get_temp::<bool>(only_contested_id).unwrap_or(false));
        if ui.checkbox(&mut only_contested, "contested only").changed() {
            ui.data_mut(|d| d.insert_temp(only_contested_id, only_contested));
        }
    });

    let filter: String = ui.data_mut(|d| {
        d.get_temp::<String>(egui::Id::new("cl_world_filter"))
            .unwrap_or_default()
    });
    let only_contested: bool = ui.data_mut(|d| {
        d.get_temp::<bool>(egui::Id::new("cl_only_contested"))
            .unwrap_or(false)
    });
    let needle = filter.trim().to_lowercase();

    let factions: Vec<(FactionId, String)> = state
        .sector
        .factions
        .iter()
        .map(|f| (f.id.clone(), f.name.to_string()))
        .collect();

    let mut rows: Vec<(usize, usize)> = Vec::new();
    for (si, sys) in state.sector.systems.iter().enumerate() {
        for (wi, w) in sys.worlds.iter().enumerate() {
            if !needle.is_empty() && !w.name.to_lowercase().contains(&needle) {
                continue;
            }
            let distinct: BTreeSet<&FactionId> = w.claims.iter().map(|c| &c.faction_id).collect();
            let contested = distinct.len() > 1;
            if only_contested && !contested {
                continue;
            }
            rows.push((si, wi));
        }
    }

    egui::ScrollArea::vertical()
        .id_salt("cl_world_scroll")
        .show(ui, |ui| {
            if rows.is_empty() {
                ui.colored_label(Color32::GRAY, "no worlds match the current filter.");
                return;
            }
            for (si, wi) in rows {
                show_world_row(ui, state, si, wi, &factions);
                ui.add_space(2.0);
            }
        });
}

fn show_world_row(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
    factions: &[(FactionId, String)],
) {
    let (name, wid, claims_snapshot, contested) = {
        let w = &state.sector.systems[sys_idx].worlds[w_idx];
        let distinct: BTreeSet<FactionId> = w.claims.iter().map(|c| c.faction_id.clone()).collect();
        (
            w.name.to_string(),
            w.id.clone(),
            w.claims.clone(),
            distinct.len() > 1,
        )
    };

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(name).strong());
            if contested {
                ui.label(
                    RichText::new("CONTESTED")
                        .color(Color32::from_rgb(220, 160, 80))
                        .monospace(),
                );
            }
            ui.label(
                RichText::new(format!("{} claims", claims_snapshot.len()))
                    .color(Color32::DARK_GRAY)
                    .monospace(),
            );
            if ui.small_button("→ WORLD").clicked() {
                state.selected_world_id = Some(wid.clone());
                state.active_tab = BuilderTab::World;
            }
        });

        let mut remove: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (i, c) in claims_snapshot.iter().enumerate() {
                let (bg, fg) = claim_chip_colours(c.claim_type);
                egui::Frame::none()
                    .fill(bg)
                    .stroke(egui::Stroke::new(1.0, fg))
                    .rounding(4.0)
                    .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let label =
                                format!("{}  {:?}  {}", c.faction_id, c.claim_type, c.strength);
                            ui.label(RichText::new(label).color(fg).monospace());
                            if ui.small_button("×").clicked() {
                                remove = Some(i);
                            }
                        });
                    });
            }
            if claims_snapshot.is_empty() {
                ui.colored_label(Color32::GRAY, "no claims");
            }
        });
        if let Some(i) = remove {
            state.sector.systems[sys_idx].worlds[w_idx].claims.remove(i);
            state.dirty = true;
            state.mark_validation_dirty();
        }

        show_add_claim_row(ui, state, sys_idx, w_idx, &wid, factions);
    });
}

fn show_add_claim_row(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
    wid: &WorldId,
    factions: &[(FactionId, String)],
) {
    if factions.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "no factions in this sector — add factions to enable claims.",
        );
        return;
    }
    let row_id = egui::Id::new(("cl_add_claim", wid.as_str()));
    #[derive(Clone)]
    struct AddBuf {
        faction: FactionId,
        claim_type: ClaimType,
        strength: u8,
    }
    let default = AddBuf {
        faction: factions[0].0.clone(),
        claim_type: ClaimType::LegalSovereignty,
        strength: 50,
    };
    let mut buf: AddBuf = ui.data_mut(|d| d.get_temp::<AddBuf>(row_id).unwrap_or(default));

    ui.horizontal(|ui| {
        ui.label("add:");
        let label = factions
            .iter()
            .find(|(fid, _)| fid == &buf.faction)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "(none)".into());
        egui::ComboBox::from_id_salt(("cl_add_fac", wid.as_str()))
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (fid, n) in factions {
                    if ui.selectable_label(&buf.faction == fid, n).clicked() {
                        buf.faction = fid.clone();
                    }
                }
            });
        egui::ComboBox::from_id_salt(("cl_add_kind", wid.as_str()))
            .selected_text(format!("{:?}", buf.claim_type))
            .show_ui(ui, |ui| {
                for k in CLAIM_TYPES {
                    ui.selectable_value(&mut buf.claim_type, *k, format!("{k:?}"));
                }
            });
        ui.add(egui::DragValue::new(&mut buf.strength).range(0..=100));
        if ui.button("+ claim").clicked() {
            state.sector.systems[sys_idx].worlds[w_idx]
                .claims
                .push(FactionClaim {
                    faction_id: buf.faction.clone(),
                    claim_type: buf.claim_type,
                    strength: buf.strength,
                });
            state.dirty = true;
            state.mark_validation_dirty();
        }
    });
    ui.data_mut(|d| d.insert_temp(row_id, buf));
}

fn claim_chip_colours(kind: ClaimType) -> (Color32, Color32) {
    match kind {
        ClaimType::LegalSovereignty => (Color32::from_rgb(40, 60, 100), Color32::LIGHT_BLUE),
        ClaimType::ImperialMandate => (Color32::from_rgb(80, 70, 30), Color32::YELLOW),
        ClaimType::TreatyRight => (Color32::from_rgb(40, 80, 80), Color32::LIGHT_GREEN),
        ClaimType::ReligiousMandate => (Color32::from_rgb(80, 60, 30), Color32::LIGHT_YELLOW),
        ClaimType::DynasticRight => (Color32::from_rgb(80, 30, 70), Color32::LIGHT_RED),
        ClaimType::CommercialCharter => (Color32::from_rgb(40, 90, 50), Color32::GREEN),
        ClaimType::MilitaryOccupation => (Color32::from_rgb(100, 30, 30), Color32::LIGHT_RED),
        ClaimType::AncientDomain => (Color32::from_rgb(50, 50, 60), Color32::LIGHT_GRAY),
        ClaimType::HuntingGround => (Color32::from_rgb(60, 50, 30), Color32::LIGHT_YELLOW),
        ClaimType::CovertWrit => (Color32::from_rgb(30, 30, 60), Color32::LIGHT_BLUE),
        ClaimType::Rebellion => (Color32::from_rgb(120, 30, 30), Color32::LIGHT_RED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::GeneratedSector;

    fn faction_id(s: &str) -> FactionId {
        FactionId::from(s)
    }

    fn empty() -> GeneratedSector {
        GeneratedSector::empty("t", "T", "seed", 8, 8)
    }

    fn install_world_with_claims(s: &mut GeneratedSector, claims: Vec<FactionClaim>) {
        let sys_id = s
            .add_system(sectorforge::sector_model::HexCoord { q: 0, r: 0 }, "Sys")
            .unwrap();
        let wid = s.add_world_to_system(&sys_id, "World").unwrap();
        let _ = wid;
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
}
