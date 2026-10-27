# WinUI window and layout standards

Canonical rules for every window, dialog, manager and panel in `apps/aad-winui`, and the audit
of the existing screens against them. Token values: [UI_TOKENS.md](UI_TOKENS.md). Sizing
mechanics (`ShellFit`): [WINDOW_SIZING.md](WINDOW_SIZING.md). Density background:
[COMPACT_CAD_UI.md](COMPACT_CAD_UI.md).

The goal is that inconsistent windows are hard to create: windows are opened through one
function, laid out from one component set, and checked by `cargo xtask ui` in CI.

## Visual hierarchy

| Level | Element | Treatment |
|---|---|---|
| 1 | Application window | Main shell (`MainWindow`) |
| 2 | Workspace and document tabs | `FileTabs` (`CompactTabViewItemStyle`, `FileTabHeight`) |
| 3 | Major panels | Side column (Properties, Layers, Blocks, Project), tool strips |
| 4 | Dialog / manager title | Native caption bar only; no second title inside the content |
| 5 | Section headings | `Ui.Section` / `DialogLayout.Heading`: 13 semibold, secondary colour, heading level 2 |
| 6 | Labels and fields | Body 12; labels in a shared column (`Ui.Form`) or directly above the field (`Ui.Labeled`) |
| 7 | Descriptions, validation, secondary actions | `Density.Secondary`, `Ui.Status` (live region), secondary buttons |

One primary action per footer (accent fill). Destructive actions use `DestructiveActionStyle`
(error-coloured text). No large headings or decorative borders in utility dialogs.

## Tokens

4 px spacing scale; the names match the requirement table.

| Token | DIP | Use |
|---|---|---|
| `Space1` | 4 | Tight control spacing, label-to-field gap |
| `Space2` | 8 | Standard control spacing, form row spacing, button gap |
| `Space3` | 12 | Group spacing, dialog content padding |
| `Space4` | 16 | Section padding |
| `Space5` | 20 | Major group separation |
| `Space6` | 24 | Large layout separation |

The older names (`SpaceSm` … `SpaceXxl`) remain as aliases snapped to the scale. `SpaceXs` (2)
is kept only for icon strips.

| Element | Target | Implemented |
|---|---|---|
| Toolbar icon | 16–20 | `IconSize` 16 in a 26 button |
| Input height | 26–30 | `ControlHeight` 26 (also the Fluent `TextControlThemeMinHeight` / `ComboBoxMinHeight`) |
| Compact button height | 26–30 | `DialogButtonHeight` 26 |
| Label-to-field gap | 4–6 | `Space1` 4 |
| Form row spacing | 6–8 | `Space2` 8 |
| Dialog content padding | 12–16 | `PaddingDialog` / `Density.DialogMargin` 12 |
| Section spacing | 12–16 | `Space3`–`Space4` |
| Title-bar height | 32–36 | Native caption (32 at 100 % scaling) |
| Corner radius | 4–6 | `CornerRadiusControl` / `CornerRadiusButton` 4 |

Inputs and buttons share one height (26) so controls in the same row line up. Property rows in
the side panel are 26 as well. Shell chrome (menu 24, status 22, tabs 28) stays denser than
dialogs on purpose.

## Window templates

Every secondary window calls `StandardWindow.Present(window, owner, template, persistKey?, bounds?, modal?)`
(`Standard.cs`) after setting its `Content`. It applies the themed chrome and icon, makes the
main shell the owner, sizes the window, sets the caption buttons and the Escape key.

| Template | For | Sizing | Caption buttons | Keys |
|---|---|---|---|---|
| `Dialog` | Confirmations, small forms, settings | Content-fit, not resizable (320×72 to 560×640) | Close | Esc closes |
| `PropertyDialog` | Property and style dialogs, engine forms | Content first, then user-resizable, size remembered (400×320 to 720×800) | Close | Esc closes |
| `Manager` | Layer, hatch, plot style, page setup, dimension / text / multileader style managers | Resizable, size remembered (560×360 to 1400×1000) | Maximize, close | — |
| `Floating` | Palettes beside the canvas (quick properties, tool bar picker, command list) | Resizable, size remembered (260×240 to 480×900) | Close | — |

Rules:

- **Ownership.** Every secondary window is owned by the main shell (`GWLP_HWNDPARENT`): it stays
  above the drawing, minimizes and restores with the shell, and has no taskbar button of its own.
  Owned windows have no minimize button. Nothing uses system-wide always-on-top any more.
- **Modality.** `modal: true` disables the owner until the window closes and then reactivates it.
  Only the save prompt is modal; managers are modeless so they work beside the canvas.
- **Bounds.** Use the template's bounds. Pass `WindowBounds` only when the content needs more
  (the layer table needs 720 DIP; the hatch grid 480×420; drafting settings 480 tall).
- **Title bar.** One strategy: the native caption, painted with the theme colours
  (`ShellChrome`), `ExtendsContentIntoTitleBar = false`. No custom title bars and no duplicate
  title inside the content.

## Components

Code-built windows compose these (`Standard.cs`, `DialogLayout.cs`, `Density.cs`); they read
only tokens.

| Requirement name | Implementation |
|---|---|
| StandardDialog | `WindowTemplate.Dialog` / `PropertyDialog` + `Ui.DialogPage(content, footer)` (scrolling content over a pinned footer) |
| StandardManagerWindow | `WindowTemplate.Manager` + `Ui.ManagerPage(toolbar, content, footer)` (content takes the free space) |
| CompactTitleBar | Native caption via `ShellChrome` (see Title bar) |
| DialogFooter | `Ui.Footer(status, actions…)`: status / validation left, actions right, `Space2` apart; XAML `DialogFooterStyle` |
| FormSection | `Ui.Section(heading, body…)` |
| FormRow | `Ui.Form((label, field)…)` (aligned label column) or `Ui.Labeled(label, field)` (label above) |
| StandardToolbar | `Ui.Toolbar(items…)`, `Ui.ToolbarDivider()`; XAML `ManagerToolbarStyle`, `IconButtonStyle`, `ToolStripDividerStyle` |
| StandardTabBar | `CompactTabViewItemStyle` (document tabs) |
| StandardDataGrid | Not yet a component: the layer table is hand-built in `LayerManagerWindow` (see follow-ups) |
| StandardPropertyPanel | Side-panel Properties in `MainWindow.xaml.cs` (`PropertyRowHeight`) |
| Button styles | `Ui.Action(label, ActionKind.Primary / Secondary / Destructive)`; XAML `PrimaryActionStyle`, `CompactButtonStyle`, `DestructiveActionStyle` |
| Inputs | `Density.DialogField`, `Density.DialogCombo`, `DialogLayout.Field`; XAML `DialogFieldStyle`, `DialogComboStyle`, `CompactCheckBoxStyle`, `CompactRadioButtonStyle` |
| Typography, spacing, colour | `Themes/*Tokens.xaml` and `Density` |

Icon-only buttons need `ToolTipService.ToolTip` and `AutomationProperties.Name`.

## Enforcement

`cargo xtask ui` (part of `cargo xtask ci`, so it runs on every pull request) scans
`apps/aad-winui`:

1. A file that creates a window (`new Window …` or a `: Window` class) must call
   `StandardWindow.Present`; direct `ShellIcon.Apply` / `ShellFit.*` calls are refused outside the
   design-system files. Listed exceptions: `App.xaml.cs` (main window), `MainWindow.xaml.cs`
   (PDF, Panels and Clear history windows still to move), `CanvasMenu.cs` (borderless popup),
   `CommandChrome.cs` (command overlays), `SmartBlockWindow.cs` (from the smart-blocks change).
2. Hard-coded dimensions (`Margin`, `Padding`, `Spacing`, sizes, `FontSize`, `CornerRadius` written
   as numbers in C# or XAML) are counted per file and may not exceed the recorded count. New
   files start at zero. `cargo xtask ui --counts` prints the counts so a cleaned file's limit can
   be lowered in `xtask/src/ui.rs`.

The rules are unit-tested in `xtask/src/ui.rs`. They do not render anything: visual review is
still manual (see Validation).

## Audit of existing screens (2026-10-27)

Shared root causes found before the fixes:

1. **No ownership.** Only the shortcut menu and command overlays had an owner. Every dialog and
   manager was an independent top-level window: own taskbar button, could fall behind the
   drawing, minimize button that stranded it. The save prompt used system-wide always-on-top.
2. **Per-window chrome code.** Each window repeated `ShellIcon.Apply` + `ShellFit.*` with its own
   min/max numbers (22 different sets) and its own presenter flags.
3. **Off-grid spacing.** The scale was 2/4/6/10/12/14/16; dialog padding 10, label gap 2.
4. **Two implementations of the same component.** `Density.FieldRow` (label 12) and
   `DialogLayout.Labeled` (label 11); `Density.DialogStack` and `DialogLayout.Root`.
5. **Primary not distinguishable.** `PrimaryActionStyle` was the neutral button style; there was no
   destructive style.
6. **Footers scrolled away.** Dialogs put the whole page, actions included, in one `ScrollViewer`.
7. **Text encoding.** Five sources were Windows-1252, and several strings had lost their
   punctuation to `?` (tooltips such as "ANSI31 ? description", "Load?", "Current style: ?").

| Window | File | Owner | Template now | Size (min → max DIP) | Notes |
|---|---|---|---|---|---|
| Layer Properties Manager | `LayerManagerWindow.xaml(.cs)` | Shell | Manager | 720×420 → 1400×1000 | Toolbar split into layer actions + search, and a **Layer states** group (restore, delete as destructive, save); footer uses `DialogFooterStyle`; cell padding tokens |
| Hatch Editor | `HatchWindow.cs` | Shell | Manager | 480×420 → 1200×1000 | Was one scrolling stack capped at 560 wide. Now the pattern grid takes the free space, Scale / Angle in an aligned form section, pinned footer (status, Set as defaults, Apply as primary) |
| Dimension / Text / Multileader Style | `DimStyleWindow.cs`, `TextStyleWindow.cs`, `MLeaderStyleWindow.cs` | Shell | Manager | 560×360 → 1400×1000 | Content still capped (`MaxWidth`) and page-scrolled; footer not yet pinned |
| Plot Style Manager | `PlotStyleWindow.cs` | Shell | Manager | as above | As above |
| Page Setup Manager | `PageSetupWindow.cs` | Shell | Manager | as above | As above |
| Linetype Manager | `LinetypeWindow.cs` | Shell | PropertyDialog | 400×320 → 720×800 | Separator / placeholder punctuation fixed |
| Batch Plot | `BatchPlotWindow.cs` | Shell | PropertyDialog | as above | Content capped at 480 |
| Parameters Manager | `ParametersWindow.cs` | Shell | PropertyDialog | as above | Content capped at 480 |
| Drafting Settings | `DraftingWindow.cs` | Shell | PropertyDialog | 400×480 → 720×900 | |
| Engine forms | `FormDialog.cs` | Shell | PropertyDialog | 400×320 → 720×800 | Label directly above each field; validation line and OK / Cancel in a pinned footer |
| Save prompt | `SavePrompt.cs` | Shell, **modal** | Dialog | content | Pinned footer; Esc = Cancel; no always-on-top |
| Units, Point Style, Room Label Settings | `UnitsWindow.cs`, `PointStyleWindow.cs`, `RoomSettingsWindow.cs` | Shell | Dialog | content | Converted to UTF-8 |
| Quick Select, Calculator, Pomodoro, About | `QuickSelectWindow.cs`, `CalcWindow.cs`, `PomodoroWindow.cs`, `AboutWindow.cs` | Shell | Dialog | content | Pomodoro read-out and About logo use tokens |
| Quick Properties | `QuickWindow.cs` | Shell | Floating | 260×240 → 480×900 | Was always-on-top system-wide |
| Tool bar picker | `ToolsWindow.cs` | Shell | Floating | as above | |
| Command list | `CommandList.cs` | Shell | Floating | 360×280 → 720×800 | |
| Shortcut menu | `CanvasMenu.cs` | Shell | Exception (popup) | content | Borderless, dismisses on deactivate |
| Command overlays | `CommandChrome.cs` | Shell | Exception (overlay) | layout | |
| PDF preview, Panels, Clear history | `MainWindow.xaml.cs` | None | Not converted | per call | Waits for the smart-blocks change, which edits the same file |
| Smart blocks | `SmartBlockWindow.cs` (smart-blocks change) | — | Not converted | — | Convert after it lands |
| Block manager | — | — | — | — | There is no Block Manager window; blocks are the side-panel Blocks tab (changed by the smart-blocks work) |

Not covered by this audit: the egui window (optional `--egui`), which has its own theme in
`crates/ui-egui`.

## Validation

Automated (CI): `cargo xtask ui` rules and their unit tests; the Windows job builds the WinUI
shell (`dotnet build apps/aad-winui/Aad.WinUi.csproj`).

Manual, per changed screen, at 100 %, 150 % and 200 % scaling and in both themes:

1. Open from the menu and from the typed command; the window opens above the drawing and has no
   taskbar button of its own.
2. Minimize the shell: the window hides with it and returns with it.
3. Resize to the minimum: nothing clips or overlaps; tables and lists scroll inside.
4. Resize larger (resizable templates): the main content grows; no empty band appears.
5. Tab through every control; focus is visible; Esc closes dialogs (not managers).
6. Primary action is the accent button; destructive actions are red text.
7. Long names (layer names, pattern descriptions) trim or wrap; empty lists show their empty text.
8. Open and close ten times; the process exits when the shell closes.
9. Save prompt: the shell cannot be clicked while it is open, and gets focus back after.

Screenshot regression baselines (`docs/baselines/`) and automated UI tests (WinAppDriver /
UI Automation) do not exist yet.

## Follow-ups

- Move the PDF, Panels and Clear history windows in `MainWindow.xaml.cs`, and `SmartBlockWindow.cs`,
  to `StandardWindow.Present` once the smart-blocks change has merged.
- Give the style managers (dimension, text, multileader, plot style, page setup, linetype, batch
  plot, parameters) `Ui.ManagerPage` / `Ui.DialogPage` layouts so lists fill the window and
  footers stay pinned, then drop their content `MaxWidth` caps and lower their literal counts.
- A `StandardDataGrid` component (header row, row height, zebra, selection, sort) extracted from
  the layer table.
- `ShellFit` measures with unbounded width, so wrapped text pushes the first size to the maximum;
  measure at a preferred width instead, which removes the need for content caps.
- Brushes resolved in code (`Density.ThemeBrush`, used by `Ui.Footer` and the panels) are read once
  when the element is built, so a Dark/Light switch repaints the window fill but not those parts
  until the window is reopened. Same limitation as before this change; fix by moving them to
  `ThemeResource` lookups in XAML styles.
- Screenshot baselines through the control channel (`ui.screenshot`) and UI Automation tests.
