using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;
using AwayPhotoRawEditor.Imaging;

namespace AwayPhotoRawEditor.Forms;

/// <summary>設定: 使用 LibRaw / 高精度 RAW 處理流程. Saved settings affect RawLoader.</summary>
public sealed class SettingsForm : Form
{
    private readonly DarkCheckBox _useLibRaw;
    private readonly ComboBox _precision;   // RAW 處理精度：0 = 8-bit、1 = 16-bit（存成 UseHighPrecisionRawPipeline）
    private readonly DarkCheckBox _showNumber;
    private readonly DarkCheckBox _showScrollBars;
    private readonly DarkCheckBox _useGpu;
    private readonly UiStyleCard _classicCard;
    private readonly UiStyleCard _paperCard;
    private readonly ComboBox _language;
    private readonly ComboBox _uiScale;
    private readonly Label _fontSummary;
    private FontSizes _fonts;
    private UiStyle _selectedStyle;

    /// <summary>可選的固定介面倍率（0 = 自動，另外列在最前面）。</summary>
    private static readonly int[] ScalePercents = { 100, 125, 150, 175, 200 };

    private sealed record ScaleChoice(int Percent, string Display)
    {
        public override string ToString() => Display;
    }

    public SettingsForm()
    {
        Text = "設定";
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false; MinimizeBox = false;
        StartPosition = FormStartPosition.CenterParent;
        ClientSize = Ui.FitWorkArea(548, 728);

        // 以下座標一律是 96 DPI 設計值，透過 Ui.Place / Ui.S 縮放。
        var header = new Label { Text = "設定", Font = Theme.UIPx(Theme.Sizes.DialogTitle, FontStyle.Bold), ForeColor = Theme.Text };
        Ui.Place(header, 20, 14, 500, 28);
        var themeLabel = SectionLabel("介面風格", 48);
        var themeHint = new Label
        {
            Text = "點選預覽即可切換，套用後立即生效",
            ForeColor = Theme.TextFaint, Font = Theme.Small
        };
        Ui.Place(themeHint, 225, 50, 303, 20);

        _fonts = AppSettings.Current.FontSizes.Clone();   // 按確定才寫回
        _selectedStyle = AppSettings.Current.InterfaceStyle;
        _classicCard = new UiStyleCard(
            UiStyle.ClassicDark, L.T("經典深色"), L.T("低亮度專業工作區\n藍色重點操作"));
        Ui.Place(_classicCard, 20, 74, 248, 112);
        _paperCard = new UiStyleCard(
            UiStyle.WarmPaper, L.T("暖白相紙"), L.T("明亮暖灰工作區\n陶土橘重點操作"));
        Ui.Place(_paperCard, 280, 74, 248, 112);
        _classicCard.Click += (_, _) => SelectStyle(UiStyle.ClassicDark);
        _paperCard.Click += (_, _) => SelectStyle(UiStyle.WarmPaper);
        SelectStyle(_selectedStyle);

        var languageLabel = SectionLabel("語言", 210);
        _language = UiFactory.Combo(
            L.LanguageDisplayName(AppLanguage.TraditionalChinese),
            L.LanguageDisplayName(AppLanguage.English),
            L.LanguageDisplayName(AppLanguage.Japanese),
            L.LanguageDisplayName(AppLanguage.Korean),
            L.LanguageDisplayName(AppLanguage.SimplifiedChinese),
            L.LanguageDisplayName(AppLanguage.German),
            L.LanguageDisplayName(AppLanguage.French),
            L.LanguageDisplayName(AppLanguage.Spanish));
        Ui.Place(_language, 20, 234, 260, 28);
        _language.SelectedIndex = System.Math.Clamp((int)AppSettings.Current.UiLanguage, 0, 7);
        var languageHint = new Label
        {
            Text = "變更語言後將自動重新啟動程式",
            ForeColor = Theme.TextFaint, Font = Theme.Small
        };
        Ui.Place(languageHint, 294, 237, 234, 22);

        // 介面大小：自動 = 跟隨系統縮放，但不超過螢幕容得下的大小。
        var scaleLabel = SectionLabel("介面大小", 280);
        _uiScale = UiFactory.Combo();
        _uiScale.Items.Add(new ScaleChoice(0, $"{L.T("自動（依螢幕大小）")}　—　{Ui.AutoFitScale() * 100:0}%"));
        foreach (int p in ScalePercents) _uiScale.Items.Add(new ScaleChoice(p, $"{p}%"));
        Ui.Place(_uiScale, 20, 304, 260, 28);
        int curScale = AppSettings.Current.UiScalePercent;
        _uiScale.SelectedIndex = Math.Max(0, _uiScale.Items.Cast<ScaleChoice>().ToList().FindIndex(c => c.Percent == curScale));
        var scaleHint = new Label
        {
            Text = "變更介面大小後將自動重新啟動程式",
            ForeColor = Theme.TextFaint, Font = Theme.Small
        };
        Ui.Place(scaleHint, 294, 307, 234, 22);

        // 字體大小…：逐項微調各字級（介面大小是等比縮放整份 UI，這個是個別字級的比例）
        var fontBtn = new FlatButton { Text = "字體大小…" };
        Ui.Place(fontBtn, 20, 338, 130, 30);
        fontBtn.Click += (_, _) =>
        {
            using var dlg = new FontSizeForm(_fonts);
            if (dlg.ShowDialog(this) == DialogResult.OK)
            {
                _fonts = dlg.Result;
                UpdateFontSummary();
            }
        };
        _fontSummary = new Label { ForeColor = Theme.TextFaint, Font = Theme.Small, TextAlign = ContentAlignment.MiddleLeft };
        Ui.Place(_fontSummary, 158, 341, 370, 24);
        UpdateFontSummary();

        var optionsLabel = SectionLabel("一般選項", 386);
        _useLibRaw = NewCheck("使用 LibRaw", AppSettings.Current.UseLibRaw, 412);
        // RAW 處理精度：二選一的品質等級（不是功能開關），所以用下拉；旁邊「說明」點了跳簡短敘述。
        // 版面：標籤 20..150、下拉 156..346、說明連結 354 起（德文標籤故意挑短的翻譯，150 放得下）
        var precisionLabel = new Label
        {
            Text = L.T("RAW 處理精度"), ForeColor = Theme.Text, Font = Theme.Normal,
            TextAlign = ContentAlignment.MiddleLeft, AutoEllipsis = true
        };
        Ui.Place(precisionLabel, 20, 442, 132, 26);
        _precision = UiFactory.Combo(L.T("8-bit（省空間）"), L.T("16-bit（高精度）"));
        Ui.Place(_precision, 156, 441, 190, 28);
        _precision.SelectedIndex = AppSettings.Current.UseHighPrecisionRawPipeline ? 1 : 0;
        var precisionHelp = new LinkLabel
        {
            Text = L.T("說明"), Font = Theme.Normal, BackColor = Theme.WindowBg,
            LinkColor = Theme.AccentHover, ActiveLinkColor = Theme.Accent, VisitedLinkColor = Theme.AccentHover,
            LinkBehavior = LinkBehavior.HoverUnderline, TextAlign = ContentAlignment.MiddleLeft, AutoSize = false
        };
        Ui.Place(precisionHelp, 354, 442, 170, 26);
        precisionHelp.LinkClicked += (_, _) =>
            MessageBox.Show(this, L.T(L.RawPrecisionHelp), L.T("RAW 處理精度"), MessageBoxButtons.OK, MessageBoxIcon.Information);
        _showNumber = NewCheck("在縮圖左上顯示編號 (#1, #2 …)", AppSettings.Current.ShowThumbnailNumber, 472);
        _showScrollBars = NewCheck("顯示捲軸（視窗過矮時左右欄可捲動）", AppSettings.Current.ShowColumnScrollBars, 502);
        _useGpu = NewCheck("使用 GPU 加速算圖（偵測不到或失敗時自動改用 CPU）", AppSettings.Current.UseGpu, 532);

        string libState = LibRawInterop.Available ? "libraw.dll 已載入" : "libraw.dll 未找到（將退回 WIC / 嵌入預覽）";
        var note = new Label { Text = libState, ForeColor = Theme.TextFaint, Font = Theme.Small };
        Ui.Place(note, 20, 568, 500, 20);
        string exifState = Exif.ExifReader.ExifToolAvailable ? "exiftool.exe 已載入" : "exiftool.exe 未找到（將退回 WIC metadata）";
        var note2 = new Label { Text = exifState, ForeColor = Theme.TextFaint, Font = Theme.Small };
        Ui.Place(note2, 20, 588, 500, 20);
        // GPU 狀態：裝置名稱，或為什麼沒在用（設定關閉／沒有 D3D12 硬體裝置）
        var note3 = new Label { Text = GpuStateText(), ForeColor = Theme.TextFaint, Font = Theme.Small };
        Ui.Place(note3, 20, 608, 500, 20);

        var bottom = new Panel { Dock = DockStyle.Bottom, Height = Ui.S(60), BackColor = Theme.PanelBg2 };
        var ok = new FlatButton { Text = "套用", Primary = true }; Ui.Place(ok, 340, 14, 88, 32);
        var cancel = new FlatButton { Text = "取消" }; Ui.Place(cancel, 436, 14, 96, 32);
        // 恢復預設：只把視窗裡的控制項改回預設值（按「套用」才寫檔、按「取消」就丟掉），與字體大小視窗的行為一致。
        // 預設值從 new AppSettings() 取，免得和 AppSettings 的初始值脫節。語言刻意不動——那是使用者的身分設定，不是偏好。
        var defaults = new FlatButton { Text = "恢復預設" }; Ui.Place(defaults, 16, 14, 210, 32);   // 210：德文 "Standard wiederherstellen" 才放得下
        defaults.Click += (_, _) =>
        {
            var d = new AppSettings();
            SelectStyle(d.InterfaceStyle);
            _uiScale.SelectedIndex = 0;                       // 自動（依螢幕大小）
            _fonts = new FontSizes();
            UpdateFontSummary();
            _useLibRaw.Checked = d.UseLibRaw;
            _precision.SelectedIndex = d.UseHighPrecisionRawPipeline ? 1 : 0;
            _showNumber.Checked = d.ShowThumbnailNumber;
            _showScrollBars.Checked = d.ShowColumnScrollBars;
            _useGpu.Checked = d.UseGpu;
        };
        void ApplyAndClose()
        {
            AppSettings.Current.InterfaceStyle = _selectedStyle;
            AppSettings.Current.UiLanguage = (AppLanguage)Math.Max(0, _language.SelectedIndex);
            if (_uiScale.SelectedItem is ScaleChoice sc) AppSettings.Current.UiScalePercent = sc.Percent;
            AppSettings.Current.FontSizes = _fonts;
            AppSettings.Current.UseLibRaw = _useLibRaw.Checked;
            AppSettings.Current.UseHighPrecisionRawPipeline = _precision.SelectedIndex == 1;
            AppSettings.Current.ShowThumbnailNumber = _showNumber.Checked;
            AppSettings.Current.ShowColumnScrollBars = _showScrollBars.Checked;
            AppSettings.Current.UseGpu = _useGpu.Checked;
            Imaging.Gpu.GpuPipeline.Enabled = _useGpu.Checked;
            AppSettings.Current.Save();
            DialogResult = DialogResult.OK;
            Close();
        }
        void CancelAndClose()
        {
            DialogResult = DialogResult.Cancel;
            Close();
        }
        ok.Click += (_, _) => ApplyAndClose();
        cancel.Click += (_, _) => CancelAndClose();
        bottom.Controls.AddRange(new Control[] { defaults, ok, cancel });

        Controls.AddRange(new Control[]
        {
            header, themeLabel, themeHint, _classicCard, _paperCard,
            languageLabel, _language, languageHint,
            scaleLabel, _uiScale, scaleHint, fontBtn, _fontSummary, optionsLabel,
            _useLibRaw, precisionLabel, _precision, precisionHelp, _showNumber, _showScrollBars, _useGpu, note, note2, note3, bottom
        });
        L.Apply(this);
        KeyPreview = true;
        KeyDown += (_, e) =>
        {
            if (e.KeyCode == Keys.Enter && ActiveControl is not UiStyleCard)
            {
                ApplyAndClose();
                e.SuppressKeyPress = true;
            }
            else if (e.KeyCode == Keys.Escape) { CancelAndClose(); e.SuppressKeyPress = true; }
        };
    }

    private static Label SectionLabel(string text, int top)
    {
        var l = new Label { Text = text, ForeColor = Theme.Accent, Font = Theme.UIPx(Theme.Sizes.Normal, FontStyle.Bold) };
        Ui.Place(l, 20, top, 180, 20);
        return l;
    }

    /// <summary>設定視窗的 GPU 一行：翻譯的前綴 + 執行期裝置名稱（裝置名稱不翻譯）。</summary>
    private static string GpuStateText()
    {
        if (!AppSettings.Current.UseGpu) return L.T("GPU：已停用（設定）");
        return Imaging.Gpu.GpuPipeline.IsAvailable
            ? L.T("GPU：") + Imaging.Gpu.GpuPipeline.DeviceName
            : L.T("GPU：未偵測到可用裝置，使用 CPU");
    }

    private static DarkCheckBox NewCheck(string text, bool value, int top)
    {
        var check = new DarkCheckBox
        {
            Text = text, BackColor = Theme.WindowBg, Checked = value
        };
        Ui.Place(check, 20, top, 500, 24);
        return check;
    }

    /// <summary>按鈕右側的一行摘要：預設就寫「預設比例」，改過才列出主要字級。</summary>
    private void UpdateFontSummary()
    {
        var def = new FontSizes();
        bool isDefault = _fonts.Signature() == def.Signature();
        _fontSummary.Text = isDefault
            ? L.T("預設比例")
            : L.F("已自訂：一般 {0}px、區塊標題 {1}px、小字 {2}px",
                _fonts.Normal, _fonts.SectionTitle, _fonts.Small);
    }

    private void SelectStyle(UiStyle style)
    {
        _selectedStyle = style;
        _classicCard.Selected = style == UiStyle.ClassicDark;
        _paperCard.Selected = style == UiStyle.WarmPaper;
    }

    /// <summary>Clickable theme card with a miniature, palette-accurate UI preview.</summary>
    private sealed class UiStyleCard : Control
    {
        private bool _selected;
        private bool _hover;
        private readonly string _title;
        private readonly string _description;

        public UiStyle Style { get; }
        public bool Selected
        {
            get => _selected;
            set { if (_selected != value) { _selected = value; Invalidate(); } }
        }

        public UiStyleCard(UiStyle style, string title, string description)
        {
            Style = style;
            _title = title;
            _description = description;
            TabStop = true;
            Cursor = Cursors.Hand;
            AccessibleName = title;
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                     ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw |
                     ControlStyles.Selectable, true);
        }

        protected override void OnMouseEnter(System.EventArgs e)
        {
            _hover = true; Invalidate(); base.OnMouseEnter(e);
        }

        protected override void OnMouseDown(MouseEventArgs e)
        {
            Focus();
            base.OnMouseDown(e);
        }

        protected override void OnMouseLeave(System.EventArgs e)
        {
            _hover = false; Invalidate(); base.OnMouseLeave(e);
        }

        protected override void OnGotFocus(System.EventArgs e)
        {
            Invalidate(); base.OnGotFocus(e);
        }

        protected override void OnLostFocus(System.EventArgs e)
        {
            Invalidate(); base.OnLostFocus(e);
        }

        protected override void OnKeyDown(KeyEventArgs e)
        {
            if (e.KeyCode is Keys.Enter or Keys.Space)
            {
                OnClick(System.EventArgs.Empty);
                e.Handled = true;
            }
            base.OnKeyDown(e);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            PaintHelpers.EnableHighQuality(g);
            var card = new RectangleF(1, 1, Width - 2, Height - 2);
            var fill = _hover ? Theme.PanelBg3 : Theme.PanelBg;
            var border = Selected || Focused ? Theme.Accent : _hover ? Theme.BorderLight : Theme.Border;
            PaintHelpers.FillRounded(g, card, Ui.S(8f), fill);
            PaintHelpers.DrawRounded(g, card, Ui.S(8f), border, Selected ? Ui.S(2f) : Ui.SMin(1));

            TextRenderer.DrawText(g, _title, Theme.Header, new Rectangle(Ui.S(12), Ui.S(8), Width - Ui.S(50), Ui.S(22)),
                Theme.Text, TextFormatFlags.Left | TextFormatFlags.VerticalCenter);

            // Selection indicator.
            var ring = new RectangleF(Width - Ui.S(29f), Ui.S(10f), Ui.S(16f), Ui.S(16f));
            using (var ringFill = new SolidBrush(Theme.PanelBg3)) g.FillEllipse(ringFill, ring);
            using (var ringPen = new Pen(Selected ? Theme.Accent : Theme.BorderLight, Selected ? Ui.S(2f) : Ui.SMin(1)))
                g.DrawEllipse(ringPen, ring);
            if (Selected)
            {
                using var dot = new SolidBrush(Theme.Accent);
                g.FillEllipse(dot, Width - Ui.S(24.5f), Ui.S(14.5f), Ui.S(7f), Ui.S(7f));
            }

            DrawPreview(g, Theme.PaletteFor(Style), Ui.Rect(12, 38, 92, 60));
            TextRenderer.DrawText(g, _description, Theme.Small,
                new Rectangle(Ui.S(116), Ui.S(40), Width - Ui.S(126), Ui.S(52)), Theme.TextDim,
                TextFormatFlags.Left | TextFormatFlags.WordBreak);
        }

        /// <summary>迷你 UI 縮圖預覽：座標同樣是 96 DPI 設計值。</summary>
        private static void DrawPreview(Graphics g, Theme.Palette p, Rectangle r)
        {
            PaintHelpers.FillRounded(g, r, Ui.S(5f), p.WindowBg);
            using (var border = new Pen(p.Border, Ui.SMin(1))) g.DrawRectangle(border, r);
            using (var bar = new SolidBrush(p.PanelBg2))
                g.FillRectangle(bar, r.X + Ui.S(1), r.Y + Ui.S(1), r.Width - Ui.S(2), Ui.S(13));
            using (var side = new SolidBrush(p.PanelBg))
            {
                g.FillRectangle(side, r.X + Ui.S(1), r.Y + Ui.S(15), Ui.S(23), r.Height - Ui.S(16));
                g.FillRectangle(side, r.Right - Ui.S(20), r.Y + Ui.S(15), Ui.S(19), r.Height - Ui.S(16));
            }
            using (var viewer = new SolidBrush(p.ViewerBg))
                g.FillRectangle(viewer, r.X + Ui.S(26), r.Y + Ui.S(17), r.Width - Ui.S(48), r.Height - Ui.S(24));
            using (var accent = new SolidBrush(p.Accent))
                g.FillRectangle(accent, r.X + Ui.S(65), r.Y + Ui.S(5), Ui.S(19), Ui.S(5));
            using (var text = new Pen(p.TextDim, Ui.SMin(1)))
            {
                g.DrawLine(text, r.X + Ui.S(5), r.Y + Ui.S(22), r.X + Ui.S(19), r.Y + Ui.S(22));
                g.DrawLine(text, r.X + Ui.S(5), r.Y + Ui.S(28), r.X + Ui.S(17), r.Y + Ui.S(28));
                g.DrawLine(text, r.Right - Ui.S(16), r.Y + Ui.S(23), r.Right - Ui.S(5), r.Y + Ui.S(23));
            }
        }
    }
}
