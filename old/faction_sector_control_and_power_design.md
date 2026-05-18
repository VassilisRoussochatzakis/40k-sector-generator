# Sector Faction Control and Power Model for a Rust Galaxy Map Application

## 1. Purpose

This document describes a Rust application design for depicting a sector of the galaxy populated by a very large faction catalog. It focuses on how the application should model faction control, faction power, planetary authority, system-wide dominance, interstellar projection, and graphical representation on a sector map.

The uploaded faction list is best treated as **static faction seed data**, not as a finished political simulation. Each entry gives the application a faction identity, a broad faction kind, a spawn/importance weight, a default political temperament, and preferences for world types, governments, and notable features. The runtime simulation should then convert those static entries into **sector-specific faction instances**, **presences**, **claims**, **assets**, **conflicts**, and **map overlays**.

## 2. Source Data Summary

The supplied TOML file contains **995 faction entries** across **32 faction kinds**.

The observed faction fields are:

```toml
[[factions]]
id = "..."
name = "..."
kind = "..."
weight = 1.0
default_disposition = "..."
preferred_world_types = ["..."]
preferred_governments = ["..."]
preferred_notable_features = ["..."]
```

### Faction kinds in the file

| Kind | Count | Map role | Suggested visual treatment |
| --- | --- | --- | --- |
| `adeptus_astartes` | 99 | Imperial elite military | Fortress-monastery icons, rapid-response reach, small but decisive expeditionary halos |
| `chaos_space_marine` | 61 | Chaos warbands | Jagged borders, corruption bloom, raid vectors, warband banners |
| `imperial_guard` | 53 | Imperial conventional military | Garrison shields, recruitment bars, front-line fortification bands |
| `aeldari` | 49 | Xenos / Aeldari | Webway-thread overlays, surgical presence markers, hidden-route glyphs |
| `minor_xenos` | 49 | Xenos / minor powers | Alien enclave shapes, uncertain intel hatching, distinct species glyphs |
| `tau` | 42 | Xenos / expansionist polity | Sphere-of-influence rings, client-state gradients, diplomatic route lines |
| `imperial` | 41 | Imperial civil authority | Strong administrative borders, tithe stamps, lawful sovereignty fills |
| `mechanicus` | 41 | Mechanicus / forge authority | Cog-rings, forge-node networks, relic/tech heat overlays |
| `ork` | 40 | Ork empires and Waaagh!s | Irregular green tide fronts, mob density splats, scrap-route arrows |
| `necron` | 38 | Ancient dynasties | Tomb-world awakening pulses, cold geometric borders, dormant/active layers |
| `criminal` | 36 | Underworld / smugglers | Dotted underlay, black-market route lines, port enclave pips |
| `tyranid` | 36 | Tyranid hive fleets | Biomass consumption front, tendril paths, stripped-world aftermath icons |
| `inquisition` | 33 | Imperial covert authority | Low-opacity seals, classified overlays, redaction stripes |
| `drukhari` | 31 | Xenos raiders | Raid-shadow arcs, slave-route spikes, temporary terror-control zones |
| `cult` | 30 | Heretical or local cults | Infiltration hatching, unrest cells, corruption pressure rings |
| `daemon` | 30 | Warp entities | Warp breach blooms, instability noise, reality-collapse contours |
| `genestealer_cult` | 30 | Hybrid insurgency | Infiltration growth stages, hidden brood nodes, uprising timers |
| `merchant` | 29 | Commercial powers | Trade-route thickness, freeport icons, market-share rings |
| `adepta_sororitas` | 29 | Imperial religious military | Shrine halos, faith-control auras, convent-fortress markers |
| `leagues_of_votann` | 28 | Kin holds and leagues | Holdfast hexes, contract routes, resource-extraction overlays |
| `traitor_guard` | 23 | Traitor conventional military | Occupied garrisons, mutiny risk, corrupted front-line bands |
| `rebel` | 21 | Separatists / insurgents | Contested population heat, uprising flags, administrative break lines |
| `imperial_knight` | 21 | Imperial knight houses | Noble heraldry shields, oath-route lines, household fief rings |
| `dark_mechanicum` | 20 | Dark Mechanicum | Blasphemous forge glyphs, scrap-code haze, corrupted manufactoria |
| `collegia_titanica` | 18 | Titan legions | Strategic engine icons, threat-radius rings, forge-tithe lanes |
| `chaos_knight` | 17 | Chaos knight houses | Dread household sigils, terror-radius rings, corrupted fief markers |
| `harlequin` | 17 | Aeldari Harlequins | Hidden performance-route threads, sudden-appearance markers, minimal persistent fill |
| `traitor_titan_legion` | 16 | Traitor titan legions | Catastrophic threat rings, corrupted engine icons, scorched-route lines |
| `talons_of_the_emperor` | 14 | Talons of the Emperor | Rare golden intervention sigils, custodial protection rings, black-site markers |
| `xenos` | 1 | Generic xenos threat | Hostile contact warning icons, uncertain boundaries, red-alert hatching |
| `grey_knights` | 1 | Daemon-hunting elite | Classified anti-daemon strike rings, sanctified breach seals, hidden presence |
| `deathwatch` | 1 | Xenos-hunting elite | Watch-station glyphs, interdiction arcs, xenos-threat targeting rings |

### Default dispositions

| Disposition | Count | Simulation meaning |
| --- | --- | --- |
| `hostile` | 383 | Direct threat, invasion, terror control, predatory expansion. |
| `zealous` | 145 | Ideological projection, crusades, faith/awe pressure. |
| `opportunistic` | 135 | Commercial, mercenary, pirate, or flexible influence. |
| `lawful` | 113 | Stable legal control, bureaucracy, treaties, strong claims. |
| `secretive` | 111 | Hidden presence, classified missions, cells, black sites. |
| `insular` | 108 | Strong local control, limited external diplomacy, guarded routes. |

### Preferred world types

| Preferred world type | Occurrences |
| --- | --- |
| `FrontierWorld` | 523 |
| `DeathWorld` | 470 |
| `HiveWorld` | 363 |
| `IndustrialWorld` | 275 |
| `ForgeWorld` | 260 |
| `XenosWorld` | 257 |
| `BastionWorld` | 248 |
| `ShrineWorld` | 141 |
| `Orbital` | 133 |
| `AgriWorld` | 86 |
| `PenalWorld` | 80 |
| `ResearchStation` | 77 |
| `FeudalWorld` | 68 |

### Preferred governments

| Preferred government | Occurrences |
| --- | --- |
| `Warlords` | 295 |
| `XenosOverlords` | 250 |
| `MilitaryGovernor` | 246 |
| `Demagogue` | 218 |
| `GuildsCombine` | 123 |
| `MechanicusForgeLord` | 116 |
| `Megacorporations` | 99 |
| `MagistrateCouncil` | 92 |
| `RevolutionaryJunta` | 74 |
| `EcclesiarchicalAppointee` | 69 |
| `ExploratorAuthority` | 41 |
| `LocalReligiousAuthorities` | 30 |
| `RogueTraderDynasty` | 29 |

### Most common notable feature preferences

| Preferred notable feature | Occurrences |
| --- | --- |
| `WarZone` | 415 |
| `HostileXenos` | 274 |
| `ImportantShrine` | 222 |
| `XenosInfiltrators` | 219 |
| `DaemonicCorruption` | 198 |
| `ScholaProgenium` | 169 |
| `Quarantined` | 161 |
| `XenoRuins` | 154 |
| `ForbiddenTech` | 140 |
| `ArchaeotechRuins` | 107 |
| `TradeHub` | 99 |
| `TheSilentTrade` | 96 |
| `PoliceState` | 92 |
| `PopularUprising` | 81 |
| `CivilWar` | 80 |
| `Freeport` | 65 |
| `WitchHunt` | 64 |
| `LocalTech` | 62 |
| `MajorSpaceyard` | 55 |
| `Prosperous` | 43 |

### Weight interpretation

Observed `weight` values range from **0.6** to **10.0**, with an average of **1.35** and a median of **1.2**.

Recommended interpretation:

- `weight` is not raw military strength.
- Use it as a **spawn prior**, **strategic significance hint**, and **default prominence multiplier**.
- A high-weight faction should be more likely to appear, be politically legible, or own major assets.
- A low-weight faction can still become powerful if it controls valuable worlds, routes, relics, fleets, or hidden networks.

## 3. Core Design Principle: Separate Presence, Control, Claim, and Power

Do not store a single boolean like `controlled_by`. A faction can be present without ruling, rule legally without military control, dominate trade without governing, occupy orbit while losing the surface, or secretly infiltrate a planet while the map still displays another sovereign.

Use four separate concepts.

### 3.1 Presence

Presence means the faction has people, bases, ships, agents, cult cells, embassies, temples, tomb complexes, nests, fleets, or commercial interests in a place.

Examples:

- The Inquisition has a hidden cell on a hive world.
- The Adeptus Mechanicus owns an orbital research station.
- A Genestealer Cult controls three underhive districts but not the planetary government.
- A Rogue Trader compact dominates local commerce but does not govern.
- A Tyranid splinter fleet is in-system but has not landed yet.

### 3.2 Control

Control means the faction can reliably make things happen there.

Control is multi-dimensional:

- **Administrative control:** laws, taxes, bureaucratic authority.
- **Military control:** garrisons, fleets, orbital batteries, blockade power.
- **Economic control:** ports, contracts, manufactoria, supply chains, trade monopolies.
- **Ideological control:** faith, propaganda, cult loyalty, doctrine, legitimacy.
- **Covert control:** blackmail, sleeper cells, infiltration, secret police, hidden cults.
- **Orbital control:** stations, fleet pickets, void superiority, shipyards.
- **Logistical control:** ability to reinforce, extract, supply, and communicate.

### 3.3 Claim

Claim is what the faction says it owns or has a right to influence.

Claims can be:

- Legal claims.
- Hereditary claims.
- Religious claims.
- Treaty claims.
- Occupation claims.
- Ancient tomb-world claims.
- Crusade mandates.
- Commercial charters.
- Secret inquisitorial writs.
- Purely predatory hunting grounds.

Claims are excellent for **map borders**, while control is better for **fills and overlays**.

### 3.4 Power

Power is a faction's ability to project influence beyond the places it already controls.

Power should be derived from assets and relationships:

- Worlds controlled.
- Population.
- Industry.
- Fleet tonnage.
- armies and garrisons.
- Titan/knight/elite formations.
- Trade income.
- Warp route access.
- Diplomatic legitimacy.
- Covert network reach.
- Religious/cultural authority.
- Special faction logic, such as Tyranid biomass or Necron awakening level.

Power changes slowly unless an event destroys a capital, fleet, forge, route, or legitimacy base.

## 4. Recommended Rust Data Model

The application should have two layers of faction data:

1. **Static catalog data** loaded from TOML.
2. **Runtime sector state** generated or simulated for a specific campaign/sector.

### 4.1 Static faction catalog

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct FactionCatalog {
    pub factions: Vec<FactionDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FactionDef {
    pub id: String,
    pub name: String,
    pub kind: FactionKindId,
    pub weight: f32,
    pub default_disposition: Disposition,
    #[serde(default)]
    pub preferred_world_types: Vec<WorldType>,
    #[serde(default)]
    pub preferred_governments: Vec<GovernmentType>,
    #[serde(default)]
    pub preferred_notable_features: Vec<NotableFeature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct FactionKindId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Lawful,
    Hostile,
    Secretive,
    Zealous,
    Insular,
    Opportunistic,
}
```

Use string-backed IDs for `kind` instead of a closed enum if you want mod support. A strict enum is convenient during early development, but this file already contains dozens of kinds, and future expansions will likely add more.

### 4.2 Runtime faction instance

A static faction definition becomes a runtime faction instance only if it appears in the generated sector.

```rust
#[derive(Debug, Clone)]
pub struct FactionInstance {
    pub id: FactionId,
    pub def_id: String,
    pub display_name: String,
    pub kind: FactionKindId,
    pub disposition: Disposition,

    pub home_node: Option<ControlNodeId>,
    pub capital_node: Option<ControlNodeId>,

    pub strategic_power: PowerProfile,
    pub doctrine: FactionDoctrine,
    pub diplomacy: DiplomacyState,
    pub style: FactionStyle,

    pub visibility: IntelVisibility,
    pub active: bool,
}
```

### 4.3 Power profile

```rust
#[derive(Debug, Clone, Default)]
pub struct PowerProfile {
    pub administrative: f32,
    pub military: f32,
    pub naval: f32,
    pub economic: f32,
    pub industrial: f32,
    pub ideological: f32,
    pub covert: f32,
    pub logistical: f32,
    pub xenotech_or_warp: f32,
    pub legitimacy: f32,
}

impl PowerProfile {
    pub fn total_projection(&self) -> f32 {
        self.administrative * 0.8
            + self.military * 1.1
            + self.naval * 1.2
            + self.economic * 0.9
            + self.industrial * 0.9
            + self.ideological * 0.7
            + self.covert * 0.7
            + self.logistical * 1.0
            + self.xenotech_or_warp * 0.8
            + self.legitimacy * 0.5
    }
}
```

The weights above are not universal. They are a default model. For example, Tyranids should weigh biomass, fleet presence, and consumption momentum more heavily than legitimacy or administration. The Inquisition should weigh covert and legitimacy more than population or taxation. Merchant factions should weigh economic and logistical factors.

### 4.4 Control nodes

Everything that can be controlled should be represented as a node.

```rust
#[derive(Debug, Clone)]
pub enum ControlNodeKind {
    Sector,
    Subsector,
    StarSystem,
    Planet,
    Moon,
    OrbitalStation,
    AsteroidBelt,
    WarpGate,
    JumpPoint,
    TradeHub,
    SurfaceRegion,
    Settlement,
    HiveSpire,
    Underhive,
    Manufactorum,
    Shrine,
    TombComplex,
    WebwayGate,
    SpaceHulk,
}

#[derive(Debug, Clone)]
pub struct ControlNode {
    pub id: ControlNodeId,
    pub name: String,
    pub kind: ControlNodeKind,
    pub parent: Option<ControlNodeId>,
    pub children: Vec<ControlNodeId>,

    pub world_type: Option<WorldType>,
    pub government: Option<GovernmentType>,
    pub features: Vec<NotableFeature>,

    pub position_sector: Vec2,
    pub position_system: Option<Vec2>,

    pub strategic_value: StrategicValue,
    pub presences: Vec<FactionPresence>,
    pub claims: Vec<FactionClaim>,
    pub conflict: Option<ConflictState>,
    pub intel: IntelState,
}
```

A star system is not just one control node. It should contain child nodes for planets, moons, stations, routes, jump points, belts, and major surface regions. This allows one faction to control the main world, another to control the outer stations, and a third to control a hidden base.

### 4.5 Faction presence on a node

```rust
#[derive(Debug, Clone)]
pub struct FactionPresence {
    pub faction_id: FactionId,

    /// 0..100 per dimension
    pub admin: f32,
    pub military: f32,
    pub orbital: f32,
    pub economic: f32,
    pub ideological: f32,
    pub covert: f32,
    pub logistics: f32,

    /// 0..100; how accepted the faction is by locals and peer authorities.
    pub legitimacy: f32,

    /// 0..100; how visible the presence is to the player.
    pub visibility: f32,

    /// Used for hidden cults, sleeper cells, dormant tombs, or unconfirmed fleets.
    pub confirmed: bool,

    pub stance: LocalStance,
    pub last_changed_tick: u64,
}
```

### 4.6 Claim model

```rust
#[derive(Debug, Clone)]
pub struct FactionClaim {
    pub faction_id: FactionId,
    pub claim_type: ClaimType,
    pub strength: f32,       // 0..100
    pub recognized_by: Vec<FactionId>,
    pub contested_by: Vec<FactionId>,
    pub expires_tick: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum ClaimType {
    LegalSovereignty,
    ImperialMandate,
    TreatyRight,
    ReligiousMandate,
    DynasticRight,
    CommercialCharter,
    MilitaryOccupation,
    AncientDomain,
    HuntingGround,
    CovertWrit,
    Rebellion,
}
```

Claims should drive **border outlines** and political tooltips. Control should drive the actual fill color, occupation bands, and warning overlays.

## 5. Control Score and Dominance Classification

Each node should compute a control summary from all faction presences.

### 5.1 Local control score

A useful default formula:

```text
local_control =
    admin        * 0.22 +
    military     * 0.20 +
    orbital      * 0.12 +
    economic     * 0.14 +
    ideological  * 0.10 +
    covert       * 0.08 +
    logistics    * 0.08 +
    legitimacy   * 0.06
```

This is intentionally not only military. A faction with soldiers but no administration may occupy a world, but it should not feel like a stable government. A faction with trade dominance but no garrison should feel influential but vulnerable. A faction with high covert control may be hidden until an uprising.

### 5.2 Dominance states

| Control score | State | Meaning | Map depiction |
|---:|---|---|---|
| 0-9 | Rumored | Scattered rumors, no stable presence | Tiny unknown marker or no display unless intel layer is active |
| 10-24 | Presence | Agents, merchants, embassies, raiders, cells, shrine missions | Small faction pip or thin ring |
| 25-44 | Influence | Meaningful leverage but not rule | Partial halo, faction chip, light heatmap |
| 45-59 | Contested | Could plausibly seize or lose control | Split fill, striped border, conflict icon |
| 60-79 | Controlled | De facto local control | Dominant fill and normal border |
| 80-100 | Stronghold | Deeply entrenched, hard to dislodge | Thick border, capital/fortress glyph, high-saturation fill |

### 5.3 Multiple winners

A node can have several simultaneous winners:

- **Sovereign:** strongest legal or administrative claimant.
- **Occupier:** strongest military/orbital controller.
- **Economic hegemon:** strongest economic presence.
- **Popular authority:** strongest ideological/legitimacy presence.
- **Hidden master:** strongest covert presence.
- **Map owner:** the faction shown as dominant at the current zoom/layer.

Tooltips should display all of these separately.

Example:

```text
World: Karthax Primus
Sovereign: Imperial Administration
Occupier: Traitor Guard
Orbital Controller: Imperial Navy
Economic Hegemon: Free Trader Compact
Hidden Master: Genestealer Cult
Status: Blockaded / Insurgency / Contested
```

This produces a much richer sector simulation than a single owner field.

## 6. Hierarchical Control: Surface, Planet, Orbit, System, Subsector, Sector

### 6.1 Surface-level control

A planet should be divided into important regions, not necessarily a full tactical map.

Good region types:

- Capital city.
- Major hive.
- Underhive.
- Polar defense network.
- Orbital elevator.
- Shrine continent.
- Forge complex.
- Agri-belt.
- Promethium field.
- Wasteland.
- Tomb complex.
- Xenos enclave.
- Rebel-held mountains.
- Voidport.
- Astropathic relay.
- PDF fortress.
- Ecclesiarchy cathedral.
- Manufactorum district.

Surface regions feed into planetary control.

```text
planetary_admin = weighted average of admin across capital, bureaucracy, major settlements
planetary_military = weighted average of garrisons, fortresses, battlefronts
planetary_orbital = derived mostly from orbitals, stations, fleets, batteries
planetary_economic = ports + industry + trade + agriculture + mines
planetary_ideological = shrines + cults + propaganda + education
planetary_covert = underhives + infiltrated institutions + secret bases
```

Graphical depiction:

- At high zoom: surface regions can be colored individually.
- At system zoom: a planet icon can have colored segments or rings.
- At sector zoom: only the dominant faction, conflict state, and strategic value should be visible.

### 6.2 Planet-level control

Each planet should display:

- Dominant visible controller.
- Legal claimant.
- Military occupier.
- Hidden influence warnings, if known.
- Stability.
- Strategic value.
- Blockade/siege status.
- Uprising/corruption/infestation state.
- Tithe/compliance/tribute state if relevant.

Planet icon recommendation:

```text
Center fill      = de facto controller
Outer ring       = legal sovereign or primary claim
Second ring      = orbital controller
Small side pips  = top 1-3 secondary presences
Hatching         = contested / infiltrated / corrupted / quarantined
Glow             = strategic importance or active war
```

### 6.3 Orbital control

Orbital control should be its own dimension. A faction may hold orbit while another controls the surface.

Track:

- Orbital stations.
- Shipyards.
- Defense platforms.
- Blockade fleets.
- Patrol squadrons.
- Minefields.
- Jump point pickets.
- Sensor nets.
- Astropathic relays.
- Warp gates or webway gates.

Orbital control affects:

- Reinforcement speed.
- Trade access.
- Siege pressure.
- Escape routes.
- Bombardment risk.
- Visibility/intel.
- Invasion feasibility.

Graphical depiction:

- Planet outer ring: orbital owner.
- Lock icon: blockade.
- Anchor icon: station/shipyard.
- Sword icon: battlefleet in orbit.
- Dashed orbital ring: contested voidspace.
- Flickering ring: unstable or intermittent control.

### 6.4 System-level control

A star system aggregates child nodes, but it should not erase internal conflict.

Compute the system's display state from:

- Main inhabited planet control.
- Orbital control.
- Capital/station control.
- Jump point control.
- Route safety.
- Total strategic value under each faction.
- Conflict intensity.

A system can be classified as:

| System state | Criteria | Visual |
|---|---|---|
| Pacified | One faction has strong control across most valuable nodes | Clean filled star/system disc |
| Fragmented | Different factions hold different planets/stations | Pie-split system marker |
| Blockaded | Orbital controller hostile to main planet controller | Ring lock, route warning |
| Warzone | High military presence from hostile factions | Pulsing red/orange conflict halo |
| Infiltrated | High covert presence but low visible control | Subtle hatch or hidden intel overlay |
| Quarantined | Feature or policy prevents normal route/trade | Hazard ring and route suppression |
| Uncharted | Low intel | Desaturated marker, uncertainty fog |

### 6.5 Interstellar control

Interstellar control is not continuous territory in deep space. It is control over:

- Routes.
- Jump points.
- Refueling nodes.
- Astropathic relays.
- Safe harbors.
- Patrol ranges.
- Trade convoys.
- Navy stations.
- Pirate corridors.
- Webway paths.
- Hive fleet tendrils.
- Waaagh! migration fronts.
- Crusade routes.

Represent interstellar power as **graph control** rather than pure area ownership.

```rust
#[derive(Debug, Clone)]
pub struct RouteEdge {
    pub id: RouteId,
    pub from: ControlNodeId,
    pub to: ControlNodeId,
    pub route_kind: RouteKind,
    pub distance: f32,
    pub stability: f32,
    pub travel_risk: f32,
    pub controllers: Vec<RouteControl>,
}

#[derive(Debug, Clone)]
pub struct RouteControl {
    pub faction_id: FactionId,
    pub patrol: f32,
    pub toll: f32,
    pub interdiction: f32,
    pub piracy: f32,
    pub secrecy: f32,
    pub confidence: f32,
}
```

Graphical depiction:

- Route color: primary controller or threat.
- Route width: traffic/logistical importance.
- Dash pattern: unstable, hidden, or contested.
- Glow: high strategic traffic.
- Crossbars: interdiction/blockade.
- Moving particles: active convoys, fleets, migration, or tendrils.
- Hidden route layer: webway, smuggling, black ship routes, secret Inquisition paths.

## 7. Faction Power Projection

### 7.1 Sources of power

Give every runtime faction a power budget generated from controlled nodes and assets.

```text
administrative_power =
    sum(node.population_value * admin_control * legitimacy)

military_power =
    sum(garrison_strength + mobile_armies + fortifications)

naval_power =
    sum(fleet_tonnage + shipyard_capacity + patrol_bases + orbital_batteries)

economic_power =
    sum(trade_value + port_value + contracts + tribute)

industrial_power =
    sum(forge_output + manufactoria + mining + shipbuilding)

ideological_power =
    sum(shrine_value + culture + propaganda + cult_presence)

covert_power =
    sum(cells + black_sites + infiltrated_governments + hidden assets)

logistical_power =
    sum(route_access + depot_capacity + stable warp lanes + relay control)
```

### 7.2 Projection falloff

A faction's power should weaken with distance unless routes, fleets, gates, or covert networks support it.

```text
projected_power_at_node =
    source_power
    * route_quality
    * logistics_modifier
    * doctrine_modifier
    * intel_modifier
    / (1.0 + distance * distance_falloff)
```

Different kinds should use different projection logic:

- **Imperial civil authority:** strong where administrative networks, tithes, and legal claims exist; weak in frontier space.
- **Mechanicus:** strong near forge worlds, research stations, spaceyards, archaeotech, and forbidden tech.
- **Merchants/criminals:** strong along trade routes, freeports, ports, and prosperous worlds.
- **Adeptus Astartes / elite strike forces:** low administrative control, high military shock projection around fleets and fortress nodes.
- **Inquisition:** low visible control, high covert reach, strong around outposts, witch hunts, quarantines, and corruption events.
- **Orks:** power grows from local conflict and mob density, with fronts expanding irregularly.
- **Tyranids:** power is fleet-tendril and biomass based; worlds behind the front may become consumed rather than governed.
- **Necrons:** dormant claims can become active suddenly; control may jump when tomb complexes awaken.
- **Aeldari / Harlequins / Drukhari:** use hidden route networks and intermittent control rather than continuous borders.
- **Tau:** sphere-of-influence expansion, client states, diplomacy, and supply routes.
- **Chaos / daemons / cults:** corruption, warp instability, cult networks, military occupation, and ritual sites.
- **Genestealer cults:** hidden infiltration, uprising thresholds, and eventual open control.
- **Leagues of Votann:** strong around holds, contracts, resource extraction, and defensible route networks.

## 8. Using the Faction Preferences During Generation

The TOML preferences should guide where factions appear.

### 8.1 Placement scoring

For every candidate faction and candidate node:

```text
placement_score =
    faction.weight
    * world_type_match
    * government_match
    * notable_feature_match
    * distance_from_existing_presence
    * sector_story_bias
    * rarity_modifier
```

Suggested match values:

| Match type | Multiplier |
|---|---:|
| Strong preferred world type | 2.0 |
| Weak compatible world type | 1.2 |
| No match | 1.0 |
| Bad fit | 0.3 |
| Preferred government match | 1.6 |
| Preferred notable feature match | 1.8 |
| Multiple notable feature matches | 2.2+ |
| Already saturated by same kind | 0.4 |
| Story-critical placement | 2.0-5.0 |

### 8.2 Role generation

After placement, assign a local role:

```rust
pub enum LocalFactionRole {
    Sovereign,
    Governor,
    Occupier,
    Garrison,
    FleetPresence,
    ShrineAuthority,
    ForgeAuthority,
    MerchantLeague,
    CriminalSyndicate,
    RebelCell,
    CultCell,
    HiddenInfiltrator,
    TombClaimant,
    Raider,
    Protector,
    Patron,
    Expedition,
    QuarantineAuthority,
    ClientStateSponsor,
}
```

A faction's kind and disposition should bias the role. For example:

- `imperial` + `lawful` -> sovereign, governor, tithe authority.
- `mechanicus` + `insular` -> forge authority, research enclave, tech monopoly.
- `merchant` + `opportunistic` -> trade hegemon, freeport patron.
- `inquisition` + `secretive` -> black site, cell, quarantine authority.
- `rebel` + `hostile` -> rebel cell, liberated district, breakaway government.
- `tyranid` + `hostile` -> fleet tendril, infestation, consumed biomass.
- `criminal` + `opportunistic` -> smuggling ring, pirate haven, underworld governor.
- `genestealer_cult` + `hostile` -> hidden infiltrator, uprising seed, brood-held underhive.

## 9. Graphical Representation

## 9.1 Core visual grammar

Use a consistent visual language:

| Visual channel | Meaning |
|---|---|
| Fill color | De facto visible controller |
| Outer border | Legal claim or recognized sovereignty |
| Inner ring | Orbital or military controller |
| Halo | Influence radius or ideological pressure |
| Hatching | Contested, infiltrated, corrupted, quarantined, or uncertain |
| Dotted underline | Covert or criminal presence |
| Route line color | Route controller or main route threat |
| Route line width | Traffic, supply importance, or fleet capacity |
| Route dash pattern | Hidden, unstable, blockaded, or unreliable route |
| Icon/glyph | Faction kind or special asset |
| Opacity | Intel confidence or strength |
| Pulse animation | Active conflict, invasion, ritual, uprising, or fleet movement |
| Noise/distortion | Warp corruption, daemonic instability, or sensor uncertainty |

### 9.2 Sector map levels of detail

At different zoom levels, display different abstractions.

#### Sector zoom

Show:

- Systems as nodes.
- Major route graph.
- High-level territorial influence.
- Warzones.
- Quarantines.
- Fleet fronts.
- Sector capitals and major strongholds.

Avoid:

- Showing all 995 factions at once.
- Showing more than 1-3 faction markers per system.
- Rendering every hidden cell by default.

Recommended system marker at sector zoom:

```text
[system disc]
center fill = dominant map owner
outer ring = sovereign claim
small top-right pip = active conflict
small bottom-left pip = hidden threat known
halo = influence pressure
route lines = interstellar connectivity
```

#### Subsector zoom

Show:

- More secondary presences.
- Route control.
- Patrol zones.
- Trade intensity.
- Regional front lines.
- Local hegemons.
- Contested borders.

#### System zoom

Show:

- Star, planets, moons, belts, stations, jump points.
- Planetary/orbital split control.
- Blockades.
- Fleets.
- Stations and defense platforms.
- Local route exits.

#### Planet zoom

Show:

- Major surface regions.
- City/hive/continent-level control.
- Rebel-held zones.
- Infiltrated underhives.
- Shrines, manufactoria, spaceports, tombs, xenos sites.
- Stability and unrest.

### 9.3 Area borders

For continuous-looking sector control, create soft territories using:

- Voronoi cells around systems.
- Route-weighted influence fields.
- Hex grids.
- Jump-route graph regions.
- Convex hulls around stronghold clusters.
- Influence contours.

Do not let the area fill imply empty deep-space sovereignty too strongly. In a grimdark sector map, the meaningful control is over systems and routes.

Recommended area layers:

1. **Sovereignty layer:** recognized claims and administrative territories.
2. **Military layer:** occupied systems, fronts, fleet patrol zones.
3. **Economic layer:** trade dominance, freeports, merchant compacts.
4. **Religious/ideological layer:** shrine influence, cult spread, propaganda.
5. **Covert layer:** hidden cells, Inquisition influence, criminal networks.
6. **Threat layer:** xenos swarms, Waaagh! fronts, warp storms, raids.
7. **Intel layer:** confidence, uncertainty, false reports.

### 9.4 Split control symbols

A node with multiple major factions should not be painted as a single owner.

Use:

- Pie wedges for top controllers.
- Concentric rings for dimensions.
- Stripes for contested occupation.
- Side pips for secondary factions.
- Tooltip list for full faction presence.
- Alert icon for hidden high-threat presences.

Example visual convention:

```text
Planet center       = surface controller
Outer ring          = sovereign claim
Orbit ring          = void/orbital controller
Left pip            = economic hegemon
Right pip           = covert/hidden known presence
Bottom pip          = ideological authority
Hatching            = contested or unstable
```

### 9.5 Influence heatmaps

Influence heatmaps should be calculated per faction and per dimension. Do not render all at once.

Possible heatmap modes:

- `Control`: where a faction actually rules.
- `Military`: where its armies and fleets can strike.
- `Trade`: where it can shape markets and supply.
- `Covert`: where hidden networks are likely active.
- `Faith`: ideological or religious influence.
- `Threat`: hostile expansion pressure.
- `Intel`: what the player knows.

Heatmap value at a point:

```text
heat = max over nearby nodes and routes:
    node_presence_dimension
    * node_strategic_value
    * route_connectivity
    * falloff(distance)
    * visibility
```

### 9.6 Faction style generation

Do not manually assign a unique color and icon to 995 factions. Instead:

1. Assign a base palette and glyph family by `kind`.
2. Vary hue, pattern, icon border, or banner detail by `id`.
3. Reserve intense colors for active map layers.
4. Use line/pattern differences for accessibility.
5. Store user-overridable styles in a separate config file.

```rust
#[derive(Debug, Clone)]
pub struct FactionStyle {
    pub base_color: Rgba,
    pub border_color: Rgba,
    pub accent_color: Rgba,
    pub glyph: GlyphId,
    pub pattern: PatternId,
    pub route_style: RouteStyle,
    pub hidden_style: HiddenStyle,
}
```

Suggested mapping:

- Kind controls base color family.
- Disposition controls border shape or animation.
- Control dimension controls ring/fill position.
- Intelligence confidence controls opacity.
- Conflict intensity controls pulse frequency.

Disposition styling:

| Disposition | Suggested visual behavior |
|---|---|
| `lawful` | Clean borders, solid lines, administrative stamps |
| `hostile` | Aggressive outlines, warning halos, pulsing conflict markers |
| `secretive` | Low opacity, redacted labels, dotted hidden underlays |
| `zealous` | Radiant halos, shrine/creed glyphs, high-contrast banners |
| `insular` | Thick local borders, low route emphasis, fortress markers |
| `opportunistic` | Route emphasis, market pips, shifting contract overlays |

## 10. Handling a Massive Faction List Without UI Overload

A sector map with 995 possible factions needs aggressive filtering and aggregation.

### 10.1 Faction browser

Include a side panel with:

- Search by name/id.
- Filter by kind.
- Filter by disposition.
- Filter by active/inactive.
- Filter by known/unknown.
- Filter by currently visible map area.
- Sort by total power, number of presences, threat, control, or story relevance.
- Pin favorite factions.

### 10.2 Map aggregation

At sector zoom:

- Show dominant controller.
- Show only active conflicts and major threats.
- Collapse related minor presences into "Other Imperial", "Criminal Networks", "Minor Xenos", etc.
- Show exact factions on hover or selection.

At system zoom:

- Show top 3-5 factions by local relevance.
- Hide tiny presences unless the selected layer requires them.

At planet zoom:

- Show all local presences above a threshold.
- Show hidden presences only if discovered.

### 10.3 Importance score

For UI selection, compute a display importance score:

```text
display_importance =
    local_control_score * 0.35
    + local_power_projection * 0.20
    + conflict_relevance * 0.20
    + strategic_node_value * 0.15
    + player_interest_pin * 0.10
```

Then display only the top N factions for the current view.

## 11. Conflict, Stability, and Control Change

### 11.1 Stability

Every node should track stability separately from control.

```rust
pub struct StabilityState {
    pub public_order: f32,
    pub economic_health: f32,
    pub legitimacy: f32,
    pub corruption: f32,
    pub fear: f32,
    pub rebellion_risk: f32,
    pub xenos_threat: f32,
    pub warp_instability: f32,
    pub famine_or_resource_stress: f32,
}
```

A world can be strongly controlled and still unstable. A terror regime may score high military control but low legitimacy, creating long-term revolt risk.

### 11.2 Conflict state

```rust
pub struct ConflictState {
    pub participants: Vec<FactionId>,
    pub conflict_kind: ConflictKind,
    pub intensity: f32,
    pub momentum: Vec<(FactionId, f32)>,
    pub started_tick: u64,
    pub visible_to_player: bool,
}

pub enum ConflictKind {
    BorderWar,
    CivilWar,
    Invasion,
    Blockade,
    Insurgency,
    WitchHunt,
    Purge,
    Crusade,
    Waaagh,
    HiveFleetConsumption,
    TombAwakening,
    DaemonicIncursion,
    TradeWar,
    ColdWar,
    ShadowWar,
}
```

### 11.3 Control change

Control should change through events and gradual pressure.

Sources of control change:

- Battle result.
- Siege attrition.
- Uprising.
- Coup.
- Treaty.
- Trade dependency.
- Religious conversion.
- Covert infiltration.
- Quarantine.
- Exterminatus or world devastation.
- Fleet withdrawal.
- Supply collapse.
- Discovery of hidden faction.
- Tomb awakening.
- Genestealer uprising.
- Warp storm isolation.

Use hysteresis so map control does not flicker every tick:

```text
if challenger_score > current_score + 15 for 3 consecutive ticks:
    node.visible_controller = challenger
```

For contested systems, keep the contested state until one side has a clear lead and conflict intensity drops.

## 12. Intel and Fog of War

A grimdark sector map should not always tell the truth.

Track the distinction between:

- Actual simulation state.
- Last known state.
- Rumored state.
- Propaganda state.
- Classified state.
- Player-discovered state.

```rust
pub struct IntelState {
    pub confidence: f32,        // 0..100
    pub last_verified_tick: u64,
    pub source: IntelSource,
    pub hidden_presences_known: Vec<FactionId>,
    pub suspected_presences: Vec<FactionId>,
}

pub enum IntelSource {
    DirectSensor,
    SpyNetwork,
    AstropathicReport,
    MerchantRumor,
    InquisitionSeal,
    RefugeeReport,
    EnemyBroadcast,
    AncientRecord,
    PlayerVisit,
}
```

Graphical depiction:

- Low confidence: desaturated, blurred, noisy, or question-marked.
- Old report: faded label with timestamp.
- Suspected hidden faction: dashed marker.
- Classified faction: redacted name until revealed.
- Conflicting reports: split tooltip with confidence values.

## 13. Rust Architecture

### 13.1 Recommended crates

Potential crate choices:

- `serde` + `toml` for loading the faction catalog.
- `petgraph` for system route graphs and path-based influence.
- `slotmap` or `generational-arena` for stable IDs.
- `bevy_ecs`, `hecs`, or `legion` if using an ECS architecture.
- `bevy` for a full Rust game/application stack.
- `egui` or `bevy_egui` for inspector panels and filters.
- `wgpu` if building a custom renderer.
- `rapier` is not necessary unless tactical physics are part of the application.

### 13.2 Data ownership

Keep simulation and rendering separate.

```text
FactionCatalog
    static definitions loaded from TOML

SectorState
    systems, nodes, routes, factions, presences, claims, conflicts

SimulationSystems
    update control, conflicts, diplomacy, economy, intel

RenderWorld
    cached geometry, styles, labels, interaction hitboxes

UIState
    selected node, selected faction, visible layers, search filters
```

### 13.3 ECS component approach

If using Bevy:

```rust
#[derive(Component)]
pub struct MapNodeEntity {
    pub node_id: ControlNodeId,
}

#[derive(Component)]
pub struct RenderedFactionControl {
    pub visible_controller: Option<FactionId>,
    pub sovereign_claimant: Option<FactionId>,
    pub orbital_controller: Option<FactionId>,
    pub top_secondary: Vec<FactionId>,
    pub contested: bool,
    pub intel_confidence: f32,
}

#[derive(Component)]
pub struct RouteEntity {
    pub route_id: RouteId,
}
```

Simulation runs on IDs and data resources. Rendering systems convert summaries into mesh/material/label changes.

### 13.4 Caching

Do not recompute everything every frame.

Cache:

- Node control summaries.
- Top factions per node.
- Influence heatmap tiles.
- Route controller summaries.
- Label visibility.
- Faction display importance.
- Current map layer meshes.

Invalidate caches when:

- A presence changes.
- A node's features change.
- A fleet moves.
- A route changes.
- Player layer/filter changes.
- Intel is updated.

## 14. Example Control Recalculation

```rust
pub fn recompute_node_control(node: &ControlNode) -> NodeControlSummary {
    let mut scores: Vec<(FactionId, f32)> = Vec::new();

    for p in &node.presences {
        let score =
            p.admin * 0.22
            + p.military * 0.20
            + p.orbital * 0.12
            + p.economic * 0.14
            + p.ideological * 0.10
            + p.covert * 0.08
            + p.logistics * 0.08
            + p.legitimacy * 0.06;

        scores.push((p.faction_id, score));
    }

    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let top = scores.get(0).copied();
    let second = scores.get(1).copied();

    let contested = match (top, second) {
        (Some((_, a)), Some((_, b))) => a - b < 15.0 && a > 35.0,
        _ => false,
    };

    NodeControlSummary {
        dominant: top.map(|x| x.0),
        dominant_score: top.map(|x| x.1).unwrap_or(0.0),
        contested,
        top_factions: scores.into_iter().take(5).collect(),
    }
}
```

## 15. Example Map Tooltip

```text
System: Volund's Gate
Status: Fragmented / Blockaded
Sovereign Claim: Imperial Administration (72)
Visible Controller: Adeptus Mechanicus (61)
Orbital Controller: Battlefleet Detachment (68)
Economic Hegemon: Free Trader Compact (77)
Hidden/Suspected: Inquisition cell, criminal syndicate
Routes:
  - Thule Reach: Patrolled, low piracy
  - Ash Wound: Contested, high warp risk
Strategic Value:
  Industry 84, Naval 63, Faith 12, Xenos Risk 34
Recent Events:
  - Dockyard strike
  - Black ship arrival
  - Unconfirmed cult purge
```

## 16. Special Rules by Faction Archetype

### 16.1 Imperial civil factions

Imperial civil factions should often share sovereignty rather than behave as enemies. The Imperial Administration, Administratum-like groups, tithe authorities, courts, local governors, and similar bodies can overlap.

Represent them as a **governance stack**:

```text
Imperial Sovereignty
  - Sector authority
  - Planetary governor
  - Administratum/tithe authority
  - Arbites/legal enforcement
  - Ecclesiarchy legitimacy
  - Navy/Guard military protection
```

A world can be "Imperial" on the main map but still show internal competition between the Administratum, Ecclesiarchy, Mechanicus, noble houses, and Navy.

### 16.2 Mechanicus

Mechanicus control should often be enclave-based:

- Forge worlds.
- Research stations.
- Orbital shipyards.
- Tech shrines.
- Excavation zones.
- Archaeotech ruins.
- Forbidden technology sites.

They may have high industrial and technological control without controlling planetary civil law.

### 16.3 Inquisition and covert elites

Do not render Inquisition control as normal territory unless the user selects a classified/covert layer.

They should appear as:

- Black sites.
- Investigation zones.
- Quarantine seals.
- Witch hunt overlays.
- Sudden intervention events.
- Hidden presence in tooltips when discovered.

### 16.4 Space Marines, Grey Knights, Deathwatch, Talons

These factions are often not administrators. They project force through:

- Fortress-monasteries.
- Strike cruisers.
- Watch stations.
- Oathbound protectorates.
- Crusade routes.
- Classified intervention zones.

Represent them with high military projection, elite icons, and route/fleet reach rather than broad civil fills.

### 16.5 Merchants and criminals

Merchants and criminal factions are best represented through routes, ports, and market shares.

Map depiction:

- Route thickness for commerce.
- Freeport icons.
- Market share bars on planets.
- Dotted hidden smuggling lines.
- Contract influence halos.
- Blockade/trade-war alerts.

### 16.6 Rebels, cults, and genestealer cults

These should grow in stages:

1. Rumor.
2. Hidden cell.
3. Local influence.
4. District control.
5. Parallel government.
6. Open uprising.
7. Planetary seizure.
8. Interstellar spread or suppression.

Use hatching and pips before full fills. A rebel faction should rarely begin with clean borders unless it has already won a civil war.

### 16.7 Orks

Orks should be front-like and density-driven:

- Mob density.
- Boss control.
- Waaagh! momentum.
- Scrap economy.
- Captured ships.
- Raid vectors.
- Infighting risk.

Their borders should look rough, unstable, and expanding.

### 16.8 Tyranids

Tyranids do not "govern" in a normal way.

Track:

- Fleet tendrils.
- Biomass pressure.
- Infiltration/infestation.
- Consumption progress.
- Shadow-in-the-warp intensity.
- Consumed/dead worlds.

Map depiction:

- Tendril route lines.
- Consumption gradient.
- World status changing from inhabited to besieged to consumed.
- Suppressed communication routes near high shadow intensity.

### 16.9 Necrons

Necron control can be dormant.

Track:

- Dormant claim.
- Awakening level.
- Active tomb complexes.
- Dynasty command nodes.
- Scarab activity.
- Pylon/null-zone effects.

A tomb world may look Imperial until awakening reveals a stronger ancient claim.

### 16.10 Aeldari, Drukhari, and Harlequins

Use route secrecy and intermittent appearance.

- Aeldari: hidden webway influence, craftworld protection zones, surgical raids.
- Drukhari: raid arcs, terror zones, slave routes, temporary predation areas.
- Harlequins: minimal persistent territory, strong hidden-route mobility, event-driven appearances.

### 16.11 Tau

Tau-style powers should project stable spheres through diplomacy, client states, commerce, and garrisons.

Track:

- Sept/core control.
- Client state influence.
- Water-caste diplomacy.
- Trade dependency.
- Military protectorates.
- Ideological conversion.

Use smooth influence rings, client-state borders, and diplomatic route overlays.

### 16.12 Chaos, daemons, and traitor forces

Chaos control should include both normal occupation and metaphysical corruption.

Track:

- Military occupation.
- Corruption level.
- Cult density.
- Warp instability.
- Ritual sites.
- Daemonic manifestation probability.
- Traitor logistics.

Map depiction:

- Corruption bloom.
- Warp-noise texture.
- Ritual glyphs.
- Jagged front lines.
- Red/black hatching for unstable reality.
- Separate traitor military control from daemonic influence.

## 17. Save Data Format

Keep save data separate from the static faction file.

```ron
SectorSave(
    seed: 123456,
    tick: 987,
    active_factions: [...],
    nodes: [...],
    routes: [...],
    presences: [...],
    claims: [...],
    conflicts: [...],
    intel: [...],
)
```

Store IDs, not names. Names can change in localization, but IDs should remain stable.

## 18. Recommended Map Interaction Flow

1. Player opens sector map.
2. Sector view shows dominant powers, major fronts, trade routes, and alerts.
3. Player selects a system.
4. System panel shows control summary: sovereign, occupier, orbital controller, economic hegemon, hidden threats.
5. Player toggles layer: military, trade, faith, covert, threat, intel.
6. Player zooms into system.
7. Planets show ring-based control split.
8. Player selects planet.
9. Planet panel shows surface regions and faction presences.
10. Player opens faction panel.
11. Faction panel shows total power, controlled nodes, claims, enemies, allies, known assets, and projected influence.

## 19. Implementation Checklist

### Simulation

- [ ] Load faction catalog from TOML.
- [ ] Normalize faction kind and disposition IDs.
- [ ] Generate active sector factions from weights and story rules.
- [ ] Generate star systems, planets, route graph, governments, and features.
- [ ] Place faction presences using preferences.
- [ ] Create claims independently from control.
- [ ] Compute local control scores by dimension.
- [ ] Aggregate surface -> planet -> system -> subsector -> sector.
- [ ] Compute route control and interstellar projection.
- [ ] Simulate stability, conflict, diplomacy, and intel.
- [ ] Cache map display summaries.

### Rendering

- [ ] Assign faction styles by kind and ID.
- [ ] Render sector nodes and route graph.
- [ ] Render sovereignty, military, trade, covert, and threat layers.
- [ ] Render planet rings and pips for split control.
- [ ] Render contested states with hatching/animation.
- [ ] Render intel confidence through opacity/noise.
- [ ] Render tooltips with separate sovereign/occupier/economic/covert values.
- [ ] Add zoom-based level of detail.
- [ ] Add faction search, filters, and pins.
- [ ] Add legend explaining visual grammar.

### UX

- [ ] Never show all factions by default.
- [ ] Default to strategic relevance.
- [ ] Let users pin factions.
- [ ] Let users compare two or more factions.
- [ ] Let users toggle hidden/covert data only when available.
- [ ] Provide accessible patterns, not color alone.
- [ ] Use tooltips for explanation, not giant map labels.

## 20. Minimal Viable Version

A practical first version could implement:

1. Load the TOML faction catalog.
2. Generate 40-100 systems.
3. Generate 5-20 active factions from weights.
4. Assign each system 1 dominant faction and 0-3 secondary presences.
5. Track four dimensions only: military, economic, administrative, covert.
6. Render:
   - System fill = dominant controller.
   - Outer ring = legal claimant.
   - Small pips = secondary presences.
   - Route color = route controller.
   - Hatching = contested.
7. Add tooltip with full faction breakdown.
8. Add filters by kind and disposition.

Then expand to planetary regions, orbital control, detailed route logistics, hidden intel, and special faction rules.

## 21. Final Recommendation

The strongest design is a **layered control simulation over a graph of systems, planets, regions, and routes**. The faction list should be the identity catalog; actual politics should emerge from runtime `FactionPresence`, `FactionClaim`, `PowerProfile`, `ConflictState`, and `IntelState` records.

Graphically, the map should avoid a single-owner model. Use fills for visible control, borders for claims, rings for orbital/military control, pips for secondary presences, route styling for interstellar power, hatching for contested or hidden states, and opacity/noise for intelligence confidence.

This will let a Rust application depict not just who owns a star system, but **how** they own it, **where** their power comes from, **who challenges it**, and **what the player actually knows**.
