using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>Simple vertical stack: children full-width, stacked top-down, panel auto-heights.</summary>
public sealed class VStackPanel : Panel
{
    private readonly List<Control> _order = new();
    private bool _inRelayout;
    /// <summary>96 DPI 設計值，排版時才乘 Ui.Scale。</summary>
    public int Gap { get; set; } = 6;
    /// <summary>96 DPI 設計值，排版時才乘 Ui.Scale。</summary>
    public int SidePad { get; set; } = 0;

    public VStackPanel()
    {
        SetStyle(ControlStyles.OptimizedDoubleBuffer, true);
        BackColor = Theme.PanelBg;
    }

    public void SetChildren(params Control[] children)
    {
        foreach (Control c in _order) c.SizeChanged -= OnChildSizeChanged;
        Controls.Clear();
        _order.Clear();
        foreach (var c in children) Add(c);
        Relayout();
    }

    public void Add(Control c)
    {
        _order.Add(c);
        c.SizeChanged += OnChildSizeChanged;
        Controls.Add(c);
        Relayout();
    }

    private void OnChildSizeChanged(object? s, EventArgs e) => Relayout();

    private void Relayout()
    {
        if (_inRelayout) return;         // guard against re-entrant layout (setting Height -> OnResize -> Relayout)
        _inRelayout = true;
        try
        {
            int y = 0;
            int pad = Ui.S(SidePad), gap = Ui.S(Gap);
            int w = Math.Max(Ui.S(10), Width - 2 * pad);
            foreach (var c in _order)
            {
                if (!c.Visible) continue;
                c.Left = pad; c.Top = y; c.Width = w;
                y += c.Height + gap;
            }
            Height = Math.Max(0, y);
        }
        finally { _inRelayout = false; }
    }

    protected override void OnResize(EventArgs e) { base.OnResize(e); Relayout(); }
}
