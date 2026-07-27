using System;
using System.Drawing;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>Self-drawn dark checkbox with a high-contrast accent tick when checked.</summary>
public sealed class DarkCheckBox : CheckBox
{
    private bool _hover;
    private const int Box = 17;

    public DarkCheckBox()
    {
        SetStyle(ControlStyles.UserPaint | ControlStyles.AllPaintingInWmPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
        FlatStyle = FlatStyle.Flat;
        ForeColor = Theme.Text;
        BackColor = Theme.PanelBg;
        Font = Theme.Normal;
        AutoSize = false;
        Cursor = Cursors.Hand;
        Height = 24;
        CheckedChanged += (_, _) => Invalidate();
    }

    protected override void OnMouseEnter(EventArgs e) { _hover = true; Invalidate(); base.OnMouseEnter(e); }
    protected override void OnMouseLeave(EventArgs e) { _hover = false; Invalidate(); base.OnMouseLeave(e); }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        using (var bg = new SolidBrush(BackColor)) g.FillRectangle(bg, ClientRectangle);

        int by = (Height - Box) / 2;
        var r = new RectangleF(0.5f, by + 0.5f, Box, Box);
        if (Checked)
        {
            PaintHelpers.FillRounded(g, r, 4, Enabled ? Theme.Accent : Theme.AccentDim);
            using var pen = new Pen(Color.White, 2f) { StartCap = System.Drawing.Drawing2D.LineCap.Round, EndCap = System.Drawing.Drawing2D.LineCap.Round };
            g.DrawLines(pen, new[] { new PointF(4, by + 9), new PointF(7.5f, by + 12.5f), new PointF(13, by + 5) });
        }
        else
        {
            PaintHelpers.FillRounded(g, r, 4, Theme.PanelBg3);
            PaintHelpers.DrawRounded(g, r, 4, _hover && Enabled ? Theme.Accent : Theme.BorderLight, _hover ? 1.5f : 1f);
        }

        var textRect = new Rectangle(Box + 9, 0, Width - Box - 9, Height);
        TextRenderer.DrawText(g, Text, Font, textRect, Enabled ? ForeColor : Theme.TextFaint,
            TextFormatFlags.Left | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
    }
}
