using System.Collections.Generic;
using System.IO;
using System.Xml.Serialization;
using AwayPhotoRawEditor.App;

namespace AwayPhotoRawEditor.Storage;

public sealed class VirtualCopyEntry
{
    public string Path { get; set; } = "";
    public int Index { get; set; }
}

/// <summary>Contents of RAW_TEMP/preview_list.xml.</summary>
[XmlRoot("PreviewList")]
public sealed class PreviewList
{
    /// <summary>Item keys hidden from the preview (PhotoItem.Key values).</summary>
    public List<string> Hidden { get; set; } = new();

    /// <summary>Virtual copies (original path + copy index) to recreate on load.</summary>
    public List<VirtualCopyEntry> VirtualCopies { get; set; } = new();
}

/// <summary>Loads / saves the per-folder preview list (hidden items + virtual copies).</summary>
public static class PreviewListStore
{
    private static readonly XmlSerializer Serializer = new(typeof(PreviewList));

    public static PreviewList Load(string imageFolder)
    {
        var path = AppPaths.PreviewListPath(imageFolder);
        if (!File.Exists(path)) return new PreviewList();
        try
        {
            using var fs = File.OpenRead(path);
            return Serializer.Deserialize(fs) as PreviewList ?? new PreviewList();
        }
        catch { return new PreviewList(); }
    }

    public static void Save(string imageFolder, PreviewList list)
    {
        try
        {
            var path = AppPaths.PreviewListPath(imageFolder);
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            using var fs = File.Create(path);
            Serializer.Serialize(fs, list);
        }
        catch { /* non-fatal */ }
    }
}
