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

use sectorforge_gui_core::card;
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;

use super::{
    analytics, briefing, control, diff, economy, export, factions, history, hooks, interestingness,
    invariants as invariants_panel, iterative_gen, map, missions, personae, project, prose,
    regions, relations, routes, search, segmentum, sites, subsectors, system, validation, world,
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
    (
        "OUTPUT",
        &[
            BuilderTab::Segmentum,
            BuilderTab::IterativeGen,
            BuilderTab::Export,
        ],
    ),
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
        let can_back = !state.selection.nav_back_stack.is_empty();
        let can_forward = !state.selection.nav_forward_stack.is_empty();
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
    // §COLUMNS §6.1: every entry gets one common width — the widest label
    // (`INTERESTINGNESS`) — so the selectable highlight boxes line up instead of
    // hugging each label. Measured once from the live font set below.
    let item_w = max_tab_label_width(ui);
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for (label, tabs) in TAB_CLUSTERS {
                ui_kit::collapsing_section(ui, ("nav_cluster", *label), label, true, |ui| {
                    // A fixed-width, cross-justified column makes each
                    // `selectable_label` fill `item_w` (left-aligned text).
                    ui.allocate_ui_with_layout(
                        egui::vec2(item_w, 0.0),
                        egui::Layout::top_down_justified(egui::Align::LEFT),
                        |ui| {
                            ui.set_min_width(item_w);
                            for tab in *tabs {
                                let selected = state.active_tab == *tab;
                                // §BEAUTY — the active tab reads as a "cold void of
                                // space" plate: a deep, near-black starfield that softly
                                // twinkles, ringed by a cool starlight hairline, instead
                                // of the brass-barred accent the roster rails use. Hover
                                // still lifts, so unselected tabs respond to the pointer.
                                let (resp, _) = card::selectable_void_plate(
                                    ui,
                                    ("nav_tab", *tab),
                                    selected,
                                    |ui| {
                                        ui.label(tab.label());
                                    },
                                );
                                if resp.clicked() {
                                    state.set_active_tab(*tab);
                                }
                            }
                        },
                    );
                });
            }
        });
}

/// Galley width (px) of the widest [`BuilderTab`] label in the rail's button
/// font (the widest is `INTERESTINGNESS`). Measured from the live font set so it
/// tracks the active theme rather than being hard-coded.
fn widest_tab_label_px(ctx: &egui::Context) -> f32 {
    let font_id = egui::TextStyle::Button.resolve(&ctx.style());
    BuilderTab::ALL
        .iter()
        .map(|tab| {
            ctx.fonts(|f| {
                f.layout_no_wrap(
                    tab.label().to_owned(),
                    font_id.clone(),
                    egui::Color32::WHITE,
                )
                .size()
                .x
            })
        })
        .fold(0.0_f32, f32::max)
}

/// The common width every nav-rail entry is sized to: the widest label plus the
/// selectable's horizontal padding.
fn max_tab_label_width(ui: &egui::Ui) -> f32 {
    widest_tab_label_px(ui.ctx()) + ui.spacing().button_padding.x * 2.0
}

/// Exact width the `builder_nav_rail` side panel needs so the widest entry box
/// (`INTERESTINGNESS`) fits without clipping. The panel is pinned to this width
/// (non-resizable); only the `☰` / `‹‹` toggle hides it. On top of the entry
/// width this adds the chrome around each entry: the collapsing-header indent,
/// the scrollbar, and the 8px group-frame + 8px side-panel inner margins (both
/// applied on each side, matching [`show_nav_rail`] and `app.rs`).
pub fn rail_width(ctx: &egui::Context) -> f32 {
    let style = ctx.style();
    let item_w = widest_tab_label_px(ctx) + style.spacing.button_padding.x * 2.0;
    let indent = style.spacing.indent;
    let scrollbar = style.spacing.scroll.bar_width + style.spacing.scroll.bar_inner_margin;
    item_w + indent + scrollbar + 2.0 * (8.0 + 8.0) + 4.0
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
        BuilderTab::IterativeGen => iterative_gen::show(ui, state),
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
    fn bundled_fonts_nav_plate_centers_are_clickable() {
        // Regression for the "dead tabs" bug: with the bundled OFL fonts (the
        // shipping default — see `default = bundled-fonts`) the inner row label
        // is taller than egui's built-in face and grows to fill the plate's
        // centre. Before the `card::selectable_plate` fix that non-clickable
        // label became the topmost widget under the pointer and swallowed the
        // click, so every nav plate was dead down the middle — exactly where a
        // user clicks the label. This walks each tab's clickable y-band and
        // asserts it is GAP-FREE (no dead centre). It only catches the bug with
        // bundled fonts installed, so install them like `main.rs` does.
        use std::collections::BTreeMap;
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1400.0, 900.0));
        let btn = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        // Sweep a column of clicks down the rail; record which tab each y selects.
        let mut hits: BTreeMap<i32, &'static str> = BTreeMap::new();
        let mut y = 50i32;
        while y < 880 {
            let pos = egui::pos2(70.0, y as f32);
            let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
            state.active_tab = BuilderTab::Invariants; // sentinel: detect any switch
            let ctx = egui::Context::default();
            sectorforge_gui_core::fonts::install(&ctx);
            let frame = |ctx: &egui::Context, state: &mut BuilderState| {
                egui::TopBottomPanel::top("builder_workspace_tabs")
                    .show(ctx, |ui| show_top_bar(ui, state));
                egui::TopBottomPanel::bottom("builder_status")
                    .show(ctx, |ui| crate::builder::panels::status::show(ui, state));
                egui::SidePanel::left("builder_nav_rail")
                    .resizable(false)
                    .exact_width(rail_width(ctx))
                    .show(ctx, |ui| show_nav_rail(ui, state));
                egui::CentralPanel::default().show(ctx, |ui| show_active_panel(ui, state));
            };
            let ri = |events: Vec<egui::Event>| egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let _ = ctx.run(ri(vec![]), |ctx| frame(ctx, &mut state));
            let _ = ctx.run(ri(vec![egui::Event::PointerMoved(pos)]), |ctx| {
                frame(ctx, &mut state)
            });
            let _ = ctx.run(ri(vec![btn(pos, true)]), |ctx| frame(ctx, &mut state));
            let _ = ctx.run(ri(vec![btn(pos, false)]), |ctx| frame(ctx, &mut state));
            if state.active_tab != BuilderTab::Invariants {
                hits.insert(y, state.active_tab.label());
            }
            y += 2;
        }
        // For every tab that appears, its band [min,max] must be contiguous at
        // the 2px sweep step — a hole means a dead centre (the bug).
        let mut band: BTreeMap<&'static str, (i32, i32)> = BTreeMap::new();
        for (yy, label) in &hits {
            let e = band.entry(label).or_insert((*yy, *yy));
            e.0 = e.0.min(*yy);
            e.1 = e.1.max(*yy);
        }
        assert!(
            band.len() >= 18,
            "expected most tabs reachable in a 900px rail, got {}",
            band.len()
        );
        for (label, (lo, hi)) in &band {
            let mut yy = *lo;
            while yy <= *hi {
                assert!(
                    hits.get(&yy) == Some(label),
                    "tab {label} has a dead spot at y={yy} inside its band {lo}..{hi} \
                     — selectable_plate row click is being swallowed by inner content",
                );
                yy += 2;
            }
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
