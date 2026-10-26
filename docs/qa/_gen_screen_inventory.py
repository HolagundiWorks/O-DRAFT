"""Generate docs/qa/screen-inventory.md from WinUI scan + engine menus."""
from __future__ import annotations

from pathlib import Path

OUT = Path(__file__).with_name("screen-inventory.md")
MENU_TSV = Path(__file__).with_name("_menu_cmds_temp.tsv")
ROOT = Path(r"D:/Work Development/Repos/O-DRAFT")
WINUI = ROOT / "apps/aad-winui"

NOT_TESTED = "not_tested"
EVIDENCE = "—"


def row(id_: str, surface: str, typ: str, source: str, opened: str) -> str:
    # Source column: plain path/text (no fenced backticks — nested ` breaks tables).
    src = source.replace("|", "\\|").replace("`", "'")
    op = opened.replace("|", "\\|")
    return f"| {id_} | {surface} | {typ} | {src} | {op} | {NOT_TESTED} | {EVIDENCE} |"


def main() -> None:
    lines: list[str] = []
    lines.append("# O-Draft WinUI screen inventory")
    lines.append("")
    lines.append("Scope: `apps/aad-winui` (WinUI 3 shell). Engine menu paths from `crates/engine` `CommandSpec::menu()` (via `crates/aad-host/src/chrome.rs` → host `TextMenu` JSON).")
    lines.append("")
    lines.append("**Testing status:** all rows default to `not_tested` (no pass/fail claimed). **Evidence:** — until linked from the QA campaign.")
    lines.append("")
    lines.append("| ID | Surface | Type | Source file | How opened | Testing status | Evidence |")
    lines.append("| --- | --- | --- | --- | --- | --- | --- |")

    winui_prefix = str(WINUI).replace("\\", "/")

    # --- Main shell ---
    main_rows = [
        ("main.window", "O-Draft main shell", "main", f"{winui_prefix}/MainWindow.xaml", "Application launch (`Program.cs` → `MainWindow`)"),
        ("main.splash", "Startup splash overlay", "overlay", f"{winui_prefix}/MainWindow.xaml (SplashOverlay)", "Shown at load until host/canvas ready"),
        ("main.menu", "Menu bar (engine-driven + shell)", "menu", f"{winui_prefix}/MainWindow.xaml (Bar); MainWindow.xaml.cs BuildMenus()", "Always visible when Window ▸ Menu bar checked; populated from host TextMenu"),
        ("main.arch-bar", "Architecture bar (Layer / Space / Floor + arch tools)", "toolbar", f"{winui_prefix}/MainWindow.xaml (ArchBarHost)", "Always visible under menu; tool buttons call StartCommand"),
        ("main.selector.layer", "Current layer combo", "toolbar", f"{winui_prefix}/MainWindow.xaml (LayerBox)", "Arch bar; changes current layer"),
        ("main.selector.space", "Drawing space combo", "toolbar", f"{winui_prefix}/MainWindow.xaml (SpaceBox)", "Arch bar; saved view/spaces from host"),
        ("main.selector.floor", "Current floor combo", "toolbar", f"{winui_prefix}/MainWindow.xaml (FloorBox)", "Arch bar; floor selection"),
        ("main.toolstrip", "Left tool strip (draw/modify/annotate)", "toolbar", f"{winui_prefix}/MainWindow.xaml (ToolStripHost); BuildTools()", "Window ▸ Tool bar; gear opens customize window"),
        ("main.canvas", "Drawing canvas host", "main", f"{winui_prefix}/MainWindow.xaml (CanvasHost)", "Primary document view (native child HWND)"),
        ("main.side-panel", "Side panel tab host (Properties / Layer / Blocks)", "panel", f"{winui_prefix}/MainWindow.xaml (SideTabs)", "Docked right; auto-hide rail when unpinned"),
        ("main.panel.properties", "Properties tab", "panel", f"{winui_prefix}/MainWindow.xaml (PropertiesTab, PropertiesPanel)", "Side tab; ShowSidePanel(Properties); Ctrl+1; ui.panel.properties"),
        ("main.panel.layers", "Layer side tab (merged palette)", "panel", f"{winui_prefix}/MainWindow.xaml (LayersTab, LayersSlot)", "Side tab; layer merge embeds LayerManagerWindow body"),
        ("main.panel.blocks", "Blocks tab", "panel", f"{winui_prefix}/MainWindow.xaml (BlocksTab, BlocksPanel)", "Side tab; ui.dialog.blocks / ShowSidePanel(Blocks)"),
        ("main.side-rail", "Collapsed side-panel rail", "overlay", f"{winui_prefix}/MainWindow.xaml (SideRail)", "When side panel collapsed; tap to expand"),
        ("main.splitter", "Side panel width grip", "toolbar", f"{winui_prefix}/MainWindow.xaml (LeftSplitter)", "Drag or arrow keys when focused"),
        ("main.status", "Status bar", "status", f"{winui_prefix}/MainWindow.xaml (StatusRow)", "Window ▸ Status bar"),
        ("main.status.drawings", "Open drawing tabs + New", "status", f"{winui_prefix}/MainWindow.xaml (DrawingTabs)", "Status bar left cluster"),
        ("main.status.nameplate", "Nameplate designer button", "status", f"{winui_prefix}/MainWindow.xaml (NameplateButton)", "Status bar; runs sheet.designer"),
        ("main.status.coords", "Cursor coordinates", "status", f"{winui_prefix}/MainWindow.xaml (Coords)", "Host chrome refresh"),
        ("main.status.modes", "Snap / Ortho / Polar / OTrack / Dyn / LW / Transparency / Quick props toggles", "status", f"{winui_prefix}/MainWindow.xaml (status toggles)", "Status bar right cluster"),
        ("main.status.units", "Drafting units combo", "status", f"{winui_prefix}/MainWindow.xaml (UnitsBox)", "Status bar; drawing.units metadata"),
        ("main.status.layers-btn", "Open layer manager", "status", f"{winui_prefix}/MainWindow.xaml (LayersButton)", "Status bar layers icon → OpenLayers()"),
        ("main.status.pdf", "PDF preview", "status", f"{winui_prefix}/MainWindow.xaml (PdfButton)", "Status bar → PDF window"),
        ("main.status.workspace", "Classic / Pro workspace", "status", f"{winui_prefix}/MainWindow.xaml (WorkspaceButton)", "Status bar; Ctrl+F11 toggle"),
        ("main.status.theme", "Light / dark theme", "status", f"{winui_prefix}/MainWindow.xaml (ThemeButton)", "Status bar"),
        ("main.status.ai-chip", "AI assistant chip", "status", f"{winui_prefix}/MainWindow.xaml (AiChip)", "Focus Ask bar (CommandChrome host window)"),
        ("main.status.shilpi-chip", "ShilpiDB connection chip + flyout", "menu", f"{winui_prefix}/MainWindow.xaml.cs OnShilpiChip", "Status bar when visible; MenuFlyout for vdb.* commands"),
        ("main.ask-bar", "Ask pill / AI prompt (hosted in separate HWND)", "overlay", f"{winui_prefix}/CommandChrome.cs; MainWindow.xaml (CommandBar, AskInputPill)", "Ctrl+9 toggle command line area; AI chip; assistant mode"),
        ("main.ask-history", "Command history list (Ask bar)", "overlay", f"{winui_prefix}/MainWindow.xaml (HistoryList)", "F2 toggles command history open state"),
        ("main.ask-assistant", "Allow / Decline assistant tools", "overlay", f"{winui_prefix}/MainWindow.xaml (AssistantButtons)", "When host reports pending assistant action"),
    ]
    for r in main_rows:
        lines.append(row(*r))

    # Architecture bar tools
    arch_tools = [
        ("wall.draw", "Wall"), ("wall.door", "Door"), ("wall.window", "Window"),
        ("room", "Room"), ("rarea", "Room area"), ("grid.axis", "Axis grid"),
        ("column.place", "Column"), ("stair.plan", "Stair"), ("north.arrow", "North"),
    ]
    for cid, label in arch_tools:
        lines.append(row(
            f"arch.tool.{cid}",
            f"Architecture bar: {label}",
            "toolbar",
            f"{winui_prefix}/MainWindow.xaml.cs (`ArchitectureTools`)",
            f'Arch bar button → StartCommand("{cid}")',
        ))

    # Tool strip catalog
    tools = [
        ("line", "Line"), ("pline", "Polyline"), ("circle", "Circle"), ("arc", "Arc"), ("rectang", "Rectangle"),
        ("move", "Move"), ("copy", "Copy"), ("rotate", "Rotate"), ("offset", "Offset"), ("trim", "Trim"),
        ("erase", "Erase"), ("dimlinear", "Dimension"), ("text", "Text"), ("hatch", "Hatch"),
    ]
    for cid, label in tools:
        lines.append(row(
            f"tool.{cid}",
            f"Tool strip: {label}",
            "toolbar",
            f"{winui_prefix}/MainWindow.xaml.cs (`ToolCatalog`/`BuildTools`)",
            f'Left strip when enabled in customize list → StartCommand("{cid}")',
        ))
    lines.append(row(
        "tool.customize-gear",
        "Tool strip: Customize tool bar",
        "toolbar",
        f"{winui_prefix}/MainWindow.xaml.cs (`BuildTools`)",
        "Gear tile → OpenTools()",
    ))

    # Shell Window menu (WinUI-only)
    shell_window = [
        ("shell.menu-bar", "Menu bar", "Window ▸ Menu bar toggle"),
        ("shell.tool-bar", "Tool bar", "Window ▸ Tool bar toggle"),
        ("shell.status-bar", "Status bar", "Window ▸ Status bar toggle"),
        ("shell.command-line", "Command line", "Window ▸ Command line toggle"),
        ("shell.pin-panels", "Pin side panels", "Window ▸ Pin side panels"),
        ("shell.props", "Properties", "Window ▸ Properties"),
        ("shell.layers-palette", "Layers palette", "Window ▸ Layers palette"),
        ("shell.blocks", "Blocks", "Window ▸ Blocks"),
        ("shell.parameters", "Parameters Manager", "Window ▸ Parameters Manager"),
        ("shell.workspace.classic", "Classic workspace", "Window ▸ Classic workspace"),
        ("shell.workspace.pro", "Pro workspace", "Window ▸ Pro workspace"),
        ("shell.workspace.reset", "Reset this workspace", "Window ▸ Reset this workspace"),
        ("shell.calc", "Calculator", "Window ▸ Calculator"),
        ("shell.pomodoro", "Pomodoro", "Window ▸ Pomodoro"),
        ("shell.customize-toolbar", "Customize tool bar", "Window ▸ Customize tool bar"),
        ("shell.nameplate", "Nameplate designer", "Window ▸ Nameplate designer"),
        ("shell.ucs", "UCS icon", "Window ▸ UCS icon"),
    ]
    for sid, label, how in shell_window:
        lines.append(row(
            f"win.menu.{sid}",
            f"Window menu: {label}",
            "menu",
            f"{winui_prefix}/MainWindow.xaml.cs (`EnsureShellMenus`)",
            how,
        ))

    lines.append(row(
        "help.commands",
        "Help: Command list",
        "menu",
        f"{winui_prefix}/MainWindow.xaml.cs (`EnsureHelp`)",
        "Help menu or F1 → CommandList.Show",
    ))
    lines.append(row(
        "help.about",
        "Help: About O-Draft",
        "menu",
        f"{winui_prefix}/AboutWindow.cs",
        "Help menu → AboutWindow.Show",
    ))

    # Top-level windows / managers
    windows = [
        ("win.layer-manager", "Layer Properties Manager", "manager", "LayerManagerWindow.xaml", "Menu `layer`/`layers`; status layers button; `OpenLayers()`; merge mode docks body in side tab"),
        ("win.drafting", "Drafting Settings", "dialog", "DraftingWindow.cs", "`dsettings`/`ds`/`se`; menu/osnap shortcut menu"),
        ("win.units", "Display units and plot", "dialog", "UnitsWindow.cs", "`units`/`un`; Format menu"),
        ("win.text-style", "Text Style", "dialog", "TextStyleWindow.cs", "`style`/`st`"),
        ("win.page-setup", "Page Setup Manager", "dialog", "PageSetupWindow.cs", "`pagesetup`"),
        ("win.plot-style", "Plot Style Manager", "dialog", "PlotStyleWindow.cs", "`stylesmanager`"),
        ("win.batch-plot", "Batch Plot", "dialog", "BatchPlotWindow.cs", "`plot.batch`"),
        ("win.dim-style", "Dimension Style", "dialog", "DimStyleWindow.cs", "`dimstyle`"),
        ("win.linetype", "Linetype Manager", "dialog", "LinetypeWindow.cs", "`linetype`/`lt`/`ltype`"),
        ("win.parameters", "Parameters Manager", "manager", "ParametersWindow.cs", "`ui.dialog.parameters`; Window menu"),
        ("win.hatch", "Hatch Editor", "dialog", "HatchWindow.cs", "`hatchedit`/`he`/`hatch.patterns`"),
        ("win.point-style", "Point Style", "dialog", "PointStyleWindow.cs", "`ddptype`"),
        ("win.mleader-style", "Multileader Style", "dialog", "MLeaderStyleWindow.cs", "`mleaderstyle`/`mls`"),
        ("win.room-settings", "Room Label Settings", "dialog", "RoomSettingsWindow.cs", "`room.settings`"),
        ("win.quick-select", "Quick Select", "dialog", "QuickSelectWindow.cs", "`qselect`/`qs`/`ui.dialog.qselect`"),
        ("win.quick-props", "Quick Properties", "dialog", "QuickWindow.cs", "Auto when `_qpmode` and selection quick JSON from host"),
        ("win.commands", "Commands (reference)", "dialog", "CommandList.cs", "F1; Help; `ui.dialog.commands`"),
        ("win.about", "About O-Draft", "dialog", "AboutWindow.cs", "`ui.dialog.about`; Help menu"),
        ("win.calc", "Calculator", "dialog", "CalcWindow.cs", "`cal`/`calc`; Window menu"),
        ("win.pomodoro", "Pomodoro", "dialog", "PomodoroWindow.cs", "Window menu"),
        ("win.tools", "Tool bar (customize)", "dialog", "ToolsWindow.cs", "`ui.dialog.customizetop`; tool strip gear"),
        ("win.panels-float", "Panels (floating side tabs)", "panel", "MainWindow.xaml.cs (`FloatLeft`)", "Side panel Float button"),
        ("win.pdf", "PDF preview", "dialog", "MainWindow.xaml.cs (`OnPdf`)", "Status bar PDF button"),
        ("win.save-prompt", "Save changes prompt", "dialog", "SavePrompt.cs", "Close drawing / quit with dirty document"),
        ("win.clear-history", "Clear command history", "dialog", "MainWindow.xaml.cs (`OnClearHistory`)", "Hidden UI wiring / clear history action"),
        ("win.canvas-menu", "Canvas context menu window", "menu", "CanvasMenu.cs", "Host popup JSON (`ReadPopup`); right-click / snap menu"),
        ("win.form-dynamic", "Engine form dialog (title from form JSON)", "dialog", "FormDialog.cs", "Any command with host form spec via `ReadForm`"),
        ("chrome.ask", "Ask bar top-level window", "overlay", "CommandChrome.cs (`_bar`)", "Hosts `CommandBar` XAML; owned by main window"),
        ("chrome.cad-input", "CAD command line at cursor", "overlay", "CommandChrome.cs (`_dyn`)", "Ctrl+9 / F6; dynamic input at canvas"),
    ]
    for wid, surface, typ, file, how in windows:
        lines.append(row(wid, surface, typ, f"{winui_prefix}/{file}", how))

    # Layer manager ContentDialogs
    layer_dialogs = [
        ("layer.dlg.delete", "Delete layers", "Layer manager Delete / context Delete"),
        ("layer.dlg.rename", "Rename layer", "Rename / F2 / context menu"),
        ("layer.dlg.color", "Layer color", "Color cell edit"),
        ("layer.dlg.transparency", "Transparency", "Transparency cell edit"),
    ]
    for lid, title, how in layer_dialogs:
        lines.append(row(
            lid,
            f"Layer manager ContentDialog: {title}",
            "dialog",
            f"{winui_prefix}/LayerManagerWindow.xaml.cs (CompactContentDialog, Ask)",
            how,
        ))
    lines.append(row(
        "layer.ctx-menu",
        "Layer row context MenuFlyout",
        "menu",
        f"{winui_prefix}/LayerManagerWindow.xaml.cs (OnRowMenu)",
        "Right-click layer row in manager",
    ))

    # File pickers (system)
    pickers = [
        ("picker.open", "File Open (drawing)", "MainWindow.xaml.cs `PickFile` (`open`)", "Menu File ▸ Open; Ctrl+O"),
        ("picker.save", "File Save", "MainWindow.xaml.cs `PickFile` (`qsave`)", "Menu File ▸ Save; Ctrl+S"),
        ("picker.save-as", "File Save As", "MainWindow.xaml.cs `PickFile` (`saveas`)", "Menu File ▸ Save As"),
        ("picker.plot-pdf", "Plot PDF save", "MainWindow.xaml.cs `FinishPlot`", "Plot/exportpdf flow → FileSavePicker `.pdf`"),
        ("picker.vdb-import", "ShilpiDB import .vdb", "MainWindow.xaml.cs `PickVdbCore`", "Menu / `vdb.import`"),
        ("picker.vdb-export", "ShilpiDB export .vdb", "MainWindow.xaml.cs `PickVdbCore`", "Menu / `vdb.export`"),
        ("picker.pdf-open", "Open PDF from preview", "MainWindow.xaml.cs (PDF window branch)", "PDF preview toolbar"),
    ]
    for pid, surface, src, how in pickers:
        lines.append(row(pid, surface, "dialog", f"{winui_prefix}/{src.split()[0]}", how))

    # Engine forms (FormDialog instances)
    forms = [
        ("project", "Project Details"), ("floors", "Floors and Heights"), ("beams", "Beam Depths"),
        ("schedule", "Opening Schedule Edit"), ("notes", "Notes Edit"), ("legend.edit", "Legend Entries"),
        ("symbol.desc", "Block Description"), ("takeoff.rates", "Take-off Rates"), ("room", "Room Label"),
        ("xref.attach", "Attach Xref"), ("xref.detach", "Detach Xref"), ("xref.bind", "Bind Xref"),
        ("floor.activate", "Activate Floor"), ("floor.declare", "Declare Floor Region"),
        ("floor.export", "Export Floor Drawing"), ("floor.xref", "Floor as Xref"),
        ("takeoff.restore", "Restore Take-off Tables"), ("hcwlayers", "Architectural Layer Set"),
        ("takeoff.linear", "Linear Take-off"), ("takeoff.brick", "Brickwork Take-off"),
        ("takeoff.beams", "Beams and Lintels Take-off"), ("takeoff.columns", "Columns Take-off"),
        ("takeoff.rects", "Rectangles Take-off"), ("takeoff.paint", "Wall Paint Take-off"),
        ("takeoff.ceiling", "Ceiling Take-off"), ("takeoff.floor", "Flooring Take-off"),
        ("takeoff.area", "Area Take-off"), ("takeoff.slab", "Slab Take-off"),
        ("wall.draw", "Draw Wall form"), ("wall.door", "Door form"), ("wall.window", "Window form"),
        ("grid.axis", "Axis grid form"), ("column.place", "Column form"), ("lintel.draw", "Lintel form"),
        ("stair.plan", "Stair form"), ("lift.shaft", "Lift shaft form"), ("rail.draw", "Rail form"),
        ("level.mark", "Level mark form"), ("section.marker", "Section marker form"),
        ("elevation.marker", "Elevation marker form"), ("north.arrow", "North arrow form"),
        ("slope.arrow", "Slope arrow form"), ("electrical.connect", "Electrical connect form"),
        ("electrical.number", "Electrical numbering form"), ("electrical.schedule", "Electrical Schedule form"),
        ("bridge.export", "Bridge export form"),
    ]
    for fid, title in forms:
        lines.append(row(
            f"form.{fid}",
            f"Form dialog: {title}",
            "dialog",
            f"{winui_prefix}/FormDialog.cs; crates/engine/src/forms.rs (id {fid})",
            f"Command {fid} (or alias) when engine returns form JSON",
        ))

    lines.append("")
    lines.append("## Engine menu bar entries")
    lines.append("")
    lines.append("Populated at runtime from host menu JSON (`BuildMenus()`). Top-level order: File, Edit, View, Insert, Format, Tools, Draw, Dimension, Modify, Architecture, AORMS, Window, Help (`crates/aad-host/src/chrome.rs`).")
    lines.append("")
    lines.append("| ID | Surface | Type | Source file | How opened | Testing status | Evidence |")
    lines.append("| --- | --- | --- | --- | --- | --- | --- |")

    if MENU_TSV.is_file():
        for line in MENU_TSV.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            cmd_id, path, src = line.split("\t", 2)
            lines.append(row(
                f"menu.{cmd_id}",
                path,
                "menu",
                f"crates/engine/src/cmd/{src} (+ host chrome)",
                f"Menu bar path; command id {cmd_id} via StartCommand / accelerator",
            ))
    else:
        lines.append(row("menu.*", "(menu TSV missing — re-run `_extract_menus.py`)", "menu", "crates/engine", "—",))

    lines.append("")
    lines.append("## egui-only surfaces (not in WinUI shell)")
    lines.append("")
    lines.append("Windows default desktop build uses WinUI (`apps/aad-winui`). The optional egui shell (`crates/ui-egui`, `aad --egui`) hosts additional or alternate UI:")
    lines.append("")
    lines.append("| ID | Surface | Type | Source file | How opened | Testing status | Evidence |")
    lines.append("| --- | --- | --- | --- | --- | --- | --- |")

    egui_only = [
        ("egui.mtext-editor", "Text Editor (MText)", "dialog", "crates/ui-egui/src/dialogs.rs", "MText edit flow in egui"),
        ("egui.import-aqc", "Import from AQC", "dialog", "crates/ui-egui/src/widgets.rs", "egui widget window"),
        ("egui.customize-topbar", "Customize Top Bar", "dialog", "crates/ui-egui/src/topbar.rs", "View menu in egui"),
        ("egui.space-editor", "Drawing space editor windows", "dialog", "crates/ui-egui/src/spaceeditor.rs", "Space commands in egui"),
        ("egui.context-apply-space", "Apply drawing space", "dialog", "crates/ui-egui/src/contextbar.rs", "Context bar in egui"),
        ("egui.plot-style-alt", "Plot style dialogs (egui layout)", "manager", "crates/ui-egui/src/plotstyles.rs", "Parallel to WinUI `PlotStyleWindow`"),
        ("egui.layer-window", "Layer manager (egui window)", "manager", "crates/ui-egui/src/layers.rs", "Parallel to WinUI `LayerManagerWindow`"),
        ("egui.form-dialog", "Form dialog (egui window)", "dialog", "crates/ui-egui/src/formdialog.rs", "Parallel to WinUI `FormDialog`"),
        ("egui.panels.toolbars", "Separate draw/modify left panels", "toolbar", "crates/ui-egui/src/toolbars.rs", "Classic egui docking"),
    ]
    for r in egui_only:
        lines.append(row(*r))

    lines.append("")
    lines.append("## Scan notes")
    lines.append("")
    lines.append(f"- WinUI project files scanned: `{winui_prefix}` (*.cs, *.xaml).")
    lines.append("- Floating panels: `LayerManagerWindow` (standalone or merged), `FloatLeft` → **Panels** window.")
    lines.append("- Managers: Layer Properties Manager, Parameters Manager, Linetype Manager, Page Setup Manager, Plot Style Manager.")
    lines.append(f"- Engine menu entries enumerated: {sum(1 for _ in MENU_TSV.read_text(encoding='utf-8').splitlines()) if MENU_TSV.is_file() else '?'}.")

    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Wrote {OUT} ({len(lines)} lines)")


if __name__ == "__main__":
    main()
