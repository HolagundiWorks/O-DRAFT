# Environment (Phase A)

Captured: 2026-10-26.

## Hardware and OS

| Item | Value |
|---|---|
| OS | Microsoft Windows NT 10.0.26300.0 |
| CPU | AMD Ryzen 7 7435HS (16 logical processors) |
| RAM | 15.8 GB |
| GPU | NVIDIA GeForce RTX 4060 Laptop GPU (~4 GB reported AdapterRAM) |
| Primary display | 1920×1080, 32 bpp, working area 1872×1080 |
| Display scaling | LogPixels unset (treated as 96 DPI / 100%) |

## Toolchain

| Tool | Version |
|---|---|
| rustc | 1.99.0 (b940084d7 2026-09-28) |
| cargo | 1.99.0 (5f94df478 2026-08-27) |
| rustup active | stable-x86_64-pc-windows-msvc |
| .NET SDK | 8.0.425 |

## Git baseline

| Item | Value |
|---|---|
| HEAD | `2f4af48975846fd71747389ce94d4d71af4dfc0a` — *Ship takeoff restore, floor regions, and WinUI shell polish.* |
| Branch | `main...origin/main` |
| Working tree | Dirty: UI consistency refactor, polar tooltip fill, eTransmit removal, dynamic openings, layer panel changes (not committed) |

QA measurements that depend on host DLL or WinUI chrome use **this dirty tree**, not clean HEAD.

## Architecture (audit)

| Layer | Location | Notes |
|---|---|---|
| WinUI 3 shell | `apps/aad-winui` | Primary product UI; unpackaged net8 x64 |
| Host ABI | `crates/aad-host` | Session + canvas mesh + panels JSON |
| Engine / commands | `crates/engine` | Command registry, undo snapshots |
| Document model | `crates/doc` | Entities, blocks, layers, project JSON |
| Geometry | `crates/geom` | Primitives, transforms |
| Render | `crates/render` | Display list / tessellation |
| IO | `crates/io` (+ DWG worker) | DXF; DWG via isolated worker |
| Snapping / grips | `crates/engine` `snap`, `grips` | |
| Optional egui shell | `crates/ui-egui`, `apps/aad` `--egui` | Secondary; not the sale WinUI path |

## Build configurations under test

| Config | Command | Status |
|---|---|---|
| Engine + host release | `cargo build -p aad-host --release` | built earlier this session (openings + panels) |
| WinUI Debug x64 | `dotnet build apps\aad-winui\Aad.WinUi.csproj -c Debug -p:Platform=x64` | built earlier this session |
| Engine Debug tests | `cargo test -p aad-engine --lib …` | see [regression-report.md](regression-report.md) |
| Full `cargo xtask ci` | not yet run this campaign | |
| WinUI Release | not yet run this campaign | |
