# Canvas smoothness

How the WinUI GPU path paints the drawing, and the plan to make it look less jagged.

## Pipeline (today)

1. `aad_render::build` → display list (tessellation, LOD, model frustum cull).
2. `aad_render::mesh::build_mesh` → vertex/instance bytes; hairlines become screen-space quads.
3. `aad-host` wgpu (DX12 preferred) uploads the mesh and presents into the Win32 child HWND.

Stroke anti-aliasing is fragment coverage on those quads. Fills and TrueType glyphs need MSAA.

## Lineweights in model space

**Decision (Phase 1):** With LWDISPLAY on, object lineweights are **screen pixels** derived from millimetres (`lw × 3.78` points), never thinner than the hairline floor, and monotonic in lw. They do **not** scale with zoom (AutoCAD model-space behaviour). Plotting still uses real millimetre widths. Exact plotted width in the model view remains a wide polyline, not a lineweight.

## Phase 1 — six small fixes

| Fix | Change |
|---|---|
| Jagged fills / text | `SAMPLE_COUNT = 4` on the WinUI GPU path |
| 0.30 / 0.35 thinner than 0.25 | `wide_width` floors at `HAIRLINE_POINTS` (0.25–0.35 mm share the hairline until ~0.40 mm) |
| Dark lines look thin on white | Coverage raised toward display gamma when the swapchain is linear |
| Steppy zoom | Wheel ~1.22× per notch (was ~1.49×) |
| Tearing | Default present mode `AutoVsync` (`AAD_VSYNC=0` for low-latency) |
| Missing geometry after zoom + pan | Model cull covers the mesh origin cell; list cache keys that cell |

Each fix has an automated test. Before/after screenshots need a Windows machine.

## Phase 2 / 3 (later)

Phase 2: sharper SHX/stroke text at mid zoom, hatch edge AA polish, optional LWDISPLAY defaults.  
Phase 3: edit/zoom timing (worker rebuild cost, LOD band hysteresis) once Phase 1 is verified on screen.

## Timing notes

- Fine pan: view uniform only (mesh key stable inside the origin cell).
- Crossing an origin cell or zoom/LOD band: display-list rebuild (async for band-only; sync when the pan cell changes).
- Edits: incremental `rebuild` when the pan cell and LOD band match.
