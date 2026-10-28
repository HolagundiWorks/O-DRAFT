# Take-off recorded in ShilpiDB

O-Draft records its take-off in a ShilpiDB database, and O-CoreOp reads it from there. The contract
(version 1, revised 2026-10-19) is `docs/TAKEOFF-SHILPIDB.md` in the O-CoreOp repository; the hand-off that
asked for this is `docs/HANDOFF-O-DRAFT-TAKEOFF.md` there. This page says what exists on the O-Draft side.

## Commands

### `takeoff.record`
`{server?, path?, file?, annotate?, paper?, level?, bearing?}` (Architecture > Take-off > Record in ShilpiDB;
also reachable from scripts, the control channel and MCP).

- It runs the same collection step as `bridge.export` (`cmd/bridge.rs`, `collect`), so a record and the
  bridge file's row are the same object.
- **Target.** By default the shared `shilpid` on `127.0.0.1:7420`, started (with `shilpi-http` on 7421) when
  nothing answers; the programs are found through `SHILPIDB_BIN` (a folder), `%LocalAppData%\ShilpiDB\bin`,
  then `PATH`, and the data file is `%LocalAppData%\OSAS\takeoff\takeoff.vdb`. `server: "host:port"` names
  another server and never starts one. `path` writes a `.vdb` file instead; `file: true` puts it beside the
  saved drawing (`<drawing>.takeoff.vdb`).   A file that has records but no header is refused, so an entity
  store written by `vdb.export` is never overwritten. File mode writes
  host format **version 1** (same record layout `shilpid` uses) so the
  `.takeoff.vdb` can be opened with `shilpid --data`; entity `.vdb`
  exports stay on host version 2.
- **Protocol** (the contract has no transactions, so the header is the completion marker): claim the slot by
  writing the header with `"state": "recording"` and reading it back; write every row; delete this slot's rows
  that are no longer measured; delete the old annotated-PDF chunks and write the new ones; write the header
  again with `"state": "complete"` (and `annotated_pdf`); save. A recording that dies in between leaves the
  drawing `recording`, which O-CoreOp refuses to import.
- The result gives the target, `slot`, `headerId`, counts per kind, `written`, `added`, `deleted` (and
  `changed` / `unchanged` for a file), the PDF's size, anything `skipped` (two rows with the same number) and
  the warnings. A drawing that cannot be plotted still records; the result then warns that there is no
  annotated drawing.

### `takeoff.annotate`
`{path?, paper?}` (Architecture > Take-off > Annotated Drawing (PDF)) writes the annotated drawing as a PDF
(default `<drawing>.takeoff.pdf`). It works on a copy of the drawing: each measured item is drawn again on the
layer `TAKEOFF-ANNOT` in the colour of its kind (walls red, openings green, columns blue, lintels magenta,
rooms cyan, slabs orange) with its mark and measure beside it, and a title line (drawing, revision, date) and a
legend with counts stand below the drawing. The drawing itself is not changed.

### `takeoff.restore`
`{name?, at?, clear?, path?, server?, file?}` (Architecture > Take-off > Restore Tables) redraws the
`MEASURE-TABLE` summaries from the drawing's saved `takeoff:*` project entries. With `path` / `server` /
`file: true` it first reloads those entries from the ShilpiDB header's `takeoff_tables` snapshot (written by
`takeoff.record` whenever any take-off has been run), then redraws. Use this after `takeoff.clear` or to
reproduce tables from a `.vdb` recorded earlier.

## Records

Everything is millimetres, whatever the drawing units. The id is `(kind << 56) | (slot << 32) | n`.

| Kind | Record |
|---|---|
| 1 | Header, one per drawing, `n = 0`: `format` `hcw-takeoff-shilpi`, `version` 1, `units` `mm`, `state`, `source` (`app` `O-Draft`, `drawing`, `drawing_id`, `project_ref` (the suite's `ProjectRef`) while the drawing is linked to a project in the OSAS company file, `exported`, `revision_sha256` while the saved file still matches its recorded revision), and the bridge file's `project`, `levels`, `beam_depths_mm`, `opening_schedule`; `annotated_pdf` (`sha256`, `bytes`, `chunks`, `pages`, `generated`) once a PDF is stored |
| 3 to 8 | Wall, opening, column, lintel, room, slab: the payload is the bridge row as written; the box is the item's bounds in millimetres (the room's box when one is drawn, else its name text; zeros when it has no geometry). A wall row also carries `wall_type` (`external` or `internal`), `thickness_mm` (from the entity mark, else 230 mm full brick / 115 mm half brick) and, when `takeoff.paint` has been run, `paint_external_m2` and `paint_internal_m2` |
| 9 | A chunk of the annotated PDF: raw bytes, at most 512 KiB, `n` from 0, zero box; at most 64 MiB in all |
| 10 | Electrical switchboard row: `switchboard`, `height`, `5amp_socket`, `15amp_socket`, `connected_fixture`, `two_way_switches`, `one_way_switches` (written by `electrical.connect`, included by `takeoff.record`) |
| 11 | Electrical circuit row: one wired point, with `switchboard`, `point`, `kind` and the same count columns |

## Metadata and legacy layers

`takeoff.draw` and `takeoff.tag` store a `TakeoffMark` (`kind`, optional `thicknessMm`) on the entity. Measure commands use that mark. An entity with no mark is still measured from its `MEASURE-*` layer. `takeoff.maplayers` copies the layer onto the mark (`MEASURE-FULLBRICK` → external wall 230 mm, `MEASURE-HALFBRICK` → internal wall 115 mm, and the other measure layers to their kinds). Marks are mirrored in the project entry `takeoffMeta` so a DXF save keeps them; open restores them onto entities that have none.

A saved drawing that has take-off results, an opening schedule or electrical ids is recorded again after a successful save (`annotate` false). The Record button still runs `takeoff.record` with the annotated PDF.

`n` is the entity handle of the measured thing: the measured line or polyline of a take-off row, the
block insert of an opening, the room's name text, or the first face line of a committed parametric
wall. Each row's `ref` is a letter and that handle in hex (`W1A2`, `O3F`, `C41`, `L52`, `R6B`,
`S77`), or `PW-<instanceId>` for a parametric wall, so the same thing gets the same `ref` and id
every time, including after the take-off is run again. A saved take-off row without a handle keeps
its old position-based `ref`, is not recorded, and the export warns about it.

**Slot.** One database holds many drawings, so each owns a 24-bit slot: `SlotFor(drawing_id)` is the 32-bit
FNV-1a of the id folded as `(h ^ (h >> 24)) & 0xFFFFFF` (1 if that is 0; the contract's two test vectors are
unit tests). When the header at that slot belongs to another drawing the next slot is tried (after the last
comes 1). The slot is kept in the drawing (`takeoffSlot` in the project data, with its `drawing_id`) so it
never changes; a kept slot that another drawing has taken since is not trusted.

**Number.** `n` is the entity handle of the measured thing when it fits in 32 bits, else the 32-bit FNV-1a of
its hex text: the measured line or polyline of a take-off row, the block insert of an opening, the room's name
text. Each row's `ref` is a letter and that handle in hex (`W1A2`, `O3F`, `C41`, `L52`, `R6B`, `S77`), so the
same thing gets the same `ref` and id every time, including after the take-off is run again. A saved take-off
row without a handle keeps its old position-based `ref`, is not recorded, and the export warns about it.

A rectangular slab (four right angles, straight sides) also carries `length_mm`, `breadth_mm` and
`"rectangular": true`; any other outline carries only `area_m2`. Only measured facts are recorded: no finishes,
rates or costs.

### Parametric walls

Walls committed with `param.commit` are collected by the same `bridge.collect` step. Linked, current
instances become wall rows with `thickness_mm`, `layer` `A-WALL` and `ref` `PW-<instanceId>`; stale
and detached instances are left out with a warning. Brick take-off walls carry the nominal layer
thickness (230 / 115 mm) and a row `_note` that it is not measured. Details: [parametric.md](parametric.md).
Hand-off for the O-CoreOp team: [HANDOFF-O-COREOP-PARAMETRIC-TAKEOFF.md](HANDOFF-O-COREOP-PARAMETRIC-TAKEOFF.md).

### Floor named on every row

When `level` is omitted, rows are stamped with the **active drawing level**, else the first floor
name, else `Ground`. A drawing with more than one floor that omits `level` gets a warning that the
export covers one floor only — pass `level` explicitly for First / Second / ….

## Tests

`crates/engine/tests/takeoff_record.rs`: the records equal the bridge rows and the header is
`complete`; ids carry kind, slot and number; recording again keeps the ids and one changed wall changes one
record; a wall no longer measured is deleted; refs survive a rebuilt take-off; millimetres for an imperial
drawing; rectangular and L-shaped slabs; two drawings in one database stay apart and each keeps its slot; the
default file, refusal of an entity store and of an unreachable server; an empty drawing writes the header only;
the annotated drawing (a stand-in plotter receives the measured items on `TAKEOFF-ANNOT`, a 1.2 MB PDF is
stored as three chunks that add up, and recording again without it removes them). Unit tests in
`cmd/takeoffrec.rs` cover the contract's slot test vectors, id layout, chunking and the slot probe.

`crates/engine/tests/quantity_trust.rs` plus fixture `fixtures/quantity-trust-checklist.json`: known wall/opening
numbers (two 6000×3000×230 walls; D2 + W3 on the front; `project_ref`; face net 32.67 m²) through
`takeoff.record`, catching double-count, `wall_ref`, and thickness regressions. Linux/cloud driver:
`scripts/takeoff-e2e-harness.sh` (optional `TAKEOFF_E2E_SHILPID=1`, `TAKEOFF_TRUST_EXPORT=…`). WinUI product
steps stay on a Windows machine (OSAS store `takeoff-e2e-runbook.md`).

One more test is ignored (it needs the server program): it records to a running `shilpid` three times, once
after erasing a wall (`AAD_SHILPID_ADDR=127.0.0.1:7430 cargo test -p aad-engine --test takeoff_record -- --ignored`,
with `shilpid --bind 127.0.0.1:7430 --data x.vdb`).

## Rows O-CoreOp asked for (done)

After reading a database O-Draft wrote, O-CoreOp asked for four changes (`HANDOFF-O-DRAFT-TAKEOFF.md` section 8). All four are in `bridge.export` and therefore in `takeoff.record`:

- **Rooms** carry `level`, `length_mm` and `breadth_mm`, and `rectangular`: the exact sides of a rectangular `ROOM-RECT` outline, else the sides of the outline's box with `"rectangular": false`. A room with no drawn outline sends its area only and the export warns.
- **Openings** carry `wall_ref`: preferred from the opening's `MEASURE-DEDUCT` line matched to a measured wall (same rule as `takeoff.brick`); otherwise the measured wall nearest to the insertion point within half thickness plus 150 mm, with a row `_note` that the link is a proximity guess. An opening on no measured wall gets none and the export warns.
- **Walls** carry `thickness_mm`: when parallel `A-WALL` faces sit beside the MEASURE centre line, the face spacing is measured; otherwise 230 mm on `MEASURE-FULLBRICK` / 115 mm on `MEASURE-HALFBRICK` with a nominal `_note` and export warning.
- A ventilator is written as `Window`; `source.drawing` is `(unsaved)` for a drawing that has not been saved.

Checked: `rows_carry_what_o_coreop_needs_to_use_them_without_guessing`; the failure test `a_recording_that_dies_half_way_leaves_the_drawing_recording_and_the_next_one_completes_it`; and `apps/aad-cli/tests/takeoff_pdf.rs`, which uses the real plotter: the PDF is stored in chunks that reassemble to the bytes and SHA-256 the header names, and an ignored test run against a real `shilpid` recorded two drawings with their PDFs.

## Phases (estimation and running bill)

A measured dimension belongs to an **estimation phase** (a stage of the work: Foundation, Plinth, Superstructure,
Finishing by default) and a **running-bill phase** (Phase 1 to 3 by default). Both are names from lists the drawing
keeps, chosen in O-Draft while measuring and shown as columns by O-CoreOp. The take-off mark carries them
(`phase`, `billPhase`) and so does every exported row (`phase`, `bill_phase`, both optional: a row with none has no
field, so an older reader is unaffected).

- `takeoff.phases {estimation?, billing?, current?: {estimation?, billing?}}` reads or edits the two lists and the
  *current* phases. Lists are trimmed, case-insensitive-unique, at most 50 names of 60 characters; the current phase
  must be in its list. The WinUI take-off bar has a picker for each (`apps/aad-winui/TakeoffPhases.cs`). The bar is text only (flat text buttons in the groups Draw, Compute and Sync, with dividers); O-CoreOp's Take-off page bar is drawn the same way.
- `takeoff.draw`, `takeoff.tag` and `takeoff.maplayers` stamp the current phases on what they mark, or the `phase` /
  `billPhase` of the call (a name outside the list is refused; null clears).
- `takeoff.phase {estimation?, billing?, handles?}` changes the phases of dimensions already marked; unmarked objects
  are skipped and counted, never guessed.
- Walls, columns, lintels and slabs carry the phases of their own mark. An opening has no mark: it takes the phases
  of the wall it sits in (`wall_ref`) unless it has its own. Rooms and electrical rows carry none yet.
- The marks are mirrored in the project entry that a DXF round trip keeps (`takeoffMeta`), so the phases survive a
  save as DXF.

Checked by `crates/engine/tests/takeoff_phases.rs` (defaults, list editing and hostile input, stamping, change and
undo, exported wall and opening rows, no columns when none are set) and
`aad-host` `panels_carry_the_take_off_phases_for_the_pickers`. The WinUI pickers were compiled but not clicked
through, and O-CoreOp's table columns are a change in that repository.

## Not done

- **PDF underlay and calibration**: built for vector PDFs, see [pdf-underlay.md](pdf-underlay.md); scanned PDFs are not supported. (Old text follows.) O-CoreOp's PDF panel is gone; O-Draft had no PDF underlay or scale
  calibration yet, so a project that only has PDF drawings cannot be measured. That is the next piece of work.
- Not checked by O-CoreOp yet: the database with a real PDF and two drawings (O-Draft checked them itself, section "Rows O-CoreOp asked for").
- The header's `levels` rows also carry `ceiling_mm` and `lintel_bottom_mm` from the bridge file; O-CoreOp ignores them.
- No menu entry yet in the WinUI shell other than the engine registry's (the commands appear under
  Architecture > Take-off from the registry).
- Selecting a measured item from its ShilpiDB box (highlight / zoom to the recorded bounds) is not built yet.
- A check for two drawings that measure the same floor (the double-count risk): O-CoreOp warns on
  import; O-Draft also warns at `takeoff.record` when another complete slot in the same database
  already has measured rows on the same company-file `project_ref` and stamped floor (warns, does
  not block). Without a company-file link there is no strong project match, so record-time overlap
  is not reported.

## Floor regions and per-entity level

`floor.declare` / `floor.refresh` store `entityFloors` (handle → floor name). `bridge.export` /
`takeoff.record` stamp each measured row's `level` from that map when the entity is assigned;
unmapped rows still use the export-wide `level` (explicit param, active level, or first floor).
A multi-floor drawing without regions still warns; with regions it notes that unmapped rows use the
fallback level.

## Floor regions (single drawing)

`floor.declare` / `floor.refresh` / `floor.regions` / `floor.export` / `floor.xref` assign a window (or
selection) of objects to a named floor, store the assignment in the drawing (`floorRegions`,
`entityFloors`), and can write each floor as a separate DXF then attach it as an xref. See the command
help under Architecture > Floors.
