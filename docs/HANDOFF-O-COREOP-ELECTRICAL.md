# Electrical take-off hand-off (O-Draft → O-CoreOp)

O-Draft records switchboard and circuit rows in the same ShilpiDB take-off as walls and openings
(`takeoff.record`). The contract note is kinds 10 and 11 in O-CoreOp `docs/TAKEOFF-SHILPIDB.md`.

## What O-Draft writes

`electrical.connect` stores two arrays on the drawing's `electrical` project entry, and
`bridge.collect` / `takeoff.record` copy them:

| Kind | Bridge key | Columns |
|---|---|---|
| 10 | `electrical` | `switchboard`, `height` (null until a mounting height is stored), `5amp_socket`, `15amp_socket`, `connected_fixture`, `two_way_switches`, `one_way_switches` |
| 11 | `electrical_circuits` | `switchboard`, `point`, `kind`, and the same count columns as the board row (`5amp_socket`, `15amp_socket`, `connected_fixture`, `two_way_switches`, `one_way_switches` — each 0 or 1 for that point) |

`height` is null. Mounting height is not read from the block yet. Circuit sizing, cable length and
demand load are not computed.

`electrical.autonumber` defaults to on: inserting a block whose name matches the electrical map
numbers it (`electrical.number` rules). `{enabled: false}` turns that off.

## What O-CoreOp reads

`ShilpiTakeoff` maps kind 10 to `electrical` and kind 11 to `electrical_circuits`. `CadImporter`
upserts those rows onto sheets of the same names (`source_ref` = `ref`), the same way as walls.
The take-off page poll (5 s) lists a drawing as soon as its header is `complete` and says so when
a drawing that was not complete becomes complete. Import is still the user's Read action.

Masonry rows gain `wall_type`. Plaster proposal uses that: external walls are one external face
and one internal face; internal walls are two internal faces.
