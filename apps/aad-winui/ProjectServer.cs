using System.Diagnostics;
using System.Net.Http;

namespace Aad.WinUi;

/// <summary>
/// The ShilpiDB server for the drawing that is open: <c>shilpid</c> on 127.0.0.1:7420 and its <c>shilpi-http</c> gateway on 127.0.0.1:7421, serving
/// <c>&lt;drawing&gt;.takeoff.vdb</c>. 7420 is where <c>takeoff.record</c> records by default, so the take-off toolbars write into the project's own file with no
/// setting, and O-CoreOp reads (and saves its project) through the gateway, so a recorded take-off reaches it at once. One drawing is served at a time: the
/// active one. Changing drawing saves the old file and serves the new one; closing the app saves and stops the server.
/// </summary>
internal static class ProjectServer
{
    internal const int BackendPort = 7420, GatewayPort = 7421;
    internal const string GatewayUrl = "http://127.0.0.1:7421";

    private static Process? _db, _http;
    private static string? _file;
    private static readonly HttpClient Client = new() { Timeout = TimeSpan.FromSeconds(3) };

    /// <summary>The gateway address while this app is serving <paramref name="vdbPath"/>, else null.</summary>
    internal static string? UrlFor(string? vdbPath) =>
        vdbPath is not null && _file is not null && _db is { HasExited: false }
        && string.Equals(System.IO.Path.GetFullPath(vdbPath), _file, StringComparison.OrdinalIgnoreCase) ? GatewayUrl : null;

    /// <summary>
    /// Serve <paramref name="vdbPath"/> (null: serve nothing). Returns true when it is being served on return. Safe to call again for the same file.
    /// A server already answering on the ports is left alone and nothing is served (O-CoreOp then reads the file itself).
    /// </summary>
    internal static async Task<bool> EnsureAsync(string? vdbPath)
    {
        var full = vdbPath is null ? null : System.IO.Path.GetFullPath(vdbPath);
        if (full is not null && full == _file && _db is { HasExited: false })
        {
            return true;
        }
        await StopAsync();
        if (full is null)
        {
            return false;
        }
        var bin = FindBin();
        if (bin is null)
        {
            return false;                                    // ShilpiDB is not installed: the take-off falls back to writing the file itself
        }
        try
        {
            if (await AnswersAsync())
            {
                return false;                                // a server this app did not start holds the ports (another program's, or one left running): leave it alone
            }
            System.IO.Directory.CreateDirectory(System.IO.Path.GetDirectoryName(full)!);
            _db = Start(System.IO.Path.Combine(bin, "shilpid.exe"), $"--bind 127.0.0.1:{BackendPort} --data \"{full}\" --autosave-secs 2 --log-level error");
            _http = Start(System.IO.Path.Combine(bin, "shilpi-http.exe"), $"--bind 127.0.0.1:{GatewayPort} --backend 127.0.0.1:{BackendPort}");
            for (var i = 0; i < 50; i++)
            {
                if (_db.HasExited)
                {
                    break;
                }
                if (await AnswersAsync())
                {
                    _file = full;
                    return true;
                }
                await Task.Delay(100);
            }
        }
        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or System.IO.IOException)
        {
        }
        await StopAsync();
        return false;
    }

    /// <summary>Save the file, then stop both programs.</summary>
    internal static async Task StopAsync()
    {
        if (_db is not null || _http is not null)
        {
            await SaveAsync();
        }
        foreach (var p in new[] { _http, _db })
        {
            try { if (p is { HasExited: false }) { p.Kill(entireProcessTree: true); } } catch { }
            p?.Dispose();
        }
        _db = _http = null;
        _file = null;
    }

    internal static void StopNow() => StopAsync().GetAwaiter().GetResult();

    private static async Task SaveAsync()
    {
        try
        {
            using var r = await Client.PostAsync(GatewayUrl + "/save", new StringContent(""));
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException)
        {
        }
    }

    private static async Task<bool> AnswersAsync()
    {
        try
        {
            using var r = await Client.GetAsync(GatewayUrl + "/health");
            return r.IsSuccessStatusCode;
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException)
        {
            return false;
        }
    }

    private static Process Start(string exe, string args)
    {
        var p = Process.Start(new ProcessStartInfo(exe, args) { CreateNoWindow = true, UseShellExecute = false })
            ?? throw new InvalidOperationException("could not start " + System.IO.Path.GetFileName(exe));
        Job.Attach(p);                                       // both programs end with this app, even if it crashes
        return p;
    }

    /// <summary>A Windows job that kills what is in it when this app's last handle to it closes (the app ends).</summary>
    private static class Job
    {
        private static readonly IntPtr Handle = Create();

        private static IntPtr Create()
        {
            try
            {
                var job = CreateJobObject(IntPtr.Zero, null);
                var info = new ExtendedLimits();
                info.Basic.LimitFlags = 0x2000;              // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                var size = System.Runtime.InteropServices.Marshal.SizeOf(info);
                var ptr = System.Runtime.InteropServices.Marshal.AllocHGlobal(size);
                try
                {
                    System.Runtime.InteropServices.Marshal.StructureToPtr(info, ptr, false);
                    SetInformationJobObject(job, 9, ptr, (uint)size);
                }
                finally
                {
                    System.Runtime.InteropServices.Marshal.FreeHGlobal(ptr);
                }
                return job;
            }
            catch (Exception)
            {
                return IntPtr.Zero;
            }
        }

        internal static void Attach(Process p)
        {
            if (Handle != IntPtr.Zero)
            {
                AssignProcessToJobObject(Handle, p.Handle);
            }
        }

        [System.Runtime.InteropServices.DllImport("kernel32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
        private static extern IntPtr CreateJobObject(IntPtr attrs, string? name);

        [System.Runtime.InteropServices.DllImport("kernel32.dll")]
        private static extern bool SetInformationJobObject(IntPtr job, int cls, IntPtr info, uint size);

        [System.Runtime.InteropServices.DllImport("kernel32.dll")]
        private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

        [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
        private struct BasicLimits
        {
            public long PerProcessUserTimeLimit, PerJobUserTimeLimit;
            public uint LimitFlags;
            public UIntPtr MinimumWorkingSetSize, MaximumWorkingSetSize;
            public uint ActiveProcessLimit;
            public UIntPtr Affinity;
            public uint PriorityClass, SchedulingClass;
        }

        [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
        private struct IoCounters
        {
            public ulong A, B, C, D, E, F;
        }

        [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
        private struct ExtendedLimits
        {
            public BasicLimits Basic;
            public IoCounters Io;
            public UIntPtr ProcessMemoryLimit, JobMemoryLimit, PeakProcessMemoryUsed, PeakJobMemoryUsed;
        }
    }

    /// <summary>The folder holding shilpid and shilpi-http: beside the app, SHILPIDB_BIN, %LOCALAPPDATA%\ShilpiDB\bin, then PATH.</summary>
    private static string? FindBin()
    {
        var dirs = new List<string?>
        {
            AppContext.BaseDirectory,
            Environment.GetEnvironmentVariable("SHILPIDB_BIN"),
            System.IO.Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ShilpiDB", "bin"),
        };
        dirs.AddRange((Environment.GetEnvironmentVariable("PATH") ?? "").Split(System.IO.Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries));
        return dirs.FirstOrDefault(d => !string.IsNullOrWhiteSpace(d)
            && System.IO.File.Exists(System.IO.Path.Combine(d, "shilpid.exe")) && System.IO.File.Exists(System.IO.Path.Combine(d, "shilpi-http.exe")));
    }
}
