# BUILDER.md — A new user's walkthrough

This guide takes a brand-new user through `sectorforge-builder` from the very
first launch to a small, complete sector they can re-open and export. It walks
through each capability in the order you would naturally need it, with no
assumptions about which button to click or which tab to look at next.

The target is a **tiny 8 × 6 sector with 5 systems**. That is small enough that
every step is visible at a glance and every panel is exercised at least once,
but big enough that subsectors, routes, factions, regions and conflict all have
something to act on. Once you have done it the long way, the rest of the
builder will feel familiar even though you have not touched most of its tabs.

Throughout this document:

- A **tab** is one of the buttons in the strip across the top of the window
  (PROJECT, MAP, SYSTEM, WORLD, FACTIONS, …).
- A **section** is a collapsing header inside a tab. They are closed by default
  unless noted; click the header bar to open or close one.
- Coordinates are written `(q, r)` — the axial hex grid coordinates used by the
  sector. `(0, 0)` is the top-left hex.

If you are already comfortable with the broader pipeline, [GUIDE.md](GUIDE.md)
is the canonical engineering reference. This file is the new-user procedural
companion to it.

---

## 0. Before you start

### 0.1 Build the binary

From a terminal in the project root:

```bash
cargo build --release -p sectorforge-builder
```

The first build pulls in `egui` and a number of other crates, so expect it to
take a couple of minutes. Subsequent builds are fast.

### 0.2 Launch the builder

Two equivalent invocations:

```bash
cargo run --release -p sectorforge-builder
cargo run --release -p sectorforge-builder -- --help     # see CLI flags
```

You can pass `--project <path>` to open an existing project directly; for this
walkthrough leave it off so we start from the empty splash.

### 0.3 What you should see

A native desktop window with three obvious regions:

- A **horizontal tab strip** at the very top: `PROJECT MAP SYSTEM WORLD
  FACTIONS CONTROL REGIONS ROUTES SUBSECTORS ECONOMY RELATIONS HISTORY PERSONAE
  HOOKS SITES MISSIONS PROSE ANALYTICS INTERESTINGNESS SEARCH DIFF BRIEFING
  SEGMENTUM EXPORT`. There are 24 tabs total. Don't be alarmed — most of them
  are derived views over the same sector and you only need a handful for a
  basic build.
- The **active tab's panel** below the strip. On a fresh launch it shows the
  PROJECT tab.
- A **status footer** that surfaces validation, invariants, and the health pip
  (a small coloured dot — green = clean, yellow = warnings, red = error).

The active tab is highlighted in the strip; click any other tab to switch.

---

## 1. Create the project

A *project* is a directory on disk holding your sector state plus the data
catalogues (worlds, factions, regions, …) it draws from. We start by making
one.

### 1.1 Open the wizard

1. Make sure the **PROJECT** tab is selected.
2. Click **New project…** at the top of the tab.

A small modal dialog appears titled *New project* with five fields:

| Field        | Set it to        | Meaning                                            |
|--------------|------------------|----------------------------------------------------|
| Project id   | `tutorial-sector`| Used as the folder name and as the sector id.      |
| Title        | `Tutorial Sector`| Human-readable title shown in legends and exports. |
| Seed         | `walkthrough-1`  | Deterministic RNG seed. Any string works.          |
| Width        | `8`              | Hexes across.                                      |
| Height       | `6`              | Hexes down.                                        |

### 1.2 Pick a destination on disk

Click **Choose folder & create…**. The OS file picker opens. Navigate to a
folder you want the project to live next to (e.g. your `Documents` folder) and
confirm. The builder will create a subfolder named after the project id
(`tutorial-sector/`) inside that folder.

If the create succeeds the modal closes, the title bar updates to your project
title, and the **Tree** section in PROJECT lists every file that was scaffolded
on disk:

- `sectorforge.toml` — your project config.
- `data/worlds/worlds.toml` — the world catalogue.
- `data/factions/factions.toml` — the faction roster.
- `data/regions/regions.toml` — warp-region templates.
- a few more catalogue files under `data/`.
- `sector.json` — your sector itself; empty at this point.

If you ever want to peek at one, click the file name in the tree.

### 1.3 Save now

The PROJECT tab has a **Save** button next to **New project…**. Click it.
This writes the in-memory state out to the folder you chose. You will return
to this button frequently — the builder *does* auto-save, but a manual save
after every major step is a habit worth forming.

> **Tip.** The status footer shows a `dirty` indicator whenever the in-memory
> state has changes that have not been written to disk. After Save it should
> clear.

---

## 2. Tour the empty sector

Before we put anything on the map, get a feel for the layout.

### 2.1 The MAP tab

Click **MAP**. The panel now shows:

- A **toolbox row** at the top labelled `tool:` with six selectable buttons:
  `SELECT`, `ADD SYSTEM`, `DELETE SYSTEM`, `MOVE SYSTEM`, `ADD ROUTE`,
  `REGION PAINT`. Exactly one is highlighted at a time. The default is
  `SELECT`.
- A **zoom slider** labelled `hex` ranging 12–64 pixels.
- A grid of **empty hexes** filling the panel. With width 8 and height 6 you
  get 48 hexes. Coordinates are not labelled, but the top-left hex is `(0, 0)`,
  q increases to the right, r increases downward.

Drag the slider up to roughly `40` so the hexes are comfortably clickable.

### 2.2 The other tabs (read-only at this point)

Click through **SYSTEM**, **WORLD**, **FACTIONS**, **ROUTES**, **REGIONS**.
Each tab politely tells you there is nothing to inspect yet — e.g. SYSTEM says
*"No systems in this sector — use the MAP tab's ADD SYSTEM tool."* That is the
normal pattern: every tab gracefully empties itself when its data does not
exist.

The **FACTIONS** tab is the exception. It already shows the default roster
that the new-project wizard copied into `factions.toml`. We will edit that
shortly.

Return to **MAP** to continue.

---

## 3. Place your first system

### 3.1 Arm the ADD SYSTEM tool

In the MAP toolbox click **ADD SYSTEM**. The button highlights to show it is
armed. The mouse cursor still looks normal, but any click on an empty hex is
now an "add system here" action.

### 3.2 Click an empty hex

Click the hex at approximately `(2, 2)`. A small modal opens titled *Place
system* showing:

- `hex (2, 2)` — confirmation of the chosen cell.
- A text field pre-filled with `Sys-1`.

Type a real name, e.g. `Velikan`, and click **Place**.

The dialog closes. A coloured star appears on the hex with `Velikan` labelled
beside it. The toolbox is still on ADD SYSTEM so further clicks would add
more systems — click **SELECT** in the toolbox to disarm it before you
accidentally place duplicates.

### 3.3 Verify it landed

Click **SYSTEM** in the top tab strip. The tab now shows:

- A **system picker** dropdown defaulting to `Velikan`.
- An **Identity** collapsing section with the system id, name, coordinates,
  and a `SystemKind` selector (DeepSpace, Inhabited, Frontier, etc.).
- A **Star** section with star colour code, colour name, and spectral type.
- A handful of further sections, each a collapsing header — most are
  prefixed with their spec reference: `Tags + Notes`, `Worlds (§8)`,
  `Routes (§9 — read-only here)`, `Primary factions (§10)`, `Control
  (§11)`, `Overlays (§28..§32 — managed elsewhere)`, `§AR1 — Archetypes
  (§30)`, `§AR2 — Auto-assign archetypes`, `§AR3 — Archetype rules`,
  `§S5 — Generate one system here`, `§S4 — Bulk operations`, plus
  injected orbital / conflict / intel sub-sections.

You have your first system. Two notes:

- The system has no worlds yet — the **Worlds** link section will say
  `0 worlds`.
- The star colour was picked by the default generation rules and you can
  change it freely from the **Star** section.

---

## 4. Give the system worlds

There are two ways to add worlds: hand-author each one, or let the generator
populate the system for you. We will do both, so you have seen both flows.

### 4.1 Generate a few worlds first

In the SYSTEM tab scroll down to the **§S5 — Generate one system here**
collapsing header near the bottom and click **Regenerate this system**.

The builder runs the standalone single-system generator using the project's
seed plus the system's id. After a moment the **Worlds** link section
updates: `Velikan` now has, typically, 2–4 worlds attached. The worlds are
named, classified, and faction-tagged.

### 4.2 Inspect a generated world

Click the **WORLD** tab. The world picker at the top now lists every world
across every system; pick `Velikan I` (or whichever the generator named the
first one).

Each world has the following sections, each a collapsing header (most
prefixed with their spec reference):

- **Identity** — id, name, parent system.
- **Classification** — `WorldType`, e.g. `HiveWorld`, `AgriWorld`, …
- **Environment** — atmosphere, temperature, biosphere (each its own
  dropdown bound to a canonical enum).
- **Society** — population scale, tech level, government.
- **§W5 — Notable features** — multi-select of `NotableFeature` tags
  (`TradeHub`, `AdministrativeCapital`, `CultActivity`, etc.).
- **§W6 — Coupling warnings** — derived sanity checks across the world's
  fields.
- **Tags + Notes** — free-form strings.
- **Faction presence (§10)** — per-faction influence + dominance rows
  (this is where **+ Add presence** lives — see §6.2).
- **§W7 — Claims** — `FactionClaim` rows.
- **Control summary (§11 — read-only)** — derived control roll-up.
- **Overlays (§28 / §32 summary)** — derived overlay summary.
- **Conflict** + **Intel** — sub-panels injected by the conflict/intel
  modules.
- **§H8 — Chronicle snippets** — history events anchored on this world.
- **§W4 — Re-roll from candidate pool** — one-shot regenerate of just
  this world.

Try this: open **Classification**, change the WorldType to `HiveWorld`. Open
**Society**, set `Population` to one of the high tiers and `Government` to
`MilitaryGovernor`. The status footer's validation pip may flicker; the
builder is debouncing a re-validation pass.

### 4.3 Hand-author a world

Adding a blank world lives on the **SYSTEM** tab, not WORLD. Switch back to
SYSTEM, make sure `Velikan` is the focused system, and expand the **Worlds
(§8)** collapsing header. Click **+ Add world**.

A new blank world named `World-N` is appended to the system. The header
shows a `→ <id> <name>` link for it — click that link to jump straight into
the WORLD tab with the new world selected. Rename it from the **Identity**
section, then set its classification, environment and society fields the
same way you edited the generated one.

Save the project (PROJECT tab → **Save**) so you do not lose what you have.

---

## 5. Curate the faction roster

The default factions.toml the wizard scaffolded contains a small starter
roster. Make it yours.

### 5.1 Read the existing roster

Click **FACTIONS**. The tab shows a list of faction definitions, each with:

- `id` (e.g. `imperium`, `chaos`, …).
- `display name`.
- `kind` (a dropdown over the known faction kinds — Imperium, Chaos,
  Mechanicus, Cult, Genestealer Cult, Tau, Aeldari, Drukhari, Ork, Tyranid,
  Necron, etc.).
- `disposition` (lawful, insular, secretive, opportunistic, hostile, zealous).
- a base **weight** that drives how aggressively the generator places it.
- preference lists — `preferred_world_types`, `preferred_governments`,
  `preferred_features`.
- a colour swatch (the legend tint).

### 5.2 Trim the roster

For a tutorial sector five factions is plenty. Use the per-row **Remove**
button on any factions you don't want — keep at least:

- One Imperial faction.
- One Chaos faction.
- One xenos faction (Ork or Tyranid is the easiest visual).
- One criminal / minor faction.
- The Genestealer Cult, if you want to see hidden-presence rendering work.

### 5.3 Add a custom faction

The roster is a three-level hierarchy: **top faction → subfaction → row**.
Clicking **+ Add faction** at the top of the panel appends a new *subfaction
row* — by default it inherits its top faction from its `kind` (a row with
`kind: imperial` lands underneath the `imperium` top group, a row with
`kind: chaos` under `chaos`, and so on). There is no separate "add
top-level faction" button: a brand-new top faction is created by adding a
row and then giving it its own top id via the hierarchy editor below.

Click **+ Add faction** now. A row called `new_faction_1 / "New faction"` is
appended and auto-selected. In the right-hand inspector fill in:

| Section            | Field                    | Value                                              |
|--------------------|--------------------------|----------------------------------------------------|
| §F1 Identity       | id                       | `house-velikan`                                    |
|                    | name                     | `House Velikan`                                    |
|                    | kind                     | `imperial` (closest match for a Rogue Trader)      |
|                    | default_disposition      | `opportunistic`                                    |
|                    | weight                   | `12`                                               |
| §F3 Hierarchy      | faction (top id)         | `house-velikan` *(makes it a brand-new top group)* |
|                    | faction_name (top display) | `House Velikan`                                  |
|                    | subfaction (mid id)      | leave empty (defaults to `kind`)                   |
| §F1 Preferences    | preferred_world_types    | `HiveWorld`, `AgriWorld`                           |
|                    | preferred_notable_features | `TradeHub`, `AdministrativeCapital`              |
| §F2 Style override | fill / accent            | pick a distinctive colour                          |

The `Resolved hierarchy:` line at the bottom of §F3 confirms how the row
will be grouped — with the values above it should read `house-velikan >
imperial > house-velikan`. Leave §F3 blank and the row would instead nest
under whatever top group the `imperial` kind resolves to.

### 5.4 Persist the roster

Look for **Save factions.toml** (or similar) at the bottom of the panel.
Click it. This writes the roster back to `data/factions/factions.toml`. Then
do the usual PROJECT → **Save** to commit the sector itself.

---

## 6. Assign factions to the worlds

The roster only describes *who exists*. We still have to tell the sector
*where they are present*.

### 6.1 The two ways

You can either edit the per-world faction list by hand from the WORLD tab's
**Factions** section, or let the generator make a first-pass assignment based
on the preferences you authored.

We will do the auto-assign and then tweak by hand.

### 6.2 Seed the sector roster from the catalogue

**Important:** the **FACTIONS** tab edits `data/factions/factions.toml` —
the *catalogue* of factions that *could* exist. The WORLD tab's **+ Add
presence** dropdown does not read from the catalogue: it reads from the
sector's own roster (`sector.factions`), which is the list of factions
that have actually been instantiated into this sector. Until that roster
is seeded, the WORLD presence editor will say *"no factions in the sector
roster — add factions in FACTIONS first."* — confusingly, even when
FACTIONS is full. There is no "+ Add to sector roster" button; the roster
is only populated by a full regeneration pass.

To seed it, go to **PROJECT → Generation (§6) → Live preview (§G3)** and
click **Regenerate preview now**. When the green `PREVIEW READY` chip
appears, click the green **Apply preview (§G4)** button. The full
generator runs end-to-end: every faction the catalogue defines is now
materialised in `sector.factions`, with first-pass presence rows attached
to worlds whose `WorldType` / `Government` / `NotableFeature` mix matches
each faction's preferences.

Open **WORLD** again, pick `Velikan I`, expand the **Factions** section.
You should now see one to three factions listed, each with their
**influence tier** (Dominant / Significant / Minor / Hidden) and
**dominance state** (Rumored, Presence, Influence, Contested, Controlled,
Stronghold). The **+ Add presence** row at the bottom of the Factions
section is now populated with every roster faction that does not yet have
a row on this world.

> **Note.** Per-system **Regenerate this system** (§4.1) does *not*
> re-aggregate the sector roster — only a full regenerate via Generation
> §G4 does. If you add a new faction to the catalogue later, repeat the
> §G3 → §G4 cycle to get it into the roster.

### 6.3 Add a hidden Genestealer cult

In the **Factions** section of any world that currently only has Imperial
presence:

1. Click **+ Add presence**.
2. Pick the Genestealer Cult faction from the dropdown.
3. Set influence tier to **Hidden** and dominance state to **Presence**.
4. Confirm.

This is the kind of edit that makes the chronicle interesting downstream —
the cult will show up in the hooks and history feeds.

---

## 7. Place a few more systems

We need at least four more systems for routes, subsectors and regions to feel
substantial. Repeat §3 for each:

| Name      | Hex   |
|-----------|-------|
| Cassio    | (5, 1) |
| Drasso    | (6, 4) |
| Mendix    | (3, 5) |
| Outpost-7 | (0, 3) |

For each: arm **ADD SYSTEM** in the MAP toolbox, click the hex, type the
name, hit **Place**, then **SELECT** to disarm.

Once they exist, run **Regenerate this system** (in §S5 — Generate one
system here) on each from the SYSTEM tab, or use the **PROJECT →
Generation (§6)** collapsing header. The Generation section also exposes
the seed-locked re-roll (§G2), the §G3/§G4 live preview, and per-rectangle
partial regeneration (§G5) if you want the whole sector populated at once
instead.

### 7.1 Move a system you placed in the wrong spot

If a system landed on the wrong hex:

1. In MAP, click **SELECT** (or **MOVE SYSTEM**).
2. Click and drag the system's star from its current hex to the desired one.
3. If the destination is already occupied by another system, a *Hex occupied*
   dialog appears with **Swap** and **Cancel**. **Swap** exchanges the two
   systems' coordinates as a single undoable command.

If the destination is outside the sector bounds (e.g. q ≥ width), the move
is rejected and you get a *Coord (q, r) is outside …* message.

### 7.2 Rename without re-placing

Double-click a system's star on the map. A *Rename system* dialog opens with
the current name. Edit, hit **Rename**. This is also undoable.

---

## 8. Routes

### 8.1 Draw a route by dragging

In the MAP toolbox click **ADD ROUTE**. Click and *hold* on the `Velikan`
star, drag your cursor toward the `Cassio` star, and release on top of it.
A line appears between the two stars. The builder switches you over to the
ROUTES tab automatically, with the new route selected.

If you don't like dragging, click-click also works in ADD ROUTE mode:
single-click the start system, then single-click the end system.

### 8.2 Inspect the route

The ROUTES tab shows:

- A summary line: number of routes, number of connected components, total
  hop count.
- A **route picker**.
- A per-route inspector with:
  - **Identity / endpoints** — id, from/to system, `RouteType` (default
    `ChartedPassage`), `RouteStability` (Stable / Unstable / Hazardous /
    Perilous).
  - **Distance** — hex distance with an `auto=` suggestion and `Use auto`
    button.
  - **Tags** — free-form tag list.
  - **Route control** — per-faction patrol / toll / interdiction / piracy
    / secrecy / confidence grid, with **Add control row** and **Re-derive
    controls** buttons.
- A **Route rules** section lower in the panel that edits modifier
  templates (warp turbulence, pirate interdict, blockade, …) — these are
  applied globally rather than per individual route from the inspector.
- A **Hidden routes** section that builds bulk hidden-route topologies
  (webway / black ship / smuggling lane) from selected endpoints — the
  per-route inspector itself does not expose an individual `hidden` flag.

Set the Velikan ↔ Cassio route to Stable, then add routes:

- `Velikan` ↔ `Mendix`
- `Cassio` ↔ `Drasso`
- `Drasso` ↔ `Mendix`
- `Outpost-7` ↔ `Velikan` — set this one's stability to `Hazardous`

### 8.3 Check sector connectivity

The summary line above the picker shows **components: N**. With five systems
and the routes above, it should read `components: 1`. If a system is
isolated the number will be 2 or higher; expand the **Ensure connected**
collapsing header and click **Run connector now** to insert union-find
connector routes automatically.

---

## 9. Subsectors

### 9.1 Open the tab

Click **SUBSECTORS**. The panel shows your existing subsectors (clustered
automatically from the system positions) and the per-cluster inspector for
each.

The default clustering target is small enough that five systems normally
fall into 1 or 2 subsectors. The controls (each tagged with its spec
reference) are:

- **§SUB2 — Recluster** — change *target systems / subsector* via the
  DragValue and click **Apply target & refresh**. (The clustering re-runs
  on the next MAP-tab tick, not on click — the button only resets the
  cache.)
- **§SUB3 — Manual reassignment** — for each system in the picked
  cluster, pick a destination cluster from the *move to* dropdown. This
  is a dropdown, not a drag-and-drop; the override survives reclustering
  and is flagged `manual` on the row.
- **§SUB4 — Capital override** — pick a manual capital per subsector
  from the dropdown.
- **§SUB5 — Colour override** — open the colour picker to override the
  per-subsector tint that shows on the MAP.

Pick one cluster and:

1. Pick **Velikan** as the capital from the §SUB4 dropdown.
2. Open the §SUB5 colour picker and pick a noticeable colour.

The MAP picks up the colour override immediately.

> **Note.** Subsector *labels* and *names* are derived from the
> clustering pass — the builder does not expose a per-subsector label
> override field. To rename, edit the clustering inputs (system count /
> manual reassignment) rather than the label directly.

---

## 10. Warp regions

Regions overlay broad warp phenomena onto the hex grid — warp storms, calm
zones, immaterium tides, gellar-disruption fields. They affect both visuals
and route logic.

### 10.1 Create a region

Click **REGIONS**. The panel has:

- An invariant summary at the top (red chips for overlap or out-of-bounds —
  ignore if green).
- A **region picker** with a `+ new region` button.

Click **+ new region**. Fill in:

| Field   | Value             |
|---------|-------------------|
| id      | `tideline-east`   |
| name    | `The Eastern Tideline` |
| kind    | `WarpStorm`       |
| glyph   | leave default     |

### 10.2 Paint the region's hexes

Switch back to **MAP** and click **REGION PAINT** in the toolbox.

- The label at the top will now mention which region your brush is bound to
  (the region currently selected on REGIONS).
- **Left-click** any hex to paint it into the region.
- **Right-click** any hex to erase it from the region.
- Click and hold to brush continuously.

Paint a small irregular blob of 5–8 hexes near the right edge of the sector.

If you click a hex with no region selected, the builder pops a *"Pick a
region in the REGIONS tab before painting."* dialog — that is the cue to
flip back, select one, and try again.

### 10.3 Optional: grow from a seed

Back on **REGIONS**, in the **Grow seeded region** section you can give a
seed coordinate and a target hex count and have the generator grow the
region for you. Useful when you want something organic and don't want to
hand-paint.

### 10.4 Apply route effects

The **§REG4 — route effect preview** section shows how many of your routes
currently cross the region. If you click **Apply effects to routes** the
builder rewrites their `route_modifier` and `stability` to reflect the warp
condition — Hazardous routes appear differently on the map.

---

## 11. Read the derived layers

You have now placed everything that lives in the model directly. The next
batch of tabs are *derived* — they read the sector and produce content from
it. You don't generally edit them first; you read them and adjust if needed.

### 11.1 CONTROL

Click **CONTROL**. The panel shows:

- A **§C7 / §C8 — MAP overlays** section with an overlay picker:
  `None`, `PowerProjection`, `InfluenceField`, `Administrative`,
  `Military`, `Orbital`, `Naval`, `Mercantile`, `Industrial`,
  `Logistical`, `Informational`, `Religious`, `Sympathetic`. Picking one
  retints the MAP hexes; picking `None` clears the overlay.
- A **§C1..§C3 — World presence** editor (per-world faction presence /
  influence / dominance), plus **§C4 / §C5 — System control**, **§C6 —
  PowerProfile preview**, **§CL1/§CL2 — Per-world claims**, **§CL3 —
  Contested**, and **§CL4 — Bulk convert claims**.

### 11.2 ECONOMY

Click **ECONOMY**. You can:

- Edit per-world `ResourceVector` and `StrategicOutput`.
- Override per-system tithe / supply / priority.
- See `Stranded` badges for worlds whose dependency graph is broken (and
  the MAP will draw a red ring around the system they live in).
- Toggle a **lifeline lanes** highlight that paints the top supplier→consumer
  edges onto the route layer.
- Choose a heatmap mode (tithe, supply, food, trade volume).

### 11.3 RELATIONS

Click **RELATIONS**. This is the faction-vs-faction stance matrix. The tab
shows which faction pairs are hostile, allied, or neutral, plus a tension
scalar derived from world-level overlaps.

### 11.4 HISTORY

Click **HISTORY**. The panel is split into:

- **Config** — epoch start/end, per-anchor caps.
- **Eras** — labelled time bands.
- **Event rules** — when system state X is active, prefer event kind Y.
- **Chronicle** — the actual derived timeline of events.

Click **Regenerate chronicle** (at the top of the panel). The chronicle
list populates with foundation events, faction claims, contested-control
flips, cult exposures, and so on. Manual events you add via the §H5 —
add event header → **+ event (open wizard)** survive future
regenerations.

### 11.5 PERSONAE, HOOKS, PROSE, BRIEFING, SITES, MISSIONS

> **As of this writing, all six of these tabs are placeholders** — they
> open to a stub that says *"Phase D §… — not yet wired"*. The
> underlying engines are real and exposed via the `sectorforge` CLI
> (`personae`, `hooks`, `prose`, `briefing`, `sites`, `missions`
> subcommands); the builder panels have not landed yet.
>
> Conceptually:
>
> - **PERSONAE** — named NPCs per faction presence.
> - **HOOKS** — plot hook templates that condition on the model state.
> - **PROSE** — deterministic gazetteer paragraphs per world.
> - **BRIEFING** — a longer prose pack stitched together for the whole
>   sector.
> - **SITES** — notable sites / encounter locations per world.
> - **MISSIONS** — mission seeds anchored on systems / worlds.

### 11.6 SEARCH (live)

The **SEARCH** tab is wired — it is the declarative wish-based seed search,
in-app. *"Find me a seed where the Imperium is contested by Tau on at least
two hive worlds."*

1. Click **SEARCH**. If you have not searched before, click **+ Create
   wishes.toml** to start an empty wish list.
2. **§SR4 — Search config**: set `base_seed` (tick *use project seed* to
   track the project's own seed, or untick to type a fixed base), `budget`
   (how many candidate seeds to try), and `report_top` (how many near-misses
   to keep).
3. **§SR1 — Constraints**: pick a constraint kind from the dropdown and click
   **+ Add constraint**. Each constraint renders its own form — faction
   pickers, world-type / region-kind / stance / system-state dropdowns,
   share/ratio sliders, count fields, etc. Add as many as you like.
4. If a constraint names a faction that is not in the roster, a red
   **§SR5 preflight** line appears and **Run search** is disabled until you
   fix it.
5. Click **▶ Run search**. The search runs off-thread; a progress bar shows
   `tried / budget · passed · best near-miss`. **■ Cancel** detaches a long
   run.
6. **§SR3 — Outcome**: the winning seed (if any) shows an **Apply winning
   seed** button (regenerates the sector from that seed and jumps to MAP) and
   a non-destructive **View on map**. Each near-miss row has its own
   **View** / **Apply** plus the list of constraints it failed.

### 11.7 DIFF (live)

The **DIFF** tab is wired (§DF1..§DF5). It compares two sectors — or shows
what a stretch of conflict simulation would do — and renders the result as a
click-to-expand tree.

1. Click **DIFF**. Pick a **mode** at the top:
   - **Two-sector (§DF1)** — choose a `before` and an `after`. Each slot has a
     source combo: `Current sector` (the live in-memory sector), `Snapshot`
     (pick one of your named snapshots from §12.3), or `Load file…` (browse to
     any `sector.json` on disk). The **Snapshot current as 'before'** button
     is a shortcut: it takes a fresh snapshot of the live sector and points the
     before-slot at it, so you can edit, then diff against where you started.
   - **Tick simulation (§DF2)** — set **advance N** conflict ticks. The builder
     clones the current sector, runs `advance_sector` N times (hysteresis
     preserved), and diffs the result against the un-advanced original. This is
     the "what happens if the sector simmers for N turns" view.
2. **Filters (§DF4)**: tick **skip worlds** / **skip routes** to collapse those
   strata, and raise **min faction Δ** to drop projection-power movements below
   the threshold as noise. **top deltas** caps the faction list.
3. Click **Compute diff**. The result appears below as a tree of strata
   (§DF3) — **Systems**, **Routes**, **Factions**, **Regions**, **Relations**,
   **Economy** — each header showing `+added −removed ~changed` counts. Expand
   a header to see per-entity rows; expand a changed system to see its control
   deltas and per-world changes.
4. **Export (§DF5)**: click **Choose export folder…**, then **Export diff.md +
   diff.json**. Both files land in the chosen folder — the same artefacts the
   `sectorforge diff` CLI writes.

### 11.8 ANALYTICS, INTERESTINGNESS, SEGMENTUM, EXPORT

> **INTERESTINGNESS and DIFF (§11.7) are wired**; the remaining three tabs are
> still placeholders as of this writing — Phase E work. They open to the same
> stub. Use the `sectorforge` CLI for the underlying functionality (`analyze`,
> `compose`, `generate --formats …`).
>
> Conceptually:
>
> - **ANALYTICS** — counts, distributions, completeness checks.
> - **INTERESTINGNESS** — scored "is this sector dramatically interesting"
>   metrics with a per-profile preset (live).
> - **SEGMENTUM** — multi-sector composition.
> - **EXPORT** — bundle PNG / SVG / HTML / JSON / Markdown writes; see
>   §14 for the CLI fallback.

---

## 12. Validation, invariants, undo

These are not tabs — they are always-on systems you should know about.

### 12.1 The footer

Every tab has a footer that surfaces (left to right): project path, a
`● dirty` / `clean` indicator, a tri-coloured **health pip** (green /
yellow / red) followed by an inline validation + invariant summary
(`validation: N err / M warn · invariants: K`), the command-bus cursor
(`cmd P/Q`), the derivation cache size, and a job spinner when async
work is running.

- **Validation** counts pre-generation rule checks (your config and data
  are coherent).
- **Invariant violations** counts post-generation integrity failures (no
  orphan worlds, no overlapping region hexes, …).
- The **health pip** is green when both are clean, yellow when there are
  only warnings (or no report yet), red when at least one error or
  violation fires.

The footer is read-only — there is no clickable chip to expand the
failing rules in the current build. To see the actual issue list, either
re-run `cargo run --bin sectorforge -- validate --project <path>` from a
terminal, or use the inline per-section hints (e.g. the
`ROUTE_DISTANCE_MISMATCH` warning that appears in the route inspector,
or the `§REG6` chips at the top of REGIONS).

### 12.2 Undo / Redo

Every structural edit goes through the command bus (R4) and is undoable.

- `Cmd+Z` (macOS) / `Ctrl+Z` (Win/Linux) — undo.
- `Cmd+Shift+Z` / `Ctrl+Shift+Z` — redo.

The undo ring holds 200 commands by default. There is no undo limit warning
when commands get evicted off the end, so for big-bang edits use snapshots
(see next).

### 12.3 Snapshots

A snapshot is a named save point — a frozen copy of the sector at a given
command-log position. Useful before doing something risky.

Look for the **Snapshots** section under PROJECT or in the preferences
panel. Click **+ snapshot**, type a name (`pre-routes`, `pre-cult`, etc.),
hit confirm. You can revert to a snapshot later from the same list.

---

## 13. Navigating between tabs

The builder is built around 24 tabs that share state. To move quickly
between them, every entity name shown anywhere in the UI is a clickable
link that jumps you to the tab that owns that entity, with the entity
already selected.

### 13.1 Clicking entity references

Whenever you see a faction id, a system name, a world reference, a route
id, a region name, or a subsector id rendered in the inspector, it is
clickable. The link is usually styled with a small `→` prefix
(e.g. `→ imperium`, `→ Velikan`, `→ R-007 Velikan → Cassio`). Clicking
sends you to the FACTIONS, SYSTEM, WORLD, ROUTES, REGIONS or SUBSECTORS
tab respectively, with the inspector already focused on that entity.

For example:
- On the **WORLD** tab, the header line shows `system: → Velikan`; click
  it to jump to the SYSTEM tab with Velikan selected.
- On the **SYSTEM** tab, the **Worlds** section lists every world as a
  `→ wid name` link; click any to drop into the WORLD tab with that
  world focused.
- In the **HISTORY** chronicle, each event has a `focus` button that
  jumps to whatever the event is anchored on — system, world, region,
  route, subsector, all handled uniformly.

### 13.2 Back / forward

Two chevrons sit to the left of the tab strip (`‹ ›`). They walk a
per-session navigation history of up to 64 jumps. The same actions are
bound to **Alt+←** and **Alt+→** (Linux/Win) or **⌥+←** / **⌥+→** (macOS).

The history does **not** persist across sessions — reopening a project
starts the stacks empty. It is also separate from undo (Cmd+Z): going
back through linked entities does not undo your edits.

### 13.3 Tab-only jumps

A handful of links are marked `→ MAP`, `→ CONTROL`, `→ HISTORY tab` etc.
These jump to a tab without changing what is selected inside it. Useful
when a panel wants to send you to a global view rather than a specific
entity (e.g. ECONOMY's lifeline button sends you to MAP with no system
focus change).

---

## 14. Save, close, reopen

### 13.1 Save

Make sure you are on **PROJECT** and click **Save**. The status footer
should clear its `dirty` indicator.

### 13.2 Close the builder

Just close the window. If there are unsaved changes the builder will prompt
before exiting.

### 13.3 Reopen

Two paths:

- Relaunch with `cargo run --release -p sectorforge-builder -- --project
  <path-to-tutorial-sector>`.
- Or launch with no args, click **Open project…** on PROJECT, and pick the
  folder.

The sector, factions, regions, snapshots and command log all round-trip
through disk, so what you see should match what you left.

---

## 15. Export

Click **EXPORT**. (As of this writing the export panel is in active
development — if it shows a placeholder, use the CLI for now:)

```bash
cargo run --release -p sectorforge -- generate \
    --project ./tutorial-sector \
    --out ./tutorial-sector/out \
    --formats json,markdown,png,svg,html
```

The CLI consumes the same project folder the builder writes to, so anything
you saved is what gets exported. The outputs land in `./tutorial-sector/out/`:

- `sector.json` — full machine-readable model.
- `sector.md` — Markdown briefing.
- `sector.png` — bitmap of the hex map with legend.
- `sector.svg` — vector version of the same.
- `sector.html` — self-contained interactive HTML viewer (open in a browser).

---

## 16. Where to go next

Now that you have built a small sector end-to-end:

- Use the **SEARCH** tab (§11.6) to find a seed with specific properties and
  apply it straight onto the current project, then keep editing from there.
- In **PROJECT → Generation (§6)**, click **Open preset launcher…** to
  switch to one of the curated presets (`m42-classic`,
  `embattled-frontier`, `mercantile-crossroads`, `dead-sector`) and see
  how the tonal knobs change the output without changing the catalogues.
- Compose multiple sector projects into a super-manifest with the
  `sectorforge compose` CLI (the **SEGMENTUM** tab is currently a
  placeholder).
- Read [OVERVIEW.md](OVERVIEW.md) for the qualitative tour of the system, then
  [GUIDE.md](GUIDE.md) for the engineering-level details on every module the
  builder calls into.

If a tab in this walkthrough still shows a *placeholder* / *Phase E* banner,
that capability is on the roadmap and not yet wired through to the UI; the
underlying engine still exposes it via the `sectorforge` CLI binary.
