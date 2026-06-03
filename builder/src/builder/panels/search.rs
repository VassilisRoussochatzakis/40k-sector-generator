//! SEARCH tab (§N1 / §N2) — constraint-directed seed search.
//!
//! Implements `docs/BUILDER_REQS.txt §SR1..§SR5`:
//!   * §SR1 a form widget per constraint kind in `sectorforge::search`.
//!   * §SR2 an off-thread run with a live progress bar (tried / passed /
//!     best-miss), backed by [`crate::builder::search_run::SearchState`].
//!   * §SR3 the outcome panel: winning seed (Apply) + top-N near misses
//!     (View + Apply).
//!   * §SR4 `base_seed` / `budget` / `report_top` editors.
//!   * §SR5 a live faction-id existence preflight, shown before the run (the
//!     library re-checks it inside `run_search`).

use std::collections::BTreeSet;

use egui::{Color32, RichText};

use sectorforge::search::{Constraint, WishesFile};
use sectorforge::worlds::WorldType;
use sectorforge_gui_core::ui_kit;

use crate::builder::search_run::NewConstraintKind;
use crate::builder::state::BuilderTab;
use crate::builder::BuilderState;

const RESOURCES: &[&str] = &[
    "ore",
    "promethium",
    "foodstuffs",
    "manufactured",
    "archeotech",
    "recruits",
];

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    // §SR2: drain any completed search worker first so the outcome renders
    // the same frame it lands.
    if state.search.pump() {
        ui.ctx().request_repaint();
    }

    ui.heading("Search (§SR1..§SR5)");
    ui.label(
        RichText::new("Declare what the sector should look like, then enumerate seeds to find one that matches.")
            .weak(),
    );

    if state.data_catalogs.worlds.is_none() {
        ui.separator();
        ui.colored_label(
            Color32::from_rgb(235, 180, 50),
            "Search needs a project with a worlds catalogue. Open or create a project first.",
        );
        return;
    }

    // Roster used for faction combos + the §SR5 preflight. The library accepts
    // either a faction id or a kind, so the known set carries both.
    let factions: Vec<String> = state
        .data_catalogs
        .factions
        .as_ref()
        .map(|f| {
            f.factions
                .iter()
                .map(|d| d.id.as_str().to_string())
                .collect()
        })
        .unwrap_or_default();
    let known: BTreeSet<String> = state
        .data_catalogs
        .factions
        .as_ref()
        .map(|f| {
            f.factions
                .iter()
                .flat_map(|d| [d.id.as_str().to_string(), d.kind.clone()])
                .collect()
        })
        .unwrap_or_default();
    let project_seed = state.config.generation.seed.clone();

    ui.separator();

    if state.search.wishes.is_none() {
        ui.add_space(6.0);
        if ui.button("+ Create wishes.toml").clicked() {
            state.search.wishes = Some(WishesFile {
                search: Default::default(),
                constraints: Vec::new(),
            });
        }
        ui.label(RichText::new("No wishes document yet.").weak());
        return;
    }

    // ── §SR4 search config + §SR1 constraint editor ────────────────────────
    let mut preflight_unknown: Vec<String> = Vec::new();
    let budget_hint;

    {
        let wishes = state.search.wishes.as_mut().unwrap();
        budget_hint = wishes.search.budget.max(1);

        // §SR4: base_seed / budget / report_top.
        ui_kit::collapsing_section(
            ui,
            "sr_sr4_search_config",
            "§SR4 — Search config",
            true,
            |ui| {
                egui::Grid::new("sr4-search-config")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("base_seed");
                        ui.horizontal(|ui| {
                            let mut use_project = wishes.search.base_seed.is_none();
                            if ui.checkbox(&mut use_project, "use project seed").changed() {
                                wishes.search.base_seed = if use_project {
                                    None
                                } else {
                                    Some(project_seed.clone())
                                };
                            }
                            if let Some(seed) = wishes.search.base_seed.as_mut() {
                                ui.text_edit_singleline(seed);
                            } else {
                                ui.label(
                                    RichText::new(format!("(project: {project_seed})")).weak(),
                                );
                            }
                        });
                        ui.end_row();

                        ui.label("budget");
                        ui.add(
                            egui::DragValue::new(&mut wishes.search.budget)
                                .range(1..=100_000)
                                .speed(1.0),
                        );
                        ui.end_row();

                        ui.label("report_top");
                        ui.add(
                            egui::DragValue::new(&mut wishes.search.report_top)
                                .range(1..=50)
                                .speed(1.0),
                        );
                        ui.end_row();
                    });
            },
        );

        // §SR1: per-constraint form widgets.
        ui_kit::collapsing_section(
            ui,
            "sr_sr1_constraints",
            &format!("§SR1 — Constraints ({})", wishes.constraints.len()),
            true,
            |ui| {
                let mut remove_idx: Option<usize> = None;
                for (i, c) in wishes.constraints.iter_mut().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(constraint_kind_label(c)).strong().monospace());
                            if ui.small_button("× remove").clicked() {
                                remove_idx = Some(i);
                            }
                        });
                        constraint_editor(ui, i, c, &factions);
                    });
                }
                if let Some(i) = remove_idx {
                    wishes.constraints.remove(i);
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui_kit::combo("sr1-add-kind", state.search.new_constraint_kind.label())
                        .show_ui(ui, |ui| {
                            for kind in NewConstraintKind::ALL {
                                if ui
                                    .selectable_label(
                                        state.search.new_constraint_kind == *kind,
                                        kind.label(),
                                    )
                                    .clicked()
                                {
                                    state.search.new_constraint_kind = *kind;
                                }
                            }
                        });
                    if ui.button("+ Add constraint").clicked() {
                        let default_faction = factions.first().cloned().unwrap_or_default();
                        wishes
                            .constraints
                            .push(state.search.new_constraint_kind.make(&default_faction));
                    }
                });
            },
        );

        // §SR5: live faction-id preflight against the roster.
        for c in &wishes.constraints {
            if let Some(id) = referenced_faction(c) {
                if !id.is_empty() && !known.contains(&id) {
                    preflight_unknown.push(id);
                }
            }
        }
    }

    // ── Run / cancel (§SR2) ────────────────────────────────────────────────
    ui.separator();
    if !preflight_unknown.is_empty() {
        ui.colored_label(
            Color32::from_rgb(220, 80, 80),
            format!(
                "§SR5 preflight: unknown faction id(s): {}",
                preflight_unknown.join(", ")
            ),
        );
        ui.label(
            RichText::new("Fix or remove these before running — the search will abort otherwise.")
                .weak(),
        );
    }

    let mut run_clicked = false;
    let mut cancel_clicked = false;
    ui.horizontal(|ui| {
        let can_run = !state.search.is_running() && preflight_unknown.is_empty();
        if ui
            .add_enabled(can_run, egui::Button::new("▶ Run search"))
            .clicked()
        {
            run_clicked = true;
        }
        if state.search.is_running() && ui.button("■ Cancel").clicked() {
            cancel_clicked = true;
        }
    });

    // §SR2: progress while a worker is in flight.
    if state.search.is_running() {
        let snap = state.search.progress_snapshot();
        let (tried, passed, best, budget) = match snap {
            Some(p) => (p.tried, p.passed, p.best_miss, p.budget),
            None => (0, 0, None, budget_hint),
        };
        let frac = if budget == 0 {
            0.0
        } else {
            tried as f32 / budget as f32
        };
        ui.add(egui::ProgressBar::new(frac).show_percentage());
        let best_txt = best
            .map(|m| format!("{m:.3}"))
            .unwrap_or_else(|| "—".to_string());
        ui.label(
            RichText::new(format!(
                "tried {tried}/{budget} · passed {passed} · best near-miss {best_txt}"
            ))
            .monospace(),
        );
    }

    if cancel_clicked {
        state.search.cancel();
    }

    if run_clicked {
        // Snapshot the wishes + synthesize the project input, then dispatch the
        // off-thread search. Both reads finish before the mutable spawn borrow.
        if let (Some(wishes), Some(input)) = (
            state.search.wishes.clone(),
            state.synthesize_project_input(),
        ) {
            let ctx = ui.ctx().clone();
            state.search.spawn(&ctx, input, wishes);
        }
    }

    // §SR3: outcome panel.
    if let Some(err) = state.search.error.clone() {
        ui.separator();
        ui.colored_label(Color32::from_rgb(220, 80, 80), err);
    }

    show_outcome(ui, state);
}

// ── §SR3 outcome (§COLUMNS master-detail) ────────────────────────────────────

/// §COLUMNS — renders the §SR3 outcome as master-detail: the candidate roster
/// (winner + near misses) pins to a left rail, the selected hit's full
/// constraint breakdown + Apply/View controls fill the central pane. Which hit
/// is shown is pure view state, so it lives in an `ui.data` temp keyed to the
/// outcome's `base_seed` (no `SearchState` field exists for it). Apply/View are
/// likewise threaded out through a temp and run after the split closes so the
/// model edit isn't held across the rail/detail `&mut state` borrows.
fn show_outcome(ui: &mut egui::Ui, state: &mut BuilderState) {
    let Some(outcome) = state.search.outcome.clone() else {
        return;
    };
    ui.separator();
    ui.heading("§SR3 — Outcome");
    ui.label(
        RichText::new(format!(
            "base `{}` · budget {} · evaluated {}",
            outcome.base_seed, outcome.budget, outcome.candidates_evaluated
        ))
        .monospace(),
    );
    for e in &outcome.preflight_errors {
        ui.colored_label(Color32::from_rgb(220, 80, 80), e);
    }

    // Default the selection to the winner, else the first near miss. The temp
    // is re-seeded whenever the stored selection no longer matches a row in the
    // current outcome (e.g. a fresh search produced a different candidate set).
    let sel_id = egui::Id::new(("sr_selected_seed", outcome.base_seed.as_str()));
    let valid = |seed: &str| -> bool {
        outcome.winning.as_ref().is_some_and(|w| w.seed == seed)
            || outcome.near_misses.iter().any(|c| c.seed == seed)
    };
    let default_seed = outcome
        .winning
        .as_ref()
        .map(|w| w.seed.clone())
        .or_else(|| outcome.near_misses.first().map(|c| c.seed.clone()));
    let mut selected: Option<String> = ui.data_mut(|d| d.get_temp::<String>(sel_id));
    if selected.as_deref().map(&valid) != Some(true) {
        selected = default_seed.clone();
    }

    if outcome.winning.is_none() && outcome.near_misses.is_empty() {
        if outcome.preflight_errors.is_empty() {
            ui.add_space(6.0);
            ui.colored_label(
                Color32::from_rgb(235, 180, 50),
                "No seed satisfied every constraint within the budget.",
            );
        }
        return;
    }

    // Left rail: candidate roster. Right pane: selected-hit detail. The list
    // closure runs fully (setting the selection temp) before the detail closure,
    // so each may borrow `&mut state`-free `ui.data` in turn.
    egui::SidePanel::left("search_results")
        .resizable(true)
        .default_width(260.0)
        .width_range(180.0..=460.0)
        .show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    if let Some(win) = &outcome.winning {
                        let sel = selected.as_deref() == Some(win.seed.as_str());
                        if ui
                            .selectable_label(
                                sel,
                                RichText::new(format!("★ WINNER #{} · {}", win.n, win.seed))
                                    .monospace()
                                    .color(Color32::from_rgb(90, 200, 120)),
                            )
                            .clicked()
                        {
                            selected = Some(win.seed.clone());
                        }
                    }
                    if !outcome.near_misses.is_empty() {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!("Near misses ({})", outcome.near_misses.len()))
                                .strong(),
                        );
                        for cand in &outcome.near_misses {
                            let sel = selected.as_deref() == Some(cand.seed.as_str());
                            if ui
                                .selectable_label(
                                    sel,
                                    RichText::new(format!(
                                        "#{} · {} · miss {:.3}",
                                        cand.n, cand.seed, cand.total_miss
                                    ))
                                    .monospace(),
                                )
                                .clicked()
                            {
                                selected = Some(cand.seed.clone());
                            }
                        }
                    }
                });
            if let Some(seed) = &selected {
                ui.data_mut(|d| d.insert_temp(sel_id, seed.clone()));
            }
        });

    let mut apply_seed: Option<String> = None;
    let mut view_seed: Option<String> = None;
    egui::CentralPanel::default().show_inside(ui, |ui| {
        let Some(seed) = selected.clone() else {
            ui_kit::placeholder(ui, "Select a candidate from the roster on the left.");
            return;
        };
        let Some(cand) = outcome
            .winning
            .iter()
            .chain(outcome.near_misses.iter())
            .find(|c| c.seed == seed)
        else {
            ui_kit::placeholder(ui, "Candidate no longer in the current outcome.");
            return;
        };
        let is_winner = outcome.winning.as_ref().is_some_and(|w| w.seed == seed);
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                if is_winner {
                    ui.colored_label(
                        Color32::from_rgb(90, 200, 120),
                        format!("WINNER — candidate #{} · seed `{}`", cand.n, cand.seed),
                    );
                } else {
                    ui.colored_label(
                        Color32::from_rgb(235, 180, 50),
                        format!(
                            "Near miss — candidate #{} · seed `{}` · total miss {:.3}",
                            cand.n, cand.seed, cand.total_miss
                        ),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply seed").clicked() {
                        apply_seed = Some(cand.seed.clone());
                    }
                    if ui.button("View on map").clicked() {
                        view_seed = Some(cand.seed.clone());
                    }
                });
                ui.separator();
                for c in &cand.constraints {
                    let (mark, col) = if c.passed {
                        ("✓", Color32::from_rgb(120, 200, 130))
                    } else {
                        ("✗", Color32::from_rgb(200, 120, 120))
                    };
                    ui.label(
                        RichText::new(format!(
                            "{mark} {} · obs {} · req {}",
                            c.label, c.observed, c.required
                        ))
                        .monospace()
                        .color(col),
                    );
                }
            });
    });

    // Apply / view regenerate the working sector from the chosen seed. Apply
    // commits (dirty + auto-save) and jumps to MAP; View is a non-destructive
    // look that leaves the project clean.
    if let Some(seed) = apply_seed {
        match state.apply_search_seed(&seed, true) {
            Ok(()) => state.active_tab = BuilderTab::Map,
            Err(e) => state.search.error = Some(e.to_string()),
        }
    } else if let Some(seed) = view_seed {
        match state.apply_search_seed(&seed, false) {
            Ok(()) => state.active_tab = BuilderTab::Map,
            Err(e) => state.search.error = Some(e.to_string()),
        }
    }
}

// ── Per-kind constraint widgets (§SR1) ───────────────────────────────────────

fn constraint_editor(ui: &mut egui::Ui, idx: usize, c: &mut Constraint, factions: &[String]) {
    egui::Grid::new(("sr1-constraint", idx))
        .num_columns(2)
        .spacing([10.0, 4.0])
        .show(ui, |ui| match c {
            Constraint::FactionShareMin { faction_id, min } => {
                row_faction(ui, idx, faction_id, factions);
                row_frac(ui, "min share", min);
            }
            Constraint::FactionShareMax { faction_id, max } => {
                row_faction(ui, idx, faction_id, factions);
                row_frac(ui, "max share", max);
            }
            Constraint::FactionWorldCountMin { faction_id, min } => {
                row_faction(ui, idx, faction_id, factions);
                row_u32(ui, "min worlds", min);
            }
            Constraint::FactionWorldCountMax { faction_id, max } => {
                row_faction(ui, idx, faction_id, factions);
                row_u32(ui, "max worlds", max);
            }
            Constraint::FactionSystemCountMin { faction_id, min } => {
                row_faction(ui, idx, faction_id, factions);
                row_u32(ui, "min systems", min);
            }
            Constraint::FactionSystemCountMax { faction_id, max } => {
                row_faction(ui, idx, faction_id, factions);
                row_u32(ui, "max systems", max);
            }
            Constraint::WorldTypeExists {
                world_type,
                dominant_faction_id,
                min_count,
            } => {
                ui.label("world_type");
                world_type_combo(ui, idx, world_type);
                ui.end_row();
                ui.label("dominant faction");
                opt_faction_combo(ui, idx, dominant_faction_id, factions);
                ui.end_row();
                row_u32(ui, "min count", min_count);
            }
            Constraint::ContestedWorldMin { min, n_way } => {
                row_u32(ui, "min contested", min);
                ui.label("n-way");
                ui.horizontal(|ui| {
                    let mut on = n_way.is_some();
                    if ui.checkbox(&mut on, "require").changed() {
                        *n_way = if on { Some(2) } else { None };
                    }
                    if let Some(k) = n_way.as_mut() {
                        ui.add(egui::DragValue::new(k).range(2..=20));
                    }
                });
                ui.end_row();
            }
            Constraint::ContestedWorldMax { max } => {
                row_u32(ui, "max contested", max);
            }
            Constraint::SystemStateCountMin { state, min } => {
                ui.label("state");
                system_state_combo(ui, idx, state);
                ui.end_row();
                row_u32(ui, "min systems", min);
            }
            Constraint::SystemStateCountMax { state, max } => {
                ui.label("state");
                system_state_combo(ui, idx, state);
                ui.end_row();
                row_u32(ui, "max systems", max);
            }
            Constraint::RouteGraphConnected => {
                ui.label(RichText::new("route graph is one component").weak());
                ui.end_row();
            }
            Constraint::NoArticulationPoints => {
                ui.label(RichText::new("no single point of disconnection").weak());
                ui.end_row();
            }
            Constraint::DiameterMax { max_hops } => {
                row_u32(ui, "max hops", max_hops);
            }
            Constraint::IsolatedSystemsMax { max } => {
                row_u32(ui, "max isolated", max);
            }
            Constraint::ContestedRatioMin { min } => {
                row_frac(ui, "min ratio", min);
            }
            Constraint::ContestedRatioMax { max } => {
                row_frac(ui, "max ratio", max);
            }
            Constraint::StanceCountMin { stance, min } => {
                ui.label("stance");
                stance_combo(ui, idx, stance);
                ui.end_row();
                row_u32(ui, "min pairs", min);
            }
            Constraint::StanceCountMax { stance, max } => {
                ui.label("stance");
                stance_combo(ui, idx, stance);
                ui.end_row();
                row_u32(ui, "max pairs", max);
            }
            Constraint::RegionCountMin { region_kind, min } => {
                ui.label("region kind");
                region_kind_combo(ui, idx, region_kind);
                ui.end_row();
                row_u32(ui, "min regions", min);
            }
            Constraint::RegionCountMax { region_kind, max } => {
                ui.label("region kind");
                region_kind_combo(ui, idx, region_kind);
                ui.end_row();
                row_u32(ui, "max regions", max);
            }
            Constraint::EconomyStrandedMax { max } => {
                row_u32(ui, "max stranded", max);
            }
            Constraint::EconomyResourceMin { resource, min } => {
                ui.label("resource");
                resource_combo(ui, idx, resource);
                ui.end_row();
                ui.label("min balance");
                ui.add(egui::DragValue::new(min).speed(1.0));
                ui.end_row();
            }
            // §SR1 ships the constraint kinds listed in the spec; any future
            // `#[non_exhaustive]` variant falls through to a read-only note.
            _ => {
                ui.label(RichText::new("(constraint not editable in this build)").weak());
                ui.end_row();
            }
        });
}

fn row_faction(ui: &mut egui::Ui, idx: usize, faction_id: &mut String, factions: &[String]) {
    ui.label("faction");
    faction_combo(ui, idx, faction_id, factions);
    ui.end_row();
}

fn row_u32(ui: &mut egui::Ui, label: &str, v: &mut u32) {
    ui.label(label);
    ui.add(egui::DragValue::new(v).range(0..=100_000).speed(1.0));
    ui.end_row();
}

fn row_frac(ui: &mut egui::Ui, label: &str, v: &mut f32) {
    ui.label(label);
    ui.add(egui::Slider::new(v, 0.0..=1.0).clamping(egui::SliderClamping::Always));
    ui.end_row();
}

fn faction_combo(ui: &mut egui::Ui, idx: usize, current: &mut String, factions: &[String]) {
    let selected = if current.is_empty() {
        "(pick faction)".to_string()
    } else {
        current.clone()
    };
    ui_kit::combo(("sr1-faction", idx), selected).show_ui(ui, |ui| {
        for f in factions {
            if ui.selectable_label(current == f, f).clicked() {
                *current = f.clone();
            }
        }
    });
}

fn opt_faction_combo(
    ui: &mut egui::Ui,
    idx: usize,
    current: &mut Option<String>,
    factions: &[String],
) {
    let selected = current.clone().unwrap_or_else(|| "(any)".to_string());
    ui_kit::combo(("sr1-opt-faction", idx), selected).show_ui(ui, |ui| {
        if ui.selectable_label(current.is_none(), "(any)").clicked() {
            *current = None;
        }
        for f in factions {
            if ui
                .selectable_label(current.as_deref() == Some(f), f)
                .clicked()
            {
                *current = Some(f.clone());
            }
        }
    });
}

fn world_type_combo(ui: &mut egui::Ui, idx: usize, current: &mut String) {
    ui_kit::combo(("sr1-worldtype", idx), current.clone()).show_ui(ui, |ui| {
        for v in WorldType::VARIANTS {
            let value = v.to_string();
            if ui
                .selectable_label(*current == value, v.display_name())
                .clicked()
            {
                *current = value;
            }
        }
    });
}

fn resource_combo(ui: &mut egui::Ui, idx: usize, current: &mut String) {
    ui_kit::combo(("sr1-resource", idx), current.clone()).show_ui(ui, |ui| {
        for r in RESOURCES {
            if ui.selectable_label(current == r, *r).clicked() {
                *current = (*r).to_string();
            }
        }
    });
}

fn system_state_combo(
    ui: &mut egui::Ui,
    idx: usize,
    current: &mut sectorforge::search::SystemStateName,
) {
    use sectorforge::search::SystemStateName as S;
    const ALL: &[S] = &[
        S::Pacified,
        S::Fragmented,
        S::Blockaded,
        S::Warzone,
        S::Infiltrated,
        S::Quarantined,
        S::Uncharted,
    ];
    ui_kit::combo(("sr1-sysstate", idx), current.as_slug()).show_ui(ui, |ui| {
        for v in ALL {
            if ui.selectable_label(*current == *v, v.as_slug()).clicked() {
                *current = *v;
            }
        }
    });
}

fn stance_combo(ui: &mut egui::Ui, idx: usize, current: &mut sectorforge::search::StanceName) {
    use sectorforge::search::StanceName as S;
    const ALL: &[S] = &[
        S::Allied,
        S::Aligned,
        S::Neutral,
        S::Rival,
        S::Hostile,
        S::AtWar,
    ];
    ui_kit::combo(("sr1-stance", idx), current.as_slug()).show_ui(ui, |ui| {
        for v in ALL {
            if ui.selectable_label(*current == *v, v.as_slug()).clicked() {
                *current = *v;
            }
        }
    });
}

fn region_kind_combo(
    ui: &mut egui::Ui,
    idx: usize,
    current: &mut sectorforge::search::RegionKindName,
) {
    use sectorforge::search::RegionKindName as R;
    const ALL: &[R] = &[
        R::WarpStorm,
        R::Turbulence,
        R::CalmCorridor,
        R::Blackout,
        R::Anomaly,
    ];
    ui_kit::combo(("sr1-region", idx), current.as_slug()).show_ui(ui, |ui| {
        for v in ALL {
            if ui.selectable_label(*current == *v, v.as_slug()).clicked() {
                *current = *v;
            }
        }
    });
}

// ── Labels + preflight mirror ────────────────────────────────────────────────

fn constraint_kind_label(c: &Constraint) -> &'static str {
    match c {
        Constraint::FactionShareMin { .. } => "faction_share_min",
        Constraint::FactionShareMax { .. } => "faction_share_max",
        Constraint::FactionWorldCountMin { .. } => "faction_world_count_min",
        Constraint::FactionWorldCountMax { .. } => "faction_world_count_max",
        Constraint::FactionSystemCountMin { .. } => "faction_system_count_min",
        Constraint::FactionSystemCountMax { .. } => "faction_system_count_max",
        Constraint::WorldTypeExists { .. } => "world_type_exists",
        Constraint::ContestedWorldMin { .. } => "contested_world_min",
        Constraint::ContestedWorldMax { .. } => "contested_world_max",
        Constraint::SystemStateCountMin { .. } => "system_state_count_min",
        Constraint::SystemStateCountMax { .. } => "system_state_count_max",
        Constraint::RouteGraphConnected => "route_graph_connected",
        Constraint::NoArticulationPoints => "no_articulation_points",
        Constraint::DiameterMax { .. } => "diameter_max",
        Constraint::IsolatedSystemsMax { .. } => "isolated_systems_max",
        Constraint::ContestedRatioMin { .. } => "contested_ratio_min",
        Constraint::ContestedRatioMax { .. } => "contested_ratio_max",
        Constraint::StanceCountMin { .. } => "stance_count_min",
        Constraint::StanceCountMax { .. } => "stance_count_max",
        Constraint::RegionCountMin { .. } => "region_count_min",
        Constraint::RegionCountMax { .. } => "region_count_max",
        Constraint::EconomyStrandedMax { .. } => "economy_stranded_max",
        Constraint::EconomyResourceMin { .. } => "economy_resource_min",
        _ => "(constraint)",
    }
}

/// §SR5 mirror of the library's private `referenced_faction`: the id a
/// constraint depends on, if any.
fn referenced_faction(c: &Constraint) -> Option<String> {
    match c {
        Constraint::FactionShareMin { faction_id, .. }
        | Constraint::FactionShareMax { faction_id, .. }
        | Constraint::FactionWorldCountMin { faction_id, .. }
        | Constraint::FactionWorldCountMax { faction_id, .. }
        | Constraint::FactionSystemCountMin { faction_id, .. }
        | Constraint::FactionSystemCountMax { faction_id, .. } => Some(faction_id.clone()),
        Constraint::WorldTypeExists {
            dominant_faction_id: Some(id),
            ..
        } => Some(id.clone()),
        _ => None,
    }
}
