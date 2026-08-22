using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Text;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Exif;
using AwayPhotoRawEditor.Imaging;
using AwayPhotoRawEditor.Imaging.Gpu;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Diagnostics;

/// <summary>
/// <c>--gputest &lt;img&gt; &lt;report&gt;</c>：同一張影像、同一組調整，CPU 與 GPU 各跑一次，
/// 量「最大差／平均差／超過 1/255 的像素比例／8-bit 輸出不同的位元組數」與耗時。
/// 兩邊只差浮點運算順序，預期：最大差 &lt; 1e-3、8-bit 不同的位元組極少且只差 1。
/// 改 ImageProcessor 或 GpuShaders 的算式後跑這個確認兩邊還是同一條管線。
/// </summary>
public static class GpuParity
{
    public static void Run(string imagePath, string reportPath)
    {
        var sb = new StringBuilder();
        void Line(string s) => sb.AppendLine(s);
        try
        {
            Line("=== AwayPhotoRawEditor GPU / CPU 對照 ===");
            Line($"目標檔案: {imagePath}");
            Line($"GPU     : {GpuPipeline.StatusText}");
            Line($"CPU     : {Environment.ProcessorCount} threads");
            Line("");

            if (!GpuPipeline.IsAvailable)
            {
                Line("GPU 不可用，沒有東西可以對照。");
                return;
            }

            var loader = new RawLoader { UseLibRaw = true, UseHighPrecisionRawPipeline = true };
            var exif = ExifReader.Read(imagePath);
            loader.EnrichCameraColor(imagePath, exif);
            var cam = exif.Camera;
            Line($"相機色彩資料: {(cam is { IsValid: true } ? "有（矩陣白平衡）" : "無（黑體近似）")}");

            using var proxy = loader.LoadProxyFloat(imagePath)
                ?? throw new InvalidOperationException("無法載入 proxy");
            Line($"proxy   : {proxy.Width} x {proxy.Height}");
            Line("");

            foreach (var (name, adj) in Cases())
            {
                Line($"[{name}]");
                Compare(Line, proxy, adj, cam);
                Line("");
            }

            // 全解析度：只跑「綜合」一組，看匯出等級的加速比
            FloatImageBuffer? full = null;
            try { full = loader.DecodeFullFloat(imagePath); } catch (Exception ex) { Line("全解析度解碼失敗: " + ex.Message); }
            if (full is not null)
            {
                using (full)
                {
                    Line($"[全解析度 綜合] {full.Width} x {full.Height}");
                    Compare(Line, full, Combined(), cam, repeats: 1);
                }
            }
        }
        catch (Exception ex)
        {
            Line("");
            Line("!! 例外: " + ex);
        }
        finally
        {
            File.WriteAllText(reportPath, sb.ToString(), Encoding.UTF8);
        }
    }

    private static void Compare(Action<string> line, FloatImageBuffer src, ImageAdjustments adj, CameraColorInfo? cam, int repeats = 3)
    {
        ProcessContext Ctx(bool cpu) => new() { Camera = cam, WhiteBalanceReference = WhiteBalanceReference.Decode, ForceCpu = cpu };

        var sw = Stopwatch.StartNew();
        using var cpuOut = ImageProcessor.ApplyToFloat(src, adj, Ctx(true));
        long cpuMs = sw.ElapsedMilliseconds;

        // 第一次含 shader/pipeline 建立，另外計；之後取最短
        var ctxFirst = Ctx(false);
        sw.Restart();
        using var gpuOut = ImageProcessor.ApplyToFloat(src, adj, ctxFirst);
        long gpuFirstMs = sw.ElapsedMilliseconds;
        long gpuBest = gpuFirstMs;
        for (int i = 1; i < repeats; i++)
        {
            sw.Restart();
            using var again = ImageProcessor.ApplyToFloat(src, adj, Ctx(false));
            gpuBest = Math.Min(gpuBest, sw.ElapsedMilliseconds);
        }

        line($"  用到 GPU : {ctxFirst.UsedGpu}（常駐: {ctxFirst.GpuResident}）{(GpuPipeline.LastError is null ? "" : "   (last error: " + GpuPipeline.LastError + ")")}");
        line($"  尺寸     : CPU {cpuOut.Width}x{cpuOut.Height} / GPU {gpuOut.Width}x{gpuOut.Height}");
        line($"  耗時     : CPU {cpuMs} ms / GPU 首次 {gpuFirstMs} ms / GPU 最快 {gpuBest} ms  → {(gpuBest > 0 ? (cpuMs / (double)gpuBest).ToString("0.0") : "∞")}x");
        if (cpuOut.Width != gpuOut.Width || cpuOut.Height != gpuOut.Height) { line("  !! 尺寸不同"); return; }

        var a = cpuOut.Data; var b = gpuOut.Data;
        double sum = 0; float max = 0; long over = 0, bytesDiff = 0, bytesDiff2 = 0;
        int n = cpuOut.Width * cpuOut.Height;
        for (int i = 0; i < a.Length; i++)
        {
            if ((i & 3) == 3) continue; // alpha
            float ca = Math.Clamp(a[i], 0f, 1f), cb = Math.Clamp(b[i], 0f, 1f);
            float d = Math.Abs(ca - cb);
            sum += d; if (d > max) max = d;
            if (d > 1f / 255f) over++;
            int ia = (int)(ca * 255f + 0.5f), ib = (int)(cb * 255f + 0.5f);
            int bd = Math.Abs(ia - ib);
            if (bd >= 1) bytesDiff++;
            if (bd >= 2) bytesDiff2++;
        }
        long channels = (long)n * 3;
        line($"  最大差   : {max:0.000000}   平均差: {sum / channels:0.00000000}");
        line($"  >1/255   : {over} / {channels} ({100.0 * over / channels:0.0000}%)");
        line($"  8-bit 差 : ≥1: {bytesDiff} ({100.0 * bytesDiff / channels:0.0000}%)   ≥2: {bytesDiff2}");
    }

    private static IEnumerable<(string, ImageAdjustments)> Cases()
    {
        yield return ("v1 預設（LUT 恆等 + 白平衡中性）", new ImageAdjustments());

        var legacy = new ImageAdjustments { Exposure = 0.6, Temperature = 4200, Tint = 15, Contrast = 20 };
        legacy.PipelineVersion = 0;
        yield return ("舊版 曝光/色溫/色調/對比", legacy);

        yield return ("v1 曝光/色溫/色調", new ImageAdjustments { Exposure = 1.2, Temperature = 3600, Tint = -20 });
        yield return ("v1 色調曲線全開", new ImageAdjustments { Contrast = 35, Highlights = -40, Shadows = 30, Whites = 15, Blacks = -20 });
        yield return ("鮮豔度/飽和度", new ImageAdjustments { Vibrance = 40, Saturation = -25 });
        yield return ("降噪 + 銳利化", new ImageAdjustments { NoiseReduction = 60, Sharpening = 70 });
        yield return ("柔化（負銳利度）", new ImageAdjustments { Sharpening = -50 });
        yield return ("漸層 ×2", WithGradients(new ImageAdjustments()));
        yield return ("漸層 + 銳利化（漸層獨立 pass）", WithGradients(new ImageAdjustments { Sharpening = 30 }));
        yield return ("暗角", new ImageAdjustments { Vignette = 60 });
        yield return ("裁切 + 角度", new ImageAdjustments { CropX = 0.1, CropY = 0.15, CropWidth = 0.7, CropHeight = 0.6, CropAngle = 7 });
        yield return ("廣角變形", new ImageAdjustments { Distortion = 40 });
        yield return ("旋轉 90 + 裁切", new ImageAdjustments { Rotation = Rotation.R90, CropX = 0.05, CropY = 0.05, CropWidth = 0.9, CropHeight = 0.8 });
        yield return ("綜合", Combined());
    }

    private static ImageAdjustments Combined() => WithGradients(new ImageAdjustments
    {
        Exposure = 0.4, Temperature = 6200, Tint = 8, Contrast = 15, Highlights = -25, Shadows = 20,
        Vibrance = 20, Saturation = 5, NoiseReduction = 30, Sharpening = 40, Vignette = 35,
        CropX = 0.05, CropY = 0.05, CropWidth = 0.85, CropHeight = 0.85, CropAngle = -3, Distortion = -20
    });

    private static ImageAdjustments WithGradients(ImageAdjustments a)
    {
        a.Gradients.Add(new LinearGradient { CenterX = 0.5, CenterY = 0.3, Angle = 0, Range = 0.25, Exposure = -0.8, Contrast = 10, Saturation = -20 });
        a.Gradients.Add(new LinearGradient { CenterX = 0.4, CenterY = 0.7, Angle = 35, Range = 0.2, Exposure = 0.5, Highlights = -30, Shadows = 20 });
        return a;
    }
}
