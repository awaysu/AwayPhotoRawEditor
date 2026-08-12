using System.Drawing;
using System.Runtime.InteropServices;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>
/// 深色垂直捲動容器：包住一個固定寬度的內容控制項（左右欄的 FlowLayoutPanel），
/// 內容高度超過可視高度時，右緣顯示 10px 自繪細捲軸（此時內容縮窄 10px，
/// 捲軸與內容不重疊——重疊的兄弟控制項 z-order 在 WinForms 不可靠），
/// 支援 滾輪 / 拖曳滑塊 / 點擊軌道跳頁。
/// 滾輪用 IMessageFilter 依「游標位置」轉送：游標在 AdjustmentSlider 上時滾輪仍是
/// 微調數值，其餘區域則捲動欄位（WinForms 預設滾輪送給焦點控制項，不攔會捲不動）。
/// </summary>
public sealed class DarkScrollHost : Panel, IMessageFilter
{
    private static int BarW => Ui.S(10);   // 96 DPI 設計值
    private const int WM_MOUSEWHEEL = 0x020A;

    [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(Point pt);
    [DllImport("user32.dll")] private static extern IntPtr SendMessage(IntPtr hWnd, int msg, IntPtr wParam, IntPtr lParam);

    private readonly Control _content;
    private readonly ScrollBarOverlay _bar;
    private int _scrollY;
    private int _contentHeight;
    private bool _inRelayout;
    private bool _filterAdded;

    public DarkScrollHost(Control content)
    {
        _content = content;
        _content.Dock = DockStyle.None;
        _bar = new ScrollBarOverlay(this) { Visible = false };   // 先建好，Controls.Add 會立刻觸發 OnLayout
        Controls.Add(_content);
        Controls.Add(_bar);
        _bar.BringToFront();
    }

    private bool _scrollEnabled = true;

    /// <summary>是否啟用捲動（設定「顯示捲軸」）。停用時內容佔滿寬度、超出部分直接裁切（舊行為）。</summary>
    public bool ScrollEnabled
    {
        get => _scrollEnabled;
        set
        {
            if (_scrollEnabled == value) return;
            _scrollEnabled = value;
            if (!value) _scrollY = 0;
            Relayout();
        }
    }

    private int MaxScroll => Math.Max(0, _contentHeight - ClientSize.Height);

    protected override void OnLayout(LayoutEventArgs levent)
    {
        base.OnLayout(levent);
        Relayout();
    }

    private void Relayout()
    {
        if (_inRelayout || _bar is null || ClientSize.Width <= 0 || ClientSize.Height <= 0) return;
        _inRelayout = true;
        try
        {
            _contentHeight = Math.Max(
                _content.GetPreferredSize(new Size(ClientSize.Width, 0)).Height,
                ClientSize.Height);
            _scrollY = _scrollEnabled ? Math.Clamp(_scrollY, 0, MaxScroll) : 0;

            bool need = _scrollEnabled && MaxScroll > 0;
            int barW = BarW;
            int contentW = ClientSize.Width - (need ? barW : 0);
            _content.SetBounds(0, -_scrollY, contentW, _contentHeight);
            if (need)
            {
                _bar.SetBounds(ClientSize.Width - barW, 0, barW, ClientSize.Height);
                _bar.Invalidate();
            }
            _bar.Visible = need;
        }
        finally { _inRelayout = false; }
    }

    private void ScrollBy(int dy) => ScrollTo(_scrollY + dy);

    private void ScrollTo(int y)
    {
        y = _scrollEnabled ? Math.Clamp(y, 0, MaxScroll) : 0;
        if (y == _scrollY) return;
        _scrollY = y;
        _content.Top = -_scrollY;
        _bar.Invalidate();
    }

    protected override void OnMouseWheel(MouseEventArgs e)
    {
        ScrollBy(Ui.S(-e.Delta));   // 捲動量隨縮放，手感一致
        base.OnMouseWheel(e);
    }

    // ---- 滾輪轉送（依游標位置） -------------------------------------------

    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        if (!_filterAdded) { Application.AddMessageFilter(this); _filterAdded = true; }
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing && _filterAdded) { Application.RemoveMessageFilter(this); _filterAdded = false; }
        base.Dispose(disposing);
    }

    public bool PreFilterMessage(ref Message m)
    {
        if (m.Msg != WM_MOUSEWHEEL || !_scrollEnabled || !IsHandleCreated || !Visible || MaxScroll <= 0) return false;

        var pos = new Point(unchecked((short)((long)m.LParam & 0xFFFF)),
                            unchecked((short)(((long)m.LParam >> 16) & 0xFFFF)));
        if (!RectangleToScreen(ClientRectangle).Contains(pos)) return false;

        // 找出游標下的控制項；非本容器內（如 combo 下拉彈窗）不攔截
        var target = FromChildHandle(WindowFromPoint(pos));
        if (target is null) return false;
        if (target != this && !Contains(target)) return false;

        for (var c = target; c != null && c != this; c = c.Parent)
        {
            if (c is AdjustmentSlider)
            {
                SendMessage(c.Handle, WM_MOUSEWHEEL, m.WParam, m.LParam);   // 滑桿保持滾輪微調
                return true;
            }
            if ((c is ComboBox || c is TextBoxBase || c is NumericUpDown) && c.ContainsFocus)
                return false;                                               // 焦點輸入控制項維持原生行為
        }
        int delta = unchecked((short)((long)m.WParam >> 16));
        ScrollBy(Ui.S(-delta));
        return true;
    }

    // ---- 自繪捲軸 ---------------------------------------------------------

    private sealed class ScrollBarOverlay : Control
    {
        private readonly DarkScrollHost _host;
        private bool _dragging;
        private int _dragOffset;
        private bool _hover;

        public ScrollBarOverlay(DarkScrollHost host)
        {
            _host = host;
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                     ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
            BackColor = Theme.PanelBg;
        }

        private Rectangle ThumbRect()
        {
            int max = _host.MaxScroll;
            if (max <= 0 || Height <= 0 || _host._contentHeight <= 0) return Rectangle.Empty;
            int th = Math.Max(Ui.S(30), (int)(Height * (Height / (float)_host._contentHeight)));
            int ty = (int)(_host._scrollY / (float)max * (Height - th));
            return new Rectangle(Ui.S(2), ty, Width - Ui.S(4), th);
        }

        private void ScrollToThumbY(int thumbY)
        {
            int max = _host.MaxScroll;
            int th = ThumbRect().Height;
            if (max <= 0 || Height - th <= 0) return;
            _host.ScrollTo((int)(thumbY / (float)(Height - th) * max));
        }

        protected override void OnMouseDown(MouseEventArgs e)
        {
            if (e.Button != MouseButtons.Left) return;
            var tr = ThumbRect();
            if (tr.Contains(e.Location)) { _dragging = true; _dragOffset = e.Y - tr.Y; }
            else ScrollToThumbY(e.Y - tr.Height / 2);       // 點軌道：滑塊置中到點擊處
            base.OnMouseDown(e);
        }

        protected override void OnMouseMove(MouseEventArgs e)
        {
            if (_dragging) ScrollToThumbY(e.Y - _dragOffset);
            else { bool h = ThumbRect().Contains(e.Location); if (h != _hover) { _hover = h; Invalidate(); } }
            base.OnMouseMove(e);
        }

        protected override void OnMouseUp(MouseEventArgs e) { _dragging = false; base.OnMouseUp(e); }
        protected override void OnMouseLeave(EventArgs e) { _hover = false; Invalidate(); base.OnMouseLeave(e); }

        protected override void OnMouseWheel(MouseEventArgs e)
        {
            _host.ScrollBy(Ui.S(-e.Delta));
            base.OnMouseWheel(e);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            PaintHelpers.EnableHighQuality(g);
            g.Clear(BackColor);
            var tr = ThumbRect();
            if (!tr.IsEmpty)
                PaintHelpers.FillRounded(g, tr, Ui.S(3f), _dragging || _hover ? Theme.SliderFill : Theme.BorderLight);
        }
    }
}
