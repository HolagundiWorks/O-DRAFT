using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>
/// Drawn marks for the short Architecture toolbar. Gray is geometry already there; blue is what
/// the command adds.
/// </summary>
internal static partial class ToolIcons
{
    public static UIElement Architecture(string id, bool dark)
    {
        var canvas = new Canvas { Width = 24, Height = 24 };
        DrawArchitecture(new Pen(canvas, dark), id);
        return new Viewbox { Width = 22, Height = 22, Child = canvas, Stretch = Stretch.Uniform };
    }

    private static void DrawArchitecture(Pen pen, string id)
    {
        switch (id)
        {
            case "wall.draw":
                pen.Base(3, 8, 21, 8);
                pen.Accent(3, 14, 21, 14);
                pen.Base(3, 8, 3, 14);
                pen.Accent(21, 8, 21, 14);
                break;
            case "wall.door":
                pen.Base(4, 6, 4, 20);
                pen.Base(16, 6, 16, 20);
                pen.Accent(4, 20, 16, 20);
                pen.Arc(4, 20, 12, 0, 90, pen.AccentBrush);
                break;
            case "wall.window":
                pen.Base(3, 7, 21, 7);
                pen.Base(3, 17, 21, 17);
                pen.Accent(6, 7, 6, 17);
                pen.Accent(12, 7, 12, 17);
                pen.Accent(18, 7, 18, 17);
                break;
            case "room":
                pen.Box(4, 5, 16, 14, pen.BaseBrush);
                pen.Label("R", 12, 13, 9, pen.AccentBrush);
                break;
            case "rarea":
                pen.Stroke(true, pen.AccentBrush, (4, 18), (6, 5), (19, 7), (20, 19));
                pen.Label("A", 12, 13, 8, pen.BaseBrush);
                break;
            case "grid.axis":
                pen.Base(4, 6, 4, 20);
                pen.Base(12, 4, 12, 20);
                pen.Accent(20, 6, 20, 20);
                pen.Base(2, 18, 22, 18);
                pen.Circle(4, 6, 1.6, pen.BaseBrush);
                pen.Circle(12, 4, 1.6, pen.BaseBrush);
                pen.Circle(20, 6, 1.6, pen.AccentBrush);
                break;
            case "column.place":
                pen.Base(4, 4, 20, 20);
                pen.Base(20, 4, 4, 20);
                pen.FillRect(9, 9, 6, 6, pen.AccentBrush);
                break;
            case "stair.plan":
                pen.Stroke(false, pen.AccentBrush, (3, 20), (3, 16), (8, 16), (8, 12), (13, 12), (13, 8), (18, 8), (18, 4), (22, 4));
                break;
            case "north.arrow":
                pen.Circle(12, 13, 8, pen.BaseBrush);
                pen.Stroke(true, pen.AccentBrush, (12, 5), (15, 16), (12, 13), (9, 16));
                break;
            default:
                for (var k = 0; k < 3; k++)
                {
                    var y = 5.0 + k * 5;
                    pen.Stroke(true, k == 0 ? pen.AccentBrush : pen.BaseBrush, (3, y + 2), (12, y - 2), (21, y + 2), (12, y + 6));
                }
                break;
        }
    }
}
