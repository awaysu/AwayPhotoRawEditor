using System.Drawing;
using System.Drawing.Drawing2D;

namespace AwayPhotoRawEditor.Controls;

/// <summary>Shared drawing helpers for the custom controls.</summary>
public static class PaintHelpers
{
    public static GraphicsPath RoundedRect(RectangleF r, float radius)
    {
        var path = new GraphicsPath();
        if (radius <= 0) { path.AddRectangle(r); return path; }
        float d = radius * 2;
        d = Math.Min(d, Math.Min(r.Width, r.Height));
        path.AddArc(r.X, r.Y, d, d, 180, 90);
        path.AddArc(r.Right - d, r.Y, d, d, 270, 90);
        path.AddArc(r.Right - d, r.Bottom - d, d, d, 0, 90);
        path.AddArc(r.X, r.Bottom - d, d, d, 90, 90);
        path.CloseFigure();
        return path;
    }

    public static void FillRounded(Graphics g, RectangleF r, float radius, Color color)
    {
        using var p = RoundedRect(r, radius);
        using var b = new SolidBrush(color);
        g.FillPath(b, p);
    }

    /// <param name="width">線寬（實際像素）。0 = 自動，取隨 DPI 縮放的 1 設計像素。</param>
    public static void DrawRounded(Graphics g, RectangleF r, float radius, Color color, float width = 0f)
    {
        using var p = RoundedRect(r, radius);
        using var pen = new Pen(color, width > 0 ? width : Ui.SMin(1));
        g.DrawPath(pen, p);
    }

    public static void EnableHighQuality(Graphics g)
    {
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.InterpolationMode = InterpolationMode.HighQualityBicubic;
        g.PixelOffsetMode = PixelOffsetMode.HighQuality;
        g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.ClearTypeGridFit;
    }
}
