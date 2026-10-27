using System.Text.Json;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>
/// Level tabs under the drawing and the sheet-set rail beside it. Both are thin views of the
/// chrome state: every click runs an engine command (<c>level.*</c>, <c>sheetset.*</c>), and the
/// forms for new level, level settings and delete level come from the engine
/// (<c>forms.rs</c>), like the other dialogs.
/// </summary>
public sealed partial class MainWindow
{
    private sealed class LevelTab
    {
        public long Id { get; set; }
        public string Name { get; set; } = "";
        public bool Active { get; set; }
        public bool Traced { get; set; }
        public double Elevation { get; set; }

        /// <summary>The settings of the trace that shows this level under or over the active one.</summary>
        public TraceSettings? Trace { get; set; }
    }

    private sealed class TraceSettings
    {
        public int Fade { get; set; } = 60;
        public string Colour { get; set; } = "grey";
        public string Order { get; set; } = "below";
        public bool Snap { get; set; }
        public List<string>? Layers { get; set; }

        public override string ToString() =>
            Fade + "% fade, " + Colour + ", " + Order + (Snap ? ", snaps" : "") + (Layers is { Count: > 0 } ? ", " + Layers.Count + " layer(s)" : "");
    }

    private sealed class SheetSetTab
    {
        public long Id { get; set; }
        public string Name { get; set; } = "";
        public bool Active { get; set; }
        public string LayerPrefix { get; set; } = "";
    }

    private const string TraceMark = "◐";

    private string _levelKey = "";
    private string _sheetKey = "";

    private void ShowLevelBars(ChromeState state)
    {
        ShowLevelTabs(state);
        ShowSheetRail(state);
    }

    private void ShowLevelTabs(ChromeState state)
    {
        var key = state.DesignMode + "|" + string.Join("|", state.Levels.Select(l => l.Id + ":" + l.Name + ":" + l.Active + ":" + l.Traced + ":" + l.Elevation + ":" + l.Trace))
            + "|" + string.Join("|", state.SheetSets.Where(s => s.Active).Select(s => s.Name));
        if (key == _levelKey)
        {
            return;
        }
        _levelKey = key;
        LevelTabs.Children.Clear();
        foreach (var level in state.Levels.ToList())
        {
            LevelTabs.Children.Add(MakeLevelTab(level, state.DesignMode));
        }
        if (state.Levels.Count == 0)
        {
            // The drawing has one implicit level: show it as the tab it is, so the bar is never empty.
            var only = SeamlessTab.Create(
                new TextBlock { Text = "Level 1", FontSize = Density.FontCaption, FontWeight = FontWeights.SemiBold, VerticalAlignment = VerticalAlignment.Center },
                true,
                TabEdge.Bottom,
                new Thickness(12, 0, 12, 0));
            only.MinHeight = 26;
            AutomationProperties.SetName(only, "Current level Level 1");
            ToolTipService.SetToolTip(only, "The drawing's only level. Use + to add a level for each floor.");
            LevelTabs.Children.Add(only);
        }
        var sheet = state.SheetSets.FirstOrDefault(s => s.Active);
        ToolTipService.SetToolTip(LevelHint, sheet == null ? null : "The level underneath is a read-only reference: only " + sheet.Name + " annotation is edited.");
        LevelHint.Text = state.Levels.Count == 0
            ? "One level. Add a level to draw each floor on its own."
            : sheet != null
                ? "Annotating " + sheet.Name
                : "";
    }

    private Button MakeLevelTab(LevelTab level, bool design)
    {
        var label = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.SpaceXs };
        if (level.Traced)
        {
            label.Children.Add(new TextBlock { Text = TraceMark, FontSize = Density.FontCaption, VerticalAlignment = VerticalAlignment.Center });
        }
        label.Children.Add(new TextBlock
        {
            Text = level.Name,
            MaxWidth = 140,
            FontSize = Density.FontCaption,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
            FontWeight = level.Active ? FontWeights.SemiBold : FontWeights.Normal,
        });
        var tab = SeamlessTab.Create(label, level.Active, TabEdge.Bottom, new Thickness(12, 0, 12, 0));
        tab.MinHeight = 26;
        AutomationProperties.SetName(tab, (level.Active ? "Current level " : "Switch to level ") + level.Name);
        var tip = level.Name + "  elevation " + level.Elevation.ToString("0.###", System.Globalization.CultureInfo.InvariantCulture);
        if (level.Traced)
        {
            tip += "  shown as a trace (" + (level.Trace?.ToString() ?? "default settings") + ")";
        }
        ToolTipService.SetToolTip(tab, tip + "  (right-click for more)");
        tab.Click += (_, _) =>
        {
            RunShellCommand("level.activate", new Dictionary<string, object> { ["level"] = level.Id });
        };
        // Opened by hand: the bar is at the window bottom, and a context flyout does not always honour its placement.
        tab.RightTapped += (_, e) =>
        {
            e.Handled = true;
            BuildLevelMenu(level, design).ShowAt(tab, new FlyoutShowOptions { Placement = FlyoutPlacementMode.TopEdgeAlignedLeft });
        };
        return tab;
    }

    private MenuFlyout BuildLevelMenu(LevelTab level, bool design)
    {
        var menu = new MenuFlyout { Placement = FlyoutPlacementMode.TopEdgeAlignedLeft };
        var settings = new MenuFlyoutItem { Text = "Rename, elevation and height..." };
        settings.Click += (_, _) => StartLevelForm("level.set", level);
        menu.Items.Add(settings);
        if (!level.Active)
        {
            var trace = new ToggleMenuFlyoutItem { Text = "Trace over the current level", IsChecked = level.Traced };
            trace.Click += (_, _) => RunShellCommand(
                "level.trace",
                new Dictionary<string, object> { [trace.IsChecked ? "add" : "remove"] = new[] { level.Id } });
            menu.Items.Add(trace);
            var settingsItem = new MenuFlyoutItem { Text = level.Traced ? "Trace settings..." : "Trace with settings..." };
            settingsItem.Click += (_, _) => StartTraceForm(level);
            menu.Items.Add(settingsItem);
        }
        menu.Items.Add(new MenuFlyoutSeparator());
        var delete = new MenuFlyoutItem { Text = "Delete level..." };
        delete.Click += (_, _) => StartLevelForm("level.delete", level);
        menu.Items.Add(delete);
        return menu;
    }

    private void ShowSheetRail(ChromeState state)
    {
        var key = state.DesignMode + "|" + string.Join("|", state.SheetSets.Select(s => s.Id + ":" + s.Name + ":" + s.Active + ":" + s.LayerPrefix));
        if (key == _sheetKey)
        {
            return;
        }
        _sheetKey = key;
        SheetRail.Children.Clear();
        SheetRailHost.Visibility = Visibility.Visible;
        SheetRail.Children.Add(MakeRailEntry(
            "Design",
            "Design: draw the building. Sheet-set annotation is not shown.",
            state.DesignMode,
            () => RunShellCommand("sheetset.activate", new Dictionary<string, object>())));
        foreach (var set in state.SheetSets.ToList())
        {
            var item = set;
            var tip = item.Name + ": annotation for each level" + (item.LayerPrefix.Length > 0 ? ", on layers starting " + item.LayerPrefix : "");
            SheetRail.Children.Add(MakeRailEntry(item.Name, tip, item.Active, () => RunShellCommand("sheetset.activate", new Dictionary<string, object> { ["set"] = item.Id })));
        }
        // Without tabs the rail would be empty: this makes the standard discipline tabs (and layers, symbols).
        if (state.SheetSets.Count == 0)
        {
            SheetRail.Children.Add(MakeRailEntry(
                "+ Drawing tabs",
                "Add the standard tabs: Working drawing, Electrical, Plumbing, Interior and False ceiling, each with its layers and symbols.",
                false,
                () => RunShellCommand("sheetset.standard", new Dictionary<string, object>())));
        }
    }

    /// <summary>
    /// A vertical tab: the name is rotated 90 degrees (reading upward) inside a canvas of fixed size,
    /// because a render transform does not change layout and the canvas does not clip.
    /// </summary>
    private static Button MakeRailEntry(string name, string tip, bool active, Action activate)
    {
        const double width = 26;
        const double lineHeight = 15;
        var length = Math.Clamp(name.Length * 6.5 + 28, 72, 168);
        var text = new TextBlock
        {
            Text = name,
            Width = length,
            TextAlignment = TextAlignment.Center,
            FontSize = Density.FontCaption,
            TextTrimming = TextTrimming.CharacterEllipsis,
            FontWeight = active ? FontWeights.SemiBold : FontWeights.Normal,
            RenderTransformOrigin = new Windows.Foundation.Point(0, 0),
            RenderTransform = new RotateTransform { Angle = -90 },
        };
        Canvas.SetLeft(text, (width - lineHeight) / 2);
        Canvas.SetTop(text, length);
        var entry = SeamlessTab.Create(new Canvas { Width = width, Height = length, Children = { text } }, active, TabEdge.Left, new Thickness(0));
        entry.Width = width;
        entry.Height = length;
        entry.MinWidth = width;
        entry.MinHeight = length;
        AutomationProperties.SetName(entry, (active ? "Current: " : "Switch to ") + name);
        ToolTipService.SetToolTip(entry, tip);
        entry.Click += (_, _) => activate();
        return entry;
    }

    private void OnAddLevel(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        var form = Native.ReadForm(_host, "level.new");
        if (form == null)
        {
            return;
        }
        var count = 0;
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadText(_host, Native.TextChrome));
            count = doc.RootElement.TryGetProperty("levels", out var levels) && levels.ValueKind == JsonValueKind.Array ? levels.GetArrayLength() : 0;
        }
        catch (JsonException)
        {
        }
        form = InjectFormField(form, "name", "Level " + Math.Max(2, count + 1));
        FormDialog.Show(this, _host, form, AfterCommand);
    }

    /// <summary>Open the engine form <paramref name="id"/> for one level.</summary>
    private void StartLevelForm(string id, LevelTab level)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        var form = Native.ReadForm(_host, id);
        if (form == null)
        {
            return;
        }
        form = InjectFormField(form, "level", level.Name);
        if (id == "level.set")
        {
            form = InjectFormField(form, "elevation", level.Elevation.ToString("0.###", System.Globalization.CultureInfo.InvariantCulture));
        }
        FormDialog.Show(this, _host, form, AfterCommand);
    }

    /// <summary>Open the Trace Settings form for <paramref name="level"/>, filled with the trace's current settings.</summary>
    private void StartTraceForm(LevelTab level)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        var form = Native.ReadForm(_host, "level.trace");
        if (form == null)
        {
            return;
        }
        form = InjectFormField(form, "add", level.Name);
        if (level.Trace is { } trace)
        {
            form = InjectFormField(form, "fade", trace.Fade.ToString(System.Globalization.CultureInfo.InvariantCulture));
            form = InjectFormField(form, "colour", trace.Colour);
            form = InjectFormField(form, "order", trace.Order);
            form = InjectFormField(form, "snap", trace.Snap ? "true" : "false");
            form = InjectFormField(form, "layers", trace.Layers is { Count: > 0 } layers ? string.Join(", ", layers) : "*");
        }
        FormDialog.Show(this, _host, form, AfterCommand);
    }

    /// <summary>Run one engine command with JSON parameters; show its error in the history.</summary>
    private void RunShellCommand(string id, Dictionary<string, object> parameters)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (Native.Execute(_host, id, JsonSerializer.Serialize(parameters)) != 0)
        {
            History.Text = Native.LastError(_host);
        }
        AfterCommand();
    }
}
