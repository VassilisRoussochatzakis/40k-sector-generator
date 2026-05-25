//! Tunables for chronicle derivation: epochs, per-anchor caps, eras, rules.

use serde::{Deserialize, Serialize};

use super::model::EventKind;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HistoryConfig {
    /// Toggle for embedding `chronicle` in generated `sector.json`. The
    /// standalone CLI command forces derivation even when the generated
    /// sector was produced before this field existed.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Imperial-style millennium prefix. Foundation events anchor at
    /// `{epoch_start}.000`, present-day events at `{epoch_end}.999`.
    #[serde(default = "default_epoch_start")]
    pub epoch_start: u32,
    #[serde(default = "default_epoch_end")]
    pub epoch_end: u32,
    /// Maximum events listed per world. The most narratively-weighty events
    /// survive truncation first.
    #[serde(default = "default_per_world")]
    pub max_events_per_world: u32,
    /// Maximum events listed per system (system-anchored events only).
    #[serde(default = "default_per_system")]
    pub max_events_per_system: u32,
    /// Maximum route-origin events listed per route.
    #[serde(default = "default_per_route")]
    pub max_events_per_route: u32,
    /// Cap on the sector-wide "Key events" digest in the Markdown output.
    #[serde(default = "default_key_events")]
    pub key_events_top_n: u32,
    /// Maximum subsector-capital events embedded in the generated chronicle.
    /// Huge sectors sample this many representative systems instead of running
    /// full subsector clustering only for flavor text.
    #[serde(default = "default_max_subsector_events")]
    pub max_subsector_events: u32,
    /// Era bands used to label generated events. `allowed_events` may be
    /// empty, in which case the era is a catch-all fallback.
    #[serde(default = "default_eras")]
    pub eras: Vec<HistoryEra>,
    /// Optional declarative rules that ensure at least N events are present
    /// for matching current-state facts (for example, Warzone ⇒ War).
    #[serde(default)]
    pub event_rules: Vec<HistoryEventRule>,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            epoch_start: default_epoch_start(),
            epoch_end: default_epoch_end(),
            max_events_per_world: default_per_world(),
            max_events_per_system: default_per_system(),
            max_events_per_route: default_per_route(),
            key_events_top_n: default_key_events(),
            max_subsector_events: default_max_subsector_events(),
            eras: default_eras(),
            event_rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HistoryFile {
    pub history: HistoryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryEra {
    pub id: String,
    pub label: String,
    pub relative_start: i32,
    pub relative_end: i32,
    #[serde(default = "default_era_weight")]
    pub weight: f32,
    #[serde(default)]
    pub allowed_events: Vec<EventKind>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryEventRule {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub when_system_state: Option<String>,
    #[serde(default)]
    pub prefer_event: Option<String>,
    #[serde(default = "default_minimum_events")]
    pub minimum_events: u32,
}

fn default_epoch_start() -> u32 {
    36
}
fn default_epoch_end() -> u32 {
    42
}
fn default_per_world() -> u32 {
    6
}
fn default_per_system() -> u32 {
    3
}
fn default_per_route() -> u32 {
    1
}
fn default_key_events() -> u32 {
    20
}
fn default_max_subsector_events() -> u32 {
    64
}
fn default_true() -> bool {
    true
}
fn default_era_weight() -> f32 {
    1.0
}
fn default_minimum_events() -> u32 {
    1
}

fn default_eras() -> Vec<HistoryEra> {
    use EventKind::*;
    vec![
        HistoryEra {
            id: "age_of_foundation".into(),
            label: "Age of Foundation".into(),
            relative_start: -900,
            relative_end: -650,
            weight: 1.0,
            allowed_events: vec![Foundation, Discovery],
        },
        HistoryEra {
            id: "age_of_compliance".into(),
            label: "Age of Compliance".into(),
            relative_start: -649,
            relative_end: -300,
            weight: 1.0,
            allowed_events: vec![
                ImperialMandateGranted,
                CommercialCharter,
                DynasticClaim,
                Consecration,
                Annexation,
            ],
        },
        HistoryEra {
            id: "age_of_fracture".into(),
            label: "Age of Fracture".into(),
            relative_start: -299,
            relative_end: -80,
            weight: 1.0,
            allowed_events: vec![
                Secession,
                Uprising,
                CultExposed,
                AeldariActivity,
                TauContact,
            ],
        },
        HistoryEra {
            id: "age_of_wounds".into(),
            label: "Age of Wounds".into(),
            relative_start: -79,
            relative_end: -1,
            weight: 1.0,
            allowed_events: vec![
                NecronAwakening,
                TyranidContact,
                OrkWaaagh,
                ChaosIncursion,
                WarpStormSurge,
                QuarantineDeclared,
                Blockade,
            ],
        },
        HistoryEra {
            id: "present_crisis".into(),
            label: "Present Crisis".into(),
            relative_start: 0,
            relative_end: 0,
            weight: 1.0,
            allowed_events: vec![Reconquest, Purge],
        },
    ]
}
