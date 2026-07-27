using System.IO;

namespace AwayPhotoRawEditor.Models;

/// <summary>
/// One entry in the preview list / thumbnail strip. A virtual copy shares the
/// original file on disk and is distinguished only by <see cref="VirtualCopyIndex"/>;
/// its identity string is "path|copy:N".
/// </summary>
public sealed class PhotoItem
{
    public string SourcePath { get; }

    /// <summary>0 = the original; 1,2,3… = virtual copies.</summary>
    public int VirtualCopyIndex { get; }

    public PhotoItem(string sourcePath, int virtualCopyIndex = 0)
    {
        SourcePath = sourcePath;
        VirtualCopyIndex = virtualCopyIndex;
    }

    /// <summary>Stable identity used as a dictionary key and in preview_list.xml.</summary>
    public string Key => VirtualCopyIndex <= 0 ? SourcePath : $"{SourcePath}|copy:{VirtualCopyIndex}";

    public bool IsVirtualCopy => VirtualCopyIndex > 0;

    public string FileName => Path.GetFileName(SourcePath);

    public string DisplayName =>
        VirtualCopyIndex <= 0 ? FileName : $"{FileName}  (copy {VirtualCopyIndex})";

    // ---- Transient UI state (not persisted here) ------------------------

    /// <summary>Photo has non-default adjustments (shows the "edited" badge).</summary>
    public bool IsEdited { get; set; }

    /// <summary>Hidden from the preview (persisted in preview_list.xml).</summary>
    public bool IsHidden { get; set; }

    /// <summary>This item is the current "copy settings" source (shows a marker).</summary>
    public bool IsCopySettingsSource { get; set; }

    // ---- Key parsing ----------------------------------------------------

    public static (string path, int copyIndex) ParseKey(string key)
    {
        int i = key.LastIndexOf("|copy:", StringComparison.Ordinal);
        if (i < 0) return (key, 0);
        var path = key[..i];
        return int.TryParse(key[(i + 6)..], out var n) ? (path, n) : (path, 0);
    }

    public static PhotoItem FromKey(string key)
    {
        var (path, n) = ParseKey(key);
        return new PhotoItem(path, n);
    }

    public override string ToString() => Key;
}
