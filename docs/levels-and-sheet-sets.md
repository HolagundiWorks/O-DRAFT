# Levels and sheet sets

**Status: Phase 1 done; Phases 2 to 6 planned.** Only what "Phase 1 status" lists is implemented and
tested. Capability matrix: [status.md](status.md). Source: the implementation plan "Level model spaces
and sheet sets" (2026-10-26), taken as the decision record below.

Summary: keep one drawing file, but split its single model space into named **levels** (each its own
entity store at a shared origin) and add **sheet sets** (discipline annotation spaces that reference
levels read-only). Every phase is useful and testable alone, starting with levels.

## Requirements

- Levels replace the single Model tab. Each level (Basement, Ground, First, Terrace, ...) is a model
  space for drafting and design. Level tabs sit at the bottom, where the Model tab is now.
- Any level can be **traced** under any other, several at once.
- Sheet sets are tabs on the left (Working drawing, Electrical, Plumbing, Interior, False ceiling).
  They hold annotation and dimensions, reference levels, and never change design geometry.
- Paper layouts stay. Each viewport says which level, section or elevation and which sheet set it shows.
- **Sections and elevations** (added 2026-10-27) are a second kind of model space, drafted by hand
  in 2D, each tied to a marker on a level, with read-only projection guides worked out from the
  levels. See [Sections and elevations](#sections-and-elevations-phase-4).

## Concepts

| Term | Meaning |
|---|---|
| Level | Named model space with its own objects. All levels share one world origin and one layer table. Walls, doors, rooms, columns, stairs, take-off and the parametric graph act on the active level only. Elevation and height come from the existing `levels` / `floors` data |
| Trace | Read-only faded display of another level under the active one. Per trace: fade (0-90 %), colour (grey or true), order (below or above), object-snap switch, optional layer list. Never selectable or editable. At most 8 per level; a level may not trace itself |
| Sheet set | Named discipline with one annotation store **per level** (text, dimensions, leaders, tags, symbols, hatches). While active, the level is shown locked and only annotation is drawn |
| Sheet view | (sheet set, level): that level's geometry (locked, filtered by the set's layer visibility) plus the set's annotation for it |
| Design mode | No sheet set picked. The default; where drafting happens |
| View space | A section or elevation: its own store, tied to a marker on a level, drafted by hand, with projection guides (Phase 4) |
| Layout | Paper, unchanged. A viewport names a level or view space, an optional sheet set and optional traces |

Annotation lives in its own store, not on layers, so two disciplines never collide, take-off never
counts annotation, and deleting a sheet set never touches the design.

## Sections and elevations (Phase 4)

Sections and elevations are a second kind of model space (a **view space**), drafted by hand in 2D
like a level, but each is tied to a marker on a level and shows **projection guides** computed from
the plans. Nothing is generated from a 3D model: guides are 2D projections of plan geometry plus
heights already stored in the drawing, so the 2D-only scope holds.

### View space

- Its own store and tab (`Section A-A`, `Elevation North`), drafted with the normal tools.
- Fixed coordinates: X is the distance along the marker line from its first point; Y is height in
  drawing units from datum 0. A level at +3.150 m is at Y = 3150 (mm drawings) in every section.
- Tied to a **marker** on a level. Today `section.marker` and `elevation.marker` (`cmd/symbols.rs`)
  draw loose lines, circles and text with no link. They become one block insert each
  (`AAD-SECTION`, `AAD-ELEVATION`) carrying the view id as extension data, so moving or rotating
  the marker updates the view. `section.link {handle | window}` adopts an existing loose marker.
- A **depth** (how far past the line it looks, > 0) and the levels it covers (default all).

```text
SpaceId(u32)                      shares the id counter with LevelId; never reused
ViewSpace { id, kind Section|Elevation, name (1..64, unique among views), level: LevelId (the marker's plan),
            marker: Option<Handle>, line: (Vec2, Vec2) plan coords, look Left|Right, depth > 0,
            covers: Option<Vec<LevelId>>, traces[<=8] (other view spaces only), view, entities (skip) }
Drawing += views (<=128)
EditTarget += View(SpaceId); SheetSet.annotations is keyed by space (level or view), not LevelId only
```

`switch_level` generalises to the edit-target switch (Phase 3 introduces `EditTarget`); the swap
invariant stays in one function. `model_stores` / `model_stores_mut` include view stores, so purge,
audit, layer rename and the other drawing-wide walks of Phase 1 cover them with no new call sites.

### Projection guides

Read-only lines computed from the levels, drawn through the existing trace path
(`crates/render/src/trace.rs`, quiet colour, `VIEWPORT_CONTENT` handle so never selectable),
snappable, and recomputed when a covered level's store changes (`Arc::ptr_eq`, the same cache rule
as traces). Guides are never saved; they are recomputed on open. What the user drafts is never moved.

| Guide | Source | Section | Elevation |
|---|---|---|---|
| Floor and top-of-storey lines, labelled with the level name | each level's `elevation`, `height` | yes | yes |
| Wall faces | cut line crossing wall lines on each covered level | cut walls, both faces | outer faces seen within depth |
| Door and window jambs | opening inserts (`walls.rs`) within depth | yes | yes |
| Sill and head lines | opening attributes `OSILL`, `OHEIGHT`, `OHEAD` | yes | yes |
| Columns, stair start and end | column and stair objects crossing or seen | yes | seen only |

Guide maths lives in `crates/engine/src/sectionguide.rs`: plain 2D line/segment intersection and
projection onto the marker line. Hostile-input rules apply: zero-length line, depth 0 or negative,
NaN or infinite heights, more than 100 000 guide lines (capped, with a note), nested blocks bounded by
`MAX_BLOCK_DEPTH`. Guides know only O-Draft walls, openings, columns and stairs (or objects carrying
their attributes); plain lines give none. Roofs, sloped slabs and curved walls in elevation are
drafted by hand.

### Commands (`cmd/views.rs`)

| Command | Purpose |
|---|---|
| `section.new {p1, p2, look?, depth?, label?, level?}` | Place a linked section marker on a level and create its view space (does not switch to it from JSON) |
| `elevation.new {at, angle, depth?, name?, level?}` | Same for an elevation |
| `view.list`, `view.set {view, name?, depth?, covers?, look?}`, `view.delete {view, force?}`, `view.activate {view}` | Manage view spaces; each is one undo step; deleting the marker leaves the view with `marker: None` and a note |
| `view.trace` | Traces between view spaces (levels are not offered: plan and section coordinates differ; guides do that job) |
| `section.link {handle}` | Adopt an existing loose marker |
| `section.draft {view, slabThickness?}` | Turn the current guides into ordinary editable objects in one undo step: cut walls as closed outlines, slabs as bands, openings as outlines between sill and head. Running again adds only what is new; never moves drafted objects |
| `section.check {view}` | List guides that no longer meet drafted geometry (for example a wall moved 150 mm), as a report; changes nothing |
| `section.levelmarks {view}` | Place `level.mark` symbols at each floor line, named after the levels |

### UI, sheet sets, layouts, files

- **Tabs:** a second group in the bottom row between levels and layouts
  (`Ground | First · Section A-A | Elevation North · Title plate | Layout1`); a group with more than
  about six tabs collapses into a dropdown chip (`Sections ▾`). Double-click on a marker opens its
  view; right-click on a view tab offers Show marker. `chrome_json` gains `views`.
- **Sheet sets:** annotation per space, so Working drawing holds Section A-A's dimensions and
  Electrical the switch heights on Elevation North.
- **Layouts:** `Viewport.level` generalises to a space id (level or view). The marker head's sheet
  field is filled from the layout whose viewport shows that view, as `elevation.marker` takes `sheet`
  by hand today.
- **Files:** view stores as blocks `ODRAFT$VIEW$<id>` with keep-alive INSERTs on `ODRAFT-STORES`;
  view records (kind, marker handle, line, look, depth, covers, traces) in `levels2` (bump its
  version; older readers already fall back to one level on an unknown version). The marker stays an
  ordinary block in other CAD programs.

### Done when

Moving a wall on Ground moves its guide in Section A-A without touching drafted lines,
`section.check` reports the gap, the drawing round-trips through DXF and DWG with views, markers and
links intact, and the hostile-input tests pass.

## Data model (`crates/doc`, L1)

`Drawing.model` always holds the one store being edited; every other store waits in its slot. Commands
keep writing to `d.model`, so the existing call sites do not change; only drawing-wide code (purge,
audit, layer rename, extents) must learn about the other stores.

```text
LevelId(u32), SheetSetId(u32)        stable, never reused
Level     { id, name (1..64, unique), tab_order, elevation, height, traces[<=8], view, entities (skip) }
Trace     { level, fade 0..=90, colour Grey|TrueColour, order Below|Above, snap, layers? }
SheetSet  { id, name, tab_order, space?, layer_prefix, dim_style?, text_style?, annotations: LevelId -> store (skip) }
EditTarget = Level(LevelId) | Sheet(SheetSetId, LevelId)
Drawing  += levels (<=64), sheet_sets (<=32), target
```

Methods: `switch_target(EditTarget)` (the only place the swap invariant lives; used by commands, undo and
file load), `store(target)`, `all_stores()`, `reference_stores()`. Handles stay drawing-wide so a
sheet-set dimension can reference a wall handle on its level. Because levels live inside `Drawing`,
undo restores them, including the active tab. `Viewport` gains `level?` (none = first level),
`sheet_set?` and `traces`. All limits are checked on every input path (JSON, DXF metadata).

## Commands (`cmd/levels.rs`, `cmd/sheetset.rs`)

| Command | Purpose |
|---|---|
| `level.list`, `level.new {name, elevation?, height?, after?, copyFrom?}`, `level.set`, `level.delete` (refused for the last level or one in use, unless `force`), `level.activate` | Manage levels; each is one undo step |
| `level.trace {level?, add?, remove?, fade?, colour?, order?, snap?, layers?}` | Edit a level's traces |
| `level.copyto {levels, selection?}`, `level.moveto {level}` | Copy or move the selection between levels at the same coordinates |
| `sheetset.list / .new / .set / .delete`, `sheetset.activate {set\|null}` | Manage sheet sets; `new` with `preset` creates the five standard ones |
| `viewport.set` (extended) `{handle, level?, sheetSet?, traces?}` | What a layout viewport shows |
| `level.import / level.export {path, level}`; `level.fromfloors`; `project.merge` | Migration (below) |

Rules: in a sheet view the `protect.rs` check treats reference stores as locked and names the level;
draw commands in a sheet view put objects on the set's layers (`E-DIM`, `E-TEXT`, ...); selection,
`qselect`, `filter`, `selectsimilar` see only the edit target; inquiry commands may read traces and
the referenced level through snaps; programmatic calls never open dialogs.

## UI (WinUI shell stays thin)

- **Bottom tabs:** level tabs in tab order, a `+` (`level.new`), a divider, then layout tabs. Right
  click on a level: Rename, Elevation and height, Copy level, Delete, Trace over current level. A
  **Trace** flyout lists the other levels (tick, fade slider, grey/true colour, below/above, snap).
- **Left sheet-set rail** (about 32 DIP): Design, then the sheet sets, then `+`. Choosing one keeps the
  current level. In a sheet view the draw strip swaps to annotation tools and the Architecture bar
  hides. The rail hides in layouts.
- Layer manager "Show for" filter (All / This level / This sheet set); viewport properties gain Level,
  Sheet set and Traces; new-drawing and `project.setup` offer levels; Ctrl+PageUp/PageDown change
  level, Ctrl+Shift+PageUp/PageDown change sheet set. Tab data comes from `chrome_json`
  (`levels`, `layouts`, `sheetSets`). The egui shell follows later.

## Rendering, snaps and selection

Phase 2 changes `aad_render` and the host canvas; **coordinate the order with the rendering work
before starting it.**

- One cached mesh per store. Draw order: traces Below, the referenced level (sheet view), the edit
  target, traces Above. Grey maps colours to the theme's quiet colour; fade sets alpha; trace
  lineweights draw thinnest.
- A trace mesh rebuilds only when its level's store changes (`Arc::ptr_eq`), so tab switches and pans
  stay on the cached path of [PERFORMANCE.md](PERFORMANCE.md). `lod_for_view` applies to traces.
- Layout viewports read `d.store(viewport.level)` plus the set's annotation and the viewport's traces;
  frozen layers and colour overrides apply to all.
- Object snaps use the edit target, the referenced level in a sheet view, and traces with `snap` on
  (markers in the quiet colour). Select and grips act on the edit target only. Zoom Extents uses the
  edit target plus visible traces.
- Budget: with three traces of 25 000 objects each, pan and zoom stay cached and a test shows that a
  level switch does not rebuild unchanged meshes.

## DXF and DWG round-trip

The file stays a normal DXF. DWG uses the same bridge.

| What | Written as |
|---|---|
| First level in tab order | Model space, whatever tab was active on save |
| Other levels | Block `ODRAFT$LVL$<id>` |
| Sheet-set annotation for a level | Block `ODRAFT$ANN$<set id>$<level id>` |
| Keep-alive | One INSERT of each block in model space on layer `ODRAFT-STORES` (frozen, not plotted) so `PURGE` keeps them |
| Level, trace, sheet-set records | JSON under `levels2` in `AAD_PROJECT` (version, ids, names, order, elevations, traces, sets, active target) |
| Section and elevation spaces (Phase 4) | Blocks `ODRAFT$VIEW$<id>`; records in `levels2` |
| Viewport level, set, traces | Extension data `AAD_VP` on the VIEWPORT |

Reading: the blocks named in `levels2` are read back into stores and the keep-alive INSERTs and layer
are hidden from the user. Missing or damaged metadata never fails the open: the drawing loads as one
level and a warning is reported; a record naming a missing block gives an empty level and a warning.
Handles inside level blocks keep their values. Counts, ids and nesting (`MAX_BLOCK_DEPTH`) are
bounded, with a `hostile_dxf_does_not_panic`-style test. Known limit: in another CAD program a
viewport set to a non-first level still shows model space; `level.export` gives one DXF per level.

## Migration

Nothing changes in an existing drawing until a second level or a sheet set is added; such files save
as before. They open as one level named after `floor.current`, else "Level 1". `floors` / `levels`
project data stay the source of names and elevations (`level.new` and `level.set` keep them in step).
Floor projects (one file per floor with Xrefs) stay supported through `level.import`, `level.export`
and `project.merge`. Drawing spaces keep working; a sheet set points at one. Take-off and ShilpiDB
rows gain the level id in their slot; single-level rows keep their ids. Viewports without level data
show the first level.

## Decisions (from the plan's open questions)

1. Levels in one file are the default for new projects; floor projects remain an import/export path.
2. Sheet-set annotation is stored per level.
3. The rail lists Design plus the five presets on every new drawing.
4. Traces are saved with the level.
5. Open: sections and elevations use guides plus an on-demand `section.draft`; drafted lines do not
   follow plan changes automatically (assumed until the owner decides otherwise).

## Roadmap

The plan names six phases (sections and elevations were added as Phase 4 on 2026-10-27); the split
below follows the plan (Phase 1 carries the file format and drawing-wide audit; Phase 2 is the
rendering work). Each phase ends with `cargo xtask ci` and its named tests, and updates
[status.md](status.md) in the same change.

| Phase | Scope | Done when |
|---|---|---|
| 1. Levels core | `doc` types, `switch_target`, `levels.rs` commands (list/new/set/delete/activate/copyto/moveto/fromfloors), classification of every `.model` use as edit-target or drawing-wide, drawing-wide commands use `all_stores()`, DXF/DWG round-trip with `levels2`, hostile-input tests | A two-level drawing saves, reopens and undoes identically; purge, audit, layer delete and extents see both levels; old files load as one level unchanged |
| 2. Traces and level tabs | `level.trace`, trace rendering and cache, snaps, bottom level tabs and Trace flyout, `chrome_json` data | Three 25 000-object traces keep pans cached; a tab switch rebuilds no unchanged mesh |
| 3. Sheet sets | `SheetSet`, per-level annotation stores, `sheetset.*`, `protect.rs` reference locking, annotation layer prefixes, left rail, annotation tool strip | In a sheet view a level object edit is refused naming the level; annotation of two sets on one level does not collide; presets create their spaces |
| 4. Sections and elevations | View spaces, linked markers, `section.*` / `view.*` commands, projection guides (`sectionguide.rs`), view tab group, per-space annotation, `ODRAFT$VIEW$` round-trip | See [Done when](#done-when) above |
| 5. Layouts and exchange | `Viewport.level/sheet_set/traces`, `viewport.set`, layout tessellation per store, `AAD_VP` data, Export for exchange | One layout holds the Electrical and Plumbing First-floor plans side by side and round-trips |
| 6. Migration and polish | `level.import/export`, `project.merge`, take-off level ids, layer "Show for" filter, shortcuts, new-drawing and `project.setup` flow, egui tabs, memory load test (20 levels) | A floor project merges into one file in one undo step; docs and status complete |

## Phase 1 status: done, with the limits below

Implemented and tested (`crates/engine/tests/levels.rs`, unit tests in `crates/io/src/levels_file.rs`):

- `crates/doc/src/levels.rs`: `LevelId`, `Level`, `Trace` (data only, not drawn yet), and on
  `Drawing` `levels`, `active_level`, `switch_level`, `add_level`, `rename_level`, `remove_level`,
  `move_level`, `model_stores` / `model_stores_mut`, `level_store`. A drawing with no levels is one
  implicit level and is unchanged until the first `level.new`.
- Commands, each one undo step with count, name and parameter limits (64 levels, unique names):
  `level.list`, `level.new` (the first call names the existing level after the current floor, else
  `Level 1`, or `first`), `level.set` (name, elevation, height, order; a rename also renames the
  matching entry of the floor list and the active floor, and updates title-block `FLOOR` fields),
  `level.delete` (`force` for a level with objects or traces; the last level stays),
  `level.activate` (refused while the block editor is open; clears the selection), `level.moveto`,
  `level.copyto` (new handles, same coordinates; layer locks are honoured on a move) and
  `level.fromfloors` (an empty level per floor of the floor list, elevation from the floor heights,
  the existing objects stay on the first floor).
- **File format** (`crates/io/src/levels_file.rs`): with two or more levels, DXF and DWG store the
  first level as model space, the others as blocks `ODRAFT$LVL$<id>` kept alive by frozen,
  non-plotting INSERTs on layer `ODRAFT-STORES`, and the level records under `levels2` in the
  project data. Reading restores levels, traces, the active level and handles, and hides the
  plumbing. Damaged metadata (bad version, duplicate or zero ids, duplicate or empty names, an
  unknown active id, bad traces, more than 64 levels, negative height) opens the file as one level
  and never fails with a note on the command line; a missing level block gives an empty level and a note. One level or none writes the same
  bytes as before.
- Drawing-wide readers and writers cover every level: `PURGE` blocks, layer in-use check, layer
  rename and merge, `AUDIT` (finds and repairs errors on any level), data-extraction and compare
  walks, text styles in use, unsaved-object counts, `Drawing::entity` and `entity_count`.
- Objects on an inactive level are found but not editable from the active one: `move` and similar
  skip them.

Classification of the other `.model` users: they stay on the **active level** by design, because
the result belongs to the level being worked on. These are `assoc` (dimension associativity),
permit title-block tools (`bplt`), legends, the door and window schedule block counts, layer
mapping (`layermap`, which keeps a source layer that another level still uses), the WinUI Blocks panel reference list, and the finite-value check. A later
phase may widen the schedule and legend to the whole building.

Damaged or incomplete level data opens the file as one level (or a level empty) and the command line says so (`levelsNote`, set by the reader and reported by `open`).

Known limits: the viewer shows the active level only (no tabs, traces or sheet sets yet, Phases 2 and 3); in
another CAD program a level other than the first is a block definition.

## Risks

- **Drawing-wide code that only reads `model`** (purge, audit, layer rename/delete, find and replace,
  data extraction, `overkill`, xref bind) would delete things used only on another level. Phase 1
  classifies every `.model` user and adds a test per drawing-wide command with content on a second
  level.
- Another CAD program exploding or renaming the `ODRAFT$` blocks loses the level on reload; this is
  reported as a warning.
- Memory: all levels are in memory; entities are shared through `Arc`, but a 20-level tower needs a
  load test.
- Overlap with rendering work in Phase 2.
