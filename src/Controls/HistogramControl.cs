using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;
using AwayPhotoRawEditor.Imaging;

namespace AwayPhotoRawEditor.Controls;

/// <summary>Draws an RGB histogram (three additive channel curves) on a dark panel.</summary>
public sealed class HistogramControl : Control
{
    private Histogram? _hist;

    public Histogram? Histogram
    {
        get => _hist;
        set { _hist = value; Invalidate(); }
    }

    public HistogramControl()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
        Height = Ui.S(120);
        BackColor = Theme.ViewerBg;
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        var r = new Rectangle(0, 0, Width, Height);
        PaintHelpers.FillRounded(g, new RectangleF(0.5f, 0.5f, Width - 1, Height - 1), Ui.S(4f), Theme.ViewerBg);

        // grid
        using (var pen = new Pen(Color.FromArgb(40, 255, 255, 255), Ui.SMin(1)))
            for (int i = 1; i < 4; i++)
            {
                int x = Width * i / 4;
                g.DrawLine(pen, x, Ui.S(2f), x, Height - Ui.S(2f));
            }

        if (_hist is null) { PaintHelpers.DrawRounded(g, new RectangleF(0.5f, 0.5f, Width - 1, Height - 1), Ui.S(4f), Theme.Border); return; }

        g.SetClip(r);
        g.CompositingMode = CompositingMode.SourceOver;
        DrawChannel(g, _hist.R, Color.FromArgb(150, 235, 70, 70));
        DrawChannel(g, _hist.G, Color.FromArgb(150, 70, 210, 90));
        DrawChannel(g, _hist.B, Color.FromArgb(150, 80, 130, 235));
        g.ResetClip();

        PaintHelpers.DrawRounded(g, new RectangleF(0.5f, 0.5f, Width - 1, Height - 1), Ui.S(4f), Theme.Border);
    }

    private void DrawChannel(Graphics g, int[] bins, Color color)
    {
        int h = Height, w = Width;
        float inset = Ui.S(2f);                      // 上下留白，避免曲線貼齊邊框
        float max = Math.Max(1, _hist!.Max);
        var pts = new PointF[258];
        pts[0] = new PointF(0, h);
        for (int i = 0; i < 256; i++)
        {
            float x = i / 255f * (w - 1);
            float v = Math.Min(1f, bins[i] / max);
            float y = h - inset - v * (h - 2 * inset);
            pts[i + 1] = new PointF(x, y);
        }
        pts[257] = new PointF(w - 1, h);
        using var brush = new SolidBrush(color);
        using var path = new GraphicsPath();
        path.AddPolygon(pts);
        g.FillPath(brush, path);
    }
}
