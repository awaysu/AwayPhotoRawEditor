using System.IO;

namespace AwayPhotoRawEditor.Diagnostics;

/// <summary>Lightweight append-only file tracer for diagnosing native crashes (enable with AWPR_TRACE=1).</summary>
public static class Trace
{
    private static readonly object Gate = new();
    private static readonly bool Enabled = Environment.GetEnvironmentVariable("AWPR_TRACE") == "1";
    private static readonly string Path =
        System.IO.Path.Combine(System.IO.Path.GetTempPath(), "awpr_trace.txt");

    public static void Log(string msg)
    {
        if (!Enabled) return;
        lock (Gate)
        {
            try
            {
                File.AppendAllText(Path, $"[{Environment.CurrentManagedThreadId,3}] {msg}\r\n");
            }
            catch { }
        }
    }
}
