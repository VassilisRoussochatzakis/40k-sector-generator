//! MAP tab subsector + lookup cache (§S2 / §SUB1).
//!
//! Rebuilds the per-sector [`MapViewCache`] (subsector clustering + hex →
//! system lookup + region tints) when the underlying sector slice digest
//! changes. Pure — no UI side effects.

use sectorforge::subsectors::SubsectorConfig;
use sectorforge_gui_core::sector_view::SectorMapCache;

use crate::builder::derivation_cache::digest_input;
use crate::builder::state::MapViewCache;
use crate::builder::BuilderState;

/// Rebuilds [`MapViewCache`] when the underlying sector slice digest changes.
/// Pure — no UI side effects. Cheap when the cache is hot.
pub(super) fn refresh_map_cache(state: &mut BuilderState) {
    let digest = sector_view_digest(state);
    let stale = state
        .map_view
        .cache
        .as_ref()
        .map(|c| c.digest != digest)
        .unwrap_or(true);
    if !stale {
        return;
    }
    // #25: `catch_build_subsectors` also catches an engine panic from a
    // structurally-valid-but-inconsistent loaded sector (an `expect` inside
    // `build_subsectors` the `Result` doesn't cover), surfacing it as the same
    // `last_subsector_error` string instead of aborting the builder. Under
    // release `panic = "abort"` the catch is a no-op; the gui-core panic-hook
    // crash note (#4) is the fallback there.
    let mut subsectors = match crate::builder::panels::subsectors::catch_build_subsectors(
        &state.sector,
        SubsectorConfig {
            target_systems_per_subsector: state.subsector_target_systems.max(1),
            ..SubsectorConfig::default()
        },
    ) {
        Ok(v) => {
            state.feedback.last_subsector_error = None;
            v
        }
        Err(e) => {
            state.feedback.last_subsector_error = Some(e);
            Vec::new()
        }
    };
    crate::builder::panels::subsectors::apply_subsector_overrides(&mut subsectors, state);
    let lookup = SectorMapCache::new(&state.sector, &subsectors);
    state.map_view.cache = Some(MapViewCache {
        digest,
        subsectors,
        lookup,
    });
}

pub(super) fn sector_view_digest(state: &BuilderState) -> String {
    // Hash the minimal slice that drives subsector clustering + region tints.
    // Keeping the slice narrow avoids invalidating the cache on unrelated
    // edits (e.g. faction prose). §SUB2..§SUB4 overrides also feed in so the
    // cache rebuilds when the user reclusters, moves systems between cells,
    // or overrides a capital.
    let sector = &state.sector;
    #[derive(serde::Serialize)]
    struct Slice<'a> {
        w: u32,
        h: u32,
        systems: Vec<(&'a str, i32, i32)>,
        routes: Vec<(&'a str, &'a str, &'a str)>,
        regions: Vec<(&'a str, Vec<(i32, i32)>)>,
        sub_target: u32,
        sub_sys: Vec<(&'a str, &'a str)>,
        sub_cap: Vec<(&'a str, &'a str)>,
    }
    let slice = Slice {
        w: sector.width,
        h: sector.height,
        systems: sector
            .systems
            .iter()
            .map(|s| (s.id.as_str(), s.coord.q, s.coord.r))
            .collect(),
        routes: sector
            .routes
            .iter()
            .map(|r| {
                (
                    r.id.as_str(),
                    r.from_system_id.as_str(),
                    r.to_system_id.as_str(),
                )
            })
            .collect(),
        regions: sector
            .regions
            .iter()
            .map(|r| (r.id.as_str(), r.hexes.iter().map(|h| (h.q, h.r)).collect()))
            .collect(),
        sub_target: state.subsector_target_systems,
        sub_sys: state
            .subsector_system_overrides
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect(),
        sub_cap: state
            .subsector_capital_overrides
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect(),
    };
    digest_input(&slice)
}
