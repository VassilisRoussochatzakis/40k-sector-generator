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

use super::types::{BuilderTab, HealthLevel};
use super::BuilderState;
use crate::builder::derivation_cache::{digest_input, DepClass, DerivationKind, DerivationStatus};

impl BuilderState {
    /// §V3: arm the debounced live-validation timer. Cheap — just stamps
    /// `Instant::now`. The actual `validate(&project)` call happens in
    /// [`Self::pump_validation`] after `validation_debounce` elapses.
    pub fn mark_validation_dirty(&mut self) {
        self.feedback.validation_dirty_since = Some(Instant::now());
    }

    /// Mark the session dirty and flag the catalog file that backs it (§E13):
    /// the configured input path, or `default` when the project has not pinned
    /// one. Replaces the `if let Some(rel) = … { … } else { … default … }`
    /// boilerplate each catalog panel's `on_catalog_edited` hand-rolled.
    pub(crate) fn mark_catalog_dirty(&mut self, configured: Option<String>, default: &str) {
        self.dirty = true;
        self.dirty_files
            .insert(configured.unwrap_or_else(|| default.to_string()));
    }

    // ── §39 live derivations (LD1..LD4) ─────────────────────────────────────

    /// LD1 — BLAKE3 fingerprint of the input slice `kind` reads: the generator
    /// version, the kind's domain-separator key, the serialized sector slice
    /// for each [`DepClass`] in [`DerivationKind::deps`], and any per-kind
    /// catalog/config knobs. Two derivations that read the same slice never
    /// collide because the key is folded in. A stable fingerprint means the
    /// cached value is still valid (the precise half of LD2).
    pub fn derivation_fingerprint(&self, kind: DerivationKind) -> String {
        let mut parts: Vec<String> = vec![
            sectorforge::GENERATOR_VERSION.to_string(),
            kind.key().to_string(),
        ];
        for dep in kind.deps() {
            parts.push(match dep {
                DepClass::SystemsWorlds => digest_input(&self.sector.systems),
                DepClass::Factions => digest_input(&self.sector.factions),
                DepClass::Regions => digest_input(&self.sector.regions),
                DepClass::Routes => digest_input(&self.sector.routes),
                DepClass::RelationsCfg => digest_input(&(
                    &self.data_catalogs.relations,
                    self.config.generation.relations.min_world_presence,
                )),
                DepClass::EconomyCfg => digest_input(&(
                    &self.data_catalogs.economy,
                    &self.world_economy_overrides,
                    &self.world_strategic_overrides,
                    &self.system_tithe_overrides,
                    &self.system_supply_overrides,
                    &self.system_priority_overrides,
                )),
            });
        }
        parts.push(self.derivation_config_digest(kind));
        digest_input(&parts)
    }

    /// Per-kind catalog / toggle knobs that sit outside the [`DepClass`] slices
    /// but still change the derived output (player-edition masks, the analytics
    /// `[analyze]` config, the briefing profile, …). Empty for kinds whose
    /// output is fully determined by their dependency slices.
    fn derivation_config_digest(&self, kind: DerivationKind) -> String {
        match kind {
            DerivationKind::Personae => digest_input(&self.data_catalogs.personae),
            DerivationKind::Hooks => {
                digest_input(&(&self.data_catalogs.hooks, self.hooks_panel.player_edition))
            }
            DerivationKind::Sites => {
                digest_input(&(&self.data_catalogs.sites, self.sites_panel.player_edition))
            }
            DerivationKind::Missions => digest_input(&(
                &self.data_catalogs.missions,
                self.missions_panel.player_edition,
            )),
            DerivationKind::Prose => digest_input(&self.data_catalogs.prose),
            DerivationKind::History => digest_input(&self.data_catalogs.history),
            DerivationKind::Analytics => digest_input(&self.analytics.config),
            DerivationKind::Briefing => digest_input(&(
                &self.briefing_panel.preset,
                &self.briefing_panel.observer,
                self.briefing_panel.min_confidence,
            )),
            DerivationKind::Interestingness => digest_input(&self.interestingness_panel.profile),
            _ => String::new(),
        }
    }

    /// LD2 — invalidate every derivation downstream of the given mutation
    /// classes. Called by the command bus (`run` / `undo` / `redo`) with
    /// `BuilderCommand::dep_classes`, and by catalog panels when they edit a
    /// `relations.toml` / `economy.toml` slice outside the command bus.
    pub fn invalidate_derivations(&mut self, classes: &[DepClass]) {
        self.derivations.invalidate(classes);
    }

    /// LD3/LD4 — record that `kind`'s cached value matches the current input.
    /// Panels and the `recompute_*` methods call this after (re)deriving so the
    /// ledger fingerprint tracks the value actually on display.
    pub fn mark_derivation_fresh(&mut self, kind: DerivationKind) {
        let fp = self.derivation_fingerprint(kind);
        self.derivations.mark_fresh(kind, fp);
    }

    /// LD3 — current freshness of `kind` for the status bar / panel stale tag.
    pub fn derivation_status(&self, kind: DerivationKind) -> DerivationStatus {
        let fp = self.derivation_fingerprint(kind);
        self.derivations.status(kind, &fp)
    }

    /// Map a top tab to the overlay derivation it renders, when that overlay is
    /// one the builder re-derives from a state-level `recompute_*` method. The
    /// user-triggered overlays (analytics / briefing / interestingness) own
    /// their config builders inside the panel and self-refresh there, so they
    /// are deliberately absent here.
    fn tab_auto_derivation(tab: BuilderTab) -> Option<DerivationKind> {
        Some(match tab {
            BuilderTab::Economy => DerivationKind::Economy,
            BuilderTab::Relations => DerivationKind::Relations,
            BuilderTab::History => DerivationKind::History,
            BuilderTab::Personae => DerivationKind::Personae,
            BuilderTab::Hooks => DerivationKind::Hooks,
            BuilderTab::Sites => DerivationKind::Sites,
            BuilderTab::Missions => DerivationKind::Missions,
            BuilderTab::Prose => DerivationKind::Prose,
            _ => return None,
        })
    }

    /// LD4 — re-derive `kind` if it is stale (a dependency changed since the
    /// last derive). Cold overlays are left untouched so the panel's explicit
    /// first-derive action still gates the initial computation; only overlays
    /// the user has already opened are kept live. The precise half of LD2 lives
    /// here too: a dependency-table hit whose fingerprint is unchanged clears
    /// the stale flag without recomputing.
    pub fn ensure_fresh(&mut self, kind: DerivationKind) {
        if !self.derivations.is_stale(kind) {
            return;
        }
        let current = self.derivation_fingerprint(kind);
        if self.derivations.fingerprints.get(&kind) == Some(&current) {
            self.derivations.mark_fresh(kind, current);
            return;
        }
        self.recompute_derivation(kind);
        self.mark_derivation_fresh(kind);
    }

    /// Dispatch a stale auto-derived overlay to its `recompute_*` method.
    /// Only the eight overlays with a state-level recompute are handled; the
    /// rest are panel-driven or computed live each frame.
    fn recompute_derivation(&mut self, kind: DerivationKind) {
        match kind {
            DerivationKind::Economy => self.recompute_economy(),
            DerivationKind::Relations => self.recompute_relations(),
            DerivationKind::History => self.recompute_chronicle(),
            DerivationKind::Personae => self.recompute_personae(),
            DerivationKind::Hooks => self.recompute_hooks(),
            DerivationKind::Sites => self.recompute_sites(),
            DerivationKind::Missions => self.recompute_missions(),
            DerivationKind::Prose => self.recompute_prose(),
            _ => {}
        }
    }

    /// LD3/LD4 — per-frame freshness pump, called from the app's
    /// `pump_active_state`. Lazily re-derives the overlay the active tab
    /// renders if a prior mutation marked it stale, so the panel always paints
    /// a live result. Off-tab overlays stay flagged stale (surfaced in the
    /// status bar) until visited; a future background thread (LD3) will refresh
    /// them ahead of time.
    pub fn pump_derivations(&mut self) {
        if let Some(kind) = Self::tab_auto_derivation(self.active_tab) {
            self.ensure_fresh(kind);
        }
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
        // D6: this install is *not* per-frame — every caller reaches here via
        // `ensure_fresh`, which gates on the LD1/LD2 BLAKE3 fingerprint and
        // only recomputes when an economy dependency actually changed.
        self.sector.economy = std::sync::Arc::new(report);
        if feed_stability {
            let report = self.sector.economy.as_ref().clone();
            apply_stability_nudge(&report, self.sector_mut());
        }
        self.dirty = true;
        self.invariant_report = Some(check_sector(&self.sector));
        self.mark_validation_dirty();
        self.trigger_auto_save();
        // §E4 feed-stability nudges per-world stability — a systems/worlds
        // input change — so every other overlay reading stability is now stale.
        if feed_stability {
            self.derivations.invalidate(&[DepClass::SystemsWorlds]);
        }
        // §39 table: the economy config also feeds hooks (lifeline / starving),
        // so an economy-catalog edit (which routes here) leaves hooks stale.
        self.derivations.invalidate(&[DepClass::EconomyCfg]);
        self.mark_derivation_fresh(DerivationKind::Economy);
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
        // §39 table: the relations config also feeds the briefing pack, so a
        // relations-catalog edit (which routes here) leaves briefing stale.
        self.derivations.invalidate(&[DepClass::RelationsCfg]);
        self.mark_derivation_fresh(DerivationKind::Relations);
    }

    /// §H6 helper: derive a fresh chronicle from the live sector + history
    /// catalog while preserving every `manual = true` event. Pure — installs
    /// nothing — so the passive ([`Self::recompute_chronicle`]) and undoable
    /// ([`Self::recompute_chronicle_undoable`]) paths share one body.
    fn compute_chronicle(&self) -> sectorforge::history::SectorChronicle {
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
        report
    }

    /// §H6 passive LD4 refresh: rebuild `sector.chronicle` in place when the
    /// History overlay is viewed stale (driven by [`Self::pump_derivations`]).
    /// Off-bus by design — like the sibling `recompute_*` derivations
    /// (economy / relations / …), a lazy display refresh is not itself an undo
    /// step, so merely viewing the tab never evicts the redo tail. Manual
    /// events are always preserved, so it loses no user-authored history.
    ///
    /// D2 / D11 precedence: the user-initiated "Regenerate chronicle" button
    /// and the `history_auto_recompute` catalog-edit trigger go through
    /// [`Self::recompute_chronicle_undoable`] instead, so those recomputes land
    /// on the undo stack and cannot silently diverge from a prior
    /// `EditChronicle` snapshot. This passive refresh is the only off-bus
    /// chronicle writer left; it preserves manual events and self-heals on the
    /// next pump, so it carries no data-loss risk.
    pub fn recompute_chronicle(&mut self) {
        self.sector.chronicle = self.compute_chronicle();
        self.dirty = true;
        self.mark_validation_dirty();
        self.trigger_auto_save();
        self.mark_derivation_fresh(DerivationKind::History);
    }

    /// §H6 / D2 / D11: user-initiated chronicle regenerate, routed through the
    /// command bus as an `EditChronicle` so the recompute is undoable. Used by
    /// the "Regenerate chronicle" button and the `history_auto_recompute`
    /// catalog-edit trigger. Manual events are preserved (via
    /// [`Self::compute_chronicle`]); `run` captures the prior chronicle as the
    /// undo `before`, and the overlay is re-marked fresh afterwards.
    pub fn recompute_chronicle_undoable(
        &mut self,
    ) -> Result<(), crate::builder::errors::BuilderError> {
        use crate::builder::command::BuilderCommand;
        let after = self.compute_chronicle();
        self.run(BuilderCommand::EditChronicle {
            before: None,
            after: Box::new(after),
        })?;
        self.mark_derivation_fresh(DerivationKind::History);
        Ok(())
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
        self.mark_derivation_fresh(DerivationKind::Personae);
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
        cfg.hide_hidden_hooks = self.hooks_panel.player_edition;
        let report = sectorforge::hooks::derive_with(&self.sector, &cfg);
        self.hooks_report = Some(report);
        self.mark_validation_dirty();
        self.mark_derivation_fresh(DerivationKind::Hooks);
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
        cfg.player_edition = self.sites_panel.player_edition;
        let report = sectorforge::sites::derive_with(&self.sector, &cfg);
        self.sites_report = Some(report);
        self.mark_validation_dirty();
        self.mark_derivation_fresh(DerivationKind::Sites);
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
        cfg.player_edition = self.missions_panel.player_edition;
        let report = sectorforge::missions::derive_with(&self.sector, &cfg);
        self.missions_report = Some(report);
        self.mark_validation_dirty();
        self.mark_derivation_fresh(DerivationKind::Missions);
    }

    /// §PR1..§PR4 — rebuild [`sectorforge::prose::ProseReport`] from the live
    /// sector + the in-memory `data_catalogs.prose` (falling back to defaults
    /// when no catalog is loaded). The result is stashed on
    /// [`Self::prose_report`] so the PROSE tab can render without re-running
    /// the derivation each frame.
    ///
    /// Manual overrides embedded in the catalog (`ProseConfig::overrides`)
    /// are applied by `derive_with` itself after the deterministic
    /// derivation, so the recompute path automatically preserves them across
    /// regenerates.
    pub fn recompute_prose(&mut self) {
        let cfg = self.data_catalogs.prose.clone().unwrap_or_default();
        let report = sectorforge::prose::derive_with(&self.sector, &cfg);
        self.prose_report = Some(report);
        self.mark_validation_dirty();
        self.mark_derivation_fresh(DerivationKind::Prose);
    }

    /// §V3: per-frame poll from the UI. When the debounce window has elapsed
    /// since the last mutation, build a synthetic [`ProjectInput`] from the
    /// in-memory catalogs and run [`validate`] against it. Returns `true`
    /// when a fresh report was produced this tick so the caller can request a
    /// repaint.
    pub fn pump_validation(&mut self) -> bool {
        let Some(since) = self.feedback.validation_dirty_since else {
            return false;
        };
        if since.elapsed() < self.feedback.validation_debounce {
            return false;
        }
        self.revalidate_now();
        true
    }

    /// §V3: synchronous re-validation. Clears the debounce timer regardless
    /// of whether catalogs were complete enough to build a `ProjectInput` —
    /// otherwise an incomplete catalog would re-arm every tick.
    pub fn revalidate_now(&mut self) {
        self.feedback.validation_dirty_since = None;
        if let Some(input) = self.synthesize_project_input() {
            self.validation_report = Some(validate(&input));
            self.feedback.last_validation_skip_reason = None;
        } else {
            // D10: no worlds catalog → `validate` cannot run. Record why so the
            // status bar shows the skip instead of silently leaving the prior
            // (or empty) report in place looking like a clean pass.
            self.feedback.last_validation_skip_reason =
                Some("no worlds catalog loaded".to_string());
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
            catalogs: std::sync::Arc::new(sectorforge::ProjectCatalogs {
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
                prose: self.data_catalogs.prose.clone().unwrap_or_default(),
            }),
            input_digests: BTreeMap::new(),
        })
    }

    /// §V3 / §V4: derive the status-bar health pip from validation +
    /// invariants. Red — any validation error or invariant violation (and,
    /// under §V4 strict mode, any validation warning). Yellow — warnings or no
    /// report yet. Green — both clean.
    pub fn health_level(&self) -> HealthLevel {
        let v_has_err = self
            .validation_report
            .as_ref()
            .is_some_and(|r| !r.errors.is_empty());
        let inv_has_violation = self
            .invariant_report
            .as_ref()
            .is_some_and(|r| !r.violations.is_empty());
        let v_has_warn = self
            .validation_report
            .as_ref()
            .is_some_and(|r| !r.warnings.is_empty());
        // §V4: strict mode promotes validation warnings to errors.
        if v_has_err || inv_has_violation || (self.validation_strict && v_has_warn) {
            return HealthLevel::Red;
        }
        let v_missing = self.validation_report.is_none();
        let inv_missing = self.invariant_report.is_none();
        if v_has_warn || v_missing || inv_missing {
            return HealthLevel::Yellow;
        }
        HealthLevel::Green
    }

    /// §V6 — pre-export refuse-on-error gate (parity with `sectorforge
    /// generate`). Recomputes the validation report (via
    /// [`Self::revalidate_now`]) and the invariant report against the live
    /// sector, then returns `Some(reason)` when an export must be refused: any
    /// validation error, any invariant violation, or — under §V4 strict mode —
    /// any validation warning. Returns `None` when the sector is clean enough
    /// to export.
    pub fn export_block_reason(&mut self) -> Option<String> {
        self.revalidate_now();
        self.invariant_report = Some(sectorforge::invariants::check_sector(&self.sector));

        let (val_errors, val_warns) = self
            .validation_report
            .as_ref()
            .map(|r| (r.errors.len(), r.warnings.len()))
            .unwrap_or((0, 0));
        let inv_count = self
            .invariant_report
            .as_ref()
            .map(|r| r.violations.len())
            .unwrap_or(0);
        let strict_warns = self.validation_strict && val_warns > 0;

        if val_errors == 0 && inv_count == 0 && !strict_warns {
            return None;
        }

        let mut parts: Vec<String> = Vec::new();
        if val_errors > 0 {
            parts.push(format!("{val_errors} validation error(s)"));
        }
        if strict_warns {
            parts.push(format!("{val_warns} validation warning(s) (strict mode)"));
        }
        if inv_count > 0 {
            parts.push(format!("{inv_count} invariant violation(s)"));
        }
        Some(format!(
            "Export refused — {}. Resolve them in the VALIDATION / INVARIANTS \
             tabs (or turn off strict mode), then export again.",
            parts.join(" + ")
        ))
    }

    /// §CF4 / §CF5: run `BuilderCommand::AdvanceConflictTicks` for `ticks` and
    /// append per-system + per-world diff rows to the `conflict_panel` tick log.
    /// Diffs
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
        let next_index = self
            .conflict_panel
            .tick_log
            .back()
            .map(|e| e.tick_index + 1)
            .unwrap_or(0);
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
        if self.conflict_panel.tick_log.len() >= self.conflict_panel.tick_log_capacity {
            self.conflict_panel.tick_log.pop_front();
        }
        self.conflict_panel.tick_log.push_back(entry);
    }
}
