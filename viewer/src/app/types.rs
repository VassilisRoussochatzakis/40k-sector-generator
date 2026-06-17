use crate::system_view::SystemSelection;

#[derive(Debug, Clone)]
pub enum PendingExport {
    SectorPng,
    SectorSvg,
    AllSystemPngs,
    SystemPng(sectorforge::ids::SystemId),
    /// §11 NEW.md: self-contained interactive HTML map.
    SectorHtml,
}

#[derive(Debug)]
pub struct ExportJobResult(pub String);

impl ExportJobResult {
    pub fn into_status(self) -> String {
        self.0
    }
}

impl sectorforge_gui_core::jobs::FromJobPanic for ExportJobResult {
    fn from_job_panic(message: String) -> Option<Self> {
        Some(Self(format!("export panicked: {message}")))
    }
}

#[derive(Debug, Clone)]
pub enum View {
    Sector,
    System {
        system_id: sectorforge::ids::SystemId,
        selection: SystemSelection,
    },
    Edit,
    Data,
    Planner,
    Dashboard,
    Factions,
    Relations,
    Regions,
    Trade,
    History,
    Segmentum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactionsMode {
    Overview,
    Designer,
}
