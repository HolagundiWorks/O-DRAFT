using System.Text.Json;
using System.Text.Json.Serialization;

namespace Aad.WinUi;

/// <summary>Bundled DXF block sets beside the program: the standard furniture set
/// (<c>furniture/standard</c>) and the discipline sets (<c>blocks/&lt;discipline&gt;</c>).
/// Each folder has a <c>catalog.json</c>; files it lists that are missing are skipped.</summary>
internal static class FurnitureCatalog
{
    internal sealed record Item(
        [property: JsonPropertyName("id")] string Id,
        [property: JsonPropertyName("title")] string Title,
        [property: JsonPropertyName("file")] string File,
        [property: JsonPropertyName("category")] string Category,
        [property: JsonPropertyName("size_mm")] string? SizeMm,
        [property: JsonPropertyName("note")] string? Note);

    private sealed record CatalogFile(
        [property: JsonPropertyName("name")] string Name,
        [property: JsonPropertyName("items")] List<Item> Items);

    internal sealed record Entry(Item Item, string Path);

    /// <summary>One discipline set: folder id, catalogue name and its blocks.</summary>
    internal sealed record Section(string Id, string Name, IReadOnlyList<Entry> Entries);

    /// <summary>Discipline folders under <c>blocks</c>, in panel order.</summary>
    private static readonly string[] DisciplineIds = { "plumbing", "hvac", "electrical", "structural", "interior" };

    private static IReadOnlyList<Entry>? _entries;
    private static IReadOnlyList<Section>? _sections;
    private static string? _name;

    internal static string Name => _name ?? "Standard furniture";

    internal static IReadOnlyList<Entry> Load()
    {
        if (_entries != null)
            return _entries;
        foreach (var dir in CandidateDirs("furniture", "standard"))
        {
            if (Read(dir) is (string name, List<Entry> list))
            {
                _name = string.IsNullOrWhiteSpace(name) ? "Standard furniture" : name;
                _entries = list;
                return _entries;
            }
        }
        _entries = Array.Empty<Entry>();
        return _entries;
    }

    /// <summary>The discipline sets that are present (read once).</summary>
    internal static IReadOnlyList<Section> LoadDisciplines()
    {
        if (_sections != null)
            return _sections;
        var sections = new List<Section>();
        foreach (var id in DisciplineIds)
        {
            foreach (var dir in CandidateDirs("blocks", id))
            {
                if (Read(dir) is (string name, List<Entry> list))
                {
                    sections.Add(new Section(id, string.IsNullOrWhiteSpace(name) ? id : name, list));
                    break;
                }
            }
        }
        _sections = sections;
        return _sections;
    }

    /// <summary>The catalogue in <paramref name="dir"/> with the files that exist, or null.</summary>
    private static (string Name, List<Entry> Entries)? Read(string dir)
    {
        var catalog = System.IO.Path.Combine(dir, "catalog.json");
        if (!System.IO.File.Exists(catalog))
            return null;
        try
        {
            var raw = JsonSerializer.Deserialize<CatalogFile>(System.IO.File.ReadAllText(catalog));
            if (raw?.Items == null || raw.Items.Count == 0)
                return null;
            var list = new List<Entry>();
            foreach (var item in raw.Items.Take(2000))
            {
                if (string.IsNullOrWhiteSpace(item.File) || item.File.Contains("..", StringComparison.Ordinal))
                    continue;
                var path = System.IO.Path.Combine(dir, item.File);
                if (System.IO.File.Exists(path))
                    list.Add(new Entry(item, path));
            }
            return list.Count == 0 ? null : (raw.Name ?? "", list);
        }
        catch (JsonException)
        {
            return null;
        }
        catch (IOException)
        {
            return null;
        }
    }

    private static IEnumerable<string> CandidateDirs(string group, string set)
    {
        var baseDir = AppContext.BaseDirectory;
        yield return System.IO.Path.Combine(baseDir, group, set);
        yield return System.IO.Path.Combine(baseDir, "assets", group, set);
        // Dev: repo assets next to the WinUI project.
        yield return System.IO.Path.GetFullPath(System.IO.Path.Combine(baseDir, "..", "..", "..", "..", "..", "assets", group, set));
    }
}
