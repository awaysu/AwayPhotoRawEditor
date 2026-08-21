using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;

namespace AwayPhotoRawEditor.Imaging;

/// <summary>
/// Reads / writes the RAW_TEMP cache artifacts: thumbnail JPEGs, 8-bit proxy PNGs
/// and a lossless float32 proxy (used by the high-precision pipeline in place of a
/// 16-bit PNG). Also hosts high-quality resize helpers.
/// </summary>
public static class CacheManager
{
    private const uint FloatMagic = 0x46504152; // "RAPF"

    public static void SaveJpeg(Bitmap bmp, string path, long quality = 88)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var enc = GetEncoder(ImageFormat.Jpeg);
        using var p = new EncoderParameters(1);
        p.Param[0] = new EncoderParameter(System.Drawing.Imaging.Encoder.Quality, quality);
        bmp.Save(path, enc, p);
    }

    public static void SavePng(Bitmap bmp, string path)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        bmp.Save(path, ImageFormat.Png);
    }

    public static Bitmap? LoadBitmap(string path)
    {
        if (!File.Exists(path)) return null;
        return WicDecoder.LoadFile(path);
    }

    // ---- 16-bit proxy (.f16) ---------------------------------------------
    // LibRaw 的高精度輸出本來就是 16-bit 整數，存 ushort 不損失任何精度、檔案是 float32 的一半
    //（2560×1707 ≈ 35MB）。舊的 .f32 其實裝的是 8-bit 量化過的值（見 CLAUDE.md「高精度管線」），
    // 換副檔名讓它自然失效，不用寫版本判斷。

    private const uint HalfMagic = 0x36315041; // "AP16"

    public static void SaveHalf(FloatImageBuffer buf, string path)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        using var fs = File.Create(path);
        using var bw = new BinaryWriter(fs);
        bw.Write(HalfMagic);
        bw.Write(buf.Width);
        bw.Write(buf.Height);
        var data = buf.Data;
        var bytes = new byte[data.Length * sizeof(ushort)];
        for (int i = 0; i < data.Length; i++)
        {
            float v = data[i];
            int q = v <= 0f ? 0 : v >= 1f ? 65535 : (int)(v * 65535f + 0.5f);
            bytes[i * 2] = (byte)q;
            bytes[i * 2 + 1] = (byte)(q >> 8);
        }
        bw.Write(bytes);
    }

    public static FloatImageBuffer? LoadHalf(string path)
    {
        if (!File.Exists(path)) return null;
        try
        {
            using var fs = File.OpenRead(path);
            using var br = new BinaryReader(fs);
            if (br.ReadUInt32() != HalfMagic) return null;
            int w = br.ReadInt32(), h = br.ReadInt32();
            if (w <= 0 || h <= 0 || (long)w * h > 64L * 1024 * 1024) return null;
            var buf = new FloatImageBuffer(w, h);
            var bytes = br.ReadBytes(buf.Data.Length * sizeof(ushort));
            if (bytes.Length != buf.Data.Length * sizeof(ushort)) return null;
            const float inv = 1f / 65535f;
            var data = buf.Data;
            for (int i = 0; i < data.Length; i++)
                data[i] = (bytes[i * 2] | (bytes[i * 2 + 1] << 8)) * inv;
            return buf;
        }
        catch { return null; }
    }

    // ---- float resize ----------------------------------------------------

    /// <summary>Downscale a float buffer so its long edge is ≤ <paramref name="maxDim"/>, by
    /// separable area averaging — stays in float the whole way (the old Bitmap round-trip
    /// quantised the "high-precision" proxy to 8 bits). Returns the input itself when no
    /// scaling is needed. Only downscales.</summary>
    public static FloatImageBuffer ResizeFloatToMaxDim(FloatImageBuffer src, int maxDim)
    {
        int longSide = Math.Max(src.Width, src.Height);
        if (longSide <= maxDim) return src;
        double scale = (double)maxDim / longSide;
        int dw = Math.Max(1, (int)Math.Round(src.Width * scale));
        int dh = Math.Max(1, (int)Math.Round(src.Height * scale));

        // horizontal pass: src (W×H) → tmp (dw×H)
        var tmp = new float[dw * src.Height * 4];
        var sd = src.Data;
        int sw = src.Width;
        System.Threading.Tasks.Parallel.For(0, src.Height, y =>
        {
            int srow = y * sw * 4, trow = y * dw * 4;
            for (int x = 0; x < dw; x++)
            {
                double x0 = x * (double)sw / dw, x1 = (x + 1) * (double)sw / dw;
                float r = 0, g = 0, b = 0, wsum = 0;
                for (int sx = (int)x0; sx < Math.Min(sw, (int)Math.Ceiling(x1)); sx++)
                {
                    float cover = (float)(Math.Min(x1, sx + 1) - Math.Max(x0, sx));
                    if (cover <= 0) continue;
                    int si = srow + sx * 4;
                    r += sd[si] * cover; g += sd[si + 1] * cover; b += sd[si + 2] * cover; wsum += cover;
                }
                int ti = trow + x * 4;
                if (wsum > 0) { tmp[ti] = r / wsum; tmp[ti + 1] = g / wsum; tmp[ti + 2] = b / wsum; }
                tmp[ti + 3] = 1f;
            }
        });

        // vertical pass: tmp (dw×H) → dst (dw×dh)
        var dst = new FloatImageBuffer(dw, dh);
        var dd = dst.Data;
        int sh = src.Height;
        System.Threading.Tasks.Parallel.For(0, dh, y =>
        {
            double y0 = y * (double)sh / dh, y1 = (y + 1) * (double)sh / dh;
            int drow = y * dw * 4;
            for (int x = 0; x < dw; x++)
            {
                float r = 0, g = 0, b = 0, wsum = 0;
                for (int sy = (int)y0; sy < Math.Min(sh, (int)Math.Ceiling(y1)); sy++)
                {
                    float cover = (float)(Math.Min(y1, sy + 1) - Math.Max(y0, sy));
                    if (cover <= 0) continue;
                    int ti = (sy * dw + x) * 4;
                    r += tmp[ti] * cover; g += tmp[ti + 1] * cover; b += tmp[ti + 2] * cover; wsum += cover;
                }
                int di = drow + x * 4;
                if (wsum > 0) { dd[di] = r / wsum; dd[di + 1] = g / wsum; dd[di + 2] = b / wsum; }
                dd[di + 3] = 1f;
            }
        });
        return dst;
    }

    // ---- Lossless float32 proxy (legacy .f32; kept so old caches still parse) ------------

    public static void SaveFloat(FloatImageBuffer buf, string path)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        using var fs = File.Create(path);
        using var bw = new BinaryWriter(fs);
        bw.Write(FloatMagic);
        bw.Write(buf.Width);
        bw.Write(buf.Height);
        var bytes = new byte[buf.Data.Length * sizeof(float)];
        Buffer.BlockCopy(buf.Data, 0, bytes, 0, bytes.Length);
        bw.Write(bytes);
    }

    public static FloatImageBuffer? LoadFloat(string path)
    {
        if (!File.Exists(path)) return null;
        try
        {
            using var fs = File.OpenRead(path);
            using var br = new BinaryReader(fs);
            if (br.ReadUInt32() != FloatMagic) return null;
            int w = br.ReadInt32(), h = br.ReadInt32();
            var buf = new FloatImageBuffer(w, h);
            var bytes = br.ReadBytes(buf.Data.Length * sizeof(float));
            if (bytes.Length != buf.Data.Length * sizeof(float)) return null;
            Buffer.BlockCopy(bytes, 0, buf.Data, 0, bytes.Length);
            return buf;
        }
        catch { return null; }
    }

    // ---- Resize helpers --------------------------------------------------

    /// <summary>Resize preserving aspect ratio so the result fits within maxW×maxH.</summary>
    public static Bitmap ResizeToFit(Bitmap src, int maxW, int maxH)
    {
        double scale = Math.Min((double)maxW / src.Width, (double)maxH / src.Height);
        if (scale >= 1.0) scale = 1.0;
        int w = Math.Max(1, (int)Math.Round(src.Width * scale));
        int h = Math.Max(1, (int)Math.Round(src.Height * scale));
        return ResizeTo(src, w, h);
    }

    /// <summary>Resize preserving aspect ratio so the longest side equals maxDim (never upsizes).</summary>
    public static Bitmap ResizeToMaxDim(Bitmap src, int maxDim)
    {
        int longSide = Math.Max(src.Width, src.Height);
        if (longSide <= maxDim) return (Bitmap)src.Clone();
        double scale = (double)maxDim / longSide;
        return ResizeTo(src, Math.Max(1, (int)Math.Round(src.Width * scale)),
                             Math.Max(1, (int)Math.Round(src.Height * scale)));
    }

    public static Bitmap ResizeTo(Bitmap src, int w, int h)
    {
        var dst = new Bitmap(w, h, PixelFormat.Format32bppArgb);
        using var g = Graphics.FromImage(dst);
        g.InterpolationMode = InterpolationMode.HighQualityBicubic;
        g.PixelOffsetMode = PixelOffsetMode.HighQuality;
        g.SmoothingMode = SmoothingMode.HighQuality;
        g.DrawImage(src, new Rectangle(0, 0, w, h));
        return dst;
    }

    private static ImageCodecInfo GetEncoder(ImageFormat format)
    {
        foreach (var c in ImageCodecInfo.GetImageEncoders())
            if (c.FormatID == format.Guid) return c;
        throw new InvalidOperationException("Encoder not found: " + format);
    }
}
