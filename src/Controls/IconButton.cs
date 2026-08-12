using System.Drawing;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>
/// Small square icon button drawing a glyph (unicode symbol or short text).
/// Supports a toggled/checked state (e.g. the white-balance eyedropper).
/// </summary>
public sealed class IconButton : Control
{
    private bool _hover, _pressed;
    private bool _checked;

    public string Glyph { get; set; } = "";
    /// <summary>圖示字級，單位是 100% 下的像素（設定 →「字體大小」可調）。</summary>
    public int GlyphPx { get; set; } = Theme.Sizes.IconGlyph;
    public bool Checkable { get; set; }

    public bool Checked
    {
        get => _checked;
        set { if (_checked != value) { _checked = value; Invalidate(); CheckedChanged?.Invoke(this, EventArgs.Empty); } }
    }

    public event EventHandler? CheckedChanged;

    public IconButton()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
        Size = Ui.Sz(30, 30);
        ForeColor = Theme.Text;
        Cursor = Cursors.Hand;
    }

    protected override void OnMouseEnter(EventArgs e) { _hover = true; Invalidate(); base.OnMouseEnter(e); }
    protected override void OnMouseLeave(EventArgs e) { _hover = false; _pressed = false; Invalidate(); base.OnMouseLeave(e); }
    protected override void OnMouseDown(MouseEventArgs e) { if (e.Button == MouseButtons.Left) { _pressed = true; Invalidate(); } base.OnMouseDown(e); }
    protected override void OnMouseUp(MouseEventArgs e)
    {
        _pressed = false;
        if (Checkable && e.Button == MouseButtons.Left && ClientRectangle.Contains(e.Location)) Checked = !Checked;
        Invalidate();
        base.OnMouseUp(e);
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        var r = new RectangleF(0.5f, 0.5f, Width - 1, Height - 1);

        Color back = _checked ? Theme.Accent
            : _pressed ? Theme.PanelBg3 : _hover ? Theme.PanelBg3 : Theme.PanelBg2;
        PaintHelpers.FillRounded(g, r, Ui.S(4f), back);
        PaintHelpers.DrawRounded(g, r, Ui.S(4f), _checked ? Theme.AccentHover : (_hover ? Theme.BorderLight : Theme.Border));

        using var f = Theme.UIPx(GlyphPx);   // 字級由 GDI+ 依 DPI 換算，不乘 Ui.Scale
        Color fg = _checked ? Color.White : Enabled ? ForeColor : Theme.TextFaint;
        TextRenderer.DrawText(g, Glyph, f, new Rectangle(0, 0, Width, Height), fg,
            TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
    }
}
