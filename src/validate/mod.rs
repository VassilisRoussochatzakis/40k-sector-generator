//! Validation + diff layer.
//!
//! `validation` runs pre-generation checks on a loaded project.
//! `invariants` runs post-generation checks on a built `GeneratedSector`
//! (spec §11.11). `diff` computes a structural before/after report.

pub mod diff;
pub mod invariants;
pub mod validation;
