#![forbid(unsafe_code)]
//! GUI module: egui-based viewer for generated sectors. Modular by entity:
//! `palette` owns colors, `sector_view` / `system_view` are pure render widgets,
//! `info_panel` formats text, `app` wires everything together.

pub mod app;
pub mod dashboard;
pub mod data_editor;
pub mod editor;
pub mod factions_overview;
pub mod preset_gallery;
pub mod route_planner;
pub mod segmentum_view;

pub use app::App;
pub use sectorforge_gui_core::jobs::{spawn_job, JobContext, JobHandle};
pub use sectorforge_gui_core::{
    heatmap, info_panel, jobs, palette, sector_view, system_view, theme, ui_kit,
};
