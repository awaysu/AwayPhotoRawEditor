using System;
using System.Collections.Generic;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;
using AwayPhotoRawEditor.Exif;
using AwayPhotoRawEditor.Export;
using AwayPhotoRawEditor.Imaging;
using AwayPhotoRawEditor.Models;
using AwayPhotoRawEditor.Panels;
using AwayPhotoRawEditor.Rendering;
using AwayPhotoRawEditor.Storage;

namespace AwayPhotoRawEditor.Forms;

/// <summary>Application main window — assembles the engine, panels and controls.</summary>
public sealed class MainForm : Form
{
    private readonly RawLoader _loader = new();
    private readonly RenderScheduler _scheduler = new();
    // Global export config (persisted). The 標誌/watermark lives here now; when enabled it is
    // drawn live on the main preview and applied on export.
    private readonly ExportSettings _exportSettings = ExportSettings.Load();

    // panels / controls
    private readonly ImageViewer _viewer = new() { Dock = DockStyle.Fill };
    private readonly ThumbnailStrip _strip = new() { Dock = DockStyle.Top, Height = 158, ShowNumber = AppSettings.Current.ShowThumbnailNumber };
    private readonly HistogramPanel _hist = new();
    private readonly PhotoInfoPanel _photoInfo = new();
    private readonly BasicAdjustPanel _basic = new();
    private DarkScrollHost? _leftScroll, _rightScroll;   // 左右欄捲動容器（設定「顯示捲軸」開關）
    private readonly ColorPanel _color = new();
    private readonly DetailPanel _detail = new();
    private readonly ToolsPanel _tools = new();
    private readonly PresetPanel _preset = new();

    // 無照片時要一起停用的重設/還原按鈕（見 SetEditorEnabled）。
    private FlatButton _bcdResetBtn = null!;
    private FlatButton _resetAllBtn = null!;
    private FlatButton _undoStepBtn = null!;
    private Label _pathLabel = null!;
    private Label _status = null!;   // LibRaw / status label in the viewer toolbar
    private FlatButton _origBtn = null!;   // 對照原圖 toggle in the viewer toolbar
    private bool _showOriginal;            // render with neutral adjustments (keep geometry)

    // state
    private readonly List<PhotoItem> _items = new();
    private PhotoItem? _current;
    private ImageAdjustments? _adj;
    private ImageAdjustments? _snapshot;
    private ExifData? _exif;
    private FloatImageBuffer? _proxy;
    private int _fullLong = 1, _proxyLong = 1;
    private bool _dirty;
    private bool _resetViewNext;
    private string _folder = "";
    /// <summary>One undo step: the current photo's prior state, plus (for batch edits) the
    /// prior state of every other affected photo so the whole step can be reverted together.</summary>
    private sealed class UndoStep
    {
        public required ImageAdjustments Current;
        public List<(PhotoItem item, ImageAdjustments adj)>? Others;
    }

    private readonly Stack<UndoStep> _undo = new();
    private readonly Stack<UndoStep> _redo = new();   // cleared on new edits / photo switch
    private ImageAdjustments? _copiedAdj;

    // Multi-selection batch editing: when >1 photo is selected, the other selected photos
    // (captured at the start of an edit gesture) receive the same changed fields as the
    // current photo, flushed to disk when the current photo commits.
    private List<PhotoItem>? _syncTargets;
    private ImageAdjustments? _syncBaseline;

    public MainForm()
    {
        Text = "AwayPhotoRawEditor";
        try { Icon = System.Drawing.Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { /* icon 載不到就用預設 */ }
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        ClientSize = new Size(1366, 820);
        MinimumSize = new Size(1100, 700);
        StartPosition = FormStartPosition.CenterScreen;
        WindowState = FormWindowState.Maximized;
        KeyPreview = true;

        // 拖放資料夾（或圖檔 → 開啟其所在資料夾）即開啟
        AllowDrop = true;
        DragEnter += (_, e) => { if (GetDroppedFolder(e) != null) e.Effect = DragDropEffects.Copy; };
        DragDrop += (_, e) => { var dir = GetDroppedFolder(e); if (dir != null) OpenFolder(dir); };

        BuildLayout();
        WirePanels();
        WireViewer();
        WireStrip();
        SetupScheduler();
        ResetPanelsToDefault();
        SetEditorEnabled(false);   // 尚未載入照片：所有設定顯示預設值且不可調整
        UpdateStatus();
        L.Apply(this);
    }

    private bool _autoOpened;

    protected override void OnShown(EventArgs e)
    {
        base.OnShown(e);
        if (_autoOpened) return;
        _autoOpened = true;
        var last = AppSettings.Current.LastFolder;
        if (!string.IsNullOrWhiteSpace(last) && Directory.Exists(last))
            OpenFolder(last);
    }

    /// <summary>Test hook: open a folder programmatically (used by the --shot diagnostic).</summary>
    public void TestOpen(string folder) => OpenFolder(folder);

    // ---- layout ----------------------------------------------------------

    private void BuildLayout()
    {
        var top = BuildTopBar();

        _strip.BackColor = Theme.PanelBg;
        _strip.Dock = DockStyle.Top;

        // Left + center share a region that carries the thumbnail strip on top; the
        // right column lives beside it and therefore spans the full height (from just
        // below the top bar down to the bottom), so the histogram sits at the very top.
        var leftCenter = new TableLayoutPanel
        {
            Dock = DockStyle.Fill, ColumnCount = 2, RowCount = 1, BackColor = Theme.WindowBg, Margin = Padding.Empty
        };
        leftCenter.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 330));
        leftCenter.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        leftCenter.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        leftCenter.Controls.Add(BuildLeftColumn(), 0, 0);
        leftCenter.Controls.Add(BuildCenterColumn(), 1, 0);

        var leftCenterHost = new Panel { Dock = DockStyle.Fill, BackColor = Theme.WindowBg, Margin = Padding.Empty };
        leftCenterHost.Controls.Add(leftCenter);   // Fill first
        leftCenterHost.Controls.Add(_strip);       // Top last -> claims the top of this region only

        var body = new TableLayoutPanel
        {
            Dock = DockStyle.Fill, ColumnCount = 2, RowCount = 1, BackColor = Theme.WindowBg, Margin = Padding.Empty
        };
        body.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        body.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 320));
        body.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        body.Controls.Add(leftCenterHost, 0, 0);
        body.Controls.Add(BuildRightColumn(), 1, 0);

        // Dock order: body (Fill) first, top bar (Top) last so the top bar claims the
        // very top and body (incl. the full-height right column) fills the rest.
        Controls.Add(body);
        Controls.Add(top);
    }

    private Panel BuildTopBar()
    {
        var top = new Panel { Dock = DockStyle.Top, Height = 60, BackColor = Theme.PanelBg2 };

        var appMenu = new IconButton { Glyph = "☰", GlyphSize = 20f, Left = 14, Top = 9, Width = 46, Height = 42 };
        appMenu.Click += (_, _) => ShowAppMenu(appMenu);

        var logo = new Label { Text = "AwayPhotoRawEditor", Left = 72, Top = 0, Width = 290, Height = 60, ForeColor = Theme.Text, Font = Theme.UI(15f, FontStyle.Bold), TextAlign = ContentAlignment.MiddleLeft };

        var openBtn = new FlatButton { Text = "📁  開啟資料夾", Primary = true, Left = 370, Top = 14, Width = 132, Height = 32 };
        openBtn.Click += (_, _) => PickFolder();

        const int pathLeft = 516;
        _pathLabel = new Label { Left = pathLeft, Top = 0, Height = 60, ForeColor = Theme.TextDim, Font = Theme.Normal, TextAlign = ContentAlignment.MiddleLeft, AutoEllipsis = true, Text = "尚未選擇資料夾" };

        var exportBtn = new FlatButton { Text = "匯出全部照片", Primary = true, Width = 180, Height = 32, Top = 14, Anchor = AnchorStyles.Top | AnchorStyles.Right };
        exportBtn.Left = top.Width - 16 - 180;
        exportBtn.Click += (_, _) => ExportAll();

        var exportCurBtn = new FlatButton { Text = "匯出目前照片", Primary = true, Width = 140, Height = 32, Top = 14, Anchor = AnchorStyles.Top | AnchorStyles.Right };
        exportCurBtn.Left = exportBtn.Left - 8 - 140;
        exportCurBtn.Click += (_, _) => ExportCurrent();

        top.Resize += (_, _) =>
        {
            exportBtn.Left = top.Width - 16 - 180;
            exportCurBtn.Left = exportBtn.Left - 8 - 140;
            _pathLabel.Width = Math.Max(40, exportCurBtn.Left - pathLeft - 12);
        };
        top.Controls.AddRange(new Control[] { appMenu, logo, openBtn, _pathLabel, exportCurBtn, exportBtn });
        return top;
    }

    private Panel BuildLeftColumn()
    {
        var flow = new FlowLayoutPanel
        {
            FlowDirection = FlowDirection.TopDown, WrapContents = false,
            AutoScroll = false, BackColor = Theme.PanelBg, Padding = new Padding(10, 4, 10, 8)
        };
        foreach (var s in new SectionPanel[] { _basic, _color, _detail })
        {
            s.Dock = DockStyle.None;
            s.Margin = new Padding(0, 0, 0, 10);
            flow.Controls.Add(s);
        }
        _basic.Margin = new Padding(0, 0, 0, 4);   // 色彩、細節上方留 4px
        _color.Margin = new Padding(0, 0, 0, 4);

        // 基本 / 色彩 / 細節 重設 (below 細節)
        _bcdResetBtn = new FlatButton { Text = "基本／色彩／細節 重設", Margin = new Padding(0, 0, 0, 10) };
        _bcdResetBtn.Size = new Size(310, 30);
        _bcdResetBtn.Click += (_, _) => ResetBasicColorDetail();
        flow.Controls.Add(_bcdResetBtn);

        _preset.Dock = DockStyle.None;
        _preset.Margin = new Padding(0, 0, 0, 10);
        flow.Controls.Add(_preset);

        // 視窗太矮時可垂直捲動（自繪深色捲軸；由設定「顯示捲軸」開關，預設關）
        _leftScroll = new DarkScrollHost(flow)
        {
            Dock = DockStyle.Fill, BackColor = Theme.PanelBg, Margin = Padding.Empty,
            ScrollEnabled = AppSettings.Current.ShowColumnScrollBars
        };
        return _leftScroll;
    }

    private Panel BuildCenterColumn()
    {
        var center = new Panel { Dock = DockStyle.Fill, BackColor = Theme.WindowBg, Margin = Padding.Empty };

        var toolbar = new Panel { Dock = DockStyle.Bottom, Height = 36, BackColor = Theme.PanelBg2 };
        var fitBtn = new FlatButton { Text = "適合", Left = 10, Top = 5, Width = 60, Height = 26 };
        var z100 = new FlatButton { Text = "100%", Left = 76, Top = 5, Width = 60, Height = 26 };
        var z200 = new FlatButton { Text = "200%", Left = 142, Top = 5, Width = 60, Height = 26 };
        fitBtn.Click += (_, _) => { _viewer.ZoomFit(); UpdateStatus(); };
        z100.Click += (_, _) => { _viewer.Zoom100(); UpdateStatus(); };
        z200.Click += (_, _) => { _viewer.Zoom200(); UpdateStatus(); };
        _origBtn = new FlatButton { Text = "對照原圖", Left = 216, Top = 5, Width = 88, Height = 26 };
        _origBtn.Click += (_, _) => ToggleShowOriginal();
        _status = new Label { Dock = DockStyle.Right, Width = 160, ForeColor = Theme.TextDim, Font = Theme.Small, TextAlign = ContentAlignment.MiddleCenter, Padding = new Padding(0, 0, 12, 0), Text = "未使用LibRaw讀取" };
        toolbar.Controls.AddRange(new Control[] { fitBtn, z100, z200, _origBtn, _status });

        _viewer.BackColor = Theme.ViewerBg;
        center.Controls.Add(_viewer);
        center.Controls.Add(toolbar);
        return center;
    }

    private Panel BuildRightColumn()
    {
        var host = new Panel { Dock = DockStyle.Fill, BackColor = Theme.PanelBg, Margin = Padding.Empty };
        var flow = new FlowLayoutPanel
        {
            FlowDirection = FlowDirection.TopDown, WrapContents = false,
            AutoScroll = false, BackColor = Theme.PanelBg, Padding = new Padding(15, 4, 15, 8)
        };
        foreach (var s in new SectionPanel[] { _hist, _photoInfo, _tools })
        {
            s.Dock = DockStyle.None;
            s.Margin = new Padding(0, 0, 0, 14);
            flow.Controls.Add(s);
        }

        // 重設 (原始狀態) + 恢復上一步 pinned to the bottom of the right column
        var actionHost = new Panel { Dock = DockStyle.Bottom, Height = 96, BackColor = Theme.PanelBg };
        _resetAllBtn = new FlatButton { Text = "全部重設" };
        _resetAllBtn.SetBounds(15, 8, 290, 34);
        _resetAllBtn.Click += (_, _) => ResetAllAdjustments();
        _undoStepBtn = new FlatButton { Text = "恢復上一步" };
        _undoStepBtn.SetBounds(15, 50, 290, 34);
        _undoStepBtn.Click += (_, _) => DoUndo();
        actionHost.Controls.AddRange(new Control[] { _resetAllBtn, _undoStepBtn });

        // 視窗太矮時捲動上方區塊；底部「全部重設／恢復上一步」維持固定不捲
        _rightScroll = new DarkScrollHost(flow)
        {
            Dock = DockStyle.Fill, BackColor = Theme.PanelBg,
            ScrollEnabled = AppSettings.Current.ShowColumnScrollBars
        };
        host.Controls.Add(_rightScroll); // Fill first
        host.Controls.Add(actionHost);  // Bottom last -> claims the bottom
        return host;
    }

    // ---- wiring ----------------------------------------------------------

    private void WirePanels()
    {
        foreach (var p in new AdjustPanelBase[] { _basic, _color, _detail, _tools })
        {
            p.EditBegin += PushUndo;
            p.Changed += () => OnAdjustmentChanged();
        }

        _color.WbPickerToggled += on => { _viewer.WhiteBalancePickerActive = on; _viewer.Invalidate(); };
        _color.UseAsShotWb += UseAsShotWb;

        _tools.ToolChanged += OnToolChanged;
        _tools.RotateLeft += () => Rotate(-90);
        _tools.RotateRight += () => Rotate(90);
        _tools.ResetCrop += ResetCropGeometry;
        _tools.CropAspectChanged += ApplyCropAspect;
        _tools.HealModeChanged += m => _viewer.HealMode = m;
        _tools.HealSizeChanged += s => { if (_viewer.SetHealBrushSize(s)) OnAdjustmentChanged(); };
        _tools.ClearHeal += () => { if (_adj != null) { PushUndo(); _adj.HealSpots.Clear(); OnAdjustmentChanged(); } };
        _tools.AddGradient += OnAddGradient;

        _preset.ApplyPreset += ApplyPreset;
    }

    private void WireViewer()
    {
        _viewer.EditBegin += (_, _) => PushUndo();
        _viewer.EditChanged += (_, _) => OnAdjustmentChanged();
        _viewer.ViewChanged += (_, _) => UpdateStatus();
        _viewer.WhiteBalancePicked += OnWhiteBalancePicked;
        _viewer.GradientSelectionChanged += () => { if (_adj != null) _tools.Bind(_adj); };
    }

    private void WireStrip()
    {
        _strip.SelectionChanged += (_, _) =>
        {
            var cur = _strip.CurrentItem;
            if (cur != null && !ReferenceEquals(cur, _current)) LoadPhoto(cur);
        };
        _strip.ItemActivated += item => LoadPhoto(item);
        _strip.ItemRightClicked += (item, pt) => ShowThumbnailMenu(item, pt);
    }

    private void SetupScheduler()
    {
        _scheduler.JobFactory = () =>
        {
            if (_proxy is null || _adj is null) return null;
            var adj = _adj.Clone();
            // 對照原圖：neutral adjustments but keep geometry so the framing doesn't jump.
            if (_showOriginal)
                adj = new ImageAdjustments
                {
                    CropAspectRatio = adj.CropAspectRatio, CropAngle = adj.CropAngle,
                    CropX = adj.CropX, CropY = adj.CropY, CropWidth = adj.CropWidth, CropHeight = adj.CropHeight,
                    Rotation = adj.Rotation, Distortion = adj.Distortion
                };
            var proxy = _proxy;
            // Gradient/Heal overlays live in the pre-geometry frame → skip all geometry.
            // Crop overlay → apply distortion + 90° rotation, skip only the crop rectangle.
            bool skipGeom = _viewer.Tool is ToolMode.Gradient or ToolMode.Heal;
            bool skipCropRect = _viewer.Tool is ToolMode.Crop;
            double wmScale = _fullLong > 0 ? (double)_proxyLong / _fullLong : 1.0;
            var ctx = new ProcessContext
            {
                SkipGeometry = skipGeom, SkipCropRect = skipCropRect,
                WatermarkScale = wmScale, Watermark = _exportSettings.BuildWatermark()
            };
            return token => { ctx.Token = token; return ImageProcessor.Apply(proxy, adj, ctx); };
        };
        _scheduler.Completed = OnRenderDone;
        _scheduler.Failed = ex => _status.Text = L.T("算圖失敗：") + ex.Message;
    }

    // ---- folder / scan ---------------------------------------------------

    private void PickFolder()
    {
        using var dlg = new FolderPickerForm(_folder);
        if (dlg.ShowDialog(this) == DialogResult.OK && Directory.Exists(dlg.SelectedPath))
            OpenFolder(dlg.SelectedPath);
    }

    private void OpenFolder(string folder)
    {
        SaveCurrentIfDirty();

        _folder = folder;
        if (!Program.Headless)   // 診斷截圖（--shot 等）不可污染使用者的 LastFolder / 開啟紀錄
        {
            AppSettings.Current.LastFolder = folder;
            AppSettings.Current.PushRecentFolder(folder);
            AppSettings.Current.Save();
        }
        _pathLabel.Text = folder;

        List<string> files;
        try
        {
            files = Directory.EnumerateFiles(folder)
                .Where(f => AppPaths.IsSupported(f) && !AppPaths.IsRawTemp(f))
                .OrderBy(f => f, StringComparer.OrdinalIgnoreCase).ToList();
        }
        catch (Exception ex)
        {
            MessageBox.Show(this, L.T("無法讀取資料夾：") + ex.Message, "AwayPhotoRawEditor", MessageBoxButtons.OK, MessageBoxIcon.Warning);
            return;
        }

        var preview = PreviewListStore.Load(folder);
        var hidden = new HashSet<string>(preview.Hidden);

        _items.Clear();
        foreach (var f in files)
        {
            if (!hidden.Contains(f)) _items.Add(new PhotoItem(f));
            foreach (var vc in preview.VirtualCopies.Where(v => string.Equals(v.Path, f, StringComparison.OrdinalIgnoreCase)))
            {
                var key = $"{f}|copy:{vc.Index}";
                if (!hidden.Contains(key)) _items.Add(new PhotoItem(f, vc.Index));
            }
        }

        _current = null; _adj = null; _proxy = null;
        // 載入完成前（或資料夾沒有照片時）維持預設且不可調整；ApplyLoaded 會再啟用。
        ResetPanelsToDefault();
        SetEditorEnabled(false);
        _strip.SetItems(_items, 0);
        _viewer.SetImage(null, true);
        _photoInfo.SetExif(null); _hist.SetHistogram(null);
        UpdateStatus();

        // Generate thumbnail + full-preview caches with a cancelable progress window,
        // so selecting any photo later is instant (no on-demand RAW decode).
        var itemsCopy = _items.ToList();
        using (var pf = new ProgressForm(L.T("產生快取（縮圖＋預覽）"),
                   (prog, token) => GenerateThumbnailCaches(itemsCopy, prog, token),
                   subtitle: L.T("第一次產生快取與縮圖檔案需要一些時間\n請稍等..."),
                   doneMessage: L.T("完成，可以開始編輯")))
            pf.ShowDialog(this);

        if (_items.Count > 0) LoadPhoto(_items[0]);
        UpdateStatus();
    }

    /// <summary>Close the current folder: persist pending edits, then clear the whole view.</summary>
    private void CloseFolder()
    {
        ClearEditor();   // saves pending edits, resets panels to defaults, disables editing
        if (!string.IsNullOrEmpty(_folder)) SavePreviewList();

        _folder = "";
        _items.Clear();
        _copiedAdj = null;
        _strip.SetItems(_items, 0);
        _pathLabel.Text = L.T("尚未選擇資料夾");
        if (!Program.Headless)
        {
            // 使用者主動關閉資料夾 → 下次啟動維持「關閉狀態」，不再自動開啟這個資料夾
            AppSettings.Current.LastFolder = "";
            AppSettings.Current.Save();
        }
        UpdateStatus();
    }

    /// <summary>清空主編輯區（無照片狀態）：先存回未儲存的編輯，再把畫面、直方圖、
    /// EXIF 清空，所有調整面板回到預設值並停用（無法設定）。</summary>
    private void ClearEditor()
    {
        _scheduler.CancelPending();
        SaveCurrentIfDirty();

        _current = null; _adj = null; _snapshot = null; _exif = null; _proxy = null;
        _undo.Clear(); _redo.Clear();
        _dirty = false;
        if (_showOriginal) { _showOriginal = false; _origBtn.Primary = false; }

        _viewer.SetImage(null, true);
        _viewer.Adjustments = null;
        _photoInfo.SetExif(null);
        _hist.SetHistogram(null);
        _hist.SetMean(0, 0, 0);

        ResetPanelsToDefault();
        SetEditorEnabled(false);
    }

    /// <summary>把所有調整面板綁到一份全新的預設值（脫離目前照片），並取消滴管/工具選取。</summary>
    private void ResetPanelsToDefault()
    {
        var def = new ImageAdjustments();
        _basic.Bind(def);
        _color.SetTemperatureMode(true);
        _color.Bind(def);
        _color.SetPickerChecked(false);
        _detail.Bind(def);
        _tools.Bind(def);
        _tools.SelectTool(ToolMode.None);
        _viewer.WhiteBalancePickerActive = false;
    }

    /// <summary>啟用／停用所有編輯控制（調整面板、風格檔、工具、重設與對照原圖按鈕）。
    /// 主編輯區沒有照片時停用，載入照片後恢復。</summary>
    private void SetEditorEnabled(bool on)
    {
        _basic.Enabled = on;
        _color.Enabled = on;
        _detail.Enabled = on;
        _tools.Enabled = on;
        _preset.Enabled = on;
        _bcdResetBtn.Enabled = on;
        _resetAllBtn.Enabled = on;
        _undoStepBtn.Enabled = on;
        _origBtn.Enabled = on;
    }

    /// <summary>Close the folder, then delete its regenerable cache files (thumbnails + proxies).
    /// Edit XMLs and preview_list are kept, so edits survive; caches rebuild on next open.</summary>
    private void CloseFolderAndClearCache()
    {
        if (string.IsNullOrEmpty(_folder)) return;
        var r = MessageBox.Show(this,
            L.T("關閉資料夾並刪除此資料夾的快取與縮圖檔案？\n（編輯設定會保留，下次開啟會重新產生快取）"),
            L.T("刪除快取縮圖"), MessageBoxButtons.OKCancel, MessageBoxIcon.Warning);
        if (r != DialogResult.OK) return;

        var folder = _folder;
        CloseFolder();
        // Release any GC-held proxy / thumbnail bitmaps so their files aren't locked.
        GC.Collect(); GC.WaitForPendingFinalizers();
        int removed = DeleteCacheFiles(folder);
        _status.Text = L.F("已關閉資料夾並刪除快取縮圖（{0} 個檔案）", removed);
    }

    /// <summary>Delete thumbnail (_thumb.jpg) and proxy (.rawpipe.png / .f32) caches under the
    /// folder's RAW_TEMP, leaving adjustment XMLs and preview_list intact. Returns files deleted.</summary>
    private static int DeleteCacheFiles(string folder)
    {
        int n = 0;
        try
        {
            var dir = AppPaths.RawTempDir(folder);
            if (!Directory.Exists(dir)) return 0;
            foreach (var f in Directory.EnumerateFiles(dir))
            {
                var name = Path.GetFileName(f);
                bool isCache = name.EndsWith("_thumb.jpg", StringComparison.OrdinalIgnoreCase)
                            || name.EndsWith(".rawpipe.png", StringComparison.OrdinalIgnoreCase)
                            || name.EndsWith(".rawpipe.png.f32", StringComparison.OrdinalIgnoreCase);
                if (isCache) { try { File.Delete(f); n++; } catch { } }
            }
        }
        catch { }
        return n;
    }

    private Task GenerateThumbnailCaches(List<PhotoItem> items, IProgress<(int, int, string)> prog, CancellationToken token)
    {
        return Task.Run(() =>
        {
            int total = items.Count;
            var done = new int[1];
            using var sem = new SemaphoreSlim(Math.Max(2, Environment.ProcessorCount / 2));
            // Full-resolution proxy decode is memory-heavy, so cap it harder than thumbnails.
            using var proxySem = new SemaphoreSlim(2);
            var proxyDone = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            var tasks = items.Select(item => Task.Run(() =>
            {
                token.ThrowIfCancellationRequested();
                sem.Wait(token);
                try
                {
                    bool edited = ComputeEditedFlag(item);
                    var thumb = _loader.LoadThumbnail(item.SourcePath, 240, 160);
                    if (!token.IsCancellationRequested)
                        BeginInvoke(new Action(() =>
                        {
                            item.IsEdited = edited;
                            if (thumb != null) _strip.SetImage(item.Key, thumb);
                            else _strip.RefreshBadges();
                        }));
                    else thumb?.Dispose();

                    // Pre-build the full preview proxy once per source file (virtual copies share it).
                    bool doProxy;
                    lock (proxyDone) doProxy = proxyDone.Add(item.SourcePath);
                    if (doProxy && !token.IsCancellationRequested)
                    {
                        proxySem.Wait(token);
                        try { _loader.EnsureProxyCache(item.SourcePath); }
                        catch { }
                        finally { try { proxySem.Release(); } catch { } }
                    }
                }
                catch (OperationCanceledException) { }
                catch { }
                finally
                {
                    int d = Interlocked.Increment(ref done[0]);
                    prog.Report((d, total, item.FileName));
                    try { sem.Release(); } catch { }
                }
            }, token)).ToArray();
            try { Task.WaitAll(tasks, token); } catch { }
        }, token);
    }

    private static bool ComputeEditedFlag(PhotoItem item)
    {
        if (!AdjustmentXmlStore.Exists(item.SourcePath, item.VirtualCopyIndex)) return false;
        if (AdjustmentXmlStore.IsDefaultPlaceholder(item.SourcePath, item.VirtualCopyIndex)) return false;
        var a = AdjustmentXmlStore.Load(item.SourcePath, item.VirtualCopyIndex);
        return a is not null && !a.IsDefault;
    }

    // ---- photo load ------------------------------------------------------

    private long _loadVersion;

    private void LoadPhoto(PhotoItem item, bool force = false)
    {
        // Ignore duplicate loads of the same item (e.g. SetItems selection + explicit call).
        if (!force && ReferenceEquals(item, _current)) return;

        SaveCurrentIfDirty();
        _current = item;
        long v = ++_loadVersion;   // supersede any in-flight load
        _undo.Clear(); _redo.Clear();
        if (_showOriginal) { _showOriginal = false; _origBtn.Primary = false; }
        _resetViewNext = true;
        _status.Text = L.T("載入中… ") + item.DisplayName;

        var loader = _loader;
        Task.Run(() =>
        {
            ExifData exif;
            ImageAdjustments adj;
            FloatImageBuffer? proxy;
            try
            {
                exif = AdjustmentXmlStore.LoadExif(item.SourcePath, item.VirtualCopyIndex) ?? ExifReader.Read(item.SourcePath);
                adj = AdjustmentXmlStore.Load(item.SourcePath, item.VirtualCopyIndex)
                      ?? AdjustmentXmlStore.EnsureDefault(item.SourcePath, exif, item.VirtualCopyIndex);
                proxy = loader.LoadProxyFloat(item.SourcePath);
            }
            catch (Exception ex)
            {
                BeginInvoke(new Action(() => _status.Text = L.T("載入失敗：") + ex.Message));
                return;
            }
            BeginInvoke(new Action(() => ApplyLoaded(item, v, adj, exif, proxy)));
        });
    }

    private void ApplyLoaded(PhotoItem item, long version, ImageAdjustments adj, ExifData exif, FloatImageBuffer? proxy)
    {
        // Drop superseded loads. Do NOT dispose the previous proxy — an in-flight render
        // may still reference it; let the GC reclaim it once no render holds it.
        if (version != _loadVersion || !ReferenceEquals(item, _current)) return;
        _adj = adj;
        _snapshot = adj.Clone();
        _exif = exif;
        _proxy = proxy;
        _proxyLong = proxy is null ? 1 : Math.Max(proxy.Width, proxy.Height);
        _fullLong = exif.Width > 0 && exif.Height > 0 ? Math.Max(exif.Width, exif.Height) : _proxyLong;
        _dirty = false;

        RebindAll();
        SetEditorEnabled(true);
        _photoInfo.SetExif(exif);
        item.IsEdited = !adj.IsDefault;
        _strip.RefreshBadges();
        _scheduler.Schedule(immediate: true);
        UpdateStatus();
    }

    private void RebindAll()
    {
        if (_adj is null) return;
        _basic.Bind(_adj);
        _color.SetTemperatureMode(_current == null || AppPaths.IsRaw(_current.SourcePath));
        _color.Bind(_adj);
        _detail.Bind(_adj);
        _tools.Bind(_adj);
        _viewer.Adjustments = _adj;
        _viewer.HealBrushSize = _adj.HealSize;
    }

    private void OnRenderDone(Bitmap bmp)
    {
        var hist = ImageStats.ComputeHistogram(bmp);
        var (r, g, b) = ImageStats.MeanColor(bmp);
        _viewer.SetImage(bmp, _resetViewNext);
        _resetViewNext = false;
        _hist.SetHistogram(hist);
        _hist.SetMean(r, g, b);
        UpdateStatus();
    }

    // ---- adjustment change flow -----------------------------------------

    private void OnAdjustmentChanged(bool immediate = false)
    {
        if (_adj is null) return;
        _dirty = true;
        if (_current != null) _current.IsEdited = !_adj.IsDefault;
        // Provisional feedback: mark the batch targets edited now (finalized on flush).
        if (_syncTargets != null) foreach (var it in _syncTargets) it.IsEdited = true;
        if (_current != null || _syncTargets != null) _strip.RefreshBadges();
        _scheduler.Schedule(immediate);
    }

    private void PushUndo()
    {
        if (_adj is null) return;
        _redo.Clear();   // a fresh edit invalidates the redo branch
        // Editing while 對照原圖 is on would be blind — switch back to the edited view.
        if (_showOriginal) { _showOriginal = false; _origBtn.Primary = false; }
        var step = new UndoStep { Current = _adj.Clone() };

        // Begin a batch-edit session on the first edit while multiple photos are selected:
        // remember the other selected photos + the pre-edit state (to diff against later), and
        // snapshot their prior adjustments on this step so an undo can revert them too.
        if (_syncTargets is null)
        {
            var others = SelectedOthers();
            if (others.Count > 0)
            {
                _syncTargets = others;
                _syncBaseline = _adj.Clone();
                step.Others = others.Select(it =>
                    (it, (AdjustmentXmlStore.Load(it.SourcePath, it.VirtualCopyIndex) ?? new ImageAdjustments()).Clone())).ToList();
            }
        }

        _undo.Push(step);
        if (_undo.Count > 80)
        {
            var arr = _undo.ToArray();          // top-first
            _undo.Clear();
            for (int i = Math.Min(80, arr.Length) - 1; i >= 0; i--) _undo.Push(arr[i]);
        }
    }

    private void DoUndo()
    {
        if (_undo.Count == 0 || _adj is null) return;
        var step = _undo.Pop();
        _redo.Push(new UndoStep { Current = _adj.Clone() });   // enable Ctrl+Y to come back
        _adj = step.Current;

        // If this step opened a batch edit, restore every other affected photo to its prior
        // state and cancel any still-pending sync so it can't re-apply after the revert.
        if (step.Others is not null)
        {
            foreach (var (it, prior) in step.Others)
            {
                try
                {
                    AdjustmentXmlStore.Save(it.SourcePath, prior.Clone(), it.VirtualCopyIndex,
                        AdjustmentXmlStore.LoadExif(it.SourcePath, it.VirtualCopyIndex));
                    it.IsEdited = !prior.IsDefault;
                }
                catch { }
            }
            _syncTargets = null; _syncBaseline = null;
            _strip.RefreshBadges();
        }

        _snapshot ??= _adj.Clone();
        RebindAll();
        OnAdjustmentChanged(immediate: true);
    }

    /// <summary>Ctrl+Y：重做上一次被 Ctrl+Z 還原的編輯（單張；批次同步不重播）。</summary>
    private void DoRedo()
    {
        if (_redo.Count == 0 || _adj is null) return;
        var step = _redo.Pop();
        _undo.Push(new UndoStep { Current = _adj.Clone() });   // push directly: no new batch session
        _adj = step.Current;
        _snapshot ??= _adj.Clone();
        RebindAll();
        OnAdjustmentChanged(immediate: true);
    }

    /// <summary>全部重設：把所有調整還原成原始狀態（多選時套用到所有選取的相片）。</summary>
    private void ResetAllAdjustments() => ResetSelected(a => a.ResetAll());

    /// <summary>只重設 基本調整 / 色彩 / 細節（不動裁切/漸層/修護/標誌；多選時套用到所有選取的相片）。</summary>
    private void ResetBasicColorDetail() => ResetSelected(a => a.ResetBasicColorDetail());

    /// <summary>Apply a reset to the current photo and, when multiple are selected, to every
    /// other selected photo immediately (so their edited badges clear right away).</summary>
    private void ResetSelected(Action<ImageAdjustments> reset)
    {
        if (_adj is null) return;
        PushUndo();
        reset(_adj);
        foreach (var it in SelectedOthers())
        {
            try
            {
                var a = AdjustmentXmlStore.Load(it.SourcePath, it.VirtualCopyIndex) ?? new ImageAdjustments();
                reset(a);
                it.IsEdited = !a.IsDefault;
                AdjustmentXmlStore.Save(it.SourcePath, a, it.VirtualCopyIndex,
                    AdjustmentXmlStore.LoadExif(it.SourcePath, it.VirtualCopyIndex));
            }
            catch { }
        }
        // A reset supersedes any pending slider-sync for this session.
        _syncTargets = null; _syncBaseline = null;
        RebindAll();
        _strip.RefreshBadges();
        OnAdjustmentChanged(immediate: true);
    }

    private void SaveCurrentIfDirty()
    {
        FlushBatchSync();
        if (_current is null || _adj is null || !_dirty) return;
        try
        {
            AdjustmentXmlStore.Save(_current.SourcePath, _adj, _current.VirtualCopyIndex, _exif);
            _snapshot = _adj.Clone();
            _dirty = false;
        }
        catch (Exception ex) { _status.Text = L.T("儲存調整失敗：") + ex.Message; }
    }

    /// <summary>The selected photos other than the current one (empty unless a multi-selection).</summary>
    private List<PhotoItem> SelectedOthers()
    {
        var sel = _strip.SelectedItems;
        if (sel.Count <= 1) return new List<PhotoItem>();
        return sel.Where(it => !ReferenceEquals(it, _current)).ToList();
    }

    /// <summary>Persist the pending multi-selection edit: copy the fields the user changed on the
    /// current photo (vs. the captured baseline) onto every other selected photo, then end the session.</summary>
    private void FlushBatchSync()
    {
        var targets = _syncTargets;
        var baseline = _syncBaseline;
        _syncTargets = null; _syncBaseline = null;
        if (targets is null || baseline is null || _adj is null) return;

        foreach (var it in targets)
        {
            try
            {
                var a = AdjustmentXmlStore.Load(it.SourcePath, it.VirtualCopyIndex) ?? new ImageAdjustments();
                a.ApplyDelta(_adj, baseline);
                it.IsEdited = !a.IsDefault;
                AdjustmentXmlStore.Save(it.SourcePath, a, it.VirtualCopyIndex,
                    AdjustmentXmlStore.LoadExif(it.SourcePath, it.VirtualCopyIndex));
            }
            catch { }
        }
        _strip.RefreshBadges();
    }

    // ---- tools -----------------------------------------------------------

    private void OnToolChanged(ToolMode mode)
    {
        _viewer.Tool = mode;
        _viewer.WhiteBalancePickerActive = false;
        _color.SetPickerChecked(false);
        if (mode == ToolMode.Heal && _adj != null) _viewer.HealBrushSize = _adj.HealSize;
        _scheduler.Schedule(immediate: true); // switch skip-geometry view
    }

    private void OnAddGradient()
    {
        if (_adj is null) return;
        PushUndo();
        _adj.Gradients.Add(new LinearGradient());   // defaults: near top, angle 0, range 0.25
        _adj.ActiveGradientIndex = _adj.Gradients.Count - 1;
        _tools.Bind(_adj);          // point the gradient sliders at the new gradient
        OnAdjustmentChanged(immediate: true);
    }

    private void Rotate(int deltaDeg)
    {
        if (_adj is null) return;
        PushUndo();
        int cur = (int)_adj.Rotation;
        int next = ((cur + deltaDeg) % 360 + 360) % 360;
        _adj.Rotation = (Rotation)next;
        OnAdjustmentChanged(immediate: true);
    }

    private void ResetCropGeometry()
    {
        if (_adj is null) return;
        PushUndo();
        _adj.CropX = 0; _adj.CropY = 0; _adj.CropWidth = 1; _adj.CropHeight = 1;
        _adj.CropAngle = 0; _adj.Distortion = 0; _adj.Rotation = Rotation.R0; _adj.CropAspectRatio = "Original";
        _tools.Bind(_adj);
        OnAdjustmentChanged(immediate: true);
    }

    private void ApplyCropAspect(string aspect)
    {
        if (_adj is null || _proxy is null) return;
        if (aspect is "Original") { _adj.CropX = 0; _adj.CropY = 0; _adj.CropWidth = 1; _adj.CropHeight = 1; OnAdjustmentChanged(true); return; }
        double ratio = ParseAspect(aspect, _proxy.Width, _proxy.Height);
        if (ratio <= 0) { OnAdjustmentChanged(); return; }
        // fit largest centered rect of the given ratio (in image pixels) into the full frame
        double imgRatio = (double)_proxy.Width / _proxy.Height;
        double w, h;
        if (ratio > imgRatio) { w = 1.0; h = imgRatio / ratio; }
        else { h = 1.0; w = ratio / imgRatio; }
        _adj.CropWidth = w; _adj.CropHeight = h;
        _adj.CropX = (1 - w) / 2; _adj.CropY = (1 - h) / 2;
        OnAdjustmentChanged(immediate: true);
    }

    private static double ParseAspect(string aspect, int w, int h)
    {
        if (aspect == "Custom") return 0;
        var parts = aspect.Split(':');
        if (parts.Length == 2 && double.TryParse(parts[0], out var a) && double.TryParse(parts[1], out var b) && b > 0)
            return a / b;
        return 0;
    }

    // ---- white balance ---------------------------------------------------

    private void OnWhiteBalancePicked(double nx, double ny)
    {
        if (_adj is null || _proxy is null) return;
        int x = Math.Clamp((int)(nx * _proxy.Width), 0, _proxy.Width - 1);
        int y = Math.Clamp((int)(ny * _proxy.Height), 0, _proxy.Height - 1);
        int i = (y * _proxy.Width + x) * 4;
        var (t, tint) = EstimateWhiteBalance(_proxy.Data[i], _proxy.Data[i + 1], _proxy.Data[i + 2]);
        PushUndo();
        _adj.Temperature = ClampTempForCurrent(t); _adj.Tint = tint;
        _color.Bind(_adj);
        _viewer.WhiteBalancePickerActive = false;
        _color.SetPickerChecked(false);
        OnAdjustmentChanged(immediate: true);
    }

    private void AutoWhiteBalance()
    {
        if (_adj is null || _proxy is null) return;
        double sr = 0, sg = 0, sb = 0; long n = 0;
        var d = _proxy.Data;
        for (int p = 0; p < d.Length; p += 4 * 37) { sr += d[p]; sg += d[p + 1]; sb += d[p + 2]; n++; }
        if (n == 0) return;
        var (t, tint) = EstimateWhiteBalance((float)(sr / n), (float)(sg / n), (float)(sb / n));
        PushUndo();
        _adj.Temperature = ClampTempForCurrent(t); _adj.Tint = tint;
        _color.Bind(_adj);
        OnAdjustmentChanged(immediate: true);
    }

    private void UseAsShotWb()
    {
        if (_adj is null) return;
        if (_exif is { HasAsShotWhiteBalance: true })
        {
            PushUndo();
            _adj.Temperature = ClampTempForCurrent(_exif.ColorTemperature);
            _adj.Tint = _exif.Tint;
            _color.Bind(_adj);
            OnAdjustmentChanged(immediate: true);
        }
        else _status.Text = L.T("此相片沒有可用的拍攝白平衡資訊");
    }

    /// <summary>Clamp a Kelvin value into the range the 色溫 slider can display for the current
    /// photo — non-RAW files use a ±100 scale (≈5200±3000K), so keep the stored value inside it
    /// or the slider position would no longer match the applied white balance.</summary>
    private double ClampTempForCurrent(double kelvin) =>
        _current != null && !AppPaths.IsRaw(_current.SourcePath)
            ? ColorPanel.ClampToNonRawRange(kelvin) : kelvin;

    /// <summary>Search a temperature/tint whose WB multipliers neutralise the sampled colour.</summary>
    private static (double temp, double tint) EstimateWhiteBalance(float r, float g, float b)
    {
        if (r <= 1e-4f && g <= 1e-4f && b <= 1e-4f) return (5200, 0);
        double bestT = 5200, bestErr = double.MaxValue;
        for (double t = 2000; t <= 12000; t += 100)
        {
            var (mr, mg, mb) = ImageProcessor.WhiteBalanceMultipliers(t, 0);
            double rr = r * mr, bb = b * mb;
            double err = Math.Abs(rr - bb);
            if (err < bestErr) { bestErr = err; bestT = t; }
        }
        var (mr2, mg2, mb2) = ImageProcessor.WhiteBalanceMultipliers(bestT, 0);
        double gg = g * mg2, avg = (r * mr2 + b * mb2) / 2;
        double tint = Math.Clamp((gg - avg) * 300, -100, 100); // + green -> negative tint to compensate
        return (bestT, -tint);
    }

    // ---- presets ---------------------------------------------------------

    private void ApplyPreset(string name)
    {
        if (_adj is null) return;
        PushUndo();
        // Prefer the user's saved override for this preset (any preset, built-in or 自訂);
        // otherwise fall back to the preset's built-in default values.
        if (!PresetStore.ApplyCustom(name, _adj))
            PresetProfile.Get(name)?.ApplyTo(_adj);
        RebindAll();
        OnAdjustmentChanged(immediate: true);
    }

    // ---- thumbnail context menu -----------------------------------------

    private void ShowThumbnailMenu(PhotoItem item, Point screenPt)
    {
        var menu = new ContextMenuStrip { BackColor = Theme.PanelBg2, ForeColor = Theme.Text, ShowImageMargin = false };
        void Add(string text, Action act, bool enabled = true)
        {
            var mi = menu.Items.Add(text);
            mi.Enabled = enabled;
            if (enabled) mi.Click += (_, _) => act();
        }

        var applyPreset = new ToolStripMenuItem(L.T("套用風格檔")) { ForeColor = Theme.Text };
        foreach (var name in PresetProfile.BuiltInNames.Concat(PresetStore.CustomNames()))
        {
            var n = name;
            var sub = applyPreset.DropDownItems.Add(PresetProfile.BuiltIn.ContainsKey(n) ? L.T(n) : n);
            sub.Click += (_, _) => ApplyPresetToSelection(n);
        }

        Add(L.T("全選"), () => _strip.SelectAll());
        Add(L.T("反向選擇"), () => _strip.InvertSelection());
        Add(L.T("取消全選"), () => _strip.DeselectAll());
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(applyPreset);
        // Copy settings only makes sense for a single photo — disable it on a multi-selection.
        Add(L.T("複製照片設定"), () => CopySettings(item), _strip.SelectedItems.Count <= 1);
        Add(L.T("貼上照片設定"), PasteSettings);
        menu.Items.Add(new ToolStripSeparator());
        Add(L.T("建立副本"), () => CreateVirtualCopy(item));
        Add(L.T("移除於預覽列"), () => RemoveFromPreview(item));
        Add(L.T("刪除檔案"), () => DeletePhotoFile(item));
        menu.Items.Add(new ToolStripSeparator());
        Add(L.T("匯出照片"), ExportSelection);

        menu.Show(screenPt);
    }

    private void CopySettings(PhotoItem item)
    {
        var a = AdjustmentXmlStore.Load(item.SourcePath, item.VirtualCopyIndex);
        if (ReferenceEquals(item, _current) && _adj != null) a = _adj;
        _copiedAdj = a?.Clone();
        foreach (var it in _items) it.IsCopySettingsSource = ReferenceEquals(it, item);
        _strip.RefreshBadges();
        _status.Text = L.T("已複製相片設定");
    }

    private void PasteSettings()
    {
        if (_copiedAdj is null) { _status.Text = L.T("尚未複製任何設定"); return; }
        foreach (var it in _strip.SelectedItems)
        {
            var a = _copiedAdj.Clone();
            if (ReferenceEquals(it, _current)) { PushUndo(); _adj = a; RebindAll(); _dirty = true; }
            else AdjustmentXmlStore.Save(it.SourcePath, a, it.VirtualCopyIndex, AdjustmentXmlStore.LoadExif(it.SourcePath, it.VirtualCopyIndex));
            it.IsEdited = !a.IsDefault;
        }
        _strip.RefreshBadges();
        OnAdjustmentChanged(immediate: true);
        _status.Text = L.T("已貼上相片設定");
    }

    private void CreateVirtualCopy(PhotoItem item)
    {
        int next = _items.Where(i => string.Equals(i.SourcePath, item.SourcePath, StringComparison.OrdinalIgnoreCase))
                         .Select(i => i.VirtualCopyIndex).DefaultIfEmpty(0).Max() + 1;
        var copy = new PhotoItem(item.SourcePath, next);
        var baseAdj = AdjustmentXmlStore.Load(item.SourcePath, item.VirtualCopyIndex) ?? new ImageAdjustments();
        AdjustmentXmlStore.Save(copy.SourcePath, baseAdj.Clone(), copy.VirtualCopyIndex, AdjustmentXmlStore.LoadExif(item.SourcePath, item.VirtualCopyIndex));

        int idx = _items.FindIndex(i => ReferenceEquals(i, item));
        _items.Insert(idx + 1, copy);
        SavePreviewList();
        RefreshStripKeepSelection();

        // thumbnail for the copy = same source thumb
        var thumb = _loader.LoadThumbnailCache(item.SourcePath);
        if (thumb != null) _strip.SetImage(copy.Key, thumb);
        _status.Text = L.T("已建立虛擬副本");
    }

    private void RemoveFromPreview(PhotoItem item)
    {
        var preview = PreviewListStore.Load(_folder);
        if (!preview.Hidden.Contains(item.Key)) preview.Hidden.Add(item.Key);
        PreviewListStore.Save(_folder, preview);
        _items.RemoveAll(i => ReferenceEquals(i, item));
        RefreshStripKeepSelection();
        _status.Text = L.T("已從預覽移除");
    }

    /// <summary>Un-hide every photo hidden via 移除於預覽列 and reload the folder.</summary>
    private void RestoreHiddenPhotos()
    {
        if (string.IsNullOrEmpty(_folder)) return;
        var preview = PreviewListStore.Load(_folder);
        if (preview.Hidden.Count == 0) { _status.Text = L.T("沒有已隱藏的照片"); return; }
        int n = preview.Hidden.Count;
        preview.Hidden.Clear();
        PreviewListStore.Save(_folder, preview);
        OpenFolder(_folder);
        _status.Text = L.F("已還原 {0} 張隱藏的照片", n);
    }

    private void DeletePhotoFile(PhotoItem item)
    {
        if (item.IsVirtualCopy) { RemoveFromPreview(item); return; }
        if (MessageBox.Show(this, L.F("確定刪除檔案？（會移到資源回收桶）\n{0}", item.FileName), L.T("刪除照片檔案"),
            MessageBoxButtons.YesNo, MessageBoxIcon.Warning) != DialogResult.Yes) return;
        try
        {
            // Source photo goes to the recycle bin (recoverable); caches are hard-deleted (regenerable).
            Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(item.SourcePath,
                Microsoft.VisualBasic.FileIO.UIOption.OnlyErrorDialogs,
                Microsoft.VisualBasic.FileIO.RecycleOption.SendToRecycleBin);
            try { File.Delete(AppPaths.ThumbnailPath(item.SourcePath)); } catch { }
            try { File.Delete(AppPaths.ProxyPath(item.SourcePath)); } catch { }
            try { File.Delete(AppPaths.AdjustmentXmlPath(item.SourcePath, item.VirtualCopyIndex)); } catch { }
            _items.RemoveAll(i => ReferenceEquals(i, item));
            RefreshStripKeepSelection();
            _status.Text = L.T("已刪除檔案");
        }
        catch (Exception ex)
        {
            MessageBox.Show(this, L.T("刪除失敗：") + ex.Message, "AwayPhotoRawEditor", MessageBoxButtons.OK, MessageBoxIcon.Error);
        }
    }

    private void ApplyPresetToSelection(string name)
    {
        foreach (var it in _strip.SelectedItems)
        {
            ImageAdjustments a;
            if (ReferenceEquals(it, _current) && _adj != null) { PushUndo(); a = _adj; }
            else a = AdjustmentXmlStore.Load(it.SourcePath, it.VirtualCopyIndex) ?? new ImageAdjustments();

            if (!PresetStore.ApplyCustom(name, a))
                PresetProfile.Get(name)?.ApplyTo(a);

            it.IsEdited = !a.IsDefault;
            if (!ReferenceEquals(it, _current))
                AdjustmentXmlStore.Save(it.SourcePath, a, it.VirtualCopyIndex, AdjustmentXmlStore.LoadExif(it.SourcePath, it.VirtualCopyIndex));
        }
        if (_adj != null) RebindAll();
        _strip.RefreshBadges();
        OnAdjustmentChanged(immediate: true);
        _status.Text = L.F("已套用風格檔：{0}", PresetProfile.BuiltIn.ContainsKey(name) ? L.T(name) : name);
    }

    private void RefreshStripKeepSelection()
    {
        // 預覽列被清空（最後一張被移除/刪除）：主編輯區的照片也要移除，
        // 面板回到預設並停用。
        if (_items.Count == 0)
        {
            _strip.SetItems(_items, 0);
            ClearEditor();
            return;
        }

        var cur = _current;
        int idx = cur != null ? _items.FindIndex(i => ReferenceEquals(i, cur)) : 0;
        _strip.SetItems(_items, Math.Max(0, idx));
        foreach (var it in _items)
        {
            var t = _loader.LoadThumbnailCache(it.SourcePath);
            if (t != null) _strip.SetImage(it.Key, t);
        }
    }

    private void SavePreviewList()
    {
        var preview = PreviewListStore.Load(_folder);
        preview.VirtualCopies = _items.Where(i => i.IsVirtualCopy)
            .Select(i => new VirtualCopyEntry { Path = i.SourcePath, Index = i.VirtualCopyIndex }).ToList();
        PreviewListStore.Save(_folder, preview);
    }

    // ---- app menu / settings / export entry points -----------------------

    private void ShowAppMenu(Control anchor)
    {
        var menu = new ContextMenuStrip { BackColor = Theme.PanelBg2, ForeColor = Theme.Text, ShowImageMargin = false };
        menu.Items.Add(L.T("開啟資料夾…"), null, (_, _) => PickFolder());
        var closeItem = menu.Items.Add(L.T("關閉資料夾"), null, (_, _) => CloseFolder());
        closeItem.Enabled = !string.IsNullOrEmpty(_folder);
        var closeDelItem = menu.Items.Add(L.T("關閉資料夾並刪除快取縮圖"), null, (_, _) => CloseFolderAndClearCache());
        closeDelItem.Enabled = !string.IsNullOrEmpty(_folder);

        // 還原被「移除於預覽列」隱藏的照片（顯示數量，無隱藏時停用）。
        int hiddenCount = 0;
        if (!string.IsNullOrEmpty(_folder))
            try { hiddenCount = PreviewListStore.Load(_folder).Hidden.Count; } catch { }
        var restoreItem = menu.Items.Add(
            hiddenCount > 0 ? L.F("還原已隱藏的照片（{0} 張）", hiddenCount) : L.T("還原已隱藏的照片"),
            null, (_, _) => RestoreHiddenPhotos());
        restoreItem.Enabled = hiddenCount > 0;

        var refreshItem = menu.Items.Add(L.T("重新整理資料夾  (F5)"), null, (_, _) => RefreshFolder());
        refreshItem.Enabled = !string.IsNullOrEmpty(_folder);

        // 紀錄：右側子選單顯示所有開過的資料夾紀錄（最近在前），點選即開啟。
        var recentItem = new ToolStripMenuItem(L.T("紀錄")) { ForeColor = Theme.Text };
        var recents = AppSettings.Current.RecentFolders;
        if (recents.Count == 0)
        {
            recentItem.DropDownItems.Add(new ToolStripMenuItem(L.T("（尚無開啟紀錄）")) { Enabled = false, ForeColor = Color.Black });
        }
        else
        {
            // 紀錄子選單用黑色字（灰色在淺色下拉選單上看不清楚）。
            foreach (var path in recents)
            {
                var p = path;
                var sub = new ToolStripMenuItem(p) { ForeColor = Color.Black, ToolTipText = p };
                sub.Click += (_, _) =>
                {
                    if (Directory.Exists(p)) OpenFolder(p);
                    else MessageBox.Show(this, L.F("資料夾已不存在：\n{0}", p), L.T("紀錄"), MessageBoxButtons.OK, MessageBoxIcon.Warning);
                };
                recentItem.DropDownItems.Add(sub);
            }
            recentItem.DropDownItems.Add(new ToolStripSeparator());
            var clear = recentItem.DropDownItems.Add(L.T("清除紀錄"));
            clear.ForeColor = Color.Black;
            clear.Click += (_, _) => { AppSettings.Current.RecentFolders.Clear(); AppSettings.Current.Save(); };
        }
        menu.Items.Add(recentItem);

        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(L.T("設定…"), null, (_, _) => ShowSettings());
        menu.Items.Add(L.T("編輯風格檔…"), null, (_, _) => ShowPresetEditor());
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(L.T("關於"), null, (_, _) => { using var dlg = new AboutForm(); dlg.ShowDialog(this); });
        menu.Items.Add(L.T("結束"), null, (_, _) => Close());
        menu.Show(anchor, new Point(0, anchor.Height));
    }

    /// <summary>編輯風格檔視窗：編輯/新增/恢復預設都在裡面；關閉後刷新下拉清單與縮圖右鍵選單。</summary>
    private void ShowPresetEditor()
    {
        using var dlg = new PresetEditorForm();
        dlg.ShowDialog(this);
        _preset.ReloadNames();
    }

    private void ShowSettings()
    {
        var previousStyle = AppSettings.Current.InterfaceStyle;
        var previousLanguage = AppSettings.Current.UiLanguage;
        using var dlg = new SettingsForm();
        if (dlg.ShowDialog(this) == DialogResult.OK)
        {
            if (previousLanguage != AppSettings.Current.UiLanguage)
            {
                Application.Restart();
                Close();
                return;
            }
            if (previousStyle != AppSettings.Current.InterfaceStyle)
                Theme.Apply(AppSettings.Current.InterfaceStyle, this);
            _strip.ShowNumber = AppSettings.Current.ShowThumbnailNumber;
            if (_leftScroll != null) _leftScroll.ScrollEnabled = AppSettings.Current.ShowColumnScrollBars;
            if (_rightScroll != null) _rightScroll.ScrollEnabled = AppSettings.Current.ShowColumnScrollBars;
            _loader.SyncFromSettings();
            // Adjustments unaffected, but re-render in case precision/decoder changed.
            if (_current != null) LoadPhoto(_current, force: true);
            UpdateStatus();
        }
    }

    /// <summary>Top-bar「匯出全部照片」— exports every photo in the preview list.</summary>
    private void ExportAll() => ExportPhotos(_items.ToList());

    /// <summary>Top-bar「匯出目前照片」— exports only the single photo shown in the main preview.</summary>
    private void ExportCurrent()
    {
        if (_current is null) { MessageBox.Show(this, L.T("目前沒有可匯出的照片。"), L.T("匯出"), MessageBoxButtons.OK, MessageBoxIcon.Information); return; }
        ExportPhotos(new List<PhotoItem> { _current });
    }

    /// <summary>Thumbnail-strip「匯出照片」— exports the current single/multi selection.</summary>
    private void ExportSelection() => ExportPhotos(_strip.SelectedItems.ToList());

    private void ExportPhotos(List<PhotoItem> targets)
    {
        if (targets.Count == 0) { MessageBox.Show(this, L.T("沒有可匯出的相片。"), L.T("匯出"), MessageBoxButtons.OK, MessageBoxIcon.Information); return; }
        SaveCurrentIfDirty();

        // The 標誌/watermark is edited live inside the dialog: it mutates _exportSettings and
        // raises WatermarkChanged so the main preview reflects the toggle in real time.
        var settings = _exportSettings;
        using var dlg = new ExportForm(settings, targets.Count);
        dlg.WatermarkChanged += () => { if (_current != null) _scheduler.Schedule(immediate: true); };
        if (dlg.ShowDialog(this) != DialogResult.OK || !dlg.StartRequested) return;

        List<string>? written = null;
        using (var pf = new ProgressForm(L.T("匯出相片"),
            async (prog, token) => written = await Exporter.Export(targets, settings, _loader, prog, token)))
        {
            pf.ShowDialog(this);
            if (pf.Error != null)
            {
                MessageBox.Show(this, pf.Error.Message, L.T("匯出失敗"), MessageBoxButtons.OK, MessageBoxIcon.Error);
                return;
            }
            if (pf.Canceled) { _status.Text = L.T("匯出已取消"); return; }
        }
        _status.Text = written != null ? L.F("已匯出 {0} 張相片", written.Count) : L.T("匯出完成");
    }

    // ---- status / lifecycle ---------------------------------------------

    private void UpdateStatus()
    {
        _status.Text = !_loader.LibRawAvailable ? L.T("未使用LibRaw讀取")
            : _loader.LastFullDecodeUsedLibRaw ? L.T("LibRaw 讀取中")
            : AppSettings.Current.UseLibRaw ? L.T("LibRaw 已啟用") : L.T("未使用LibRaw讀取");
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        // Don't hijack keys while the user is typing (slider click-to-type editor, combos…):
        // Del / arrows / Ctrl+Z belong to the textbox there. Also ignore everything in
        // headless diagnostic mode — the off-screen window can steal real keystrokes.
        if (Program.Headless || IsTextInputActive()) { base.OnKeyDown(e); return; }

        if (e.Control && e.KeyCode == Keys.Z) { DoUndo(); e.Handled = true; }
        else if (e.Control && e.KeyCode == Keys.Y) { DoRedo(); e.Handled = true; }
        else if (e.KeyCode == Keys.Left) { StepSelection(-1); e.Handled = true; }
        else if (e.KeyCode == Keys.Right) { StepSelection(1); e.Handled = true; }
        else if (e.KeyCode == Keys.Delete && e.Shift) { if (_strip.CurrentItem is { } del) DeletePhotoFile(del); e.Handled = true; }
        else if (e.KeyCode == Keys.Delete) { RemoveSelectedFromPreview(); e.Handled = true; }
        else if (e.KeyCode == Keys.Escape) { CancelPickerOrTool(); e.Handled = true; }
        else if (e.KeyCode == Keys.F5) { RefreshFolder(); e.Handled = true; }
        else if (e.KeyCode == Keys.Oem5) { ToggleShowOriginal(); e.Handled = true; }   // '\' 對照原圖
        base.OnKeyDown(e);
    }

    /// <summary>True when the focused control is a text-input (so global shortcuts must not fire).</summary>
    private bool IsTextInputActive()
    {
        Control? c = ActiveControl;
        while (c is ContainerControl { ActiveControl: not null } cc) c = cc.ActiveControl;
        return c is TextBoxBase or ComboBox or NumericUpDown;
    }

    /// <summary>Del：把目前選取的照片全部「移除於預覽列」（可由選單還原）。</summary>
    private void RemoveSelectedFromPreview()
    {
        var sel = _strip.SelectedItems.ToList();
        foreach (var it in sel) RemoveFromPreview(it);
    }

    /// <summary>Esc：先取消白平衡滴管，否則取消目前選取的工具。</summary>
    private void CancelPickerOrTool()
    {
        if (_viewer.WhiteBalancePickerActive)
        {
            _viewer.WhiteBalancePickerActive = false;
            _color.SetPickerChecked(false);
            _viewer.Invalidate();
        }
        else if (_viewer.Tool != ToolMode.None)
            _tools.SelectTool(ToolMode.None);
    }

    /// <summary>F5：重新掃描目前資料夾（外部新增/移除照片後），盡量保留目前選取。</summary>
    private void RefreshFolder()
    {
        if (string.IsNullOrEmpty(_folder)) return;
        var curKey = _current?.Key;
        OpenFolder(_folder);
        if (curKey != null)
        {
            int idx = _items.FindIndex(i => i.Key == curKey);
            if (idx > 0) _strip.SelectIndex(idx);   // SelectionChanged → LoadPhoto
        }
        _status.Text = L.T("已重新整理資料夾");
    }

    /// <summary>對照原圖（工具列鈕 / '\' 鍵）：以中性調整（保留裁切與旋轉）重算畫面。</summary>
    private void ToggleShowOriginal()
    {
        if (_adj is null) return;
        _showOriginal = !_showOriginal;
        _origBtn.Primary = _showOriginal;
        _scheduler.Schedule(immediate: true);
    }

    /// <summary>Resolve a drag-drop payload to a folder to open (folder itself, or an image file's folder).</summary>
    private static string? GetDroppedFolder(DragEventArgs e)
    {
        if (e.Data?.GetData(DataFormats.FileDrop) is not string[] paths || paths.Length == 0) return null;
        var p = paths[0];
        if (Directory.Exists(p)) return AppPaths.IsRawTemp(p) ? Path.GetDirectoryName(p) : p;
        if (File.Exists(p) && AppPaths.IsSupported(p)) return Path.GetDirectoryName(p);
        return null;
    }

    private void StepSelection(int delta)
    {
        if (_current is null || _items.Count == 0) return;
        int idx = _items.FindIndex(i => ReferenceEquals(i, _current));
        idx = Math.Clamp(idx + delta, 0, _items.Count - 1);
        _strip.SelectIndex(idx);
    }

    protected override void OnFormClosing(FormClosingEventArgs e)
    {
        _scheduler.CancelPending();
        SaveCurrentIfDirty();
        if (!string.IsNullOrEmpty(_folder)) SavePreviewList();
        base.OnFormClosing(e);
    }
}
