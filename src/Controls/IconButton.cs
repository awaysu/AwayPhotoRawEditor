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
    public float GlyphSize { get; set; } = 12f;
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
        Size = new Size(30, 30);
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
        PaintHelpers.FillRounded(g, r, 4f, back);
        PaintHelpers.DrawRounded(g, r, 4f, _checked ? Theme.AccentHover : (_hover ? Theme.BorderLight : Theme.Border));

        using var f = new Font(Theme.FontFamily, GlyphSize);
        Color fg = _checked ? Color.White : Enabled ? ForeColor : Theme.TextFaint;
        TextRenderer.DrawText(g, Glyph, f, new Rectangle(0, 0, Width, Height), fg,
            TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
    }
}
