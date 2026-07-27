using System.Drawing;
using System.Drawing.Imaging;

namespace AwayPhotoRawEditor.Imaging;

/// <summary>RGB histogram (256 bins per channel).</summary>
public sealed class Histogram
{
    public int[] R { get; } = new int[256];
    public int[] G { get; } = new int[256];
    public int[] B { get; } = new int[256];
    public int Max { get; set; }
}

public static class ImageStats
{
    /// <summary>Compute a per-channel 256-bin histogram from a bitmap.</summary>
    public static unsafe Histogram ComputeHistogram(Bitmap bmp)
    {
        var h = new Histogram();
        var rect = new Rectangle(0, 0, bmp.Width, bmp.Height);
        var bd = bmp.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        try
        {
            for (int y = 0; y < bd.Height; y++)
            {
                byte* row = (byte*)bd.Scan0 + (long)y * bd.Stride;
                for (int x = 0; x < bd.Width; x++)
                {
                    h.B[row[x * 4 + 0]]++;
                    h.G[row[x * 4 + 1]]++;
                    h.R[row[x * 4 + 2]]++;
                }
            }
        }
        finally { bmp.UnlockBits(bd); }

        int max = 0;
        for (int i = 1; i < 255; i++) // ignore pure 0/255 spikes when scaling
        {
            if (h.R[i] > max) max = h.R[i];
            if (h.G[i] > max) max = h.G[i];
            if (h.B[i] > max) max = h.B[i];
        }
        h.Max = max <= 0 ? 1 : max;
        return h;
    }

    /// <summary>Average colour of the whole bitmap.</summary>
    public static unsafe (double r, double g, double b) MeanColor(Bitmap bmp)
    {
        double sr = 0, sg = 0, sb = 0;
        long n = 0;
        var rect = new Rectangle(0, 0, bmp.Width, bmp.Height);
        var bd = bmp.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        try
        {
            for (int y = 0; y < bd.Height; y++)
            {
                byte* row = (byte*)bd.Scan0 + (long)y * bd.Stride;
                for (int x = 0; x < bd.Width; x++)
                {
                    sb += row[x * 4 + 0];
                    sg += row[x * 4 + 1];
                    sr += row[x * 4 + 2];
                    n++;
                }
            }
        }
        finally { bmp.UnlockBits(bd); }
        if (n == 0) return (0, 0, 0);
        return (sr / n, sg / n, sb / n);
    }
}
