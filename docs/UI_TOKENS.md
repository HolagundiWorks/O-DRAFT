# WinUI design tokens

Canonical token system for `apps/aad-winui`. Visual direction: **compact CAD density**
(AutoCAD 2007–inspired chrome sizing) with **Fluent** restraint — graphite/charcoal dark
workspace, neutral off-white light workspace, and a restrained Windows blue accent.

Full inventory, acceptance checklist, and exceptions: [COMPACT_CAD_UI.md](COMPACT_CAD_UI.md).

## Resource dictionaries

Merged from `App.xaml`:

| File | Contents |
|---|---|
| `Themes/ColorTokens.xaml` | Semantic brushes + title-bar colours (Light / Default / Dark) |
| `Themes/TypographyTokens.xaml` | Font sizes; default faces Segoe UI / Consolas |
| `Themes/SpacingTokens.xaml` | Compact space scale + paddings |
| `Themes/ControlTokens.xaml` | Heights, icon size, radii, Fluent min overrides |
| `Themes/ControlStyles.xaml` | Icon buttons, menus, status/side-panel chrome, inputs |

Code-built UI uses `Density`. At launch, `ShellFonts.Resolve()` upgrades to **Segoe UI Variable** and
**Cascadia Mono** when those font files are installed (WinUI has no CSS-style `FontFamily` fallbacks).
`ShellTheme.VerifyTokens()` logs any missing semantic brushes to `%TEMP%\aad-winui.log`.

## Colour direction

| Role | Dark | Light |
|---|---|---|
| App / title bar | `#1B1B1B` | `#F3F3F3` |
| Elevated surface | `#2C2C2C` | `#FFFFFF` |
| Accent | `#60CDFF` | `#0078D4` |
| Drawing canvas | `#000000` | `#FFFFFF` (CAD preference; not shell chrome) |
| Splash text | `#FFFFFF` on splash background (`SplashForegroundBrush`) | same |
| Selection zebra | Accent at 25% alpha (`Density.SelectionFill`) | Accent at 25% alpha |
| Side panel | `SidePanelWidth` / `Density.SidePanelWidth` = **240** DIP | same |
| AI channel edge | Accent blue | Accent blue |
| CAD channel edge | Success green | Success green |

**No Mica/acrylic on floating command chrome or the drawing HWND.** Solid fills keep overlays
readable over the canvas.

## Dual command channels (visual)

| Channel | Cue |
|---|---|
| Fixed AI bar | Top accent border + `AiChannelLabelStyle` (“Ask”) |
| Cursor CAD field | Transparent overlay + mono type (`CommandChrome`; no box) |

Behaviour: [COMMAND_BAR.md](COMMAND_BAR.md).

## Compact sizing (DIP)

| Token | Value |
|---|---|
| `MenuBarHeight` / `MenuItemHeight` | 24 / 22 |
| `FileTabHeight` / `StatusBarHeight` / `CommandLineHeight` | 24 / 22 / 22 |
| `ControlHeight` / `DialogButtonHeight` | 24 / 26 |
| `IconButtonSize` / `IconSize` | 26 / 16 |
| `PropertyRowHeight` / `PanelHeaderHeight` | 24 / 24 |
| `FontSizeCaption` / `FontSizeBody` / `FontSizeSection` | 11 / 12 / 13 |

## Spacing scale

| Token | DIP |
|---|---|
| `SpaceXs` … `SpaceXxl` | 2, 4, 6, 10, 12, 14, 16 |
| Panel padding | 6×4 (`PaddingPanel`) |
| Dialog padding | 10 (`PaddingDialog` / `Density.DialogMargin`) |

## Intentionally out of scope

- CAD canvas entity colours (host renderer).
- Drawn icon geometry (paths stay the same; display size is 16).
- A permanent large chat panel (would steal drawing space).

## Related

- [COMPACT_CAD_UI.md](COMPACT_CAD_UI.md) — audit, checklist, exceptions
- [WINDOW_SIZING.md](WINDOW_SIZING.md) — content-fit + persisted tool window sizes
- [COMMAND_BAR.md](COMMAND_BAR.md)
- [UI_UX.md](UI_UX.md)
