using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.Foundation;
using WinRT.Interop;

namespace Aad.WinUi;

/// <summary>
/// Fixed AI bar and cursor-anchored CAD prompt. Both are top-level windows so the drawing
/// child does not cover them. Geometry and command text stay in the host.
/// </summary>
internal sealed class CommandChrome
{
    private readonly Window _bar = new() { Title = "Ask" };
    private readonly Window _dyn = new() { Title = "CAD input" };
    private readonly Border _cadCard;
    private readonly Border _dynRoot;
    private bool _barReady;
    private bool _dynReady;

    /// <summary>Cursor field — 1.5× the previous half-size (70×18 → 105×27 DIP).</summary>
    public const double DynWidthDip = 105;
    public const double DynHeightDip = 27;
    /// <summary>UI face size for typed CAD text — matches shell body / theme typography.</summary>
    public static double DynFontSize => Density.FontBody;

    public TextBox CadInput { get; }
    public ListView SuggestList { get; }
    public TextBlock CadPrompt { get; }

    public CommandChrome()
    {
        Configure(_bar);
        Configure(_dyn);
        ShellWindows.Track(_bar);
        ShellWindows.Track(_dyn);
        // Cursor CAD: typed text only — no chrome box, just ink on the drawing.
        CadPrompt = new TextBlock
        {
            Visibility = Visibility.Collapsed,
            Text = "",
            IsHitTestVisible = false,
        };
        AutomationProperties.SetName(CadPrompt, "CAD prompt");
        CadInput = new TextBox
        {
            FontFamily = ShellFonts.UiFamily,
            FontSize = DynFontSize,
            MinHeight = DynHeightDip,
            MinWidth = DynWidthDip,
            BorderThickness = new Thickness(0),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(0),
            VerticalAlignment = VerticalAlignment.Center,
            PlaceholderText = "",
        };
        var clear = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        CadInput.Resources["TextControlBorderThemeThickness"] = new Thickness(0);
        CadInput.Resources["TextControlBorderThemeThicknessFocused"] = new Thickness(0);
        CadInput.Resources["TextControlBackground"] = clear;
        CadInput.Resources["TextControlBackgroundFocused"] = clear;
        CadInput.Resources["TextControlBackgroundPointerOver"] = clear;
        CadInput.Resources["TextControlBackgroundDisabled"] = clear;
        AutomationProperties.SetName(CadInput, "CAD command line");
        SuggestList = new ListView
        {
            MaxHeight = 54,
            Visibility = Visibility.Collapsed,
            Background = clear,
            BorderThickness = new Thickness(0),
            FontFamily = ShellFonts.UiFamily,
            FontSize = DynFontSize,
            SelectionMode = ListViewSelectionMode.Single,
            IsItemClickEnabled = true,
        };
        AutomationProperties.SetName(SuggestList, "Command suggestions");
        SuggestList.ItemContainerStyle = SuggestItemStyle(clear);
        var cadColumn = new StackPanel
        {
            Spacing = 1,
            Children = { CadInput, SuggestList },
        };
        _cadCard = new Border
        {
            Padding = new Thickness(0),
            CornerRadius = new CornerRadius(0),
            BorderThickness = new Thickness(0),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Child = cadColumn,
        };
        _dynRoot = new Border
        {
            Padding = new Thickness(0),
            CornerRadius = new CornerRadius(0),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Child = _cadCard,
        };
        _dyn.Content = _dynRoot;
        var barHwnd = WindowNative.GetWindowHandle(_bar);
        var dynHwnd = WindowNative.GetWindowHandle(_dyn);
        Native.MatchButtonCorners(barHwnd);
        Native.MakeWindowClear(barHwnd);
        Native.MatchButtonCorners(dynHwnd);
        Native.MakeWindowClear(dynHwnd);
        _bar.SystemBackdrop = new ClearBackdrop();
        _dyn.SystemBackdrop = new ClearBackdrop();
        ApplyTheme(SuiteAppearancePrefs.Load());
        ClearChromeTitleBar(_bar);
        ClearChromeTitleBar(_dyn);
    }

    public Windows.Graphics.PointInt32 BarPosition => _bar.AppWindow.Position;

    public void Attach(FrameworkElement body, IntPtr owner)
    {
        body.Visibility = Visibility.Visible;
        if (body is Border border)
        {
            // Composer layout owns its own cards/pills; keep the HWND host clear.
            border.BorderThickness = new Thickness(0);
            border.CornerRadius = new CornerRadius(0);
            border.Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
            border.BorderBrush = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        }
        _bar.Content = body;
        Own(_bar, owner, ref _barReady);
        Own(_dyn, owner, ref _dynReady);
        AllowActivate(_dyn);
    }

    public void WireCad(
        KeyEventHandler previewKeyDown,
        TextChangedEventHandler textChanged,
        TypedEventHandler<UIElement, CharacterReceivedRoutedEventArgs> characterReceived,
        ItemClickEventHandler suggestClick)
    {
        CadInput.PreviewKeyDown += previewKeyDown;
        CadInput.TextChanged += textChanged;
        CadInput.CharacterReceived += characterReceived;
        SuggestList.ItemClick += suggestClick;
    }

    public void FocusCad() => CadInput.Focus(FocusState.Programmatic);

    public void ApplyTheme(ElementTheme theme)
    {
        var resolved = theme == ElementTheme.Default
            ? (Density.Theme == ElementTheme.Default ? ElementTheme.Dark : Density.Theme)
            : theme;
        var dark = resolved != ElementTheme.Light;
        Density.Theme = resolved;
        // Ask + dyn: clear SystemBackdrop — null paints an opaque theme plate (second “box”) under the pill.
        ShellChrome.Apply(_bar, resolved);
        if (_bar.SystemBackdrop is not ClearBackdrop)
        {
            _bar.SystemBackdrop = new ClearBackdrop();
        }
        if (_dyn.SystemBackdrop is not ClearBackdrop)
        {
            _dyn.SystemBackdrop = new ClearBackdrop();
        }
        // Dyn CAD field: typed ink only — never ShellChrome.Paint (that fills AppBackground over the drawing).
        _dynRoot.RequestedTheme = resolved;
        CadInput.RequestedTheme = resolved;
        SuggestList.RequestedTheme = resolved;
        if (_bar.Content is FrameworkElement body)
        {
            body.RequestedTheme = resolved;
        }
        if (_bar.Content is Border barCard)
        {
            // HWND stays square/clear; AskInputPill alone draws the rounded pill.
            barCard.Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
            barCard.BorderThickness = new Thickness(0);
            barCard.CornerRadius = new CornerRadius(0);
            barCard.BorderBrush = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        }
        var barHwnd = WindowNative.GetWindowHandle(_bar);
        var dynHwnd = WindowNative.GetWindowHandle(_dyn);
        Native.Frame(barHwnd, Native.DwmColorNone, dark);
        Native.MatchButtonCorners(barHwnd);
        Native.MakeWindowClear(barHwnd);
        Native.Frame(dynHwnd, Native.DwmColorNone, dark);
        Native.MatchButtonCorners(dynHwnd);
        Native.MakeWindowClear(dynHwnd);
        // Match shell + canvas: light ink on dark canvas, dark ink on light canvas.
        var clear = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        _dynRoot.Background = clear;
        _cadCard.Background = clear;
        _cadCard.BorderThickness = new Thickness(0);
        _cadCard.BorderBrush = clear;
        _dynRoot.CornerRadius = new CornerRadius(0);
        _cadCard.CornerRadius = new CornerRadius(0);
        // Canvas-contrasted ink (same as host overlay crosshair), not chrome TextPrimary alone.
        var ink = new SolidColorBrush(dark ? Color(255, 0xE8, 0xE8, 0xE8) : Color(255, 0x1A, 0x23, 0x32));
        var muted = new SolidColorBrush(dark ? Color(255, 0xC8, 0xC8, 0xC8) : Color(255, 0x64, 0x74, 0x8B));
        var accent = Density.ThemeBrush("CadChannelBrush")
            ?? Density.ThemeBrush("AccentBrush")
            ?? new SolidColorBrush(dark ? Color(255, 0x6C, 0xCB, 0x8A) : Color(255, 0x0F, 0x7B, 0x3A));
        // TextBox template ignores Control.Foreground — drive the theme resource keys instead.
        SetBrush(CadInput, "TextControlForeground", ink);
        SetBrush(CadInput, "TextControlForegroundFocused", ink);
        SetBrush(CadInput, "TextControlForegroundPointerOver", ink);
        SetBrush(CadInput, "TextControlForegroundDisabled", muted);
        SetBrush(CadInput, "TextControlPlaceholderForeground", muted);
        SetBrush(CadInput, "TextControlPlaceholderForegroundFocused", muted);
        SetBrush(CadInput, "TextControlPlaceholderForegroundPointerOver", muted);
        SetBrush(CadInput, "TextControlBackground", clear);
        SetBrush(CadInput, "TextControlBackgroundFocused", clear);
        SetBrush(CadInput, "TextControlBackgroundPointerOver", clear);
        SetBrush(CadInput, "TextControlBackgroundDisabled", clear);
        SetBrush(CadInput, "TextControlButtonForeground", ink);
        // Kill Fluent fill plates that still show as an opaque block over the canvas.
        SetBrush(CadInput, "ControlFillColorDefaultBrush", clear);
        SetBrush(CadInput, "ControlFillColorSecondaryBrush", clear);
        SetBrush(CadInput, "ControlFillColorTertiaryBrush", clear);
        SetBrush(CadInput, "ControlFillColorTransparentBrush", clear);
        SetBrush(CadInput, "TextControlElevationBorderFocusedBrush", clear);
        SetBrush(CadInput, "TextControlElevationBorderBrush", clear);
        SetBrush(CadInput, "TextControlBorderBrush", clear);
        SetBrush(CadInput, "TextControlBorderBrushFocused", clear);
        SetBrush(CadInput, "TextControlBorderBrushPointerOver", clear);
        CadInput.Foreground = ink;
        CadInput.Background = clear;
        CadInput.BorderThickness = new Thickness(0);
        CadInput.BorderBrush = clear;
        if (accent is SolidColorBrush accentSolid)
        {
            CadInput.SelectionHighlightColor = accentSolid;
        }
        CadInput.FontFamily = ShellFonts.UiFamily;
        CadInput.FontSize = DynFontSize;
        CadInput.PlaceholderText = "";
        SuggestList.Foreground = ink;
        SuggestList.Background = clear;
        SuggestList.BorderThickness = new Thickness(0);
        SuggestList.FontFamily = ShellFonts.UiFamily;
        SuggestList.FontSize = DynFontSize;
        SuggestList.ItemContainerStyle = SuggestItemStyle(clear, ink);
        ClearChromeTitleBar(_bar);
        ClearChromeTitleBar(_dyn);
    }

    /// <summary>Keep floating chrome HWNDs fully clear — no title-bar plate under the Ask pill.</summary>
    private static void ClearChromeTitleBar(Window window)
    {
        var title = window.AppWindow.TitleBar;
        var clear = Microsoft.UI.Colors.Transparent;
        title.ExtendsContentIntoTitleBar = true;
        title.BackgroundColor = clear;
        title.InactiveBackgroundColor = clear;
        title.ButtonBackgroundColor = clear;
        title.ButtonInactiveBackgroundColor = clear;
        title.ButtonHoverBackgroundColor = clear;
        title.ButtonPressedBackgroundColor = clear;
        title.ForegroundColor = clear;
        title.InactiveForegroundColor = clear;
        title.ButtonForegroundColor = clear;
        title.ButtonInactiveForegroundColor = clear;
    }

    public void ShowBar(bool on)
    {
        if (on)
        {
            if (_bar.AppWindow.IsVisible)
            {
                return;
            }
            var dark = _bar.Content is FrameworkElement body && body.RequestedTheme != ElementTheme.Light;
            var hwnd = WindowNative.GetWindowHandle(_bar);
            Native.Frame(hwnd, Native.DwmColorNone, dark);
            Native.MatchButtonCorners(hwnd);
            Native.MakeWindowClear(hwnd);
            _bar.AppWindow.Show();
            Native.ShowWindow(hwnd, 8);
            return;
        }
        if (_bar.AppWindow.IsVisible)
        {
            _bar.AppWindow.Hide();
        }
    }

    public void PlaceBar(int x, int y, int width, int height)
    {
        if (width < 1 || height < 1)
        {
            return;
        }
        var size = _bar.AppWindow.Size;
        if (size.Width != width || size.Height != height)
        {
            _bar.AppWindow.Resize(new Windows.Graphics.SizeInt32(width, height));
        }
        var pos = _bar.AppWindow.Position;
        if (pos.X != x || pos.Y != y)
        {
            _bar.AppWindow.Move(new Windows.Graphics.PointInt32(x, y));
        }
    }

    /// <summary>Cursor-anchored CAD field. Prompt/dims args are ignored (typed text only).</summary>
    public void ShowCad(
        bool on,
        int x,
        int y,
        int width,
        int height,
        string prompt,
        bool dims,
        string aLabel,
        string a,
        string bLabel,
        string b,
        int active)
    {
        _ = (prompt, dims, aLabel, a, bLabel, b, active);
        if (!on)
        {
            if (_dyn.AppWindow.IsVisible)
            {
                _dyn.AppWindow.Hide();
            }
            return;
        }
        CadPrompt.Visibility = Visibility.Collapsed;
        CadInput.PlaceholderText = "";
        var pixelsWide = Math.Max(1, width);
        var pixelsHigh = Math.Max(1, height);
        var size = _dyn.AppWindow.Size;
        if (size.Width != pixelsWide || size.Height != pixelsHigh)
        {
            _dyn.AppWindow.Resize(new Windows.Graphics.SizeInt32(pixelsWide, pixelsHigh));
        }
        var pos = _dyn.AppWindow.Position;
        if (pos.X != x || pos.Y != y)
        {
            _dyn.AppWindow.Move(new Windows.Graphics.PointInt32(x, y));
        }
        if (_dyn.AppWindow.IsVisible)
        {
            return;
        }
        var hwnd = WindowNative.GetWindowHandle(_dyn);
        Native.MatchButtonCorners(hwnd);
        Native.MakeWindowClear(hwnd);
        _dyn.AppWindow.Show();
        Native.ShowWindow(hwnd, 8);
        Native.Lift(hwnd);
    }

    public void ShowDyn(bool on, int x, int y, int width, int height, string aLabel, string a, string bLabel, string b, int active) =>
        ShowCad(on, x, y, width, height, "", false, aLabel, a, bLabel, b, active);

    public void Close()
    {
        try
        {
            _bar.Close();
        }
        catch (Exception)
        {
        }
        try
        {
            _dyn.Close();
        }
        catch (Exception)
        {
        }
    }

    private static void AllowActivate(Window window)
    {
        var hwnd = WindowNative.GetWindowHandle(window);
        var extra = Native.GetWindowLongPtr(hwnd, Native.GwlExStyle).ToInt64();
        extra &= ~Native.WsExTransparent;
        extra &= ~Native.WsExNoActivate;
        Native.SetWindowLongPtr(hwnd, Native.GwlExStyle, new IntPtr(extra));
    }

    private static void Own(Window window, IntPtr owner, ref bool ready)
    {
        if (ready || owner == IntPtr.Zero)
        {
            return;
        }
        Native.SetWindowLongPtr(WindowNative.GetWindowHandle(window), Native.GwlpHwndParent, owner);
        ready = true;
    }

    private static void Configure(Window window)
    {
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.SetBorderAndTitleBar(false, false);
            presenter.IsResizable = false;
            presenter.IsMinimizable = false;
            presenter.IsMaximizable = false;
        }
        Native.RoundWindow(WindowNative.GetWindowHandle(window));
    }

    private static Style SuggestItemStyle(Brush background, Brush? foreground = null)
    {
        var style = new Style(typeof(ListViewItem));
        style.Setters.Add(new Setter(FrameworkElement.MinHeightProperty, DynHeightDip));
        style.Setters.Add(new Setter(Control.PaddingProperty, new Thickness(0)));
        style.Setters.Add(new Setter(Control.FontFamilyProperty, ShellFonts.UiFamily));
        style.Setters.Add(new Setter(Control.FontSizeProperty, DynFontSize));
        style.Setters.Add(new Setter(Control.HorizontalContentAlignmentProperty, HorizontalAlignment.Left));
        style.Setters.Add(new Setter(Control.BackgroundProperty, background));
        if (foreground != null)
        {
            style.Setters.Add(new Setter(Control.ForegroundProperty, foreground));
        }
        return style;
    }

    private static void SetBrush(FrameworkElement element, string key, Brush brush) =>
        element.Resources[key] = brush;

    private static Windows.UI.Color Color(byte a, byte r, byte g, byte b) =>
        Windows.UI.Color.FromArgb(a, r, g, b);
}
