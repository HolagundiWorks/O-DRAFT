# WinUI window sizing policy

Sizing mechanics for secondary windows in `apps/aad-winui`. Which window gets which policy is
decided by its template in [UI_STANDARDS.md](UI_STANDARDS.md): a window calls
`StandardWindow.Present`, and that picks the mode and the bounds below. The main CAD shell and
its drawing child are **not** auto-sized from document content.

## Helpers

| Type | Role |
|---|---|
| `StandardWindow` | Apply a window template: themed chrome, shell ownership, the `ShellFit` mode and bounds for that template, caption buttons, Esc, optional modality |
| `ShellFit` | Measure content (`DesiredSize`), add non-client overhead, clamp to min/max and the monitor work area, then `AppWindow.Resize` |
| `ShellWindows` | Track every secondary top-level window; close them when the main shell closes so the process exits |
| `ShellChrome` / `ShellIcon.Apply` | Solid theme chrome; `Apply` also registers the window with `ShellWindows` |

## Modes (`ShellFitMode`)

| Mode | When | Behaviour |
|---|---|---|
| **Content** | Small dialogs (About, Save prompt, clear history, calculator, Pomodoro, units, rooms, point style, quick select, canvas menu) | Size once to measured content on load; not user-resizable |
| **Adaptive** | Engine form dialogs (`FormDialog`); other forms that grow/shrink with content | Re-measure after content changes (`ShellFit.Invalidate`) until the user resizes |
| **UserResizable** | Layer manager, hatch, styles, plot/page setup, drafting settings, PDF, tools, command list, quick properties | Initial content-based size; after the user resizes, keep their size (persisted in `shell.json` when `persistKey` is set) |

Templates map onto the modes: `Dialog` → Content, `PropertyDialog` / `Manager` / `Floating` →
UserResizable. Adaptive is no longer used by any window (engine forms moved to `PropertyDialog`
to avoid SizeChanged↔Resize flicker).

## Limits

- Defaults come from the window's template (`WindowBounds.For`); a call passes `WindowBounds` only when its content needs more. Typical content max is about 640×640 DIP, managers up to 1400×1000 DIP.
- Content is measured with unbounded width, so a window whose text wraps can open at its maximum width; such windows cap their content with `MaxWidth` until the measure is changed (see [UI_STANDARDS.md](UI_STANDARDS.md#follow-ups)).
- Target size is clamped to the nearest monitor **work area** (taskbar excluded), with a small margin.
- If content exceeds the max or work area, the window stops growing; content should scroll inside the window.
- DPI: measurements are in DIP; resize uses pixels via `XamlRoot.RasterizationScale` and measured frame overhead (`GetWindowRect` − `GetClientRect`).

## Excluded (do not auto-fit from drawing content)

- Main `MainWindow` (user/desktop size, maximise, saved shell prefs).
- Drawing canvas / swap-chain child.
- Command chrome (`CommandChrome` AI bar and cursor CAD field) — placed and sized by the shell layout, not `ShellFit`.

## Ownership and modality

Every secondary window is owned by the main shell (`GWLP_HWNDPARENT`), so it stays above the
drawing, hides and restores with the shell, and has no taskbar button; owned windows have no
minimize button. `StandardWindow.Present(..., modal: true)` disables the owner until the window
closes (only the save prompt). Details: [UI_STANDARDS.md](UI_STANDARDS.md#window-templates).

## Process lifetime

Closing the main window closes all tracked secondary windows (`ShellWindows.CloseAllOwned`)
and calls `Application.Exit`. Leaving tool windows open after the shell closed used to keep
the unpackaged process alive.

## Verification (manual)

1. Open About — window fits the short stack; no large empty region.
2. Open Layer manager — resize it; close and reopen — size restores from `%LocalAppData%\AAD\shell.json` (`windowSizes.layers`).
3. Open Hatch / Room settings — solid shell chrome; size within the work area on a small laptop display.
4. Open calculator while the sample drawing is open, then close the main window — `ODraft.WinUi` process exits (no orphan dialogs).
5. Theme toggle with a tool window open — chrome theme updates; size is not forced if the user already resized.
6. Engine form with a validation error — window grows via `ShellFit.Invalidate` (Adaptive) when the error line appears.

Compile check: `dotnet build apps/aad-winui/Aad.WinUi.csproj -c Debug -p:Platform=x64`. Token resolve check runs at launch (`ShellTheme.VerifyTokens`).
