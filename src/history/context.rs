//! Read-only borrowed context threaded through every emit-family module.

use std::collections::BTreeMap;

use crate::sector_model::GeneratedSector;

use super::config::HistoryConfig;

pub(super) struct EmitContext<'a> {
    pub(super) cfg: &'a HistoryConfig,
    pub(super) sector: &'a GeneratedSector,
    pub(super) faction_names: &'a BTreeMap<&'a str, &'a str>,
    pub(super) system_names: &'a BTreeMap<&'a str, &'a str>,
}
