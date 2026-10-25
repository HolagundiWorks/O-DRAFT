using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>
/// Design tokens and helpers for code-built UI. XAML counterparts live in <c>Themes/*.xaml</c>.
/// Values match a compact desktop developer-app density (IDE-like), not a tablet layout.
/// </summary>
internal static class Density
{
    // --- Spacing (aligned with Hcw.Ui SuiteStyles: 4 / 8 / 12 / 16 / 20 / 24) ---
    public const double SpaceXs = 2;
    public const double SpaceSm = 4;
    public const double SpaceMd = 8;
    public const double SpaceLg = 12;
    public const double SpaceXl = 16;
    public const double SpaceSection = 20;
    public const double SpaceXxl = 24;

    public static Thickness DialogMargin { get; } = new Thickness(SpaceLg, SpaceMd, SpaceLg, SpaceMd);
    public static Thickness PanelPadding { get; } = new Thickness(SpaceLg, SpaceMd, SpaceLg, SpaceMd);
    public static Thickness PanelPaddingTight { get; } = new Thickness { Left = SpaceMd, Top = SpaceSm, Right = SpaceMd, Bottom = SpaceSm };
    public static Thickness CommandBarPadding { get; } = new Thickness { Left = SpaceLg, Top = SpaceSm, Right = SpaceLg, Bottom = SpaceSm };
    public static Thickness StatusPadding { get; } = new Thickness { Left = SpaceLg, Top = 4, Right = SpaceLg, Bottom = SpaceSm };
    public static Thickness LabelFieldGap { get; } = new Thickness { Left = 0, Top = 0, Right = 0, Bottom = SpaceXs };
    public static Thickness ControlPadding { get; } = new Thickness(SpaceLg, 4, SpaceLg, 4);
    public static Thickness PageMargin { get; } = new Thickness(12, 10, 12, 12);

    // --- Control sizing (suite canon: hit 36 · icons 0.6× · pane 260 · radius 2) ---
    public const double MinTarget = 36;
    public const double ControlHeight = 36;
    public const double ControlHeightCompact = 36;
    public const double IconButton = 24;
    public const double IconSize = 13;
    public const double IconSizeSm = 10;
    public const double SplitterWidth = 8;
    /// <summary>Default docked side-panel width (matches <c>SidePanelWidth</c> / SuitePaneWidth).</summary>
    public const double SidePanelWidth = 260;
    public const double CornerChrome = 2;
    public const double CornerControl = 2;

    // --- Typography (suite: body 13 · section 15 · title 22) ---
    public static double Font = FontBody;
    public static bool OverrideFace;
    public static string Face = "Segoe UI Variable";
    public const double FontCaption = 12;
    public const double FontBody = 13;
    public const double FontCommand = 13;
    public const double FontSection = 15;
    public const double FontHeading = 15;
    public const double FontPageTitle = 22;
    /// <summary>Preferred UI face name; use <see cref="FontUiResolved"/> after <see cref="ShellFonts.Resolve"/>.</summary>
    public const string FontUi = "Segoe UI";
    /// <summary>Preferred mono face name; use <see cref="FontMonoResolved"/> after resolve.</summary>
    public const string FontMono = "Consolas";
    public static string FontUiResolved { get; set; } = FontUi;
    public static string FontMonoResolved { get; set; } = FontMono;

    public static void Use(string preset)
    {
        OverrideFace = true;
        Font = FontBody;
        Face = preset switch
        {
            "editorial" => "Georgia",
            "drafting" => "Consolas",
            _ => "Segoe UI",
        };
    }

    public static void Apply(DependencyObject node)
    {
        if (node is Control control)
        {
            control.FontSize = Font;
            if (OverrideFace)
            {
                control.FontFamily = new FontFamily(Face);
            }
        }
        else if (node is TextBlock text)
        {
            text.FontSize = Font;
            if (OverrideFace && text.FontFamily?.Source != FontMonoResolved)
            {
                text.FontFamily = new FontFamily(Face);
            }
            if (OverrideFace && Face == FontMono)
            {
                text.FontFamily = new FontFamily(Face);
            }
        }
        var count = VisualTreeHelper.GetChildrenCount(node);
        for (var i = 0; i < count; i++)
        {
            Apply(VisualTreeHelper.GetChild(node, i));
        }
    }

    /// <summary>The theme the shell is showing. Brushes are looked up in this theme's dictionary.</summary>
    public static ElementTheme Theme { get; set; } = ElementTheme.Default;

    public static Brush? ThemeBrush(string key)
    {
        var name = Theme == ElementTheme.Light ? "Light" : "Default";
        if (FindThemed(Application.Current.Resources, name, key, 0) is { } brush)
        {
            return brush;
        }
        if (Theme != ElementTheme.Light
            && FindThemed(Application.Current.Resources, "Dark", key, 0) is { } darkBrush)
        {
            return darkBrush;
        }
        return Application.Current.Resources.TryGetValue(key, out var fallback) && fallback is Brush any ? any : null;
    }

    public static Windows.UI.Color? ThemeColor(string key)
    {
        var name = Theme == ElementTheme.Light ? "Light" : "Default";
        if (FindThemedObject(Application.Current.Resources, name, key, 0) is Windows.UI.Color color)
        {
            return color;
        }
        if (Theme != ElementTheme.Light
            && FindThemedObject(Application.Current.Resources, "Dark", key, 0) is Windows.UI.Color dark)
        {
            return dark;
        }
        return null;
    }

    /// <summary>App shell / page fill for the active theme.</summary>
    public static Brush AppBackground() =>
        ThemeBrush("AppBackgroundBrush")
        ?? new SolidColorBrush(Theme == ElementTheme.Light
            ? Windows.UI.Color.FromArgb(255, 0xF3, 0xF3, 0xF3)
            : Windows.UI.Color.FromArgb(255, 0x1B, 0x1B, 0x1B));

    public static Brush CanvasBackground() =>
        ThemeBrush("CanvasBackgroundBrush")
        ?? new SolidColorBrush(Theme == ElementTheme.Light
            ? Microsoft.UI.Colors.White
            : Microsoft.UI.Colors.Black);

    /// <summary>Accent colour from tokens (for selection zebra fills).</summary>
    public static Windows.UI.Color AccentColor()
    {
        if (ThemeBrush("AccentBrush") is SolidColorBrush brush)
        {
            return brush.Color;
        }
        return Theme == ElementTheme.Light
            ? Windows.UI.Color.FromArgb(255, 0x00, 0x78, 0xD4)
            : Windows.UI.Color.FromArgb(255, 0x60, 0xCD, 0xFF);
    }

    public static Brush SelectionFill(byte alpha = 64)
    {
        var c = AccentColor();
        return new SolidColorBrush(Windows.UI.Color.FromArgb(alpha, c.R, c.G, c.B));
    }

    public static Brush ZebraFill() =>
        new SolidColorBrush(Windows.UI.Color.FromArgb(16, 128, 128, 128));

    /// <summary>Secondary body text at full opacity (avoids Opacity &lt; 1 contrast failures).</summary>
    public static TextBlock Secondary(string text, TextWrapping wrap = TextWrapping.NoWrap)
    {
        var block = new TextBlock { Text = text, TextWrapping = wrap, FontSize = FontBody };
        block.Foreground = ThemeBrush("TextSecondaryBrush")
            ?? ThemeBrush("TextFillColorSecondaryBrush");
        return block;
    }

    public static TextBlock SectionHeading(string text)
    {
        var block = new TextBlock
        {
            Text = text,
            FontSize = FontSection,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Foreground = ThemeBrush("TextSecondaryBrush")
                ?? ThemeBrush("TextFillColorSecondaryBrush"),
        };
        Name(block, text);
        return block;
    }

    public static StackPanel DialogStack(params UIElement[] children)
    {
        var stack = new StackPanel { Spacing = SpaceMd, Margin = DialogMargin };
        foreach (var child in children)
        {
            stack.Children.Add(child);
        }
        return stack;
    }

    public static StackPanel FieldRow(string label, FrameworkElement field)
    {
        var row = new StackPanel { Spacing = SpaceXs };
        var text = new TextBlock { Text = label, FontSize = FontBody };
        Name(text, label);
        row.Children.Add(text);
        row.Children.Add(field);
        return row;
    }

    public static void Name(DependencyObject target, string name)
    {
        if (name.Length > 0)
        {
            AutomationProperties.SetName(target, name);
        }
    }

    private static Brush? FindThemed(ResourceDictionary dictionary, string theme, string key, int depth)
    {
        if (FindThemedObject(dictionary, theme, key, depth) is Brush brush)
        {
            return brush;
        }
        return null;
    }

    private static object? FindThemedObject(ResourceDictionary dictionary, string theme, string key, int depth)
    {
        if (depth > 8)
        {
            return null;
        }
        if (dictionary.ThemeDictionaries.TryGetValue(theme, out var themed)
            && themed is ResourceDictionary inner
            && inner.TryGetValue(key, out var value))
        {
            return value;
        }
        foreach (var merged in dictionary.MergedDictionaries)
        {
            if (FindThemedObject(merged, theme, key, depth + 1) is { } found)
            {
                return found;
            }
        }
        return null;
    }
}
