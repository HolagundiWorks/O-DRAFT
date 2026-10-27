using System.Text.Json;
using System.Text.Json.Nodes;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Windows.Foundation;

#if ODRAFT_GRAPH
using ODraft.Graph;
#endif

namespace Aad.WinUi;

/// <summary>
/// Side-panel graph editor (Phase 6): visual node canvas plus JSON. Evaluate uses in-process
/// <c>GraphEngine</c> when <c>ODRAFT_GRAPH</c> is set; Commit uses <c>companion.commit</c>.
/// Preview is stashed with <c>companion.preview</c> for the canvas ghost overlay.
/// </summary>
public sealed partial class MainWindow
{
    private string? _graphPreviewJson;
    private bool _graphNodesMode = true;
    private bool _graphSyncing;
    private string? _graphSelectedId;
    private Point? _graphDragOrigin;
    private Point? _graphNodeOrigin;
    private Border? _graphDragBorder;

    private static readonly JsonSerializerOptions GraphJsonOpts = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = true,
    };

    private const string DefaultGraphJson = """
        {
          "nodes": [
            {"id":"wall","type":"wallOutline","x":0,"y":0,"length":4000,"thickness":230,"level":"Ground","uiX":40,"uiY":40},
            {"id":"door","type":"door","wallX0":0,"wallY0":0,"wallX1":4000,"wallY1":0,"x":2000,"y":0,"width":900,"thickness":200,"level":"Ground","uiX":280,"uiY":40}
          ],
          "edges": []
        }
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
        ApplyGraphModeUi();
        RebuildGraphCanvasFromJson();
#if ODRAFT_GRAPH
        GraphStatus.Text = "Nodes: edit cards, Evaluate (ghost on canvas), then Commit.";
#else
        GraphStatus.Text = "Rebuild with sibling O-Dynamo for in-process evaluate, or Connect Companion.";
#endif
    }

    private void OnGraphModeNodes(object sender, RoutedEventArgs e)
    {
        _graphNodesMode = true;
        SyncJsonFromCanvasPositions();
        ApplyGraphModeUi();
        RebuildGraphCanvasFromJson();
    }

    private void OnGraphModeJson(object sender, RoutedEventArgs e)
    {
        SyncJsonFromCanvasPositions();
        _graphNodesMode = false;
        ApplyGraphModeUi();
    }

    private void ApplyGraphModeUi()
    {
        if (GraphCanvasScroll == null || GraphEditor == null)
        {
            return;
        }
        GraphCanvasScroll.Visibility = _graphNodesMode ? Visibility.Visible : Visibility.Collapsed;
        GraphEditor.Visibility = _graphNodesMode ? Visibility.Collapsed : Visibility.Visible;
    }

    private void OnGraphJsonLostFocus(object sender, RoutedEventArgs e)
    {
        if (_graphNodesMode)
        {
            return;
        }
        RebuildGraphCanvasFromJson();
    }

    private void OnGraphAddWall(object sender, RoutedEventArgs e) => AddGraphNode("wallOutline", new Dictionary<string, JsonNode?>
    {
        ["x"] = JsonValue.Create(0),
        ["y"] = JsonValue.Create(0),
        ["length"] = JsonValue.Create(4000),
        ["thickness"] = JsonValue.Create(230),
        ["level"] = JsonValue.Create("Ground"),
    });

    private void OnGraphAddDoor(object sender, RoutedEventArgs e) => AddGraphNode("door", new Dictionary<string, JsonNode?>
    {
        ["wallX0"] = JsonValue.Create(0),
        ["wallY0"] = JsonValue.Create(0),
        ["wallX1"] = JsonValue.Create(4000),
        ["wallY1"] = JsonValue.Create(0),
        ["x"] = JsonValue.Create(2000),
        ["y"] = JsonValue.Create(0),
        ["width"] = JsonValue.Create(900),
        ["thickness"] = JsonValue.Create(200),
        ["level"] = JsonValue.Create("Ground"),
    });

    private void OnGraphAddNumber(object sender, RoutedEventArgs e) => AddGraphNode("number", new Dictionary<string, JsonNode?>
    {
        ["value"] = JsonValue.Create(4000),
    });

    private void AddGraphNode(string type, Dictionary<string, JsonNode?> props)
    {
        SyncJsonFromCanvasPositions();
        JsonObject root;
        try
        {
            root = JsonNode.Parse(GraphEditor.Text)?.AsObject() ?? new JsonObject();
        }
        catch (JsonException)
        {
            root = JsonNode.Parse(DefaultGraphJson)!.AsObject();
        }
        var nodes = root["nodes"] as JsonArray ?? new JsonArray();
        root["nodes"] = nodes;
        if (root["edges"] is not JsonArray)
        {
            root["edges"] = new JsonArray();
        }
        var id = NextGraphId(nodes, type);
        var node = new JsonObject { ["id"] = id, ["type"] = type, ["uiX"] = 40 + nodes.Count * 24, ["uiY"] = 40 + nodes.Count * 16 };
        foreach (var (k, v) in props)
        {
            node[k] = v is null ? null : v.DeepClone();
        }
        nodes.Add(node);
        _graphSyncing = true;
        GraphEditor.Text = root.ToJsonString(GraphJsonOpts);
        _graphSyncing = false;
        _graphSelectedId = id;
        _graphNodesMode = true;
        ApplyGraphModeUi();
        RebuildGraphCanvasFromJson();
    }

    private static string NextGraphId(JsonArray nodes, string type)
    {
        var prefix = type.Length <= 4 ? type : type[..4];
        for (var n = 1; n < 10_000; n++)
        {
            var id = prefix + n;
            if (!nodes.Any(x => x?["id"]?.GetValue<string>() == id))
            {
                return id;
            }
        }
        return prefix + Guid.NewGuid().ToString("N")[..6];
    }

    private void RebuildGraphCanvasFromJson()
    {
        if (GraphCanvas == null || _graphSyncing)
        {
            return;
        }
        GraphCanvas.Children.Clear();
        JsonObject root;
        try
        {
            root = JsonNode.Parse(string.IsNullOrWhiteSpace(GraphEditor.Text) ? DefaultGraphJson : GraphEditor.Text)?.AsObject()
                ?? new JsonObject();
        }
        catch (JsonException ex)
        {
            GraphStatus.Text = "JSON: " + ex.Message;
            return;
        }
        var nodes = root["nodes"] as JsonArray ?? new JsonArray();
        var edges = root["edges"] as JsonArray ?? new JsonArray();
        var positions = new Dictionary<string, Point>(StringComparer.Ordinal);
        var index = 0;
        foreach (var item in nodes)
        {
            if (item is not JsonObject node)
            {
                continue;
            }
            var id = node["id"]?.GetValue<string>() ?? "";
            if (id.Length == 0)
            {
                continue;
            }
            var type = node["type"]?.GetValue<string>() ?? "?";
            var ux = node["uiX"]?.GetValue<double>() ?? (40 + (index % 3) * 220);
            var uy = node["uiY"]?.GetValue<double>() ?? (40 + (index / 3) * 120);
            index++;
            positions[id] = new Point(ux, uy);
            var card = MakeGraphCard(id, type, node, ux, uy);
            GraphCanvas.Children.Add(card);
        }
        foreach (var item in edges)
        {
            if (item is not JsonObject edge)
            {
                continue;
            }
            var from = edge["from"]?.GetValue<string>() ?? "";
            var to = edge["to"]?.GetValue<string>() ?? "";
            if (!positions.TryGetValue(from, out var a) || !positions.TryGetValue(to, out var b))
            {
                continue;
            }
            var line = new Line
            {
                X1 = a.X + 180,
                Y1 = a.Y + 36,
                X2 = b.X,
                Y2 = b.Y + 36,
                Stroke = new SolidColorBrush(Windows.UI.Color.FromArgb(255, 120, 140, 160)),
                StrokeThickness = 1.5,
                IsHitTestVisible = false,
            };
            GraphCanvas.Children.Insert(0, line);
        }
        GraphCanvas.Width = Math.Max(720, positions.Values.Select(p => p.X).DefaultIfEmpty(0).Max() + 240);
        GraphCanvas.Height = Math.Max(520, positions.Values.Select(p => p.Y).DefaultIfEmpty(0).Max() + 140);
    }

    private Border MakeGraphCard(string id, string type, JsonObject node, double ux, double uy)
    {
        var selected = string.Equals(id, _graphSelectedId, StringComparison.Ordinal);
        var body = new StackPanel { Spacing = 4 };
        body.Children.Add(new TextBlock
        {
            Text = type,
            FontSize = Density.FontCaption,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        body.Children.Add(new TextBlock
        {
            Text = id,
            FontSize = Density.FontCaption,
            Opacity = 0.7,
        });
        var summary = GraphNodeSummary(node);
        if (summary.Length > 0)
        {
            body.Children.Add(new TextBlock
            {
                Text = summary,
                FontSize = Density.FontCaption,
                TextWrapping = TextWrapping.Wrap,
            });
        }
        var card = new Border
        {
            Width = 180,
            MinHeight = 72,
            Padding = new Thickness(8),
            CornerRadius = new CornerRadius(Density.CornerControl),
            Background = new SolidColorBrush(selected
                ? Windows.UI.Color.FromArgb(255, 40, 70, 100)
                : Windows.UI.Color.FromArgb(255, 32, 36, 42)),
            BorderBrush = new SolidColorBrush(selected
                ? Windows.UI.Color.FromArgb(255, 90, 180, 255)
                : Windows.UI.Color.FromArgb(255, 70, 78, 88)),
            BorderThickness = new Thickness(selected ? 2 : 1),
            Child = body,
            Tag = id,
        };
        ToolTipService.SetToolTip(card, "Drag to move. Double-click to edit fields.");
        card.PointerPressed += OnGraphCardPressed;
        card.PointerMoved += OnGraphCardMoved;
        card.PointerReleased += OnGraphCardReleased;
        card.PointerCaptureLost += OnGraphCardReleased;
        card.DoubleTapped += OnGraphCardDoubleTapped;
        Canvas.SetLeft(card, ux);
        Canvas.SetTop(card, uy);
        return card;
    }

    private static string GraphNodeSummary(JsonObject node)
    {
        var parts = new List<string>();
        foreach (var key in new[] { "length", "thickness", "width", "value", "level", "text", "name" })
        {
            if (node[key] is JsonValue v)
            {
                parts.Add($"{key}={v.ToJsonString().Trim('"')}");
            }
            if (parts.Count >= 3)
            {
                break;
            }
        }
        return string.Join(" · ", parts);
    }

    private void OnGraphCardPressed(object sender, PointerRoutedEventArgs e)
    {
        if (sender is not Border border || border.Tag is not string id)
        {
            return;
        }
        _graphSelectedId = id;
        _graphDragBorder = border;
        _graphDragOrigin = e.GetCurrentPoint(GraphCanvas).Position;
        _graphNodeOrigin = new Point(Canvas.GetLeft(border), Canvas.GetTop(border));
        border.CapturePointer(e.Pointer);
        RebuildGraphCanvasFromJson();
        e.Handled = true;
    }

    private void OnGraphCardMoved(object sender, PointerRoutedEventArgs e)
    {
        if (_graphDragBorder is null || _graphDragOrigin is null || _graphNodeOrigin is null)
        {
            return;
        }
        var now = e.GetCurrentPoint(GraphCanvas).Position;
        var dx = now.X - _graphDragOrigin.Value.X;
        var dy = now.Y - _graphDragOrigin.Value.Y;
        Canvas.SetLeft(_graphDragBorder, Math.Max(0, _graphNodeOrigin.Value.X + dx));
        Canvas.SetTop(_graphDragBorder, Math.Max(0, _graphNodeOrigin.Value.Y + dy));
    }

    private void OnGraphCardReleased(object sender, PointerRoutedEventArgs e)
    {
        if (_graphDragBorder is not null)
        {
            try
            {
                _graphDragBorder.ReleasePointerCapture(e.Pointer);
            }
            catch (UnauthorizedAccessException)
            {
            }
        }
        _graphDragBorder = null;
        _graphDragOrigin = null;
        _graphNodeOrigin = null;
        SyncJsonFromCanvasPositions();
        RebuildGraphCanvasFromJson();
    }

    private async void OnGraphCardDoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        if (sender is not Border border || border.Tag is not string id)
        {
            return;
        }
        await EditGraphNodeAsync(id);
    }

    private async Task EditGraphNodeAsync(string id)
    {
        JsonObject root;
        try
        {
            root = JsonNode.Parse(GraphEditor.Text)?.AsObject() ?? new JsonObject();
        }
        catch (JsonException)
        {
            return;
        }
        var nodes = root["nodes"] as JsonArray;
        var node = nodes?.OfType<JsonObject>().FirstOrDefault(n => n["id"]?.GetValue<string>() == id);
        if (node is null)
        {
            return;
        }
        var box = new TextBox
        {
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap,
            Text = node.ToJsonString(GraphJsonOpts),
            MinHeight = 160,
            FontFamily = new FontFamily("Consolas"),
            FontSize = Density.FontCaption,
        };
        var dialog = new ContentDialog
        {
            Title = "Edit node " + id,
            Content = box,
            PrimaryButtonText = "Apply",
            CloseButtonText = "Cancel",
            XamlRoot = Content.XamlRoot,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }
        try
        {
            var edited = JsonNode.Parse(box.Text)?.AsObject();
            if (edited is null)
            {
                GraphStatus.Text = "Node JSON was empty.";
                return;
            }
            edited["id"] = id;
            if (edited["type"] is null)
            {
                edited["type"] = node["type"]?.DeepClone();
            }
            var idx = nodes!.IndexOf(node);
            nodes[idx] = edited;
            _graphSyncing = true;
            GraphEditor.Text = root.ToJsonString(GraphJsonOpts);
            _graphSyncing = false;
            RebuildGraphCanvasFromJson();
        }
        catch (JsonException ex)
        {
            GraphStatus.Text = ex.Message;
        }
    }

    private void SyncJsonFromCanvasPositions()
    {
        if (GraphCanvas == null || GraphEditor == null)
        {
            return;
        }
        JsonObject root;
        try
        {
            root = JsonNode.Parse(string.IsNullOrWhiteSpace(GraphEditor.Text) ? DefaultGraphJson : GraphEditor.Text)?.AsObject()
                ?? new JsonObject();
        }
        catch (JsonException)
        {
            return;
        }
        var nodes = root["nodes"] as JsonArray;
        if (nodes is null)
        {
            return;
        }
        foreach (var child in GraphCanvas.Children.OfType<Border>())
        {
            if (child.Tag is not string id)
            {
                continue;
            }
            var node = nodes.OfType<JsonObject>().FirstOrDefault(n => n["id"]?.GetValue<string>() == id);
            if (node is null)
            {
                continue;
            }
            node["uiX"] = Canvas.GetLeft(child);
            node["uiY"] = Canvas.GetTop(child);
        }
        _graphSyncing = true;
        GraphEditor.Text = root.ToJsonString(GraphJsonOpts);
        _graphSyncing = false;
    }

    private void OnGraphEvaluate(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            GraphStatus.Text = "Host is not ready.";
            return;
        }
        SyncJsonFromCanvasPositions();
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
                new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.CamelCase });
            StashCompanionPreview(_graphPreviewJson);
            GraphStatus.Text =
                $"OK: {evaluated.Lines.Count} lines, {evaluated.Circles.Count} circles, {evaluated.Arcs.Count} arcs — ghost on canvas.";
        }
        catch (Exception ex) when (ex is JsonException or ArgumentException or InvalidOperationException)
        {
            GraphStatus.Text = ex.Message;
            _graphPreviewJson = null;
        }
#else
        using (var graphDoc = JsonDocument.Parse(text))
        {
            var payload = JsonSerializer.Serialize(new { graph = graphDoc.RootElement });
            if (Native.Execute(_host, "companion.evaluate", payload) != 0)
            {
                GraphStatus.Text = Native.LastError(_host);
                _graphPreviewJson = null;
                return;
            }
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            if (doc.RootElement.TryGetProperty("result", out var result))
            {
                _graphPreviewJson = result.GetRawText();
                GraphStatus.Text = "Evaluated via companion pipe — ghost on canvas.";
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

    private void StashCompanionPreview(string resultJson)
    {
        try
        {
            using var doc = JsonDocument.Parse(resultJson);
            var payload = JsonSerializer.Serialize(new { result = doc.RootElement });
            _ = Native.Execute(_host, "companion.preview", payload);
        }
        catch (JsonException)
        {
        }
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
            payload = JsonSerializer.Serialize(new { result = preview.RootElement });
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
        // Clear ghost after a successful commit so it does not sit on top of real geometry.
        _ = Native.Execute(_host, "companion.preview", "{\"result\":null}");
        _graphPreviewJson = null;
        AfterCommand();
    }
}
