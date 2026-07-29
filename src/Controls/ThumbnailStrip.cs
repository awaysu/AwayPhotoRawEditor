using System.Collections.Generic;
using System.Drawing;
using System.Linq;
using System.Windows.Forms;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Controls;

/// <summary>
/// Horizontal thumbnail strip with multi-selection (Ctrl/Shift), status badges
/// (edited / virtual copy / copy-source), a custom horizontal scrollbar and
/// right-click support. Thumbnails are supplied asynchronously via <see cref="SetImage"/>.
/// </summary>
public sealed class ThumbnailStrip : Control
{
    private sealed class Cell { public required PhotoItem Item; public Bitmap? Image; }

    private readonly List<Cell> _cells = new();
    private readonly HashSet<int> _selected = new();
    private int _anchor = -1;
    private int _current = -1;
    private int _scrollX;
    private int _hoverIndex = -1;
    private bool _draggingScroll;
    private int _scrollDragOffset;

    private const int CellW = 180;
    private const int TextH = 18;
    private const int BarH = 16;

    private bool _showNumber = true;

    public event EventHandler? SelectionChanged;
    public event Action<PhotoItem>? ItemActivated;
    public event Action<PhotoItem, Point>? ItemRightClicked;

    /// <summary>Whether to draw the #1.. index number on each thumbnail.</summary>
    public bool ShowNumber
    {
        get => _showNumber;
        set { if (_showNumber != value) { _showNumber = value; Invalidate(); } }
    }

    public ThumbnailStrip()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw |
                 ControlStyles.Selectable, true);
        BackColor = Theme.PanelBg2;
        Height = 104;
    }

    public IReadOnlyList<PhotoItem> Items => _cells.Select(c => c.Item).ToList();
    public IReadOnlyList<PhotoItem> SelectedItems => _selected.OrderBy(i => i).Select(i => _cells[i].Item).ToList();
    public PhotoItem? CurrentItem => _current >= 0 && _current < _cells.Count ? _cells[_current].Item : null;
    public int Count => _cells.Count;

    public void SetItems(IEnumerable<PhotoItem> items, int selectIndex = 0)
    {
        foreach (var c in _cells) c.Image?.Dispose();
        _cells.Clear();
        _selected.Clear();
        foreach (var it in items) _cells.Add(new Cell { Item = it });
        _current = _cells.Count > 0 ? Math.Clamp(selectIndex, 0, _cells.Count - 1) : -1;
        if (_current >= 0) _selected.Add(_current);
        _anchor = _current;
        _scrollX = 0;
        Invalidate();
        SelectionChanged?.Invoke(this, EventArgs.Empty);
    }

    /// <summary>Attach (or replace) the thumbnail image for a photo key.</summary>
    public void SetImage(string key, Bitmap image)
    {
        var cell = _cells.FirstOrDefault(c => c.Item.Key == key);
        if (cell is null) { image.Dispose(); return; }
        cell.Image?.Dispose();
        cell.Image = image;
        Invalidate();
    }

    public void RefreshBadges() => Invalidate();

    public void SelectIndex(int index, bool scroll = true)
    {
        if (index < 0 || index >= _cells.Count) return;
        _selected.Clear();
        _selected.Add(index);
        _current = _anchor = index;
        if (scroll) EnsureVisible(index);
        Invalidate();
        SelectionChanged?.Invoke(this, EventArgs.Empty);
    }

    /// <summary>Select every thumbnail. The current photo is left unchanged.</summary>
    public void SelectAll()
    {
        if (_cells.Count == 0) return;
        _selected.Clear();
        for (int i = 0; i < _cells.Count; i++) _selected.Add(i);
        Invalidate();
        SelectionChanged?.Invoke(this, EventArgs.Empty);
    }

    /// <summary>Toggle the selection state of every thumbnail.</summary>
    public void InvertSelection()
    {
        if (_cells.Count == 0) return;
        for (int i = 0; i < _cells.Count; i++) { if (!_selected.Remove(i)) _selected.Add(i); }
        Invalidate();
        SelectionChanged?.Invoke(this, EventArgs.Empty);
    }

    /// <summary>Clear the selection (the current photo stays loaded in the viewer).</summary>
    public void DeselectAll()
    {
        if (_selected.Count == 0) return;
        _selected.Clear();
        Invalidate();
        SelectionChanged?.Invoke(this, EventArgs.Empty);
    }

    // ---- layout / scrolling ---------------------------------------------

    private int ContentWidth => _cells.Count * CellW;
    private int MaxScroll => Math.Max(0, ContentWidth - Width);
    private int CellImageH => Height - TextH - BarH - 6;

    private Rectangle CellRect(int i) => new(i * CellW - _scrollX, 2, CellW, Height - BarH - 2);
    private int IndexAt(Point p)
    {
        if (p.Y > Height - BarH) return -1;
        int i = (p.X + _scrollX) / CellW;
        return i >= 0 && i < _cells.Count ? i : -1;
    }

    private void EnsureVisible(int i)
    {
        int left = i * CellW, right = left + CellW;
        if (left - _scrollX < 0) _scrollX = left;
        else if (right - _scrollX > Width) _scrollX = right - Width;
        _scrollX = Math.Clamp(_scrollX, 0, MaxScroll);
    }

    protected override void OnMouseWheel(MouseEventArgs e)
    {
        _scrollX = Math.Clamp(_scrollX - e.Delta, 0, MaxScroll);
        UpdateHover(e.Location);
        Invalidate();
        base.OnMouseWheel(e);
    }

    // ---- selection / mouse ----------------------------------------------

    protected override void OnMouseDown(MouseEventArgs e)
    {
        Focus();
        // scrollbar
        var sb = ScrollThumbRect();
        if (e.Y > Height - BarH)
        {
            if (sb.Contains(e.Location)) { _draggingScroll = true; _scrollDragOffset = e.X - sb.X; }
            else if (MaxScroll > 0) _scrollX = Math.Clamp((int)((e.X / (float)Width) * ContentWidth) - Width / 2, 0, MaxScroll);
            Invalidate();
            return;
        }

        int i = IndexAt(e.Location);
        if (e.Button == MouseButtons.Right)
        {
            if (i >= 0)
            {
                if (!_selected.Contains(i)) { _selected.Clear(); _selected.Add(i); _current = _anchor = i; }
                Invalidate(); SelectionChanged?.Invoke(this, EventArgs.Empty);
                ItemRightClicked?.Invoke(_cells[i].Item, PointToScreen(e.Location));
            }
            return;
        }
        if (e.Button != MouseButtons.Left || i < 0) return;

        bool ctrl = (ModifierKeys & Keys.Control) != 0;
        bool shift = (ModifierKeys & Keys.Shift) != 0;
        if (shift && _anchor >= 0)
        {
            _selected.Clear();
            for (int k = Math.Min(_anchor, i); k <= Math.Max(_anchor, i); k++) _selected.Add(k);
            _current = i;
        }
        else if (ctrl)
        {
            if (!_selected.Add(i)) _selected.Remove(i);
            _current = _anchor = i;
        }
        else
        {
            _selected.Clear(); _selected.Add(i); _current = _anchor = i;
        }
        Invalidate();
        SelectionChanged?.Invoke(this, EventArgs.Empty);
    }

    protected override void OnMouseDoubleClick(MouseEventArgs e)
    {
        int i = IndexAt(e.Location);
        if (i >= 0) ItemActivated?.Invoke(_cells[i].Item);
        base.OnMouseDoubleClick(e);
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        if (_draggingScroll)
        {
            float ratio = ContentWidth / (float)Width;
            _scrollX = Math.Clamp((int)((e.X - _scrollDragOffset) * ratio), 0, MaxScroll);
            Invalidate();
            return;
        }
        UpdateHover(e.Location);
        base.OnMouseMove(e);
    }

    private void UpdateHover(Point p)
    {
        int i = IndexAt(p);
        if (i != _hoverIndex) { _hoverIndex = i; Invalidate(); }
    }

    protected override void OnMouseUp(MouseEventArgs e) { _draggingScroll = false; base.OnMouseUp(e); }
    protected override void OnMouseLeave(EventArgs e) { _hoverIndex = -1; Invalidate(); base.OnMouseLeave(e); }

    // ---- keyboard --------------------------------------------------------

    protected override void OnKeyDown(KeyEventArgs e)
    {
        if (e.Control && e.KeyCode == Keys.A)          // Ctrl+A：全選縮圖
        {
            SelectAll();
            e.Handled = e.SuppressKeyPress = true;
            return;
        }
        base.OnKeyDown(e);
    }

    private Rectangle ScrollThumbRect()
    {
        if (MaxScroll <= 0) return Rectangle.Empty;
        float frac = Width / (float)ContentWidth;
        int tw = Math.Max(30, (int)(Width * frac));
        int tx = (int)((_scrollX / (float)MaxScroll) * (Width - tw));
        return new Rectangle(tx, Height - BarH + 1, tw, BarH - 2);
    }

    // ---- paint -----------------------------------------------------------

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        g.Clear(Theme.PanelBg2);

        int first = Math.Max(0, _scrollX / CellW);
        int last = Math.Min(_cells.Count - 1, (_scrollX + Width) / CellW);
        for (int i = first; i <= last; i++) DrawCell(g, i);

        // scrollbar
        if (MaxScroll > 0)
        {
            using var track = new SolidBrush(Theme.PanelBg);
            g.FillRectangle(track, 0, Height - BarH, Width, BarH);
            PaintHelpers.FillRounded(g, ScrollThumbRect(), 3, Theme.BorderLight);
        }
    }

    private void DrawCell(Graphics g, int i)
    {
        var cell = _cells[i];
        var rect = CellRect(i);
        bool selected = _selected.Contains(i);

        var pad = new Rectangle(rect.X + 4, rect.Y + 2, rect.Width - 8, CellImageH);
        using (var bg = new SolidBrush(selected ? Theme.PanelBg3 : (i == _hoverIndex ? Theme.PanelBg : Theme.PanelBg2)))
            g.FillRectangle(bg, rect.X + 2, rect.Y, rect.Width - 4, rect.Height);

        // image
        Rectangle photoRect = pad;
        if (cell.Image is { } img)
        {
            photoRect = FitRect(img.Size, pad);
            g.DrawImage(img, photoRect);
        }
        else
        {
            using var b = new SolidBrush(Theme.ViewerBg);
            g.FillRectangle(b, pad);
            TextRenderer.DrawText(g, "…", Theme.Normal, pad, Theme.TextFaint,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
        }

        // filename
        TextRenderer.DrawText(g, cell.Item.FileName, Theme.Small,
            new Rectangle(rect.X + 4, rect.Bottom - TextH, rect.Width - 8, TextH),
            selected ? Theme.Text : Theme.TextDim,
            TextFormatFlags.HorizontalCenter | TextFormatFlags.EndEllipsis);

        // selection border
        if (selected)
            using (var pen = new Pen(Theme.Accent, 2))
                g.DrawRectangle(pen, rect.X + 2, rect.Y + 1, rect.Width - 5, rect.Height - 2);

        // index number (top-left, starting from #1) — DisplayNumber counts the folder's full
        // list including hidden photos, so numbers skip when a photo is hidden (#1, #3…).
        if (_showNumber)
        {
            string num = "#" + (cell.Item.DisplayNumber > 0 ? cell.Item.DisplayNumber : i + 1);
            var numSize = TextRenderer.MeasureText(num, Theme.Small);
            var numRect = new Rectangle(photoRect.X + 3, photoRect.Y + 3, numSize.Width + 8, 15);
            PaintHelpers.FillRounded(g, numRect, 3, Color.FromArgb(185, 90, 90, 98));
            TextRenderer.DrawText(g, num, Theme.Small, numRect,
                selected ? Color.White : Color.FromArgb(235, 235, 240),
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
        }

        // status markers (top-right, right-aligned)
        int rx = photoRect.Right - 4, ry = photoRect.Y + 4;
        if (cell.Item.IsHidden)
        {
            // 隱藏 icon：劃了斜線的眼睛（隱藏且不輸出的照片，「顯示全部」模式下可見）。
            var r = new Rectangle(rx - 18, ry, 18, 13);
            PaintHelpers.FillRounded(g, r, 3, Color.FromArgb(205, 40, 40, 46));
            using (var pen = new Pen(Color.FromArgb(240, 240, 244), 1.4f))
            {
                g.DrawEllipse(pen, r.X + 4f, r.Y + 3.5f, 10f, 6f);
                g.DrawLine(pen, r.X + 3f, r.Bottom - 2f, r.Right - 3f, r.Y + 2f);
            }
            rx -= 22;
        }
        if (cell.Item.IsVirtualCopy)
        {
            var r = new Rectangle(rx - 30, ry, 30, 12);
            PaintHelpers.FillRounded(g, r, 3, Theme.CopyBadge);
            TextRenderer.DrawText(g, "copy", Theme.Small, r, Color.Black,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
            rx -= 34;
        }
        if (cell.Item.IsEdited)
        {
            using var b = new SolidBrush(Theme.EditedBadge);
            g.FillEllipse(b, rx - 9, ry, 9, 9);
            rx -= 13;
        }
        if (cell.Item.IsCopySettingsSource)
            using (var pen = new Pen(Theme.CopyBadge, 2) { DashStyle = System.Drawing.Drawing2D.DashStyle.Dash })
                g.DrawRectangle(pen, photoRect.X + 1, photoRect.Y + 1, photoRect.Width - 2, photoRect.Height - 2);
    }

    private static Rectangle FitRect(Size src, Rectangle bounds)
    {
        double scale = Math.Min((double)bounds.Width / src.Width, (double)bounds.Height / src.Height);
        int w = Math.Max(1, (int)(src.Width * scale));
        int h = Math.Max(1, (int)(src.Height * scale));
        return new Rectangle(bounds.X + (bounds.Width - w) / 2, bounds.Y + (bounds.Height - h) / 2, w, h);
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing) foreach (var c in _cells) c.Image?.Dispose();
        base.Dispose(disposing);
    }
}
