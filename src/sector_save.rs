//! `SectorSave` — IDs-only runtime state split from the catalog (§13
//! NEXT.md, design §17).
//!
//! The single-file `sector.json` written by the generator contains both
//! the static catalog (system / world descriptors, world-type tags, route
//! geometry) and runtime state (faction presences, control summaries,
//! conflict ticks, intel). A `SectorSave` strips the runtime state into a
//! separate document keyed by IDs so a tool can:
//!
//! * Persist a long-running simulation without re-emitting catalog data
//!   that hasn't changed.
//! * Round-trip `(catalog, save)` back into a fully merged
//!   `GeneratedSector` for rendering and queries.
//!
//! The generator still emits the merged form by default (back-compat);
//! callers that want the split call `split()` after generation and
//! `merge()` on load.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::conflict::ConflictState;
use crate::intel::SystemIntel;
use crate::orbital_assets::{BlockadeReport, OrbitalAsset};
use crate::sector_model::{GeneratedSector, WorldControlSummary, WorldFactionPresence};

/// Runtime state keyed only by IDs into the catalog half.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SectorSave {
    pub sector_id: String,
    pub seed: String,
    pub generator_version: String,
    /// system_id → per-system runtime state.
    pub systems: BTreeMap<crate::ids::SystemId, SystemSave>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemSave {
    pub primary_factions: Vec<crate::ids::FactionId>,
    pub control: crate::sector_model::SystemControlSummary,
    pub stability: crate::stability::StabilityState,
    pub conflict: ConflictState,
    pub intel: SystemIntel,
    pub orbital_assets: Vec<OrbitalAsset>,
    pub blockade: BlockadeReport,
    pub archetype: crate::archetypes::ArchetypeState,
    pub worlds: BTreeMap<crate::ids::WorldId, WorldSave>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldSave {
    pub factions: Vec<WorldFactionPresence>,
    pub control: WorldControlSummary,
    pub stability: crate::stability::StabilityState,
    pub conflict: ConflictState,
    pub claims: Vec<crate::sector_model::FactionClaim>,
    pub regions: Vec<crate::surface_region::SurfaceRegion>,
}

/// Extract the runtime state from a `GeneratedSector`.
#[must_use]
pub fn split(sector: &GeneratedSector) -> SectorSave {
    let mut systems: BTreeMap<crate::ids::SystemId, SystemSave> = BTreeMap::new();
    for sys in &sector.systems {
        let mut worlds: BTreeMap<crate::ids::WorldId, WorldSave> = BTreeMap::new();
        for w in &sys.worlds {
            worlds.insert(
                w.id.clone(),
                WorldSave {
                    factions: w.factions.clone(),
                    control: w.control.clone(),
                    stability: w.stability,
                    conflict: w.conflict.clone(),
                    claims: w.claims.clone(),
                    regions: w.regions.clone(),
                },
            );
        }
        systems.insert(
            sys.id.clone(),
            SystemSave {
                primary_factions: sys.primary_factions.clone(),
                control: sys.control.clone(),
                stability: sys.stability,
                conflict: sys.conflict.clone(),
                intel: sys.intel.clone(),
                orbital_assets: sys.orbital_assets.clone(),
                blockade: sys.blockade.clone(),
                archetype: sys.archetype.clone(),
                worlds,
            },
        );
    }
    SectorSave {
        sector_id: sector.id.to_string(),
        seed: sector.seed.to_string(),
        generator_version: sector.generator_version.to_string(),
        systems,
    }
}

/// Apply a `SectorSave` onto a fresh-from-catalog `GeneratedSector` to
/// restore runtime state. Catalog mismatch (different sector id or seed)
/// returns an error.
pub fn merge(sector: &mut GeneratedSector, save: SectorSave) -> Result<(), MergeError> {
    if sector.id.as_ref() != save.sector_id.as_str() {
        return Err(MergeError::SectorIdMismatch {
            catalog: sector.id.to_string(),
            save: save.sector_id,
        });
    }
    if sector.seed.as_ref() != save.seed.as_str() {
        return Err(MergeError::SeedMismatch {
            catalog: sector.seed.to_string(),
            save: save.seed,
        });
    }
    for sys in &mut sector.systems {
        let Some(ss) = save.systems.get(&sys.id) else {
            continue;
        };
        sys.primary_factions = ss.primary_factions.clone();
        sys.control = ss.control.clone();
        sys.stability = ss.stability;
        sys.conflict = ss.conflict.clone();
        sys.intel = ss.intel.clone();
        sys.orbital_assets = ss.orbital_assets.clone();
        sys.blockade = ss.blockade.clone();
        sys.archetype = ss.archetype.clone();
        for w in &mut sys.worlds {
            if let Some(ws) = ss.worlds.get(&w.id) {
                w.stability = ws.stability;
                w.conflict = ws.conflict.clone();
                w.claims = ws.claims.clone();
                w.regions = ws.regions.clone();
                w.factions = ws.factions.clone();
                w.control = ws.control.clone();
            }
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    #[error("sector id mismatch: catalog={catalog} save={save}")]
    SectorIdMismatch { catalog: String, save: String },
    #[error("seed mismatch: catalog={catalog} save={save}")]
    SeedMismatch { catalog: String, save: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        GeneratedStar, GeneratedSystem, GenerationManifest, HexCoord, SystemControlSummary,
    };

    fn empty_sector(id: &str) -> GeneratedSector {
        GeneratedSector {
            id: id.into(),
            title: "t".into(),
            seed: "abc".into(),
            generator_name: "t".into(),
            generator_version: "0.1".into(),
            width: 4,
            height: 4,
            systems: vec![GeneratedSystem {
                id: "sys-0001".into(),
                index: 1,
                name: "T".into(),
                coord: HexCoord { q: 0, r: 0 },
                kind: crate::sector_model::SystemKind::Star,
                star: Some(GeneratedStar {
                    colour_code: "A".into(),
                    colour_name: "A".into(),
                    spectral_type: None,
                    source_row_index: None,
                }),
                worlds: vec![],
                primary_factions: vec![],
                tags: vec![],
                notes: vec![],
                control: SystemControlSummary::default(),
                stability: Default::default(),
                orbital_assets: vec![],
                blockade: Default::default(),
                conflict: Default::default(),
                intel: Default::default(),
                archetype: Default::default(),
            }],
            routes: vec![],
            factions: vec![],
            manifest: GenerationManifest {
                project_id: id.into(),
                generated_at_policy: "t".into(),
                generator_name: "t".into(),
                generator_version: "0.1".into(),
                seed: "abc".into(),
                seed_hash: "t".into(),
                profile: None,
                input_digests: BTreeMap::new(),
                settings_digest: "t".into(),
                system_count: 1,
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new().into(),
            economy: Default::default(),
            chronicle: Default::default(),
        }
    }

    #[test]
    fn round_trip_split_then_merge_preserves_state() {
        let mut s = empty_sector("t1");
        s.systems[0].primary_factions = vec!["a".into(), "b".into()];
        let save = split(&s);
        let mut fresh = empty_sector("t1");
        merge(&mut fresh, save).unwrap();
        assert_eq!(fresh.systems[0].primary_factions, vec!["a", "b"]);
    }

    #[test]
    fn merge_rejects_id_mismatch() {
        let s = empty_sector("t1");
        let save = split(&s);
        let mut other = empty_sector("t2");
        assert!(merge(&mut other, save).is_err());
    }
}
