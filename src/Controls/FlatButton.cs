using System.Drawing;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>Flat, dark, self-drawn push button with optional accent (primary) style.</summary>
public sealed class FlatButton : Control
{
    private bool _hover, _pressed;
    private bool _primary;

    public bool Primary
    {
        get => _primary;
        set { if (_primary != value) { _primary = value; Invalidate(); } }
    }
    public float CornerRadius { get; set; } = 4f;
    public Color? CustomBack { get; set; }
    /// <summary>Left-align the caption with a small inset (for list/sidebar rows).</summary>
    public bool LeftAlign { get; set; }

    /// <summary>CornerRadius 與內縮值都是 96 DPI 設計值，繪製時才乘 Ui.Scale。</summary>

    public FlatButton()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
        Font = Theme.Normal;
        ForeColor = Theme.Text;
        Size = Ui.Sz(120, 30);
        Cursor = Cursors.Hand;
    }

    protected override void OnMouseEnter(EventArgs e) { _hover = true; Invalidate(); base.OnMouseEnter(e); }
    protected override void OnMouseLeave(EventArgs e) { _hover = false; _pressed = false; Invalidate(); base.OnMouseLeave(e); }
    protected override void OnMouseDown(MouseEventArgs e) { if (e.Button == MouseButtons.Left) { _pressed = true; Invalidate(); } base.OnMouseDown(e); }
    protected override void OnMouseUp(MouseEventArgs e) { _pressed = false; Invalidate(); base.OnMouseUp(e); }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        var r = new RectangleF(0.5f, 0.5f, Width - 1, Height - 1);

        Color back = CustomBack ??
            (Primary
                ? (_pressed ? Theme.AccentDim : _hover ? Theme.AccentHover : Theme.Accent)
                : (_pressed ? Theme.PanelBg3 : _hover ? Theme.PanelBg3 : Theme.PanelBg2));
        if (!Enabled) back = Theme.PanelBg2;

        PaintHelpers.FillRounded(g, r, Ui.S(CornerRadius), back);
        if (!Primary) PaintHelpers.DrawRounded(g, r, Ui.S(CornerRadius), _hover ? Theme.BorderLight : Theme.Border);

        Color fg = !Enabled ? Theme.TextFaint : Primary ? Color.White : ForeColor;
        var textRect = LeftAlign
            ? new Rectangle(Ui.S(12), 0, Width - Ui.S(16), Height)
            : new Rectangle(0, 0, Width, Height);
        var align = LeftAlign ? TextFormatFlags.Left : TextFormatFlags.HorizontalCenter;
        TextRenderer.DrawText(g, Text, Font, textRect, fg,
            align | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
    }
}
