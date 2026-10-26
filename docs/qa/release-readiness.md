# Release readiness (campaign snapshot)

**Honest status: not release-ready on the basis of this campaign alone.**

## Blockers for claiming “QA complete”

1. Screen inventory is inventory-only — nearly all surfaces remain `not_tested`.
2. No systematic CAD command / undo / save-reload campaign through WinUI.
3. No pan/zoom/GPU frame profiling; no optimization with before/after proof.
4. `cargo xtask ci` not run this campaign.
5. Open UI defects QA-2026-003 (dirty empty drawing), QA-2026-004 (Escape/Layer).

## Known pre-existing limits (not fixed here)

From [KNOWN_ISSUES.md](../KNOWN_ISSUES.md): BUG-001 residual, session-thread batch plot /
command-line open-save, ~125 ms full display-list at 100k extents, large DWG worker latency,
block-definition lock gap, unsigned installer / NOTICE gaps.

## What *was* verified this campaign

- Host / engine / doc / io lib suites green after two test fixes.
- Release `aad-cli perf` numbers at 1k / 10k / 100k entities on this hardware.
- WinUI Debug launches (~470 ms to titled window) and shows expected chrome.
- Screen inventory generated for WinUI + engine menus.

## Recommended next work (priority)

1. Root-cause QA-2026-003 and QA-2026-004.
2. Drive FormDialog + Layer Manager + Hatch + Units through a scripted control-channel /
   computer-use checklist; update screen-inventory statuses with evidence.
3. Engine command smoke: sample registry IDs via `Session::execute` + one UI path each for draw /
   modify / annotate / wall.door.
4. Repeat `aad-cli perf` 5× for median/p95; profile display-list 100k (~762 ms in this run).
5. Run `cargo xtask ci` and record in regression-report.

## Outstanding product risks (dirty tree)

Uncommitted: UI consistency, polar tooltip, eTransmit removal, dynamic openings. Sale builds from
**committed** HEAD still include eTransmit until that change is committed.
