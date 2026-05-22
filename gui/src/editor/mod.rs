//! Sector editor: load/create/edit/save a `GeneratedSector` via GUI.
//! Self-contained — does not call generator. All edits go through dropdowns
//! over the same string sets the DTOs use.

pub mod enums;
pub mod file_ops;
pub mod state;

mod dialogs;
mod factions_panel;
mod map_panel;
mod routes_panel;
mod settings_panel;
mod system_panel;
mod toolbar;
mod ui_helpers;
mod world_panel;

pub use dialogs::draw_dialog;
pub use factions_panel::show_factions;
pub use map_panel::show_map;
pub use routes_panel::show_routes;
pub use settings_panel::show_settings;
pub use state::EditorState;
pub use system_panel::show_system_inspector;
pub use toolbar::editor_toolbar;
pub use world_panel::show_world_inspector;
