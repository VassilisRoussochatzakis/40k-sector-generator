//! Top-level tab router (§N1 / §N2).
//!
//! [`show_nav_rail`] renders the left cluster nav rail — every [`BuilderTab`]
//! grouped into the labeled clusters in [`TAB_CLUSTERS`] (§UO6 P2 / §COLUMNS
//! §6.1); selecting one writes to [`BuilderState::active_tab`]. [`show_top_bar`]
//! is the slim top bar (rail toggle + back/forward chevrons + a `CLUSTER / Tab`
//! breadcrumb). [`show_active_panel`] dispatches the active tab to the matching
//! panel module under this directory — the contract every tab follows per §N2.
//!
//! Phase A wires PROJECT (project tree + open/save/new + preferences), MAP
//! (placeholder with the §N3 toolbox), and the validation / invariants
//! sub-surfaces that already exist. The remaining tabs render stub panels
//! that announce the Phase that will fill them in. The router is intentionally
//! independent from the running viewer [`crate::App`] — a host shell adopts
//! [`BuilderState`] as its root state and calls [`show_top_bar`] +
//! [`show_active_panel`] each frame.

use crate::builder::state::BuilderTab;
use crate::builder::BuilderState;

use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;

use super::{
    analytics, briefing, control, diff, economy, export, factions, history, hooks, interestingness,
    invariants as invariants_panel, map, missions, personae, project, prose, regions, relations,
    routes, search, segmentum, sites, subsectors, system, validation, world,
};

/// §UO6 P2: the top tab strip grouped into labeled clusters. Every
/// [`BuilderTab`] appears in exactly one cluster; the
/// `clusters_cover_every_tab_exactly_once` test cross-checks this against
/// [`BuilderTab::ALL`], so adding a tab to the enum without slotting it here is
/// a compile-green test failure rather than a silently-missing tab.
const TAB_CLUSTERS: &[(&str, &[BuilderTab])] = &[
    (
        "BUILD",
        &[
            BuilderTab::Project,
            BuilderTab::Map,
            BuilderTab::Subsectors,
            BuilderTab::Regions,
            BuilderTab::Routes,
        ],
    ),
    (
        "ENTITIES",
        &[
            BuilderTab::System,
            BuilderTab::World,
            BuilderTab::Factions,
            BuilderTab::Sites,
        ],
    ),
    (
        "POWER",
        &[
            BuilderTab::Control,
            BuilderTab::Economy,
            BuilderTab::Relations,
        ],
    ),
    (
        "LORE",
        &[
            BuilderTab::History,
            BuilderTab::Personae,
            BuilderTab::Hooks,
            BuilderTab::Missions,
            BuilderTab::Prose,
            BuilderTab::Briefing,
        ],
    ),
    (
        "ANALYZE",
        &[
            BuilderTab::Analytics,
            BuilderTab::Interestingness,
            BuilderTab::Search,
            BuilderTab::Diff,
        ],
    ),
    ("OUTPUT", &[BuilderTab::Segmentum, BuilderTab::Export]),
    ("CHECK", &[BuilderTab::Validation, BuilderTab::Invariants]),
];

/// §COLUMNS §6.1 — slim top bar: the nav-rail toggle (`☰`), the §LINK3
/// back/forward chevrons, and a dim `CLUSTER / Tab` breadcrumb of where you
/// are. The cluster tab list itself moved to [`show_nav_rail`], so the 26-tab
/// strip no longer wraps to two or three rows across the top.
pub fn show_top_bar(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        let hover = if state.nav_rail_collapsed {
            "Show the nav rail"
        } else {
            "Hide the nav rail"
        };
        if ui.button("☰").on_hover_text(hover).clicked() {
            state.nav_rail_collapsed = !state.nav_rail_collapsed;
        }
        ui.separator();
        let can_back = !state.nav_back_stack.is_empty();
        let can_forward = !state.nav_forward_stack.is_empty();
        if ui
            .add_enabled(can_back, egui::Button::new("‹").small())
            .on_hover_text("Back (Alt+←)")
            .clicked()
        {
            state.nav_back();
        }
        if ui
            .add_enabled(can_forward, egui::Button::new("›").small())
            .on_hover_text("Forward (Alt+→)")
            .clicked()
        {
            state.nav_forward();
        }
        ui.separator();
        // Breadcrumb: which cluster + tab is active (the rail carries the list).
        if let Some(cluster) = cluster_of(state.active_tab) {
            ui.label(
                egui::RichText::new(format!("{cluster} /"))
                    .small()
                    .color(palette::chrome_text_dim()),
            );
        }
        ui.label(egui::RichText::new(state.active_tab.label()).strong());
    });
}

/// §COLUMNS §6.1 — the left cluster nav rail. Lists every [`BuilderTab`] under
/// its [`TAB_CLUSTERS`] group as a collapsible section (default open); the
/// active tab is highlighted and clicking one sets [`BuilderState::active_tab`].
/// Replaces the wrapping horizontal strip and reclaims the 2–3 rows it ate.
/// Hidden while [`BuilderState::nav_rail_collapsed`]; the `☰` button in
/// [`show_top_bar`] brings it back.
pub fn show_nav_rail(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("NAV")
                .small()
                .color(palette::chrome_text_dim()),
        );
        if ui
            .small_button("‹‹")
            .on_hover_text("Collapse the nav rail (☰ in the top bar reopens it)")
            .clicked()
        {
            state.nav_rail_collapsed = true;
        }
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for (label, tabs) in TAB_CLUSTERS {
                ui_kit::collapsing_section(ui, ("nav_cluster", *label), label, true, |ui| {
                    for tab in *tabs {
                        let selected = state.active_tab == *tab;
                        if ui.selectable_label(selected, tab.label()).clicked() {
                            state.set_active_tab(*tab);
                        }
                    }
                });
            }
        });
}

/// The [`TAB_CLUSTERS`] label that owns `tab`, for the top-bar breadcrumb.
fn cluster_of(tab: BuilderTab) -> Option<&'static str> {
    for (label, tabs) in TAB_CLUSTERS {
        if tabs.contains(&tab) {
            return Some(*label);
        }
    }
    None
}

/// Dispatch the active tab to its panel module (§N2).
pub fn show_active_panel(ui: &mut egui::Ui, state: &mut BuilderState) {
    match state.active_tab {
        BuilderTab::Project => project::show(ui, state),
        BuilderTab::Map => map::show(ui, state),
        BuilderTab::System => system::show(ui, state),
        BuilderTab::World => world::show(ui, state),
        BuilderTab::Factions => factions::show(ui, state),
        BuilderTab::Control => control::show(ui, state),
        BuilderTab::Regions => regions::show(ui, state),
        BuilderTab::Routes => routes::show(ui, state),
        BuilderTab::Subsectors => subsectors::show(ui, state),
        BuilderTab::Economy => economy::show(ui, state),
        BuilderTab::Relations => relations::show(ui, state),
        BuilderTab::History => history::show(ui, state),
        BuilderTab::Personae => personae::show(ui, state),
        BuilderTab::Hooks => hooks::show(ui, state),
        BuilderTab::Sites => sites::show(ui, state),
        BuilderTab::Missions => missions::show(ui, state),
        BuilderTab::Prose => prose::show(ui, state),
        BuilderTab::Analytics => analytics::show(ui, state),
        BuilderTab::Interestingness => interestingness::show(ui, state),
        BuilderTab::Search => search::show(ui, state),
        BuilderTab::Diff => diff::show(ui, state),
        BuilderTab::Briefing => briefing::show(ui, state),
        BuilderTab::Segmentum => segmentum::show(ui, state),
        BuilderTab::Export => export::show(ui, state),
        // XC-1: §V1 / §V2 diagnostics are first-class tabs (the two right-most
        // entries in `BuilderTab::ALL`) so the per-error / per-violation focus
        // buttons are reachable. The status-bar health pip still reads
        // `validation_report` directly for an always-visible summary.
        BuilderTab::Validation => validation::show(ui, state),
        BuilderTab::Invariants => invariants_panel::show(ui, state),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_n1_tab_has_a_dispatch_arm() {
        // Trivial sanity: iterate the enum and assert the label is non-empty,
        // covering the `match` indirectly. The compile-time check that every
        // arm is wired lives in `show_active_panel` itself.
        for tab in BuilderTab::ALL {
            assert!(!tab.label().is_empty());
        }
    }

    #[test]
    fn clusters_cover_every_tab_exactly_once() {
        // §UO6 P2: the grouped strip must stay a total, disjoint partition of
        // `BuilderTab::ALL` — no tab dropped, none duplicated. This guards a
        // new enum variant being added without a home in `TAB_CLUSTERS`.
        let mut seen: Vec<BuilderTab> = Vec::new();
        for (_, tabs) in TAB_CLUSTERS {
            seen.extend(tabs.iter().copied());
        }
        for tab in BuilderTab::ALL {
            assert!(
                seen.contains(tab),
                "{} is not in any TAB_CLUSTERS group",
                tab.label()
            );
        }
        assert_eq!(
            seen.len(),
            BuilderTab::ALL.len(),
            "TAB_CLUSTERS has a duplicate or stray tab"
        );
        let unique: std::collections::BTreeSet<&str> = seen.iter().map(|t| t.label()).collect();
        assert_eq!(unique.len(), seen.len(), "duplicate tab in TAB_CLUSTERS");
    }

    #[test]
    fn cluster_of_is_total_over_every_tab() {
        // §COLUMNS §6.1: the top-bar breadcrumb must resolve a cluster for every
        // tab, so `cluster_of` is total over `BuilderTab::ALL`.
        for tab in BuilderTab::ALL {
            assert!(cluster_of(*tab).is_some(), "{} has no cluster", tab.label());
        }
    }

    #[test]
    fn nav_rail_and_top_bar_paint_headless() {
        // §COLUMNS §6.1: the rail + slim top bar must paint without panicking on
        // a blank state (covers the collapse toggle + cluster sections).
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::SidePanel::left("nav_rail_test").show(ctx, |ui| show_nav_rail(ui, &mut state));
            egui::TopBottomPanel::top("top_bar_test").show(ctx, |ui| show_top_bar(ui, &mut state));
        });
        assert!(!state.nav_rail_collapsed);
    }
}
