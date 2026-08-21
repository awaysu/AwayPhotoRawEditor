namespace AwayPhotoRawEditor.Models;

/// <summary>
/// EXIF / photo metadata as read by ExifReader (ExifTool or WIC fallback) and
/// optionally cached inside the adjustment XML.
/// </summary>
public sealed class ExifData
{
    public string CameraMake { get; set; } = "";
    public string CameraModel { get; set; } = "";
    public string Lens { get; set; } = "";
    public string ISO { get; set; } = "";
    public string Aperture { get; set; } = "";
    public string Shutter { get; set; } = "";
    public string FocalLength { get; set; } = "";
    public string ExposureBias { get; set; } = "";
    public string WhiteBalance { get; set; } = "";
    public string MeteringMode { get; set; } = "";

    /// <summary>As-shot colour temperature (K) if the camera recorded it; 0 = unknown.</summary>
    public double ColorTemperature { get; set; }
    /// <summary>As-shot tint if recorded.</summary>
    public double Tint { get; set; }

    /// <summary>LibRaw colour data (pre_mul / cam_mul / rgb_cam) for the linear pipeline's
    /// white-balance matrix. Null for non-RAW files or when LibRaw could not open the file.</summary>
    public CameraColorInfo? Camera { get; set; }

    public string DateTaken { get; set; } = "";
    public int Width { get; set; }
    public int Height { get; set; }
    public long FileSize { get; set; }
    public string FilePath { get; set; } = "";

    public string FileSizeDisplay =>
        FileSize <= 0 ? "" :
        FileSize >= 1024 * 1024 ? $"{FileSize / 1024.0 / 1024.0:0.0} MB" :
        $"{FileSize / 1024.0:0.0} KB";

    public string DimensionsDisplay => Width > 0 && Height > 0 ? $"{Width} x {Height}" : "";

    public bool HasAsShotWhiteBalance => ColorTemperature is >= 2000 and <= 12000;
}
