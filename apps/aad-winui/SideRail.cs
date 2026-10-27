using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>
/// The rail on the right edge: one vertical title per side panel (Properties, Layers, Blocks,
/// Project). It is always visible, open or collapsed. A title opens its panel and only that panel;
/// clicking the title of the open panel collapses the dock to the rail.
/// </summary>
public sealed partial class MainWindow
{
    private const double SideRailWidth = 28;

    private SideTab _sideTab = SideTab.Properties;
    private readonly Dictionary<SideTab, ToggleButton> _railButtons = new();
    private readonly Dictionary<SideTab, string> _sideTitles = new();

    private static readonly (SideTab Tab, string Title)[] RailTitles =
    [
        (SideTab.Properties, "Properties"),
        (SideTab.Layers, "Layers"),
        (SideTab.Blocks, "Blocks"),
        (SideTab.Project, "Project"),
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
    private static ToggleButton MakeRailTitle(string title)
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
        Canvas.SetLeft(text, (SideRailWidth - 2 + lineHeight) / 2);
        Canvas.SetTop(text, 0);
        return new ToggleButton
        {
            Content = new Canvas { Width = SideRailWidth - 2, Height = length, Children = { text } },
            Width = SideRailWidth - 2,
            Height = length,
            MinWidth = SideRailWidth - 2,
            MinHeight = length,
            Padding = new Thickness(0),
        };
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
            if (button.IsChecked != current)
            {
                button.IsChecked = current;
            }
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
