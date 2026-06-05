//! §REG1..§REG3 warp-region mutators on [`super::BuilderState`].
//!
//! D1: structural region edits (add / remove / table edits / seeded-grow) and
//! committed brush *strokes* now route through the command bus
//! ([`BuilderCommand::AddRegion`] / [`RemoveRegion`] / [`EditRegion`]) so they
//! are undoable like `SetRegionKind` / `RenameRegion` already were. The
//! per-frame paint/erase primitives ([`Self::paint_region_hex`] /
//! [`Self::erase_region_hex`]) stay off-bus on purpose: they are the *live
//! preview* applied while a brush drag is in flight. The whole stroke is
//! snapshot on [`Self::begin_region_stroke`] and committed as one
//! `EditRegion` on [`Self::commit_region_stroke`], mirroring how a system
//! drag previews with a ghost and only commits `MoveSystem` on drop.

use sectorforge::invariants::check_sector;
use sectorforge::sector_model::mutation::MutationError;

use super::super::command::BuilderCommand;
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
        // D1: route through the bus so creation is undoable. `run` handles the
        // dirty / invariant / validation / auto-save bookkeeping; `AddRegion`
        // is infallible (`sector.add_region` cannot fail), so the result is
        // discarded.
        let _ = self.run(BuilderCommand::AddRegion {
            id: id.clone(),
            name: name.to_string(),
            kind,
            centre,
        });
        // Transient UI selection (§R4 carve-out) — not document state.
        self.selected_region_id = Some(id.clone());
        id
    }

    pub fn remove_region(&mut self, id: &str) -> Result<(), BuilderError> {
        self.run(BuilderCommand::RemoveRegion {
            id: id.to_string(),
            before: None,
        })?;
        // Transient UI: move the selection off the region we just deleted.
        if self.selected_region_id.as_deref() == Some(id) {
            self.selected_region_id = self.sector.regions.first().map(|r| r.id.clone());
        }
        Ok(())
    }

    /// D1 live-preview primitive: add one hex to a region's footprint directly,
    /// off-bus. Used per-frame while a brush drag is in flight; the net change
    /// is committed as one undoable `EditRegion` by
    /// [`Self::commit_region_stroke`]. Deliberately does *not* auto-save —
    /// only the committed stroke persists.
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

    /// D1 live-preview primitive: erase one hex from a region's footprint
    /// directly, off-bus. See [`Self::paint_region_hex`].
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

    /// D1: snapshot the region about to be brushed so a paint/erase *stroke*
    /// commits as a single undoable `EditRegion` on release. No-op snapshot
    /// when the region id is unknown. Cleared by
    /// [`Self::commit_region_stroke`]. The snapshot is transient drag scratch
    /// (§R4 carve-out), so it is written directly.
    pub fn begin_region_stroke(&mut self, region_id: &str) {
        self.region_stroke_before = self
            .sector
            .regions
            .iter()
            .find(|r| r.id == region_id)
            .map(|r| Box::new(r.clone()));
    }

    /// D1: commit the in-progress brush stroke. The live region already holds
    /// the painted hexes (applied per-frame for feedback); this routes the net
    /// hex-list change through the bus as one undoable `EditRegion`. No-op when
    /// nothing was snapshot or the footprint is unchanged.
    pub fn commit_region_stroke(&mut self) {
        let Some(before) = self.region_stroke_before.take() else {
            return;
        };
        let Some(after) = self
            .sector
            .regions
            .iter()
            .find(|r| r.id == before.id)
            .cloned()
        else {
            return;
        };
        // A stroke only edits the hex footprint, so compare that (WarpRegion
        // has no `PartialEq`). No net change → no undo entry.
        if after.hexes == before.hexes {
            return;
        }
        let _ = self.run(BuilderCommand::EditRegion {
            region: before.id.clone(),
            before: Some(before),
            after: Box::new(after),
        });
    }

    /// Update a region's mutable identity / kind / hex list / centre / name.
    /// Used by REG1 (table edits) and REG3 (seeded-grow replaces hex list).
    ///
    /// D1: routes the edit through [`BuilderCommand::EditRegion`] so table and
    /// grow edits are undoable too. The closure mutates a clone; the live
    /// region is untouched until `run` applies `after`, so `EditRegion::apply`
    /// captures the correct `before`.
    pub fn update_region(
        &mut self,
        id: &str,
        mutate: impl FnOnce(&mut sectorforge::regions::WarpRegion),
    ) -> Result<(), BuilderError> {
        let mut after = self
            .sector
            .regions
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or_else(|| BuilderError::Mutation(MutationError::RegionNotFound(id.to_string())))?;
        mutate(&mut after);
        self.run(BuilderCommand::EditRegion {
            region: id.to_string(),
            before: None,
            after: Box::new(after),
        })
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
