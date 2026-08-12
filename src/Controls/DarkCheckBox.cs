using System;
using System.Drawing;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>Self-drawn dark checkbox with a high-contrast accent tick when checked.</summary>
public sealed class DarkCheckBox : CheckBox
{
    private bool _hover;
    private static int Box => Ui.S(17);   // 96 DPI 設計值

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
        Height = Ui.S(24);
        CheckedChanged += (_, _) => Invalidate();
    }

    protected override void OnMouseEnter(EventArgs e) { _hover = true; Invalidate(); base.OnMouseEnter(e); }
    protected override void OnMouseLeave(EventArgs e) { _hover = false; Invalidate(); base.OnMouseLeave(e); }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        using (var bg = new SolidBrush(BackColor)) g.FillRectangle(bg, ClientRectangle);

        int box = Box;
        int by = (Height - box) / 2;
        var r = new RectangleF(0.5f, by + 0.5f, box, box);
        if (Checked)
        {
            PaintHelpers.FillRounded(g, r, Ui.S(4f), Enabled ? Theme.Accent : Theme.AccentDim);
            using var pen = new Pen(Color.White, Ui.S(2f)) { StartCap = System.Drawing.Drawing2D.LineCap.Round, EndCap = System.Drawing.Drawing2D.LineCap.Round };
            g.DrawLines(pen, new[]
            {
                new PointF(Ui.S(4f), by + Ui.S(9f)),
                new PointF(Ui.S(7.5f), by + Ui.S(12.5f)),
                new PointF(Ui.S(13f), by + Ui.S(5f))
            });
        }
        else
        {
            PaintHelpers.FillRounded(g, r, Ui.S(4f), Theme.PanelBg3);
            PaintHelpers.DrawRounded(g, r, Ui.S(4f), _hover && Enabled ? Theme.Accent : Theme.BorderLight,
                _hover ? Ui.S(1.5f) : Ui.SMin(1));
        }

        int gap = Ui.S(9);
        var textRect = new Rectangle(box + gap, 0, Width - box - gap, Height);
        TextRenderer.DrawText(g, Text, Font, textRect, Enabled ? ForeColor : Theme.TextFaint,
            TextFormatFlags.Left | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
    }
}
