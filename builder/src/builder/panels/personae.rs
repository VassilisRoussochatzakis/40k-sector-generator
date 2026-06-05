//! PERSONAE tab (§N1 / §N2) — Phase D §PER1..§PER5.
//!
//! §PER1  Per-faction-kind persona-pool editor. Loads defaults from the
//!        built-in [`KindPools`]; per-project overrides live under
//!        `[inputs].personae` -> `data/personae.toml`. Each kind exposes
//!        editable name prefixes / roots / suffixes / single names / titles /
//!        traits.
//! §PER2  Per-anchor persona editor. Lists the derived personae — geographic
//!        anchors (system sovereign / orbital controller / economic hegemon /
//!        hidden master / per-world presences) plus org-leadership anchors
//!        (overall faction / sub-faction / force heads) — and lets users
//!        add/remove manual entries with name, title, traits, agenda.
//!        Manual entries survive regenerate because
//!        [`sectorforge::personae::derive_with`] appends `cfg.manual` last.
//! §PER6  Org-leadership toggle + presence gate (`faction_leaders`,
//!        `min_faction_presence`) live in the dominance section. They control
//!        whether/which faction-hierarchy nodes anchor a leader persona.
//! §PER3  "Regenerate" button calls [`BuilderState::recompute_personae`]
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

use std::collections::BTreeSet;

use egui::{Color32, RichText, Ui};

use sectorforge_gui_core::card;
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;
use sectorforge_gui_core::widgets;

use sectorforge::personae::{
    DominanceTier, KindPools, Persona, PersonaAnchor, PersonaeConfig, SystemSlot,
};

use crate::builder::state::{ConfirmAction, EntityRef, ModalKind};
use crate::builder::BuilderState;

const DEFAULT_PERSONAE_PATH: &str = "data/personae.toml";

/// Built-in faction kinds the editor lists by default. Users can add custom
/// kinds via the "Add kind" row — anything not in this list still derives
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

pub(crate) fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Personae");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "The named characters behind your factions — leaders, governors, and rivals derived from who holds what.",
    );
    ui.separator();

    // §COLUMNS — global controls (regenerate / player toggle) stay full-width
    // on top, then master-detail: the derived persona roster pins to a
    // resizable left rail and the selected-persona detail + dominance / manual /
    // pool editors + save fill the rest. Replaces the single-column stack whose
    // wide derived-persona grid forced a horizontal scroll.
    show_header_actions(ui, state);
    ui.separator();

    egui::SidePanel::left("personae_roster")
        .resizable(true)
        .default_width(300.0)
        .width_range(220.0..=520.0)
        .show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| show_persona_roster(ui, state));
        });

    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                show_persona_detail(ui, state);
                ui.separator();
                show_dominance_section(ui, state);
                ui.separator();
                show_manual_editor(ui, state);
                ui.separator();
                show_kind_pools_section(ui, state);
                ui.separator();
                show_save_row(ui, state);
            });
    });
}

/// Aligned label-left / control-right row with a hover tooltip. The visible
/// label reads in human terms while the tooltip names the underlying TOML
/// field plus a plain-language note, so power users keep the schema mapping.
fn labeled(ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let h = ui.spacing().interact_size.y;
        ui.add_sized(
            [140.0, h],
            egui::Label::new(RichText::new(label).color(palette::chrome_text_dim())),
        )
        .on_hover_text(help);
        add(ui);
    });
}

// ── §PER3 header actions ────────────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if widgets::primary_button(ui, "🔄  Regenerate")
            .on_hover_text(
                "Re-derive every persona from current faction presence and the rules below",
            )
            .clicked()
        {
            ensure_personae_catalog(state);
            state.recompute_personae();
        }
        ui.checkbox(&mut state.personae_auto_recompute, "Auto-update on edit")
            .on_hover_text("Re-derive automatically whenever you change a setting on this tab");
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
        ui.label(format!("{total} personae  (hand-written: {manual})"));
        if state.data_catalogs.personae.is_none() {
            ui.colored_label(
                palette::warning(),
                "using built-in defaults — Regenerate to start a personae file",
            );
        }
    });
}

// ── §PER4 dominance tier + per-anchor caps ──────────────────────────────────

fn show_dominance_section(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Who gets a persona").strong());
    ensure_personae_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.personae.as_mut() else {
        return;
    };
    let mut changed = false;

    labeled(
        ui,
        "Minimum hold",
        "Lowest dominance tier on a world that earns a named persona (schema: min_world_dominance). Higher tier means fewer worlds qualify.",
        |ui| {
            ui_kit::combo("per4_dom", tier_label(cfg.min_world_dominance)).show_ui(ui, |ui| {
                for tier in DOMINANCE_TIERS {
                    if ui
                        .selectable_value(&mut cfg.min_world_dominance, *tier, tier_label(*tier))
                        .on_hover_text(format!("key: {}", tier.as_slug()))
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        },
    );
    labeled(
        ui,
        "Max per world",
        "Most personae anchored to a single world (schema: max_per_world).",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_per_world).range(0..=64))
                .changed();
        },
    );
    labeled(
        ui,
        "Max per system",
        "Most personae anchored to a single system, across sovereign/orbital/economic/hidden slots (schema: max_per_system).",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_per_system).range(0..=64))
                .changed();
        },
    );
    labeled(
        ui,
        "Faction leaders",
        "Also create the overall / sub-faction / force heads, not just world & system personae (schema: faction_leaders).",
        |ui| {
            changed |= ui
                .checkbox(&mut cfg.faction_leaders, "Include leadership personae")
                .changed();
        },
    );
    labeled(
        ui,
        "Leader threshold",
        "How many systems-or-worlds a faction branch must hold before it earns a leader (schema: min_faction_presence).",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.min_faction_presence).range(0..=64))
                .changed();
        },
    );

    ui.colored_label(
        Color32::DARK_GRAY,
        "Raise the minimum hold for a leaner cast; the per-world and per-system caps keep crowded planets from spawning too many names.",
    );
    if changed {
        on_catalog_edited(state);
    }
}

// ── §PER2 + §PER5 persona roster (left rail) ─────────────────────────────────

/// §COLUMNS — left-rail roster of derived personae (master pane). A selectable
/// name line per persona with a faction/anchor subline; selecting a row sets
/// `selected_persona_id` (pure view state) and the detail fills the right pane.
fn show_persona_roster(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Cast").strong());
    let Some(report) = state.personae_report.clone() else {
        ui_kit::placeholder(
            ui,
            "No personae yet — click Regenerate above to derive them from your factions.",
        );
        return;
    };
    if report.personae.is_empty() {
        ui_kit::placeholder(
            ui,
            "No personae qualified — lower the minimum hold, or place faction presence in the sector first.",
        );
        return;
    }
    let selected = state.selected_persona_id.clone();
    for p in &report.personae {
        let is_selected = selected.as_deref() == Some(p.id.as_str());
        let name = if p.name.is_empty() {
            p.faction_id.to_string()
        } else {
            p.name.clone()
        };
        let (resp, _) = card::selectable_plate(ui, ("persona_row", &p.id), is_selected, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(name).strong());
                ui.label(
                    RichText::new(format!("{} · {}", p.faction_id, anchor_label(&p.anchor)))
                        .color(palette::chrome_text_dim())
                        .small(),
                );
            });
        });
        if resp.clicked() {
            state.selected_persona_id = Some(p.id.to_string());
        }
        ui.separator();
    }
}

// ── §PER2 + §PER5 persona detail (right pane) ───────────────────────────────

/// §COLUMNS — right detail pane for the persona selected in the roster. Mirrors
/// the old table columns (kind / anchor / name / title / traits / agenda) as a
/// key/value card plus the faction + anchor deep-links and the "edit manual"
/// jump that the table's per-row buttons provided.
fn show_persona_detail(ui: &mut Ui, state: &mut BuilderState) {
    let Some(report) = state.personae_report.clone() else {
        ui_kit::placeholder(
            ui,
            "Pick a persona from the cast on the left to see its details.",
        );
        return;
    };
    let Some(sel) = state.selected_persona_id.clone() else {
        ui_kit::placeholder(
            ui,
            "Pick a persona from the cast on the left to see its details.",
        );
        return;
    };
    let Some(p) = report.personae.iter().find(|p| p.id.as_str() == sel) else {
        ui_kit::placeholder(
            ui,
            "That persona is gone — click Regenerate to refresh the cast.",
        );
        return;
    };
    ui.label(RichText::new(p.name.clone()).strong().size(ui_kit::SECTION));
    egui::Grid::new("per_detail_grid")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            ui.label("Faction");
            if ui.link(p.faction_id.to_string()).clicked() {
                state.focus_entity(EntityRef::Faction(p.faction_id.clone()));
            }
            ui.end_row();
            ui.label("Type");
            if p.faction_kind.is_empty() {
                ui.colored_label(Color32::DARK_GRAY, "—");
            } else {
                ui.label(p.faction_kind.clone());
            }
            ui.end_row();
            ui.label("Anchor");
            show_anchor_link(ui, state, p);
            ui.end_row();
            ui.label("Title");
            ui.label(p.title.clone());
            ui.end_row();
            ui.label("Traits");
            if p.traits.is_empty() {
                ui.colored_label(Color32::DARK_GRAY, "—");
            } else {
                ui.label(p.traits.join(", "));
            }
            ui.end_row();
            ui.label("Agenda");
            ui.label(p.agenda.clone()).on_hover_text(format!(
                "Derived from\ntype: {}\nfaction: {}\nanchor: {}",
                if p.faction_kind.is_empty() {
                    "(unknown)"
                } else {
                    p.faction_kind.as_str()
                },
                p.faction_id,
                anchor_label(&p.anchor),
            ));
            ui.end_row();
        });
    if ui
        .button("✎  Copy to hand-written persona")
        .on_hover_text("Open this persona below as an editable manual entry")
        .clicked()
    {
        state.personae_edit_target = Some(p.id.to_string());
    }
}

fn show_anchor_link(ui: &mut Ui, state: &mut BuilderState, p: &Persona) {
    match &p.anchor {
        PersonaAnchor::System { system_id, slot }
            if ui
                .link(format!("{system_id} · {}", slot_label(*slot)))
                .clicked() =>
        {
            state.focus_entity(EntityRef::System(system_id.clone()));
        }
        PersonaAnchor::World {
            system_id,
            world_id,
        } if ui.link(format!("{system_id}/{world_id}")).clicked() => {
            state.focus_entity(EntityRef::World {
                system: system_id.clone(),
                world: world_id.clone(),
            });
        }
        PersonaAnchor::Faction {
            faction_id,
            subfaction_id,
            force_id,
            tier,
        } => {
            let label = match (subfaction_id, force_id) {
                (_, Some(force)) => format!("force · {force}"),
                (Some(sub), None) => format!("sub-faction · {sub}"),
                (None, None) => format!("{tier} · {faction_id}"),
            };
            if ui.link(label).clicked() {
                state.focus_entity(EntityRef::Faction(faction_id.clone()));
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

/// Friendly label for a dominance tier. The stored value stays the raw slug
/// (`min_world_dominance` round-trips unchanged); this only affects what the
/// reader sees in the dropdown.
fn tier_label(tier: DominanceTier) -> &'static str {
    match tier {
        DominanceTier::Presence => "Presence — a foothold",
        DominanceTier::Influence => "Influence — a hand on the scales",
        DominanceTier::Contested => "Contested — disputed ground",
        DominanceTier::Controlled => "Controlled — firmly held",
        DominanceTier::Stronghold => "Stronghold — a bastion",
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
        PersonaAnchor::Faction {
            faction_id,
            subfaction_id,
            force_id,
            tier,
        } => match (subfaction_id, force_id) {
            (_, Some(force)) => format!("force {force} ({faction_id})"),
            (Some(sub), None) => format!("sub-faction {sub} ({faction_id})"),
            (None, None) => format!("{faction_id} ({})", tier.as_slug()),
        },
        _ => "unknown".into(),
    }
}

// ── §PER2 manual entry editor ───────────────────────────────────────────────

fn show_manual_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Hand-written personae").strong());
    // PER-1: consume the one-shot "edit this persona" request the detail card's
    // copy button stored in `personae_edit_target`. Read it before the
    // `data_catalogs` borrow so the matching manual row can scroll into view.
    let edit_target = state.personae_edit_target.take();
    // Snapshot the existing faction ids before the mutable personae borrow so
    // the per-row faction picker can offer them as a dropdown (transform 6).
    let faction_ids = existing_faction_ids(state);
    ensure_personae_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.personae.as_mut() else {
        return;
    };
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("➕  Add persona")
            .on_hover_text("Add a blank persona you fill in by hand")
            .clicked()
        {
            cfg.manual.push(blank_manual_persona(cfg.manual.len()));
            changed = true;
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            "Hand-written entries are appended after auto-derivation and survive Regenerate.",
        );
    });
    if cfg.manual.is_empty() {
        ui_kit::placeholder(ui, "None yet — click Add persona to write one in.");
    } else {
        egui::Grid::new("per_manual_grid")
            .num_columns(7)
            .striped(true)
            .show(ui, |ui| {
                ui.label(RichText::new("ID").strong())
                    .on_hover_text("Unique identifier (schema: id)");
                ui.label(RichText::new("Faction").strong())
                    .on_hover_text("Which faction this person belongs to (schema: faction_id)");
                ui.label(RichText::new("Type").strong()).on_hover_text(
                    "Faction archetype used to flavour name & title (schema: faction_kind)",
                );
                ui.label(RichText::new("Name").strong())
                    .on_hover_text("Display name (schema: name)");
                ui.label(RichText::new("Title").strong())
                    .on_hover_text("Honorific or rank (schema: title)");
                ui.label(RichText::new("Traits").strong())
                    .on_hover_text("Comma-separated descriptors (schema: traits)");
                ui.label(RichText::new("Agenda").strong())
                    .on_hover_text("What they are trying to do (schema: agenda)");
                ui.end_row();

                for (idx, p) in cfg.manual.iter_mut().enumerate() {
                    let mut id_buf = p.id.to_string();
                    // PER-1: when this row is the one the user asked to edit,
                    // scroll it into view so the jump from the detail card lands
                    // on the right manual entry.
                    let is_edit_target = edit_target.as_deref() == Some(id_buf.as_str());
                    let id_resp = ui.text_edit_singleline(&mut id_buf);
                    if is_edit_target {
                        id_resp.scroll_to_me(Some(egui::Align::Center));
                    }
                    if id_resp.changed() {
                        p.id = id_buf.into();
                        changed = true;
                    }
                    if faction_id_combo(ui, idx, &mut p.faction_id, &faction_ids) {
                        changed = true;
                    }
                    changed |= ui.text_edit_singleline(&mut p.faction_kind).changed();
                    changed |= ui.text_edit_singleline(&mut p.name).changed();
                    changed |= ui.text_edit_singleline(&mut p.title).changed();
                    let mut traits_csv = p.traits.join(",");
                    if ui.text_edit_singleline(&mut traits_csv).changed() {
                        p.traits = traits_csv
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        changed = true;
                    }
                    changed |= ui.text_edit_singleline(&mut p.agenda).changed();
                    if ui
                        .button("🗑")
                        .on_hover_text("Delete this hand-written persona")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                    ui.end_row();
                }
            });
    }
    // §FRIENDLY_PANEL_PASS transform #7: a hand-written persona bypasses the undo
    // bus, so confirm the delete the row's 🗑 requested (label read while `cfg` is
    // still borrowed; dispatch happens after it is released).
    let pending_delete = remove_idx.and_then(|idx| {
        cfg.manual.get(idx).map(|p| {
            let label = if p.name.is_empty() {
                format!("persona #{}", idx + 1)
            } else {
                p.name.clone()
            };
            (idx, label)
        })
    });
    if changed {
        on_catalog_edited(state);
    }
    if let Some((idx, label)) = pending_delete {
        state.modal = Some(ModalKind::ConfirmDestructive {
            title: "Delete persona?".into(),
            body: format!("Remove the hand-written persona “{label}”."),
            action: ConfirmAction::DeleteManualPersona(idx),
        });
    }
}

/// §FRIENDLY_PANEL_PASS transform #7: delete the manual persona at `idx`
/// (confirmed payload of [`ModalKind::ConfirmDestructive`]) and run the
/// catalogue-edited bookkeeping. Manual personae bypass the undo bus.
pub(crate) fn delete_manual(state: &mut BuilderState, idx: usize) {
    {
        let Some(cfg) = state.data_catalogs.personae.as_mut() else {
            return;
        };
        if idx >= cfg.manual.len() {
            return;
        }
        cfg.manual.remove(idx);
    }
    on_catalog_edited(state);
}

/// Dropdown over the faction ids already in the project, plus an in-popup custom
/// row for ids that don't exist yet. Friendlier than free-typing a raw id —
/// most hand-written personae belong to a faction that already exists, so it
/// becomes one click. Returns `true` when the value changed.
fn faction_id_combo(
    ui: &mut Ui,
    idx: usize,
    slot: &mut sectorforge::ids::FactionId,
    options: &[String],
) -> bool {
    let mut changed = false;
    let current = slot.to_string();
    let label = if current.is_empty() {
        "(none)".to_owned()
    } else {
        current.clone()
    };
    ui_kit::combo(("per_manual_fac", idx), label).show_ui(ui, |ui| {
        for opt in options {
            if ui.selectable_label(current == *opt, opt.as_str()).clicked() {
                *slot = sectorforge::ids::FactionId::new(opt.as_str());
                changed = true;
            }
        }
        ui.separator();
        ui.label(RichText::new("custom…").small().color(Color32::DARK_GRAY));
        let mut buf = current.clone();
        if ui
            .add(
                egui::TextEdit::singleline(&mut buf)
                    .hint_text("faction id")
                    .desired_width(160.0),
            )
            .changed()
        {
            *slot = sectorforge::ids::FactionId::new(buf.trim());
            changed = true;
        }
    });
    changed
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
    ui.label(RichText::new("Name & title pools by faction type").strong());
    ensure_personae_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.personae.as_mut() else {
        return;
    };
    let mut changed = false;
    ui.colored_label(
        Color32::DARK_GRAY,
        "Empty fields fall back to the built-in defaults for that type.",
    );

    // Render one collapsing header per built-in kind plus any custom kinds
    // the user has authored.
    let mut kinds: Vec<String> = BUILTIN_KINDS.iter().map(|s| (*s).to_string()).collect();
    for key in cfg.kinds.keys() {
        if !kinds.contains(key) {
            kinds.push(key.clone());
        }
    }

    for kind in &kinds {
        ui_kit::collapsing_section(ui, ("per_kind", kind), kind, false, |ui| {
            let pools = cfg.kinds.entry(kind.clone()).or_default();
            changed |= pool_editor(ui, kind, pools);
            if ui
                .button(RichText::new("↺  Reset to defaults").color(palette::warning()))
                .on_hover_text(
                    "Clear this type's pools and fall back to the built-in names & titles",
                )
                .clicked()
            {
                cfg.kinds.remove(kind);
                changed = true;
            }
        });
    }

    ui.horizontal_wrapped(|ui| {
        ui.label("Add type:")
            .on_hover_text("Create pools for a faction type not listed above (schema: kinds key)");
        let mut new_kind = String::new();
        let resp =
            ui.add(egui::TextEdit::singleline(&mut new_kind).hint_text("type id, then Enter"));
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
            changed |= csv_row(
                ui,
                "Name prefixes",
                "Front halves of generated names (schema: name_prefixes)",
                &mut pools.name_prefixes,
            );
            changed |= csv_row(
                ui,
                "Name roots",
                "Core stems of generated names (schema: name_roots)",
                &mut pools.name_roots,
            );
            changed |= csv_row(
                ui,
                "Name suffixes",
                "Tail halves of generated names (schema: name_suffixes)",
                &mut pools.name_suffixes,
            );
            changed |= csv_row(
                ui,
                "Whole names",
                "Complete names used as-is when present (schema: single_names)",
                &mut pools.single_names,
            );
            changed |= csv_row(
                ui,
                "Titles",
                "Honorifics & ranks drawn for this type (schema: titles)",
                &mut pools.titles,
            );
            changed |= csv_row(
                ui,
                "Traits",
                "Descriptors drawn for this type (schema: traits)",
                &mut pools.traits,
            );
        });
    changed
}

fn csv_row(ui: &mut Ui, label: &str, help: &str, values: &mut Vec<String>) -> bool {
    ui.label(label).on_hover_text(help);
    let mut csv = values.join(",");
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
            .add_enabled(has_catalog, egui::Button::new("💾  Save personae"))
            .on_hover_text("Write the personae file to disk")
            .clicked()
        {
            if state.config.inputs.personae.is_none() {
                state.config.inputs.personae = Some(DEFAULT_PERSONAE_PATH.into());
            }
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Could not save the personae file: {e}"
                )));
            }
        }
        let path_label =
            state.config.inputs.personae.clone().unwrap_or_else(|| {
                format!("(not set yet — will write to {DEFAULT_PERSONAE_PATH})")
            });
        ui.colored_label(Color32::DARK_GRAY, path_label);
    });
}

// ── shared helpers ──────────────────────────────────────────────────────────

/// Unique, sorted faction ids currently defined in the project's roster. Feeds
/// the per-row faction picker in the hand-written persona editor.
fn existing_faction_ids(state: &BuilderState) -> Vec<String> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    if let Some(file) = state.data_catalogs.factions.as_ref() {
        for f in &file.factions {
            ids.insert(f.id.to_string());
        }
    }
    ids.into_iter().collect()
}

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
