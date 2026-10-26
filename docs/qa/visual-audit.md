# Visual audit

Fixed window: primary 1920×1080 @ ~100% scaling. Theme: light. Build: WinUI Debug x64 (dirty tree).

| Shot | File | Notes | Status |
|---|---|---|---|
| Empty main | [screenshots/01-main-empty.webp](screenshots/01-main-empty.webp) | Menu, arch bar (Layer/Space/Floor), tool strip, Ask pill, collapsed Properties | captured |
| Focused main | [screenshots/02-main-focused.webp](screenshots/02-main-focused.webp) | Same chrome; used for click targeting | captured |

## Observed (not full token audit)

- Architecture bar shows Layer | Space | Floor as designed in recent UI work.
- Ask pill overlays the lower canvas (“Ask about the drawing…”).
- Properties rail collapses to a right edge tab.
- Dirty asterisk on empty drawing: QA-2026-003.
- Layer dropdown Escape: QA-2026-004.

## Not captured

Managers (Layer, Hatch, Dim/Text styles, Units, Plot, Batch Plot, Parameters, Calculator, Pomodoro),
Classic vs Pro, dark theme, 150% DPI, dialog ActionRow alignment pass.
