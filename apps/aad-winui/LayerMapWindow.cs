using System.Text.Json;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Aad.WinUi;

public sealed partial class MainWindow
{
    private Window? _layerMapWindow;

    /// <summary>
    /// Architecture &gt; Levels &gt; Import Level: pick a DXF or DWG, then say which of this drawing's
    /// layers each of the file's layers goes on before the file becomes a level.
    /// </summary>
    private async Task OpenImportLevel()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_layerMapWindow != null)
        {
            PresentToolWindow(_layerMapWindow);
            return;
        }
        var picker = new FileOpenPicker();
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        picker.FileTypeFilter.Add(".dxf");
        picker.FileTypeFilter.Add(".dwg");
        var file = await picker.PickSingleFileAsync();
        if (file == null)
        {
            return;
        }
        History.Text = "Reading the layers of " + file.Name + "...";
        if (Native.Execute(_host, "layer.mapplan", JsonSerializer.Serialize(new { path = file.Path })) != 0)
        {
            History.Text = Native.LastError(_host);
            return;
        }
        var plan = Native.ReadResult(_host);
        _layerMapWindow = LayerMapWindow.Open(this, _host, file.Path, file.Name, plan, AfterCommand);
        _layerMapWindow.Closed += (_, _) => _layerMapWindow = null;
        PresentToolWindow(_layerMapWindow);
    }
}

/// <summary>
/// The layer-mapping step of an import. The engine's <c>layer.mapplan</c> lists the file's layers with
/// object counts and a suggested standard layer; the answers go back as the <c>layerMap</c> of
/// <c>level.import</c>. Layers left on "keep" come in under their own names.
/// </summary>
internal static class LayerMapWindow
{
    private const string Keep = "(keep the file's layer)";

    public static Window Open(Window owner, IntPtr host, string path, string fileName, string planJson, Action changed)
    {
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;
        var status = Density.Secondary("", TextWrapping.Wrap);
        AutomationProperties.SetLiveSetting(status, AutomationLiveSetting.Polite);
        Density.Name(status, "Layer mapping status");

        var targetNames = new List<string>();
        var targetLabels = new List<string>();
        var rows = new List<(string Layer, ComboBox Target)>();
        var list = new StackPanel { Spacing = Density.SpaceXs };
        try
        {
            using var doc = JsonDocument.Parse(planJson);
            if (doc.RootElement.TryGetProperty("targets", out var targets) && targets.ValueKind == JsonValueKind.Array)
            {
                foreach (var t in targets.EnumerateArray())
                {
                    var name = t.TryGetProperty("name", out var n) ? n.GetString() ?? "" : "";
                    var tab = t.TryGetProperty("tab", out var tb) ? tb.GetString() ?? "" : "";
                    if (name.Length > 0)
                    {
                        targetNames.Add(name);
                        targetLabels.Add(name + "  (" + tab + ")");
                    }
                }
            }
            if (doc.RootElement.TryGetProperty("layers", out var layers) && layers.ValueKind == JsonValueKind.Array)
            {
                foreach (var l in layers.EnumerateArray())
                {
                    var name = l.TryGetProperty("name", out var n) ? n.GetString() ?? "" : "";
                    var count = l.TryGetProperty("objects", out var c) && c.TryGetInt32(out var k) ? k : 0;
                    var suggested = l.TryGetProperty("suggested", out var s) && s.ValueKind == JsonValueKind.String ? s.GetString() : null;
                    if (name.Length == 0 || count == 0)
                    {
                        continue; // empty layers carry nothing to map
                    }
                    var combo = new ComboBox { MinWidth = 230, MinHeight = Density.ControlHeight };
                    combo.Items.Add(Keep);
                    foreach (var label in targetLabels)
                    {
                        combo.Items.Add(label);
                    }
                    var at = suggested == null ? -1 : targetNames.IndexOf(suggested);
                    combo.SelectedIndex = at >= 0 ? at + 1 : 0;
                    Density.Name(combo, "Layer for " + name);
                    var label2 = Density.Secondary(name + "  (" + count + ")", TextWrapping.NoWrap);
                    label2.VerticalAlignment = VerticalAlignment.Center;
                    var row = new Grid { ColumnSpacing = Density.SpaceSm };
                    row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                    row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
                    Grid.SetColumn(combo, 1);
                    row.Children.Add(label2);
                    row.Children.Add(combo);
                    list.Children.Add(row);
                    rows.Add((name, combo));
                }
            }
        }
        catch (JsonException)
        {
            status.Text = "The layer list could not be read.";
        }

        var stem = Path.GetFileNameWithoutExtension(fileName);
        var levelName = Density.DialogField(stem, "Level name");
        var import = DialogLayout.Action("Import", primary: true);
        var cancel = DialogLayout.Action("Cancel");
        var root = new StackPanel { Spacing = Density.SpaceMd, Margin = Density.DialogMargin, MinWidth = 440, MaxWidth = 560 };
        root.Children.Add(DialogLayout.Heading("Import " + fileName));
        root.Children.Add(Density.Secondary("Choose the layer each of the file's layers goes on. Matches are suggested from the layer names; change any of them. The number is how many objects the layer holds.", TextWrapping.Wrap));
        root.Children.Add(DialogLayout.Labeled("Level name", levelName));
        root.Children.Add(new ScrollViewer { Content = list, MaxHeight = 360 });
        root.Children.Add(status);
        root.Children.Add(DialogLayout.ActionRow(import, cancel));
        Density.Apply(root);

        var window = new Window { Title = "Import Level - Map Layers", Content = new Grid { RequestedTheme = theme, Children = { root } } };
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = true;
        }
        import.Click += (_, _) =>
        {
            var map = new Dictionary<string, string>();
            foreach (var (layer, combo) in rows)
            {
                var i = combo.SelectedIndex - 1;
                if (i >= 0 && i < targetNames.Count)
                {
                    map[layer] = targetNames[i];
                }
            }
            var name = string.IsNullOrWhiteSpace(levelName.Text) ? stem : levelName.Text.Trim();
            var json = JsonSerializer.Serialize(new { path, name, layerMap = map });
            if (Native.Execute(host, "level.import", json) != 0)
            {
                status.Text = Native.LastError(host);
                return;
            }
            changed();
            window.Close();
        };
        cancel.Click += (_, _) => window.Close();
        ShellIcon.Apply(window, theme);
        ShellFit.UserResizable(window, minWidthDip: 460, minHeightDip: 320, maxWidthDip: 700, maxHeightDip: 760, persistKey: "layermap");
        return window;
    }
}
