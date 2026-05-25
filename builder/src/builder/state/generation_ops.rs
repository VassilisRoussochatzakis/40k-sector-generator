//! Per-system / per-world / preview / partial-regen helpers on
//! [`super::BuilderState`]. §G2..§G5 + §S5 + §W4 wiring.

use std::collections::{BTreeMap, BTreeSet};

use sectorforge::ids::{SystemId, WorldId};
use sectorforge::invariants::check_sector;

use super::super::command::BuilderCommand;
use super::super::errors::BuilderError;
use super::super::index::BuilderIndex;
use super::BuilderState;

impl BuilderState {
    /// §S5: drop a freshly generated system at `coord`. The new system is built
    /// via [`sectorforge::generate_system_standalone`] using the in-memory
    /// catalogs, then committed through the command bus as
    /// [`super::super::command::BuilderCommand::ReplaceSystem`]. Any existing
    /// system at `coord` is replaced (the prior payload survives in the
    /// command log so undo restores it). Pinned systems at `coord` refuse the
    /// operation.
    pub fn generate_system_here(
        &mut self,
        coord: sectorforge::sector_model::HexCoord,
        index: usize,
        seed_override: Option<&str>,
    ) -> Result<SystemId, BuilderError> {
        if coord.q < 0
            || coord.r < 0
            || (coord.q as u32) >= self.sector.width
            || (coord.r as u32) >= self.sector.height
        {
            return Err(BuilderError::ParseFailed {
                file: "generate-system-here".into(),
                message: format!(
                    "coord ({},{}) outside sector {}x{}",
                    coord.q, coord.r, self.sector.width, self.sector.height
                ),
            });
        }
        if let Some(existing) = self.sector.systems.iter().find(|s| s.coord == coord) {
            if self.pinned_systems.contains(&existing.id) {
                return Err(BuilderError::ParseFailed {
                    file: "generate-system-here".into(),
                    message: format!(
                        "system {} at ({},{}) is pinned",
                        existing.id, coord.q, coord.r
                    ),
                });
            }
        }
        let mut input =
            self.synthesize_project_input()
                .ok_or_else(|| BuilderError::ParseFailed {
                    file: "generate-system-here".into(),
                    message: "worlds catalog not loaded".into(),
                })?;
        if let Some(seed) = seed_override {
            input.config.generation.seed = seed.to_string();
        }
        let new_sys =
            sectorforge::generate_system_standalone(input, index, coord).map_err(|e| {
                BuilderError::ParseFailed {
                    file: "generate-system-here".into(),
                    message: e.to_string(),
                }
            })?;
        let id = new_sys.id.clone();
        let cmd = BuilderCommand::ReplaceSystem {
            coord,
            new_system: Box::new(new_sys),
            before: None,
        };
        self.run(cmd)?;
        Ok(id)
    }

    /// §W1 / §W4: locate a world by id, returning `(system_idx, world_idx)`.
    pub fn find_world_indices(&self, id: &WorldId) -> Option<(usize, usize)> {
        self.index.worlds.get(id).copied()
    }

    /// §W4: redraw the payload (`WorldDto`, source row, tags) for the given
    /// world against the in-memory pool, preserving id / name / orbit /
    /// factions / claims / control. Pinned worlds refuse the operation.
    pub fn regenerate_world(&mut self, id: &WorldId) -> Result<(), BuilderError> {
        use sectorforge::worlds::StarColour;
        if self.pinned_worlds.contains(id) {
            return Err(BuilderError::ParseFailed {
                file: "regenerate-world".into(),
                message: format!("world {id} is pinned"),
            });
        }
        let (sys_idx, w_idx) =
            self.find_world_indices(id)
                .ok_or_else(|| BuilderError::ParseFailed {
                    file: "regenerate-world".into(),
                    message: format!("world {id} not found"),
                })?;
        let star_code = self.sector.systems[sys_idx]
            .star
            .as_ref()
            .map(|s| s.colour_code.to_string())
            .unwrap_or_else(|| "G".into());
        let star_colour: StarColour = star_code.parse().unwrap_or(StarColour::Yellow);

        let input = self
            .synthesize_project_input()
            .ok_or_else(|| BuilderError::ParseFailed {
                file: "regenerate-world".into(),
                message: "worlds catalog not loaded".into(),
            })?;
        let mut pool = sectorforge::world_pool::build_pool(
            &input.world_rows,
            &input.world_tables,
            &input.config.generation.world_selection,
        );
        if let Some(features) = &input.authored_features {
            sectorforge::world_pool::apply_authored_features(&mut pool, features);
        }
        self.world_reroll_counter = self.world_reroll_counter.wrapping_add(1);
        let (dto, source_row, tags) = sectorforge::generation::regenerate_world_payload(
            &input.config,
            &pool,
            star_colour,
            id.as_str(),
            self.world_reroll_counter,
        )
        .map_err(|e| BuilderError::ParseFailed {
            file: "regenerate-world".into(),
            message: e.to_string(),
        })?;
        let w = &mut self.sector.systems[sys_idx].worlds[w_idx];
        w.world = dto;
        w.source_row_index = source_row;
        w.tags = tags;
        self.dirty = true;
        self.invariant_report = Some(sectorforge::invariants::check_sector(&self.sector));
        self.mark_validation_dirty();
        self.trigger_auto_save();
        Ok(())
    }

    /// §G2: derive the next seed. When [`Self::seed_locked`] is true, returns
    /// the current seed unchanged. Otherwise increments
    /// [`Self::seed_reroll_counter`] and returns the
    /// `blake3("sectorforge:{seed}:reroll:{n}")` digest.
    pub fn reroll_seed(&mut self) -> String {
        if self.seed_locked {
            return self.config.generation.seed.clone();
        }
        self.seed_reroll_counter = self.seed_reroll_counter.wrapping_add(1);
        let new_seed = super::super::preview::derive_reroll_seed(
            &self.config.generation.seed,
            self.seed_reroll_counter,
        );
        self.config.generation.seed = new_seed.clone();
        new_seed
    }

    /// §G4: promote the live preview to the active sector. Manual edits to
    /// systems flagged in [`Self::pinned_systems`] survive — their
    /// pre-preview snapshot replaces the regenerated entry at the same id.
    /// Returns `true` on success; `false` when no preview is ready.
    pub fn apply_preview(&mut self) -> bool {
        let Some(mut preview_sector) = self.preview.sector.take() else {
            return false;
        };
        if !self.pinned_systems.is_empty() {
            let pinned: BTreeMap<_, _> = self
                .sector
                .systems
                .iter()
                .filter(|s| self.pinned_systems.contains(&s.id))
                .map(|s| (s.id.clone(), s.clone()))
                .collect();
            if !pinned.is_empty() {
                let mut seen: BTreeSet<_> = BTreeSet::new();
                for sys in preview_sector.systems.iter_mut() {
                    if let Some(orig) = pinned.get(&sys.id) {
                        *sys = orig.clone();
                        seen.insert(sys.id.clone());
                    }
                }
                for (id, orig) in pinned.into_iter() {
                    if !seen.contains(&id) {
                        preview_sector.systems.push(orig);
                    }
                }
                preview_sector.systems.sort_by(|a, b| a.id.cmp(&b.id));
            }
        }
        self.sector = preview_sector;
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        self.dirty = true;
        self.invariant_report = Some(check_sector(&self.sector));
        self.mark_validation_dirty();
        self.preview.clear();
        true
    }

    /// §G5: regenerate only the systems whose coord falls inside
    /// [`Self::partial_regen_rect`]. Pinned systems are skipped. Each
    /// regenerated slot is rebuilt via
    /// [`sectorforge::generate_system_standalone`] so the rest of the sector
    /// stays untouched. Returns the number of systems regenerated, or an
    /// error when no rect is selected / no project input can be synthesised.
    pub fn regenerate_partial(&mut self) -> Result<usize, BuilderError> {
        let rect = self
            .partial_regen_rect
            .ok_or_else(|| BuilderError::ParseFailed {
                file: "partial-regen".into(),
                message: "no rectangle selected".into(),
            })?;
        let input = self
            .synthesize_project_input()
            .ok_or_else(|| BuilderError::ParseFailed {
                file: "partial-regen".into(),
                message: "project worlds catalog is not loaded".into(),
            })?;

        let mut regenerated = 0usize;
        let mut replacements: Vec<sectorforge::sector_model::GeneratedSystem> = Vec::new();
        for sys in &self.sector.systems {
            if !rect.contains(sys.coord) {
                continue;
            }
            if self.pinned_systems.contains(&sys.id) {
                continue;
            }
            let new_sys =
                sectorforge::generate_system_standalone(input.clone(), sys.index, sys.coord)
                    .map_err(|e| BuilderError::ParseFailed {
                        file: format!("partial-regen:{}", sys.id),
                        message: e.to_string(),
                    })?;
            replacements.push(new_sys);
            regenerated += 1;
        }
        for new_sys in replacements {
            if let Some(slot) = self.sector.systems.iter_mut().find(|s| s.id == new_sys.id) {
                *slot = new_sys;
            }
        }
        self.sector.systems.sort_by(|a, b| a.id.cmp(&b.id));
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        if regenerated > 0 {
            self.dirty = true;
            self.invariant_report = Some(check_sector(&self.sector));
            self.mark_validation_dirty();
            self.trigger_auto_save();
        }
        Ok(regenerated)
    }
}
