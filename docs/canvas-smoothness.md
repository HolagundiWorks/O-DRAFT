# O-Draft canvas rendering: review and roadmap

Source plan: *ODraft canvas rendering review and plan* (Oct 26, 2026).  
Branch work: `fix/canvas-phase1-smoothness`.

The canvas already runs on the GPU (wgpu on Direct3D 12) with analytic anti-aliased strokes. It does **not** need a new engine. The rough look comes from specific gaps listed below. Phase 1 is small changes inside the existing renderer; Phase 2 reworks strokes, text and hatches; Phase 3 moves curves onto the GPU.

## How the canvas renders today

Every frame goes through one shared path (WinUI host and optional egui shell). The engine never draws pixels itself; it hands the GPU a pre-tessellated mesh.

1. **Display list** (`crates/render/src/lib.rs`) — entities → world polylines, filled triangles or points. Curves flatten at ~0.2 screen-pixel chord tolerance per zoom octave (`tessellation_band`).
2. **Hatches** (`hatch.rs`, `fill.rs`) — solid/gradient via slab triangulation; patterns as short hairline segments.
3. **Text** (`crates/fonts`) — stroke font as polylines; TrueType via `skrifa`, fixed 6/8 points per curve, ear-clip fill. No hinting, no glyph atlas.
4. **Mesh** (`crates/render/src/mesh.rs`) — fills as triangle lists; every polyline segment an instanced screen-space quad (width in points). Positions f32 relative to a snapped origin.
5. **GPU** (`crates/aad-host/src/gpu.rs`) — wgpu prefers DX12. WGSL expands segment quads and uses distance-to-segment coverage AA. Fills were solid with MSAA off; swapchain is sRGB (linear-light blend).
6. **Caching** (`aad-host`) — display list keyed by revision, zoom band, LOD, space, and (Phase 1) model pan cell. Pans update a uniform. Zoom-band changes rebuild on a worker.
7. **Export** (`raster.rs`) — tiny-skia with AA on (often cleaner than the live canvas).

## Problems (eight)

| # | What you see | Cause |
| --- | --- | --- |
| 1 | Jagged TrueType text, solid hatches, wide fills | Fills full coverage; `SAMPLE_COUNT` was 1 |
| 2 | With LWT on, 0.30 / 0.35 mm thinner than 0.25 mm | `wide_width`: ≤1 pt fell back to 1.5 pt hairline while 0.30→1.13 pt |
| 3 | Dark lines on white look thin / washed out | Coverage blended in linear light (sRGB swapchain) |
| 4 | Circles/arcs heavier and beaded vs straight lines | One quad per segment with round caps; joints double-blend |
| 5 | Big text faceted when zoomed in | Glyph curves fixed 6/8 steps, independent of zoom |
| 6 | Zoom jumps ~1.5×; momentary polygons; tearing while pan | Instant wheel factor; async retessellation; `AutoNoVsync` |
| 7 | Hatch/text pop; moiré when zoomed out | Hard LOD at 25 / 50 wpp; dense patterns full contrast |
| 8 | After deep zoom + far pan, geometry missing until next zoom/edit | Model cull around view centre; cache key omitted centre |

## Decisions (open questions resolved)

| Question | Decision |
| --- | --- |
| Lineweight look | **Screen pixels** in model space (AutoCAD-style; not zoom-scaled). True paper scale is a Phase 2 layout mode. |
| Vsync default | **On** (`AutoVsync`). Opt out with `AAD_VSYNC=0`. Smoothness wins over one frame of cursor lag. |
| Vello | Prototype only after Phase 1–2, if custom shaders still fall short on sheets. |

## Roadmap

### Phase 1 — small, low-risk (most visible gain)

| # | Change | Fixes | Effort | Status |
| --- | --- | --- | --- | --- |
| 1 | 4× MSAA (`SAMPLE_COUNT = 4`) on WinUI GPU path; egui when the paint target allows | #1 | 0.5 d | **Done (WinUI).** egui callback shares egui’s single-sample pass — deferred |
| 2 | Monotonic `wide_width` over the full `Lineweight::STANDARD` list; hairline is the floor | #2 | 0.5 d | **Done** (floor at `HAIRLINE_POINTS`; full-list test) |
| 3 | Gamma-aware AA: coverage contrast curve when linear-out (or UNORM target later) | #3 | 1–2 d | **Partial** (coverage `pow(…, 0.45)`); UNORM path still open for visual tuning |
| 4 | Vsync on by default; `AAD_VSYNC=0` opt-out | tearing in #6 | 0.5 d | **Done** |
| 5 | Smooth zoom: ~120 ms eased animation, accumulate notches; softer per-notch factor | stepped zoom in #6 | 2 d | **Done** (host animates when GPU live; tests snap to end) |
| 6 | Cull + cache include mesh origin / pan cell; rebuild when the view leaves the cell | #8 | 0.5 d | **Done** |

### Phase 2 — strokes, text, hatches

| # | Change | Fixes | Effort | Status |
| --- | --- | --- | --- | --- |
| 7 | Joined stroke strips (miter/round joins, shared caps) | #4 | ~1 w | Planned |
| 8 | Flatten glyphs by zoom tolerance; cache per glyph × octave | #5 | 2–3 d | Planned |
| 9 | Density fade for hatches/text instead of hard LOD cut-offs | #7 | 3 d | Planned |
| 10 | Lineweight modes: screen (model) + true scale (layouts) | presentation | 2 d | Planned |

### Phase 3 — GPU curves / optional Vello

| # | Change | Fixes | Effort | Status |
| --- | --- | --- | --- | --- |
| 11 | GPU arcs/circles/ellipses + Loop–Blinn-style glyph fills | sharp at every zoom | 3–4 w | Planned |
| 12 | Vello prototype for paper / plot preview behind a flag | quality compare | ~1 w | Planned |

## Performance (measure each change)

From `docs/PERFORMANCE.md` (Oct 2026 machine). Re-measure with `aad-cli perf` / `perf_probe` before and after.

| Hotspot | Today | Direction |
| --- | --- | --- |
| Display-list at extents | 13 ms / 10k, 129 ms / 100k | GPU curves (#11); per-entity×octave tessellation cache |
| Text-heavy rebuild | 0.32–0.38 s | Glyph cache (#8); greek bars (#9) |
| Any edit | Full `build_mesh` + new GPU buffers | Spatial mesh tiles; `write_buffer` dirty chunks |
| Layout VP pan across cell | 52 ms / 100k | Existing LRU; GPU curves help |
| GPU frame time | Unknown | Timestamp queries + host `stats` |

## Verification

Each Phase 1 change has a unit test. Before/after screenshots via `ui.screenshot` on Windows (or CI). Cloud/Linux agents cannot run the WinUI shell.

### Phase 1 test commands

```text
cargo test -p aad-render --lib mesh::tests
cargo test -p aad-render --test lod_and_frustum
cargo test -p aad-host present_mode_defaults_to_vsync
cargo test -p aad-host sample_line_and_wheel_zoom
cargo test -p aad-host fine_pan_keeps_the_cached_mesh
```
