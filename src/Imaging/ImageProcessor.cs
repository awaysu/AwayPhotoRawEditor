using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Text;
using System.Threading;
using System.Threading.Tasks;
using AwayPhotoRawEditor.Imaging.Gpu;
using AwayPhotoRawEditor.Models;
using ComputeSharp;

namespace AwayPhotoRawEditor.Imaging;

/// <summary>Per-render context (watermark scaling, cancellation).</summary>
public sealed class ProcessContext
{
    /// <summary>Multiplier applied to watermark font size and margin (output/full-res ratio).</summary>
    public double WatermarkScale { get; set; } = 1.0;
    public bool ForExport { get; set; }

    /// <summary>Global watermark overlay to draw on top of the result (null / disabled = none).</summary>
    public WatermarkSpec? Watermark { get; set; }

    /// <summary>
    /// When true, skip distortion / rotation / crop (steps 9-10) so the viewer shows
    /// the original full frame — used while the gradient / heal overlay is active
    /// (their effects live in the pre-geometry frame) so overlay coordinates stay aligned.
    /// </summary>
    public bool SkipGeometry { get; set; }

    /// <summary>
    /// When true, apply distortion + 90° rotation but skip only the crop-rectangle
    /// extraction — used while the crop tool overlay is active so the distortion slider
    /// and rotate buttons take visible effect, yet the white crop box still frames the
    /// full (distorted/rotated) image.
    /// </summary>
    public bool SkipCropRect { get; set; }

    /// <summary>Camera colour data for the linear pipeline's white-balance matrix. Null →
    /// black-body multipliers (non-RAW, LibRaw unavailable, legacy XML without the data).</summary>
    public CameraColorInfo? Camera { get; set; }

    /// <summary>What the source pixels are already balanced to: LibRaw proxies/decodes →
    /// <see cref="WhiteBalanceReference.Decode"/> (pre_mul); the camera's embedded preview
    /// (strip thumbnails) → <see cref="WhiteBalanceReference.AsShot"/>.</summary>
    public WhiteBalanceReference WhiteBalanceReference { get; set; } = WhiteBalanceReference.Decode;

    /// <summary>Set by the pipeline: at least one stage of this render ran on the GPU.</summary>
    public bool UsedGpu { get; set; }

    /// <summary>Set by the pipeline: the whole render ran on the GPU-resident fast path.</summary>
    public bool GpuResident { get; set; }

    /// <summary>Diagnostics only: force the CPU path for this render regardless of <see cref="GpuPipeline.Enabled"/>.</summary>
    public bool ForceCpu { get; set; }

    public CancellationToken Token { get; set; } = CancellationToken.None;
}

/// <summary>
/// The non-destructive rendering pipeline. Steps (per spec):
/// 1 white balance, 2 exposure, 3 tone LUT, 4 vibrance/saturation, 5 noise
/// reduction, 6 sharpen/soften, 7 gradient, 8 heal, 9 distortion,
/// 10 rotation/crop/straighten, 11 watermark. Pixel work is parallelized per row.
/// <para>
/// GPU（2026-08 起）：<see cref="RunPipeline"/> 把步驟順序寫一次，跑在三種 <see cref="IStageTarget"/> 上——
/// <see cref="ResidentTarget"/>（整張常駐 GPU，最快）、<see cref="BandedTarget"/>（影像留在 CPU 記憶體、
/// 逐段送 GPU，每一段 GPU 做不了就用 CPU 補）。下面的 CPU 函式是「參考實作」，GPU shader 逐行對照它寫；
/// 兩邊算式要一起改（--gputest 會量差異）。逐像素階段的 CPU 函式都帶列範圍 (y0, y1)，
/// 讓 GPU 中途失敗時只補算剩下的列。
/// </para>
/// </summary>
public static class ImageProcessor
{
    // ---- Public entry points --------------------------------------------

    public static Bitmap Apply(Bitmap src, ImageAdjustments adj, ProcessContext ctx)
    {
        using var buf = FloatImageBuffer.FromBitmap(src);
        using var outBuf = ApplyToFloat(buf, adj, ctx);
        var bmp = outBuf.ToBitmap();
        ApplyWatermark(bmp, ctx);
        return bmp;
    }

    public static Bitmap Apply(FloatImageBuffer src, ImageAdjustments adj, ProcessContext ctx)
    {
        using var outBuf = ApplyToFloat(src, adj, ctx);
        var bmp = outBuf.ToBitmap();
        ApplyWatermark(bmp, ctx);
        return bmp;
    }

    /// <summary>Run steps 1-10 producing a new float buffer (geometry may change).</summary>
    public static FloatImageBuffer ApplyToFloat(FloatImageBuffer src, ImageAdjustments adj, ProcessContext ctx)
    {
        // Fast path: whole image resident on the GPU. Any failure leaves 'src' untouched (we only
        // ever read it), so we simply fall through to the banded/CPU path.
        if (!ctx.ForceCpu && GpuPipeline.CanHostWhole(src.Width, src.Height))
        {
            ResidentImage? img = null;
            try
            {
                img = GpuPipeline.CreateResident(src);
                if (img is not null)
                {
                    var result = RunPipeline(new ResidentTarget(img, adj, ctx), adj, ctx);
                    ctx.UsedGpu = true;
                    ctx.GpuResident = true;
                    return result;
                }
            }
            catch (OperationCanceledException) { throw; }
            catch (Exception ex) { GpuPipeline.ReportResidentFailure(ex, (long)src.Width * src.Height * 16); }
            finally { img?.Dispose(); }
        }

        return RunPipeline(new BandedTarget(src.Clone(), adj, ctx, gpu: !ctx.ForceCpu), adj, ctx);
    }

    // ---- the pipeline, written once -------------------------------------------

    /// <summary>Where the pixels live while the pipeline runs. Each call mutates the target in place
    /// (geometry calls may change its size).</summary>
    private interface IStageTarget
    {
        int Width { get; }
        int Height { get; }
        void Pixel(PixelStageParams p);
        void Blur(BlurOp? nr, BlurOp? sh);
        void Heal();
        void Resample(ResampleParams p, int outW, int outH);
        void Rotate(Rotation rot);
        FloatImageBuffer Result();
    }

    private static FloatImageBuffer RunPipeline(IStageTarget t, ImageAdjustments adj, ProcessContext ctx)
    {
        bool blurStages = adj.NoiseReduction > 0 || adj.Sharpening != 0;
        bool gradActive = GradientActive(adj);

        // 1-4（沒有模糊類階段夾在中間時，7 漸層也併進同一個 pass）
        t.Pixel(BuildColorParams(adj, ctx, withGradients: gradActive && !blurStages));

        if (blurStages)
        {
            t.Blur(adj.NoiseReduction > 0 ? NoiseReductionOp(adj) : null,    // 5
                   adj.Sharpening != 0 ? SharpenOp(adj) : null);              // 6
            if (gradActive) t.Pixel(BuildGradientParams(adj));                // 7
        }

        if (adj.HealSpots.Count > 0) t.Heal();                                // 8

        // 10c: 暗角 — post-crop, so it always hugs the frame actually shown/exported.
        void Finish()
        {
            if (adj.Vignette != 0) t.Pixel(BuildVignetteParams(adj, t.Width, t.Height));
        }

        if (ctx.SkipGeometry) { Finish(); return t.Result(); }   // gradient/heal overlay: original frame

        if (adj.Distortion != 0)                                                // 9
            t.Resample(new ResampleParams(0, adj.Distortion / 100.0 * 0.35, 0, 0, 0, 0, 0, 0), t.Width, t.Height);

        t.Rotate(adj.Rotation);                                                 // 10a: discrete 90°, even under the crop overlay

        // Crop overlay: show the full distorted/rotated frame; the crop box is applied only
        // in the final (no-tool) view, so the white frame stays aligned while dragging.
        if (ctx.SkipCropRect)
        {
            // 角度即時預覽：整個畫面繞裁切中心旋轉（取樣方式與 CropRect 相同），
            // 白框內看到的內容就是最終裁切結果；框本身維持不旋轉。
            if (adj.CropAngle != 0)
            {
                int W = t.Width, H = t.Height;
                double cx = (adj.CropX + adj.CropWidth / 2) * W;
                double cy = (adj.CropY + adj.CropHeight / 2) * H;
                double a = adj.CropAngle * Math.PI / 180.0;
                t.Resample(new ResampleParams(1, 0, cx, cy, Math.Sin(a), Math.Cos(a), cx, cy), W, H);
            }
            Finish();
            return t.Result();
        }

        // 10b: crop rectangle (with straighten angle); a full-frame crop is a no-op.
        bool fullCrop = adj.CropX <= 0 && adj.CropY <= 0 &&
                        adj.CropWidth >= 1 && adj.CropHeight >= 1 && adj.CropAngle == 0;
        if (!fullCrop)
        {
            int W = t.Width, H = t.Height;
            int outW = Math.Max(1, (int)Math.Round(Math.Clamp(adj.CropWidth, 0.02, 1.0) * W));
            int outH = Math.Max(1, (int)Math.Round(Math.Clamp(adj.CropHeight, 0.02, 1.0) * H));
            double cx = (adj.CropX + adj.CropWidth / 2) * W;
            double cy = (adj.CropY + adj.CropHeight / 2) * H;
            double a = adj.CropAngle * Math.PI / 180.0;
            t.Resample(new ResampleParams(1, 0, cx, cy, Math.Sin(a), Math.Cos(a), outW / 2.0, outH / 2.0), outW, outH);
        }
        Finish();
        return t.Result();
    }

    /// <summary>Whole image on the GPU. Every call goes straight to the device and throws on failure.</summary>
    private sealed class ResidentTarget : IStageTarget
    {
        private readonly ResidentImage _img;
        private readonly ImageAdjustments _adj;
        private readonly ProcessContext _ctx;
        public ResidentTarget(ResidentImage img, ImageAdjustments adj, ProcessContext ctx) { _img = img; _adj = adj; _ctx = ctx; }
        public int Width => _img.Width;
        public int Height => _img.Height;
        public void Pixel(PixelStageParams p) { _ctx.Token.ThrowIfCancellationRequested(); GpuPipeline.PixelResident(_img, p); }
        public void Blur(BlurOp? nr, BlurOp? sh) { _ctx.Token.ThrowIfCancellationRequested(); GpuPipeline.BlurResident(_img, nr, sh); }
        public void Heal()
        {
            // 修護只碰幾個小圓：下載 → CPU → 上傳，比寫一個 GPU 版划算
            using var cpu = GpuPipeline.DownloadResident(_img);
            ImageProcessor.Heal(cpu, _adj, _ctx.Token);
            GpuPipeline.UploadResident(_img, cpu);
        }
        public void Resample(ResampleParams p, int outW, int outH) { _ctx.Token.ThrowIfCancellationRequested(); GpuPipeline.ResampleResident(_img, p, outW, outH); }
        public void Rotate(Rotation rot) { _ctx.Token.ThrowIfCancellationRequested(); GpuPipeline.RotateResident(_img, rot); }
        public FloatImageBuffer Result() { _ctx.Token.ThrowIfCancellationRequested(); return GpuPipeline.DownloadResident(_img); }
    }

    /// <summary>Image in CPU memory; each stage first tries the banded GPU path, then the CPU reference
    /// implementation for whatever the GPU did not finish.</summary>
    private sealed class BandedTarget : IStageTarget
    {
        private FloatImageBuffer _buf;
        private readonly ImageAdjustments _adj;
        private readonly ProcessContext _ctx;
        private readonly bool _gpu;
        public BandedTarget(FloatImageBuffer buf, ImageAdjustments adj, ProcessContext ctx, bool gpu) { _buf = buf; _adj = adj; _ctx = ctx; _gpu = gpu; }
        public int Width => _buf.Width;
        public int Height => _buf.Height;

        public void Pixel(PixelStageParams p)
        {
            var tok = _ctx.Token;
            int H = _buf.Height;
            int done = _gpu ? GpuPipeline.TryPixelStage(_buf, p, tok) : 0;
            if (done > 0) _ctx.UsedGpu = true;
            if (done >= H) return;
            int f = p.Flags;
            if ((f & PixelFlags.LegacyWb) != 0) WhiteBalanceExposure(_buf, _adj, tok, done, H);
            else if ((f & (PixelFlags.LinearMul | PixelFlags.LinearMatrix)) != 0) WhiteBalanceExposureLinear(_buf, _adj, _ctx, tok, done, H);
            if ((f & PixelFlags.ToneLut) != 0) ApplyLut(_buf, p.ToneLut!, tok, done, H);
            if ((f & PixelFlags.VibSat) != 0) VibranceSaturation(_buf, _adj, tok, done, H);
            if ((f & PixelFlags.Gradients) != 0) Gradient(_buf, _adj, !_adj.IsLegacyPipeline, tok, done, H);
            if ((f & PixelFlags.Vignette) != 0) Vignette(_buf, _adj, tok, done, H);
        }

        public void Blur(BlurOp? nr, BlurOp? sh)
        {
            var g = _gpu ? GpuPipeline.TryBlurStage(_buf, nr, sh, _ctx.Token) : null;
            if (g is not null) { _buf.Dispose(); _buf = g; _ctx.UsedGpu = true; return; }
            if (nr is not null) NoiseReduction(_buf, _adj, _ctx.Token);
            if (sh is not null) Sharpen(_buf, _adj, _ctx.Token);
        }

        public void Heal() => ImageProcessor.Heal(_buf, _adj, _ctx.Token);

        public void Resample(ResampleParams p, int outW, int outH)
        {
            var g = _gpu ? GpuPipeline.TryResample(_buf, p, outW, outH, _ctx.Token) : null;
            if (g is not null) _ctx.UsedGpu = true;
            g ??= CpuResample(_buf, p, outW, outH, _ctx.Token);
            _buf.Dispose();
            _buf = g;
        }

        public void Rotate(Rotation rot)
        {
            var r = RotateDiscrete(_buf, rot);
            if (!ReferenceEquals(r, _buf)) { _buf.Dispose(); _buf = r; }
        }

        public FloatImageBuffer Result() => _buf;
    }

    // ---- GPU parameter builders（與下面各 CPU 函式的係數一一對應）-------------

    private static PixelStageParams BuildColorParams(ImageAdjustments adj, ProcessContext ctx, bool withGradients)
    {
        var p = new PixelStageParams { Flags = PixelFlags.ToneLut, ToneLut = ToneCurve.BuildLut(adj) };
        float exp = (float)Math.Pow(2.0, adj.Exposure);
        if (adj.IsLegacyPipeline)
        {
            var (mr, mg, mb) = WhiteBalanceMultipliers(adj.Temperature, adj.Tint);
            p.Flags |= PixelFlags.LegacyWb;
            p.WbMul = new float3((float)mr * exp, (float)mg * exp, (float)mb * exp);
        }
        else
        {
            float[]? m = ctx.Camera is { IsValid: true } cam
                ? ColorScience.WhiteBalanceMatrix(cam, adj.Temperature, adj.Tint, ctx.WhiteBalanceReference)
                : null;
            if (m is not null)
            {
                p.Flags |= PixelFlags.LinearMatrix;
                p.M0 = new float3(m[0] * exp, m[1] * exp, m[2] * exp);
                p.M1 = new float3(m[3] * exp, m[4] * exp, m[5] * exp);
                p.M2 = new float3(m[6] * exp, m[7] * exp, m[8] * exp);
            }
            else
            {
                var (mr, mg, mb) = WhiteBalanceMultipliers(adj.Temperature, adj.Tint);
                p.Flags |= PixelFlags.LinearMul;
                p.WbMul = new float3((float)mr * exp, (float)mg * exp, (float)mb * exp);
            }
        }
        float sat = (float)(adj.Saturation / 100.0);
        float vib = (float)(adj.Vibrance / 100.0);
        if (sat != 0 || vib != 0) { p.Flags |= PixelFlags.VibSat; p.Sat = sat; p.Vib = vib; }
        if (withGradients) AddGradients(p, adj);
        return p;
    }

    private static PixelStageParams BuildGradientParams(ImageAdjustments adj)
    {
        var p = new PixelStageParams();
        AddGradients(p, adj);
        return p;
    }

    private static void AddGradients(PixelStageParams p, ImageAdjustments adj)
    {
        var list = new List<GradientGpu>();
        foreach (var gr in adj.Gradients)
        {
            if (!gr.HasEffect) continue;
            double a = gr.Angle * Math.PI / 180.0;
            list.Add(new GradientGpu
            {
                SinA = (float)Math.Sin(a), CosA = (float)Math.Cos(a),
                CenterX = (float)gr.CenterX, CenterY = (float)gr.CenterY,
                Inv2Range = (float)(1.0 / (2 * Math.Max(1e-3, gr.Range))),
                Exposure = (float)gr.Exposure,
                Contrast = (float)(gr.Contrast / 100.0),
                Highlights = (float)(gr.Highlights / 100.0),
                Shadows = (float)(gr.Shadows / 100.0),
                Saturation = (float)(gr.Saturation / 100.0),
            });
        }
        if (list.Count == 0) return;
        p.Flags |= PixelFlags.Gradients;
        if (!adj.IsLegacyPipeline) p.Flags |= PixelFlags.GradientLinear;
        p.Gradients = list.ToArray();
    }

    private static PixelStageParams BuildVignetteParams(ImageAdjustments adj, int width, int height)
    {
        float cx = (width - 1) * 0.5f, cy = (height - 1) * 0.5f;
        return new PixelStageParams
        {
            Flags = PixelFlags.Vignette,
            VigAmount = (float)(-adj.Vignette / 100.0),
            VigCx = cx, VigCy = cy,
            VigInvMax = 1f / MathF.Sqrt(cx * cx + cy * cy),
        };
    }

    private static BlurOp NoiseReductionOp(ImageAdjustments adj)
    {
        float strength = (float)(adj.NoiseReduction / 100.0);
        int radius = 1 + (int)Math.Round(strength * 2);
        return new BlurOp(radius, 0, Math.Clamp(strength * 0.8f, 0, 1));
    }

    private static BlurOp SharpenOp(ImageAdjustments adj)
    {
        float amt = (float)(adj.Sharpening / 100.0);
        if (amt > 0) return new BlurOp(1, 1, amt * 1.5f);
        int radius = 1 + (int)Math.Round(-amt * 2);
        return new BlurOp(radius, 0, Math.Clamp(-amt, 0, 1));
    }

    // ---- 1 + 2: white balance & exposure --------------------------------

    private static void WhiteBalanceExposure(FloatImageBuffer buf, ImageAdjustments adj, CancellationToken t, int y0, int y1)
    {
        var (mr, mg, mb) = WhiteBalanceMultipliers(adj.Temperature, adj.Tint);
        float exp = (float)Math.Pow(2.0, adj.Exposure);
        float fr = (float)mr * exp, fg = (float)mg * exp, fb = (float)mb * exp;
        var data = buf.Data;
        ForRows(buf, t, y0, y1, (y) =>
        {
            int i = y * buf.Width * 4;
            for (int x = 0; x < buf.Width; x++, i += 4)
            {
                data[i] *= fr; data[i + 1] *= fg; data[i + 2] *= fb;
            }
        });
    }

    /// <summary>
    /// v1: decode the transfer curve → white balance → exposure → re-encode, so both gains act on
    /// light rather than on gamma-encoded values. White balance is a 3×3 matrix in camera space
    /// when LibRaw gave us the camera's colour data; otherwise the black-body multipliers (now
    /// applied in linear, which is the only correct place for them).
    /// </summary>
    private static void WhiteBalanceExposureLinear(FloatImageBuffer buf, ImageAdjustments adj, ProcessContext ctx, CancellationToken t, int y0, int y1)
    {
        float exp = (float)Math.Pow(2.0, adj.Exposure);
        float[]? m = ctx.Camera is { IsValid: true } cam
            ? ColorScience.WhiteBalanceMatrix(cam, adj.Temperature, adj.Tint, ctx.WhiteBalanceReference)
            : null;

        var data = buf.Data;
        if (m is not null)
        {
            // fold exposure into the matrix
            float m0 = m[0] * exp, m1 = m[1] * exp, m2 = m[2] * exp;
            float m3 = m[3] * exp, m4 = m[4] * exp, m5 = m[5] * exp;
            float m6 = m[6] * exp, m7 = m[7] * exp, m8 = m[8] * exp;
            ForRows(buf, t, y0, y1, (y) =>
            {
                int i = y * buf.Width * 4;
                for (int x = 0; x < buf.Width; x++, i += 4)
                {
                    float r = ColorScience.Linearize(data[i]);
                    float g = ColorScience.Linearize(data[i + 1]);
                    float b = ColorScience.Linearize(data[i + 2]);
                    data[i] = ColorScience.Encode(m0 * r + m1 * g + m2 * b);
                    data[i + 1] = ColorScience.Encode(m3 * r + m4 * g + m5 * b);
                    data[i + 2] = ColorScience.Encode(m6 * r + m7 * g + m8 * b);
                }
            });
        }
        else
        {
            var (mr, mg, mb) = WhiteBalanceMultipliers(adj.Temperature, adj.Tint);
            float fr = (float)mr * exp, fg = (float)mg * exp, fb = (float)mb * exp;
            ForRows(buf, t, y0, y1, (y) =>
            {
                int i = y * buf.Width * 4;
                for (int x = 0; x < buf.Width; x++, i += 4)
                {
                    data[i] = ColorScience.Encode(ColorScience.Linearize(data[i]) * fr);
                    data[i + 1] = ColorScience.Encode(ColorScience.Linearize(data[i + 1]) * fg);
                    data[i + 2] = ColorScience.Encode(ColorScience.Linearize(data[i + 2]) * fb);
                }
            });
        }
    }

    /// <summary>RGB multipliers for a temperature (K) and tint, neutral at 5200 K / 0 tint.
    /// Black-body approximation — the legacy pipeline's only white balance, and v1's fallback
    /// when no camera colour data is available.</summary>
    public static (double r, double g, double b) WhiteBalanceMultipliers(double temperature, double tint)
    {
        var (r, g, b) = KelvinToRgb(temperature);
        var (r0, g0, b0) = KelvinToRgb(5200);
        double mr = r0 / Math.Max(1e-3, r);
        double mg = g0 / Math.Max(1e-3, g);
        double mb = b0 / Math.Max(1e-3, b);
        mr /= mg; mb /= mg; mg = 1.0;               // normalize on green
        mg *= 1.0 - Math.Clamp(tint / 100.0, -1, 1) * 0.30; // +tint = magenta (less green)
        return (mr, mg, mb);
    }

    private static (double r, double g, double b) KelvinToRgb(double kelvin)
    {
        double t = Math.Clamp(kelvin, 1000, 40000) / 100.0;
        double r, g, b;
        r = t <= 66 ? 255 : 329.698727446 * Math.Pow(t - 60, -0.1332047592);
        g = t <= 66 ? 99.4708025861 * Math.Log(t) - 161.1195681661
                    : 288.1221695283 * Math.Pow(t - 60, -0.0755148492);
        b = t >= 66 ? 255 : t <= 19 ? 0 : 138.5177312231 * Math.Log(t - 10) - 305.0447927307;
        return (Math.Clamp(r, 0, 255) / 255.0, Math.Clamp(g, 0, 255) / 255.0, Math.Clamp(b, 0, 255) / 255.0);
    }

    // ---- 3: tone LUT -----------------------------------------------------

    private static void ApplyLut(FloatImageBuffer buf, float[] lut, CancellationToken t, int y0, int y1)
    {
        var data = buf.Data;
        ForRows(buf, t, y0, y1, (y) =>
        {
            int i = y * buf.Width * 4;
            for (int x = 0; x < buf.Width; x++, i += 4)
            {
                data[i] = ToneCurve.Sample(lut, data[i]);
                data[i + 1] = ToneCurve.Sample(lut, data[i + 1]);
                data[i + 2] = ToneCurve.Sample(lut, data[i + 2]);
            }
        });
    }

    // ---- 4: vibrance / saturation ---------------------------------------

    private static void VibranceSaturation(FloatImageBuffer buf, ImageAdjustments adj, CancellationToken t, int y0, int y1)
    {
        float sat = (float)(adj.Saturation / 100.0);
        float vib = (float)(adj.Vibrance / 100.0);
        if (sat == 0 && vib == 0) return;
        var data = buf.Data;
        ForRows(buf, t, y0, y1, (y) =>
        {
            int i = y * buf.Width * 4;
            for (int x = 0; x < buf.Width; x++, i += 4)
            {
                float r = data[i], g = data[i + 1], b = data[i + 2];
                float luma = 0.299f * r + 0.587f * g + 0.114f * b;
                float max = Math.Max(r, Math.Max(g, b));
                float min = Math.Min(r, Math.Min(g, b));
                float curSat = max <= 1e-4f ? 0 : (max - min) / max;
                float f = (1f + vib * (1f - curSat)) * (1f + sat);
                data[i] = Clamp0(luma + (r - luma) * f);
                data[i + 1] = Clamp0(luma + (g - luma) * f);
                data[i + 2] = Clamp0(luma + (b - luma) * f);
            }
        });
    }

    // ---- 5: noise reduction ---------------------------------------------

    private static void NoiseReduction(FloatImageBuffer buf, ImageAdjustments adj, CancellationToken t)
    {
        float strength = (float)(adj.NoiseReduction / 100.0);
        int radius = 1 + (int)Math.Round(strength * 2);
        using var blurred = BoxBlur(buf, radius, t);
        Blend(buf, blurred, strength * 0.8f, t);
    }

    // ---- 6: sharpen / soften --------------------------------------------

    private static void Sharpen(FloatImageBuffer buf, ImageAdjustments adj, CancellationToken t)
    {
        float amt = (float)(adj.Sharpening / 100.0);
        if (amt > 0)
        {
            using var blurred = BoxBlur(buf, 1, t);
            var data = buf.Data; var bd = blurred.Data;
            float k = amt * 1.5f;
            ForRows(buf, t, (y) =>
            {
                int i = y * buf.Width * 4;
                for (int x = 0; x < buf.Width; x++, i += 4)
                {
                    data[i] = Clamp0(data[i] + k * (data[i] - bd[i]));
                    data[i + 1] = Clamp0(data[i + 1] + k * (data[i + 1] - bd[i + 1]));
                    data[i + 2] = Clamp0(data[i + 2] + k * (data[i + 2] - bd[i + 2]));
                }
            });
        }
        else
        {
            int radius = 1 + (int)Math.Round(-amt * 2);
            using var blurred = BoxBlur(buf, radius, t);
            Blend(buf, blurred, -amt, t);
        }
    }

    // ---- 7: graduated filter --------------------------------------------

    public static bool GradientActive(ImageAdjustments a)
    {
        foreach (var gr in a.Gradients) if (gr.HasEffect) return true;
        return false;
    }

    private static void Gradient(FloatImageBuffer buf, ImageAdjustments adj, bool linearExposure, CancellationToken t, int y0, int y1)
    {
        // Stack every linear gradient, each over the running buffer.
        foreach (var gr in adj.Gradients)
            if (gr.HasEffect) ApplyGradient(buf, gr, linearExposure, t, y0, y1);
    }

    /// <param name="linearExposure">v1: the gradient's exposure multiplies light (decode → × →
    /// encode) like the global exposure does; its contrast/highlights/shadows/saturation stay in
    /// the encoded domain, where they were designed.</param>
    private static void ApplyGradient(FloatImageBuffer buf, LinearGradient gr, bool linearExposure, CancellationToken t, int y0, int y1)
    {
        double a = gr.Angle * Math.PI / 180.0;
        double sinA = Math.Sin(a), cosA = Math.Cos(a);
        double centerX = gr.CenterX, centerY = gr.CenterY;
        double range = Math.Max(1e-3, gr.Range);
        float gExp = (float)gr.Exposure;
        float gCon = (float)(gr.Contrast / 100.0);
        float gHi = (float)(gr.Highlights / 100.0);
        float gSh = (float)(gr.Shadows / 100.0);
        float gSat = (float)(gr.Saturation / 100.0);
        int W = buf.Width, H = buf.Height;
        var data = buf.Data;

        ForRows(buf, t, y0, y1, (y) =>
        {
            double ny = (double)y / H;
            int i = y * W * 4;
            for (int x = 0; x < W; x++, i += 4)
            {
                double nx = (double)x / W;
                double d = (nx - centerX) * sinA + (ny - centerY) * cosA;
                float m = (float)Math.Clamp(d / (2 * range) + 0.5, 0.0, 1.0);
                m = m * m * (3 - 2 * m); // smoothstep
                if (m <= 0) continue;

                float r, g, b;
                if (gExp == 0) { r = data[i]; g = data[i + 1]; b = data[i + 2]; }
                else
                {
                    float expMul = (float)Math.Pow(2.0, gExp * m);
                    if (linearExposure)
                    {
                        r = ColorScience.Encode(ColorScience.Linearize(data[i]) * expMul);
                        g = ColorScience.Encode(ColorScience.Linearize(data[i + 1]) * expMul);
                        b = ColorScience.Encode(ColorScience.Linearize(data[i + 2]) * expMul);
                    }
                    else { r = data[i] * expMul; g = data[i + 1] * expMul; b = data[i + 2] * expMul; }
                }

                float luma = 0.299f * r + 0.587f * g + 0.114f * b;
                if (gSat != 0) { float f = 1 + gSat * m; r = luma + (r - luma) * f; g = luma + (g - luma) * f; b = luma + (b - luma) * f; }
                if (gCon != 0) { float c = gCon * m; r = 0.5f + (r - 0.5f) * (1 + c); g = 0.5f + (g - 0.5f) * (1 + c); b = 0.5f + (b - 0.5f) * (1 + c); }
                if (gHi != 0) { float wH = luma * luma * gHi * 0.5f * m; r += wH; g += wH; b += wH; }
                if (gSh != 0) { float wS = (1 - luma) * (1 - luma) * gSh * 0.5f * m; r += wS; g += wS; b += wS; }

                data[i] = Clamp0(r); data[i + 1] = Clamp0(g); data[i + 2] = Clamp0(b);
            }
        });
    }

    // ---- 10c: vignette (暗角) -------------------------------------------

    /// <summary>Radial corner shading applied post-crop: positive darkens the corners,
    /// negative brightens them. Ramps smoothly from ~1/3 radius outward.</summary>
    private static void Vignette(FloatImageBuffer buf, ImageAdjustments adj, CancellationToken t, int y0, int y1)
    {
        float amount = (float)(-adj.Vignette / 100.0);   // slider + → darken (gain < 1)
        int W = buf.Width, H = buf.Height;
        float cx = (W - 1) * 0.5f, cy = (H - 1) * 0.5f;
        float invMax = 1f / MathF.Sqrt(cx * cx + cy * cy);   // corner distance -> 1
        var data = buf.Data;

        ForRows(buf, t, y0, y1, (y) =>
        {
            float dy = (y - cy) * invMax;
            int i = y * W * 4;
            for (int x = 0; x < W; x++, i += 4)
            {
                float dx = (x - cx) * invMax;
                float r = MathF.Sqrt(dx * dx + dy * dy);           // 0 center .. 1 corner
                float m = Math.Clamp((r - 0.35f) / 0.65f, 0f, 1f); // start ~1/3 out
                m = m * m * (3 - 2 * m);                            // smoothstep
                if (m <= 0) continue;
                float g = MathF.Max(0f, 1f + amount * m);
                data[i] *= g; data[i + 1] *= g; data[i + 2] *= g;
            }
        });
    }

    // ---- 8: heal / clone ------------------------------------------------

    private static void Heal(FloatImageBuffer buf, ImageAdjustments adj, CancellationToken t)
    {
        int W = buf.Width, H = buf.Height;
        double maxDim = Math.Max(W, H);
        var src = buf.Clone(); // read from a snapshot so spots don't feed each other
        try
        {
            foreach (var spot in adj.HealSpots)
            {
                t.ThrowIfCancellationRequested();
                int radius = Math.Max(1, (int)Math.Round(spot.RadiusNorm * maxDim));
                int tx = (int)Math.Round(spot.TargetX * W), ty = (int)Math.Round(spot.TargetY * H);
                if (spot.UseInpaint) InpaintSpot(buf, src, tx, ty, radius);
                else CloneSpot(buf, src, tx, ty,
                    (int)Math.Round(spot.SourceX * W), (int)Math.Round(spot.SourceY * H), radius);
            }
        }
        finally { src.Dispose(); }
    }

    private static void CloneSpot(FloatImageBuffer dst, FloatImageBuffer src, int tx, int ty, int sx, int sy, int radius)
    {
        int W = dst.Width, H = dst.Height;
        for (int dy = -radius; dy <= radius; dy++)
            for (int dx = -radius; dx <= radius; dx++)
            {
                double dist = Math.Sqrt(dx * dx + dy * dy);
                if (dist > radius) continue;
                int px = tx + dx, py = ty + dy, qx = sx + dx, qy = sy + dy;
                if (!In(px, py, W, H) || !In(qx, qy, W, H)) continue;
                float a = (float)(1.0 - Smooth(dist / radius)); // feather edges
                int di = (py * W + px) * 4, si = (qy * W + qx) * 4;
                for (int c = 0; c < 3; c++)
                    dst.Data[di + c] = dst.Data[di + c] * (1 - a) + src.Data[si + c] * a;
            }
    }

    private static void InpaintSpot(FloatImageBuffer dst, FloatImageBuffer src, int tx, int ty, int radius)
    {
        int W = dst.Width, H = dst.Height;
        // Average colour of the surrounding ring.
        double sr = 0, sg = 0, sb = 0; int n = 0;
        int ring = radius + Math.Max(2, radius / 2);
        for (double ang = 0; ang < Math.PI * 2; ang += 0.3)
        {
            int qx = tx + (int)(Math.Cos(ang) * ring), qy = ty + (int)(Math.Sin(ang) * ring);
            if (!In(qx, qy, W, H)) continue;
            int si = (qy * W + qx) * 4;
            sr += src.Data[si]; sg += src.Data[si + 1]; sb += src.Data[si + 2]; n++;
        }
        if (n == 0) return;
        float fr = (float)(sr / n), fg = (float)(sg / n), fb = (float)(sb / n);
        for (int dy = -radius; dy <= radius; dy++)
            for (int dx = -radius; dx <= radius; dx++)
            {
                double dist = Math.Sqrt(dx * dx + dy * dy);
                if (dist > radius) continue;
                int px = tx + dx, py = ty + dy;
                if (!In(px, py, W, H)) continue;
                float a = (float)(1.0 - Smooth(dist / radius));
                int di = (py * W + px) * 4;
                dst.Data[di] = dst.Data[di] * (1 - a) + fr * a;
                dst.Data[di + 1] = dst.Data[di + 1] * (1 - a) + fg * a;
                dst.Data[di + 2] = dst.Data[di + 2] * (1 - a) + fb * a;
            }
    }

    // ---- 9 / 10: geometry resampling (CPU reference) ----------------------

    /// <summary>Inverse-map + bilinear resample — the CPU twin of <c>ResampleShader</c>.
    /// Mode 0: radial distortion (step 9). Mode 1: rotate about (Cx,Cy) with the output origin at
    /// (Ox,Oy) — both the crop extraction (origin = output centre) and the full-frame straighten
    /// preview (origin = crop centre) are this one formula.</summary>
    private static FloatImageBuffer CpuResample(FloatImageBuffer src, ResampleParams p, int outW, int outH, CancellationToken t)
    {
        int W = src.Width, H = src.Height;
        var dst = new FloatImageBuffer(outW, outH);
        if (p.Mode == 0)
        {
            double k = p.K;
            ForRows(dst, t, (y) =>
            {
                double ny = (y / (double)H - 0.5) * 2;
                int i = y * outW * 4;
                for (int x = 0; x < outW; x++, i += 4)
                {
                    double nx = (x / (double)W - 0.5) * 2;
                    double r2 = nx * nx + ny * ny;
                    double f = 1 + k * r2;
                    double sxN = nx * f, syN = ny * f;
                    double sx = (sxN / 2 + 0.5) * W, sy = (syN / 2 + 0.5) * H;
                    Sample(src, sx, sy, out float rr, out float gg, out float bb, out float aa);
                    dst.Data[i] = rr; dst.Data[i + 1] = gg; dst.Data[i + 2] = bb; dst.Data[i + 3] = aa;
                }
            });
        }
        else
        {
            double cx = p.Cx, cy = p.Cy, sinA = p.SinA, cosA = p.CosA, ox = p.Ox, oy = p.Oy;
            ForRows(dst, t, (y) =>
            {
                double ry = y - oy;
                int i = y * outW * 4;
                for (int x = 0; x < outW; x++, i += 4)
                {
                    double rx = x - ox;
                    double sx = cx + (rx * cosA - ry * sinA);
                    double sy = cy + (rx * sinA + ry * cosA);
                    Sample(src, sx, sy, out float rr, out float gg, out float bb, out float aa);
                    dst.Data[i] = rr; dst.Data[i + 1] = gg; dst.Data[i + 2] = bb; dst.Data[i + 3] = aa;
                }
            });
        }
        return dst;
    }

    private static FloatImageBuffer RotateDiscrete(FloatImageBuffer src, Rotation rot)
    {
        if (rot == Rotation.R0) return src;
        int W = src.Width, H = src.Height;
        bool swap = rot is Rotation.R90 or Rotation.R270;
        var dst = new FloatImageBuffer(swap ? H : W, swap ? W : H);
        int DW = dst.Width;
        var s = src.Data; var d = dst.Data;
        // 純粹的像素搬移，逐列平行（以前是單執行緒，6000 萬像素要 0.3 秒以上）
        Parallel.For(0, H, y =>
        {
            for (int x = 0; x < W; x++)
            {
                int nx, ny;
                switch (rot)
                {
                    case Rotation.R90: nx = H - 1 - y; ny = x; break;
                    case Rotation.R180: nx = W - 1 - x; ny = H - 1 - y; break;
                    default: nx = y; ny = W - 1 - x; break; // R270
                }
                int si = (y * W + x) * 4, di = (ny * DW + nx) * 4;
                d[di] = s[si]; d[di + 1] = s[si + 1];
                d[di + 2] = s[si + 2]; d[di + 3] = s[si + 3];
            }
        });
        return dst;
    }

    // ---- 11: watermark ---------------------------------------------------

    public static void ApplyWatermark(Bitmap bmp, ProcessContext ctx)
    {
        var wm = ctx.Watermark;
        if (wm is null || !wm.Enabled || string.IsNullOrWhiteSpace(wm.Text)) return;
        float size = (float)Math.Max(4, wm.FontSize * ctx.WatermarkScale);
        int margin = (int)Math.Round(wm.Margin * ctx.WatermarkScale);
        int alpha = (int)Math.Round(255 * (1 - Math.Clamp(wm.Transparency, 0, 100) / 100.0));

        using var g = Graphics.FromImage(bmp);
        g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
        g.SmoothingMode = SmoothingMode.AntiAlias;
        Font font;
        try { font = new Font(wm.FontName, size, GraphicsUnit.Pixel); }
        catch { font = new Font("Microsoft JhengHei UI", size, GraphicsUnit.Pixel); }
        try
        {
            SizeF ts = g.MeasureString(wm.Text, font);
            float x = wm.Position is WatermarkPosition.TopLeft or WatermarkPosition.BottomLeft
                ? margin : bmp.Width - ts.Width - margin;
            float y = wm.Position is WatermarkPosition.TopLeft or WatermarkPosition.TopRight
                ? margin : bmp.Height - ts.Height - margin;
            var c = ResolveWatermarkColor(wm.Color);
            using var brush = new SolidBrush(Color.FromArgb(alpha, c));
            g.DrawString(wm.Text, font, brush, x, y);
        }
        finally { font.Dispose(); }
    }

    private static Color ResolveWatermarkColor(WatermarkColor c) => c switch
    {
        WatermarkColor.Black => Color.FromArgb(0, 0, 0),
        WatermarkColor.Blue => Color.FromArgb(40, 120, 240),
        WatermarkColor.Yellow => Color.FromArgb(245, 210, 40),
        WatermarkColor.Green => Color.FromArgb(60, 190, 90),
        WatermarkColor.Red => Color.FromArgb(230, 50, 50),
        WatermarkColor.Gray => Color.FromArgb(150, 150, 150),
        WatermarkColor.Orange => Color.FromArgb(245, 150, 40),
        _ => Color.FromArgb(255, 255, 255),   // White
    };

    // ---- shared helpers --------------------------------------------------

    private static FloatImageBuffer BoxBlur(FloatImageBuffer src, int radius, CancellationToken t)
    {
        int W = src.Width, H = src.Height;
        var tmp = new FloatImageBuffer(W, H);
        var dst = new FloatImageBuffer(W, H);
        float norm = 1f / (radius * 2 + 1);
        // horizontal
        ForRows(src, t, (y) =>
        {
            int row = y * W * 4;
            for (int x = 0; x < W; x++)
            {
                float r = 0, g = 0, b = 0;
                for (int k = -radius; k <= radius; k++)
                {
                    int xx = Math.Clamp(x + k, 0, W - 1);
                    int i = row + xx * 4;
                    r += src.Data[i]; g += src.Data[i + 1]; b += src.Data[i + 2];
                }
                int o = row + x * 4;
                tmp.Data[o] = r * norm; tmp.Data[o + 1] = g * norm; tmp.Data[o + 2] = b * norm;
                tmp.Data[o + 3] = src.Data[o + 3];
            }
        });
        // vertical
        ForRows(dst, t, (y) =>
        {
            for (int x = 0; x < W; x++)
            {
                float r = 0, g = 0, b = 0;
                for (int k = -radius; k <= radius; k++)
                {
                    int yy = Math.Clamp(y + k, 0, H - 1);
                    int i = (yy * W + x) * 4;
                    r += tmp.Data[i]; g += tmp.Data[i + 1]; b += tmp.Data[i + 2];
                }
                int o = (y * W + x) * 4;
                dst.Data[o] = r * norm; dst.Data[o + 1] = g * norm; dst.Data[o + 2] = b * norm;
                dst.Data[o + 3] = tmp.Data[o + 3];
            }
        });
        tmp.Dispose();
        return dst;
    }

    private static void Blend(FloatImageBuffer dst, FloatImageBuffer other, float amount, CancellationToken t)
    {
        amount = Math.Clamp(amount, 0, 1);
        var a = dst.Data; var b = other.Data;
        ForRows(dst, t, (y) =>
        {
            int i = y * dst.Width * 4;
            for (int x = 0; x < dst.Width; x++, i += 4)
            {
                a[i] += (b[i] - a[i]) * amount;
                a[i + 1] += (b[i + 1] - a[i + 1]) * amount;
                a[i + 2] += (b[i + 2] - a[i + 2]) * amount;
            }
        });
    }

    private static void Sample(FloatImageBuffer buf, double fx, double fy,
        out float r, out float g, out float b, out float a)
    {
        int W = buf.Width, H = buf.Height;
        if (fx < 0) fx = 0; else if (fx > W - 1) fx = W - 1;
        if (fy < 0) fy = 0; else if (fy > H - 1) fy = H - 1;
        int x0 = (int)fx, y0 = (int)fy;
        int x1 = Math.Min(x0 + 1, W - 1), y1 = Math.Min(y0 + 1, H - 1);
        float tx = (float)(fx - x0), ty = (float)(fy - y0);
        int i00 = (y0 * W + x0) * 4, i10 = (y0 * W + x1) * 4;
        int i01 = (y1 * W + x0) * 4, i11 = (y1 * W + x1) * 4;
        r = Lerp2(buf.Data[i00], buf.Data[i10], buf.Data[i01], buf.Data[i11], tx, ty);
        g = Lerp2(buf.Data[i00 + 1], buf.Data[i10 + 1], buf.Data[i01 + 1], buf.Data[i11 + 1], tx, ty);
        b = Lerp2(buf.Data[i00 + 2], buf.Data[i10 + 2], buf.Data[i01 + 2], buf.Data[i11 + 2], tx, ty);
        a = Lerp2(buf.Data[i00 + 3], buf.Data[i10 + 3], buf.Data[i01 + 3], buf.Data[i11 + 3], tx, ty);
    }

    private static float Lerp2(float v00, float v10, float v01, float v11, float tx, float ty)
    {
        float top = v00 + (v10 - v00) * tx;
        float bot = v01 + (v11 - v01) * tx;
        return top + (bot - top) * ty;
    }

    private static void ForRows(FloatImageBuffer buf, CancellationToken t, Action<int> rowAction)
        => ForRows(buf, t, 0, buf.Height, rowAction);

    /// <summary>Rows [y0, y1) in parallel — the range lets a partially completed GPU stage hand
    /// the remaining rows to the CPU implementation.</summary>
    private static void ForRows(FloatImageBuffer buf, CancellationToken t, int y0, int y1, Action<int> rowAction)
    {
        if (y1 <= y0) return;
        var opts = new ParallelOptions { CancellationToken = t };
        Parallel.For(y0, y1, opts, rowAction);
    }

    private static bool In(int x, int y, int w, int h) => x >= 0 && y >= 0 && x < w && y < h;
    private static float Clamp0(float v) => v < 0 ? 0 : v;
    private static double Smooth(double x) { x = Math.Clamp(x, 0, 1); return x * x * (3 - 2 * x); }
}
