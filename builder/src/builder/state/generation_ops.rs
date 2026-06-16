//! Per-system / per-world / preview / partial-regen helpers on
//! [`super::BuilderState`]. §G2..§G5 + §S5 + §W4 wiring.

use std::collections::{BTreeMap, BTreeSet};

use sectorforge::ids::{SystemId, WorldId};
use sectorforge::invariants::check_sector;
use sectorforge::sector_model::{GeneratedSystem, GeneratedWorld};
use sectorforge::MutationError;
use sectorforge::SectorError;

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
        let preserved_name =
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
                Some(existing.name.clone())
            } else {
                None
            };
        let mut input =
            self.synthesize_project_input()
                .ok_or_else(|| BuilderError::ParseFailed {
                    file: "generate-system-here".into(),
                    message: "worlds catalog not loaded".into(),
                })?;
        if let Some(seed) = seed_override {
            input.config.generation.seed = seed.to_string();
        }
        let mut new_sys =
            sectorforge::generate_system_standalone(input, index, coord).map_err(|e| match e {
                SectorError::NoWorldCandidates => BuilderError::ParseFailed {
                    file: "data/worlds/worlds.toml".into(),
                    message: "world workbook has no usable generation rows — add `[[generation]]` \
                         entries with star_colour/world_type/atmosphere/temperature/biosphere/\
                         population/tech/government/weight, or scaffold from a preset that \
                         already includes them"
                        .into(),
                },
                other => BuilderError::ParseFailed {
                    file: "generate-system-here".into(),
                    message: other.to_string(),
                },
            })?;
        if let Some(name) = preserved_name {
            new_sys.name = name;
        }
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

    /// §E-S3: clone the world `world`, run `f` on the clone, and commit it via
    /// [`BuilderCommand::EditWorld`] (`before: None` — the bus captures the
    /// prior payload on apply, so undo/redo are exact). This collapses the
    /// clone-mutate-dispatch idiom the WORLD / CONTROL panels repeat: a caller
    /// writes `state.edit_world(wid, |w| { … })?` instead of cloning by hand,
    /// boxing the draft, and spelling `before: None`.
    ///
    /// The bus error is **returned**, not surfaced — each call site keeps its
    /// own `ModalKind::Message` text (they differ: "Edit failed" vs "World edit
    /// failed" vs …), so this helper never touches `self.feedback.modal`. A stale id maps
    /// to [`MutationError::WorldNotFound`], matching what `run(EditWorld)` would
    /// itself return for a missing world.
    pub fn edit_world(
        &mut self,
        world: WorldId,
        f: impl FnOnce(&mut GeneratedWorld),
    ) -> Result<(), BuilderError> {
        let (si, wi) = self
            .find_world_indices(&world)
            .ok_or_else(|| MutationError::WorldNotFound(world.to_string()))?;
        let mut draft = self.sector.systems[si].worlds[wi].clone();
        f(&mut draft);
        self.run(BuilderCommand::EditWorld {
            world,
            before: None,
            after: Box::new(draft),
        })
    }

    /// §E-S3: system counterpart to [`Self::edit_world`]. Clones system
    /// `system`, runs `f` on the clone, and commits via
    /// [`BuilderCommand::EditSystem`] (`before: None`; the `worlds` vector rides
    /// through the clone unchanged — only system-scope fields are meant to be
    /// touched). The bus error is returned so each caller keeps its own modal
    /// text; a stale id maps to [`MutationError::SystemNotFound`].
    pub fn edit_system(
        &mut self,
        system: SystemId,
        f: impl FnOnce(&mut GeneratedSystem),
    ) -> Result<(), BuilderError> {
        let si = self
            .system_index_by_id(&system)
            .ok_or_else(|| MutationError::SystemNotFound(system.to_string()))?;
        let mut draft = self.sector.systems[si].clone();
        f(&mut draft);
        self.run(BuilderCommand::EditSystem {
            system,
            before: None,
            after: Box::new(draft),
        })
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
        // `w_idx` is intentionally unused: the world index is now resolved
        // again inside `edit_world` (via `find_world_indices`) when the payload
        // is committed through the bus. Only `sys_idx` is needed here, to read
        // the host system's star colour for the reroll pool.
        let (sys_idx, _w_idx) =
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
        let mut pool = sectorforge::build_pool(
            &input.catalogs.world_rows,
            &input.catalogs.world_tables,
            &input.config.generation.world_selection,
        );
        if let Some(features) = &input.catalogs.authored_features {
            sectorforge::apply_authored_features(&mut pool, features);
        }
        self.generation.world_reroll_counter = self.generation.world_reroll_counter.wrapping_add(1);
        let (dto, source_row, tags) = sectorforge::regenerate_world_payload(
            &input.config,
            &pool,
            star_colour,
            id.as_str(),
            self.generation.world_reroll_counter,
        )
        .map_err(|e| BuilderError::ParseFailed {
            file: "regenerate-world".into(),
            message: e.to_string(),
        })?;
        // §R4: route the redrawn payload through the command bus so the reroll
        // lands on the undo/redo log. `edit_world` clones the live world,
        // applies the mutation, and dispatches `BuilderCommand::EditWorld`
        // (`before: None`, captured on apply); the bus already sets `dirty`,
        // marks validation dirty, and triggers auto-save — so no manual
        // `sector_mut` write / dirty / mark_validation_dirty / trigger_auto_save
        // is needed here.
        self.edit_world(id.clone(), |w| {
            w.world = dto;
            w.source_row_index = source_row;
            w.tags = tags;
        })
    }

    /// §G2: derive the next seed. When `generation.seed_locked` is true, returns
    /// the current seed unchanged. Otherwise increments
    /// `generation.seed_reroll_counter` and returns the
    /// `blake3("sectorforge:{seed}:reroll:{n}")` digest.
    pub fn reroll_seed(&mut self) -> String {
        if self.generation.seed_locked {
            return self.config.generation.seed.clone();
        }
        self.generation.seed_reroll_counter = self.generation.seed_reroll_counter.wrapping_add(1);
        let new_seed = super::super::preview::derive_reroll_seed(
            &self.config.generation.seed,
            self.generation.seed_reroll_counter,
        );
        self.config.generation.seed = new_seed.clone();
        new_seed
    }

    /// §G4: promote the live preview to the active sector. Manual edits to
    /// systems flagged in [`Self::pinned_systems`] survive — their
    /// pre-preview snapshot replaces the regenerated entry at the same id.
    /// Returns `true` on success; `false` when no preview is ready.
    pub fn apply_preview(&mut self) -> bool {
        let Some(mut preview_sector) = self.generation.preview.sector.take() else {
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
        self.sector = preview_sector.into();
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        // Full sector swap — every previously-derived overlay is now stale.
        self.derivations.invalidate_all();
        self.dirty = true;
        self.invariant_report = Some(check_sector(&self.sector));
        self.mark_validation_dirty();
        // The undo/redo log and snapshots described the *pre-swap* sector. After
        // a wholesale sector replacement those commands would replay against an
        // unrelated document (phantom no-op reverts), so the history is reset:
        // clear the command log, rewind the cursor to 0, and drop all snapshots.
        self.command_log.clear();
        self.command_cursor = 0;
        self.snapshots.clear();
        self.generation.preview.clear();
        true
    }

    /// §G5: regenerate only the systems whose coord falls inside
    /// the `map_view.partial_regen_rect`. Pinned systems are skipped. Each
    /// regenerated slot is rebuilt via
    /// [`sectorforge::generate_system_standalone`] so the rest of the sector
    /// stays untouched. Returns the number of systems regenerated, or an
    /// error when no rect is selected / no project input can be synthesised.
    pub fn regenerate_partial(&mut self) -> Result<usize, BuilderError> {
        let rect = self
            .map_view
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
            self.derivations.invalidate_all();
            self.dirty = true;
            self.invariant_report = Some(check_sector(&self.sector));
            self.mark_validation_dirty();
            self.trigger_auto_save();
        }
        Ok(regenerated)
    }

    /// §SR3: apply a seed found by the SEARCH tab. Pins `generation.seed` to
    /// `seed`, runs a full synchronous regenerate via
    /// [`sectorforge::generate_sector`], and replaces the working sector
    /// with the result.
    ///
    /// `commit == true` is the "Apply" action — it marks the project dirty and
    /// triggers auto-save. `commit == false` is the non-destructive "View on
    /// map" action — the sector is swapped in so it renders, but the project is
    /// left clean. Either way the index, derivation cache, and invariant report
    /// are refreshed. A search-found seed describes a *fresh* sector, so pinned
    /// systems are intentionally not preserved here (unlike §G4 apply-preview).
    pub fn apply_search_seed(&mut self, seed: &str, commit: bool) -> Result<(), BuilderError> {
        let mut input =
            self.synthesize_project_input()
                .ok_or_else(|| BuilderError::ParseFailed {
                    file: "apply-search-seed".into(),
                    message: "worlds catalog not loaded".into(),
                })?;
        input.config.generation.seed = seed.to_string();
        let sector =
            sectorforge::generate_sector(input).map_err(|e| BuilderError::ParseFailed {
                file: "apply-search-seed".into(),
                message: e.to_string(),
            })?;
        self.config.generation.seed = seed.to_string();
        self.sector = sector.into();
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        self.derivations.invalidate_all();
        self.invariant_report = Some(check_sector(&self.sector));
        self.mark_validation_dirty();
        // The undo/redo log and snapshots described the sector that was just
        // replaced wholesale. Whether or not this swap is committed, those
        // commands would now replay against an unrelated document, so the
        // history is reset: clear the command log, rewind the cursor to 0, and
        // drop all snapshots.
        self.command_log.clear();
        self.command_cursor = 0;
        self.snapshots.clear();
        if commit {
            self.dirty = true;
            self.trigger_auto_save();
        }
        Ok(())
    }
}
