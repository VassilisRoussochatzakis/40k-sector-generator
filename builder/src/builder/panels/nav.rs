//! Top-level tab router (§N1 / §N2).
//!
//! [`show_top_bar`] renders the horizontal tab strip, grouping every
//! [`BuilderTab`] into the labeled clusters in [`TAB_CLUSTERS`] (§UO6 P2);
//! selecting one writes to
//! [`BuilderState::active_tab`]. [`show_active_panel`] dispatches the active
//! tab to the matching panel module under this directory — the contract every
//! tab follows per §N2.
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

/// Render the top tab strip. Mutates [`BuilderState::active_tab`] when the
/// user clicks a tab. Leftmost two chevrons walk the §LINK3 nav history.
pub fn show_top_bar(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
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
        // §UO6 P2: walk the labeled clusters instead of the flat `ALL` list. A
        // dim cluster tag precedes each group and a separator divides them, so
        // the 26-tab strip reads as six task areas rather than one wall.
        for (ci, (label, tabs)) in TAB_CLUSTERS.iter().enumerate() {
            if ci > 0 {
                ui.separator();
            }
            ui.label(
                egui::RichText::new(*label)
                    .small()
                    .color(palette::chrome_text_dim()),
            );
            for tab in *tabs {
                let selected = state.active_tab == *tab;
                if ui.selectable_label(selected, tab.label()).clicked() {
                    state.set_active_tab(*tab);
                }
            }
        }
    });
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
}
