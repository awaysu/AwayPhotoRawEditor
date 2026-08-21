using System.Collections.Generic;
using System.Reflection;
using System.Xml.Serialization;

namespace AwayPhotoRawEditor.Models;

/// <summary>
/// The complete non-destructive edit description for one photo (or virtual copy).
/// Serialized to RAW_TEMP/{file}.rawpipe.xml. All ranges match the UI sliders.
/// </summary>
public sealed class ImageAdjustments
{
    /// <summary>Newest rendering maths. 0 = legacy (WB/exposure multiplied on gamma-encoded
    /// values, black-body Kelvin); 1 = linear light + camera-matrix white balance.</summary>
    public const int CurrentPipelineVersion = 1;

    /// <summary>
    /// Which maths renders this photo. Lives on the XML document (RawPipeDocument), not here —
    /// a plain [XmlIgnore] field so the reflection-based ValueEquals/CopyFrom/ApplyDelta ignore
    /// it (a legacy photo with untouched sliders must not count as "edited"), while
    /// MemberwiseClone still carries it. Old XMLs without the element load as 0 and keep
    /// rendering exactly as before until the user upgrades them; fresh instances are current.
    /// </summary>
    [XmlIgnore] public int PipelineVersion = CurrentPipelineVersion;

    [XmlIgnore] public bool IsLegacyPipeline => PipelineVersion < 1;

    // ---- Basic (基本調整) ------------------------------------------------
    public double Exposure { get; set; }        // v1: -5 .. +5 EV (true stops); legacy slider ±2 (≈ ±4.4 EV)
    public double Contrast { get; set; }        // -100 .. +100
    public double Highlights { get; set; }      // -100 .. +100
    public double Shadows { get; set; }         // -100 .. +100
    public double Whites { get; set; }          // -100 .. +100
    public double Blacks { get; set; }          // -100 .. +100

    // ---- Colour (色彩) ---------------------------------------------------
    public double Temperature { get; set; } = 5200; // 2000 .. 12000 K
    public double Tint { get; set; }                // -100 .. +100
    public double Vibrance { get; set; }            // -100 .. +100
    public double Saturation { get; set; }          // -100 .. +100

    // ---- Detail (細節) ---------------------------------------------------
    public double Sharpening { get; set; }      // -100 .. +100 (negative = soften)
    public double NoiseReduction { get; set; }  // 0 .. 100
    public double Vignette { get; set; }        // -100 .. +100 (positive = darken corners), post-crop

    // ---- Lens / geometry -------------------------------------------------
    public double Distortion { get; set; }      // -100 .. +100 (wide-angle correction)

    // ---- Crop ------------------------------------------------------------
    public string CropAspectRatio { get; set; } = "Original"; // Original / 3:2 / 4:3 / 16:9 / 1:1 / W:H
    public double CropAngle { get; set; }       // -90 .. +90 (fine straighten)
    public double CropX { get; set; } = 0.0;    // normalized
    public double CropY { get; set; } = 0.0;
    public double CropWidth { get; set; } = 1.0;
    public double CropHeight { get; set; } = 1.0;
    public Rotation Rotation { get; set; } = Rotation.R0; // 0/90/180/270

    // ---- Gradient (漸層) — a stack of linear gradients, added on demand -----
    public List<LinearGradient> Gradients { get; set; } = new();

    /// <summary>UI selection: which gradient the sliders / handles currently edit.
    /// Runtime only (never persisted). A plain field so the reflection-based
    /// ValueEquals/CopyFrom/ApplyDelta below ignore it automatically.</summary>
    [XmlIgnore] public int ActiveGradientIndex = -1;

    /// <summary>The currently-selected gradient, or null when there are none.
    /// Clamps a stale index to the last gradient.</summary>
    [XmlIgnore]
    public LinearGradient? ActiveGradient
    {
        get
        {
            if (Gradients.Count == 0) return null;
            if (ActiveGradientIndex < 0 || ActiveGradientIndex >= Gradients.Count)
                ActiveGradientIndex = Gradients.Count - 1;
            return Gradients[ActiveGradientIndex];
        }
    }

    // ---- Heal (修護) -----------------------------------------------------
    public double HealSize { get; set; } = 10;      // brush size 0 .. 50
    public List<HealSpot> HealSpots { get; set; } = new();

    // ---------------------------------------------------------------------

    public ImageAdjustments Clone()
    {
        var c = (ImageAdjustments)MemberwiseClone();
        c.HealSpots = new List<HealSpot>(HealSpots.Count);
        foreach (var s in HealSpots) c.HealSpots.Add(s.Clone());
        c.Gradients = new List<LinearGradient>(Gradients.Count);
        foreach (var gr in Gradients) c.Gradients.Add(gr.Clone());
        return c;
    }

    /// <summary>Reset absolutely everything to defaults (used by "預設時設定").</summary>
    public void ResetAll()
    {
        var d = new ImageAdjustments();
        CopyFrom(d);
        HealSpots.Clear();
        Gradients.Clear();
        ActiveGradientIndex = -1;
    }

    /// <summary>
    /// Reset only tonal / colour / detail fields (basic + colour + detail + distortion),
    /// leaving crop, rotation, gradient, heal and watermark intact. Used when applying a
    /// look preset so a preset doesn't blow away geometry or local edits.
    /// </summary>
    public void ResetTonal()
    {
        var d = new ImageAdjustments();
        Exposure = d.Exposure; Contrast = d.Contrast; Highlights = d.Highlights;
        Shadows = d.Shadows; Whites = d.Whites; Blacks = d.Blacks;
        Temperature = d.Temperature; Tint = d.Tint; Vibrance = d.Vibrance; Saturation = d.Saturation;
        Sharpening = d.Sharpening; NoiseReduction = d.NoiseReduction; Vignette = d.Vignette; Distortion = d.Distortion;
    }

    /// <summary>Reset only 基本調整 + 色彩 + 細節 slider values, leaving geometry
    /// (crop/distortion/rotation), gradient, heal and watermark untouched.</summary>
    public void ResetBasicColorDetail()
    {
        var d = new ImageAdjustments();
        Exposure = d.Exposure; Contrast = d.Contrast; Highlights = d.Highlights;
        Shadows = d.Shadows; Whites = d.Whites; Blacks = d.Blacks;
        Temperature = d.Temperature; Tint = d.Tint; Vibrance = d.Vibrance; Saturation = d.Saturation;
        Sharpening = d.Sharpening; NoiseReduction = d.NoiseReduction; Vignette = d.Vignette;
    }

    /// <summary>
    /// Copy into this instance every scalar field whose value differs between
    /// <paramref name="edited"/> and <paramref name="baseline"/> — i.e. the fields the user
    /// actually changed during a multi-selection edit. Fields the user did not touch (and
    /// HealSpots, which are position-specific) are left untouched, so each photo keeps its own
    /// individual settings apart from the synced change.
    /// </summary>
    public void ApplyDelta(ImageAdjustments edited, ImageAdjustments baseline)
    {
        foreach (var p in Props)
        {
            if (!p.CanWrite || p.Name == nameof(HealSpots) || p.Name == nameof(Gradients)) continue;
            var ev = p.GetValue(edited);
            if (!Equals(ev, p.GetValue(baseline)))
                p.SetValue(this, ev);
        }
    }

    private void CopyFrom(ImageAdjustments s)
    {
        foreach (var p in Props)
            if (p.CanWrite && p.Name != nameof(HealSpots) && p.Name != nameof(Gradients))
                p.SetValue(this, p.GetValue(s));
    }

    private static readonly PropertyInfo[] Props =
        typeof(ImageAdjustments).GetProperties(BindingFlags.Public | BindingFlags.Instance);

    /// <summary>Value equality across all fields (for dirty / edited detection).</summary>
    public bool ValueEquals(ImageAdjustments? o)
    {
        if (o is null) return false;
        if (ReferenceEquals(this, o)) return true;
        foreach (var p in Props)
        {
            // Skip HealSpots / Gradients (list types, compared below) and computed/read-only
            // properties such as IsDefault — evaluating IsDefault here would recurse into
            // ValueEquals forever.
            if (!p.CanWrite || p.Name == nameof(HealSpots) || p.Name == nameof(Gradients)) continue;
            if (!Equals(p.GetValue(this), p.GetValue(o))) return false;
        }
        if (HealSpots.Count != o.HealSpots.Count) return false;
        for (int i = 0; i < HealSpots.Count; i++)
            if (!HealSpots[i].ValueEquals(o.HealSpots[i])) return false;
        if (Gradients.Count != o.Gradients.Count) return false;
        for (int i = 0; i < Gradients.Count; i++)
            if (!Gradients[i].ValueEquals(o.Gradients[i])) return false;
        return true;
    }

    /// <summary>True when equal to a freshly-constructed default (a "no edits" state).</summary>
    [XmlIgnore]
    public bool IsDefault => ValueEquals(Default);

    private static readonly ImageAdjustments Default = new();
}
