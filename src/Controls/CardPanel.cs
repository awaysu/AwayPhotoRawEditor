using System.Drawing;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>
/// A rounded, flat "card" surface used to group related inputs in dialogs.
/// Paints a rounded fill (and optional border) on the parent's background so the
/// corners read as rounded. Child controls should use <see cref="CardColor"/> as
/// their own <c>BackColor</c>.
/// </summary>
public sealed class CardPanel : Panel
{
    public float CornerRadius { get; set; } = 8f;
    public Color CardColor { get; set; } = Theme.PanelBg;
    public Color? BorderColor { get; set; } = Theme.Border;

    public CardPanel()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
        BackColor = Theme.WindowBg;
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        var r = new RectangleF(0.5f, 0.5f, Width - 1, Height - 1);
        PaintHelpers.FillRounded(g, r, Ui.S(CornerRadius), CardColor);
        if (BorderColor is { } bc) PaintHelpers.DrawRounded(g, r, Ui.S(CornerRadius), bc);
    }
}
