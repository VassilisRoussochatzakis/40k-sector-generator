//! Sector editor: load/create/edit/save a `GeneratedSector` via GUI.
//! Self-contained — does not call generator. All edits go through dropdowns
//! over the same string sets the DTOs use.

pub mod enums;
pub mod file_ops;
pub mod state;

mod map_panel;
mod system_panel;
mod world_panel;
mod routes_panel;
mod factions_panel;
mod settings_panel;
mod dialogs;
mod toolbar;
mod ui_helpers;

pub use state::EditorState;
pub use toolbar::editor_toolbar;
pub use dialogs::draw_dialog;
pub use map_panel::show_map;
pub use system_panel::show_system_inspector;
pub use world_panel::show_world_inspector;
pub use routes_panel::show_routes;
pub use factions_panel::show_factions;
pub use settings_panel::show_settings;
