# Performance audit (2026-10-30)

Test drawing: `DEMO.dwg` (OneDrive\Documents\OSAS TEST, 1.3 MB, 9 672 entities, seven sheets of plans, details and title blocks). Machine: RTX 4060 laptop GPU, 16 GB. All numbers are from the shell's own control port, so they include the whole path (command, display list, mesh, GPU present, panels).

## What was already there

O-Draft already draws on the GPU through its own engine: `aad-render` builds a display list (LOD by zoom band, view culling, tiled meshes, a worker thread for band rebuilds) and `aad-host/gpu.rs` presents it with wgpu on DirectX 12, `HighPerformance` adapter, vsync (`AAD_VSYNC=0` turns it off). So the slowness was not "no hardware acceleration" and a new graphics engine would not have fixed it. It was CPU work and data volume feeding the GPU, and UI-thread work after every change.

## What was measured, and what was slow

| Step | Before | After |
|---|---|---|
| A zoom command (rpc, UI thread blocked) | 320 to 680 ms | 15 to 50 ms |
| A pan command | 330 to 490 ms | 15 to 30 ms |
| Mesh data at one zoom step (words sent to the GPU) | 14.3 million | 0.33 million |
| Display list at the next steps | 392 000 primitives, 4.7 million triangle vertices | 25 600 primitives, 27 000 triangle vertices |
| Display list build, one zoom step | 90 to 160 ms | 39 ms |
| Mesh build | 32 to 150 ms | 1.5 to 5 ms |
| Open `DEMO.dwg` (parse + first list) | 0.4 s + 0.25 s | unchanged (one-time) |

Causes, found with `AAD_PERF=1` and `cargo run --release -p aad-host --example profile_render -- file.dwg`:

1. **Text and hatches drawn at full detail however small they were on screen.** The LOD switches were absolute (world units per pixel), so they did not suit a drawing in other units; 4.7 million triangle vertices of glyphs and 340 000 hatch lines were built for text and patterns a few pixels tall. Now each text is drawn by its own height on screen (under 1.5 px: not drawn; under 6 px: a grey bar; else glyphs) and each pattern hatch whose line spacing is under 3 px becomes a tint (`Options::screen_px`; `render/src/lib.rs`, `hatch.rs`).
2. **Every zoom command rebuilt the display list on the UI thread.** The `zoom` command bumped the document revision, which the display-list cache treats as an edit. In model space it no longer does (the cache is keyed by zoom band and pan cell); paper space and viewports still redraw.
3. **Every pointer move over a new object rebuilt all side panels (about 1 s).** The Blocks thumbnails (0.5 s), the Layers list, the quick-properties window and the side rail were rebuilt on each hover change. Now only the panel on screen is rebuilt (the others when their title is chosen), the rail restyles only what changed, a hidden Layer Manager is not reloaded, and the quick-properties window is only rebuilt when its content changes.

## Tools left in

- `AAD_PERF=1` (set before starting O-Draft): ticks of the UI loop over 40 ms, with the time of each step, and host redraws over 20 ms, go to `%LOCALAPPDATA%\OSAS\odraft-perf.log`. Free when off.
- `cargo run --release -p aad-host --example profile_render -- <file.dwg> [cx cy height]`: display list and mesh time and size at six zoom steps, and what text and fill cost.

## Still slow

- Opening a large drawing builds the first display list on the UI thread (about 0.2 s for this one).
- A first zoom that crosses a zoom band still builds a list (39 ms here, on a worker), so a very large drawing will show the old detail for a moment.
- Interactive wheel zoom and middle-button pan were not timed separately (the control port drives commands); they use the same list cache.
