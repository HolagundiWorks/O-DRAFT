using System.Text.Json;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using WinRT.Interop;

namespace Aad.WinUi;

/// <summary>
/// Display units, plot scale and annotation. Drafting units are chosen on the status bar
/// (geometry stays in those units; rescale is not offered here — scale at plot).
/// </summary>
internal static class UnitsWindow
{
    private static readonly string[] DisplayUnits = ["mm", "cm", "m", "in", "ft", "ftin"];
    private static readonly string[] Papers = ["A0", "A1", "A2", "A3", "A4"];

    public static Window Open(Window owner, IntPtr host, Action changed)
    {
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;
        var status = Density.Secondary("", TextWrapping.Wrap);
        AutomationProperties.SetLiveSetting(status, AutomationLiveSetting.Polite);
        Density.Name(status, "Units status");
        var display = Combo(DisplayUnits, "mm");
        var paper = Combo(Papers, "A1");
        Density.Name(display, "Display units");
        Density.Name(paper, "Paper size");
        var scale = new TextBox { Text = "1:100", Width = 80, MinHeight = Density.ControlHeight };
        Density.Name(scale, "Plot scale");
        var stack = new StackPanel { Spacing = Density.SpaceSm, Margin = Density.DialogMargin, MaxWidth = 420 };
        stack.Children.Add(Heading("Drafting units"));
        stack.Children.Add(Note(
            "Pick the drafting unit on the status bar. Geometry is stored in those units. " +
            "Typed distances already accept multi-unit suffixes (mm, cm, m, ', \"); they convert into the drafting unit. " +
            "Use plot scale below when printing — not while drafting."));
        stack.Children.Add(Heading("Display units"));
        stack.Children.Add(Note("How dimensions show measurements. Geometry is not changed."));
        stack.Children.Add(display);
        stack.Children.Add(Heading("Plot scale and paper"));
        var plotRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.SpaceSm };
        plotRow.Children.Add(scale);
        plotRow.Children.Add(paper);
        var size = ActionButton("Size annotation");
        var check = ActionButton("Check the sheet");
        plotRow.Children.Add(size);
        plotRow.Children.Add(check);
        stack.Children.Add(plotRow);
        stack.Children.Add(Heading("Annotation conflicts"));
        var anno = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.SpaceSm };
        var find = ActionButton("Find");
        var resolve = ActionButton("Resolve overlaps");
        anno.Children.Add(find);
        anno.Children.Add(resolve);
        stack.Children.Add(anno);
        stack.Children.Add(status);

        void Run(string command, string json)
        {
            if (Native.Execute(host, command, json) != 0)
            {
                status.Text = Native.LastError(host);
                return;
            }
            status.Text = Message(Native.ReadResult(host));
            changed();
        }

        display.SelectionChanged += (_, _) =>
        {
            if (display.Tag is true)
            {
                return;
            }
            Run("display.units", Json(new { units = Selected(display) }));
        };
        size.Click += (_, _) => Run("plot.config", Json(new { scale = scale.Text, paper = Selected(paper) }));
        check.Click += (_, _) => Run("plot.validate", "{}");
        find.Click += (_, _) => Run("annotation.collisions", "{}");
        resolve.Click += (_, _) => Run("annotation.layout", Json(new { mode = "apply" }));

        display.Tag = true;
        Density.Apply(stack);
        var window = new Window
        {
            Title = "Display units and plot",
            Content = new ScrollViewer
            {
                Content = new Grid { RequestedTheme = theme, Children = { stack } },
            },
        };
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = false;
        }
        ShellIcon.Apply(window, theme);
        ShellFit.ToContent(window, minWidthDip: 400, minHeightDip: 320, maxWidthDip: 560, maxHeightDip: 560);
        display.Tag = false;
        return window;
    }

    private static Button ActionButton(string label) => Density.DialogAction(label);

    private static ComboBox Combo(IEnumerable<string> items, string selected)
    {
        var box = new ComboBox { MinWidth = 80, MinHeight = Density.ControlHeight };
        foreach (var item in items)
        {
            box.Items.Add(item);
        }
        box.SelectedItem = selected;
        return box;
    }

    private static string Selected(ComboBox box) => box.SelectedItem as string ?? "";

    private static string Json(object value) => JsonSerializer.Serialize(value);

    private static string Message(string json)
    {
        try
        {
            using var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;
            if (root.TryGetProperty("warning", out var warning) && warning.GetString() is string text && text.Length > 0)
            {
                return text;
            }
            if (root.TryGetProperty("warnings", out var warnings) && warnings.ValueKind == JsonValueKind.Array)
            {
                var lines = warnings.EnumerateArray().Select(item => item.GetString()).Where(item => !string.IsNullOrEmpty(item)).ToList();
                return lines.Count == 0 ? "The drawing fits the sheet at this scale." : string.Join("; ", lines);
            }
            if (root.TryGetProperty("count", out var count))
            {
                return count.GetUInt64() + " annotation conflict(s)";
            }
            if (root.TryGetProperty("moves", out var moves) || root.TryGetProperty("applied", out moves))
            {
                var left = root.TryGetProperty("unresolved", out var unresolved) && unresolved.ValueKind == JsonValueKind.Array ? unresolved.GetArrayLength() : 0;
                var moved = moves.ValueKind == JsonValueKind.Array ? moves.GetArrayLength() : moves.TryGetInt64(out var n) ? n : 0;
                return moved + " text object(s) moved; " + left + " could not be resolved";
            }
            return "Updated.";
        }
        catch (JsonException)
        {
            return "Updated.";
        }
    }

    private static TextBlock Heading(string text)
    {
        var block = Density.SectionHeading(text);
        AutomationProperties.SetHeadingLevel(block, AutomationHeadingLevel.Level2);
        return block;
    }

    private static TextBlock Note(string text) => Density.Secondary(text, TextWrapping.Wrap);
}
