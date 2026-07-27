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

    // ---- Lossless float32 proxy -----------------------------------------

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
