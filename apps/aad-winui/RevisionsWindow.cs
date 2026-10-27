using System.Globalization;
using System.Text.Json;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>Revision register ListView for the Revisions side panel and manager window.</summary>
internal static class RevisionsWindow
{
    private static readonly JsonSerializerOptions JsonOpts = new()
    {
        PropertyNameCaseInsensitive = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };

    private static readonly string[] StatusChoices = ["preliminary", "for approval", "for construction"];

    public static RevisionsPanelController Mount(Panel host, IntPtr engine, Action changed)
    {
        var controller = new RevisionsPanelController(engine, changed);
        host.Children.Clear();
        host.Children.Add(controller.Root);
        controller.Load();
        return controller;
    }

    public static Window Open(Window owner, IntPtr host, Action changed)
    {
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;
        var panel = new Grid { Margin = Density.DialogMargin, MinWidth = 640, MinHeight = 420 };
        var controller = Mount(panel, host, changed);
        var window = new Window { Title = "Revision register" };
        window.Content = panel;
        panel.RequestedTheme = theme;
        Density.Apply(panel);
        StandardWindow.Present(window, owner, WindowTemplate.Manager, "revisions");
        window.Closed += (_, _) => controller.Dispose();
        window.Activate();
        Native.ShowWindow(WinRT.Interop.WindowNative.GetWindowHandle(window), 8);
        return window;
    }

    internal sealed class RevisionsPanelController : IDisposable
    {
        private readonly IntPtr _host;
        private readonly Action _changed;
        private readonly ListView _list;
        private readonly TextBlock _status;
        private readonly TextBox _description;
        private readonly TextBox _by;
        private readonly TextBox _checked;
        private readonly TextBox _code;
        private readonly ComboBox _statusBox;
        private readonly Button _voidBtn;
        private readonly Button _cloudsBtn;
        private readonly Button _setBtn;
        private bool _disposed;

        public FrameworkElement Root { get; }

        public RevisionsPanelController(IntPtr host, Action changed)
        {
            _host = host;
            _changed = changed;

            _status = Density.Secondary("", TextWrapping.Wrap);
            AutomationProperties.SetLiveSetting(_status, AutomationLiveSetting.Polite);
            Density.Name(_status, "Revisions status");

            var header = new Grid { ColumnSpacing = Density.SpaceSm, Margin = new Thickness(18, 0, 0, 0) };
            foreach (var (title, width, col) in new (string, double, int)[]
            {
                ("Code", 48, 0), ("Date", 80, 1), ("Description", 140, 2), ("By", 48, 3), ("Checked", 64, 4), ("Status", 100, 5),
            })
            {
                header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(width) });
                var label = Density.SectionHeading(title);
                Density.Name(label, title);
                Grid.SetColumn(label, col);
                header.Children.Add(label);
            }

            _description = new TextBox { PlaceholderText = "What changed in this issue", MinWidth = 160 };
            Density.Name(_description, "Issue description");
            AutomationProperties.SetAutomationId(_description, "revision.description");
            _by = new TextBox { PlaceholderText = "Initials", MinWidth = 64, MaxWidth = 120 };
            Density.Name(_by, "Issued by");
            AutomationProperties.SetAutomationId(_by, "revision.by");
            _checked = new TextBox { PlaceholderText = "Initials", MinWidth = 64, MaxWidth = 120 };
            Density.Name(_checked, "Checked by");
            AutomationProperties.SetAutomationId(_checked, "revision.checked");
            _code = new TextBox { PlaceholderText = "Optional code", MinWidth = 72, MaxWidth = 120 };
            Density.Name(_code, "Revision code");
            AutomationProperties.SetAutomationId(_code, "revision.code");
            _statusBox = new ComboBox { MinWidth = 140, SelectedIndex = 2 };
            foreach (var s in StatusChoices)
            {
                _statusBox.Items.Add(s);
            }
            Density.Name(_statusBox, "Issue status");
            AutomationProperties.SetAutomationId(_statusBox, "revision.status");

            var refresh = ActionButton("Refresh", "revision.refresh");
            var issue = ActionButton("Issue", "revision.issue");
            _voidBtn = ActionButton("Void", "revision.void");
            _setBtn = ActionButton("Update", "revision.set");
            var draw = ActionButton("Draw table", "revision.table");
            _cloudsBtn = ActionButton("Propose clouds", "revision.clouds");
            _voidBtn.IsEnabled = false;
            _setBtn.IsEnabled = false;
            _cloudsBtn.IsEnabled = false;

            var toolbar = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.SpaceXs };
            toolbar.Children.Add(refresh);
            toolbar.Children.Add(issue);
            toolbar.Children.Add(_setBtn);
            toolbar.Children.Add(_voidBtn);
            toolbar.Children.Add(draw);
            toolbar.Children.Add(_cloudsBtn);

            var form = Ui.Form(
                ("Description", _description),
                ("By", _by),
                ("Checked", _checked),
                ("Code", _code),
                ("Status", _statusBox));

            _list = new ListView
            {
                SelectionMode = ListViewSelectionMode.Single,
                IsItemClickEnabled = true,
                MinHeight = 160,
                HorizontalContentAlignment = HorizontalAlignment.Stretch,
            };
            Density.Name(_list, "Revision register");
            AutomationProperties.SetAutomationId(_list, "revision.register");

            var root = new Grid { RowSpacing = Density.SpaceXs };
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

            var heading = DialogLayout.Heading("Revision register");
            var issueHeading = Density.SectionHeading("Issue a revision");
            AutomationProperties.SetHeadingLevel(issueHeading, AutomationHeadingLevel.Level3);
            var regHeading = Density.SectionHeading("Register");
            AutomationProperties.SetHeadingLevel(regHeading, AutomationHeadingLevel.Level3);

            Grid.SetRow(heading, 0);
            Grid.SetRow(toolbar, 1);
            Grid.SetRow(issueHeading, 2);
            Grid.SetRow(form, 3);
            Grid.SetRow(regHeading, 4);
            Grid.SetRow(header, 5);
            Grid.SetRow(_list, 6);
            Grid.SetRow(_status, 7);
            root.Children.Add(heading);
            root.Children.Add(toolbar);
            root.Children.Add(issueHeading);
            root.Children.Add(form);
            root.Children.Add(regHeading);
            root.Children.Add(header);
            root.Children.Add(_list);
            root.Children.Add(_status);
            Root = root;
            Density.Apply(root);

            refresh.Click += (_, _) => Load();
            issue.Click += (_, _) => Issue();
            _voidBtn.Click += (_, _) => VoidSelected();
            _setBtn.Click += (_, _) => UpdateSelected();
            draw.Click += (_, _) => DrawTable();
            _cloudsBtn.Click += (_, _) => ProposeClouds();
            _list.SelectionChanged += (_, _) => OnSelectionChanged();
        }

        public void Dispose() => _disposed = true;

        public void Load()
        {
            if (_disposed || _host == IntPtr.Zero)
            {
                return;
            }
            _list.Items.Clear();
            _voidBtn.IsEnabled = false;
            _setBtn.IsEnabled = false;
            _cloudsBtn.IsEnabled = false;
            if (ExecuteJson("revision.list", new { }) is not string json)
            {
                return;
            }
            try
            {
                using var doc = JsonDocument.Parse(json);
                if (!doc.RootElement.TryGetProperty("entries", out var list) || list.ValueKind != JsonValueKind.Array)
                {
                    _list.Items.Add(MakeEmptyItem("No revisions yet."));
                    return;
                }
                var i = 0;
                foreach (var e in list.EnumerateArray())
                {
                    var c = e.TryGetProperty("code", out var cn) ? cn.GetString() ?? "" : "";
                    var date = e.TryGetProperty("date", out var dn) ? dn.GetString() ?? "" : "";
                    var desc = e.TryGetProperty("description", out var dsn) ? dsn.GetString() ?? "" : "";
                    var byText = e.TryGetProperty("by", out var bn) ? bn.GetString() ?? "" : "";
                    var chk = e.TryGetProperty("checked", out var ch) ? ch.GetString() ?? "" : "";
                    var st = e.TryGetProperty("status", out var sn) ? sn.GetString() ?? "" : "";
                    uint? milestone = e.TryGetProperty("milestone", out var mn) && mn.TryGetUInt32(out var mid) ? mid : null;
                    _list.Items.Add(MakeEntryItem(c, date, desc, byText, chk, st, milestone, i));
                    i++;
                }
                if (i == 0)
                {
                    _list.Items.Add(MakeEmptyItem("No revisions yet."));
                }
            }
            catch (JsonException ex)
            {
                Fail(ex.Message);
            }
        }

        private static ListViewItem MakeEmptyItem(string message)
        {
            var item = new ListViewItem
            {
                Content = Density.Secondary(message),
                IsEnabled = false,
                IsHitTestVisible = false,
                MinHeight = Density.ControlHeight,
            };
            Density.Name(item, message);
            return item;
        }

        private static ListViewItem MakeEntryItem(
            string code, string date, string desc, string byText, string chk, string status, uint? milestone, int zebra)
        {
            var mark = new TextBlock
            {
                Text = " ",
                Width = 14,
                FontSize = Density.FontBody,
                VerticalAlignment = VerticalAlignment.Center,
                FontWeight = FontWeights.SemiBold,
            };
            Density.Name(mark, "Selection mark");

            var grid = new Grid { ColumnSpacing = Density.SpaceSm };
            foreach (var w in new double[] { 48, 80, 140, 48, 64, 100 })
            {
                grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(w) });
            }
            void Cell(int col, string text)
            {
                var t = Density.Secondary(text);
                t.TextTrimming = TextTrimming.CharacterEllipsis;
                Grid.SetColumn(t, col);
                grid.Children.Add(t);
            }
            Cell(0, code);
            Cell(1, date);
            Cell(2, desc);
            Cell(3, byText);
            Cell(4, chk);
            Cell(5, status);

            var row = new Grid { ColumnSpacing = Density.SpaceXs };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            Grid.SetColumn(mark, 0);
            Grid.SetColumn(grid, 1);
            row.Children.Add(mark);
            row.Children.Add(grid);

            var access = "Revision " + code + ", " + desc + ", status " + status
                + (string.IsNullOrEmpty(byText) ? "" : ", by " + byText)
                + (string.IsNullOrEmpty(chk) ? "" : ", checked by " + chk);
            var item = new ListViewItem
            {
                Content = row,
                Tag = new EntryRow(code, desc, byText, chk, status, milestone, mark),
                MinHeight = Math.Max(Density.ControlHeight, 24),
                Padding = Density.CellPadding,
                HorizontalContentAlignment = HorizontalAlignment.Stretch,
                Background = zebra % 2 == 0 ? Density.ZebraFill() : new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            };
            Density.Name(item, access);
            AutomationProperties.SetAutomationId(item, "revision.entry." + code);
            return item;
        }

        private void OnSelectionChanged()
        {
            foreach (var obj in _list.Items)
            {
                if (obj is ListViewItem { Tag: EntryRow row } item)
                {
                    var isOn = item.IsSelected;
                    row.Mark.Text = isOn ? "✓" : " ";
                    AutomationProperties.SetName(row.Mark, isOn ? "Selected" : "Not selected");
                }
            }

            if (_list.SelectedItem is not ListViewItem { Tag: EntryRow entry })
            {
                _voidBtn.IsEnabled = false;
                _setBtn.IsEnabled = false;
                _cloudsBtn.IsEnabled = false;
                return;
            }

            _voidBtn.IsEnabled = !string.IsNullOrEmpty(entry.Code) && entry.Status != "void";
            _setBtn.IsEnabled = !string.IsNullOrEmpty(entry.Code) && entry.Status != "void";
            _cloudsBtn.IsEnabled = entry.Milestone is uint m && m > 0;
            _description.Text = entry.Description;
            _by.Text = entry.By;
            _checked.Text = entry.Checked;
            _code.Text = entry.Code;
            var idx = Array.IndexOf(StatusChoices, entry.Status);
            _statusBox.SelectedIndex = idx >= 0 ? idx : 2;
        }

        private EntryRow? SelectedRow() =>
            _list.SelectedItem is ListViewItem { Tag: EntryRow row } ? row : null;

        private void Issue()
        {
            var desc = _description.Text?.Trim() ?? "";
            if (desc.Length == 0)
            {
                Fail("Enter a description.");
                return;
            }
            var status = _statusBox.SelectedItem as string ?? "for construction";
            var body = new Dictionary<string, object?>
            {
                ["description"] = desc,
                ["by"] = _by.Text?.Trim() ?? "",
                ["checked"] = _checked.Text?.Trim() ?? "",
                ["scope"] = "project",
                ["status"] = status,
            };
            var codeText = _code.Text?.Trim() ?? "";
            if (codeText.Length > 0)
            {
                body["code"] = codeText;
            }
            if (ExecuteJson("revision.issue", body) != null)
            {
                _status.Text = "Issued.";
                _description.Text = "";
                _code.Text = "";
                _changed();
                Load();
            }
        }

        private void UpdateSelected()
        {
            if (SelectedRow() is not { } row || string.IsNullOrEmpty(row.Code))
            {
                return;
            }
            var status = _statusBox.SelectedItem as string ?? "for construction";
            var body = new Dictionary<string, object?>
            {
                ["code"] = row.Code,
                ["description"] = _description.Text?.Trim() ?? "",
                ["by"] = _by.Text?.Trim() ?? "",
                ["checked"] = _checked.Text?.Trim() ?? "",
                ["status"] = status,
            };
            if (ExecuteJson("revision.set", body) != null)
            {
                _status.Text = "Updated " + row.Code + ".";
                _changed();
                Load();
            }
        }

        private void VoidSelected()
        {
            if (SelectedRow() is not { } row || string.IsNullOrEmpty(row.Code))
            {
                return;
            }
            if (ExecuteJson("revision.void", new { code = row.Code }) != null)
            {
                _status.Text = "Voided " + row.Code + ".";
                _changed();
                Load();
            }
        }

        private void DrawTable()
        {
            if (ExecuteJson("revision.table", new { all = true }) != null)
            {
                _status.Text = "Revision tables redrawn.";
                _changed();
            }
        }

        private void ProposeClouds()
        {
            if (SelectedRow()?.Milestone is not uint mid || mid == 0)
            {
                Fail("Select a revision that has a milestone.");
                return;
            }
            if (ExecuteJson("revision.clouds", new { from = mid }) is not string json)
            {
                return;
            }
            try
            {
                using var doc = JsonDocument.Parse(json);
                var n = doc.RootElement.TryGetProperty("proposals", out var p) && p.ValueKind == JsonValueKind.Array
                    ? p.GetArrayLength()
                    : 0;
                if (n == 0)
                {
                    _status.Text = "No change clusters for clouds.";
                    return;
                }
                if (ExecuteJson("revision.clouds", new { from = mid, accept = true }) != null)
                {
                    _status.Text = "Placed " + n.ToString(CultureInfo.InvariantCulture) + " revision cloud(s).";
                    _changed();
                }
            }
            catch (JsonException ex)
            {
                Fail(ex.Message);
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
    }

    private sealed class EntryRow(
        string code,
        string description,
        string by,
        string checkedBy,
        string status,
        uint? milestone,
        TextBlock mark)
    {
        public string Code { get; } = code;
        public string Description { get; } = description;
        public string By { get; } = by;
        public string Checked { get; } = checkedBy;
        public string Status { get; } = status;
        public uint? Milestone { get; } = milestone;
        public TextBlock Mark { get; } = mark;
    }
}
