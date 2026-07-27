using System;
using System.Collections.Generic;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;

namespace AwayPhotoRawEditor.Forms;

/// <summary>
/// Custom dark folder chooser: a quick-access sidebar (桌面 / 圖片 / 下載 / 文件 / 本機),
/// a path bar with an "up" button and a large, hover-highlighted folder list.
/// RAW_TEMP folders are never listed. OK returns the current folder.
/// </summary>
public sealed class FolderPickerForm : Form
{
    private readonly FolderList _list;
    private readonly Label _path;
    private readonly Label _picked;
    private string _current = "";

    public string SelectedPath { get; private set; } = "";

    public FolderPickerForm(string? initial = null)
    {
        Text = "選擇相片資料夾";
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        ClientSize = new Size(680, 500);
        StartPosition = FormStartPosition.CenterParent;
        MinimumSize = new Size(560, 420);

        // ---- bottom action bar ----
        var bottom = new Panel { Dock = DockStyle.Bottom, Height = 56, BackColor = Theme.PanelBg2 };
        _picked = new Label
        {
            Left = 16, Top = 0, Height = 56, Width = 340, ForeColor = Theme.TextDim, Font = Theme.Normal,
            TextAlign = ContentAlignment.MiddleLeft, AutoEllipsis = true
        };
        var ok = new FlatButton { Text = "選擇此資料夾", Primary = true, Width = 140, Height = 34, Top = 11, Anchor = AnchorStyles.Right | AnchorStyles.Top };
        var cancel = new FlatButton { Text = "取消", Width = 88, Height = 34, Top = 11, Anchor = AnchorStyles.Right | AnchorStyles.Top };
        void PlaceButtons() { ok.Left = bottom.Width - 16 - 140; cancel.Left = ok.Left - 8 - 88; _picked.Width = Math.Max(80, cancel.Left - 24); }
        bottom.Resize += (_, _) => PlaceButtons();
        ok.Click += (_, _) => { SelectedPath = _current; DialogResult = DialogResult.OK; Close(); };
        cancel.Click += (_, _) => { DialogResult = DialogResult.Cancel; Close(); };
        bottom.Controls.AddRange(new Control[] { _picked, ok, cancel });

        // ---- quick-access sidebar ----
        var sidebar = new Panel { Dock = DockStyle.Left, Width = 168, BackColor = Theme.PanelBg };
        var sideTitle = new Label
        {
            Dock = DockStyle.Top, Height = 34, Text = "常用位置", ForeColor = Theme.TextFaint, Font = Theme.UI(9f, FontStyle.Bold),
            TextAlign = ContentAlignment.MiddleLeft, Padding = new Padding(14, 0, 0, 0)
        };
        var places = new FlowLayoutPanel
        {
            Dock = DockStyle.Fill, FlowDirection = FlowDirection.TopDown, WrapContents = false,
            BackColor = Theme.PanelBg, Padding = new Padding(8, 4, 8, 8)
        };
        void AddPlace(string glyph, string caption, Action act)
        {
            var b = new FlatButton { Text = $"{glyph}   {caption}", LeftAlign = true, Width = 148, Height = 34, Margin = new Padding(0, 0, 0, 4) };
            b.Click += (_, _) => act();
            places.Controls.Add(b);
        }
        AddPlace("🖥", "桌面", () => Navigate(Environment.GetFolderPath(Environment.SpecialFolder.Desktop)));
        AddPlace("🖼", "圖片", () => Navigate(Environment.GetFolderPath(Environment.SpecialFolder.MyPictures)));
        AddPlace("⬇", "下載", () => Navigate(DownloadsFolder()));
        AddPlace("📄", "文件", () => Navigate(Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments)));
        AddPlace("💽", "本機", ShowDrives);
        sidebar.Controls.Add(places);
        sidebar.Controls.Add(sideTitle);

        var sideSep = new Panel { Dock = DockStyle.Left, Width = 1, BackColor = Theme.Border };

        // ---- path bar (up button + current path) ----
        var pathBar = new Panel { Dock = DockStyle.Top, Height = 48, BackColor = Theme.WindowBg, Padding = new Padding(12, 8, 12, 8) };
        var upBtn = new FlatButton { Text = "↑  上一層", Width = 92, Height = 32, Left = 12, Top = 8 };
        upBtn.Click += (_, _) => { var p = Directory.GetParent(_current)?.FullName; if (p != null) Navigate(p); else ShowDrives(); };
        _path = new Label
        {
            Left = 116, Top = 8, Height = 32, ForeColor = Theme.Text, BackColor = Theme.PanelBg2, Font = Theme.Normal,
            TextAlign = ContentAlignment.MiddleLeft, Padding = new Padding(12, 0, 0, 0), AutoEllipsis = true,
            Anchor = AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Top
        };
        _path.Width = pathBar.Width - 12 - _path.Left;
        pathBar.Controls.AddRange(new Control[] { upBtn, _path });

        // ---- folder list ----
        _list = new FolderList { Dock = DockStyle.Fill };
        _list.ItemActivated += name =>
        {
            string target = string.IsNullOrEmpty(_current) ? name : Path.Combine(_current, name);
            if (Directory.Exists(target)) Navigate(target);
        };

        // Dock order (last added wins the outer edge): Fill first, then Top, then the docked frame.
        Controls.Add(_list);
        Controls.Add(pathBar);
        Controls.Add(sideSep);
        Controls.Add(sidebar);
        Controls.Add(bottom);

        PlaceButtons();

        var start = initial is not null && Directory.Exists(initial)
            ? initial : Environment.GetFolderPath(Environment.SpecialFolder.Desktop);
        Navigate(start);
    }

    private static string DownloadsFolder()
    {
        var p = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Downloads");
        return Directory.Exists(p) ? p : Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
    }

    private void Navigate(string path)
    {
        try
        {
            _current = path;
            _path.Text = path;
            _picked.Text = "已選擇：" + path;
            var names = new List<string>();
            foreach (var dir in Directory.EnumerateDirectories(path)
                         .Where(d => !string.Equals(Path.GetFileName(d), AppPaths.RawTempFolderName, StringComparison.OrdinalIgnoreCase))
                         .OrderBy(d => d, StringComparer.OrdinalIgnoreCase))
            {
                try
                {
                    var info = new DirectoryInfo(dir);
                    if ((info.Attributes & (FileAttributes.Hidden | FileAttributes.System)) != 0) continue;
                    names.Add(info.Name);
                }
                catch { }
            }
            _list.SetItems(names);
        }
        catch (Exception ex)
        {
            MessageBox.Show(this, "無法開啟：" + ex.Message, "AwayPhotoRawEditor", MessageBoxButtons.OK, MessageBoxIcon.Warning);
        }
    }

    private void ShowDrives()
    {
        _current = "";
        _path.Text = "本機";
        _picked.Text = "請選擇資料夾";
        _list.SetItems(DriveInfo.GetDrives().Where(d => d.IsReady).Select(d => d.Name).ToList(), isDrives: true);
    }

    /// <summary>Custom-drawn, hover-highlighted list of folders (or drives) with a folder glyph.</summary>
    private sealed class FolderList : Control
    {
        private readonly List<string> _items = new();
        private bool _drives;
        private int _hover = -1;
        private int _scroll;
        private const int RowH = 32;

        public event Action<string>? ItemActivated;

        public FolderList()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                     ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
            BackColor = Theme.WindowBg;
        }

        public void SetItems(IReadOnlyList<string> items, bool isDrives = false)
        {
            _items.Clear();
            _items.AddRange(items);
            _drives = isDrives;
            _hover = -1;
            _scroll = 0;
            Invalidate();
        }

        private int MaxScroll => Math.Max(0, _items.Count * RowH - Height);
        private int IndexAt(int y) { int i = (y + _scroll) / RowH; return i >= 0 && i < _items.Count ? i : -1; }

        protected override void OnMouseWheel(MouseEventArgs e)
        {
            _scroll = Math.Clamp(_scroll - e.Delta, 0, MaxScroll);
            _hover = IndexAt(e.Y);
            Invalidate();
            base.OnMouseWheel(e);
        }

        protected override void OnMouseMove(MouseEventArgs e)
        {
            int i = IndexAt(e.Y);
            if (i != _hover) { _hover = i; Invalidate(); }
            base.OnMouseMove(e);
        }

        protected override void OnMouseLeave(EventArgs e) { _hover = -1; Invalidate(); base.OnMouseLeave(e); }

        protected override void OnMouseDoubleClick(MouseEventArgs e)
        {
            int i = IndexAt(e.Y);
            if (i >= 0) ItemActivated?.Invoke(_items[i]);
            base.OnMouseDoubleClick(e);
        }

        protected override void OnMouseClick(MouseEventArgs e)
        {
            Focus();
            base.OnMouseClick(e);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            PaintHelpers.EnableHighQuality(g);
            g.Clear(Theme.WindowBg);

            if (_items.Count == 0)
            {
                TextRenderer.DrawText(g, "（此資料夾沒有子資料夾）", Theme.Normal,
                    new Rectangle(0, 0, Width, RowH + 8), Theme.TextFaint,
                    TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
                return;
            }

            int first = Math.Max(0, _scroll / RowH);
            int last = Math.Min(_items.Count - 1, (_scroll + Height) / RowH);
            using var glyphFont = new Font(Theme.FontFamily, 11f);
            for (int i = first; i <= last; i++)
            {
                int y = i * RowH - _scroll;
                var row = new Rectangle(0, y, Width, RowH);
                if (i == _hover)
                    using (var b = new SolidBrush(Theme.PanelBg2))
                        g.FillRectangle(b, row);

                TextRenderer.DrawText(g, _drives ? "💽" : "📁", glyphFont,
                    new Rectangle(12, y, 26, RowH), Theme.TextDim,
                    TextFormatFlags.Left | TextFormatFlags.VerticalCenter);
                TextRenderer.DrawText(g, _items[i], Theme.Normal,
                    new Rectangle(44, y, Width - 56, RowH), Theme.Text,
                    TextFormatFlags.Left | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
            }
        }
    }
}
