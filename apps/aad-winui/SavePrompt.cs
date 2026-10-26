using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using WinRT.Interop;

namespace Aad.WinUi;

/// <summary>Save / Don't save / Cancel in its own window, so the drawing child does not cover it.</summary>
internal static class SavePrompt
{
    private static Window? _window;

    public static Task<int> Ask(Window owner, string title)
    {
        var done = new TaskCompletionSource<int>();
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;

        var prompt = new TextBlock
        {
            Text = "Save changes to " + title + "?",
            TextWrapping = TextWrapping.Wrap,
            FontSize = Density.FontBody,
            HorizontalAlignment = HorizontalAlignment.Left,
            Margin = new Thickness(0, 0, 0, Density.SpaceMd),
        };
        Density.Name(prompt, "Save changes to " + title + "?");

        var save = Density.DialogAction("Save", primary: true);
        var discard = Density.DialogAction("Don't save");
        var cancel = Density.DialogAction("Cancel");
        var actions = DialogLayout.ActionRow(save, discard, cancel);

        var stack = new StackPanel
        {
            Spacing = 0,
            Margin = Density.DialogMargin,
            VerticalAlignment = VerticalAlignment.Top,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            Children = { prompt, actions },
        };
        Density.Apply(stack);

        var root = new Grid
        {
            RequestedTheme = theme,
            VerticalAlignment = VerticalAlignment.Top,
            Background = Density.AppBackground(),
            Children = { stack },
        };

        var window = new Window
        {
            Title = "O-Draft",
            Content = root,
        };
        var answered = false;
        void Finish(int answer)
        {
            if (answered)
            {
                return;
            }
            answered = true;
            done.TrySetResult(answer);
            window.Close();
        }
        save.Click += (_, _) => Finish(1);
        discard.Click += (_, _) => Finish(2);
        cancel.Click += (_, _) => Finish(0);
        window.AppWindow.Closing += (_, args) =>
        {
            if (!answered)
            {
                answered = true;
                done.TrySetResult(0);
            }
        };
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = false;
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = false;
            presenter.IsAlwaysOnTop = true;
        }
        window.Closed += (_, _) =>
        {
            if (ReferenceEquals(_window, window))
            {
                _window = null;
            }
            if (!answered)
            {
                answered = true;
                done.TrySetResult(0);
            }
        };
        _window = window;
        ShellIcon.Apply(window, theme);
        // Hug content — avoid a tall empty client area under the buttons.
        ShellFit.ToContent(window, minWidthDip: 340, minHeightDip: 72, maxWidthDip: 440, maxHeightDip: 160);
        window.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(window), 8);
        return done.Task;
    }
}
