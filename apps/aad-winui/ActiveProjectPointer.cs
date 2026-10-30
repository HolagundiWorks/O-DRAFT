using System.Text.Json;

namespace Aad.WinUi;

/// <summary>
/// Tells the other suite apps which drawing is open here, so O-CoreOp can use the same ShilpiDB file. It keeps
/// <c>%LOCALAPPDATA%\OSAS\active-project.json</c> current: written when the active drawing changes (opened, saved under a new name, another tab
/// chosen), deleted when no saved drawing is active or the app closes. The ShilpiDB file is the one <c>takeoff.record</c> writes with
/// <c>file: true</c>, <c>&lt;drawing&gt;.takeoff.vdb</c> beside the drawing (the drawing's extension replaced).
/// <c>server</c> is the shilpi-http gateway (<see cref="ProjectServer"/>) while this app is serving that file, else null; O-CoreOp then reads and saves through it
/// instead of touching the file.
/// <code>{ "format": "osas-active-project", "version": 1, "app": "O-Draft", "drawing": "C:\p\house.dwg", "shilpidb": "C:\p\house.takeoff.vdb", "server": "http://127.0.0.1:7421", "updated_utc": "…" }</code>
/// </summary>
internal static class ActiveProjectPointer
{
    private static string? _last;

    internal static string FilePath => System.IO.Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "OSAS", "active-project.json");

    /// <summary>The ShilpiDB file that goes with a drawing: beside it, extension replaced by <c>takeoff.vdb</c>.</summary>
    internal static string ShilpiDbFor(string drawingPath) =>
        IsProjectFile(drawingPath) ? drawingPath : System.IO.Path.ChangeExtension(drawingPath, "takeoff.vdb");

    /// <summary>A project file (<c>.odp</c>): the drawing and its take-off are one file, so it is its own ShilpiDB-side file and needs no server.</summary>
    internal static bool IsProjectFile(string path) =>
        string.Equals(System.IO.Path.GetExtension(path), ".odp", StringComparison.OrdinalIgnoreCase);

    /// <summary>Point at <paramref name="drawingPath"/>, or clear the pointer when it is null or empty. A write that changes nothing is skipped.</summary>
    internal static void Set(string? drawingPath, string? serverUrl = null)
    {
        var path = string.IsNullOrWhiteSpace(drawingPath) ? null : System.IO.Path.GetFullPath(drawingPath);
        var key = path is null ? null : path + "|" + serverUrl;
        if (key == _last)
        {
            return;
        }
        _last = key;
        try
        {
            if (key is null)
            {
                System.IO.File.Delete(FilePath);
                return;
            }
            System.IO.Directory.CreateDirectory(System.IO.Path.GetDirectoryName(FilePath)!);
            var json = JsonSerializer.Serialize(new Dictionary<string, object?>
            {
                ["format"] = "osas-active-project",
                ["version"] = 1,
                ["app"] = "O-Draft",
                ["drawing"] = path,
                ["shilpidb"] = ShilpiDbFor(path!),
                ["server"] = serverUrl,
                ["updated_utc"] = DateTime.UtcNow.ToString("o"),
            });
            // write beside, then move over, so a reader never sees half a file
            var tmp = FilePath + ".tmp";
            System.IO.File.WriteAllText(tmp, json);
            System.IO.File.Move(tmp, FilePath, overwrite: true);
        }
        catch (Exception ex) when (ex is System.IO.IOException or UnauthorizedAccessException)
        {
            // the pointer is a convenience for O-CoreOp; never let it disturb drawing work
        }
    }
}
