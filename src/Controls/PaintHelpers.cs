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

    public static void FillRounded(Graphics g, RectangleF r, float radius, Brush brush)
    {
        using var p = RoundedRect(r, radius);
        g.FillPath(brush, p);
    }

    /// <summary>Fill a rounded rect with a left-to-right gradient through evenly spaced
    /// <paramref name="stops"/> (2 or more colours). Used by the colour-coded sliders.</summary>
    public static void FillRoundedGradient(Graphics g, RectangleF r, float radius, Color[] stops)
    {
        if (stops.Length == 0 || r.Width <= 0 || r.Height <= 0) return;
        if (stops.Length == 1) { FillRounded(g, r, radius, stops[0]); return; }

        // Inflate horizontally: LinearGradientBrush mirrors the edge pixel column, which
        // would otherwise show the first/last stop twice at the ends of the track.
        var brushRect = RectangleF.Inflate(r, 1f, 0f);
        using var brush = new LinearGradientBrush(brushRect, stops[0], stops[^1], LinearGradientMode.Horizontal)
        {
            WrapMode = WrapMode.TileFlipX
        };
        var positions = new float[stops.Length];
        for (int i = 0; i < stops.Length; i++) positions[i] = i / (float)(stops.Length - 1);
        brush.InterpolationColors = new ColorBlend { Colors = stops, Positions = positions };
        FillRounded(g, r, radius, brush);
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
