using System.IO;
using System.Xml.Serialization;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Export;

public enum ExportLocation { Desktop, SameAsSource, Custom }
public enum ExportFormat { Jpeg, Bmp, Tiff, Png }

/// <summary>檔名規則：按照原始檔案 / 日期時間 / 數字開始。</summary>
public enum RenameMode { Original, DateTime, Sequence }

/// <summary>存檔遇到相同檔名：檔名接續 _數字 / 直接覆蓋。</summary>
public enum ConflictMode { AppendNumber, Overwrite }

/// <summary>Persistent export configuration (export.xml under %AppData%).</summary>
public sealed class ExportSettings
{
    public ExportLocation Location { get; set; } = ExportLocation.Desktop;
    public string CustomPath { get; set; } = "";

    public bool UseSubFolder { get; set; } = true;
    public string SubFolder { get; set; } = "NEW_IMAGE";

    /// <summary>重新命名規則（預設：按照原始檔案）。</summary>
    public RenameMode Rename { get; set; } = RenameMode.Original;

    /// <summary>存檔遇到相同檔名的處理（預設：檔名接續 _數字）。</summary>
    public ConflictMode Conflict { get; set; } = ConflictMode.AppendNumber;

    public ExportFormat Format { get; set; } = ExportFormat.Jpeg;
    /// <summary>輸出長邊上限（像素）：把影像縮到寬、長中較長的一邊等於此值，維持比例。預設 2400。</summary>
    public int MaxLongEdge { get; set; } = 2400;
    /// <summary>輸出解析度（像素/英寸，DPI），預設 300。</summary>
    public int Resolution { get; set; } = 300;
    public int JpegQuality { get; set; } = 100;
    public bool PreserveExif { get; set; } = true;
    public bool OpenExplorerAfter { get; set; } = true;

    // ---- Watermark (標誌) — global overlay, applied on export and (when enabled) live preview ----
    public bool WatermarkEnabled { get; set; } = false;
    public string WatermarkText { get; set; } = "Watermark";
    public string WatermarkFontName { get; set; } = "Arial";
    public float WatermarkFontSize { get; set; } = 150f;   // 6 .. 300
    public int WatermarkTransparency { get; set; } = 20;   // 0 .. 100
    public WatermarkColor WatermarkColor { get; set; } = WatermarkColor.White;
    public WatermarkPosition WatermarkPosition { get; set; } = WatermarkPosition.BottomRight;
    public int WatermarkMargin { get; set; } = 30;         // 0 .. 9999 px

    /// <summary>Snapshot the watermark fields as a <see cref="WatermarkSpec"/> for the render pipeline.</summary>
    public WatermarkSpec BuildWatermark() => new()
    {
        Enabled = WatermarkEnabled, Text = WatermarkText, FontName = WatermarkFontName,
        FontSize = WatermarkFontSize, Transparency = WatermarkTransparency,
        Color = WatermarkColor, Position = WatermarkPosition, Margin = WatermarkMargin
    };

    public string Extension => Format switch
    {
        ExportFormat.Bmp => ".bmp",
        ExportFormat.Tiff => ".tif",
        ExportFormat.Png => ".png",
        _ => ".jpg"
    };

    private static readonly XmlSerializer Serializer = new(typeof(ExportSettings));

    public static ExportSettings Load()
    {
        var path = Path.Combine(AppPaths.AppDataDir, "export.xml");
        if (!File.Exists(path)) return new ExportSettings();
        try { using var fs = File.OpenRead(path); return Serializer.Deserialize(fs) as ExportSettings ?? new ExportSettings(); }
        catch { return new ExportSettings(); }
    }

    public void Save()
    {
        try
        {
            using var fs = File.Create(Path.Combine(AppPaths.AppDataDir, "export.xml"));
            Serializer.Serialize(fs, this);
        }
        catch { }
    }

    /// <summary>Resolve the destination directory (creating the sub-folder if requested).</summary>
    public string ResolveOutputDir(string sourceFilePath)
    {
        string baseDir = Location switch
        {
            ExportLocation.Desktop => Environment.GetFolderPath(Environment.SpecialFolder.Desktop),
            ExportLocation.SameAsSource => Path.GetDirectoryName(sourceFilePath) ?? "",
            _ => string.IsNullOrWhiteSpace(CustomPath) ? Environment.GetFolderPath(Environment.SpecialFolder.Desktop) : CustomPath
        };
        if (UseSubFolder && !string.IsNullOrWhiteSpace(SubFolder))
            baseDir = Path.Combine(baseDir, SubFolder);
        Directory.CreateDirectory(baseDir);
        return baseDir;
    }
}
