using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>
/// Fully clear window backdrop so the cursor CAD field does not paint an opaque plate over the drawing.
/// WinUI's default (<c>SystemBackdrop = null</c>) fills the HWND with solid theme white/black.
/// </summary>
internal sealed class ClearBackdrop : SystemBackdrop
{
    private static Windows.UI.Composition.Compositor? _compositor;
    private Windows.UI.Composition.CompositionColorBrush? _brush;

    protected override void OnTargetConnected(ICompositionSupportsSystemBackdrop connectedTarget, XamlRoot xamlRoot)
    {
        _brush = SharedCompositor.CreateColorBrush(Windows.UI.Color.FromArgb(0, 0, 0, 0));
        connectedTarget.SystemBackdrop = _brush;
        if (xamlRoot.ContentIslandEnvironment is { AppWindowId.Value: var id } && id != 0)
        {
            Native.MakeWindowClear(new IntPtr((long)id));
        }
        base.OnTargetConnected(connectedTarget, xamlRoot);
    }

    protected override void OnTargetDisconnected(ICompositionSupportsSystemBackdrop disconnectedTarget)
    {
        disconnectedTarget.SystemBackdrop = null;
        _brush?.Dispose();
        _brush = null;
        base.OnTargetDisconnected(disconnectedTarget);
    }

    private static Windows.UI.Composition.Compositor SharedCompositor
    {
        get
        {
            if (_compositor != null)
            {
                return _compositor;
            }
            Native.EnsureDispatcherQueue();
            return _compositor ??= new Windows.UI.Composition.Compositor();
        }
    }
}
