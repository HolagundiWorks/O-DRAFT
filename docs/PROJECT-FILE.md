# The project file (.osp) in O-Draft

A project file is one SQLite file with the drawing, the take-off and the project record (format `osas-project` v1). O-Draft and O-CoreOp open the same file. Suite-wide names, roles and links: [SUITE.md](SUITE.md). Format and sync plan: OSAS repository `docs/ODP-PROJECT-FILE-AND-SYNC.md`.

| Part | Where |
|---|---|
| Format library | `crates/odp` (crate `aad-odp`): create/open, edit + commit (+1 `meta.revision`), reads, `replace_takeoff` (writes only differences), examples `pack`, `inspect`, `stress`; tests include a file written by the .NET side and four concurrent writers |
| Open / save | `crates/engine/src/cmd/file.rs`: `ContainerHooks` (read/write), `format_hint` (a new project file takes `dxf` unless told). The host writes the drawing back in place only when its bytes changed |
| Take-off | `crates/engine/src/cmd/takeoffrec.rs`: `takeoff.record` writes into the open `.osp` (`OdpSink`); with no project open it asks the user to save as one. `path` may name another `.osp` |
| UI | File > Open / Save As offer `.osp` ("Project (drawing and take-off)"); the Take-off toggle in the bottom bar shows the take-off tools |
| Pointer | `apps/aad-winui/ActiveProjectPointer.cs` writes `%LOCALAPPDATA%\OSAS\active-project.json` with `project` = the open `.osp` |

ShilpiDB is gone from O-Draft: the `library.*` and `vdb.*` commands, the `.vdb` sidecar, the status chip, the agent's ShilpiDB tools and the `shilpid` bundling were removed. The drawing history now lives in the project file (`history_record` table; `<drawing>.history.osp` beside a drawing that is not one) and does not count as a revision.
