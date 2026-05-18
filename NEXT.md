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

## 8. Per-faction style auto-generation (§9.6) — done

Core hue/glyph/border logic lives in [src/faction_style.rs](src/faction_style.rs)
(pure-data, no GUI deps). The GUI wraps it as `Color32` in
[src/gui/palette.rs](src/gui/palette.rs); the sector PNG exporter wraps it as
`Rgba<u8>` and tints each system hex by the dominant faction's
`FactionStyle.fill`. Toggleable via `outputs.bitmap.faction_fill` in
`sectorforge.toml` or `--no-faction-fill` on the CLI. The bitmap legend now
shows per-faction colour swatches sourced from the same palette as the GUI.

**Still deferred:** the per-system bitmap ([src/system_map.rs](src/system_map.rs))
does not draw any faction tint yet — it is a per-planet view, so faction
fill there would have to ride on world-level dominance rather than the
system-level dominant aggregate.

## 9. Continuous sector area layers (§9.3)

Voronoi cells, route-weighted influence fields, jump-route graph regions, and
soft-territory polygons are not produced. The hex map shows only system glyphs
and route lines.

**Deferred:** ties into (3) and (8). Would also need a new caching layer in
the renderer because per-pixel influence is expensive.

## 10. Influence heatmaps (§9.5) — done

Pure scoring lives in [src/heatmap.rs](src/heatmap.rs) (shared between GUI
and PNG). The GUI wrapper is [src/gui/heatmap.rs](src/gui/heatmap.rs), surfaced
via the **HEATMAP** dropdown in the sector view controls. Modes: `Off`,
`Control`, `Military`, `Trade`, `Industrial`, `Covert`, `Faith`, `Threat`,
`Intel`. Scores are normalised across the sector each frame; `Control` uses
the dominant faction's `FactionStyle.fill`, the other modes use a fixed
per-mode tint. The PNG exporter honours the same mode — set via
`outputs.bitmap.heatmap` in `sectorforge.toml`, the CLI flag `--heatmap`, or
the GUI export which inherits the current sector-view selection.

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

## 14. Faction filter / pin UI (§10.1) — done

Implemented in
[src/gui/editor/factions_panel.rs](src/gui/editor/factions_panel.rs):
kind / disposition dropdown filters, sort by total power (asc/desc) or
name, and a star button per row to pin favourites to the top. Pin state
lives on `EditorState` (`faction_pinned: BTreeSet<String>`).

## 15. Display importance scoring (§10.3) and aggregated minor presences — done

Implemented in [src/importance.rs](src/importance.rs):
`display_importance(faction) = total_projection × √(1 + system_presence +
world_presence)`. `compute_display_buckets(sector, minor_fraction,
max_visible)` returns a ranked list mixing `DisplayBucket::Faction` entries
(top N) with `DisplayBucket::Aggregated` rollups grouped by `KindGroup`
("Other Imperial", "Minor Xenos", "Criminal Networks", etc.). The PNG
legend in [src/bitmap/mod.rs](src/bitmap/mod.rs) renders these buckets with
the same `FactionStyle.fill` swatches used on the map. Not yet wired into
the Markdown renderer or the GUI sector overlay.

## 16. Hysteresis on control change (§11.3)

The doc recommends a 3-tick lag before flipping `visible_controller`. Not
applicable until conflict simulation (§5) lands.

## 17. Industrial dimension — done

`PresenceDimensions.industrial` is now a first-class 0..=100 dimension
populated per-kind in [src/control.rs](src/control.rs) `kind_profile` (high
for Mechanicus / Forge / Titan / Votann / Tau, low for daemons / Tyranid /
xenos raiders, scaled by population like `economic`/`legitimacy`).
`PowerProfile.industrial` is derived from it directly rather than as a 0.5×
proxy of `economic`. JSON schema bump: presence dictionaries now include an
`industrial` field; the field is `#[serde(default)]` so older sector
JSONs still load.

---

If you pick up this file, the next meaningful step toward the visual
grammar in the design doc is §3 (route control) — heatmaps (§10), faction
styles (§8 in the GUI + bitmap), the filter/pin UI (§14), the industrial
dimension (§17), and display-importance bucketing (§15) have all landed.
Per-system bitmap faction tint (§8 inside [src/system_map.rs](src/system_map.rs))
remains as a smaller follow-up, and §15 is currently legend-only — wiring
the same buckets into the GUI sector-view overlay (and the Markdown
renderer) is a low-effort win once the visual treatment is decided.
