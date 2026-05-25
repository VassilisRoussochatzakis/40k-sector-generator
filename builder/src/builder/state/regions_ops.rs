//! §REG1..§REG3 warp-region mutators on [`super::BuilderState`]. Overlay edits
//! don't go through the command bus (per §D3) but each path still refreshes
//! the invariant report + arms validation + fires auto-save so REG6 lights up
//! immediately when a paint introduces an overlap / out-of-bounds hex.

use sectorforge::invariants::check_sector;

use super::super::errors::BuilderError;
use super::BuilderState;

impl BuilderState {
    pub fn add_region(
        &mut self,
        name: &str,
        kind: sectorforge::regions::RegionConditionKind,
        centre: sectorforge::sector_model::HexCoord,
    ) -> String {
        let id = self.next_region_id();
        let actual_id = self.sector.add_region(&id, name, kind, centre);
        self.selected_region_id = Some(actual_id.clone());
        self.dirty = true;
        self.invariant_report = Some(check_sector(&self.sector));
        self.mark_validation_dirty();
        self.trigger_auto_save();
        actual_id
    }

    pub fn remove_region(&mut self, id: &str) -> Result<(), BuilderError> {
        self.sector
            .remove_region(id)
            .map_err(BuilderError::Mutation)?;
        if self.selected_region_id.as_deref() == Some(id) {
            self.selected_region_id = self.sector.regions.first().map(|r| r.id.clone());
        }
        self.dirty = true;
        self.invariant_report = Some(check_sector(&self.sector));
        self.mark_validation_dirty();
        self.trigger_auto_save();
        Ok(())
    }

    pub fn paint_region_hex(
        &mut self,
        id: &str,
        hex: sectorforge::sector_model::HexCoord,
    ) -> Result<(), BuilderError> {
        self.sector
            .add_region_hex(id, hex)
            .map_err(BuilderError::Mutation)?;
        self.dirty = true;
        self.invariant_report = Some(check_sector(&self.sector));
        self.mark_validation_dirty();
        Ok(())
    }

    pub fn erase_region_hex(
        &mut self,
        id: &str,
        hex: sectorforge::sector_model::HexCoord,
    ) -> Result<(), BuilderError> {
        self.sector
            .remove_region_hex(id, hex)
            .map_err(BuilderError::Mutation)?;
        self.dirty = true;
        self.invariant_report = Some(check_sector(&self.sector));
        self.mark_validation_dirty();
        Ok(())
    }

    /// Update a region's mutable identity / kind / hex list / centre / name.
    /// Used by REG1 (table edits) and REG3 (seeded-grow replaces hex list).
    pub fn update_region(
        &mut self,
        id: &str,
        mutate: impl FnOnce(&mut sectorforge::regions::WarpRegion),
    ) -> Result<(), BuilderError> {
        let mut regions = (*self.sector.regions).clone();
        let target = regions.iter_mut().find(|r| r.id == id).ok_or_else(|| {
            BuilderError::Mutation(
                sectorforge::sector_model::mutation::MutationError::RegionNotFound(id.to_string()),
            )
        })?;
        mutate(target);
        self.sector.regions = std::sync::Arc::new(regions);
        self.dirty = true;
        self.invariant_report = Some(check_sector(&self.sector));
        self.mark_validation_dirty();
        self.trigger_auto_save();
        Ok(())
    }

    /// Pick the next free `reg-NNNN` id, matching the format used by
    /// `regions::build_regions`. Scans the existing region list so manual
    /// adds and grown regions share one namespace.
    pub fn next_region_id(&self) -> String {
        let mut max: u32 = 0;
        for r in self.sector.regions.iter() {
            if let Some(rest) = r.id.strip_prefix("reg-") {
                if let Ok(n) = rest.parse::<u32>() {
                    max = max.max(n);
                }
            }
        }
        format!("reg-{:04}", max + 1)
    }
}
