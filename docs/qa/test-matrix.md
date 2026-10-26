# Test matrix (campaign)

Status vocabulary: `pass` | `fail` | `blocked` | `not_run` | `partial`.

## Phase coverage

| Phase | Status | Notes |
|---|---|---|
| A Baseline | partial | Env, builds, key lib suites, perf N=1k/10k/100k, cold start |
| B Screen inventory | done (inventory only) | [screen-inventory.md](screen-inventory.md) — all `not_tested` |
| C Screen functional | partial | Launch, Layer combo, Properties glimpse; Escape/layer fail |
| D CAD commands | not_run | Registry exercises via existing engine tests only |
| E Undo / documents | not_run | |
| F DXF/DWG compatibility | partial | aad-io lib green after QA-2026-002; no large corpus walk |
| G Performance profile | not_run | Baseline numbers only; no stage traces |
| H Optimization | not started | No measured bottleneck fixed yet |
| I Stress / soak | not_run | |
| J Visual / a11y | partial | Two screenshots; no full token audit |
| K Automation expansion | not_run | |
| L Release readiness | draft | [release-readiness.md](release-readiness.md) |

## Representative cases executed

| ID | Case | Expected | Status | Evidence |
|---|---|---|---|---|
| TM-001 | aad-host lib suite | all pass | pass | artifacts/aad-host-lib.txt |
| TM-002 | aad-engine lib suite | all pass | pass (after fix) | artifacts/aad-engine-lib-after-fix.txt |
| TM-003 | aad-doc lib suite | all pass | pass | artifacts/aad-doc-lib.txt |
| TM-004 | aad-io lib suite | all pass | pass (after fix) | artifacts/aad-io-lib-after-fix.txt |
| TM-005 | aad-cli perf 1k/10k/100k | timings recorded | pass | artifacts/perf-*.txt |
| TM-006 | WinUI cold launch | titled window | pass | artifacts/winui-cold-start.txt; screenshots |
| TM-007 | Layer combo open | list shows layers | pass | visual session |
| TM-008 | Escape closes Layer combo | closed | fail | QA-2026-004 |
| TM-009 | Empty drawing clean (WinUI) | no dirty mark | fail | QA-2026-003 |
| TM-009b | Empty Session::new dirty | dirty=false | pass | artifacts/engine-smoke-wall-undo-dxf.txt |
| TM-009c | wall.draw + wall.door + undo/redo + DXF round-trip | counts restore; reload | pass | artifacts/engine-smoke-wall-undo-dxf.txt |
| TM-010 | Every WinUI dialog | functional pass | not_run | screen-inventory |
| TM-011 | Every registered CAD command via UI | correct geometry | not_run | |
| TM-012 | Undo/redo multi-step | restore invariants | not_run | |
| TM-013 | DXF round-trip apartment.dxf | visual+count | not_run | |
| TM-014 | DWG isolation / mutation | no crash | not_run | |
| TM-015 | Pan/zoom 100k frame times | p95 recorded | not_run | |
| TM-016 | `cargo xtask ci` | green | not_run | |
