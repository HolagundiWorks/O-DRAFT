# Version history and revision register

O-Draft keeps a **version history** of drawing stores (levels and sheet-set annotation) in a ShilpiDB sidecar beside the drawing, and a **revision register** in the drawing’s project data so sheets and the DXF/DWG carry issued codes even when the history file is missing.

## Files

| Item | Location |
| --- | --- |
| History database | `<drawing>.history.vdb` (for example `house.dxf.history.vdb`) |
| Drawing identity | Project key `history` → `{ "drawingId": "…" }` |
| Revision register | Project key `revisions` → codes style, next counter, `entries[]` |
| Revision table ownership | Project key `revisionTableOwned` (per-layout handle lists) |
| Checksum sidecar (unchanged) | `<file>.aadrev.json` via `drawing.revision` |

The history file uses record kinds `0x40`–`0x45` so it never clashes with take-off kinds 1–9. Object and definition rows are content-addressed and shared across milestones.

## Commands

| Command | Role |
| --- | --- |
| `history.save` | Named milestone (`scope`: `current` \| `level` \| `sheet` \| `project`) |
| `history.list` | List milestones (newest first) |
| `history.restore` | Replace (default) or `mode: "copy"`; one undo step; auto “Before restoring …” point first |
| `history.delete` | Delete a non-revision milestone |
| `history.prune` | Drop old automatic points (default keep 20 + one per day for 30 days) |
| `revision.issue` | Snapshot as kind `revision`, append register, set title-block `REV`, redraw tables |
| `revision.list` / `revision.set` / `revision.void` | Read / edit text / void (milestones stay; void cannot be deleted from history) |
| `revision.table` | Draw/replace the table on `A-ANNO-REVT` |
| `revision.codes` | `letters` (default), `numbers`, or `custom` |

Automatic restore points run after a successful DXF/DWG save. A failure to write history is echoed and never fails the save.

## WinUI

- **Window > Revision register** / File > Revisions > Register… → manager grid (issue, void, draw tables).
- **Window > Version history** / File > History > List… → milestone list (save, restore, delete; “Named versions only” filter).

Programmatic `engine.execute` never opens those windows.

## Not in this release

- Compare / preview overlay / revision clouds from diffs (`history.compare`, `history.preview`, `revision.clouds`).
- Office `shilpid` mirroring of history or take-off tags per revision.
- Full day-grouped History side-rail chrome from the UI mock.

## Tests

- `aad-vdb` unit tests in `crates/vdb/src/history.rs`.
- Engine integration: `crates/engine/tests/history_revision.rs` (save/restore, shared rows, issue/void/DXF register round-trip, table replace, hostile file, revision lock).
