using System.Drawing;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>
/// A titled dark section (基本調整 / 色彩 / 細節 …). Two modes:
///  • auto-stack (default): children added via <see cref="AddContent"/> stack vertically.
///  • manual (<see cref="ManualLayout"/> = true): fixed <see cref="Control.Size"/>, place
///    children directly into <see cref="ContentArea"/> using absolute coordinates.
/// </summary>
public class SectionPanel : Control
{
    // 96 DPI 設計值 → 實際像素（只在本檔內使用）。
    public static int HeaderH => Ui.S(30);
    private static int Pad => Ui.S(10);

    private readonly Panel _content;
    private bool _collapsed;
    private bool _hoverHeader;
    private bool _inRelayout;

    public string Title { get; set; }

    /// <summary>When true the section keeps its explicit Size and does not auto-stack/collapse.</summary>
    public bool ManualLayout { get; set; }

    /// <summary>The content region below the header (for manual absolute placement).</summary>
    public Panel ContentArea => _content;

    public bool Collapsed
    {
        get => _collapsed;
        set { if (!ManualLayout && _collapsed != value) { _collapsed = value; Relayout(); Invalidate(); CollapsedChanged?.Invoke(this, EventArgs.Empty); } }
    }

    public event EventHandler? CollapsedChanged;

    public SectionPanel(string title = "")
    {
        Title = title;
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer, true);
        BackColor = Theme.PanelBg;
        Dock = DockStyle.Top;
        _content = new Panel { BackColor = Theme.PanelBg, Left = 0, Top = HeaderH };
        Controls.Add(_content);
        Height = HeaderH + Ui.S(6);
    }

    /// <summary>Add a child stacked below the previous one (auto-stack mode).</summary>
    public void AddContent(Control c, int extraTopGap = 0)
    {
        c.Tag ??= extraTopGap;
        _content.Controls.Add(c);
        c.SizeChanged += (_, _) => Relayout();
        Relayout();
    }

    public void ClearContent()
    {
        foreach (Control c in _content.Controls) c.Dispose();
        _content.Controls.Clear();
        Relayout();
    }

    private void Relayout()
    {
        if (_inRelayout) return;
        _inRelayout = true;
        try
        {
            if (ManualLayout)
            {
                _content.SetBounds(0, HeaderH, Width, Math.Max(0, Height - HeaderH));
                _content.Visible = true;
                return;
            }

            int y = Ui.S(4);
            int w = Math.Max(Ui.S(10), Width - 2 * Pad);
            foreach (Control c in _content.Controls)
            {
                int gap = c.Tag is int g ? Ui.S(g) : 0;   // AddContent 的 extraTopGap 也是設計值
                y += gap;
                c.Left = Pad;
                c.Top = y;
                c.Width = w;
                y += c.Height + Ui.S(6);
            }
            _content.Width = Width;
            _content.Height = y + Ui.S(2);
            _content.Visible = !_collapsed;
            Height = HeaderH + (_collapsed ? Ui.S(4) : _content.Height);
        }
        finally { _inRelayout = false; }
    }

    protected override void OnResize(EventArgs e) { base.OnResize(e); Relayout(); }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        if (!ManualLayout)
        {
            bool h = e.Y < HeaderH;
            if (h != _hoverHeader) { _hoverHeader = h; Invalidate(); }
            Cursor = h ? Cursors.Hand : Cursors.Default;
        }
        base.OnMouseMove(e);
    }
    protected override void OnMouseLeave(EventArgs e) { _hoverHeader = false; Invalidate(); base.OnMouseLeave(e); }
    protected override void OnMouseDown(MouseEventArgs e)
    {
        if (!ManualLayout && e.Button == MouseButtons.Left && e.Y < HeaderH) Collapsed = !Collapsed;
        base.OnMouseDown(e);
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        g.Clear(Theme.PanelBg);

        var hr = new Rectangle(0, 0, Width, HeaderH);
        using (var b = new SolidBrush(_hoverHeader ? Theme.PanelBg2 : Theme.PanelBg))
            g.FillRectangle(b, hr);

        if (!ManualLayout)
        {
            float cx = Width - Ui.S(20f), cy = HeaderH / 2f;
            using var pen = new Pen(Theme.TextDim, Ui.S(1.6f));
            if (_collapsed)
            {
                g.DrawLine(pen, cx - Ui.S(3f), cy - Ui.S(3f), cx + Ui.S(2f), cy);
                g.DrawLine(pen, cx + Ui.S(2f), cy, cx - Ui.S(3f), cy + Ui.S(3f));
            }
            else
            {
                g.DrawLine(pen, cx - Ui.S(4f), cy - Ui.S(2f), cx, cy + Ui.S(3f));
                g.DrawLine(pen, cx, cy + Ui.S(3f), cx + Ui.S(4f), cy - Ui.S(2f));
            }
        }

        TextRenderer.DrawText(g, Title, Theme.Header, new Rectangle(Pad, 0, Width - Ui.S(20), HeaderH),
            Theme.Text, TextFormatFlags.Left | TextFormatFlags.VerticalCenter);

        using (var pen = new Pen(Theme.Border, Ui.SMin(1)))
            g.DrawLine(pen, Pad, HeaderH - Ui.SMin(1), Width - Pad, HeaderH - Ui.SMin(1));
    }
}
