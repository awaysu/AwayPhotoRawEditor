using System.Collections.Generic;
using System.IO;
using System.Linq;

namespace AwayPhotoRawEditor.App;

/// <summary>
/// Central knowledge of supported formats, the RAW_TEMP cache folder, cache-file
/// naming (thumb / proxy / adjustment XML incl. virtual copies) and locations of
/// the bundled external tools (libraw.dll, exiftool.exe).
/// </summary>
public static class AppPaths
{
    public const string RawTempFolderName = "RAW_TEMP";
    public const string PreviewListFileName = "preview_list.xml";

    /// <summary>Camera RAW formats.</summary>
    public static readonly HashSet<string> RawExtensions = new(StringComparer.OrdinalIgnoreCase)
    {
        ".arw", ".sr2", ".srf", ".cr2", ".cr3", ".crw", ".nef", ".nrw",
        ".raf", ".rw2", ".orf", ".pef", ".dng"
    };

    /// <summary>Regular bitmap formats.</summary>
    public static readonly HashSet<string> RegularExtensions = new(StringComparer.OrdinalIgnoreCase)
    {
        ".jpg", ".jpeg", ".png", ".tif", ".tiff", ".bmp"
    };

    /// <summary>All extensions the folder scanner accepts.</summary>
    public static readonly HashSet<string> SupportedExtensions =
        new(RawExtensions.Concat(RegularExtensions), StringComparer.OrdinalIgnoreCase);

    public static bool IsRaw(string path) => RawExtensions.Contains(Path.GetExtension(path));
    public static bool IsSupported(string path) => SupportedExtensions.Contains(Path.GetExtension(path));

    /// <summary>True when the path is (or lives inside) a RAW_TEMP cache folder.</summary>
    public static bool IsRawTemp(string path)
    {
        var name = Path.GetFileName(path.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar));
        if (string.Equals(name, RawTempFolderName, StringComparison.OrdinalIgnoreCase)) return true;
        return path.Replace('/', '\\')
                   .Contains("\\" + RawTempFolderName + "\\", StringComparison.OrdinalIgnoreCase);
    }

    // ---- RAW_TEMP cache locations ---------------------------------------

    public static string RawTempDir(string imageFolder) => Path.Combine(imageFolder, RawTempFolderName);

    public static string EnsureRawTempDir(string imageFolder)
    {
        var dir = RawTempDir(imageFolder);
        Directory.CreateDirectory(dir);
        return dir;
    }

    private static string CacheDirFor(string imagePath) =>
        RawTempDir(Path.GetDirectoryName(Path.GetFullPath(imagePath))!);

    /// <summary>RAW_TEMP/{file}_thumb.jpg</summary>
    public static string ThumbnailPath(string imagePath) =>
        Path.Combine(CacheDirFor(imagePath), Path.GetFileName(imagePath) + "_thumb.jpg");

    /// <summary>RAW_TEMP/{file}.rawpipe.png</summary>
    public static string ProxyPath(string imagePath) =>
        Path.Combine(CacheDirFor(imagePath), Path.GetFileName(imagePath) + ".rawpipe.png");

    /// <summary>
    /// RAW_TEMP/{file}.rawpipe.xml for the original, or
    /// RAW_TEMP/{file}.copyN.rawpipe.xml for a virtual copy (index &gt;= 1).
    /// </summary>
    public static string AdjustmentXmlPath(string imagePath, int virtualCopyIndex = 0)
    {
        var name = Path.GetFileName(imagePath);
        var suffix = virtualCopyIndex <= 0 ? ".rawpipe.xml" : $".copy{virtualCopyIndex}.rawpipe.xml";
        return Path.Combine(CacheDirFor(imagePath), name + suffix);
    }

    public static string PreviewListPath(string imageFolder) =>
        Path.Combine(RawTempDir(imageFolder), PreviewListFileName);

    // ---- Application data / settings ------------------------------------

    public static string AppDataDir
    {
        get
        {
            var root = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            var dir = Path.Combine(root, "AwayPhotoRawEditor");
            if (!Directory.Exists(dir))
            {
                Directory.CreateDirectory(dir);
                // 一次性遷移：把舊名 awPhotoRawEditor 的設定/風格檔/匯出設定搬過來。
                try
                {
                    var old = Path.Combine(root, "awPhotoRawEditor");
                    if (Directory.Exists(old))
                        foreach (var f in Directory.EnumerateFiles(old))
                            File.Copy(f, Path.Combine(dir, Path.GetFileName(f)), overwrite: false);
                }
                catch { }
            }
            return dir;
        }
    }

    public static string SettingsPath => Path.Combine(AppDataDir, "settings.xml");
    public static string PresetsPath => Path.Combine(AppDataDir, "presets.xml");

    // ---- External tool resolution ---------------------------------------

    private static string? _cachedLibRaw;
    private static string? _cachedExifTool;

    /// <summary>Full path to libraw.dll if it can be located, else null.</summary>
    public static string? FindLibRawDll() =>
        _cachedLibRaw ??= FindTool(new[]
        {
            "libraw\\LibRaw-0.22.1\\bin\\libraw.dll",
            "libraw\\bin\\libraw.dll",
            "libraw.dll",
        }, "libraw.dll");

    /// <summary>Full path to exiftool.exe if it can be located, else null.</summary>
    public static string? FindExifTool() =>
        _cachedExifTool ??= FindTool(new[]
        {
            "exiftool\\exiftool.exe",
            "exiftool.exe",
        }, "exiftool.exe");

    /// <summary>
    /// Searches candidate relative paths under every "tools" folder found while
    /// walking up from the executable directory, then finally the PATH.
    /// </summary>
    private static string? FindTool(string[] relativeCandidates, string leafName)
    {
        foreach (var root in ToolsRoots())
            foreach (var rel in relativeCandidates)
            {
                var p = Path.Combine(root, rel);
                if (File.Exists(p)) return p;
            }

        // Fall back to PATH.
        var pathEnv = Environment.GetEnvironmentVariable("PATH") ?? "";
        foreach (var dir in pathEnv.Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries))
        {
            try
            {
                var p = Path.Combine(dir.Trim(), leafName);
                if (File.Exists(p)) return p;
            }
            catch { /* malformed PATH entry */ }
        }
        return null;
    }

    private static IEnumerable<string> ToolsRoots()
    {
        var dir = AppContext.BaseDirectory;
        for (int i = 0; i < 8 && dir is not null; i++)
        {
            yield return Path.Combine(dir, "tools");
            yield return dir;
            dir = Directory.GetParent(dir)?.FullName;
        }
    }
}
