using System.Globalization;
using System.Text.Json;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>
/// Version history: day-grouped ListView timeline (History side panel and optional manager window).
/// Keyboard: arrows select, Enter/Space activate; selection previews the milestone.
/// </summary>
internal static class HistoryWindow
{
    private static readonly JsonSerializerOptions JsonOpts = new()
    {
        PropertyNameCaseInsensitive = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };

    /// <summary>Build the History UI into <paramref name="host"/> (side panel or scroll viewer).</summary>
    public static HistoryPanelController Mount(Panel host, IntPtr engine, XamlRoot? xamlRoot, Action changed)
    {
        var controller = new HistoryPanelController(engine, xamlRoot, changed);
        host.Children.Clear();
        host.Children.Add(controller.Root);
        controller.Load();
        return controller;
    }

    public static Window Open(Window owner, IntPtr host, Action changed)
    {
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;
        var panel = new Grid { Margin = Density.DialogMargin, MinWidth = Density.HistoryPanelMinWidth, MinHeight = Density.HistoryPanelMinHeight };
        var controller = Mount(panel, host, owner.Content?.XamlRoot, changed);
        var window = new Window { Title = "Version history" };
        window.Content = panel;
        panel.RequestedTheme = theme;
        Density.Apply(panel);
        StandardWindow.Present(window, owner, WindowTemplate.Manager, "history");
        window.Closed += (_, _) => controller.Dispose();
        window.Activate();
        Native.ShowWindow(WinRT.Interop.WindowNative.GetWindowHandle(window), 8);
        return window;
    }

    internal sealed class HistoryPanelController : IDisposable
    {
        private readonly IntPtr _host;
        private readonly XamlRoot? _xamlRoot;
        private readonly Action _changed;
        private readonly ListView _list;
        private readonly TextBlock _status;
        private readonly TextBox _nameBox;
        private readonly CheckBox _namedOnly;
        private readonly Button _restore;
        private readonly Button _restoreCopy;
        private readonly Button _compare;
        private readonly Button _nameVersion;
        private readonly Button _delete;
        private readonly Button _endPreview;
        private bool _disposed;
        private bool _suppressPreview;

        public FrameworkElement Root { get; }

        public HistoryPanelController(IntPtr host, XamlRoot? xamlRoot, Action changed)
        {
            _host = host;
            _xamlRoot = xamlRoot;
            _changed = changed;

            _status = Density.Secondary("", TextWrapping.Wrap);
            AutomationProperties.SetLiveSetting(_status, AutomationLiveSetting.Polite);
            Density.Name(_status, "History status");

            _namedOnly = new CheckBox { Content = "Named versions only", IsChecked = false };
            Density.Name(_namedOnly, "Named versions only");
            AutomationProperties.SetAutomationId(_namedOnly, "history.namedOnly");

            _nameBox = new TextBox { PlaceholderText = "e.g. Issued for tender", MinWidth = Density.SearchFieldWidth };
            Density.Name(_nameBox, "Milestone name");
            AutomationProperties.SetAutomationId(_nameBox, "history.milestoneName");

            var refresh = ActionButton("Refresh", "history.refresh");
            var save = ActionButton("Save milestone", "history.save");
            _restore = ActionButton("Restore", "history.restore");
            _restoreCopy = ActionButton("Restore as copy", "history.restoreCopy");
            _compare = ActionButton("Compare with now", "history.compare");
            _nameVersion = ActionButton("Name this version", "history.rename");
            _endPreview = ActionButton("End preview", "history.endPreview");
            _delete = ActionButton("Delete", "history.delete");
            SetActionsEnabled(false);

            var toolbar = new StackPanel { Orientation = Orientation.Vertical, Spacing = Density.SpaceXs };
            var row1 = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.SpaceXs };
            row1.Children.Add(refresh);
            row1.Children.Add(save);
            row1.Children.Add(_endPreview);
            var row2 = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.SpaceXs };
            row2.Children.Add(_restore);
            row2.Children.Add(_restoreCopy);
            var row3 = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.SpaceXs };
            row3.Children.Add(_compare);
            row3.Children.Add(_nameVersion);
            row3.Children.Add(_delete);
            toolbar.Children.Add(row1);
            toolbar.Children.Add(row2);
            toolbar.Children.Add(row3);

            _list = new ListView
            {
                SelectionMode = ListViewSelectionMode.Single,
                IsItemClickEnabled = true,
                MinHeight = Density.HistoryListMinHeight,
                HorizontalContentAlignment = HorizontalAlignment.Stretch,
            };
            Density.Name(_list, "History timeline");
            AutomationProperties.SetAutomationId(_list, "history.timeline");

            var root = new Grid { RowSpacing = Density.SpaceXs };
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

            var heading = DialogLayout.Heading("Version history");
            var saveHeading = Density.SectionHeading("Save a named version");
            AutomationProperties.SetHeadingLevel(saveHeading, AutomationHeadingLevel.Level3);
            var saveRow = DialogLayout.Labeled("Milestone name", _nameBox);
            Grid.SetRow(heading, 0);
            Grid.SetRow(toolbar, 1);
            Grid.SetRow(_namedOnly, 2);
            Grid.SetRow(_list, 3);
            Grid.SetRow(saveHeading, 4);
            var saveBlock = new StackPanel { Spacing = Density.SpaceXs };
            saveBlock.Children.Add(saveRow);
            saveBlock.Children.Add(_status);
            Grid.SetRow(saveBlock, 5);
            root.Children.Add(heading);
            root.Children.Add(toolbar);
            root.Children.Add(_namedOnly);
            root.Children.Add(_list);
            root.Children.Add(saveHeading);
            root.Children.Add(saveBlock);
            Root = root;
            Density.Apply(root);

            _namedOnly.Checked += (_, _) => Load();
            _namedOnly.Unchecked += (_, _) => Load();
            refresh.Click += (_, _) => Load();
            save.Click += (_, _) => SaveMilestone();
            _restore.Click += (_, _) => Restore("replace");
            _restoreCopy.Click += (_, _) => Restore("copy");
            _compare.Click += (_, _) => Compare();
            _nameVersion.Click += (_, _) => _ = NameSelectedAsync();
            _endPreview.Click += (_, _) => EndPreview();
            _delete.Click += (_, _) => DeleteSelected();
            _list.SelectionChanged += (_, _) => OnSelectionChanged();
            _list.ItemClick += (_, e) =>
            {
                if (e.ClickedItem is ListViewItem { Tag: MilestoneRow row })
                {
                    Preview(row.Id);
                }
            };
        }

        public void Dispose() => _disposed = true;

        public void Load()
        {
            if (_disposed || _host == IntPtr.Zero)
            {
                return;
            }
            _suppressPreview = true;
            _list.Items.Clear();
            SetActionsEnabled(false);
            if (ExecuteJson("history.list", new { }) is not string json)
            {
                _suppressPreview = false;
                return;
            }
            try
            {
                using var doc = JsonDocument.Parse(json);
                if (!doc.RootElement.TryGetProperty("milestones", out var ms) || ms.ValueKind != JsonValueKind.Array)
                {
                    _list.Items.Add(MakeEmptyItem("No history yet — save the drawing first."));
                    _suppressPreview = false;
                    return;
                }
                var named = _namedOnly.IsChecked == true;
                var items = new List<(uint Id, string Kind, string Name, string Time)>();
                foreach (var m in ms.EnumerateArray())
                {
                    var kind = m.TryGetProperty("kind", out var k) ? k.GetString() ?? "" : "";
                    if (named && kind == "auto")
                    {
                        continue;
                    }
                    var id = m.TryGetProperty("id", out var idn) && idn.TryGetUInt32(out var n) ? n : 0u;
                    if (id == 0)
                    {
                        continue;
                    }
                    var name = m.TryGetProperty("name", out var nn) ? nn.GetString() ?? "" : "";
                    var time = m.TryGetProperty("time", out var tn) ? tn.GetString() ?? "" : "";
                    items.Add((id, kind, name, time));
                }
                if (items.Count == 0)
                {
                    _list.Items.Add(MakeEmptyItem(named ? "No named versions yet." : "No history yet — save the drawing first."));
                    _suppressPreview = false;
                    return;
                }

                string? lastDay = null;
                foreach (var item in items)
                {
                    var day = DayLabel(item.Time);
                    if (day != lastDay)
                    {
                        lastDay = day;
                        _list.Items.Add(MakeDayHeader(day));
                    }
                    _list.Items.Add(MakeVersionItem(item.Id, item.Kind, item.Name, item.Time));
                }
            }
            catch (JsonException ex)
            {
                Fail(ex.Message);
            }
            _suppressPreview = false;
        }

        private static ListViewItem MakeEmptyItem(string message)
        {
            var item = new ListViewItem
            {
                Content = Density.Secondary(message, TextWrapping.Wrap),
                IsEnabled = false,
                IsHitTestVisible = false,
                MinHeight = Density.ControlHeight,
            };
            Density.Name(item, message);
            return item;
        }

        private static ListViewItem MakeDayHeader(string day)
        {
            var text = Density.Secondary(day);
            text.FontWeight = FontWeights.SemiBold;
            AutomationProperties.SetHeadingLevel(text, AutomationHeadingLevel.Level3);
            var item = new ListViewItem
            {
                Content = text,
                IsEnabled = false,
                IsHitTestVisible = false,
                MinHeight = Density.ControlHeight,
                Padding = new Thickness(Density.SpaceXs, Density.SpaceSm, Density.SpaceXs, Density.SpaceXs),
            };
            Density.Name(item, day);
            AutomationProperties.SetHeadingLevel(item, AutomationHeadingLevel.Level3);
            return item;
        }

        private static ListViewItem MakeVersionItem(uint id, string kind, string name, string time)
        {
            var title = string.IsNullOrWhiteSpace(name)
                ? (kind == "auto" ? "Automatic restore point" : kind)
                : name;
            var when = TimeOfDay(time);
            var kindLabel = kind switch
            {
                "revision" => "Revision",
                "milestone" => "Named",
                "auto" => "Auto",
                _ => kind,
            };
            var mark = new TextBlock
            {
                Text = " ",
                Width = Density.SwatchSize,
                FontSize = Density.FontBody,
                VerticalAlignment = VerticalAlignment.Center,
                FontWeight = FontWeights.SemiBold,
            };
            Density.Name(mark, "Selection mark");
            var body = new StackPanel { Spacing = Density.SpaceXs };
            body.Children.Add(new TextBlock
            {
                Text = title,
                FontSize = Density.FontBody,
                TextTrimming = TextTrimming.CharacterEllipsis,
                FontWeight = kind == "auto" ? FontWeights.Normal : FontWeights.SemiBold,
            });
            body.Children.Add(Density.Secondary(
                kindLabel + " · " + when + " · #" + id.ToString(CultureInfo.InvariantCulture)));
            var row = new Grid { ColumnSpacing = Density.SpaceXs };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            Grid.SetColumn(mark, 0);
            Grid.SetColumn(body, 1);
            row.Children.Add(mark);
            row.Children.Add(body);

            var access = title + ", " + kindLabel + ", " + when + ", version " + id.ToString(CultureInfo.InvariantCulture);
            var item = new ListViewItem
            {
                Content = row,
                Tag = new MilestoneRow(id, kind, name, mark),
                MinHeight = Math.Max(Density.ControlHeight, 24),
                Padding = Density.CellPadding,
                HorizontalContentAlignment = HorizontalAlignment.Stretch,
            };
            Density.Name(item, access);
            AutomationProperties.SetAutomationId(item, "history.version." + id.ToString(CultureInfo.InvariantCulture));
            return item;
        }

        private void OnSelectionChanged()
        {
            foreach (var obj in _list.Items)
            {
                if (obj is ListViewItem { Tag: MilestoneRow row } item)
                {
                    var selected = item.IsSelected;
                    row.Mark.Text = selected ? "✓" : " ";
                    AutomationProperties.SetName(row.Mark, selected ? "Selected" : "Not selected");
                }
            }

            if (_list.SelectedItem is ListViewItem { Tag: MilestoneRow selectedRow })
            {
                SetActionsEnabled(true, selectedRow.Kind);
                if (!_suppressPreview)
                {
                    Preview(selectedRow.Id);
                }
            }
            else
            {
                SetActionsEnabled(false);
            }
        }

        private MilestoneRow? SelectedRow() =>
            _list.SelectedItem is ListViewItem { Tag: MilestoneRow row } ? row : null;

        private void SetActionsEnabled(bool on, string kind = "")
        {
            _restore.IsEnabled = on;
            _restoreCopy.IsEnabled = on;
            _compare.IsEnabled = on;
            _nameVersion.IsEnabled = on && kind != "revision";
            _delete.IsEnabled = on && kind != "revision";
        }

        private void SaveMilestone()
        {
            var name = _nameBox.Text?.Trim() ?? "";
            if (name.Length == 0)
            {
                Fail("Enter a milestone name.");
                return;
            }
            if (ExecuteJson("history.save", new { name, scope = "project" }) != null)
            {
                _status.Text = "Saved.";
                _nameBox.Text = "";
                _changed();
                Load();
            }
        }

        private void Restore(string mode)
        {
            if (SelectedRow() is not { } row)
            {
                return;
            }
            if (ExecuteJson("history.restore", new { id = row.Id, mode }) != null)
            {
                _status.Text = (mode == "copy" ? "Restored as copy #" : "Restored #")
                    + row.Id.ToString(CultureInfo.InvariantCulture) + ".";
                _changed();
                Load();
            }
        }

        private void Preview(uint id)
        {
            if (ExecuteJson("history.preview", new { id }) is string)
            {
                _status.Text = "Previewing #" + id.ToString(CultureInfo.InvariantCulture) + " (Esc to end).";
                _changed();
            }
        }

        private void Compare()
        {
            if (SelectedRow() is not { } row)
            {
                return;
            }
            if (ExecuteJson("history.compare", new { from = row.Id }) is string json)
            {
                try
                {
                    using var doc = JsonDocument.Parse(json);
                    var counts = doc.RootElement.TryGetProperty("counts", out var c) ? c.ToString() : json;
                    _status.Text = "Compare #" + row.Id.ToString(CultureInfo.InvariantCulture) + " → now: " + counts;
                }
                catch (JsonException)
                {
                    _status.Text = json;
                }
            }
        }

        private async Task NameSelectedAsync()
        {
            if (SelectedRow() is not { } row || row.Kind == "revision")
            {
                return;
            }
            var box = new TextBox { Text = row.Name, PlaceholderText = "e.g. Issued for tender", MinWidth = Density.SidePanelWidth };
            Density.Name(box, "Version name");
            var dialog = new ContentDialog
            {
                Title = "Name this version",
                Content = DialogLayout.Labeled("Version name", box),
                PrimaryButtonText = "Name",
                CloseButtonText = "Cancel",
                DefaultButton = ContentDialogButton.Primary,
                XamlRoot = _xamlRoot ?? Root.XamlRoot,
            };
            if (await dialog.ShowAsync() != ContentDialogResult.Primary)
            {
                return;
            }
            var name = box.Text?.Trim() ?? "";
            if (name.Length == 0)
            {
                Fail("Enter a name.");
                return;
            }
            if (ExecuteJson("history.rename", new { id = row.Id, name }) != null)
            {
                _status.Text = "Named.";
                _changed();
                Load();
            }
        }

        private void EndPreview()
        {
            if (ExecuteJson("history.preview", new { off = true }) != null)
            {
                _status.Text = "Preview ended.";
                _changed();
            }
        }

        private void DeleteSelected()
        {
            if (SelectedRow() is not { } row)
            {
                return;
            }
            if (row.Kind == "revision")
            {
                Fail("Issued revisions cannot be deleted; void them in the register.");
                return;
            }
            if (ExecuteJson("history.delete", new { id = row.Id }) != null)
            {
                _status.Text = "Deleted.";
                _changed();
                Load();
            }
        }

        private void Fail(string message) => _status.Text = message;

        private string? ExecuteJson(string command, object body)
        {
            if (Native.Execute(_host, command, JsonSerializer.Serialize(body, JsonOpts)) != 0)
            {
                Fail(Native.LastError(_host));
                return null;
            }
            _status.Text = "";
            return Native.ReadResult(_host);
        }

        private static Button ActionButton(string label, string automationId)
        {
            var button = DialogLayout.Action(label);
            AutomationProperties.SetAutomationId(button, automationId);
            return button;
        }

        private static string DayLabel(string iso)
        {
            if (iso.Length >= 10 && DateTime.TryParse(iso.AsSpan(0, 10), CultureInfo.InvariantCulture, DateTimeStyles.AssumeUniversal, out var day))
            {
                var local = day.ToLocalTime().Date;
                var today = DateTime.Now.Date;
                if (local == today)
                {
                    return "Today";
                }
                if (local == today.AddDays(-1))
                {
                    return "Yesterday";
                }
                return local.ToString("dddd, d MMMM yyyy", CultureInfo.CurrentCulture);
            }
            return string.IsNullOrEmpty(iso) ? "Unknown date" : iso;
        }

        private static string TimeOfDay(string iso)
        {
            if (DateTime.TryParse(iso, CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind, out var t))
            {
                return t.ToLocalTime().ToString("t", CultureInfo.CurrentCulture);
            }
            return iso;
        }
    }

    private sealed class MilestoneRow(uint id, string kind, string name, TextBlock mark)
    {
        public uint Id { get; } = id;
        public string Kind { get; } = kind;
        public string Name { get; } = name;
        public TextBlock Mark { get; } = mark;
    }
}
