using System.Drawing;
using System.Drawing.Imaging;
using System.IO;

namespace AwayPhotoRawEditor.Imaging;

/// <summary>
/// Decoder for regular bitmap formats (jpg/png/tif/bmp) via GDI+ / WIC. Loads
/// without keeping a file lock and honours the EXIF orientation tag. Used both
/// as the primary path for non-RAW files and as a RAW fallback for already-decoded
/// preview bytes.
/// </summary>
public static class WicDecoder
{
    private const int OrientationTagId = 0x0112;

    /// <summary>Load a regular image file into a detached 32bpp bitmap (no file lock).</summary>
    public static Bitmap? LoadFile(string path)
    {
        try
        {
            byte[] bytes = File.ReadAllBytes(path);
            return LoadBytes(bytes);
        }
        catch { return null; }
    }

    /// <summary>Decode image bytes (e.g. an embedded JPEG preview) into a bitmap.</summary>
    public static Bitmap? LoadBytes(byte[] bytes)
    {
        try
        {
            using var ms = new MemoryStream(bytes, writable: false);
            using var img = Image.FromStream(ms, useEmbeddedColorManagement: true, validateImageData: false);
            int orientation = ReadOrientation(img);
            var bmp = new Bitmap(img.Width, img.Height, PixelFormat.Format32bppArgb);
            using (var g = Graphics.FromImage(bmp))
            {
                g.InterpolationMode = System.Drawing.Drawing2D.InterpolationMode.HighQualityBicubic;
                g.DrawImage(img, 0, 0, img.Width, img.Height);
            }
            return ApplyOrientation(bmp, orientation);
        }
        catch { return null; }
    }

    private static int ReadOrientation(Image img)
    {
        try
        {
            foreach (var id in img.PropertyIdList)
                if (id == OrientationTagId)
                {
                    var p = img.GetPropertyItem(OrientationTagId);
                    if (p?.Value is { Length: >= 2 }) return BitConverter.ToUInt16(p.Value, 0);
                }
        }
        catch { }
        return 1;
    }

    private static Bitmap ApplyOrientation(Bitmap bmp, int orientation)
    {
        RotateFlipType op = orientation switch
        {
            2 => RotateFlipType.RotateNoneFlipX,
            3 => RotateFlipType.Rotate180FlipNone,
            4 => RotateFlipType.RotateNoneFlipY,
            5 => RotateFlipType.Rotate90FlipX,
            6 => RotateFlipType.Rotate90FlipNone,
            7 => RotateFlipType.Rotate270FlipX,
            8 => RotateFlipType.Rotate270FlipNone,
            _ => RotateFlipType.RotateNoneFlipNone
        };
        if (op != RotateFlipType.RotateNoneFlipNone) bmp.RotateFlip(op);
        return bmp;
    }
}
