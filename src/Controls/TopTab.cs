using System.Drawing;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>Horizontal segmented tab switcher (e.g. 裁切 / 漸層 / 修護 / 標誌).</summary>
public sealed class TopTab : Control
{
    private string[] _tabs = Array.Empty<string>();
    private int _selected;
    private int _hoverIndex = -1;

    public string[] Tabs
    {
        get => _tabs;
        set { _tabs = value ?? Array.Empty<string>(); if (_selected >= _tabs.Length) _selected = 0; Invalidate(); }
    }

    /// <summary>When true, clicking the active tab deselects it (SelectedIndex = -1 = no tab).</summary>
    public bool AllowDeselect { get; set; }

    public int SelectedIndex
    {
        get => _selected;
        set
        {
            int lo = AllowDeselect ? -1 : 0;
            int v = Math.Clamp(value, lo, Math.Max(0, _tabs.Length - 1));
            if (v != _selected) { _selected = v; Invalidate(); SelectedIndexChanged?.Invoke(this, EventArgs.Empty); }
        }
    }

    public event EventHandler? SelectedIndexChanged;

    public TopTab()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
        Height = Ui.S(32);
        Font = Theme.Normal;
    }

    private int IndexAt(int x) => _tabs.Length == 0 ? -1 : Math.Clamp(x / (Width / _tabs.Length), 0, _tabs.Length - 1);

    protected override void OnMouseMove(MouseEventArgs e)
    {
        int i = IndexAt(e.X);
        if (i != _hoverIndex) { _hoverIndex = i; Invalidate(); }
        base.OnMouseMove(e);
    }
    protected override void OnMouseLeave(EventArgs e) { _hoverIndex = -1; Invalidate(); base.OnMouseLeave(e); }
    protected override void OnMouseDown(MouseEventArgs e)
    {
        if (e.Button == MouseButtons.Left)
        {
            int i = IndexAt(e.X);
            SelectedIndex = AllowDeselect && i == _selected ? -1 : i;   // click active -> deselect
        }
        base.OnMouseDown(e);
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        g.Clear(Theme.PanelBg);
        if (_tabs.Length == 0) return;

        float tw = (float)Width / _tabs.Length;
        for (int i = 0; i < _tabs.Length; i++)
        {
            var r = new RectangleF(i * tw, 0, tw, Height);
            bool sel = i == _selected;
            if (sel)
                PaintHelpers.FillRounded(g, new RectangleF(r.X + Ui.S(2f), Ui.S(3f),
                    r.Width - Ui.S(4f), Height - Ui.S(6f)), Ui.S(4f), Theme.PanelBg3);
            Color fg = sel ? Theme.Text : i == _hoverIndex ? Theme.Text : Theme.TextDim;
            TextRenderer.DrawText(g, _tabs[i], Font, Rectangle.Round(r), fg,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
            if (sel)
                using (var pen = new Pen(Theme.Accent, Ui.S(2f)))
                    g.DrawLine(pen, r.X + Ui.S(8f), Height - Ui.S(2f), r.Right - Ui.S(8f), Height - Ui.S(2f));
        }
    }
}
