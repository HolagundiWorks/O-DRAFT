using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>
/// The rail on the right edge: one vertical title per side panel (Properties, Layers, Blocks,
/// Project, Graph, History, Revisions). It is always visible, open or collapsed. A title opens its
/// panel and only that panel; clicking the title of the open panel collapses the dock to the rail.
/// </summary>
public sealed partial class MainWindow
{
    private const double SideRailWidth = 28;

    private SideTab _sideTab = SideTab.Properties;
    private readonly Dictionary<SideTab, Button> _railButtons = new();
    private readonly Dictionary<SideTab, string> _sideTitles = new();

    private static readonly (SideTab Tab, string Title)[] RailTitles =
    [
        (SideTab.Properties, "Properties"),
        (SideTab.Layers, "Layers"),
        (SideTab.Blocks, "Blocks"),
        (SideTab.Project, "Project"),
        (SideTab.Graph, "Graph"),
        (SideTab.History, "History"),
        (SideTab.Revisions, "Revisions"),
    ];

    private void BuildSideRail()
    {
        SideRailTabs.Children.Clear();
        _railButtons.Clear();
        foreach (var (tab, title) in RailTitles)
        {
            var which = tab;
            var button = MakeRailTitle(title);
            button.Click += (_, _) => OnRailTitle(which);
            _railButtons[tab] = button;
            SideRailTabs.Children.Add(button);
        }
        RefreshSideRail();
    }

    /// <summary>A vertical title: the name rotated 90 degrees (reading downward) in a canvas of fixed size.</summary>
    private static Button MakeRailTitle(string title)
    {
        const double lineHeight = 15;
        var length = Math.Clamp(title.Length * 7 + 28, 72, 120);
        var text = new TextBlock
        {
            Text = title,
            Width = length,
            TextAlignment = TextAlignment.Center,
            FontSize = Density.FontCaption,
            TextTrimming = TextTrimming.CharacterEllipsis,
            RenderTransformOrigin = new Windows.Foundation.Point(0, 0),
            RenderTransform = new RotateTransform { Angle = 90 },
        };
        // 24 wide: the tab touches the open panel on its left and leaves 4 of the rail on its right.
        const double tabWidth = SideRailWidth - 4;
        Canvas.SetLeft(text, (tabWidth + lineHeight) / 2);
        Canvas.SetTop(text, 0);
        var button = SeamlessTab.Create(new Canvas { Width = tabWidth, Height = length, Children = { text } }, false, TabEdge.Right, new Thickness(0));
        button.Width = tabWidth;
        button.Height = length;
        button.MinWidth = tabWidth;
        button.MinHeight = length;
        return button;
    }

    private void OnRailTitle(SideTab tab)
    {
        if (_leftFloat != null)
        {
            ShowSidePanel(tab);
            return;
        }
        if (!_leftCollapsed && _sideTab == tab)
        {
            _leftCollapsed = true;
            ApplyColumns();
            PlaceChild();
            SaveShell();
            return;
        }
        ShowSidePanel(tab);
        SaveShell();
    }

    /// <summary>The selected title of the open panel is checked; a collapsed dock checks none.</summary>
    private void RefreshSideRail()
    {
        if (_railButtons.Count == 0)
        {
            return;
        }
        var open = _leftFloat != null || !_leftCollapsed;
        foreach (var (tab, button) in _railButtons)
        {
            var current = open && tab == _sideTab;
            SeamlessTab.Apply(button, current, TabEdge.Right);
            var title = _sideTitles.TryGetValue(tab, out var live) && live.Length > 0 ? live : RailTitles.First(r => r.Tab == tab).Title;
            ToolTipService.SetToolTip(button, title + (current ? " (click to collapse)" : " (click to open)"));
            AutomationProperties.SetName(button, (current ? "Collapse " : "Open ") + RailTitles.First(r => r.Tab == tab).Title);
        }
        if (_sideTitles.TryGetValue(_sideTab, out var heading) && heading.Length > 0)
        {
            SidePanelTitle.Text = heading;
        }
        else
        {
            SidePanelTitle.Text = RailTitles.First(r => r.Tab == _sideTab).Title;
        }
    }
}
