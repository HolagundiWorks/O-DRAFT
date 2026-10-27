using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

#if ODRAFT_GRAPH
using ODraft.Graph;
#endif

namespace Aad.WinUi;

/// <summary>
/// Side-panel JSON graph editor (Phase 6). When the sibling O-Dynamo Graph project is referenced
/// (<c>ODRAFT_GRAPH</c>), Evaluate runs in-process; otherwise it uses <c>companion.evaluate</c>.
/// Commit always goes through <c>companion.commit</c> with the last evaluate result.
/// </summary>
public sealed partial class MainWindow
{
    private string? _graphPreviewJson;

    private static readonly JsonSerializerOptions GraphJsonOpts = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = false,
    };

    private const string DefaultGraphJson = """
        {"nodes":[
          {"id":"wall","type":"wallOutline","x":0,"y":0,"length":4000,"thickness":230,"level":"Ground"},
          {"id":"door","type":"door","wallX0":0,"wallY0":0,"wallX1":4000,"wallY1":0,"x":2000,"y":0,"width":900,"thickness":200,"level":"Ground"}
        ],"edges":[]}
        """;

    private void EnsureGraphEditorSeeded()
    {
        if (GraphEditor == null)
        {
            return;
        }
        if (string.IsNullOrWhiteSpace(GraphEditor.Text))
        {
            GraphEditor.Text = DefaultGraphJson;
        }
#if ODRAFT_GRAPH
        GraphStatus.Text = "In-process GraphEngine (O-Dynamo). Evaluate then Commit.";
#else
        GraphStatus.Text = "Connect Companion for evaluate, or rebuild with sibling O-Dynamo for in-process.";
#endif
    }

    private void OnGraphEvaluate(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            GraphStatus.Text = "Host is not ready.";
            return;
        }
        var text = GraphEditor?.Text?.Trim() ?? "";
        if (text.Length == 0 || text.Length > 512_000)
        {
            GraphStatus.Text = "Graph JSON is empty or too large.";
            return;
        }

#if ODRAFT_GRAPH
        try
        {
            using var doc = JsonDocument.Parse(text);
            var evaluated = GraphEngine.Evaluate(doc.RootElement, DateTimeOffset.UtcNow.ToUnixTimeMilliseconds());
            if (!evaluated.Ok)
            {
                var err = evaluated.Diagnostics.FirstOrDefault(d => d.Severity == "error")?.Message ?? "evaluate failed";
                GraphStatus.Text = err;
                _graphPreviewJson = null;
                return;
            }
            _graphPreviewJson = JsonSerializer.Serialize(
                new
                {
                    evaluated.Lines,
                    evaluated.Circles,
                    evaluated.Arcs,
                    evaluated.Inserts,
                    evaluated.TitleFields,
                    evaluated.Texts,
                    evaluated.Dimensions,
                },
                GraphJsonOpts);
            GraphStatus.Text =
                $"OK: {evaluated.Lines.Count} lines, {evaluated.Circles.Count} circles, {evaluated.Arcs.Count} arcs, "
                + $"{evaluated.Inserts.Count} inserts, {evaluated.Texts.Count} texts, {evaluated.Dimensions.Count} dims.";
        }
        catch (Exception ex) when (ex is JsonException or ArgumentException or InvalidOperationException)
        {
            GraphStatus.Text = ex.Message;
            _graphPreviewJson = null;
        }
#else
        // Pipe path: host must be connected to ODynamo.Host.
        var payload = JsonSerializer.Serialize(new { graph = JsonSerializer.Deserialize<JsonElement>(text) }, GraphJsonOpts);
        if (Native.Execute(_host, "companion.evaluate", payload) != 0)
        {
            GraphStatus.Text = Native.LastError(_host);
            _graphPreviewJson = null;
            return;
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            if (doc.RootElement.TryGetProperty("result", out var result))
            {
                _graphPreviewJson = result.GetRawText();
                GraphStatus.Text = "Evaluated via companion pipe.";
            }
            else
            {
                _graphPreviewJson = null;
                GraphStatus.Text = "Evaluate returned no result.";
            }
        }
        catch (JsonException ex)
        {
            GraphStatus.Text = ex.Message;
            _graphPreviewJson = null;
        }
#endif
        AfterCommand();
    }

    private void OnGraphCommit(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            GraphStatus.Text = "Host is not ready.";
            return;
        }
        if (string.IsNullOrWhiteSpace(_graphPreviewJson))
        {
            GraphStatus.Text = "Evaluate first.";
            return;
        }
        string payload;
        try
        {
            using var preview = JsonDocument.Parse(_graphPreviewJson);
            payload = JsonSerializer.Serialize(new { result = preview.RootElement }, GraphJsonOpts);
        }
        catch (JsonException ex)
        {
            GraphStatus.Text = ex.Message;
            return;
        }
        if (Native.Execute(_host, "companion.commit", payload) != 0)
        {
            GraphStatus.Text = Native.LastError(_host);
            return;
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            GraphStatus.Text = doc.RootElement.GetRawText();
        }
        catch (JsonException)
        {
            GraphStatus.Text = "Committed.";
        }
        AfterCommand();
    }
}
