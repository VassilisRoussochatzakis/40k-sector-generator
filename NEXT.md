# NEXT — deferred work from `faction_sector_control_and_power_design.md`

This file lists items from the faction control / power design doc that were
**intentionally not implemented** in the current pass. Each section says
*what* is missing and *why* it was deferred, so a follow-up run can pick it up
without re-reading the design doc end-to-end.

The current implementation lives in [src/control.rs](src/control.rs); per-world
fields are on `GeneratedWorld` (`factions[].dimensions`, `factions[].dominance`,
`claims`, `control`) and per-system on `GeneratedSystem.control`; per-faction
power on `GeneratedFaction.power`.

## 1. Surface-region modelling (§6.1)

The design proposes splitting each planet into named regions (capital, hive,
underhive, forge complex, shrine continent, tomb complex, etc.) and computing
planetary control as a weighted average over those regions.

**Deferred:** would require a new `SurfaceRegion` DTO, a region-allocation
generator pass, and substantial GUI work for a "planet zoom" view. Not
currently rendered anywhere, so the cost is high relative to the readable
benefit.

## 2. Orbital control as a separate first-class dimension (§6.3)

We expose `orbital` as a presence dimension and surface it in the world /
system multi-winner snapshot, but the design wants explicit
stations / shipyards / defense platforms / blockade fleets as discrete entities
with stocks of ships and minefields.

**Deferred:** needs a new `OrbitalAsset` model, blockade detection that
inspects orbital_controller ≠ surface_dominant under siege constraints, and a
fleet inventory. Today we approximate "blockaded" purely by mismatch between
`dominant` and `orbital_controller` at system level.

## 3. Interstellar route control (§6.5)

Routes currently carry only `route_type`, `stability`, and free-form tags. The
design wants:

* `RouteControl` per route per faction: `patrol`, `toll`, `interdiction`,
  `piracy`, `secrecy`, `confidence`.
* Hidden webway / black-ship / smuggling layers.
* Crossbars for interdiction, animated convoy particles, glow on high-traffic
  edges.

**Deferred:** Touches both the data model and the bitmap renderer. The
"projection falloff over routes" formula in §7.2 also depends on this.

## 4. Power projection over routes (§7.2)

`projected_power_at_node = source_power * route_quality * doctrine *
intel / (1 + distance²)` — currently `PowerProfile` is a static aggregate, with
no falloff or doctrine modifier. Per-kind projection rules in §7 (Tau sphere
expansion, Tyranid biomass front, Necron awakening jumps, Aeldari webway
intermittency, etc.) are not implemented.

**Deferred:** depends on (3) and on per-faction doctrine state. Useful but not
needed for the static snapshot the generator produces today.

## 5. Conflict simulation (§11)

No `ConflictState`, `momentum`, `started_tick`, or `intensity` is generated.
The current model derives a *snapshot* of contestation per world and a
heuristic `SystemState` (Pacified / Fragmented / Blockaded / Warzone /
Infiltrated / Quarantined / Uncharted) but does not advance state over time.

**Deferred:** generator is currently single-shot deterministic; introducing
ticks requires a new save format and a sim loop. Out of scope for this pass.

## 6. Stability model (§11.1)

`StabilityState` (`public_order`, `corruption`, `fear`, `rebellion_risk`,
`xenos_threat`, `warp_instability`, `famine_or_resource_stress`) is not
tracked. We rely on tag/feature heuristics ("war_zone", "quarantined",
"daemonic_corruption") to classify system state.

**Deferred:** valuable for narrative campaign use but requires sim ticks
(§5) to evolve. Adding the static fields without an update loop would just
encode duplicates of existing tags.

## 7. Intel / fog of war (§12)

Per-presence `intel_confidence` is computed and serialised (derived from
`visibility`), but the broader model — `IntelSource`, `last_verified_tick`,
`suspected_presences`, `propaganda_state`, `classified_state` — is not.
Rendering does not desaturate, blur, or redact unknown factions; everything is
shown at full fidelity.

**Deferred:** requires a player/observer concept and per-faction visibility
tracking, neither of which exist in the generator today.

## 8. Per-faction style auto-generation (§9.6)

The design wants every faction to receive an auto-assigned palette + glyph
keyed by `kind` (varying hue / pattern / icon by `id`), plus disposition-driven
border behaviour (clean for `lawful`, jagged for `hostile`, low-opacity for
`secretive`, etc.). Today the bitmap renderer does not colour by faction at
all.

**Deferred:** medium effort but invasive — touches the palette module and
every renderer (sector PNG, system map, GUI map panel). Worth doing as a
self-contained follow-up.

## 9. Continuous sector area layers (§9.3)

Voronoi cells, route-weighted influence fields, jump-route graph regions, and
soft-territory polygons are not produced. The hex map shows only system glyphs
and route lines.

**Deferred:** ties into (3) and (8). Would also need a new caching layer in
the renderer because per-pixel influence is expensive.

## 10. Influence heatmaps (§9.5)

No per-faction heatmap (`Control`, `Military`, `Trade`, `Covert`, `Faith`,
`Threat`, `Intel` modes). The data needed to render these *does* exist now
(presence dimensions, route graph), so this is the most natural follow-up to
unlock visually.

**Deferred:** purely a rendering and UI surface — adding the data was the
hard part.

## 11. Special-faction archetype rules (§16)

Generic dimension profiles are in place per kind, but the design has many
faction-specific behaviours not yet implemented:

* **Imperial governance stack** (§16.1) — sovereignty shared between
  Administratum, Ecclesiarchy, Mechanicus, Navy, Knights with overlap rather
  than competition.
* **Necron dormant / awakening transitions** (§16.9) — dormant tomb claims
  flipping to active control when an awakening event fires.
* **Tyranid biomass front + consumption gradient** (§16.8) — worlds moving
  from inhabited → besieged → consumed, with suppressed comms near high
  Shadow.
* **Ork Waaagh! momentum** (§16.7) — front-line density and infighting risk.
* **Genestealer staged uprising** (§16.6) — rumor → hidden cell → district →
  parallel government → uprising → planetary seizure.
* **Tau sphere of influence** (§16.11) — smooth client-state borders driven
  by diplomacy + commerce.
* **Aeldari / Drukhari / Harlequin intermittent appearance** (§16.10) —
  hidden routes, surgical raids, no persistent fill.
* **Chaos corruption + metaphysical layer** (§16.12) — separate traitor
  military occupation from daemonic manifestation probability.

**Deferred:** each is a small project. The current generic model encodes the
*flavor* of each kind via the dimension profile, which is enough for a
snapshot but does not produce the narrative state transitions the design
calls for.

## 12. ECS / Bevy architecture (§13)

The design recommends `bevy_ecs` (or `hecs` / `legion`) with separate
`SimulationSystems`, `RenderWorld`, and `UIState` modules. The codebase today
is plain structs + serde + egui. Migrating would be a significant
refactor.

**Deferred:** the existing layered design (`generation` → `sector_model` →
`control` → `subsectors` → `render` → `gui`) is working well at current scope;
ECS only pays off once simulation ticks (§5) are real work.

## 13. Save format split (§17)

Today the generator emits a single `sector.json` per run that contains both
static catalog references *and* runtime state. The design recommends a
separate `SectorSave` keyed by IDs only.

**Deferred:** generator is still single-shot; no in-place save/load loop
exists.

## 14. Faction filter / pin UI (§10.1)

GUI faction browser does not yet filter by kind/disposition, sort by power,
or let the player pin favorites. The data is already on `GeneratedFaction`
(kind, disposition, `power.total_projection()`).

**Deferred:** GUI panel work; would slot into [src/gui/editor/factions_panel.rs](src/gui/editor/factions_panel.rs).

## 15. Display importance scoring (§10.3) and aggregated minor presences

At sector zoom the renderer does not currently aggregate "Other Imperial",
"Criminal Networks", "Minor Xenos" pills, nor compute a `display_importance`
score per faction.

**Deferred:** depends on (8) and (10).

## 16. Hysteresis on control change (§11.3)

The doc recommends a 3-tick lag before flipping `visible_controller`. Not
applicable until conflict simulation (§5) lands.

## 17. Industrial dimension

`PowerProfile.industrial` is currently a 0.5× proxy of `economic` because the
per-presence dimension set does not include `industrial`. The design's §4.3
profile separates them.

**Deferred:** add an `industrial` field to `PresenceDimensions`, populate it
from `mechanicus` / `dark_mechanicum` / `imperial_guard` profiles, and
re-derive `PowerProfile.industrial` from it directly. Small follow-up; bumped
only because it shifts the JSON schema.

---

If you pick up this file, start with §10 (heatmaps) or §17 (industrial
dimension) — both are low-risk and unlock visible value. §3 (route control)
plus §8 (faction styles) are the next meaningful step toward the visual
grammar the design doc describes.
