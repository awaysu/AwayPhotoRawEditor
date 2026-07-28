using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Controls;

/// <summary>
/// Central image viewer: zoom (fit/100/200/wheel), drag-pan, and tool overlays
/// for crop, gradient and heal, plus a white-balance eyedropper. Overlay geometry
/// is read from / written to a bound <see cref="ImageAdjustments"/> in normalized
/// coordinates relative to the displayed bitmap.
/// </summary>
public sealed class ImageViewer : Control
{
    private enum Drag
    {
        None, Pan,
        CropMove, CropL, CropR, CropT, CropB, CropTL, CropTR, CropBL, CropBR,
        GradCenter, GradRotate, GradRange, HealTarget, HealSource
    }

    private Bitmap? _image;
    private ImageAdjustments? _adj;
    private double _scale = 1;
    private PointF _offset;
    private ZoomMode _zoomMode = ZoomMode.Fit;

    // ---- zoom/pan animation ("瞬間漸放大縮小" feel) ----------------------
    private readonly System.Windows.Forms.Timer _animTimer;
    private bool _animating;
    private double _animScaleFrom, _animScaleTo;
    private PointF _animOffsetFrom, _animOffsetTo;
    private long _animStartTick;
    private const int AnimDurationMs = 150;

    private Drag _drag;
    private int _dragSpot = -1;
    private int _activeSpot = -1;   // last placed / edited heal spot — target of live size changes
    private Point _dragStart;
    private PointF _offsetStart;
    private RectangleF _cropStart;
    private bool _dragMoved;
    private const int PanThreshold = 4;   // px before a left-drag counts as a pan (vs a click)

    public ToolMode Tool { get; set; } = ToolMode.None;
    public HealMode HealMode { get; set; } = HealMode.Clone;
    public double HealBrushSize { get; set; } = 10;
    public bool WhiteBalancePickerActive { get; set; }

    public event EventHandler? EditBegin;
    public event EventHandler? EditChanged;
    public event Action<double, double>? WhiteBalancePicked;
    public event EventHandler? ViewChanged;
    /// <summary>Raised when the active gradient changes (selected in the viewer or deleted),
    /// so the tools panel can re-bind its sliders to the newly active gradient.</summary>
    public event Action? GradientSelectionChanged;

    public double ZoomPercent => _scale * 100;
    public ZoomMode ZoomMode => _zoomMode;

    public ImageViewer()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw |
                 ControlStyles.Selectable, true);
        BackColor = Theme.ViewerBg;
        TabStop = true;
        _animTimer = new System.Windows.Forms.Timer { Interval = 15 };
        _animTimer.Tick += (_, _) => AnimTick();
    }

    public ImageAdjustments? Adjustments
    {
        get => _adj;
        set { _adj = value; _activeSpot = -1; Invalidate(); }
    }

    /// <summary>Set the heal brush size for new spots and live-resize the active spot (last placed
    /// / edited). Returns true when an existing spot was resized (caller should re-render).</summary>
    public bool SetHealBrushSize(double size)
    {
        HealBrushSize = size;
        if (_adj is null || _activeSpot < 0 || _activeSpot >= _adj.HealSpots.Count) return false;
        var s = _adj.HealSpots[_activeSpot];
        double rn = Math.Max(0.005, size / 500.0);
        if (Math.Abs(s.RadiusNorm - rn) < 1e-9) return false;
        s.RadiusNorm = rn; s.Radius = size;
        Invalidate();
        return true;
    }

    public void SetImage(Bitmap? image, bool resetView)
    {
        _image?.Dispose();
        _image = image;
        // Loading/refreshing a bitmap snaps to the target view — animation is only for user zoom gestures.
        if (resetView || _zoomMode == ZoomMode.Fit) { StopAnim(); ZoomFit(animate: false); }
        else ClampOffset();
        Invalidate();
    }

    // ---- zoom ------------------------------------------------------------

    public void ZoomFit() => ZoomFit(animate: true);

    private void ZoomFit(bool animate)
    {
        _zoomMode = ZoomMode.Fit;
        if (_image is null) { StopAnim(); _scale = 1; _offset = PointF.Empty; return; }
        double target = Math.Min((double)Width / _image.Width, (double)Height / _image.Height);
        if (target <= 0 || double.IsInfinity(target)) target = 1;
        var off = CenterOffsetFor(target);
        if (animate) AnimateTo(target, off);
        else { StopAnim(); _scale = target; _offset = off; Invalidate(); ViewChanged?.Invoke(this, EventArgs.Empty); }
    }

    public void Zoom100() => SetZoom(1.0, ZoomMode.Actual100);
    public void Zoom200() => SetZoom(2.0, ZoomMode.Actual200);

    private void SetZoom(double scale, ZoomMode mode)
    {
        _zoomMode = mode;
        var center = new PointF(Width / 2f, Height / 2f);
        ZoomAround(center, scale, animate: true);
    }

    /// <summary>Zoom keeping <paramref name="ctrlPt"/> fixed; optionally eased over ~150ms.</summary>
    private void ZoomAround(PointF ctrlPt, double newScale, bool animate)
    {
        if (_image is null) return;
        newScale = Math.Clamp(newScale, 0.02, 16);
        // Anchor against the pending target when mid-flight so rapid wheel ticks accumulate smoothly.
        double baseScale = _animating ? _animScaleTo : _scale;
        PointF baseOffset = _animating ? _animOffsetTo : _offset;
        double ix = (ctrlPt.X - baseOffset.X) / baseScale;
        double iy = (ctrlPt.Y - baseOffset.Y) / baseScale;
        var off = ClampOffsetFor(newScale, new PointF(
            (float)(ctrlPt.X - ix * newScale), (float)(ctrlPt.Y - iy * newScale)));
        if (animate) AnimateTo(newScale, off);
        else { StopAnim(); _scale = newScale; _offset = off; Invalidate(); ViewChanged?.Invoke(this, EventArgs.Empty); }
    }

    private PointF CenterOffsetFor(double scale) => _image is null ? PointF.Empty : new PointF(
        (float)((Width - _image.Width * scale) / 2),
        (float)((Height - _image.Height * scale) / 2));

    // ---- animation core --------------------------------------------------

    private void AnimateTo(double targetScale, PointF targetOffset)
    {
        // Skip animating imperceptible changes.
        if (Math.Abs(targetScale - _scale) < 1e-4 &&
            Math.Abs(targetOffset.X - _offset.X) < 0.5f && Math.Abs(targetOffset.Y - _offset.Y) < 0.5f)
        {
            _scale = targetScale; _offset = targetOffset; StopAnim();
            Invalidate(); ViewChanged?.Invoke(this, EventArgs.Empty);
            return;
        }
        _animScaleFrom = _scale; _animOffsetFrom = _offset;
        _animScaleTo = targetScale; _animOffsetTo = targetOffset;
        _animStartTick = Environment.TickCount64;
        _animating = true;
        _animTimer.Start();
    }

    private void AnimTick()
    {
        double t = (Environment.TickCount64 - _animStartTick) / (double)AnimDurationMs;
        if (t >= 1) { FinishAnim(); return; }
        double e = EaseOutCubic(t);
        _scale = _animScaleFrom + (_animScaleTo - _animScaleFrom) * e;
        _offset = new PointF(
            (float)(_animOffsetFrom.X + (_animOffsetTo.X - _animOffsetFrom.X) * e),
            (float)(_animOffsetFrom.Y + (_animOffsetTo.Y - _animOffsetFrom.Y) * e));
        Invalidate();
        ViewChanged?.Invoke(this, EventArgs.Empty);
    }

    private void FinishAnim()
    {
        _scale = _animScaleTo; _offset = _animOffsetTo;
        StopAnim();
        Invalidate();
        ViewChanged?.Invoke(this, EventArgs.Empty);
    }

    private void StopAnim() { _animating = false; _animTimer.Stop(); }

    private static double EaseOutCubic(double t) { t = Math.Clamp(t, 0, 1); double u = 1 - t; return 1 - u * u * u; }

    private void ClampOffset() { if (_image is not null) _offset = ClampOffsetFor(_scale, _offset); }

    private PointF ClampOffsetFor(double scale, PointF off)
    {
        if (_image is null) return off;
        float iw = (float)(_image.Width * scale), ih = (float)(_image.Height * scale);
        off.X = iw <= Width ? (Width - iw) / 2 : Math.Clamp(off.X, Width - iw, 0);
        off.Y = ih <= Height ? (Height - ih) / 2 : Math.Clamp(off.Y, Height - ih, 0);
        return off;
    }

    protected override void OnResize(EventArgs e)
    {
        base.OnResize(e);
        // Resize should track the layout instantly, not ease.
        if (_zoomMode == ZoomMode.Fit) ZoomFit(animate: false); else ClampOffset();
    }

    protected override void OnMouseWheel(MouseEventArgs e)
    {
        if (_image is not null)
        {
            _zoomMode = ZoomMode.Custom;
            double factor = e.Delta > 0 ? 1.15 : 1 / 1.15;
            double baseScale = _animating ? _animScaleTo : _scale;
            ZoomAround(e.Location, baseScale * factor, animate: true);
        }
        base.OnMouseWheel(e);
    }

    // ---- coordinate transforms ------------------------------------------

    private PointF ImgToCtrl(float fx, float fy) => new((float)(_offset.X + fx * _scale), (float)(_offset.Y + fy * _scale));
    private PointF NormToCtrl(double nx, double ny) => _image is null ? PointF.Empty : ImgToCtrl((float)(nx * _image.Width), (float)(ny * _image.Height));
    private (double nx, double ny) CtrlToNorm(Point p)
    {
        if (_image is null) return (0, 0);
        double ix = (p.X - _offset.X) / _scale / _image.Width;
        double iy = (p.Y - _offset.Y) / _scale / _image.Height;
        return (Math.Clamp(ix, 0, 1), Math.Clamp(iy, 0, 1));
    }

    private static float Dist(PointF a, PointF b) => (float)Math.Sqrt((a.X - b.X) * (a.X - b.X) + (a.Y - b.Y) * (a.Y - b.Y));

    // ---- mouse -----------------------------------------------------------

    protected override void OnMouseDown(MouseEventArgs e)
    {
        Focus();
        _dragStart = e.Location;
        _offsetStart = _offset;
        _dragMoved = false;
        if (_image is null) return;

        if (WhiteBalancePickerActive && e.Button == MouseButtons.Left)
        {
            var (nx, ny) = CtrlToNorm(e.Location);
            WhiteBalancePicked?.Invoke(nx, ny);
            return;
        }

        if (e.Button == MouseButtons.Middle) { StopAnim(); _drag = Drag.Pan; _offsetStart = _offset; return; }

        if (e.Button == MouseButtons.Left && _adj is not null)
        {
            switch (Tool)
            {
                case ToolMode.Crop: if (BeginCropDrag(e.Location)) return; break;
                case ToolMode.Gradient: if (BeginGradientDrag(e.Location)) return; break;
                case ToolMode.Heal: if (BeginHealDrag(e.Location)) return; break;
            }
        }

        if (e.Button == MouseButtons.Right && Tool == ToolMode.Heal && _adj is not null)
        {
            DeleteHealAt(e.Location);
            return;
        }

        if (e.Button == MouseButtons.Right && Tool == ToolMode.Gradient && _adj is not null)
        {
            int idx = GradientAtPoint(e.Location);
            if (idx >= 0) ShowGradientDeleteMenu(idx, e.Location);
            return;
        }

        // default: pan with left when no tool
        if (e.Button == MouseButtons.Left && Tool == ToolMode.None)
        {
            StopAnim(); _drag = Drag.Pan; _offsetStart = _offset;
        }
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        if (_drag == Drag.Pan)
        {
            int dx = e.X - _dragStart.X, dy = e.Y - _dragStart.Y;
            if (!_dragMoved && dx * dx + dy * dy < PanThreshold * PanThreshold) return; // ignore click jitter
            _dragMoved = true;
            _offset = new PointF(_offsetStart.X + dx, _offsetStart.Y + dy);
            // Keep the current zoom level (適合/100%/200%) so a following click continues the cycle;
            // only the wheel switches to a free/custom zoom.
            ClampOffset(); Invalidate(); return;
        }
        if (_drag != Drag.None && _adj is not null) { UpdateDrag(e.Location); return; }

        // cursor feedback
        Cursor = WhiteBalancePickerActive ? Cursors.Cross :
                 Tool == ToolMode.None ? Cursors.Hand : Cursors.Default;
        base.OnMouseMove(e);
    }

    protected override void OnMouseUp(MouseEventArgs e)
    {
        bool wasPan = _drag == Drag.Pan;
        _drag = Drag.None; _dragSpot = -1;
        // A left-click without dragging cycles the zoom (適合 → 100% → 200% → 適合).
        if (wasPan && !_dragMoved && e.Button == MouseButtons.Left && !WhiteBalancePickerActive)
            CycleZoomAt(e.Location);
        base.OnMouseUp(e);
    }

    private void CycleZoomAt(Point p)
    {
        if (_image is null) return;
        if (_zoomMode == ZoomMode.Fit) { _zoomMode = ZoomMode.Actual100; ZoomAround(p, 1.0, animate: true); }
        else if (_zoomMode == ZoomMode.Actual100) { _zoomMode = ZoomMode.Actual200; ZoomAround(p, 2.0, animate: true); }
        else ZoomFit(animate: true);
    }

    // ---- crop interaction ------------------------------------------------

    private RectangleF CropCtrlRect()
    {
        var tl = NormToCtrl(_adj!.CropX, _adj.CropY);
        var br = NormToCtrl(_adj.CropX + _adj.CropWidth, _adj.CropY + _adj.CropHeight);
        return RectangleF.FromLTRB(tl.X, tl.Y, br.X, br.Y);
    }

    private bool BeginCropDrag(Point p)
    {
        var r = CropCtrlRect();
        const float h = 10;
        bool L = Math.Abs(p.X - r.Left) < h, R = Math.Abs(p.X - r.Right) < h;
        bool T = Math.Abs(p.Y - r.Top) < h, B = Math.Abs(p.Y - r.Bottom) < h;
        bool inX = p.X > r.Left - h && p.X < r.Right + h;
        bool inY = p.Y > r.Top - h && p.Y < r.Bottom + h;
        Drag d = Drag.None;
        if (L && T) d = Drag.CropTL; else if (R && T) d = Drag.CropTR;
        else if (L && B) d = Drag.CropBL; else if (R && B) d = Drag.CropBR;
        else if (L && inY) d = Drag.CropL; else if (R && inY) d = Drag.CropR;
        else if (T && inX) d = Drag.CropT; else if (B && inX) d = Drag.CropB;
        else if (r.Contains(p)) d = Drag.CropMove;
        if (d == Drag.None) return false;
        _drag = d; _cropStart = new RectangleF((float)_adj!.CropX, (float)_adj.CropY, (float)_adj.CropWidth, (float)_adj.CropHeight);
        EditBegin?.Invoke(this, EventArgs.Empty);
        return true;
    }

    private void UpdateDrag(Point p)
    {
        var (nx, ny) = CtrlToNorm(p);
        switch (_drag)
        {
            case Drag.CropMove:
            {
                var (sx, sy) = CtrlToNorm(_dragStart);
                double dx = nx - sx, dy = ny - sy;
                double x = Math.Clamp(_cropStart.X + dx, 0, 1 - _cropStart.Width);
                double y = Math.Clamp(_cropStart.Y + dy, 0, 1 - _cropStart.Height);
                _adj!.CropX = x; _adj.CropY = y;
                break;
            }
            case Drag.CropL or Drag.CropR or Drag.CropT or Drag.CropB
                or Drag.CropTL or Drag.CropTR or Drag.CropBL or Drag.CropBR:
                DragCropHandle(_drag, nx, ny); break;

            case Drag.GradCenter:
            {
                var gr = _adj!.ActiveGradient;
                if (gr != null) { gr.CenterX = Math.Clamp(nx, 0, 1); gr.CenterY = Math.Clamp(ny, 0, 1); }
                break;
            }
            case Drag.GradRange:
            {
                var gr = _adj!.ActiveGradient;
                if (gr != null)
                {
                    var c = NormToCtrl(gr.CenterX, gr.CenterY);
                    gr.Range = Math.Clamp(Dist(c, p) / (float)(_image!.Height * _scale), 0.02, 1.0);
                }
                break;
            }
            case Drag.GradRotate:
            {
                var gr = _adj!.ActiveGradient;
                if (gr != null)
                {
                    var c = NormToCtrl(gr.CenterX, gr.CenterY);
                    gr.Angle = Math.Atan2(-(p.Y - c.Y), p.X - c.X) * 180 / Math.PI;
                }
                break;
            }
            case Drag.HealTarget when _dragSpot >= 0:
                _adj!.HealSpots[_dragSpot].TargetX = nx; _adj.HealSpots[_dragSpot].TargetY = ny; break;
            case Drag.HealSource when _dragSpot >= 0:
                _adj!.HealSpots[_dragSpot].SourceX = nx; _adj.HealSpots[_dragSpot].SourceY = ny; break;
        }
        Invalidate();
        EditChanged?.Invoke(this, EventArgs.Empty);
    }

    private void SetCropEdge(double? left = null, double? right = null, double? top = null, double? bottom = null)
    {
        double x0 = _adj!.CropX, y0 = _adj.CropY, x1 = _adj.CropX + _adj.CropWidth, y1 = _adj.CropY + _adj.CropHeight;
        const double minSize = 0.05;
        if (left is double l) x0 = Math.Clamp(l, 0, x1 - minSize);
        if (right is double r) x1 = Math.Clamp(r, x0 + minSize, 1);
        if (top is double t) y0 = Math.Clamp(t, 0, y1 - minSize);
        if (bottom is double b) y1 = Math.Clamp(b, y0 + minSize, 1);
        _adj.CropX = x0; _adj.CropY = y0; _adj.CropWidth = x1 - x0; _adj.CropHeight = y1 - y0;
    }

    /// <summary>The locked crop aspect (pixel width / height). "原始"(Original) locks to the
    /// displayed photo's own proportions; 0 only when there's nothing to measure.</summary>
    private double CropAspect()
    {
        if (_adj is null || _image is null || _image.Height <= 0) return 0;
        string a = _adj.CropAspectRatio;
        if (string.IsNullOrEmpty(a) || a == "Original") return (double)_image.Width / _image.Height;
        var parts = a.Split(':');
        if (parts.Length == 2 && double.TryParse(parts[0], out var w) && double.TryParse(parts[1], out var h) && w > 0 && h > 0)
            return w / h;
        return 0;
    }

    /// <summary>Resize the crop box from an edge/corner handle. When an aspect ratio is locked the
    /// box keeps that ratio (corners pivot on the opposite corner; edges grow centered).</summary>
    private void DragCropHandle(Drag mode, double nx, double ny)
    {
        double ratio = CropAspect();
        if (ratio <= 0)
        {
            switch (mode)
            {
                case Drag.CropL: SetCropEdge(left: nx); break;
                case Drag.CropR: SetCropEdge(right: nx); break;
                case Drag.CropT: SetCropEdge(top: ny); break;
                case Drag.CropB: SetCropEdge(bottom: ny); break;
                case Drag.CropTL: SetCropEdge(left: nx, top: ny); break;
                case Drag.CropTR: SetCropEdge(right: nx, top: ny); break;
                case Drag.CropBL: SetCropEdge(left: nx, bottom: ny); break;
                case Drag.CropBR: SetCropEdge(right: nx, bottom: ny); break;
            }
            return;
        }

        double iw = _image!.Width, ih = _image.Height;
        double x0 = _adj!.CropX, y0 = _adj.CropY, x1 = x0 + _adj.CropWidth, y1 = y0 + _adj.CropHeight;
        double cx = x0 + _adj.CropWidth / 2, cy = y0 + _adj.CropHeight / 2;
        const double minN = 0.05;
        double k = iw / (ih * ratio);   // normalized height = normalized width * k (keeps pixel ratio)

        switch (mode)
        {
            // ---- corners: opposite corner stays fixed ----
            case Drag.CropBR:
            {
                double wN = Math.Max(minN, nx - x0), hN = wN * k;
                if (wN > 1 - x0) { wN = 1 - x0; hN = wN * k; }
                if (hN > 1 - y0) { hN = 1 - y0; wN = hN / k; }
                _adj.CropX = x0; _adj.CropY = y0; _adj.CropWidth = wN; _adj.CropHeight = hN; break;
            }
            case Drag.CropTL:
            {
                double wN = Math.Max(minN, x1 - nx), hN = wN * k;
                if (wN > x1) { wN = x1; hN = wN * k; }
                if (hN > y1) { hN = y1; wN = hN / k; }
                _adj.CropX = x1 - wN; _adj.CropY = y1 - hN; _adj.CropWidth = wN; _adj.CropHeight = hN; break;
            }
            case Drag.CropTR:
            {
                double wN = Math.Max(minN, nx - x0), hN = wN * k;
                if (wN > 1 - x0) { wN = 1 - x0; hN = wN * k; }
                if (hN > y1) { hN = y1; wN = hN / k; }
                _adj.CropX = x0; _adj.CropY = y1 - hN; _adj.CropWidth = wN; _adj.CropHeight = hN; break;
            }
            case Drag.CropBL:
            {
                double wN = Math.Max(minN, x1 - nx), hN = wN * k;
                if (wN > x1) { wN = x1; hN = wN * k; }
                if (hN > 1 - y0) { hN = 1 - y0; wN = hN / k; }
                _adj.CropX = x1 - wN; _adj.CropY = y0; _adj.CropWidth = wN; _adj.CropHeight = hN; break;
            }
            // ---- edges: perpendicular dimension grows centered ----
            case Drag.CropL:
            case Drag.CropR:
            {
                double wN = mode == Drag.CropL ? Math.Max(minN, Math.Min(x1, x1 - nx)) : Math.Max(minN, Math.Min(1 - x0, nx - x0));
                double hN = wN * k;
                if (hN > 1) { hN = 1; wN = hN / k; }
                double ny0 = Math.Clamp(cy - hN / 2, 0, 1 - hN);
                if (mode == Drag.CropL) _adj.CropX = x1 - wN; else _adj.CropX = x0;
                _adj.CropWidth = wN; _adj.CropY = ny0; _adj.CropHeight = hN; break;
            }
            case Drag.CropT:
            case Drag.CropB:
            {
                double hN = mode == Drag.CropT ? Math.Max(minN, Math.Min(y1, y1 - ny)) : Math.Max(minN, Math.Min(1 - y0, ny - y0));
                double wN = hN / k;
                if (wN > 1) { wN = 1; hN = wN * k; }
                double nx0 = Math.Clamp(cx - wN / 2, 0, 1 - wN);
                if (mode == Drag.CropT) _adj.CropY = y1 - hN; else _adj.CropY = y0;
                _adj.CropHeight = hN; _adj.CropX = nx0; _adj.CropWidth = wN; break;
            }
        }
    }

    // ---- gradient interaction -------------------------------------------

    private const float RotHandleDist = 64f;   // blue rotate handle: fixed screen offset, to the right of白點

    private bool BeginGradientDrag(Point p)
    {
        // On the active gradient, grab its yellow (range) or blue (rotate) handle first.
        var g = _adj!.ActiveGradient;
        if (g != null)
        {
            var center = NormToCtrl(g.CenterX, g.CenterY);
            var (ux, uy) = GradAxis(g);
            var rangePt = new PointF(center.X + (float)(ux * g.Range * _image!.Height * _scale),
                                     center.Y + (float)(uy * g.Range * _image.Height * _scale));
            var rotatePt = RotateHandlePos(center, g);
            Drag d = Dist(p, rotatePt) < 12 ? Drag.GradRotate :
                     Dist(p, rangePt) < 12 ? Drag.GradRange : Drag.None;
            if (d != Drag.None) { _drag = d; EditBegin?.Invoke(this, EventArgs.Empty); return true; }
        }

        // Otherwise, clicking any gradient's white dot selects it (and starts a move).
        int idx = GradientAtPoint(p);
        if (idx >= 0)
        {
            if (idx != _adj.ActiveGradientIndex) { _adj.ActiveGradientIndex = idx; GradientSelectionChanged?.Invoke(); }
            _drag = Drag.GradCenter;
            EditBegin?.Invoke(this, EventArgs.Empty);
            return true;
        }
        // Nothing hit — do not create here (新增線性漸層 button adds one).
        return false;
    }

    /// <summary>Index of the gradient whose white (center) handle is under <paramref name="p"/>, or -1.</summary>
    private int GradientAtPoint(Point p)
    {
        for (int i = 0; i < _adj!.Gradients.Count; i++)
            if (Dist(p, NormToCtrl(_adj.Gradients[i].CenterX, _adj.Gradients[i].CenterY)) < 12) return i;
        return -1;
    }

    private static (double ux, double uy) GradAxis(LinearGradient g)
    {
        double a = g.Angle * Math.PI / 180;
        return (Math.Sin(a), Math.Cos(a));   // axis of variation (control coords, Y down)
    }

    /// <summary>Blue rotate handle position: a fixed offset to the right of the white handle,
    /// along the gradient line (rotates with the gradient).</summary>
    private static PointF RotateHandlePos(PointF center, LinearGradient g)
    {
        double a = g.Angle * Math.PI / 180;
        return new PointF(center.X + (float)(Math.Cos(a) * RotHandleDist),
                          center.Y - (float)(Math.Sin(a) * RotHandleDist));
    }

    private void ShowGradientDeleteMenu(int index, Point p)
    {
        var menu = new ContextMenuStrip { BackColor = Theme.PanelBg2, ForeColor = Theme.Text };
        var del = new ToolStripMenuItem(L.T("刪除此線性漸層")) { ForeColor = Theme.Text };
        del.Click += (_, _) =>
        {
            if (_adj == null || index < 0 || index >= _adj.Gradients.Count) return;
            EditBegin?.Invoke(this, EventArgs.Empty);
            _adj.Gradients.RemoveAt(index);
            _adj.ActiveGradientIndex = _adj.Gradients.Count - 1;
            GradientSelectionChanged?.Invoke();
            Invalidate();
            EditChanged?.Invoke(this, EventArgs.Empty);
        };
        menu.Items.Add(del);
        menu.Show(this, p);
    }

    // ---- heal interaction ------------------------------------------------

    private bool BeginHealDrag(Point p)
    {
        for (int i = 0; i < _adj!.HealSpots.Count; i++)
        {
            var s = _adj.HealSpots[i];
            float rpx = (float)(s.RadiusNorm * Math.Max(_image!.Width, _image.Height) * _scale);
            if (Dist(p, NormToCtrl(s.TargetX, s.TargetY)) < Math.Max(8, rpx)) { _drag = Drag.HealTarget; _dragSpot = _activeSpot = i; EditBegin?.Invoke(this, EventArgs.Empty); return true; }
            if (!s.UseInpaint && Dist(p, NormToCtrl(s.SourceX, s.SourceY)) < Math.Max(8, rpx)) { _drag = Drag.HealSource; _dragSpot = _activeSpot = i; EditBegin?.Invoke(this, EventArgs.Empty); return true; }
        }
        // add new spot
        var (nx, ny) = CtrlToNorm(p);
        double radiusNorm = Math.Max(0.005, HealBrushSize / 500.0);
        var spot = new HealSpot
        {
            TargetX = nx, TargetY = ny,
            SourceX = Math.Clamp(nx - 0.06, 0, 1), SourceY = ny,
            RadiusNorm = radiusNorm, Radius = HealBrushSize,
            UseInpaint = HealMode == HealMode.Inpaint
        };
        EditBegin?.Invoke(this, EventArgs.Empty);
        _adj.HealSpots.Add(spot);
        _drag = Drag.HealTarget; _dragSpot = _activeSpot = _adj.HealSpots.Count - 1;
        Invalidate(); EditChanged?.Invoke(this, EventArgs.Empty);
        return true;
    }

    private void DeleteHealAt(Point p)
    {
        for (int i = 0; i < _adj!.HealSpots.Count; i++)
        {
            var s = _adj.HealSpots[i];
            float rpx = (float)(s.RadiusNorm * Math.Max(_image!.Width, _image.Height) * _scale);
            if (Dist(p, NormToCtrl(s.TargetX, s.TargetY)) < Math.Max(8, rpx))
            {
                EditBegin?.Invoke(this, EventArgs.Empty);
                _adj.HealSpots.RemoveAt(i);
                _activeSpot = _adj.HealSpots.Count - 1;   // fall back to the previous spot
                Invalidate(); EditChanged?.Invoke(this, EventArgs.Empty);
                return;
            }
        }
    }

    // ---- paint -----------------------------------------------------------

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        g.Clear(Theme.ViewerBg);
        if (_image is null)
        {
            TextRenderer.DrawText(g, L.T("選擇一張相片開始編輯"), Theme.Normal,
                ClientRectangle, Theme.TextFaint,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
            return;
        }

        g.InterpolationMode = _scale >= 2 ? InterpolationMode.NearestNeighbor : InterpolationMode.HighQualityBicubic;
        g.PixelOffsetMode = PixelOffsetMode.Half;
        var dst = new RectangleF(_offset.X, _offset.Y, (float)(_image.Width * _scale), (float)(_image.Height * _scale));
        g.DrawImage(_image, dst);

        if (_adj is null) return;
        g.SmoothingMode = SmoothingMode.AntiAlias;
        switch (Tool)
        {
            case ToolMode.Crop: DrawCropOverlay(g); break;
            case ToolMode.Gradient: DrawGradientOverlay(g); break;
            case ToolMode.Heal: DrawHealOverlay(g); break;
        }
        if (WhiteBalancePickerActive)
            TextRenderer.DrawText(g, L.T("點擊中性灰色區域設定白平衡"), Theme.Normal,
                new Rectangle(0, 8, Width, 20), Theme.Text, TextFormatFlags.HorizontalCenter);
    }

    private void DrawCropOverlay(Graphics g)
    {
        var r = CropCtrlRect();
        using (var dim = new SolidBrush(Color.FromArgb(140, 0, 0, 0)))
        {
            var full = new RectangleF(_offset.X, _offset.Y, (float)(_image!.Width * _scale), (float)(_image.Height * _scale));
            var region = new Region(full);
            region.Exclude(r);
            g.FillRegion(dim, region);
            region.Dispose();
        }
        using var pen = new Pen(Color.White, 1.5f);
        g.DrawRectangle(pen, r.X, r.Y, r.Width, r.Height);
        // thirds
        using var thin = new Pen(Color.FromArgb(120, 255, 255, 255));
        for (int i = 1; i < 3; i++)
        {
            float x = r.X + r.Width * i / 3, y = r.Y + r.Height * i / 3;
            g.DrawLine(thin, x, r.Y, x, r.Bottom);
            g.DrawLine(thin, r.X, y, r.Right, y);
        }
        // handles
        using var hb = new SolidBrush(Color.White);
        foreach (var pt in new[] { new PointF(r.Left, r.Top), new PointF(r.Right, r.Top),
            new PointF(r.Left, r.Bottom), new PointF(r.Right, r.Bottom),
            new PointF(r.X + r.Width / 2, r.Top), new PointF(r.X + r.Width / 2, r.Bottom),
            new PointF(r.Left, r.Y + r.Height / 2), new PointF(r.Right, r.Y + r.Height / 2) })
            g.FillRectangle(hb, pt.X - 3, pt.Y - 3, 6, 6);
    }

    private void DrawGradientOverlay(Graphics g)
    {
        var active = _adj!.ActiveGradient;   // normalizes ActiveGradientIndex
        int ai = _adj.ActiveGradientIndex;
        // Inactive gradients first, active one on top.
        for (int i = 0; i < _adj.Gradients.Count; i++)
            if (i != ai) DrawOneGradient(g, _adj.Gradients[i], false);
        if (active != null) DrawOneGradient(g, active, true);
    }

    private void DrawOneGradient(Graphics g, LinearGradient gr, bool isActive)
    {
        var center = NormToCtrl(gr.CenterX, gr.CenterY);
        var (ux, uy) = GradAxis(gr);
        float len = Math.Max(Width, Height);
        var v = new PointF((float)-uy, (float)ux);   // gradient line direction (perpendicular to axis)
        PointF P(double t) => new((float)(center.X + v.X * t), (float)(center.Y + v.Y * t));

        using (var pen = new Pen(Color.FromArgb(isActive ? 230 : 100, 255, 220, 60), isActive ? 1.6f : 1.2f))
            g.DrawLine(pen, P(-len), P(len));

        if (!isActive)
        {
            // A small white dot so an inactive gradient can be clicked to select it.
            using var wb = new SolidBrush(Color.FromArgb(200, 255, 255, 255));
            g.FillEllipse(wb, center.X - 5, center.Y - 5, 10, 10);
            return;
        }

        // Yellow range band (dashed edges + a yellow handle marking the falloff distance).
        double rangePx = gr.Range * _image!.Height * _scale;
        using (var pen = new Pen(Color.FromArgb(120, 255, 220, 60), 1f) { DashStyle = DashStyle.Dash })
        {
            var c1 = new PointF(center.X + (float)(ux * rangePx), center.Y + (float)(uy * rangePx));
            var c2 = new PointF(center.X - (float)(ux * rangePx), center.Y - (float)(uy * rangePx));
            g.DrawLine(pen, c1.X + v.X * -len, c1.Y + v.Y * -len, c1.X + v.X * len, c1.Y + v.Y * len);
            g.DrawLine(pen, c2.X + v.X * -len, c2.Y + v.Y * -len, c2.X + v.X * len, c2.Y + v.Y * len);
            using var hb = new SolidBrush(Color.FromArgb(255, 255, 220, 60));
            g.FillEllipse(hb, c1.X - 5, c1.Y - 5, 10, 10);
        }
        // Blue rotate handle, to the right of白點.
        var rot = RotateHandlePos(center, gr);
        using (var pen = new Pen(Color.FromArgb(200, 120, 200, 255), 1f)) g.DrawLine(pen, center, rot);
        using (var hb = new SolidBrush(Color.FromArgb(255, 120, 200, 255))) g.FillEllipse(hb, rot.X - 5, rot.Y - 5, 10, 10);
        // White position handle on top.
        using (var hb = new SolidBrush(Color.White))
            g.FillEllipse(hb, center.X - 6, center.Y - 6, 12, 12);
    }

    private void DrawHealOverlay(Graphics g)
    {
        foreach (var s in _adj!.HealSpots)
        {
            float rpx = (float)(s.RadiusNorm * Math.Max(_image!.Width, _image.Height) * _scale);
            var tc = NormToCtrl(s.TargetX, s.TargetY);
            using (var pen = new Pen(Color.FromArgb(230, 90, 200, 120), 1.6f))
                g.DrawEllipse(pen, tc.X - rpx, tc.Y - rpx, rpx * 2, rpx * 2);
            if (!s.UseInpaint)
            {
                var sc = NormToCtrl(s.SourceX, s.SourceY);
                using var pen = new Pen(Color.FromArgb(200, 120, 180, 240), 1.4f) { DashStyle = DashStyle.Dash };
                g.DrawEllipse(pen, sc.X - rpx, sc.Y - rpx, rpx * 2, rpx * 2);
                g.DrawLine(pen, sc, tc);
            }
        }
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing) { _animTimer.Dispose(); _image?.Dispose(); }
        base.Dispose(disposing);
    }
}
