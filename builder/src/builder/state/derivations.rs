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
use crate::builder::derivation_jobs::{
    compute_chronicle, compute_hooks, compute_missions, compute_personae, compute_prose,
    compute_relations, compute_sites, DerivationJobResult, DerivationPayload, InFlightDerivation,
};

/// LD3 drain disposition for one in-flight job this tick, separating a received
/// result from a dropped channel so [`BuilderState::pump_derivation_jobs`] can
/// borrow the in-flight map immutably while collecting, then mutate it after.
enum DerivationDrain {
    Result(DerivationJobResult),
    Disconnected,
}

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

    /// Audit finding #21 — invalidate specific overlay kinds directly. Used by
    /// a watcher-driven catalog reload to stale the per-kind overlays a catalog
    /// file feeds when no [`DepClass`] captures that input (`hooks.toml`,
    /// `sites.toml`, `missions.toml`, `prose.toml`, `personae.toml`,
    /// `history.toml`, name tables).
    pub fn invalidate_derivation_kinds(&mut self, kinds: &[DerivationKind]) {
        self.derivations.invalidate_kinds(kinds);
    }

    /// Audit finding #21 — stale every already-derived overlay. Used by a
    /// watcher-driven reload of an input whose footprint is the whole graph
    /// (`worlds.toml`, `sectorforge.toml`, the name tables) where a precise
    /// per-kind diff would invalidate everything anyway.
    pub fn invalidate_all_derivations(&mut self) {
        self.derivations.invalidate_all();
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
    /// renders if a prior mutation marked it stale, so the panel about to paint
    /// always reads a live result *this frame* — the synchronous fast path the
    /// LD3 background dispatch deliberately keeps. Off-tab overlays are refreshed
    /// off the GUI thread by [`Self::dispatch_background_derivations`].
    pub fn pump_derivations(&mut self) {
        if let Some(kind) = Self::tab_auto_derivation(self.active_tab) {
            self.ensure_fresh(kind);
        }
    }

    /// LD3 — the eight `recompute_derivation` kinds that may run on a background
    /// worker. `Economy` is deliberately excluded: when `feed_stability` is on
    /// its install calls `apply_stability_nudge(&report, self.sector_mut())`,
    /// which **mutates the sector** (per-world stability) and then invalidates
    /// `SystemsWorlds` — a document-affecting cascade that has to happen on the
    /// UI thread, so economy stays on the synchronous `ensure_fresh` path. The
    /// other seven are pure: they install a derived-cache field and nothing else.
    fn is_background_eligible(kind: DerivationKind) -> bool {
        matches!(
            kind,
            DerivationKind::Relations
                | DerivationKind::History
                | DerivationKind::Personae
                | DerivationKind::Hooks
                | DerivationKind::Sites
                | DerivationKind::Missions
                | DerivationKind::Prose
        )
    }

    /// LD3 — per-frame dispatch. For **every** stale background-eligible kind
    /// (off-tab included — this is the LD3 goal: refresh overlays ahead of being
    /// visited, not just the active tab) with no job already in flight, capture
    /// the input fingerprint, snapshot the inputs (`Arc::clone` the sector +
    /// clone the catalog cfg + scalars), flag the kind `deriving`, and spawn a
    /// worker that runs the matching pure `compute_*` fn. The worker only reads
    /// its owned snapshot, so its result is identical to the synchronous path;
    /// [`Self::pump_derivation_jobs`] re-checks the captured fingerprint before
    /// installing, discarding any result whose inputs drifted mid-flight.
    ///
    /// The active tab's `ensure_fresh` (run in [`Self::pump_derivations`]) still
    /// recomputes that one overlay synchronously the same frame, so the visible
    /// panel never waits on the worker. A kind already being derived in the
    /// foreground that frame is skipped here (its fingerprint will already match
    /// once `ensure_fresh` marks it fresh).
    pub fn dispatch_background_derivations(&mut self, ctx: &egui::Context) {
        use sectorforge_gui_core::jobs::spawn_job;
        // Snapshot the stale set first; spawning marks `deriving` but does not
        // touch `stale`, so a plain clone of the keys is a stable iteration base.
        let stale: Vec<DerivationKind> = self
            .derivations
            .stale
            .iter()
            .copied()
            .filter(|k| Self::is_background_eligible(*k))
            .filter(|k| !self.derivation_jobs.has_in_flight(*k))
            .collect();

        for kind in stale {
            let fingerprint = self.derivation_fingerprint(kind);
            let sector = self.sector.share();
            // Build the same inputs the synchronous `recompute_*` would, owned
            // so they move into the worker.
            let job = match kind {
                DerivationKind::Relations => {
                    let cfg = self.data_catalogs.relations.clone().unwrap_or_default();
                    let threshold = self.config.generation.relations.min_world_presence;
                    spawn_job(
                        "builder-derive-relations",
                        0,
                        "Deriving relations…",
                        ctx.clone(),
                        move |_| {
                            DerivationJobResult::Done(DerivationPayload::Relations(Box::new(
                                compute_relations(&sector, &cfg, threshold),
                            )))
                        },
                    )
                }
                DerivationKind::History => {
                    let cfg = self.data_catalogs.history.clone().unwrap_or_default();
                    spawn_job(
                        "builder-derive-history",
                        0,
                        "Deriving chronicle…",
                        ctx.clone(),
                        move |_| {
                            DerivationJobResult::Done(DerivationPayload::History(Box::new(
                                compute_chronicle(&sector, &cfg),
                            )))
                        },
                    )
                }
                DerivationKind::Personae => {
                    let cfg = self.data_catalogs.personae.clone().unwrap_or_default();
                    spawn_job(
                        "builder-derive-personae",
                        0,
                        "Deriving personae…",
                        ctx.clone(),
                        move |_| {
                            DerivationJobResult::Done(DerivationPayload::Personae(Box::new(
                                compute_personae(&sector, &cfg),
                            )))
                        },
                    )
                }
                DerivationKind::Hooks => {
                    let mut cfg = self.data_catalogs.hooks.clone().unwrap_or_default();
                    cfg.hide_hidden_hooks = self.hooks_panel.player_edition;
                    spawn_job(
                        "builder-derive-hooks",
                        0,
                        "Deriving hooks…",
                        ctx.clone(),
                        move |_| {
                            DerivationJobResult::Done(DerivationPayload::Hooks(Box::new(
                                compute_hooks(&sector, &cfg),
                            )))
                        },
                    )
                }
                DerivationKind::Sites => {
                    let mut cfg = self.data_catalogs.sites.clone().unwrap_or_default();
                    cfg.player_edition = self.sites_panel.player_edition;
                    spawn_job(
                        "builder-derive-sites",
                        0,
                        "Deriving sites…",
                        ctx.clone(),
                        move |_| {
                            DerivationJobResult::Done(DerivationPayload::Sites(Box::new(
                                compute_sites(&sector, &cfg),
                            )))
                        },
                    )
                }
                DerivationKind::Missions => {
                    let mut cfg = self.data_catalogs.missions.clone().unwrap_or_default();
                    cfg.player_edition = self.missions_panel.player_edition;
                    spawn_job(
                        "builder-derive-missions",
                        0,
                        "Deriving missions…",
                        ctx.clone(),
                        move |_| {
                            DerivationJobResult::Done(DerivationPayload::Missions(Box::new(
                                compute_missions(&sector, &cfg),
                            )))
                        },
                    )
                }
                DerivationKind::Prose => {
                    let cfg = self.data_catalogs.prose.clone().unwrap_or_default();
                    spawn_job(
                        "builder-derive-prose",
                        0,
                        "Deriving prose…",
                        ctx.clone(),
                        move |_| {
                            DerivationJobResult::Done(DerivationPayload::Prose(Box::new(
                                compute_prose(&sector, &cfg),
                            )))
                        },
                    )
                }
                // Not background-eligible (filtered above) — unreachable.
                _ => continue,
            };
            self.derivations.mark_deriving(kind);
            self.derivation_jobs
                .in_flight
                .insert(kind, InFlightDerivation { job, fingerprint });
        }
    }

    /// LD3 — per-frame drain of finished background derivations, called from the
    /// app's `pump_active_state` alongside [`Self::pump_derivations`]. For each
    /// in-flight job: `try_recv`. On `Empty` leave it. On `Disconnected` drop the
    /// handle (worker failure — the kind stays stale and re-dispatches next
    /// frame). On a result, **re-check** the live fingerprint against the one
    /// captured at dispatch; if it changed, discard the stale result (drop the
    /// handle, leave the kind stale so it re-dispatches against fresh inputs);
    /// otherwise install the payload into its derived-cache field — exactly the
    /// off-bus write the synchronous `recompute_*` performs — and
    /// `mark_derivation_fresh` (which clears both stale and deriving).
    ///
    /// All ledger writes here run on the UI thread only. Returns `true` when at
    /// least one result landed so the caller can request a repaint.
    pub fn pump_derivation_jobs(&mut self) -> bool {
        use std::sync::mpsc::TryRecvError;

        // Collect the kinds whose jobs have produced something this tick. We
        // resolve them via `DerivationKind::ALL` (stable order) so the drain is
        // deterministic regardless of the in-flight map.
        let mut ready: Vec<(DerivationKind, DerivationDrain)> = Vec::new();
        for kind in DerivationKind::ALL {
            let Some(slot) = self.derivation_jobs.in_flight.get(kind) else {
                continue;
            };
            match slot.job.receiver.try_recv() {
                Ok(result) => ready.push((*kind, DerivationDrain::Result(result))),
                Err(TryRecvError::Disconnected) => {
                    ready.push((*kind, DerivationDrain::Disconnected))
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        let mut landed = false;
        for (kind, drain) in ready {
            // Remove the in-flight slot up front; whether we install or discard,
            // the job is done. `captured` is the dispatch-time fingerprint.
            let Some(slot) = self.derivation_jobs.in_flight.remove(&kind) else {
                continue;
            };
            let captured = slot.fingerprint;

            let payload = match drain {
                DerivationDrain::Disconnected => {
                    // Worker thread vanished without sending — clear the
                    // `deriving` flag and leave the kind stale so the next
                    // dispatch retries. (No fallible derivations today; this is
                    // the channel-drop safety net.)
                    self.derivations.deriving.remove(&kind);
                    continue;
                }
                DerivationDrain::Result(DerivationJobResult::Failed { .. }) => {
                    self.derivations.deriving.remove(&kind);
                    continue;
                }
                DerivationDrain::Result(DerivationJobResult::Done(payload)) => payload,
            };

            // LD3 stale-guard: discard a result computed from inputs that have
            // since changed. Leave `stale` set (do NOT mark fresh) so the next
            // dispatch recomputes against the current inputs; clear `deriving`
            // so that dispatch is not blocked by this finished job.
            if self.derivation_fingerprint(kind) != captured {
                self.derivations.deriving.remove(&kind);
                landed = true;
                continue;
            }

            self.install_derivation_payload(payload);
            self.mark_derivation_fresh(kind);
            landed = true;
        }
        landed
    }

    /// LD3 — install a background-derived payload into its derived-cache home,
    /// mirroring the off-bus field write the synchronous `recompute_*` performs.
    /// Recomputed cache, not a user edit, so it is intentionally **not** routed
    /// through `BuilderCommand` (parity with the sync `self.sector.relations = …`
    /// / `self.*_report = …` installs).
    ///
    /// Note the *catalog-cascade* invalidations the sync `recompute_relations`
    /// (`invalidate(&[RelationsCfg])`) performs are deliberately **omitted** here:
    /// that cascade exists for the case where a `relations.toml` edit routes
    /// straight into the recompute off the command bus, and such edits always
    /// recompute synchronously in the panel — they never reach this background
    /// path. A kind only lands here after becoming stale via the command bus,
    /// which already marked every dependent (e.g. Briefing, via its own
    /// `SystemsWorlds` / `Factions` deps) stale at invalidation time. Re-running
    /// the cascade here would be both redundant and, off-thread, an extra ledger
    /// write with no triggering edit.
    fn install_derivation_payload(&mut self, payload: DerivationPayload) {
        match payload {
            DerivationPayload::Relations(matrix) => {
                self.sector.relations = std::sync::Arc::new(*matrix);
                self.dirty = true;
                self.mark_validation_dirty();
                self.trigger_auto_save();
            }
            DerivationPayload::History(chronicle) => {
                self.sector.chronicle = *chronicle;
                self.dirty = true;
                self.mark_validation_dirty();
                self.trigger_auto_save();
            }
            DerivationPayload::Personae(report) => {
                self.personae_report = Some(*report);
                self.mark_validation_dirty();
            }
            DerivationPayload::Hooks(report) => {
                self.hooks_report = Some(*report);
                self.mark_validation_dirty();
            }
            DerivationPayload::Sites(report) => {
                self.sites_report = Some(*report);
                self.mark_validation_dirty();
            }
            DerivationPayload::Missions(report) => {
                self.missions_report = Some(*report);
                self.mark_validation_dirty();
            }
            DerivationPayload::Prose(report) => {
                self.prose_report = Some(*report);
                self.mark_validation_dirty();
            }
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
        // LD3: identical inputs → identical output as the background worker,
        // which calls this same pure fn. Install mirrors the off-bus cache
        // write the worker's drain performs.
        let matrix = compute_relations(&self.sector, &cfg, threshold);
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
        // LD3: delegate to the free fn so the background worker computes the
        // chronicle identically (manual-event preservation included).
        compute_chronicle(&self.sector, &cfg)
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
        let report = compute_personae(&self.sector, &cfg);
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
        let report = compute_hooks(&self.sector, &cfg);
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
        let report = compute_sites(&self.sector, &cfg);
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
        let report = compute_missions(&self.sector, &cfg);
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
        let report = compute_prose(&self.sector, &cfg);
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
        // PERF1/§42: the invariant re-check runs here (debounced), not inline on
        // the command-bus apply path. It reads the live sector directly (no
        // worlds catalog needed), so it is unconditional — unlike `validate`
        // below, which needs a synthesized `ProjectInput`.
        self.invariant_report = Some(sectorforge::invariants::check_sector(&self.sector));
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
    ///
    /// This is the document-faithful assembly: every catalog is taken verbatim
    /// from `self.data_catalogs`. The iterative-gen wizard's transient
    /// regions-override patch is applied via
    /// [`Self::synthesize_project_input_with`] instead (this method delegates to
    /// it with no override, so the two are byte-identical when no patch is set).
    pub fn synthesize_project_input(&self) -> Option<ProjectInput> {
        self.synthesize_project_input_with(None)
    }

    /// ITERATIVE_GENERATION.md §5 — same as [`Self::synthesize_project_input`]
    /// but with an optional **transient regions-override patch** substituted for
    /// the regions catalog. This is the single seam through which the iterative
    /// wizard's in-memory `RegionsConfig` edit reaches a prefix/commit run
    /// **without** mutating `self.data_catalogs` (invariant #5: the override is
    /// session state, applied only when assembling the `ProjectInput`).
    ///
    /// `regions_override == None` is **byte-identical** to the legacy assembly —
    /// the regions catalog is taken verbatim from `data_catalogs.regions`
    /// exactly as before — so the nonce-0 / override-absent path stays
    /// golden-stable (invariants #2 / #6). Returns `None` when the worlds
    /// catalog is missing.
    pub fn synthesize_project_input_with(
        &self,
        regions_override: Option<&sectorforge::regions::RegionsConfig>,
    ) -> Option<ProjectInput> {
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
                // §5 / invariant #5: the transient override (if any) substitutes
                // for the regions catalog here, at ProjectInput-assembly time —
                // `data_catalogs.regions` is left untouched. `None` ⇒ verbatim
                // catalog ⇒ byte-identical to the legacy path (invariants #2/#6).
                regions: regions_override
                    .cloned()
                    .or_else(|| self.data_catalogs.regions.clone())
                    .unwrap_or_default(),
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
        // `revalidate_now` recomputes both the validation report and the
        // invariant report synchronously (not debounced) against the live
        // sector — exactly the fresh gate an export needs.
        self.revalidate_now();

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

#[cfg(test)]
mod ld3_background_tests {
    //! §39 LD3 — off-thread overlay re-derivation: pure-fn determinism
    //! equivalence + the dispatch / drain / stale-guard lifecycle.

    use super::*;
    use sectorforge::sector_model::{GeneratedFaction, GeneratedSector};
    use std::time::Duration;

    fn gen_faction(id: &str, kind: &str, disposition: &str) -> GeneratedFaction {
        GeneratedFaction {
            id: id.into(),
            name: std::sync::Arc::from(id),
            kind: std::sync::Arc::from(kind),
            disposition: std::sync::Arc::from(disposition),
            subfactions: Vec::new(),
            system_presence: vec![],
            world_presence: vec![],
            power: Default::default(),
        }
    }

    /// Two-faction fixture: enough for `relations::derive_with_threshold` to
    /// emit a pair, so a recomputed matrix is observably non-empty.
    fn seed_state() -> BuilderState {
        let mut state = BuilderState::new_blank("ld3-test", "LD3", "s", 4, 4);
        state.sector = GeneratedSector::empty("ld3-test", "LD3", "s", 4, 4).into();
        state
            .sector
            .factions
            .push(gen_faction("imp", "imperial", "lawful"));
        state
            .sector
            .factions
            .push(gen_faction("chaos", "chaos_space_marine", "hostile"));
        state
    }

    /// Bounded poll of the drain pump so a flaky/slow worker thread cannot hang
    /// the test: drain each tick, sleep briefly, cap the iterations.
    fn drain_until_done(state: &mut BuilderState, kind: DerivationKind) -> bool {
        for _ in 0..200 {
            state.pump_derivation_jobs();
            if !state.derivations.deriving.contains(&kind)
                && !state.derivation_jobs.has_in_flight(kind)
            {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    /// Determinism: the pure `compute_relations` returns the same matrix the
    /// synchronous `recompute_relations` installs from identical inputs, so a
    /// value computed off-thread is byte-identical to the on-thread one.
    #[test]
    fn pure_compute_relations_equals_sync_recompute() {
        let mut state = seed_state();
        let cfg = state.data_catalogs.relations.clone().unwrap_or_default();
        let threshold = state.config.generation.relations.min_world_presence;
        let pure = compute_relations(&state.sector, &cfg, threshold);

        state.recompute_relations();
        let sync = state.sector.relations.as_ref();

        // Equal by serialized form — the derived matrix is a pure function of
        // its input slice, so equal inputs ⇒ equal output ⇒ same JSON.
        assert_eq!(
            serde_json::to_string(&pure).unwrap(),
            serde_json::to_string(sync).unwrap(),
            "background pure-fn result must equal the synchronous recompute"
        );
    }

    /// LD3 dispatch: an off-tab stale eligible kind is flagged `deriving` and a
    /// job is stored. Uses Relations while the active tab is the default
    /// (Project), so this exercises the off-tab path specifically.
    #[test]
    fn dispatch_marks_deriving_and_stores_job_for_off_tab_kind() {
        let ctx = egui::Context::default();
        let mut state = seed_state();
        // Prime the kind as previously-derived so `invalidate` can flag it
        // stale (invalidate skips never-derived cold kinds).
        state.mark_derivation_fresh(DerivationKind::Relations);
        // A Factions-class mutation stales Relations (an off-tab overlay here).
        state.derivations.invalidate(&[DepClass::Factions]);
        assert!(state.derivations.is_stale(DerivationKind::Relations));

        state.dispatch_background_derivations(&ctx);

        assert!(
            state
                .derivations
                .deriving
                .contains(&DerivationKind::Relations),
            "dispatch must flag the off-tab kind deriving"
        );
        assert!(
            state
                .derivation_jobs
                .has_in_flight(DerivationKind::Relations),
            "dispatch must store an in-flight job for the kind"
        );

        // A second dispatch the same frame must not double-spawn.
        state.dispatch_background_derivations(&ctx);
        assert_eq!(
            state.derivation_jobs.in_flight.len(),
            1,
            "an in-flight kind is not re-dispatched"
        );

        // Drain to completion so the worker thread is joined before the test
        // ends (and assert the lifecycle clears).
        assert!(drain_until_done(&mut state, DerivationKind::Relations));
    }

    /// LD3 end-to-end: invalidate an OFF-tab kind, dispatch (deriving set
    /// becomes non-empty), then poll the drain in a bounded loop until the
    /// result lands — asserting the overlay updated and both the stale and
    /// deriving flags cleared.
    #[test]
    fn background_drain_installs_and_clears_when_fingerprint_stable() {
        let ctx = egui::Context::default();
        let mut state = seed_state();
        // Start from an empty matrix and a previously-derived (now stale) kind.
        assert!(state.sector.relations.pairs.is_empty());
        state.mark_derivation_fresh(DerivationKind::Relations);
        state.derivations.invalidate(&[DepClass::Factions]);

        state.dispatch_background_derivations(&ctx);
        assert!(
            !state.derivations.deriving.is_empty(),
            "deriving set populated after dispatch"
        );

        assert!(
            drain_until_done(&mut state, DerivationKind::Relations),
            "background result should land within the bounded poll"
        );

        // Overlay installed from the worker's payload.
        assert_eq!(
            state.sector.relations.pairs.len(),
            1,
            "the background-derived matrix must be installed"
        );
        // Both freshness flags cleared by `mark_derivation_fresh`.
        assert!(!state.derivations.is_stale(DerivationKind::Relations));
        assert!(!state
            .derivations
            .deriving
            .contains(&DerivationKind::Relations));
        assert!(!state
            .derivation_jobs
            .has_in_flight(DerivationKind::Relations));
        assert_eq!(
            state.derivation_status(DerivationKind::Relations),
            DerivationStatus::Fresh
        );
    }

    /// LD3 stale-guard: a result whose inputs drifted between dispatch and drain
    /// is discarded — the overlay is NOT installed and the kind stays stale so
    /// it re-dispatches against the fresh inputs.
    ///
    /// Race-free by construction: we mutate an input **immediately after
    /// dispatch**, so the captured fingerprint is already stale no matter when
    /// the worker finishes. The only drain outcomes are then (a) the result has
    /// arrived → the guard discards it, or (b) still in flight → keep polling.
    /// Both converge on "overlay untouched, kind still stale, slot cleared".
    #[test]
    fn background_drain_discards_result_when_fingerprint_changed() {
        let ctx = egui::Context::default();
        let mut state = seed_state();
        // Empty matrix to start; a stale, previously-derived Relations kind.
        assert!(state.sector.relations.pairs.is_empty());
        state.mark_derivation_fresh(DerivationKind::Relations);
        state.derivations.invalidate(&[DepClass::Factions]);

        state.dispatch_background_derivations(&ctx);
        assert!(state
            .derivation_jobs
            .has_in_flight(DerivationKind::Relations));

        // Drift the inputs *now*, before the result can be drained — this makes
        // the captured dispatch-time fingerprint stale for certain.
        state
            .sector
            .factions
            .push(gen_faction("orks", "ork", "hostile"));
        let live_fp = state.derivation_fingerprint(DerivationKind::Relations);
        let captured_fp = state
            .derivation_jobs
            .in_flight
            .get(&DerivationKind::Relations)
            .map(|s| s.fingerprint.clone())
            .expect("job in flight");
        assert_ne!(
            live_fp, captured_fp,
            "the input mutation must change the live fingerprint vs the captured one"
        );

        // Drain until the worker finishes and the guard fires (bounded poll).
        for _ in 0..200 {
            state.pump_derivation_jobs();
            if !state
                .derivation_jobs
                .has_in_flight(DerivationKind::Relations)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        // The stale 2-faction result must have been discarded, not installed:
        // the matrix is still empty.
        assert!(
            state.sector.relations.pairs.is_empty(),
            "a stale-fingerprint result must not be installed"
        );
        // The kind stays stale so the next dispatch recomputes against the
        // current (3-faction) inputs; the finished job slot and deriving flag
        // are cleared.
        assert!(
            state.derivations.is_stale(DerivationKind::Relations),
            "the kind stays stale so it re-dispatches against fresh inputs"
        );
        assert!(!state
            .derivation_jobs
            .has_in_flight(DerivationKind::Relations));
        assert!(!state
            .derivations
            .deriving
            .contains(&DerivationKind::Relations));
    }
}
