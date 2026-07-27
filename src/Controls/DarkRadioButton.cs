using System;
using System.Drawing;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>Self-drawn dark radio button with a high-contrast accent dot when selected.</summary>
public sealed class DarkRadioButton : RadioButton
{
    private bool _hover;
    private const int Ring = 17;

    public DarkRadioButton()
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

        int cy = Height / 2;
        var ringRect = new RectangleF(0.5f, cy - Ring / 2f + 0.5f, Ring, Ring);
        using (var fill = new SolidBrush(Theme.PanelBg3)) g.FillEllipse(fill, ringRect);
        using (var pen = new Pen(Checked ? Theme.Accent : (_hover && Enabled ? Theme.Accent : Theme.BorderLight), Checked || _hover ? 1.6f : 1f))
            g.DrawEllipse(pen, ringRect);
        if (Checked)
        {
            const int dot = 8;
            using var b = new SolidBrush(Enabled ? Theme.Accent : Theme.AccentDim);
            g.FillEllipse(b, (Ring - dot) / 2f + 0.5f, cy - dot / 2f, dot, dot);
        }

        var textRect = new Rectangle(Ring + 9, 0, Width - Ring - 9, Height);
        TextRenderer.DrawText(g, Text, Font, textRect, Enabled ? ForeColor : Theme.TextFaint,
            TextFormatFlags.Left | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
    }
}
