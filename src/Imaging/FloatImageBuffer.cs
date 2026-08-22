using System.Drawing;
using System.Drawing.Imaging;
using System.Threading.Tasks;

namespace AwayPhotoRawEditor.Imaging;

/// <summary>
/// High-precision RGBA image buffer (float per channel, nominally 0..1 but may
/// exceed during processing). Backs the high-precision RAW pipeline and the
/// 16-bit proxy cache. Channel order in <see cref="Data"/> is R, G, B, A.
/// </summary>
public sealed class FloatImageBuffer : IDisposable
{
    public int Width { get; }
    public int Height { get; }

    /// <summary>Length = Width * Height * 4, laid out row-major as R,G,B,A.</summary>
    public float[] Data { get; private set; }

    /// <summary>Buffers are plain managed arrays; Dispose just drops the reference to help GC.</summary>
    public void Dispose() => Data = System.Array.Empty<float>();

    public FloatImageBuffer(int width, int height)
    {
        if (width <= 0 || height <= 0) throw new ArgumentException("Invalid image size");
        long len = (long)width * height * 4;
        if (len > int.MaxValue) throw new ArgumentException("Image too large for a single buffer");
        Width = width;
        Height = height;
        Data = new float[len];
    }

    public int Index(int x, int y) => (y * Width + x) * 4;

    public FloatImageBuffer Clone()
    {
        var c = new FloatImageBuffer(Width, Height);
        Array.Copy(Data, c.Data, Data.Length);
        return c;
    }

    /// <summary>Convert an 8-bit bitmap into a normalized float buffer.</summary>
    public static unsafe FloatImageBuffer FromBitmap(Bitmap bmp)
    {
        int w = bmp.Width, h = bmp.Height;
        var buf = new FloatImageBuffer(w, h);
        var rect = new Rectangle(0, 0, w, h);
        var bd = bmp.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        try
        {
            const float inv = 1f / 255f;
            nint scan0 = bd.Scan0; int stride = bd.Stride;
            var data = buf.Data;
            // 逐列平行：全解析度匯出時這個轉換單執行緒要幾百毫秒
            Parallel.For(0, h, y =>
            {
                byte* row = (byte*)scan0 + (long)y * stride;
                int o = y * w * 4;
                for (int x = 0; x < w; x++, o += 4)
                {
                    // 32bppArgb in memory is B,G,R,A
                    data[o + 0] = row[x * 4 + 2] * inv;
                    data[o + 1] = row[x * 4 + 1] * inv;
                    data[o + 2] = row[x * 4 + 0] * inv;
                    data[o + 3] = row[x * 4 + 3] * inv;
                }
            });
        }
        finally { bmp.UnlockBits(bd); }
        return buf;
    }

    /// <summary>Materialize this buffer into a 32bpp bitmap (clamped to 0..1).</summary>
    public unsafe Bitmap ToBitmap()
    {
        var bmp = new Bitmap(Width, Height, PixelFormat.Format32bppArgb);
        var rect = new Rectangle(0, 0, Width, Height);
        var bd = bmp.LockBits(rect, ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
        try
        {
            nint scan0 = bd.Scan0; int stride = bd.Stride;
            int w = Width; var data = Data;
            Parallel.For(0, Height, y =>
            {
                byte* row = (byte*)scan0 + (long)y * stride;
                int o = y * w * 4;
                for (int x = 0; x < w; x++, o += 4)
                {
                    row[x * 4 + 0] = ToByte(data[o + 2]); // B
                    row[x * 4 + 1] = ToByte(data[o + 1]); // G
                    row[x * 4 + 2] = ToByte(data[o + 0]); // R
                    row[x * 4 + 3] = ToByte(data[o + 3]); // A
                }
            });
        }
        finally { bmp.UnlockBits(bd); }
        return bmp;
    }

    private static byte ToByte(float v)
    {
        int i = (int)(v * 255f + 0.5f);
        return (byte)(i < 0 ? 0 : i > 255 ? 255 : i);
    }
}
