namespace AwayPhotoRawEditor.Models;

/// <summary>
/// A single heal / clone point. Positions are stored in normalized image
/// coordinates (0..1) so they survive proxy vs full-resolution scaling.
/// </summary>
public sealed class HealSpot
{
    /// <summary>Where the fix is painted (normalized 0..1).</summary>
    public double TargetX { get; set; }
    public double TargetY { get; set; }

    /// <summary>Where the source pixels are sampled from (normalized 0..1). Clone mode.</summary>
    public double SourceX { get; set; }
    public double SourceY { get; set; }

    /// <summary>Radius in pixels at the resolution it was authored on (informational).</summary>
    public double Radius { get; set; } = 10;

    /// <summary>Radius as a fraction of the image's larger dimension (resolution independent).</summary>
    public double RadiusNorm { get; set; } = 0.02;

    /// <summary>True = inpaint from surrounding pixels; false = clone from source.</summary>
    public bool UseInpaint { get; set; }

    public HealSpot Clone() => (HealSpot)MemberwiseClone();

    public bool ValueEquals(HealSpot o) =>
        TargetX == o.TargetX && TargetY == o.TargetY &&
        SourceX == o.SourceX && SourceY == o.SourceY &&
        Radius == o.Radius && RadiusNorm == o.RadiusNorm &&
        UseInpaint == o.UseInpaint;
}
