namespace AwayPhotoRawEditor.Models;

/// <summary>
/// A single linear (graduated) filter. Multiple of these stack on one photo
/// (see <see cref="ImageAdjustments.Gradients"/>). Geometry is in normalized image
/// coordinates: <see cref="CenterX"/>/<see cref="CenterY"/> is the white handle (position),
/// <see cref="Range"/> the yellow band (falloff, in image-height units) and
/// <see cref="Angle"/> the blue rotate handle (degrees).
/// </summary>
public sealed class LinearGradient
{
    public double CenterX { get; set; } = 0.5;
    public double CenterY { get; set; } = 0.15;   // new gradients start near the top
    public double Angle { get; set; }             // degrees
    public double Range { get; set; } = 0.25;

    public double Exposure { get; set; }          // -5 .. +5
    public double Contrast { get; set; }          // -100 .. +100
    public double Highlights { get; set; }        // -100 .. +100
    public double Shadows { get; set; }           // -100 .. +100
    public double Saturation { get; set; }        // -100 .. +100

    /// <summary>True when this gradient actually changes any pixels.</summary>
    public bool HasEffect =>
        Exposure != 0 || Contrast != 0 || Highlights != 0 || Shadows != 0 || Saturation != 0;

    public LinearGradient Clone() => (LinearGradient)MemberwiseClone();

    public bool ValueEquals(LinearGradient o) =>
        CenterX == o.CenterX && CenterY == o.CenterY && Angle == o.Angle && Range == o.Range &&
        Exposure == o.Exposure && Contrast == o.Contrast && Highlights == o.Highlights &&
        Shadows == o.Shadows && Saturation == o.Saturation;
}
