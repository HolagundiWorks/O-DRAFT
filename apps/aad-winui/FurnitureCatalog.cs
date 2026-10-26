using System.Text.Json;
using System.Text.Json.Serialization;

namespace Aad.WinUi;

/// <summary>Bundled standard furniture DXF set beside the program (<c>furniture/standard</c>).</summary>
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

    private static IReadOnlyList<Entry>? _entries;
    private static string? _name;

    internal static string Name => _name ?? "Standard furniture";

    internal static IReadOnlyList<Entry> Load()
    {
        if (_entries != null)
            return _entries;
        foreach (var dir in CandidateDirs())
        {
            var catalog = System.IO.Path.Combine(dir, "catalog.json");
            if (!System.IO.File.Exists(catalog))
                continue;
            try
            {
                var raw = JsonSerializer.Deserialize<CatalogFile>(System.IO.File.ReadAllText(catalog));
                if (raw?.Items == null || raw.Items.Count == 0)
                    continue;
                var list = new List<Entry>();
                foreach (var item in raw.Items)
                {
                    var path = System.IO.Path.Combine(dir, item.File);
                    if (System.IO.File.Exists(path))
                        list.Add(new Entry(item, path));
                }
                if (list.Count == 0)
                    continue;
                _name = string.IsNullOrWhiteSpace(raw.Name) ? "Standard furniture" : raw.Name;
                _entries = list;
                return _entries;
            }
            catch (JsonException)
            {
                /* try next folder */
            }
            catch (IOException)
            {
                /* try next folder */
            }
        }
        _entries = Array.Empty<Entry>();
        return _entries;
    }

    private static IEnumerable<string> CandidateDirs()
    {
        var baseDir = AppContext.BaseDirectory;
        yield return System.IO.Path.Combine(baseDir, "furniture", "standard");
        yield return System.IO.Path.Combine(baseDir, "assets", "furniture", "standard");
        // Dev: repo assets next to the WinUI project.
        var repo = System.IO.Path.GetFullPath(System.IO.Path.Combine(baseDir, "..", "..", "..", "..", "..", "assets", "furniture", "standard"));
        yield return repo;
    }
}
