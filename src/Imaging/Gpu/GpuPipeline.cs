using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;
using AwayPhotoRawEditor.Models;
using ComputeSharp;

namespace AwayPhotoRawEditor.Imaging.Gpu;

/// <summary>逐像素階段的參數（由 ImageProcessor 依 ImageAdjustments / ProcessContext 組出來）。</summary>
internal sealed class PixelStageParams
{
    public int Flags;
    public float[]? ToneLut;
    public GradientGpu[] Gradients = Array.Empty<GradientGpu>();
    public float3 WbMul = new(1f, 1f, 1f);
    public float3 M0, M1, M2;
    public float Sat, Vib;
    public float VigAmount, VigCx, VigCy, VigInvMax;
}

/// <summary>一個「方框模糊 + 收尾」運算：Mode 0 = Blend(amount)、1 = Unsharp(amount)。</summary>
internal readonly record struct BlurOp(int Radius, int Mode, float Amount);

/// <summary>幾何重取樣參數：Mode 0 = 徑向變形(K)；Mode 1 = 繞 (Cx,Cy) 旋轉、輸出座標先減 (Ox,Oy)。
/// 用 double 是為了讓 CPU 參考實作的算式和改版前逐 bit 相同；送進 shader 時才轉 float。</summary>
internal readonly record struct ResampleParams(int Mode, double K, double Cx, double Cy, double SinA, double CosA, double Ox, double Oy);

/// <summary>整張常駐在 GPU 上的影像（快速路徑）。尺寸會隨幾何階段改變。</summary>
internal sealed class ResidentImage : IDisposable
{
    internal GraphicsDevice Device { get; }
    internal ReadWriteBuffer<float4> Cur { get; set; }
    public int Width { get; internal set; }
    public int Height { get; internal set; }

    internal ResidentImage(GraphicsDevice device, ReadWriteBuffer<float4> cur, int width, int height)
    {
        Device = device; Cur = cur; Width = width; Height = height;
    }

    public void Dispose() => Cur.Dispose();
}

/// <summary>
/// ImageProcessor 的 GPU 後端（ComputeSharp / Direct3D 12）。
/// <para>
/// 兩條路徑：
/// <list type="bullet">
/// <item><b>常駐（resident）</b>——影像放得進 <see cref="ResidentCapBytes"/> 時，整張上傳一次、
/// 所有階段都在顯示卡上接力、最後下載一次。主預覽的 proxy（約 4 百萬像素）永遠走這條。</item>
/// <item><b>分段（banded）</b>——把影像切成「列段」（每段 ≤ <see cref="MaxBandPixels"/> 像素 ≈ 32 MB）
/// 逐段上傳→運算→下載。任何尺寸、任何顯示卡都能跑；逐像素階段就地寫回，失敗時回傳「已完成的列數」
/// 讓呼叫端用 CPU 補完剩下的列；模糊／幾何階段寫進新 buffer，失敗時原 buffer 原封不動。</item>
/// </list>
/// 常駐路徑失敗 → 降低上限、改走分段；分段路徑連續失敗 3 次或裝置遺失 → 整個程式只走 CPU。
/// </para>
/// <para>
/// 裝置選擇：只用硬體加速的 D3D12 裝置（不用 WARP 軟體算圖——那比多執行緒 CPU 還慢），
/// 多顆時挑專用記憶體最大的（筆電 內顯+獨顯 時選獨顯）。
/// </para>
/// </summary>
public static class GpuPipeline
{
    /// <summary>使用者設定（AppSettings.UseGpu）。關閉時所有 Try* 立刻回傳「沒做」。</summary>
    public static bool Enabled { get; set; } = true;

    /// <summary>分段路徑每段最多處理的像素數（float4 → 32 MB）。</summary>
    private const int MaxBandPixels = 2_000_000;

    private const int MaxConsecutiveFailures = 3;

    private static readonly object Lock = new();
    private static GraphicsDevice? _device;
    private static bool _initTried;
    private static volatile bool _broken;
    private static int _failures;
    private static long _residentCap;
    private static ReadOnlyBuffer<float>? _decodeLut;
    private static ReadOnlyBuffer<float>? _encodeLut;
    private static readonly float[] OneFloat = new float[1];
    private static readonly GradientGpu[] OneGradient = new GradientGpu[1];

    /// <summary>選到的裝置名稱（例如 "Intel(R) Iris(R) Xe Graphics"），沒有時為空字串。</summary>
    public static string DeviceName { get; private set; } = "";

    /// <summary>最近一次初始化／運算失敗的訊息（診斷用）。</summary>
    public static string? LastError { get; private set; }

    /// <summary>ComputeSharp 套件版本（關於視窗的第三方元件列）。</summary>
    public static string LibraryVersion => typeof(GraphicsDevice).Assembly.GetName().Version?.ToString(3) ?? "";

    /// <summary>常駐路徑允許的單張影像大小（float4 位元組數）。依裝置記憶體推算，
    /// 配置失敗時會自動下修。</summary>
    public static long ResidentCapBytes => _residentCap;

    /// <summary>true = 設定開啟、裝置存在、尚未判定故障。</summary>
    public static bool IsAvailable => Enabled && !_broken && EnsureDevice() is not null;

    /// <summary>供 UI 顯示用的狀態摘要。</summary>
    public static string StatusText
    {
        get
        {
            if (!Enabled) return "GPU: off";
            var dev = EnsureDevice();
            if (dev is null || _broken) return "GPU: unavailable" + (LastError is null ? "" : " (" + LastError + ")");
            return "GPU: " + DeviceName + $" (resident cap {_residentCap >> 20} MB)";
        }
    }

    // ---- device -----------------------------------------------------------

    private static GraphicsDevice? EnsureDevice()
    {
        if (_initTried) return _device;
        lock (Lock)
        {
            if (_initTried) return _device;
            _initTried = true;
            try
            {
                var candidates = new List<GraphicsDevice>();
                foreach (var d in GraphicsDevice.QueryDevices(info => info.IsHardwareAccelerated))
                    candidates.Add(d);
                if (candidates.Count == 0)
                {
                    LastError = "no hardware-accelerated Direct3D 12 device";
                    Diagnostics.Trace.Log("[gpu] " + LastError);
                    return null;
                }
                var best = candidates.OrderByDescending(d => d.DedicatedMemorySize).ThenByDescending(d => d.SharedMemorySize).First();
                foreach (var d in candidates) if (!ReferenceEquals(d, best)) d.Dispose();

                // 常駐上限：同時會有最多三份整圖（目前／輸出／模糊暫存）。獨顯看專用記憶體，
                // 內顯（專用記憶體極小、跟系統共用）看共享記憶體；單一資源另外上限 2 GB。
                long dedicated = (long)best.DedicatedMemorySize, shared = (long)best.SharedMemorySize;
                long budget = dedicated >= (1L << 30) ? dedicated / 2 : shared / 4;
                _residentCap = Math.Clamp(budget / 3, 0, 2L << 30);

                // 先把兩張轉換曲線查表放上去（整個程序共用；ColorScience 的表是 static readonly）
                _decodeLut = best.AllocateReadOnlyBuffer(ColorScience.DecodeTable);
                _encodeLut = best.AllocateReadOnlyBuffer(ColorScience.EncodeTable);
                best.DeviceLost += (_, reason) =>
                {
                    _broken = true;
                    LastError = "device lost: " + reason;
                    Diagnostics.Trace.Log("[gpu] " + LastError);
                };
                _device = best;
                DeviceName = best.Name;
                Diagnostics.Trace.Log($"[gpu] using {best.Name} (dedicated {dedicated >> 20} MB, shared {shared >> 20} MB, resident cap {_residentCap >> 20} MB)");
                return best;
            }
            catch (Exception ex)
            {
                LastError = ex.Message;
                Diagnostics.Trace.Log("[gpu] init failed: " + ex);
                _device = null;
                return null;
            }
        }
    }

    private static GraphicsDevice? Ready()
    {
        if (!Enabled || _broken) return null;
        return EnsureDevice();
    }

    private static void Fail(Exception ex, string stage)
    {
        LastError = stage + ": " + ex.Message;
        Diagnostics.Trace.Log("[gpu] " + stage + " failed: " + ex);
        if (Interlocked.Increment(ref _failures) >= MaxConsecutiveFailures) _broken = true;
    }

    private static void Succeeded() => Interlocked.Exchange(ref _failures, 0);

    private static Span<float4> AsFloat4(FloatImageBuffer b) => MemoryMarshal.Cast<float, float4>(b.Data.AsSpan());

    private static PixelStageShader MakePixelShader(ReadWriteBuffer<float4> pixels, ReadOnlyBuffer<float> tone,
        ReadOnlyBuffer<GradientGpu> grads, PixelStageParams p, int W, int H, int rowOffset)
        => new(pixels, tone, _decodeLut!, _encodeLut!, grads,
               W, H, rowOffset, p.Flags, p.Gradients.Length,
               ToneCurve.LutSize, ColorScience.DecodeLutSize, ColorScience.EncodeLutSize,
               p.WbMul, p.M0, p.M1, p.M2, p.Sat, p.Vib,
               p.VigAmount, p.VigCx, p.VigCy, p.VigInvMax);

    private static ResampleShader MakeResampleShader(ReadWriteBuffer<float4> src, ReadWriteBuffer<float4> dst,
        int srcW, int srcH, int dstW, int rowOffset, ResampleParams p)
        => new(src, dst, srcW, srcH, dstW, rowOffset, p.Mode, (float)p.K, (float)p.Cx, (float)p.Cy,
               (float)p.SinA, (float)p.CosA, (float)p.Ox, (float)p.Oy);

    // =====================================================================
    // Resident path — whole image on the GPU; methods THROW on failure, the
    // caller (ImageProcessor.ApplyToFloat) falls back to the banded path.
    // =====================================================================

    /// <summary>影像放不放得進常駐路徑（裝置可用且 W×H×16 ≤ 上限）。</summary>
    public static bool CanHostWhole(int width, int height)
        => Ready() is not null && (long)width * height * 16 <= _residentCap;

    internal static ResidentImage? CreateResident(FloatImageBuffer src)
    {
        var dev = Ready();
        if (dev is null) return null;
        if ((long)src.Width * src.Height * 16 > _residentCap) return null;
        var buf = dev.AllocateReadWriteBuffer<float4>(AsFloat4(src));
        return new ResidentImage(dev, buf, src.Width, src.Height);
    }

    internal static void PixelResident(ResidentImage img, PixelStageParams p)
    {
        using var tone = img.Device.AllocateReadOnlyBuffer(p.ToneLut ?? OneFloat);
        using var grads = img.Device.AllocateReadOnlyBuffer(p.Gradients.Length > 0 ? p.Gradients : OneGradient);
        img.Device.For(img.Width, img.Height, MakePixelShader(img.Cur, tone, grads, p, img.Width, img.Height, 0));
    }

    internal static void BlurResident(ResidentImage img, BlurOp? nr, BlurOp? sh)
    {
        if (nr is null && sh is null) return;
        int W = img.Width, H = img.Height;
        var dev = img.Device;
        using var tmp = dev.AllocateReadWriteBuffer<float4>(W * H);
        var a = img.Cur;
        var b = dev.AllocateReadWriteBuffer<float4>(W * H);
        try
        {
            foreach (var op in new[] { nr, sh })
            {
                if (op is not { } o) continue;
                float norm = 1f / (o.Radius * 2 + 1);
                dev.For(W, H, new BlurHShader(a, tmp, W, o.Radius, norm));
                dev.For(W, H, new BlurVFinishShader(tmp, a, b, W, H, 0, o.Radius, norm, o.Mode, o.Amount));
                (a, b) = (b, a);
            }
            img.Cur = a;
        }
        finally { b.Dispose(); }   // 'b' is whichever buffer ended up unused（swap 後可能是原來的 Cur）
    }

    internal static void ResampleResident(ResidentImage img, ResampleParams p, int outW, int outH)
    {
        var outBuf = img.Device.AllocateReadWriteBuffer<float4>(outW * outH);
        try
        {
            img.Device.For(outW, outH, MakeResampleShader(img.Cur, outBuf, img.Width, img.Height, outW, 0, p));
        }
        catch { outBuf.Dispose(); throw; }
        img.Cur.Dispose();
        img.Cur = outBuf; img.Width = outW; img.Height = outH;
    }

    internal static void RotateResident(ResidentImage img, Rotation rot)
    {
        if (rot == Rotation.R0) return;
        int W = img.Width, H = img.Height;
        bool swap = rot is Rotation.R90 or Rotation.R270;
        int dstW = swap ? H : W, dstH = swap ? W : H;
        var outBuf = img.Device.AllocateReadWriteBuffer<float4>(W * H);
        try
        {
            img.Device.For(W, H, new RotateShader(img.Cur, outBuf, W, H, dstW, rot == Rotation.R90 ? 1 : rot == Rotation.R180 ? 2 : 3));
        }
        catch { outBuf.Dispose(); throw; }
        img.Cur.Dispose();
        img.Cur = outBuf; img.Width = dstW; img.Height = dstH;
    }

    internal static FloatImageBuffer DownloadResident(ResidentImage img)
    {
        var b = new FloatImageBuffer(img.Width, img.Height);
        img.Cur.CopyTo(AsFloat4(b), 0);
        return b;
    }

    internal static void UploadResident(ResidentImage img, FloatImageBuffer b)
    {
        if (b.Width != img.Width || b.Height != img.Height) throw new ArgumentException("size mismatch");
        img.Cur.CopyFrom(AsFloat4(b), 0);
    }

    /// <summary>常駐路徑失敗：把上限降到這張圖以下（之後同尺寸直接走分段），小圖失敗才算一次故障。</summary>
    internal static void ReportResidentFailure(Exception ex, long imageBytes)
    {
        LastError = "resident: " + ex.Message;
        Diagnostics.Trace.Log($"[gpu] resident path failed for {imageBytes >> 20} MB: {ex}");
        lock (Lock) _residentCap = Math.Min(_residentCap, Math.Max(0, imageBytes - 1));
        if (imageBytes <= (64L << 20)) Fail(ex, "resident (small image)");
    }

    // =====================================================================
    // Banded path — image stays in CPU memory, streamed in row bands.
    // =====================================================================

    /// <summary>
    /// 對 <paramref name="buf"/> 就地執行逐像素階段。回傳已完成的列數：0 = GPU 不可用（什麼都沒動）、
    /// 小於 Height = 中途失敗（前面的列已完成，呼叫端從該列起用 CPU 補完）。
    /// </summary>
    internal static int TryPixelStage(FloatImageBuffer buf, PixelStageParams p, CancellationToken t)
    {
        var dev = Ready();
        if (dev is null) return 0;
        int W = buf.Width, H = buf.Height;
        int rowsPerBand = Math.Clamp(MaxBandPixels / W, 1, H);
        int done = 0;
        try
        {
            using var pixels = dev.AllocateReadWriteBuffer<float4>(rowsPerBand * W);
            using var tone = dev.AllocateReadOnlyBuffer(p.ToneLut ?? OneFloat);
            using var grads = dev.AllocateReadOnlyBuffer(p.Gradients.Length > 0 ? p.Gradients : OneGradient);
            var all = AsFloat4(buf);
            for (int y0 = 0; y0 < H; y0 += rowsPerBand)
            {
                t.ThrowIfCancellationRequested();
                int rows = Math.Min(rowsPerBand, H - y0);
                var band = all.Slice(y0 * W, rows * W);
                pixels.CopyFrom(band, 0);
                dev.For(W, rows, MakePixelShader(pixels, tone, grads, p, W, H, y0));
                pixels.CopyTo(band, 0);
                done = y0 + rows;
            }
            Succeeded();
            return done;
        }
        catch (OperationCanceledException) { throw; }
        catch (Exception ex) { Fail(ex, "pixel stage"); return done; }
    }

    /// <summary>
    /// 降噪(<paramref name="nr"/>) 接 銳利化/柔化(<paramref name="sh"/>)，兩者皆可為 null（但至少一個）。
    /// 每段帶上 halo（兩個半徑之和）一起上傳，段內的夾值只在影像邊緣才會真的發生，結果與全圖運算相同。
    /// 回傳新 buffer；null = GPU 不可用或失敗（來源未被更動）。
    /// </summary>
    internal static FloatImageBuffer? TryBlurStage(FloatImageBuffer src, BlurOp? nr, BlurOp? sh, CancellationToken t)
    {
        var dev = Ready();
        if (dev is null) return null;
        if (nr is null && sh is null) return null;
        int W = src.Width, H = src.Height;
        int rNr = nr?.Radius ?? 0, rSh = sh?.Radius ?? 0, R = rNr + rSh;
        int rowsPerBand = Math.Clamp(MaxBandPixels / W, 1, H);
        int maxRegion = Math.Min(H, rowsPerBand + 2 * R);
        FloatImageBuffer? dst = null;
        try
        {
            dst = new FloatImageBuffer(W, H);
            using var bufA = dev.AllocateReadWriteBuffer<float4>(maxRegion * W);
            using var tmp = dev.AllocateReadWriteBuffer<float4>(maxRegion * W);
            using var bufB = nr is null ? null : dev.AllocateReadWriteBuffer<float4>(maxRegion * W);
            using var outBuf = sh is null ? null : dev.AllocateReadWriteBuffer<float4>(rowsPerBand * W);
            var srcAll = AsFloat4(src);
            var dstAll = AsFloat4(dst);

            for (int y0 = 0; y0 < H; y0 += rowsPerBand)
            {
                t.ThrowIfCancellationRequested();
                int rows = Math.Min(rowsPerBand, H - y0);
                int r0 = Math.Max(0, y0 - R), r1 = Math.Min(H, y0 + rows + R);
                int regionRows = r1 - r0, rowStart = y0 - r0;
                bufA.CopyFrom(srcAll.Slice(r0 * W, regionRows * W), 0);

                ReadWriteBuffer<float4> cur = bufA;
                if (nr is { } n)
                {
                    float norm = 1f / (n.Radius * 2 + 1);
                    dev.For(W, regionRows, new BlurHShader(cur, tmp, W, n.Radius, norm));
                    dev.For(W, regionRows, new BlurVFinishShader(tmp, cur, bufB!, W, regionRows, 0, n.Radius, norm, n.Mode, n.Amount));
                    cur = bufB!;
                }
                if (sh is { } s)
                {
                    float norm = 1f / (s.Radius * 2 + 1);
                    dev.For(W, regionRows, new BlurHShader(cur, tmp, W, s.Radius, norm));
                    dev.For(W, rows, new BlurVFinishShader(tmp, cur, outBuf!, W, regionRows, rowStart, s.Radius, norm, s.Mode, s.Amount));
                    outBuf!.CopyTo(dstAll.Slice(y0 * W, rows * W), 0);
                }
                else
                {
                    cur.CopyTo(dstAll.Slice(y0 * W, rows * W), rowStart * W);
                }
            }
            Succeeded();
            return dst;
        }
        catch (OperationCanceledException) { dst?.Dispose(); throw; }
        catch (Exception ex) { dst?.Dispose(); Fail(ex, "blur stage"); return null; }
    }

    /// <summary>反向映射重取樣到 <paramref name="outW"/>×<paramref name="outH"/>。來源整張上傳（超過常駐
    /// 上限直接回 null 交給 CPU），輸出分段下載。null = 未執行／失敗。</summary>
    internal static FloatImageBuffer? TryResample(FloatImageBuffer src, ResampleParams p, int outW, int outH, CancellationToken t)
    {
        var dev = Ready();
        if (dev is null) return null;
        if ((long)src.Width * src.Height * 16 > _residentCap) return null;
        FloatImageBuffer? dst = null;
        try
        {
            using var srcBuf = dev.AllocateReadWriteBuffer<float4>(AsFloat4(src));
            int rowsPerBand = Math.Clamp(MaxBandPixels / outW, 1, outH);
            using var outBuf = dev.AllocateReadWriteBuffer<float4>(rowsPerBand * outW);
            dst = new FloatImageBuffer(outW, outH);
            var dstAll = AsFloat4(dst);
            for (int y0 = 0; y0 < outH; y0 += rowsPerBand)
            {
                t.ThrowIfCancellationRequested();
                int rows = Math.Min(rowsPerBand, outH - y0);
                dev.For(outW, rows, MakeResampleShader(srcBuf, outBuf, src.Width, src.Height, outW, y0, p));
                outBuf.CopyTo(dstAll.Slice(y0 * outW, rows * outW), 0);
            }
            Succeeded();
            return dst;
        }
        catch (OperationCanceledException) { dst?.Dispose(); throw; }
        catch (Exception ex) { dst?.Dispose(); Fail(ex, "resample"); return null; }
    }
}
