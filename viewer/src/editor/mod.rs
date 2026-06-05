//! Sector editor: load/create/edit/save a `GeneratedSector` via GUI.
//! Self-contained — does not call generator. All edits go through dropdowns
//! over the same string sets the DTOs use.

pub mod enums;
pub mod file_ops;
pub mod state;

mod dialogs;
mod factions_panel;
mod generation_panel;
mod map_panel;
mod routes_panel;
mod settings_panel;
mod system_panel;
mod toolbar;
mod ui_helpers;
mod wishes_panel;
mod world_panel;

pub(crate) use dialogs::draw_dialog;
pub(crate) use factions_panel::show_factions;
pub(crate) use generation_panel::show_generation_settings;
pub(crate) use map_panel::show_map;
pub(crate) use routes_panel::show_routes;
pub(crate) use settings_panel::show_settings;
pub use state::{EditorState, PreviewJobResult};
pub(crate) use system_panel::show_system_inspector;
pub(crate) use toolbar::editor_toolbar;
pub(crate) use wishes_panel::show_wishes;
pub(crate) use world_panel::show_world_inspector;
