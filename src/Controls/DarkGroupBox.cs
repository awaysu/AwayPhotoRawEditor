using System.Drawing;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>
/// Dark-themed GroupBox: a rounded PanelBg fill with a border, and the caption text
/// sitting on the top border (classic group-box look) rendered in the dark palette.
/// Child controls should use <see cref="Theme.PanelBg"/> as their own BackColor.
/// </summary>
public sealed class DarkGroupBox : GroupBox
{
    public DarkGroupBox()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
        BackColor = Theme.WindowBg;   // painted behind the rounded fill so corners read
        ForeColor = Theme.Text;
        Font = Theme.Normal;          // children inherit normal; the caption uses Theme.Header
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        g.Clear(BackColor);

        string title = Text ?? "";
        var titleFont = Theme.Header;
        Size ts = title.Length > 0 ? TextRenderer.MeasureText(g, title, titleFont) : Size.Empty;
        float top = title.Length > 0 ? ts.Height / 2f : 0.5f;

        var r = new RectangleF(0.5f, top, Width - 1, Height - top - 1);
        PaintHelpers.FillRounded(g, r, 8f, Theme.PanelBg);
        PaintHelpers.DrawRounded(g, r, 8f, Theme.Border);

        if (title.Length > 0)
        {
            const int tx = 12;
            using (var b = new SolidBrush(Theme.PanelBg))   // notch out the border behind the caption
                g.FillRectangle(b, tx - 4, 0, ts.Width + 8, ts.Height);
            TextRenderer.DrawText(g, title, titleFont, new Point(tx, 0), Theme.Text, TextFormatFlags.NoPadding);
        }
    }
}
