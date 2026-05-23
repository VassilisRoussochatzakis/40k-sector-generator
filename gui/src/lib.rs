//! GUI module: egui-based viewer for generated sectors. Modular by entity:
//! `palette` owns colors, `sector_view` / `system_view` are pure render widgets,
//! `info_panel` formats text, `app` wires everything together.

pub mod app;
pub mod dashboard;
pub mod data_editor;
pub mod editor;
pub mod factions_overview;
pub mod heatmap;
pub mod info_panel;
pub mod jobs;
pub mod palette;
pub mod preset_gallery;
pub mod route_planner;
pub mod sector_view;
pub mod segmentum_view;
pub mod system_view;

pub use app::App;
pub use jobs::{JobHandle, JobContext, spawn_job};
