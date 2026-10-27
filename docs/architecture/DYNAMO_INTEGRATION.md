# Dynamo companion integration

O-Draft does not embed Autodesk Dynamo. A separate O-Dynamo companion
(`ODynamo.Host`) evaluates a small 2D drafting graph over a local named pipe.
Wall take-off graphs remain on `aad-param` (`param.*`); see [parametric.md](../parametric.md).
Enhancement roadmap: [dynamo-enhancement-plan.md](../dynamo-enhancement-plan.md).

## Pipe protocol (v2)

Named pipe: `ODraft.Dynamo` (Windows only). JSON lines, one request or push per line.

| Method | Direction | Purpose |
|---|---|---|
| `ping` | O-Draft → Host | Liveness |
| `evaluate` | O-Draft → Host | Evaluate the host graph (or `params.graph`) with the stored context |
| `context.levels` | O-Draft → Host | `{ levels: [{name, elevation, height, active}] }` |
| `context.project` | O-Draft → Host | `{ project: { title, owner, …, details, floors } }` |
| `subscribe` | O-Draft → Host | Host pushes `evaluation` on graph edits (debounced 300 ms) |
| `unsubscribe` | O-Draft → Host | Stop pushes |
| `drawing.changed` | O-Draft → Host | Acknowledged; source-entity tracking is later |
| `evaluation` | Host → O-Draft | Push: `{ lines, circles, arcs, inserts, titleFields, diagnostics, generation }` |

Evaluate / evaluation `result` includes `arcs` (radians, CCW from +X), `inserts`
(`blockName`, `x`, `y`, `rotation` degrees, `attributes`, optional `level`), and
`titleFields` (`field`, `value`).

## Engine commands

| Command | Role |
|---|---|
| `companion.connect` | Ping, push levels + project context, mark connected |
| `companion.disconnect` | Drop subscription and connected flag |
| `companion.status` | `{ connected, subscribed, pipe, preview, available }` |
| `companion.evaluate` | One-shot evaluate; stores `preview` |
| `companion.subscribe` / `unsubscribe` | Background reader for live pushes into `preview` |
| `companion.preview` | Last stored preview |
| `companion.commit` | Write `result` (or last preview): lines/circles/arcs/inserts onto named levels; title fields onto the active/first title block; creates missing levels; unknown blocks become warnings; one undo step |

Menus: Architecture > Companion. WinUI invokes these through `app.run` (no separate bridge class).

Non-Windows / wasm: pipe methods return a clear error (`status.available` is false); `companion.commit` from a supplied `result` works on every target.

## Graph context nodes (O-Dynamo)

| Node | Role |
|---|---|
| `levelSelect` | Outputs level `name`, `elevation`, `height` from the last `context.levels` |
| `projectField` | Reads a field from the last `context.project` (top-level or `details`) |
| `blockInsert` | Emits a block insert (`name`, `x`, `y`, `rotation` degrees, `attribs`, optional `level`) |
| `titleField` | Emits a title-block field tag/value (e.g. `REV`, `SCALE`) |

## Verified

- O-Dynamo: `dotnet test` on `ODraft.Graph.Tests` (14 passed), including `blockInsert` and `titleField`.
- O-Draft: `cargo test -p aad-engine --test companion` (10 passed): level commit + undo, block insert with missing-block warning, title-field write.

## Not done

- GPU / overlay ghost of the preview on the canvas (preview is stored on the session only).
- Sheet-set annotation output (Phase 5).
- In-shell WinUI node editor (Phase 6).
