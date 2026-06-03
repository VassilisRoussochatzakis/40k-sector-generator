//! RELATIONS tab (§N1 / §N2) — Phase C §REL1..§REL9.
//!
//! §REL1  Diplomacy matrix grid. One row per derived [`FactionRelation`] with a
//!        click target that arms [`BuilderState::relations_selected_pair`] for
//!        the cell editor below.
//! §REL2  Cell editor exposes both the symmetric attitudes and the directional
//!        `a→b` / `b→a` view of the picked pair. Edits land in
//!        [`RelationOverride`] under `RelationsConfig::overrides`.
//! §REL3  Kind-pair base stance rules editor ([`KindRule`]).
//! §REL4  Disposition delta rules editor ([`DispositionRule`]).
//! §REL5  Legacy `(faction_id, faction_id)` stance pin editor
//!        ([`PairOverride`]).
//! §REL6  Rich [`RelationOverride`] editor (same cell editor as REL1).
//! §REL7  `feed_conflict` toggle — copied onto the derived matrix so the
//!        conflict tick can read it without a reload.
//! §REL8  `[generation.relations].min_world_presence` slider — drives the
//!        matrix size when [`BuilderState::recompute_relations`] runs.
//! §REL9  "Recompute matrix" button + auto-recompute on edit toggle.
//!
//! The panel never edits `sector.relations` directly — all mutations land in
//! [`BuilderState::data_catalogs::relations`] and a recompute pass rewrites the
//! published matrix.

use egui::{Color32, RichText, Ui};

use sectorforge::ids::FactionId;
use sectorforge::relations::{
    DispositionRule, KindRule, PairOverride, RelationAttitude, RelationOverride, RelationsConfig,
    Stance, TreatyStatus,
};
use sectorforge_gui_core::ui_kit;

use crate::builder::state::EntityRef;
use crate::builder::BuilderState;

const ATTITUDES: &[RelationAttitude] = &[
    RelationAttitude::Allied,
    RelationAttitude::Friendly,
    RelationAttitude::Transactional,
    RelationAttitude::Suspicious,
    RelationAttitude::Hostile,
    RelationAttitude::ExistentialEnemy,
];

const STANCES: &[Stance] = &[
    Stance::Allied,
    Stance::Aligned,
    Stance::Neutral,
    Stance::Rival,
    Stance::Hostile,
    Stance::AtWar,
];

const TREATIES: &[TreatyStatus] = &[
    TreatyStatus::None,
    TreatyStatus::Pact,
    TreatyStatus::Truce,
    TreatyStatus::Vassalage,
    TreatyStatus::Charter,
    TreatyStatus::Nonaggression,
    TreatyStatus::Vendetta,
];

const DEFAULT_RELATIONS_PATH: &str = "data/factions/relations.toml";

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Relations");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "diplomacy matrix, per-pair overrides, kind / disposition rules.",
    );
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_header_actions(ui, state);
            ui.separator();
            show_settings(ui, state);
            ui.separator();
            show_matrix_grid(ui, state);
            ui.separator();
            show_cell_editor(ui, state);
            ui.separator();
            show_pair_overrides(ui, state);
            ui.separator();
            show_kind_rules(ui, state);
            ui.separator();
            show_disposition_rules(ui, state);
            ui.separator();
            show_save_row(ui, state);
        });
}

// ── header actions (§REL9) ──────────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Recompute matrix").clicked() {
            state.recompute_relations();
        }
        ui.checkbox(
            &mut state.relations_auto_recompute,
            "auto-recompute on edit",
        );
        let pairs = state.sector.relations.pairs.len();
        let facs = state.sector.factions.len();
        ui.label(format!("factions: {facs}  |  pairs: {pairs}"));
        if state.data_catalogs.relations.is_none() {
            ui.colored_label(
                Color32::from_rgb(220, 170, 80),
                "no relations.toml loaded (defaults apply)",
            );
        }
    });
}

// ── §REL7 / §REL8 settings ──────────────────────────────────────────────────

fn show_settings(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("settings").strong());
    let mut feed = state
        .data_catalogs
        .relations
        .as_ref()
        .map(|c| c.feed_conflict)
        .unwrap_or(false);
    let mut min_pres = state.config.generation.relations.min_world_presence;
    let mut cfg_changed = false;
    let mut gen_changed = false;

    ui.horizontal_wrapped(|ui| {
        if ui
            .checkbox(&mut feed, "feed_conflict — bias conflict tick by stance")
            .changed()
        {
            cfg_changed = true;
        }
        ui.separator();
        ui.label("min_world_presence");
        if ui.add(egui::Slider::new(&mut min_pres, 1..=10)).changed() {
            gen_changed = true;
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            "matrix scope: factions with ≥ N world presences",
        );
    });

    if cfg_changed {
        ensure_relations_catalog(state);
        if let Some(cfg) = state.data_catalogs.relations.as_mut() {
            cfg.feed_conflict = feed;
        }
        on_catalog_edited(state);
    }
    if gen_changed {
        state.config.generation.relations.min_world_presence = min_pres;
        state.dirty = true;
        state.dirty_files.insert("sectorforge.toml".into());
        state.mark_validation_dirty();
        if state.relations_auto_recompute {
            state.recompute_relations();
        }
    }
}

// ── §REL1 matrix grid ───────────────────────────────────────────────────────

fn show_matrix_grid(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("diplomacy matrix").strong());
    if state.sector.relations.pairs.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "Matrix empty. Click Recompute matrix above (need ≥ 2 factions).",
        );
        return;
    }
    let selected = state.relations_selected_pair.clone();
    egui::ScrollArea::horizontal()
        .id_salt("rel_grid_scroll")
        .show(ui, |ui| {
            egui::Grid::new("rel_grid")
                .striped(true)
                .min_col_width(64.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("A").strong());
                    ui.label(RichText::new("B").strong());
                    ui.label(RichText::new("Public").strong());
                    ui.label(RichText::new("Secret").strong());
                    ui.label(RichText::new("Treaty").strong());
                    ui.label(RichText::new("Tension").strong());
                    ui.label(RichText::new("Trust").strong());
                    ui.label(RichText::new("Fear").strong());
                    ui.label(RichText::new("Rivalry").strong());
                    ui.label(RichText::new("").strong());
                    ui.end_row();

                    for rel in state.sector.relations.pairs.clone() {
                        let pair = (rel.a.clone(), rel.b.clone());
                        let is_selected = selected.as_ref() == Some(&pair);
                        if ui
                            .selectable_label(is_selected, rel.a.to_string())
                            .clicked()
                        {
                            state.relations_selected_pair = Some(pair.clone());
                        }
                        if ui
                            .selectable_label(is_selected, rel.b.to_string())
                            .clicked()
                        {
                            state.relations_selected_pair = Some(pair.clone());
                        }
                        ui.colored_label(
                            attitude_color(rel.public_attitude),
                            rel.public_attitude.label(),
                        );
                        ui.colored_label(
                            attitude_color(rel.secret_attitude),
                            rel.secret_attitude.label(),
                        );
                        ui.label(rel.treaty_status.label());
                        ui.label(tension_text(rel.tension));
                        ui.label(metric_text(rel.metrics.trust));
                        ui.label(metric_text(rel.metrics.fear));
                        ui.label(metric_text(rel.metrics.rivalry));
                        if ui.button("edit").clicked() {
                            state.relations_selected_pair = Some(pair.clone());
                        }
                        ui.end_row();
                    }
                });
        });
}

fn attitude_color(a: RelationAttitude) -> Color32 {
    match a {
        RelationAttitude::Allied => Color32::from_rgb(110, 220, 150),
        RelationAttitude::Friendly => Color32::from_rgb(170, 210, 130),
        RelationAttitude::Transactional => Color32::GRAY,
        RelationAttitude::Suspicious => Color32::from_rgb(220, 200, 110),
        RelationAttitude::Hostile => Color32::from_rgb(230, 140, 90),
        RelationAttitude::ExistentialEnemy => Color32::from_rgb(230, 90, 90),
        _ => Color32::GRAY,
    }
}

fn tension_text(t: f32) -> RichText {
    let colour = if t >= 70.0 {
        Color32::from_rgb(230, 90, 90)
    } else if t >= 40.0 {
        Color32::from_rgb(220, 170, 80)
    } else if t >= 15.0 {
        Color32::from_rgb(200, 200, 130)
    } else {
        Color32::GRAY
    };
    RichText::new(format!("{t:>5.1}")).color(colour).monospace()
}

fn metric_text(v: u8) -> RichText {
    let colour = if v >= 70 {
        Color32::from_rgb(220, 170, 80)
    } else if v >= 40 {
        Color32::LIGHT_GREEN
    } else {
        Color32::GRAY
    };
    RichText::new(format!("{v:>3}")).color(colour).monospace()
}

// ── §REL1 + §REL2 + §REL6 cell editor ───────────────────────────────────────

fn show_cell_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("pair editor").strong());
    let Some(pair) = state.relations_selected_pair.clone() else {
        ui.colored_label(Color32::GRAY, "Click a pair in the grid to edit.");
        return;
    };
    let Some(rel) = state
        .sector
        .relations
        .pairs
        .iter()
        .find(|p| p.a == pair.0 && p.b == pair.1)
        .cloned()
    else {
        ui.colored_label(
            Color32::GRAY,
            "Selected pair not in current matrix. Recompute or pick another.",
        );
        return;
    };

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("{} ↔ {}", rel.a, rel.b)).strong());
        ui.colored_label(
            Color32::DARK_GRAY,
            format!(
                "derived: public {}, secret {}, treaty {}, tension {:.1}",
                rel.public_attitude.label(),
                rel.secret_attitude.label(),
                rel.treaty_status.label(),
                rel.tension,
            ),
        );
        if ui.button("× clear selection").clicked() {
            state.relations_selected_pair = None;
        }
    });
    ui.colored_label(
        Color32::DARK_GRAY,
        format!(
            "cause: {}",
            if rel.cause.is_empty() {
                "—"
            } else {
                &rel.cause
            }
        ),
    );

    // Pull current override (if any) for this canonical pair.
    let mut ov = existing_override(state, &pair).unwrap_or_else(|| RelationOverride {
        a: pair.0.to_string(),
        b: pair.1.to_string(),
        ..RelationOverride::default()
    });
    let mut changed = false;

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("Symmetric attitude / treaty").italics());
        changed |= attitude_combo(ui, "public_attitude", "rel_pub", &mut ov.public_attitude);
        changed |= attitude_combo(ui, "secret_attitude", "rel_sec", &mut ov.secret_attitude);
        changed |= treaty_combo(ui, "treaty_status", "rel_treaty", &mut ov.treaty_status);
    });

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("Directional view").italics());
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(format!("{} → {}", rel.a, rel.b));
                ui.colored_label(
                    Color32::DARK_GRAY,
                    format!(
                        "derived: pub {}, sec {}",
                        rel.a_to_b.public_attitude.label(),
                        rel.a_to_b.secret_attitude.label()
                    ),
                );
                changed |=
                    attitude_combo(ui, "a→b public", "rel_ab_pub", &mut ov.a_public_attitude);
                changed |=
                    attitude_combo(ui, "a→b secret", "rel_ab_sec", &mut ov.a_secret_attitude);
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.label(format!("{} → {}", rel.b, rel.a));
                ui.colored_label(
                    Color32::DARK_GRAY,
                    format!(
                        "derived: pub {}, sec {}",
                        rel.b_to_a.public_attitude.label(),
                        rel.b_to_a.secret_attitude.label()
                    ),
                );
                changed |=
                    attitude_combo(ui, "b→a public", "rel_ba_pub", &mut ov.b_public_attitude);
                changed |=
                    attitude_combo(ui, "b→a secret", "rel_ba_sec", &mut ov.b_secret_attitude);
            });
        });
    });

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(
            RichText::new(
                "Dimensions (trust / fear / rivalry / ideological / economic / military / covert)",
            )
            .italics(),
        );
        changed |= u8_slider(ui, "trust", &mut ov.trust, rel.metrics.trust);
        changed |= u8_slider(ui, "fear", &mut ov.fear, rel.metrics.fear);
        changed |= u8_slider(ui, "rivalry", &mut ov.rivalry, rel.metrics.rivalry);
        changed |= u8_slider(
            ui,
            "ideological_distance",
            &mut ov.ideological_distance,
            rel.metrics.ideological_distance,
        );
        changed |= u8_slider(
            ui,
            "economic_dependency",
            &mut ov.economic_dependency,
            rel.metrics.economic_dependency,
        );
        changed |= u8_slider(
            ui,
            "military_pressure",
            &mut ov.military_pressure,
            rel.metrics.military_pressure,
        );
        changed |= u8_slider(
            ui,
            "covert_activity",
            &mut ov.covert_activity,
            rel.metrics.covert_activity,
        );
    });

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("Cause / reason override").italics());
        let mut buf = ov.reason.clone().unwrap_or_default();
        if ui.text_edit_singleline(&mut buf).changed() {
            ov.reason = if buf.is_empty() { None } else { Some(buf) };
            changed = true;
        }
    });

    let pinned = ov_has_any_value(&ov);
    ui.horizontal_wrapped(|ui| {
        if pinned && ui.button("Clear override for pair").clicked() {
            remove_override(state, &pair);
            on_catalog_edited(state);
            return;
        }
        let badge = if pinned {
            RichText::new("override pinned").color(Color32::LIGHT_GREEN)
        } else {
            RichText::new("derived (no override)").color(Color32::GRAY)
        };
        ui.label(badge);
        if ui.button("Jump to FACTIONS tab (a)").clicked() {
            state.focus_entity(EntityRef::Faction(pair.0.clone()));
        }
        if ui.button("Jump to FACTIONS tab (b)").clicked() {
            state.focus_entity(EntityRef::Faction(pair.1.clone()));
        }
    });

    if changed {
        upsert_override(state, ov);
        on_catalog_edited(state);
    }
}

fn ov_has_any_value(ov: &RelationOverride) -> bool {
    ov.public_attitude.is_some()
        || ov.secret_attitude.is_some()
        || ov.a_public_attitude.is_some()
        || ov.b_public_attitude.is_some()
        || ov.a_secret_attitude.is_some()
        || ov.b_secret_attitude.is_some()
        || ov.treaty_status.is_some()
        || ov.trust.is_some()
        || ov.fear.is_some()
        || ov.rivalry.is_some()
        || ov.ideological_distance.is_some()
        || ov.economic_dependency.is_some()
        || ov.military_pressure.is_some()
        || ov.covert_activity.is_some()
        || ov.reason.is_some()
}

fn existing_override(
    state: &BuilderState,
    pair: &(FactionId, FactionId),
) -> Option<RelationOverride> {
    let (lo, hi) = canonical_pair(pair);
    state
        .data_catalogs
        .relations
        .as_ref()?
        .overrides
        .iter()
        .find(|o| matches_pair(&o.a, &o.b, &lo, &hi))
        .cloned()
}

fn upsert_override(state: &mut BuilderState, ov: RelationOverride) {
    ensure_relations_catalog(state);
    let Some(cfg) = state.data_catalogs.relations.as_mut() else {
        return;
    };
    let (lo, hi) = canonical_pair(&(
        FactionId::from(ov.a.as_str()),
        FactionId::from(ov.b.as_str()),
    ));
    cfg.overrides
        .retain(|o| !matches_pair(&o.a, &o.b, &lo, &hi));
    cfg.overrides.push(RelationOverride {
        a: lo.to_string(),
        b: hi.to_string(),
        ..ov
    });
}

fn remove_override(state: &mut BuilderState, pair: &(FactionId, FactionId)) {
    let (lo, hi) = canonical_pair(pair);
    if let Some(cfg) = state.data_catalogs.relations.as_mut() {
        cfg.overrides
            .retain(|o| !matches_pair(&o.a, &o.b, &lo, &hi));
    }
}

fn canonical_pair(pair: &(FactionId, FactionId)) -> (FactionId, FactionId) {
    if pair.0 <= pair.1 {
        (pair.0.clone(), pair.1.clone())
    } else {
        (pair.1.clone(), pair.0.clone())
    }
}

fn matches_pair(a: &str, b: &str, lo: &FactionId, hi: &FactionId) -> bool {
    let (oa, ob) = if a <= b { (a, b) } else { (b, a) };
    oa == lo.as_ref() && ob == hi.as_ref()
}

fn attitude_combo(
    ui: &mut Ui,
    label: &str,
    id_salt: &str,
    field: &mut Option<RelationAttitude>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let current_label = field.map(RelationAttitude::label).unwrap_or("(inherit)");
        ui_kit::combo(id_salt, current_label).show_ui(ui, |ui| {
            if ui.selectable_label(field.is_none(), "(inherit)").clicked() {
                *field = None;
                changed = true;
            }
            for v in ATTITUDES {
                if ui.selectable_label(*field == Some(*v), v.label()).clicked() {
                    *field = Some(*v);
                    changed = true;
                }
            }
        });
    });
    changed
}

fn treaty_combo(ui: &mut Ui, label: &str, id_salt: &str, field: &mut Option<TreatyStatus>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let current_label = field.map(TreatyStatus::label).unwrap_or("(inherit)");
        ui_kit::combo(id_salt, current_label).show_ui(ui, |ui| {
            if ui.selectable_label(field.is_none(), "(inherit)").clicked() {
                *field = None;
                changed = true;
            }
            for v in TREATIES {
                if ui.selectable_label(*field == Some(*v), v.label()).clicked() {
                    *field = Some(*v);
                    changed = true;
                }
            }
        });
    });
    changed
}

fn stance_combo(ui: &mut Ui, id_salt: &str, field: &mut Stance) -> bool {
    let mut changed = false;
    ui_kit::combo(id_salt, field.label()).show_ui(ui, |ui| {
        for v in STANCES {
            if ui.selectable_label(*field == *v, v.label()).clicked() {
                *field = *v;
                changed = true;
            }
        }
    });
    changed
}

fn u8_slider(ui: &mut Ui, label: &str, field: &mut Option<u8>, derived: u8) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let mut enabled = field.is_some();
        if ui.checkbox(&mut enabled, "pin").changed() {
            *field = if enabled { Some(derived) } else { None };
            changed = true;
        }
        let mut value = field.unwrap_or(derived);
        let resp = ui.add_enabled(
            enabled,
            egui::Slider::new(&mut value, 0..=100).clamping(egui::SliderClamping::Always),
        );
        if resp.changed() {
            *field = Some(value);
            changed = true;
        }
        ui.colored_label(Color32::DARK_GRAY, format!("derived: {derived}"));
    });
    changed
}

// ── §REL5 legacy pair_overrides ─────────────────────────────────────────────

fn show_pair_overrides(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("pair_overrides (legacy stance pin)").strong());
    let factions: Vec<FactionId> = state.sector.factions.iter().map(|f| f.id.clone()).collect();
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;

    ensure_relations_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.relations.as_mut() else {
        ui.colored_label(
            Color32::GRAY,
            "No relations catalog. Load relations.toml or click any edit above.",
        );
        return;
    };

    ui.collapsing(format!("entries ({})", cfg.pair_overrides.len()), |ui| {
        for (idx, row) in cfg.pair_overrides.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} ↔ {}", row.a, row.b)).monospace());
                if stance_combo(ui, &format!("rel_po_st_{idx}"), &mut row.stance) {
                    changed = true;
                }
                let mut cause = row.cause.clone().unwrap_or_default();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut cause)
                            .hint_text("cause")
                            .desired_width(220.0),
                    )
                    .changed()
                {
                    row.cause = if cause.is_empty() { None } else { Some(cause) };
                    changed = true;
                }
                if ui
                    .button(RichText::new("× remove").color(Color32::LIGHT_RED))
                    .clicked()
                {
                    remove_idx = Some(idx);
                }
            });
        }
        if let Some(i) = remove_idx {
            cfg.pair_overrides.remove(i);
            changed = true;
        }
        ui.separator();
        ui.label(RichText::new("+ add pin").italics());
        if factions.len() < 2 {
            ui.colored_label(Color32::GRAY, "Need ≥ 2 factions to pin a pair.");
        } else {
            let a_default = factions[0].clone();
            let b_default = factions[1].clone();
            let mut add_a = a_default;
            let mut add_b = b_default;
            let mut add_stance = Stance::Neutral;
            ui.horizontal(|ui| {
                faction_combo(ui, "rel_po_add_a", &factions, &mut add_a);
                faction_combo(ui, "rel_po_add_b", &factions, &mut add_b);
                stance_combo(ui, "rel_po_add_st", &mut add_stance);
                if ui.button("add").clicked() && add_a != add_b {
                    let (lo, hi) = if add_a <= add_b {
                        (add_a.to_string(), add_b.to_string())
                    } else {
                        (add_b.to_string(), add_a.to_string())
                    };
                    cfg.pair_overrides.retain(|p| !(p.a == lo && p.b == hi));
                    cfg.pair_overrides.push(PairOverride {
                        a: lo,
                        b: hi,
                        stance: add_stance,
                        cause: None,
                    });
                    changed = true;
                }
            });
        }
    });

    if changed {
        on_catalog_edited(state);
    }
}

fn faction_combo(ui: &mut Ui, id_salt: &str, options: &[FactionId], value: &mut FactionId) {
    ui_kit::combo(id_salt, value.to_string()).show_ui(ui, |ui| {
        for f in options {
            if ui.selectable_label(value == f, f.as_ref()).clicked() {
                *value = f.clone();
            }
        }
    });
}

// ── §REL3 kind_rules ────────────────────────────────────────────────────────

fn show_kind_rules(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("kind_rules (kind × kind → base stance)").strong());
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;

    ensure_relations_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.relations.as_mut() else {
        ui.colored_label(Color32::GRAY, "No relations catalog loaded.");
        return;
    };
    ui.collapsing(format!("rules ({})", cfg.kind_rules.len()), |ui| {
        for (idx, row) in cfg.kind_rules.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut row.a)
                            .desired_width(140.0)
                            .hint_text("kind A"),
                    )
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut row.b)
                            .desired_width(140.0)
                            .hint_text("kind B"),
                    )
                    .changed()
                {
                    changed = true;
                }
                if stance_combo(ui, &format!("rel_kr_st_{idx}"), &mut row.stance) {
                    changed = true;
                }
                let mut cause = row.cause.clone().unwrap_or_default();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut cause)
                            .hint_text("cause")
                            .desired_width(220.0),
                    )
                    .changed()
                {
                    row.cause = if cause.is_empty() { None } else { Some(cause) };
                    changed = true;
                }
                if ui
                    .button(RichText::new("×").color(Color32::LIGHT_RED))
                    .clicked()
                {
                    remove_idx = Some(idx);
                }
            });
        }
        if let Some(i) = remove_idx {
            cfg.kind_rules.remove(i);
            changed = true;
        }
        ui.separator();
        if ui.button("+ kind_rule").clicked() {
            cfg.kind_rules.push(KindRule {
                a: String::new(),
                b: String::new(),
                stance: Stance::Neutral,
                cause: None,
            });
            changed = true;
        }
    });

    if changed {
        on_catalog_edited(state);
    }
}

// ── §REL4 disposition_rules ─────────────────────────────────────────────────

fn show_disposition_rules(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("disposition_rules (disposition × disposition → level delta)").strong());
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;

    ensure_relations_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.relations.as_mut() else {
        ui.colored_label(Color32::GRAY, "No relations catalog loaded.");
        return;
    };
    ui.collapsing(format!("rules ({})", cfg.disposition_rules.len()), |ui| {
        for (idx, row) in cfg.disposition_rules.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut row.a)
                            .desired_width(140.0)
                            .hint_text("disp A"),
                    )
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut row.b)
                            .desired_width(140.0)
                            .hint_text("disp B"),
                    )
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .add(egui::DragValue::new(&mut row.delta).range(-3..=3).speed(1))
                    .changed()
                {
                    changed = true;
                }
                let mut cause = row.cause.clone().unwrap_or_default();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut cause)
                            .hint_text("cause")
                            .desired_width(220.0),
                    )
                    .changed()
                {
                    row.cause = if cause.is_empty() { None } else { Some(cause) };
                    changed = true;
                }
                if ui
                    .button(RichText::new("×").color(Color32::LIGHT_RED))
                    .clicked()
                {
                    remove_idx = Some(idx);
                }
            });
        }
        if let Some(i) = remove_idx {
            cfg.disposition_rules.remove(i);
            changed = true;
        }
        ui.separator();
        if ui.button("+ disposition_rule").clicked() {
            cfg.disposition_rules.push(DispositionRule {
                a: String::new(),
                b: String::new(),
                delta: 0,
                cause: None,
            });
            changed = true;
        }
    });

    if changed {
        on_catalog_edited(state);
    }
}

// ── save row ────────────────────────────────────────────────────────────────

fn show_save_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("relations.toml").strong());
    let has_catalog = state.data_catalogs.relations.is_some();
    let has_path = state.config.inputs.relations.is_some();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(has_catalog, egui::Button::new("Save relations.toml"))
            .clicked()
        {
            if state.config.inputs.relations.is_none() {
                state.config.inputs.relations = Some(DEFAULT_RELATIONS_PATH.into());
            }
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Save relations.toml failed: {e}"
                )));
            }
        }
        let path_label = state
            .config
            .inputs
            .relations
            .clone()
            .unwrap_or_else(|| format!("(unset; will write to {DEFAULT_RELATIONS_PATH})"));
        ui.colored_label(Color32::DARK_GRAY, path_label);
        let _ = has_path;
    });
}

// ── shared helpers ──────────────────────────────────────────────────────────

fn ensure_relations_catalog(state: &mut BuilderState) {
    if state.data_catalogs.relations.is_none() {
        state.data_catalogs.relations = Some(RelationsConfig::default());
    }
    if state.config.inputs.relations.is_none() {
        state.config.inputs.relations = Some(DEFAULT_RELATIONS_PATH.into());
    }
}

fn ensure_relations_catalog_if_needed(state: &mut BuilderState) {
    if state.data_catalogs.relations.is_none() {
        state.data_catalogs.relations = Some(RelationsConfig::default());
    }
}

fn on_catalog_edited(state: &mut BuilderState) {
    state.dirty = true;
    if let Some(rel) = state.config.inputs.relations.clone() {
        state.dirty_files.insert(rel);
    } else {
        state.dirty_files.insert(DEFAULT_RELATIONS_PATH.into());
    }
    state.mark_validation_dirty();
    if state.relations_auto_recompute {
        state.recompute_relations();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::factions::FactionDef;
    use sectorforge::factions::FactionsFile;
    use sectorforge::sector_model::GeneratedSector;

    fn faction_def(id: &str, kind: &str, disposition: &str) -> FactionDef {
        FactionDef {
            id: id.into(),
            name: id.into(),
            faction: None,
            faction_name: None,
            subfaction: None,
            subfaction_name: None,
            kind: kind.into(),
            default_disposition: disposition.into(),
            weight: 1.0,
            preferred_world_types: Vec::new(),
            preferred_governments: Vec::new(),
            preferred_notable_features: Vec::new(),
            style_fill: None,
            style_accent: None,
            style_glyph: None,
            style_border: None,
            legend_visible: None,
        }
    }

    fn gen_faction(
        id: &str,
        kind: &str,
        disposition: &str,
    ) -> sectorforge::sector_model::GeneratedFaction {
        sectorforge::sector_model::GeneratedFaction {
            id: id.into(),
            name: std::sync::Arc::from(id),
            kind: std::sync::Arc::from(kind),
            disposition: std::sync::Arc::from(disposition),
            subfactions: Vec::new(),
            system_presence: vec![],
            world_presence: vec![],
            power: Default::default(),
        }
    }

    fn seed_state() -> BuilderState {
        let mut state = BuilderState::new_blank("rel-test", "Rel", "s", 4, 4);
        state.sector = GeneratedSector::empty("rel-test", "Rel", "s", 4, 4).into();
        state
            .sector
            .factions
            .push(gen_faction("imp", "imperial", "lawful"));
        state
            .sector
            .factions
            .push(gen_faction("chaos", "chaos_space_marine", "hostile"));
        state.data_catalogs.factions = Some(FactionsFile {
            factions: vec![
                faction_def("imp", "imperial", "lawful"),
                faction_def("chaos", "chaos_space_marine", "hostile"),
            ],
        });
        state
    }

    #[test]
    fn recompute_relations_builds_pair_for_two_factions() {
        let mut state = seed_state();
        assert!(state.sector.relations.pairs.is_empty());
        state.recompute_relations();
        assert_eq!(state.sector.relations.pairs.len(), 1);
        let pair = &state.sector.relations.pairs[0];
        assert_eq!(pair.stance, Stance::AtWar);
    }

    #[test]
    fn upsert_override_pins_attitude_through_recompute() {
        let mut state = seed_state();
        let pair = (FactionId::from("imp"), FactionId::from("chaos"));
        let (lo, hi) = canonical_pair(&pair);
        let ov = RelationOverride {
            a: lo.to_string(),
            b: hi.to_string(),
            public_attitude: Some(RelationAttitude::Friendly),
            ..RelationOverride::default()
        };
        upsert_override(&mut state, ov);
        state.recompute_relations();
        let rel = &state.sector.relations.pairs[0];
        assert_eq!(rel.public_attitude, RelationAttitude::Friendly);
    }

    #[test]
    fn pair_override_wins_over_kind_default() {
        let mut state = seed_state();
        ensure_relations_catalog(&mut state);
        let cfg = state.data_catalogs.relations.as_mut().unwrap();
        cfg.pair_overrides.push(PairOverride {
            a: "chaos".into(),
            b: "imp".into(),
            stance: Stance::Allied,
            cause: Some("test".into()),
        });
        state.recompute_relations();
        let rel = &state.sector.relations.pairs[0];
        assert_eq!(rel.stance, Stance::Allied);
    }

    #[test]
    fn feed_conflict_round_trips_to_matrix() {
        let mut state = seed_state();
        ensure_relations_catalog(&mut state);
        state
            .data_catalogs
            .relations
            .as_mut()
            .unwrap()
            .feed_conflict = true;
        state.recompute_relations();
        assert!(state.sector.relations.feed_conflict);
    }

    #[test]
    fn min_world_presence_filters_matrix() {
        let mut state = seed_state();
        // Both factions have zero world_presence -> fallback uses all factions
        // (pair emitted regardless of threshold).
        state.config.generation.relations.min_world_presence = 5;
        state.recompute_relations();
        assert_eq!(state.sector.relations.pairs.len(), 1);
    }

    #[test]
    fn canonical_pair_sorts_inputs() {
        let a = FactionId::from("zeta");
        let b = FactionId::from("alpha");
        let (lo, hi) = canonical_pair(&(a, b));
        assert_eq!(lo.as_ref(), "alpha");
        assert_eq!(hi.as_ref(), "zeta");
    }
}
