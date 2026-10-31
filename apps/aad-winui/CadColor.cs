using System.Globalization;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>Parse and format engine colour names (<c>aad_color::Color::name</c> / <c>parse</c>).</summary>
internal readonly record struct CadColorSelection(
    CadColorKind Kind,
    byte Index = 7,
    byte R = 255,
    byte G = 255,
    byte B = 255,
    string? BookName = null,
    string? EntryName = null)
{
    public string ToEngineName() => Kind switch
    {
        CadColorKind.ByLayer => "ByLayer",
        CadColorKind.ByBlock => "ByBlock",
        CadColorKind.Index => CadColor.IndexName(Index),
        CadColorKind.True or CadColorKind.Book => string.Create(CultureInfo.InvariantCulture, $"{R},{G},{B}"),
        _ => "ByLayer",
    };

    public string DisplayName => Kind switch
    {
        CadColorKind.ByLayer => "ByLayer",
        CadColorKind.ByBlock => "ByBlock",
        CadColorKind.Index => CadColor.IndexName(Index),
        CadColorKind.Book when !string.IsNullOrEmpty(BookName) && !string.IsNullOrEmpty(EntryName) => $"{BookName} · {EntryName}",
        CadColorKind.True or CadColorKind.Book => string.Create(CultureInfo.InvariantCulture, $"{R},{G},{B}"),
        _ => "ByLayer",
    };

    public (byte R, byte G, byte B) ResolveRgb()
    {
        return Kind switch
        {
            CadColorKind.ByLayer or CadColorKind.ByBlock => AciPalette.Rgb(7),
            CadColorKind.Index => AciPalette.Rgb(Index),
            CadColorKind.True or CadColorKind.Book => (R, G, B),
            _ => AciPalette.Rgb(7),
        };
    }
}

internal enum CadColorKind
{
    ByLayer,
    ByBlock,
    Index,
    True,
    Book,
}

internal static class CadColor
{
    public static bool TryParse(string? text, out CadColorSelection selection)
    {
        selection = new CadColorSelection(CadColorKind.ByLayer);
        if (string.IsNullOrWhiteSpace(text))
        {
            return false;
        }
        var t = text.Trim();
        if (t.Equals("ByLayer", StringComparison.OrdinalIgnoreCase))
        {
            selection = new CadColorSelection(CadColorKind.ByLayer);
            return true;
        }
        if (t.Equals("ByBlock", StringComparison.OrdinalIgnoreCase))
        {
            selection = new CadColorSelection(CadColorKind.ByBlock);
            return true;
        }
        if (t.Equals("black", StringComparison.OrdinalIgnoreCase))
        {
            selection = new CadColorSelection(CadColorKind.Index, 7);
            return true;
        }
        string[] named = ["", "red", "yellow", "green", "cyan", "blue", "magenta", "white"];
        for (var i = 1; i < named.Length; i++)
        {
            if (t.Equals(named[i], StringComparison.OrdinalIgnoreCase))
            {
                selection = new CadColorSelection(CadColorKind.Index, (byte)i);
                return true;
            }
        }
        if (t.StartsWith("Color ", StringComparison.OrdinalIgnoreCase)
            && byte.TryParse(t.AsSpan(6).Trim(), NumberStyles.Integer, CultureInfo.InvariantCulture, out var ci)
            && ci is >= 1 and <= 255)
        {
            selection = new CadColorSelection(CadColorKind.Index, ci);
            return true;
        }
        if (ushort.TryParse(t, NumberStyles.Integer, CultureInfo.InvariantCulture, out var num))
        {
            selection = num switch
            {
                0 => new CadColorSelection(CadColorKind.ByBlock),
                256 => new CadColorSelection(CadColorKind.ByLayer),
                >= 1 and <= 255 => new CadColorSelection(CadColorKind.Index, (byte)num),
                _ => selection,
            };
            return num is 0 or 256 or (>= 1 and <= 255);
        }
        if (t.StartsWith('#') && t.Length == 7
            && byte.TryParse(t.AsSpan(1, 2), NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var hr)
            && byte.TryParse(t.AsSpan(3, 2), NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var hg)
            && byte.TryParse(t.AsSpan(5, 2), NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var hb))
        {
            selection = new CadColorSelection(CadColorKind.True, R: hr, G: hg, B: hb);
            return true;
        }
        var parts = t.Split(',');
        if (parts.Length == 3
            && byte.TryParse(parts[0].Trim(), NumberStyles.Integer, CultureInfo.InvariantCulture, out var r)
            && byte.TryParse(parts[1].Trim(), NumberStyles.Integer, CultureInfo.InvariantCulture, out var g)
            && byte.TryParse(parts[2].Trim(), NumberStyles.Integer, CultureInfo.InvariantCulture, out var b))
        {
            selection = new CadColorSelection(CadColorKind.True, R: r, G: g, B: b);
            return true;
        }
        var dot = t.IndexOf('·');
        if (dot > 0)
        {
            var book = t[..dot].Trim();
            var entry = t[(dot + 1)..].Trim();
            foreach (var cb in ColorBookCatalog.All())
            {
                if (!cb.Name.Equals(book, StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }
                var hit = cb.Entries.FirstOrDefault(e => e.Name.Equals(entry, StringComparison.OrdinalIgnoreCase));
                if (hit is not null)
                {
                    selection = new CadColorSelection(CadColorKind.Book, R: hit.R, G: hit.G, B: hit.B, BookName: cb.Name, EntryName: hit.Name);
                    return true;
                }
            }
        }
        return false;
    }

    public static string IndexName(byte index) => index switch
    {
        1 => "Red",
        2 => "Yellow",
        3 => "Green",
        4 => "Cyan",
        5 => "Blue",
        6 => "Magenta",
        7 => "White",
        _ => string.Create(CultureInfo.InvariantCulture, $"Color {index}"),
    };

    public static SolidColorBrush SwatchBrush(string engineName)
    {
        if (!TryParse(engineName, out var sel))
        {
            return new SolidColorBrush(Microsoft.UI.Colors.Gray);
        }
        var (r, g, b) = sel.ResolveRgb();
        return new SolidColorBrush(Windows.UI.Color.FromArgb(255, r, g, b));
    }
}
