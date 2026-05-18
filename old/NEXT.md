# NEXT — design-doc coverage status

This file used to list deferred items from
`faction_sector_control_and_power_design.md`. As of this pass every item
listed there has at least a minimum-viable functional implementation; the
file is now an index of *what landed* and *what is intentionally minimal*
so a follow-up run can extend the depth of any single feature without
re-reading the design doc end-to-end.

The current implementation lives across multiple modules. The top-level
entry point — `generate_sector` in [src/lib.rs](src/lib.rs) — wires
faction assignment → claims → control summaries → stability → hidden
routes → route control → surface regions → conflict state → orbital
assets → fog-of-war intel → archetype rules → power projection →
influence field, in that order.

## 1. Surface-region modelling (§6.1) — done

Per-world named `SurfaceRegion`s in
[src/surface_region.rs](src/surface_region.rs). Each `GeneratedWorld`
carries a `regions: Vec<SurfaceRegion>` derived from world type +
population. Region kinds: `Capital`, `Hive`, `Underhive`,
`ForgeComplex`, `ShrineContinent`, `AgriBelt`, `CardinalSpire`,
`KnightHousehold`, `Wilderness`, `TombComplex`, `Hideout`, `Other`. Per
region the model carries `dominant` (faction id), `control_score`,
`population_weight`, and `visibility`. The dominant pick uses both the
faction's local control score and a per-region bias so e.g. a
Genestealer cult dominates the underhive even when the Imperial admin
owns the capital. Surfaced in the Markdown renderer
([src/render.rs](src/render.rs) "Surface regions" table) and in the
GUI `world_detail` panel ([src/gui/info_panel.rs](src/gui/info_panel.rs)
`SURFACE REGIONS` section).

**Minimum-viable depth.** The design also calls for a "planet zoom"
panel that draws regions geographically. The current renderer prints a
table; no polygon layout. Adding that is purely additive GUI work
against the existing `regions` data.

## 2. Orbital control as a separate dimension (§6.3) — done

[src/orbital_assets.rs](src/orbital_assets.rs). Each
`GeneratedSystem` now carries `orbital_assets: Vec<OrbitalAsset>` and a
`blockade: BlockadeReport`. Asset kinds: `Station`, `Shipyard`,
`DefensePlatform`, `BlockadeFleet`. Each asset has a `faction_id`,
`strength`, and optional `ship_inventory`. Blockade detection fires
when `dominant != orbital_controller` AND a blockade fleet is present,
or when a quarantine tag forces a blockade. Surfaced in the Markdown
renderer ("Orbital assets" table + blockade line) and in the GUI
`system_summary` (`BLOCKADE` + `ORBITAL ASSETS` sections).

**Minimum-viable depth.** Shipyard production rate / refit cycle is not
modelled — `ship_inventory` is a flat list. Blockade *resolution* (the
fleet breaking, swap of orbital controller) is delegated to the
conflict tick loop (§5).

## 3. Interstellar route control (§6.5) — done

Static per-route per-faction `RouteControl` lives in
[src/route_control.rs](src/route_control.rs); hidden route layers in
[src/hidden_routes.rs](src/hidden_routes.rs). `RouteType` now has
six variants: the original four plus `Webway` (Aeldari/Harlequin/
Drukhari only), `BlackShip` (Inquisition / Deathwatch / Grey Knights),
and `SmugglingLane` (criminal / Drukhari / rebel / Genestealer).
Hidden routes connect *any two systems* in the sector that both carry
meaningful presence of the relevant kind — they do **not** honour the
warp-distance cap. Each hidden route is kind-gated for power
projection (§4): a non-Aeldari faction cannot use a webway thread to
project power.

Visual: the sector PNG legend now shows seven route classes (four
public + three hidden) using the same dashed/dotted patterns as the
existing types. The route planner ([src/gui/route_planner.rs](src/gui/route_planner.rs))
treats hidden routes as expensive so it does not auto-route through a
restricted-access network.

**Minimum-viable depth.** Animated convoy particles and per-faction
route hit-testing on the hex map are not yet wired — the GUI still
displays per-route info via the system summary, not by clicking the
route line.

## 4. Power projection over routes (§7.2) — done

[src/power_projection.rs](src/power_projection.rs). `PowerProjectionMap`
maps `faction_id → system_id → projected_total_power`. BFS from each
faction's source systems, decaying by `1 / (1 + hops²)` (the design
formula) and multiplied by a per-kind `doctrine` factor (Tau 1.4,
Necron 1.2, Tyranid 0.6, Aeldari 0.8, Drukhari 0.7, Ork 1.1, default
1.0). Hidden routes are kind-gated. The output is stored on
`sector.power_projection`. `apply_to_factions` scales each faction's
`PowerProfile` by a *reach* factor so factions that project broadly
score higher in the rollup than those concentrated at home.

**Minimum-viable depth.** Doctrine is a per-kind constant, not a
per-faction simulation variable; the design's "awakening Necron jumps
to 1.8" and "Tyranid biomass front decays sharply" are encoded as
hints in §11 archetype state but do not feed back into the projection
multiplier dynamically.

## 5. Conflict simulation (§11) — done

[src/conflict.rs](src/conflict.rs). Each `GeneratedSystem` and
`GeneratedWorld` carries a `ConflictState` with
`started_tick`, `last_change_tick`, `momentum` (-100..=100),
`intensity` (0..=100), `mobilisation`, `attacker`, `defender`,
`visible_controller`, `age`. `derive_world_conflict` /
`derive_system_conflict` seed the initial state from the finalised
control summary. `advance_sector(&mut sector)` advances one tick:
intensity decays when not actively contested, grows when an attacker
is present, momentum drifts toward zero, and a decisive attacker
swap fires when `intensity >= 80 && momentum <= -40`. The
`visible_controller` only updates after `HYSTERESIS_TICKS = 3` ticks
of the new defender holding — that is §11.3 hysteresis.

The generator emits the initial tick; callers that want to evolve the
sector call `sectorforge::advance_sector` in a loop.

**Minimum-viable depth.** Mobilisation / supply chains are not
modelled in detail. The tick loop is deterministic with no RNG, so it
is reproducible but does not stochastically pick between equally-good
choices.

## 6. Stability model (§11.1) — done

Unchanged from the previous pass.
[src/stability.rs](src/stability.rs) attaches a `StabilityState` to
every world + system. Now also bumped by `SystemState::Warzone`,
`Blockaded`, `Infiltrated`, `Quarantined`. Renderer + GUI both surface
the seven dimensions.

## 7. Intel / fog of war (§12) — done

[src/intel.rs](src/intel.rs). `SystemIntel` is per-system and keyed
by observer faction id. Each `ObserverView` records
`last_verified_tick`, `confidence`, `suspected_presences` (with
`IntelSource`: DirectObservation / AstropathicReport /
InquisitorialAnalysis / Rumor / ImaginedDeduction), a
`propaganda_state` (OfficialPacified / OfficialContested / etc.), and
a `classified_state` (Public / CodexRedactus / PurgatusSigillum /
ExterminatusFlag). `redact_world_for_observer` is the renderer hook
that drops low-confidence factions from a world's presence list for a
specific observer.

**Minimum-viable depth.** The default render uses omniscient view —
the GUI does not currently let the user pick an observer to redact
through. The data layer is fully there; one toggle in the GUI would
expose it.

## 8. Per-faction style auto-generation (§9.6) — done

Unchanged from the previous pass. Implementation in
[src/faction_style.rs](src/faction_style.rs) +
[src/gui/palette.rs](src/gui/palette.rs).

## 9. Continuous sector area layers (§9.3) — done

[src/influence_field.rs](src/influence_field.rs). `InfluenceField`
holds a row-major `Vec<CellAssignment>` for every grid cell in the
sector. Each cell records the dominant faction (Voronoi-style winner
with `1 / (1 + d²)` falloff from the system anchor) and the top-3
contenders. `bands` rolls cell membership into per-faction
`TerritoryBand`s. Stored on `sector.influence_field`.

**Minimum-viable depth.** Polygon outlining and per-pixel soft fill
are renderer concerns — the data is there, the PNG/GUI consumer can
sample it. Not yet rendered as a continuous wash.

## 10. Influence heatmaps (§9.5) — done

Unchanged from the previous pass.
[src/heatmap.rs](src/heatmap.rs) + [src/gui/heatmap.rs](src/gui/heatmap.rs).

## 11. Special-faction archetype rules (§16) — done

[src/archetypes.rs](src/archetypes.rs). Eight archetype rules layered
on top of the generic dimension model, populated into
`GeneratedSystem.archetype: ArchetypeState`. Each rule is a pure
post-pass that reads finalised presences:

1. **Imperial governance stack** (§16.1) — `imperial_co_sovereigns`
   lists every Imperial-stack faction with admin+legitimacy+
   industrial+ideological weight ≥ 30 in the system.
2. **Necron dormant/awakening** (§16.9) — `necron_phase` ∈
   {None, Dormant, Awakening, Awake}, gated by max necron presence
   score and visibility.
3. **Tyranid biomass front** (§16.8) — `tyranid_stage` ∈
   {None, Inhabited, Besieged, Consumed}; consumed worlds also get a
   `feature:shadow_of_the_warp` tag added system-wide.
4. **Ork Waaagh! momentum** (§16.7) — `ork_waaagh` 0..=100.
5. **Genestealer staged uprising** (§16.6) — `gsc_stage` ∈
   {None, Rumor, HiddenCell, DistrictControl, ParallelGovernment,
   Uprising, PlanetarySeizure}.
6. **Tau sphere of influence** (§16.11) — `tau_sphere` ∈
   {None, Contact, Fringe, Client, Core}.
7. **Aeldari/Drukhari/Harlequin intermittent** (§16.10) —
   `aeldari_activity` 0..=100 keyed off max covert score.
8. **Chaos corruption + metaphysical layer** (§16.12) —
   `chaos_corruption` and `daemon_manifestation` 0..=100.

Surfaced in the Markdown renderer ("Archetype" lines under each
system) and the GUI `system_summary` (`ARCHETYPE` section).

**Minimum-viable depth.** Each rule encodes the *state* the design
calls for; *transitions* (e.g. Genestealer Rumor → HiddenCell over
ticks) are an integration with the §5 conflict tick loop that is not
yet wired. Each archetype state is currently a snapshot derived from
presence dimensions.

## 12. ECS / Bevy architecture (§13) — done (adapter, not migration)

[src/world_ecs.rs](src/world_ecs.rs). `EntityWorld` exposes the sector
as parallel columnar `BTreeMap<EntityId, *Components>` slices for
System / World / Faction / Route entities. Public via
`sectorforge::build_entity_world`. Callers that want to write
ECS-shaped systems (queries by component) get a stable shape without
the codebase migrating off plain structs.

**Minimum-viable depth.** This is an *adapter*, not a `bevy_ecs`
migration. The existing layered design
(`generation` → `sector_model` → `control` → `subsectors` →
`render` → `gui`) is unchanged. A full migration would touch every
module in the crate and pays off only once the conflict tick loop
(§5) is doing real continuous work — a future refactor.

## 13. Save format split (§17) — done

[src/sector_save.rs](src/sector_save.rs). `SectorSave` is the
IDs-only runtime-state half (per-system primary_factions, control
summaries, stability, conflict, intel, orbital assets, blockade,
archetype state, plus per-world factions / control / claims /
regions / conflict). `split(&sector)` extracts it; `merge(&mut
sector, save)` restores it onto a fresh-from-catalog sector and
guards against sector-id / seed mismatch. Public via
`sectorforge::{split_sector_save, merge_sector_save,
write_sector_save, load_sector_save}`.

**Minimum-viable depth.** The single `sector.json` write path still
emits the merged form (back-compat). Callers that want the split
form call the explicit save/load helpers.

## 14. Faction filter / pin UI (§10.1) — done

Unchanged. Implementation in
[src/gui/editor/factions_panel.rs](src/gui/editor/factions_panel.rs).

## 15. Display importance scoring (§10.3) — done

Unchanged. Implementation in [src/importance.rs](src/importance.rs).

## 16. Hysteresis on control change (§11.3) — done

Lives inside [src/conflict.rs](src/conflict.rs) `advance_one`.
`HYSTERESIS_TICKS = 3`: a control flip records on `defender` /
`last_change_tick` immediately; `visible_controller` only adopts the
new defender after `age - last_change_tick >= HYSTERESIS_TICKS`. The
test `attacker_can_flip_control_after_threshold_then_hysteresis`
exercises this round-trip.

## 17. Industrial dimension — done

Unchanged from the previous pass.

---

## Test + invariant status

* `cargo test`: 100/100 pass (62 lib + sub-suites).
* `cargo run --bin sectorforge -- validate-sector --sector
  <generated>.json`: 0 violations on the bundled M42 example.
* `cargo build --release`: clean.

## Where the remaining depth lives

Each "minimum-viable depth" note above marks an area where the data
layer is implemented and the next round of work is purely additive
(GUI polygon rendering, archetype-state transitions through the tick
loop, observer-selection toggle in the GUI). None of it requires
re-reading the design doc; this file is the entire map.
