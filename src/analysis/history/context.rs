//! Read-only borrowed context threaded through every emit-family module.

use std::collections::BTreeMap;

use crate::sector_model::GeneratedSector;

use super::config::HistoryConfig;

pub(super) struct EmitContext<'a> {
    pub(super) cfg: &'a HistoryConfig,
    pub(super) sector: &'a GeneratedSector,
    pub(super) faction_names: &'a BTreeMap<&'a str, &'a str>,
    pub(super) system_names: &'a BTreeMap<&'a str, &'a str>,
    /// Re-roll suffix folded into the per-event
    /// `("history-event","<anchor>:<kind>:<ordinal>")` RNG discriminator. Empty
    /// (the default) reproduces the legacy key byte-for-byte; `":r{n}"` yields a
    /// deterministically distinct chronicle. Threaded from
    /// [`crate::generation::generate_prefix`] via the `Stage::Chronicle` nonce.
    pub(super) reroll_suffix: &'a str,
}
