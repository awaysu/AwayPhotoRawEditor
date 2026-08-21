using System.IO;
using System.Xml.Serialization;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Storage;

/// <summary>Root of a per-photo .rawpipe.xml document.</summary>
[XmlRoot("RawPipe")]
public sealed class RawPipeDocument
{
    /// <summary>True when written by EnsureDefault and never edited (a placeholder).</summary>
    public bool IsPlaceholder { get; set; }
    /// <summary>Rendering maths for this photo (see <see cref="ImageAdjustments.PipelineVersion"/>).
    /// Absent in XMLs written before v1.0.15 → deserialises as 0 = legacy.</summary>
    public int PipelineVersion { get; set; }
    public ImageAdjustments Adjustments { get; set; } = new();
    public ExifData? Exif { get; set; }
}

/// <summary>
/// Persists <see cref="ImageAdjustments"/> (and cached EXIF) to
/// RAW_TEMP/{file}.rawpipe.xml (or .copyN.rawpipe.xml for virtual copies).
/// </summary>
public static class AdjustmentXmlStore
{
    private static readonly XmlSerializer Serializer = new(typeof(RawPipeDocument));

    public static void Save(string imagePath, ImageAdjustments adjustments, int copyIndex = 0, ExifData? exif = null)
    {
        var path = AppPaths.AdjustmentXmlPath(imagePath, copyIndex);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);

        // Preserve previously stored EXIF if none supplied.
        exif ??= LoadDocument(path)?.Exif;

        var doc = new RawPipeDocument
        {
            IsPlaceholder = false, PipelineVersion = adjustments.PipelineVersion,
            Adjustments = adjustments, Exif = exif
        };
        using var fs = File.Create(path);
        Serializer.Serialize(fs, doc);
    }

    public static ImageAdjustments? Load(string imagePath, int copyIndex = 0)
        => LoadDocument(AppPaths.AdjustmentXmlPath(imagePath, copyIndex))?.Adjustments;

    public static ExifData? LoadExif(string imagePath, int copyIndex = 0)
        => LoadDocument(AppPaths.AdjustmentXmlPath(imagePath, copyIndex))?.Exif;

    /// <summary>Everything in the document in a single read — Load / LoadExif /
    /// IsDefaultPlaceholder each re-parse the file, which adds up over a whole folder.</summary>
    public static (ImageAdjustments? Adjustments, ExifData? Exif, bool IsPlaceholder) LoadAll(
        string imagePath, int copyIndex = 0)
    {
        var doc = LoadDocument(AppPaths.AdjustmentXmlPath(imagePath, copyIndex));
        return (doc?.Adjustments, doc?.Exif, doc?.IsPlaceholder ?? false);
    }

    /// <summary>
    /// Returns the stored adjustments, or creates a default placeholder (marked
    /// IsPlaceholder) and returns fresh defaults if no XML exists yet.
    /// </summary>
    public static ImageAdjustments EnsureDefault(string imagePath, ExifData? exif = null, int copyIndex = 0)
    {
        var path = AppPaths.AdjustmentXmlPath(imagePath, copyIndex);
        var existing = LoadDocument(path);
        if (existing is not null) return existing.Adjustments;

        var adj = new ImageAdjustments();
        // Seed as-shot white balance: from the camera multipliers when LibRaw gave us the colour
        // data (the linear pipeline's own Kelvin scale), else the EXIF-reported Kelvin.
        if (exif?.Camera is { IsValid: true } cam && Imaging.ColorScience.AsShot(cam) is { } shot)
        {
            adj.Temperature = Math.Clamp(shot.kelvin, Imaging.ColorScience.MinKelvin, Imaging.ColorScience.MaxKelvin);
            adj.Tint = shot.tint;
        }
        else if (exif is { HasAsShotWhiteBalance: true })
            adj.Temperature = exif.ColorTemperature;

        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var doc = new RawPipeDocument { IsPlaceholder = true, PipelineVersion = adj.PipelineVersion, Adjustments = adj, Exif = exif };
        try
        {
            using var fs = File.Create(path);
            Serializer.Serialize(fs, doc);
        }
        catch { /* non-fatal */ }
        return adj;
    }

    public static bool IsDefaultPlaceholder(string imagePath, int copyIndex = 0)
    {
        var doc = LoadDocument(AppPaths.AdjustmentXmlPath(imagePath, copyIndex));
        return doc?.IsPlaceholder ?? false;
    }

    public static bool Exists(string imagePath, int copyIndex = 0)
        => File.Exists(AppPaths.AdjustmentXmlPath(imagePath, copyIndex));

    public static void Delete(string imagePath, int copyIndex = 0)
    {
        try { File.Delete(AppPaths.AdjustmentXmlPath(imagePath, copyIndex)); } catch { }
    }

    private static RawPipeDocument? LoadDocument(string path)
    {
        if (!File.Exists(path)) return null;
        try
        {
            using var fs = File.OpenRead(path);
            var doc = Serializer.Deserialize(fs) as RawPipeDocument;
            // The version rides on the document; hand it to the adjustments so the pipeline
            // (which only ever sees ImageAdjustments) knows which maths to use.
            if (doc is not null) doc.Adjustments.PipelineVersion = doc.PipelineVersion;
            return doc;
        }
        catch { return null; }
    }
}
