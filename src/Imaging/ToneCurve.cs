using System;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Imaging;

/// <summary>
/// Builds a 1-D tone response LUT (input 0..1 → output 0..1) composing
/// Blacks/Whites end points, Shadows/Highlights region shifts and Contrast.
/// Applied per channel by <see cref="ImageProcessor"/>.
/// </summary>
public static class ToneCurve
{
    public const int LutSize = 1024;

    public static float[] BuildLut(ImageAdjustments a)
    {
        var lut = new float[LutSize];

        double blacks = a.Blacks / 100.0;      // -1..1
        double whites = a.Whites / 100.0;      // -1..1
        double contrast = a.Contrast / 100.0;  // -1..1
        double hi = a.Highlights / 100.0;      // -1..1
        double sh = a.Shadows / 100.0;         // -1..1

        // End points: negative Blacks deepens blacks; positive Whites lifts whites.
        double bl = Math.Clamp(-blacks * 0.12, -0.1, 0.4);   // black input pivot
        double wl = Math.Clamp(1.0 - whites * 0.12, 0.6, 1.1); // white input pivot
        double span = Math.Max(1e-3, wl - bl);

        for (int i = 0; i < LutSize; i++)
        {
            double x = (double)i / (LutSize - 1);

            // 1) black / white point remap
            double v = (x - bl) / span;
            v = Math.Clamp(v, 0.0, 1.0);

            // 2) shadows / highlights region shift
            double wH = v * v;                 // emphasis on brights
            double wS = (1 - v) * (1 - v);     // emphasis on darks
            v += hi * 0.28 * wH + sh * 0.28 * wS;
            v = Math.Clamp(v, 0.0, 1.0);

            // 3) contrast S-curve around mid grey
            double t = v - 0.5;
            v = 0.5 + t * (1.0 + contrast) + contrast * 0.6 * t * (0.25 - t * t);
            lut[i] = (float)Math.Clamp(v, 0.0, 1.0);
        }
        return lut;
    }

    /// <summary>Sample a LUT with linear interpolation; input clamped to 0..1.</summary>
    public static float Sample(float[] lut, float x)
    {
        if (x <= 0f) return lut[0];
        if (x >= 1f) return lut[^1];
        float f = x * (LutSize - 1);
        int i = (int)f;
        float frac = f - i;
        return lut[i] + (lut[i + 1] - lut[i]) * frac;
    }
}
