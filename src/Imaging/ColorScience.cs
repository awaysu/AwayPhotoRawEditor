using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Imaging;

/// <summary>Which white balance the source pixels were already balanced to.</summary>
public enum WhiteBalanceReference
{
    /// <summary>LibRaw proxy / full decode: balanced with <c>pre_mul</c> (daylight matrix multipliers).</summary>
    Decode,
    /// <summary>Camera-rendered preview (embedded JPEG): the as-shot <c>cam_mul</c> is baked in.</summary>
    AsShot
}

/// <summary>
/// Colour math for the linear-light pipeline (PipelineVersion ≥ 1):
/// the transfer curve LibRaw bakes into its output, CCT ↔ chromaticity on the
/// Planckian locus, and the camera-space white-balance matrix built from LibRaw's
/// <c>pre_mul</c> / <c>cam_mul</c> / <c>rgb_cam</c>.
/// </summary>
public static class ColorScience
{
    // ---- transfer curve --------------------------------------------------
    // LibRaw 預設 gamm = {0.45, 4.5}（init_close_utils.cpp:32）＝ BT.709 OETF：
    //   V = 4.5 L                  (L < 0.018)
    //   V = 1.099 L^0.45 − 0.099   (otherwise)
    // WIC 解出來的 JPEG 是 sRGB，跟 709 差在趾部幾個百分點；對「在已烘焙的 JPEG 上
    // 調白平衡」這種本來就是近似的事，統一用一條曲線比較重要。

    private const int DecodeLutSize = 4096;
    private const int EncodeLutSize = 8192;
    private static readonly float[] DecodeLut = BuildDecodeLut();
    private static readonly float[] EncodeLut = BuildEncodeLut();

    private static float[] BuildDecodeLut()
    {
        var lut = new float[DecodeLutSize + 1];
        for (int i = 0; i <= DecodeLutSize; i++) lut[i] = (float)DecodeExact((double)i / DecodeLutSize);
        return lut;
    }

    private static float[] BuildEncodeLut()
    {
        var lut = new float[EncodeLutSize + 1];
        for (int i = 0; i <= EncodeLutSize; i++) lut[i] = (float)EncodeExact((double)i / EncodeLutSize);
        return lut;
    }

    public static double EncodeExact(double l) =>
        l <= 0 ? 0 : l < 0.018 ? 4.5 * l : 1.099 * Math.Pow(l, 0.45) - 0.099;

    public static double DecodeExact(double v) =>
        v <= 0 ? 0 : v < 0.081 ? v / 4.5 : Math.Pow((v + 0.099) / 1.099, 1.0 / 0.45);

    /// <summary>Encoded (display) value → linear light. Input clamped to 0..1.</summary>
    public static float Linearize(float v) => Sample(DecodeLut, DecodeLutSize, v);

    /// <summary>Linear light → encoded value. Input clamped to 0..1 (highlights clip here,
    /// exactly where the legacy path's tone LUT clipped them).</summary>
    public static float Encode(float l) => Sample(EncodeLut, EncodeLutSize, l);

    private static float Sample(float[] lut, int n, float x)
    {
        if (!(x > 0f)) return 0f;          // also catches NaN
        if (x >= 1f) return lut[n];
        float f = x * n;
        int i = (int)f;
        float t = f - i;
        return lut[i] + (lut[i + 1] - lut[i]) * t;
    }

    // ---- XYZ ↔ linear sRGB (D65) ------------------------------------------

    private static readonly double[] XyzToSrgb =
    {
         3.2404542, -1.5371385, -0.4985314,
        -0.9692660,  1.8760108,  0.0415560,
         0.0556434, -0.2040259,  1.0572252
    };

    private static readonly double[] SrgbToXyz =
    {
        0.4124564, 0.3575761, 0.1804375,
        0.2126729, 0.7151522, 0.0721750,
        0.0193339, 0.1191920, 0.9503041
    };

    // ---- Planckian locus / CCT ---------------------------------------------
    // 色溫一律沿黑體軌跡走（Kang et al. 2002 近似，1667–25000K）。日光軌跡在 4000K 以上
    // 略偏綠（Duv ≈ +0.003），但「拍攝時設定」與滑桿用同一條軌跡才會自洽——偏差由 Tint 吸收。

    public const double MinKelvin = 2000, MaxKelvin = 12000;

    /// <summary>1 tint unit = this much Duv. ±100 ≈ ±0.02 Duv, the same visual strength as the
    /// legacy ±30% green multiplier.</summary>
    public const double DuvPerTintUnit = 0.0002;

    private static (double x, double y) PlanckianXy(double T)
    {
        T = Math.Clamp(T, 1667, 25000);
        double t = 1e3 / T, t2 = t * t, t3 = t2 * t;
        double x = T <= 4000
            ? -0.2661239 * t3 - 0.2343589 * t2 + 0.8776956 * t + 0.179910
            : -3.0258469 * t3 + 2.1070379 * t2 + 0.2226347 * t + 0.240390;
        double x2 = x * x, x3 = x2 * x;
        double y = T <= 2222 ? -1.1063814 * x3 - 1.34811020 * x2 + 2.18555832 * x - 0.20219683
                 : T <= 4000 ? -0.9549476 * x3 - 1.37418593 * x2 + 2.09137015 * x - 0.16748867
                             :  3.0817580 * x3 - 5.87338670 * x2 + 3.75112997 * x - 0.37001483;
        return (x, y);
    }

    private static (double u, double v) XyToUv(double x, double y)
    {
        double d = -2 * x + 12 * y + 3;
        return (4 * x / d, 6 * y / d);
    }

    private static (double x, double y) UvToXy(double u, double v)
    {
        double d = 2 * u - 8 * v + 4;
        return (3 * u / d, 2 * v / d);
    }

    private static (double u, double v) PlanckianUv(double T)
    {
        var (x, y) = PlanckianXy(T);
        return XyToUv(x, y);
    }

    /// <summary>Unit normal to the locus at T pointing to the green side (+Duv).</summary>
    private static (double nu, double nv) GreenNormal(double T)
    {
        var (u0, v0) = PlanckianUv(T - 10);
        var (u1, v1) = PlanckianUv(T + 10);
        double du = u1 - u0, dv = v1 - v0;
        double len = Math.Sqrt(du * du + dv * dv);
        if (len < 1e-12) return (0, 1);
        du /= len; dv /= len;
        // Tangent toward higher T runs to lower u / lower v; green lies at lower u / higher v.
        return (dv, -du);
    }

    /// <summary>XYZ (Y = 1) of the illuminant at (K, tint); +tint = magenta side of the locus.</summary>
    private static (double X, double Y, double Z) IlluminantXyz(double kelvin, double tint)
    {
        kelvin = Math.Clamp(kelvin, MinKelvin, MaxKelvin);
        var (u, v) = PlanckianUv(kelvin);
        var (nu, nv) = GreenNormal(kelvin);
        double duv = -tint * DuvPerTintUnit;
        u += nu * duv; v += nv * duv;
        var (x, y) = UvToXy(u, v);
        if (y <= 1e-6) y = 1e-6;
        return (x / y, 1.0, (1 - x - y) / y);
    }

    /// <summary>Closest locus point (CCT) and signed Duv (+ = green) for a chromaticity.</summary>
    private static (double kelvin, double duv) UvToKelvinDuv(double u, double v)
    {
        double best = MinKelvin, bestD = double.MaxValue;
        for (double T = MinKelvin; T <= MaxKelvin; T += 100)
        {
            double d = Dist2(T);
            if (d < bestD) { bestD = d; best = T; }
        }
        // refine by ternary search inside the winning ±100 K bracket
        double lo = Math.Max(MinKelvin, best - 100), hi = Math.Min(MaxKelvin, best + 100);
        for (int i = 0; i < 40; i++)
        {
            double m1 = lo + (hi - lo) / 3, m2 = hi - (hi - lo) / 3;
            if (Dist2(m1) < Dist2(m2)) hi = m2; else lo = m1;
        }
        double K = (lo + hi) / 2;
        var (u0, v0) = PlanckianUv(K);
        var (nu, nv) = GreenNormal(K);
        double duv = (u - u0) * nu + (v - v0) * nv;
        return (K, duv);

        double Dist2(double T)
        {
            var (pu, pv) = PlanckianUv(T);
            return (pu - u) * (pu - u) + (pv - v) * (pv - v);
        }
    }

    // ---- camera white balance --------------------------------------------

    /// <summary>Raw camera channel response (R,G,B, G = 1) to a neutral patch lit by (K, tint).
    /// Inverse of the multipliers LibRaw would need to neutralise that light. Null when the
    /// matrix throws the illuminant out of the camera's gamut (degenerate profile).</summary>
    private static double[]? NeutralCameraResponse(CameraColorInfo cc, double kelvin, double tint)
    {
        var (X, Y, Z) = IlluminantXyz(kelvin, tint);
        var srgb = Mul3(XyzToSrgb, X, Y, Z);
        var inv = Invert3(cc.RgbCam);
        if (inv is null) return null;
        var camScaled = Mul3(inv, srgb[0], srgb[1], srgb[2]);
        var raw = new double[3];
        for (int i = 0; i < 3; i++)
        {
            raw[i] = camScaled[i] / cc.PreMul[i];
            if (!(raw[i] > 1e-9)) return null;
        }
        return NormalizeGreen(raw);
    }

    /// <summary>Camera multipliers (G = 1) that neutralise the illuminant at (K, tint).</summary>
    public static double[]? KelvinTintToCamMul(CameraColorInfo cc, double kelvin, double tint)
    {
        var raw = NeutralCameraResponse(cc, kelvin, tint);
        if (raw is null) return null;
        return NormalizeGreen(new[] { 1 / raw[0], 1 / raw[1], 1 / raw[2] });
    }

    /// <summary>(K, tint) of the illuminant that a set of camera multipliers neutralises —
    /// as-shot from <c>cam_mul</c>, or a picked neutral patch.</summary>
    public static (double kelvin, double tint)? CamMulToKelvinTint(CameraColorInfo cc, double[] mul)
    {
        if (mul.Length < 3 || !(mul[0] > 0) || !(mul[1] > 0) || !(mul[2] > 0)) return null;
        // neutral patch raw response ∝ 1/mul; LibRaw scales by pre_mul then rgb_cam → linear sRGB
        var camScaled = new double[3];
        for (int i = 0; i < 3; i++) camScaled[i] = cc.PreMul[i] / mul[i];
        var srgb = Mul3(cc.RgbCam, camScaled[0], camScaled[1], camScaled[2]);
        var xyz = Mul3(SrgbToXyz, srgb[0], srgb[1], srgb[2]);
        double sum = xyz[0] + xyz[1] + xyz[2];
        if (!(sum > 1e-9) || !(xyz[1] > 0)) return null;
        var (u, v) = XyToUv(xyz[0] / sum, xyz[1] / sum);
        var (K, duv) = UvToKelvinDuv(u, v);
        return (K, Math.Clamp(-duv / DuvPerTintUnit, -100, 100));
    }

    /// <summary>As-shot (K, tint) from the camera's recorded multipliers.</summary>
    public static (double kelvin, double tint)? AsShot(CameraColorInfo cc) =>
        CamMulToKelvinTint(cc, cc.CamMul);

    /// <summary>
    /// 3×3 matrix (row-major) that re-balances <b>linear sRGB</b> pixels — already balanced to
    /// <paramref name="reference"/> — to (K, tint). Built in camera space:
    /// M = rgb_cam · diag(mul_target / mul_reference) · rgb_cam⁻¹. Null → caller falls back to
    /// the black-body multipliers.
    /// </summary>
    public static float[]? WhiteBalanceMatrix(CameraColorInfo cc, double kelvin, double tint, WhiteBalanceReference reference)
    {
        if (!cc.IsValid) return null;
        var target = KelvinTintToCamMul(cc, kelvin, tint);
        if (target is null) return null;
        var refMul = reference == WhiteBalanceReference.AsShot ? cc.CamMul : cc.PreMul;
        var inv = Invert3(cc.RgbCam);
        if (inv is null) return null;

        var gains = new double[3];
        for (int i = 0; i < 3; i++)
        {
            if (!(refMul[i] > 0)) return null;
            gains[i] = target[i] / refMul[i];
        }
        // M = RgbCam · diag(g) · inv
        var md = new double[9];
        for (int r = 0; r < 3; r++)
            for (int c = 0; c < 3; c++)
            {
                double s = 0;
                for (int k = 0; k < 3; k++) s += cc.RgbCam[r * 3 + k] * gains[k] * inv[k * 3 + c];
                md[r * 3 + c] = s;
            }

        // 亮度正規化：讓中性灰的 Y 不變。相機空間裡 G=1 的增益經過矩陣後 sRGB 亮度會飄
        // （6500→3200K 約掉 20%），白平衡滑桿不該順便改曝光——舊版保持 sRGB G=1 也是同樣用意。
        double yr = 0.2126729, yg = 0.7151522, yb = 0.0721750;
        double gr = md[0] + md[1] + md[2], gg = md[3] + md[4] + md[5], gb = md[6] + md[7] + md[8];
        double Y = yr * gr + yg * gg + yb * gb;
        if (!(Y > 1e-6)) return null;
        var m = new float[9];
        for (int i = 0; i < 9; i++) m[i] = (float)(md[i] / Y);
        return m;
    }

    /// <summary>Given a sampled linear-sRGB pixel that should be neutral (WB picker), the camera
    /// multipliers that would make it so — feed to <see cref="CamMulToKelvinTint"/>.</summary>
    public static double[]? NeutralizingCamMul(CameraColorInfo cc, double r, double g, double b, WhiteBalanceReference reference)
    {
        if (!cc.IsValid) return null;
        var inv = Invert3(cc.RgbCam);
        if (inv is null) return null;
        var patch = Mul3(inv, r, g, b);            // camera-scaled response of the patch
        var white = Mul3(inv, 1, 1, 1);            // camera-scaled response of a true neutral
        var refMul = reference == WhiteBalanceReference.AsShot ? cc.CamMul : cc.PreMul;
        var mul = new double[3];
        for (int i = 0; i < 3; i++)
        {
            if (!(patch[i] > 1e-9) || !(white[i] > 1e-9)) return null;
            mul[i] = refMul[i] * white[i] / patch[i];
        }
        return NormalizeGreen(mul);
    }

    // ---- small linear algebra ---------------------------------------------

    private static double[] Mul3(double[] m, double a, double b, double c) => new[]
    {
        m[0] * a + m[1] * b + m[2] * c,
        m[3] * a + m[4] * b + m[5] * c,
        m[6] * a + m[7] * b + m[8] * c
    };

    public static double[]? Invert3(double[] m)
    {
        double a = m[0], b = m[1], c = m[2], d = m[3], e = m[4], f = m[5], g = m[6], h = m[7], i = m[8];
        double det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
        if (Math.Abs(det) < 1e-12 || double.IsNaN(det)) return null;
        double s = 1 / det;
        return new[]
        {
            (e * i - f * h) * s, (c * h - b * i) * s, (b * f - c * e) * s,
            (f * g - d * i) * s, (a * i - c * g) * s, (c * d - a * f) * s,
            (d * h - e * g) * s, (b * g - a * h) * s, (a * e - b * d) * s
        };
    }

    public static double[] NormalizeGreen(double[] v)
    {
        double g = v[1] > 1e-12 ? v[1] : 1;
        return new[] { v[0] / g, 1.0, v[2] / g };
    }
}
