//! Derived-state recomputation + health reporting on
//! [`super::BuilderState`]: economy / relations / chronicle re-derives,
//! debounced validation, and the synthetic `ProjectInput` builder used by
//! both validation and per-system regeneration.

use std::collections::BTreeMap;
use std::time::Instant;

use camino::Utf8PathBuf;

use sectorforge::input::ProjectInput;
use sectorforge::invariants::check_sector;
use sectorforge::validation::validate;

use super::types::HealthLevel;
use super::BuilderState;

impl BuilderState {
    /// §V3: arm the debounced live-validation timer. Cheap — just stamps
    /// `Instant::now`. The actual `validate(&project)` call happens in
    /// [`Self::pump_validation`] after `validation_debounce` elapses.
    pub fn mark_validation_dirty(&mut self) {
        self.validation_dirty_since = Some(Instant::now());
    }

    /// §E1..§E4 — run `economy::derive_with` against the live sector using the
    /// in-memory `data_catalogs.economy` (defaulting to an enabled, empty
    /// config when none is loaded). Per-world overrides on
    /// [`Self::world_economy_overrides`] / [`Self::world_strategic_overrides`]
    /// pin those fields on top of the derived report, the system rollups are
    /// recomputed from the patched worlds, and per-system tithe / supply /
    /// priority overrides are applied last. When `cfg.feed_stability` is on
    /// the §E4 stranded nudge is re-applied to the sector. The fresh report is
    /// installed at `sector.economy` (as an `Arc`) and invariant / validation
    /// dirty markers are armed.
    pub fn recompute_economy(&mut self) {
        use sectorforge::economy::{
            apply_stability_nudge, derive_with, ResourceVector, RESOURCE_KEYS,
        };
        let mut cfg = self.data_catalogs.economy.clone().unwrap_or_default();
        cfg.enabled = true;
        let mut report = derive_with(&self.sector, &cfg);

        let mut sys_idx: BTreeMap<sectorforge::ids::SystemId, usize> = BTreeMap::new();
        for (i, s) in report.systems.iter().enumerate() {
            sys_idx.insert(s.system_id.clone(), i);
        }

        // §E1 / §E2 — pin per-world overrides on top of the freshly derived
        // report so the user's signed sliders win against the table defaults.
        for w in report.worlds.iter_mut() {
            if let Some(v) = self.world_economy_overrides.get(&w.world_id) {
                w.vector = v.clone();
                w.shortages = RESOURCE_KEYS
                    .iter()
                    .filter(|k| v.get(k) <= -20.0)
                    .map(|k| (*k).to_string())
                    .collect();
            }
            if let Some(s) = self.world_strategic_overrides.get(&w.world_id) {
                w.strategic_output = *s;
            }
        }

        // Recompute per-system aggregates from the patched per-world rows so
        // §E1 / §E2 pins propagate into system surplus / shortage / strategic
        // priority without re-walking the heavy derivation graph.
        let mut by_system: BTreeMap<sectorforge::ids::SystemId, ResourceVector> = BTreeMap::new();
        let mut strat_by_system: BTreeMap<
            sectorforge::ids::SystemId,
            sectorforge::economy::StrategicOutput,
        > = BTreeMap::new();
        for w in &report.worlds {
            let v = by_system.entry(w.system_id.clone()).or_default();
            v.ore += w.vector.ore;
            v.promethium += w.vector.promethium;
            v.foodstuffs += w.vector.foodstuffs;
            v.manufactured += w.vector.manufactured;
            v.archeotech += w.vector.archeotech;
            v.recruits += w.vector.recruits;
            let s = strat_by_system.entry(w.system_id.clone()).or_default();
            s.food += w.strategic_output.food;
            s.ore += w.strategic_output.ore;
            s.manufacturing += w.strategic_output.manufacturing;
            s.arms += w.strategic_output.arms;
            s.ships += w.strategic_output.ships;
            s.pilgrimage += w.strategic_output.pilgrimage;
            s.psyker_tithe += w.strategic_output.psyker_tithe;
            s.manpower += w.strategic_output.manpower;
            s.knowledge += w.strategic_output.knowledge;
            s.xenos_value += w.strategic_output.xenos_value;
        }
        for sy in report.systems.iter_mut() {
            if let Some(v) = by_system.get(&sy.system_id) {
                sy.vector = v.clone();
                sy.surplus_resources = RESOURCE_KEYS
                    .iter()
                    .filter(|k| v.get(k) >= 20.0)
                    .map(|k| (*k).to_string())
                    .collect();
                sy.shortage_resources = RESOURCE_KEYS
                    .iter()
                    .filter(|k| v.get(k) <= -20.0)
                    .map(|k| (*k).to_string())
                    .collect();
            }
            if let Some(s) = strat_by_system.get(&sy.system_id) {
                sy.strategic_output = *s;
            }
        }

        // §E3 — per-system overrides on tithe / supply / priority.
        for sy in report.systems.iter_mut() {
            if let Some(t) = self.system_tithe_overrides.get(&sy.system_id) {
                sy.tithe_status = *t;
            }
            if let Some(r) = self.system_supply_overrides.get(&sy.system_id) {
                sy.supply_risk = *r;
            }
            if let Some(p) = self.system_priority_overrides.get(&sy.system_id) {
                sy.strategic_priority = *p;
            }
        }

        // Refresh sector-level balance + strategic totals from the patched rows.
        let mut sector_balance = ResourceVector::default();
        let mut strategic = sectorforge::economy::StrategicOutput::default();
        for sy in &report.systems {
            sector_balance.ore += sy.vector.ore;
            sector_balance.promethium += sy.vector.promethium;
            sector_balance.foodstuffs += sy.vector.foodstuffs;
            sector_balance.manufactured += sy.vector.manufactured;
            sector_balance.archeotech += sy.vector.archeotech;
            sector_balance.recruits += sy.vector.recruits;
            strategic.food += sy.strategic_output.food;
            strategic.ore += sy.strategic_output.ore;
            strategic.manufacturing += sy.strategic_output.manufacturing;
            strategic.arms += sy.strategic_output.arms;
            strategic.ships += sy.strategic_output.ships;
            strategic.pilgrimage += sy.strategic_output.pilgrimage;
            strategic.psyker_tithe += sy.strategic_output.psyker_tithe;
            strategic.manpower += sy.strategic_output.manpower;
            strategic.knowledge += sy.strategic_output.knowledge;
            strategic.xenos_value += sy.strategic_output.xenos_value;
        }
        report.sector_balance = sector_balance;
        report.strategic_output = strategic;

        let feed_stability = cfg.feed_stability;
        self.sector.economy = std::sync::Arc::new(report);
        if feed_stability {
            let report = self.sector.economy.as_ref().clone();
            apply_stability_nudge(&report, &mut self.sector);
        }
        let _ = sys_idx;
        self.dirty = true;
        self.invariant_report = Some(check_sector(&self.sector));
        self.mark_validation_dirty();
        self.trigger_auto_save();
    }

    /// §REL9: recompute the inter-faction diplomacy matrix from the in-memory
    /// relations catalog and the live `sector.factions` / `sector.systems`
    /// rows, then publish the result onto `sector.relations`.
    ///
    /// Honours the §REL8 `[generation.relations].min_world_presence` knob so
    /// the matrix size matches what a fresh `cargo run -- generate` would
    /// emit. `feed_conflict` is copied through from the catalog (or defaulted
    /// off when the catalog is absent) so [`sectorforge::conflict::advance_sector`]
    /// can re-read it on the next tick without a reload.
    pub fn recompute_relations(&mut self) {
        let cfg = self.data_catalogs.relations.clone().unwrap_or_default();
        let threshold = self.config.generation.relations.min_world_presence;
        let matrix = sectorforge::relations::derive_with_threshold(&self.sector, &cfg, threshold);
        self.sector.relations = std::sync::Arc::new(matrix);
        self.dirty = true;
        self.mark_validation_dirty();
        self.trigger_auto_save();
    }

    /// §H6: rebuild `sector.chronicle` from the in-memory history catalog
    /// while preserving every event flagged `manual = true`. Steps:
    ///  1. Drain the existing chronicle and split derived vs. manual events.
    ///  2. Run [`sectorforge::history::derive_with`] against the live sector
    ///     using the configured catalog (or `HistoryConfig::default()` when
    ///     none is loaded).
    ///  3. Append every preserved manual event back onto the report and
    ///     re-sort by `date` so the chronological view stays correct.
    pub fn recompute_chronicle(&mut self) {
        let cfg = self.data_catalogs.history.clone().unwrap_or_default();
        let manual: Vec<sectorforge::history::HistoryEvent> = self
            .sector
            .chronicle
            .events
            .iter()
            .filter(|e| e.manual)
            .cloned()
            .collect();
        let mut report = sectorforge::history::derive_with(&self.sector, &cfg);
        report.events.extend(manual);
        report
            .events
            .sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
        self.sector.chronicle = report;
        self.dirty = true;
        self.mark_validation_dirty();
        self.trigger_auto_save();
    }

    /// §PER1..§PER5 — rebuild [`sectorforge::personae::PersonaeReport`] from
    /// the live sector + the in-memory `data_catalogs.personae` (falling back
    /// to defaults when no catalog is loaded). The result is stashed on
    /// [`Self::personae_report`] so the PERSONAE tab can render without
    /// re-running the derivation each frame.
    ///
    /// Manual personae embedded in the catalog (`PersonaeConfig::manual`) are
    /// appended by `derive_with` itself, so the recompute path automatically
    /// preserves them across regenerates.
    pub fn recompute_personae(&mut self) {
        let cfg = self.data_catalogs.personae.clone().unwrap_or_default();
        let report = sectorforge::personae::derive_with(&self.sector, &cfg);
        self.personae_report = Some(report);
        self.mark_validation_dirty();
    }

    /// §HK1..§HK6 — rebuild [`sectorforge::hooks::HooksReport`] from the live
    /// sector + the in-memory `data_catalogs.hooks` (falling back to defaults
    /// when no catalog is loaded). The §HK5 player-edition toggle is folded
    /// into the working config so the cached report already has GM-only rows
    /// stripped when on. The result is stashed on [`Self::hooks_report`] so
    /// the HOOKS tab can render without re-running the derivation each frame.
    ///
    /// Manual hooks embedded in the catalog (`HooksConfig::manual`) are
    /// appended by `derive_with` itself, so the recompute path automatically
    /// preserves them across regenerates.
    pub fn recompute_hooks(&mut self) {
        let mut cfg = self.data_catalogs.hooks.clone().unwrap_or_default();
        cfg.hide_hidden_hooks = self.hooks_player_edition;
        let report = sectorforge::hooks::derive_with(&self.sector, &cfg);
        self.hooks_report = Some(report);
        self.mark_validation_dirty();
    }

    /// §ST1..§ST4 — rebuild [`sectorforge::sites::SitesReport`] from the live
    /// sector + the in-memory `data_catalogs.sites` (falling back to defaults
    /// when no catalog is loaded). The §ST3 player-edition toggle is folded
    /// into the working config so the cached report already has rows where
    /// `public_status != actual_status` stripped when on. The result is
    /// stashed on [`Self::sites_report`] so the SITES tab can render without
    /// re-running the derivation each frame.
    ///
    /// Manual sites embedded in the catalog (`SitesConfig::manual`) are
    /// appended by `derive_with` itself, so the recompute path automatically
    /// preserves them across regenerates.
    pub fn recompute_sites(&mut self) {
        let mut cfg = self.data_catalogs.sites.clone().unwrap_or_default();
        cfg.player_edition = self.sites_player_edition;
        let report = sectorforge::sites::derive_with(&self.sector, &cfg);
        self.sites_report = Some(report);
        self.mark_validation_dirty();
    }

    /// §M1..§M5 — rebuild [`sectorforge::missions::MissionsReport`] from the
    /// live sector + the in-memory `data_catalogs.missions` (falling back to
    /// defaults when no catalog is loaded). The §M4 player-edition toggle is
    /// folded into the working config so the cached report already has
    /// Hidden-tier-derived missions stripped when on. The result is stashed
    /// on [`Self::missions_report`] so the MISSIONS tab can render without
    /// re-running the derivation each frame.
    ///
    /// Manual missions embedded in the catalog (`MissionsConfig::manual`) are
    /// appended by `derive_with` itself after the per-anchor cap, so the
    /// recompute path automatically preserves them across regenerates.
    pub fn recompute_missions(&mut self) {
        let mut cfg = self.data_catalogs.missions.clone().unwrap_or_default();
        cfg.player_edition = self.missions_player_edition;
        let report = sectorforge::missions::derive_with(&self.sector, &cfg);
        self.missions_report = Some(report);
        self.mark_validation_dirty();
    }

    /// §V3: per-frame poll from the UI. When the debounce window has elapsed
    /// since the last mutation, build a synthetic [`ProjectInput`] from the
    /// in-memory catalogs and run [`validate`] against it. Returns `true`
    /// when a fresh report was produced this tick so the caller can request a
    /// repaint.
    pub fn pump_validation(&mut self) -> bool {
        let Some(since) = self.validation_dirty_since else {
            return false;
        };
        if since.elapsed() < self.validation_debounce {
            return false;
        }
        self.revalidate_now();
        true
    }

    /// §V3: synchronous re-validation. Clears the debounce timer regardless
    /// of whether catalogs were complete enough to build a `ProjectInput` —
    /// otherwise an incomplete catalog would re-arm every tick.
    pub fn revalidate_now(&mut self) {
        self.validation_dirty_since = None;
        if let Some(input) = self.synthesize_project_input() {
            self.validation_report = Some(validate(&input));
        }
    }

    /// §V3: build a read-only [`ProjectInput`] from the in-memory catalogs so
    /// [`sectorforge::validation::validate`] can run without touching disk.
    /// Returns `None` when the worlds catalog is missing — validation needs at
    /// minimum a workbook to walk.
    pub fn synthesize_project_input(&self) -> Option<ProjectInput> {
        let worlds = self.data_catalogs.worlds.as_ref()?;
        let (world_tables, world_rows) = worlds.to_loader_inputs();
        let authored_features = worlds.resolved_features().ok();
        let root_dir = self
            .project_path
            .clone()
            .unwrap_or_else(|| Utf8PathBuf::from("."));
        Some(ProjectInput {
            root_dir,
            config: self.config.clone(),
            world_tables,
            world_rows,
            authored_features,
            names: self.data_catalogs.names.clone().unwrap_or_default(),
            factions: self
                .data_catalogs
                .factions
                .as_ref()
                .map(|f| f.factions.clone())
                .unwrap_or_default(),
            route_rules: self.data_catalogs.route_rules.clone().unwrap_or_default(),
            relations: self.data_catalogs.relations.clone().unwrap_or_default(),
            regions: self.data_catalogs.regions.clone().unwrap_or_default(),
            economy: self.data_catalogs.economy.clone().unwrap_or_default(),
            history: self.data_catalogs.history.clone().unwrap_or_default(),
            personae: self.data_catalogs.personae.clone().unwrap_or_default(),
            sites: self.data_catalogs.sites.clone().unwrap_or_default(),
            hooks: self.data_catalogs.hooks.clone().unwrap_or_default(),
            missions: self.data_catalogs.missions.clone().unwrap_or_default(),
            input_digests: BTreeMap::new(),
        })
    }

    /// §V3: derive the status-bar health pip from validation + invariants.
    /// Red — any validation error or invariant violation. Yellow — warnings or
    /// no report yet. Green — both clean.
    pub fn health_level(&self) -> HealthLevel {
        let v_has_err = self
            .validation_report
            .as_ref()
            .is_some_and(|r| !r.errors.is_empty());
        let inv_has_violation = self
            .invariant_report
            .as_ref()
            .is_some_and(|r| !r.violations.is_empty());
        if v_has_err || inv_has_violation {
            return HealthLevel::Red;
        }
        let v_has_warn = self
            .validation_report
            .as_ref()
            .is_some_and(|r| !r.warnings.is_empty());
        let v_missing = self.validation_report.is_none();
        let inv_missing = self.invariant_report.is_none();
        if v_has_warn || v_missing || inv_missing {
            return HealthLevel::Yellow;
        }
        HealthLevel::Green
    }

    /// §CF4 / §CF5: run `BuilderCommand::AdvanceConflictTicks` for `ticks` and
    /// append per-system + per-world diff rows to [`Self::tick_log`]. Diffs
    /// only land in the log when a momentum, intensity, defender, or visible
    /// controller field actually changed — pristine entities stay quiet.
    pub fn advance_conflict_ticks(
        &mut self,
        ticks: u32,
    ) -> Result<(), crate::builder::errors::BuilderError> {
        use super::types::{TickLogEntry, TickLogScope};
        use crate::builder::command::BuilderCommand;
        if ticks == 0 {
            return Ok(());
        }
        let cmd = BuilderCommand::AdvanceConflictTicks {
            ticks,
            before_world: Vec::new(),
            before_system: Vec::new(),
            before_dominant: Vec::new(),
        };
        self.run(cmd)?;
        let last = self
            .command_log
            .get(self.command_cursor.saturating_sub(1))
            .cloned();
        let Some(BuilderCommand::AdvanceConflictTicks {
            before_world,
            before_system,
            ..
        }) = last
        else {
            return Ok(());
        };
        let next_index = self.tick_log.back().map(|e| e.tick_index + 1).unwrap_or(0);
        let mut sys_lookup: BTreeMap<sectorforge::ids::WorldId, sectorforge::ids::SystemId> =
            BTreeMap::new();
        for sys in &self.sector.systems {
            for w in &sys.worlds {
                sys_lookup.insert(w.id.clone(), sys.id.clone());
            }
        }
        for (sys_id, before) in &before_system {
            let Some(sys) = self.sector.systems.iter().find(|s| s.id == *sys_id) else {
                continue;
            };
            let after = &sys.conflict;
            if before == after {
                continue;
            }
            self.push_tick_entry(TickLogEntry {
                tick_index: next_index,
                scope: TickLogScope::System(sys_id.clone()),
                momentum_before: before.momentum,
                momentum_after: after.momentum,
                intensity_before: before.intensity,
                intensity_after: after.intensity,
                defender_before: before.defender.clone(),
                defender_after: after.defender.clone(),
                visible_before: before.visible_controller.clone(),
                visible_after: after.visible_controller.clone(),
            });
        }
        for (world_id, before) in &before_world {
            let Some(sys_id) = sys_lookup.get(world_id).cloned() else {
                continue;
            };
            let Some(world) = self
                .sector
                .systems
                .iter()
                .flat_map(|s| s.worlds.iter())
                .find(|w| w.id == *world_id)
            else {
                continue;
            };
            let after = &world.conflict;
            if before == after {
                continue;
            }
            self.push_tick_entry(TickLogEntry {
                tick_index: next_index,
                scope: TickLogScope::World {
                    system: sys_id,
                    world: world_id.clone(),
                },
                momentum_before: before.momentum,
                momentum_after: after.momentum,
                intensity_before: before.intensity,
                intensity_after: after.intensity,
                defender_before: before.defender.clone(),
                defender_after: after.defender.clone(),
                visible_before: before.visible_controller.clone(),
                visible_after: after.visible_controller.clone(),
            });
        }
        Ok(())
    }

    fn push_tick_entry(&mut self, entry: super::types::TickLogEntry) {
        if self.tick_log.len() >= self.tick_log_capacity {
            self.tick_log.pop_front();
        }
        self.tick_log.push_back(entry);
    }
}
