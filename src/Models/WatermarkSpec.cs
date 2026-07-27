namespace AwayPhotoRawEditor.Models;

/// <summary>
/// Standalone watermark (標誌) configuration. The watermark is a global export-time
/// overlay (stored in ExportSettings) rather than a per-photo edit; when Enabled it is
/// also drawn live onto the main preview. Sizes are authored at full resolution and
/// scaled per render via <c>ProcessContext.WatermarkScale</c>.
/// </summary>
public sealed class WatermarkSpec
{
    public bool Enabled { get; set; }
    public string Text { get; set; } = "";
    public string FontName { get; set; } = "Arial";
    public float FontSize { get; set; } = 150f;      // 6 .. 300
    public int Transparency { get; set; } = 20;       // 0 .. 100
    public WatermarkColor Color { get; set; } = WatermarkColor.White;
    public WatermarkPosition Position { get; set; } = WatermarkPosition.BottomRight;
    public int Margin { get; set; } = 30;             // 0 .. 9999 px

    public WatermarkSpec Clone() => (WatermarkSpec)MemberwiseClone();
}
