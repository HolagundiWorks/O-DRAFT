using System.Globalization;
using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;

namespace Aad.WinUi;

public sealed partial class MainWindow
{
    private Window? _storySettingsWindow;

    internal void OpenStorySettings(long? selectLevelId = null)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_storySettingsWindow == null)
        {
            _storySettingsWindow = StorySettingsWindow.Open(this, _host, selectLevelId, AfterCommand);
            _storySettingsWindow.Closed += (_, _) => _storySettingsWindow = null;
        }
        PresentToolWindow(_storySettingsWindow);
    }
}

/// <summary>
/// Architecture &gt; Levels &gt; Level Settings: all storeys in one grid (elevation, floor-to-floor height),
/// with Insert Above / Below and Delete — same role as BIM "Story settings".
/// </summary>
internal static class StorySettingsWindow
{
    sealed class StoryRow
    {
        public long? Id;
        /// <summary>Level id or name used in <c>level.set</c> / <c>level.delete</c> before renames.</summary>
        public string LevelRef = "";
        public string Name = "";
        public double Elevation;
        public double Height;
        public bool MarkDeleted;
        public bool IsNew;
    }

    public static Window Open(Window owner, IntPtr host, long? selectLevelId, Action changed)
    {
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;
        var status = Density.Secondary("", TextWrapping.Wrap);

        var model = new List<StoryRow>();
        var implicitDrawing = false;
        var listHost = new ListView { SelectionMode = ListViewSelectionMode.Single, MinHeight = 280 };
        var insertAbove = DialogLayout.Action("Insert Above");
        var insertBelow = DialogLayout.Action("Insert Below");
        var deleteStory = DialogLayout.Action("Delete Story");
        var ok = DialogLayout.Action("OK", primary: true);
        var cancel = DialogLayout.Action("Cancel");

        bool LoadFromEngine()
        {
            model.Clear();
            implicitDrawing = false;
            if (Native.Execute(host, "level.list", "{}") != 0)
            {
                status.Text = Native.LastError(host);
                return false;
            }
            using var doc = JsonDocument.Parse(Native.ReadResult(host));
            var root = doc.RootElement;
            implicitDrawing = root.TryGetProperty("implicit", out var im) && im.ValueKind == JsonValueKind.True;
            if (!root.TryGetProperty("levels", out var levels) || levels.ValueKind != JsonValueKind.Array)
            {
                status.Text = "Could not read levels from the drawing.";
                return false;
            }
            foreach (var level in levels.EnumerateArray())
            {
                var id = level.TryGetProperty("id", out var idEl) && idEl.TryGetInt64(out var lid) ? lid : (long?)null;
                var name = level.TryGetProperty("name", out var n) ? n.GetString() ?? "" : "";
                var elev = level.TryGetProperty("elevation", out var e) && e.TryGetDouble(out var ev) ? ev : 0;
                var ht = level.TryGetProperty("height", out var h) && h.TryGetDouble(out var hv) ? hv : 0;
                var levelRef = implicitDrawing ? name : (id?.ToString(CultureInfo.InvariantCulture) ?? name);
                model.Add(new StoryRow
                {
                    Id = implicitDrawing ? null : id,
                    LevelRef = levelRef,
                    Name = name,
                    Elevation = elev,
                    Height = ht,
                });
            }
            return model.Count > 0;
        }

        if (!LoadFromEngine())
        {
            model.Add(new StoryRow { LevelRef = "Level 1", Name = "Level 1" });
            implicitDrawing = true;
        }

        void PaintList(int? selectDisplayIndex = null)
        {
            listHost.Items.Clear();
            var visible = VisibleByElevation(model);
            for (var i = 0; i < visible.Count; i++)
            {
                var row = visible[i];
                var storyNo = visible.Count - 1 - i;
                listHost.Items.Add(BuildRow(row, storyNo, () => PaintList(i)));
            }
            if (selectDisplayIndex is >= 0 and var si && si < listHost.Items.Count)
            {
                listHost.SelectedIndex = si;
            }
        }

        StoryRow? SelectedModelRow()
        {
            if (listHost.SelectedIndex < 0 || listHost.SelectedItem is not Grid g || g.Tag is not StoryRow row)
            {
                return null;
            }
            return row;
        }

        insertAbove.Click += (_, _) =>
        {
            var sel = SelectedModelRow();
            var visible = VisibleByElevation(model);
            var template = sel ?? visible.FirstOrDefault() ?? model[0];
            var row = new StoryRow
            {
                IsNew = true,
                LevelRef = "new:" + Guid.NewGuid().ToString("N"),
                Name = "Level " + (visible.Count + 1),
                Elevation = template.Elevation + Math.Max(0, template.Height),
                Height = template.Height > 0 ? template.Height : 3,
            };
            var modelIndex = model.IndexOf(template);
            model.Insert(modelIndex + 1, row);
            var displayIndex = visible.IndexOf(template);
            PaintList(Math.Max(0, displayIndex));
        };

        insertBelow.Click += (_, _) =>
        {
            var sel = SelectedModelRow();
            if (sel is null)
            {
                return;
            }
            var visible = VisibleByElevation(model);
            var displayIndex = visible.IndexOf(sel);
            var row = new StoryRow
            {
                IsNew = true,
                LevelRef = "new:" + Guid.NewGuid().ToString("N"),
                Name = "Level " + (visible.Count + 1),
                Elevation = sel.Elevation - (sel.Height > 0 ? sel.Height : 3),
                Height = sel.Height > 0 ? sel.Height : 3,
            };
            if (row.Elevation < -1000)
            {
                row.Elevation = sel.Elevation - 3;
            }
            model.Insert(model.IndexOf(sel), row);
            PaintList(Math.Max(0, displayIndex + 1));
        };

        deleteStory.Click += (_, _) =>
        {
            var sel = SelectedModelRow();
            if (sel is null)
            {
                return;
            }
            if (VisibleByElevation(model).Count <= 1)
            {
                status.Text = "The last level cannot be deleted.";
                return;
            }
            sel.MarkDeleted = true;
            PaintList(0);
        };

        var header = new Grid { ColumnSpacing = 8, Margin = new Thickness(0, 0, 0, 4) };
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(24) });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(40) });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(88) });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(96) });
        AddHeaderText(header, 1, "No.");
        AddHeaderText(header, 2, "Name");
        AddHeaderText(header, 3, "Elevation");
        AddHeaderText(header, 4, "Height to Next");

        var toolbar = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.Space2 };
        toolbar.Children.Add(insertAbove);
        toolbar.Children.Add(insertBelow);
        toolbar.Children.Add(deleteStory);

        var root = new StackPanel { Spacing = Density.SpaceMd, Margin = Density.DialogMargin };
        root.Children.Add(DialogLayout.Heading("Story settings"));
        root.Children.Add(Density.Secondary(
            "Storeys are listed top to bottom by elevation. Values use the drawing's unit (see Drawing Units).",
            TextWrapping.Wrap));
        root.Children.Add(toolbar);
        root.Children.Add(header);
        root.Children.Add(listHost);
        root.Children.Add(status);
        root.Children.Add(DialogLayout.ActionRow(ok, cancel));

        Density.Apply(root);
        var window = new Window
        {
            Title = "Story settings",
            Content = new ScrollViewer { Content = new Grid { RequestedTheme = theme, Children = { root } } },
        };
        StandardWindow.Present(window, owner, WindowTemplate.PropertyDialog, "storysettings", new WindowBounds(480, 420, 720, 900));

        PaintList();
        if (selectLevelId is not null)
        {
            var visible = VisibleByElevation(model);
            for (var i = 0; i < visible.Count; i++)
            {
                if (visible[i].Id == selectLevelId)
                {
                    listHost.SelectedIndex = i;
                    break;
                }
            }
        }

        cancel.Click += (_, _) => window.Close();
        ok.Click += (_, _) =>
        {
            if (!Commit(host, model, implicitDrawing, status))
            {
                return;
            }
            changed();
            window.Close();
        };

        return window;
    }

    static List<StoryRow> VisibleByElevation(List<StoryRow> rows) =>
        rows.Where(r => !r.MarkDeleted).OrderByDescending(r => r.Elevation).ToList();

    static Grid BuildRow(StoryRow row, int storyNo, Action refresh)
    {
        var grid = new Grid { ColumnSpacing = 8, MinHeight = 36, Tag = row };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(24) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(40) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(88) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(96) });

        grid.Children.Add(new Ellipse
        {
            Width = 8,
            Height = 8,
            Fill = Application.Current.Resources["TextFillColorPrimaryBrush"] as Brush,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        });

        var no = new TextBlock
        {
            Text = storyNo.ToString(CultureInfo.InvariantCulture),
            VerticalAlignment = VerticalAlignment.Center,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        };
        Grid.SetColumn(no, 1);
        grid.Children.Add(no);

        var name = new TextBox { Text = row.Name, VerticalAlignment = VerticalAlignment.Center, PlaceholderText = "Storey name" };
        name.LostFocus += (_, _) => row.Name = name.Text.Trim();
        Grid.SetColumn(name, 2);
        grid.Children.Add(name);

        var elev = new NumberBox
        {
            Value = row.Elevation,
            SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact,
            Minimum = -1e6,
            Maximum = 1e6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        elev.ValueChanged += (_, args) =>
        {
            if (double.IsNaN(args.NewValue))
            {
                return;
            }
            row.Elevation = args.NewValue;
            refresh();
        };
        Grid.SetColumn(elev, 3);
        grid.Children.Add(elev);

        var ht = new NumberBox
        {
            Value = row.Height,
            SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact,
            Minimum = 0,
            Maximum = 1e6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        ht.ValueChanged += (_, args) =>
        {
            if (double.IsNaN(args.NewValue))
            {
                return;
            }
            row.Height = Math.Max(0, args.NewValue);
        };
        Grid.SetColumn(ht, 4);
        grid.Children.Add(ht);

        return grid;
    }

    static void AddHeaderText(Grid grid, int column, string text)
    {
        var tb = Density.Secondary(text, TextWrapping.NoWrap);
        tb.VerticalAlignment = VerticalAlignment.Center;
        Grid.SetColumn(tb, column);
        grid.Children.Add(tb);
    }

    static bool Commit(IntPtr host, List<StoryRow> rows, bool implicitDrawing, TextBlock status)
    {
        var visible = rows.Where(r => !r.MarkDeleted).ToList();
        if (visible.Count == 0)
        {
            status.Text = "Keep at least one level.";
            return false;
        }

        if (implicitDrawing && visible.Count == 1 && !visible[0].IsNew)
        {
            status.Text = "This drawing has one implicit level. Use Insert Above or Below to add a storey, then OK.";
            return false;
        }

        foreach (var row in rows.Where(r => r.MarkDeleted && !r.IsNew))
        {
            var body = JsonSerializer.Serialize(new Dictionary<string, object> { ["level"] = row.LevelRef, ["force"] = true });
            if (Native.Execute(host, "level.delete", body) != 0)
            {
                status.Text = Native.LastError(host);
                return false;
            }
        }

        foreach (var row in visible.Where(r => r.IsNew))
        {
            var payload = new Dictionary<string, object>
            {
                ["name"] = row.Name,
                ["elevation"] = row.Elevation,
                ["height"] = row.Height,
            };
            if (implicitDrawing && visible.Count(r => !r.IsNew) == 0)
            {
                payload["first"] = row.Name;
            }
            if (Native.Execute(host, "level.new", JsonSerializer.Serialize(payload)) != 0)
            {
                status.Text = Native.LastError(host);
                return false;
            }
            row.IsNew = false;
            implicitDrawing = false;
        }

        foreach (var row in visible.Where(r => !r.IsNew))
        {
            var payload = new Dictionary<string, object>
            {
                ["level"] = row.LevelRef,
                ["name"] = row.Name,
                ["elevation"] = row.Elevation,
                ["height"] = row.Height,
            };
            if (Native.Execute(host, "level.set", JsonSerializer.Serialize(payload)) != 0)
            {
                status.Text = Native.LastError(host);
                return false;
            }
            row.LevelRef = row.Name;
        }

        return true;
    }
}
