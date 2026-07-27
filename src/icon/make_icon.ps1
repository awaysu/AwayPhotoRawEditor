param(
    [string]$SrcPng  = "$PSScriptRoot\icon.png",
    [string]$OutIco  = "$PSScriptRoot\icon.ico",
    [string]$Preview = "$env:TEMP\icon_alpha_256.png"
)

Add-Type -AssemblyName System.Drawing

$code = @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;

public static class IconMaker
{
    // Flood-fill near-white background from the borders -> transparent,
    // then feather light pixels adjacent to the removed region.
    public static Bitmap CutOut(Bitmap src)
    {
        int w = src.Width, h = src.Height;
        var bmp = new Bitmap(w, h, PixelFormat.Format32bppArgb);
        using (var g = Graphics.FromImage(bmp)) g.DrawImage(src, 0, 0, w, h);

        var px = new Color[w, h];
        for (int y = 0; y < h; y++)
            for (int x = 0; x < w; x++)
                px[x, y] = bmp.GetPixel(x, y);

        bool[,] bg = new bool[w, h];
        var q = new Queue<Point>();
        Action<int, int> push = (x, y) =>
        {
            if (x < 0 || y < 0 || x >= w || y >= h || bg[x, y]) return;
            var c = px[x, y];
            if (c.A < 16 || (c.R >= 240 && c.G >= 240 && c.B >= 240))
            { bg[x, y] = true; q.Enqueue(new Point(x, y)); }
        };
        for (int x = 0; x < w; x++) { push(x, 0); push(x, h - 1); }
        for (int y = 0; y < h; y++) { push(0, y); push(w - 1, y); }
        while (q.Count > 0)
        {
            var p = q.Dequeue();
            push(p.X - 1, p.Y); push(p.X + 1, p.Y); push(p.X, p.Y - 1); push(p.X, p.Y + 1);
        }

        for (int y = 0; y < h; y++)
            for (int x = 0; x < w; x++)
            {
                if (bg[x, y]) { bmp.SetPixel(x, y, Color.Transparent); continue; }
                // feather: light pixel touching removed background -> partial alpha
                bool edge = (x > 0 && bg[x - 1, y]) || (x < w - 1 && bg[x + 1, y]) ||
                            (y > 0 && bg[x, y - 1]) || (y < h - 1 && bg[x, y + 1]);
                if (!edge) continue;
                var c = px[x, y];
                int mn = Math.Min(c.R, Math.Min(c.G, c.B));
                if (mn >= 200)
                {
                    int a = (int)Math.Round((255 - mn) * 255.0 / 55.0);
                    bmp.SetPixel(x, y, Color.FromArgb(Math.Max(0, Math.Min(255, a)), c.R, c.G, c.B));
                }
            }
        return bmp;
    }

    // Trim to the opaque bounding box (+small margin) and pad to a square canvas.
    public static Bitmap TrimSquare(Bitmap src)
    {
        int w = src.Width, h = src.Height;
        int minX = w, minY = h, maxX = -1, maxY = -1;
        for (int y = 0; y < h; y++)
            for (int x = 0; x < w; x++)
                if (src.GetPixel(x, y).A > 8)
                {
                    if (x < minX) minX = x; if (x > maxX) maxX = x;
                    if (y < minY) minY = y; if (y > maxY) maxY = y;
                }
        if (maxX < 0) return new Bitmap(src);
        int bw = maxX - minX + 1, bh = maxY - minY + 1;
        int side = (int)(Math.Max(bw, bh) * 1.06);       // ~3% margin per side
        var sq = new Bitmap(side, side, PixelFormat.Format32bppArgb);
        using (var g = Graphics.FromImage(sq))
        {
            g.InterpolationMode = InterpolationMode.HighQualityBicubic;
            g.DrawImage(src, (side - bw) / 2, (side - bh) / 2,
                        new Rectangle(minX, minY, bw, bh), GraphicsUnit.Pixel);
        }
        return sq;
    }

    public static byte[] RenderPng(Bitmap src, int size)
    {
        using (var bmp = new Bitmap(size, size, PixelFormat.Format32bppArgb))
        {
            using (var g = Graphics.FromImage(bmp))
            {
                g.InterpolationMode = InterpolationMode.HighQualityBicubic;
                g.SmoothingMode = SmoothingMode.HighQuality;
                g.PixelOffsetMode = PixelOffsetMode.HighQuality;
                g.CompositingQuality = CompositingQuality.HighQuality;
                g.DrawImage(src, new Rectangle(0, 0, size, size),
                            new Rectangle(0, 0, src.Width, src.Height), GraphicsUnit.Pixel);
            }
            using (var ms = new MemoryStream()) { bmp.Save(ms, ImageFormat.Png); return ms.ToArray(); }
        }
    }

    // ICO container with PNG-encoded entries (supported since Vista).
    public static void WriteIco(string path, int[] sizes, Bitmap src)
    {
        var images = new List<byte[]>();
        foreach (var s in sizes) images.Add(RenderPng(src, s));
        using (var fs = new FileStream(path, FileMode.Create, FileAccess.Write))
        using (var bw = new BinaryWriter(fs))
        {
            bw.Write((short)0); bw.Write((short)1); bw.Write((short)sizes.Length);
            int offset = 6 + 16 * sizes.Length;
            for (int i = 0; i < sizes.Length; i++)
            {
                int s = sizes[i];
                bw.Write((byte)(s >= 256 ? 0 : s));
                bw.Write((byte)(s >= 256 ? 0 : s));
                bw.Write((byte)0); bw.Write((byte)0);
                bw.Write((short)1); bw.Write((short)32);
                bw.Write(images[i].Length);
                bw.Write(offset);
                offset += images[i].Length;
            }
            foreach (var img in images) bw.Write(img);
        }
    }
}
'@
Add-Type -TypeDefinition $code -ReferencedAssemblies System.Drawing

$src = [System.Drawing.Bitmap]::FromFile($SrcPng)
try {
    $cut = [IconMaker]::CutOut($src)
    $sq  = [IconMaker]::TrimSquare($cut)
    [IconMaker]::WriteIco($OutIco, @(256,128,64,48,32,24,16), $sq)
    # 256px preview for visual check
    [IO.File]::WriteAllBytes($Preview, [IconMaker]::RenderPng($sq, 256))
    Write-Host "OK: $OutIco ($([IO.FileInfo]::new($OutIco).Length) bytes), preview: $Preview"
}
finally { $src.Dispose() }
