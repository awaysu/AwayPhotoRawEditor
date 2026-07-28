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
    private readonly DarkCheckBox _highPrecision;
    private readonly DarkCheckBox _showNumber;
    private readonly DarkCheckBox _showScrollBars;
    private readonly UiStyleCard _classicCard;
    private readonly UiStyleCard _paperCard;
    private readonly ComboBox _language;
    private UiStyle _selectedStyle;

    public SettingsForm()
    {
        Text = "設定";
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false; MinimizeBox = false;
        StartPosition = FormStartPosition.CenterParent;
        ClientSize = new Size(548, 562);

        var header = new Label
        {
            Text = "設定", Font = Theme.UI(13f, FontStyle.Bold), ForeColor = Theme.Text,
            Left = 20, Top = 14, Width = 500, Height = 28
        };
        var themeLabel = SectionLabel("介面風格", 48);
        var themeHint = new Label
        {
            Text = "點選預覽即可切換，套用後立即生效",
            ForeColor = Theme.TextFaint, Font = Theme.Small,
            Left = 225, Top = 50, Width = 303, Height = 20
        };

        _selectedStyle = AppSettings.Current.InterfaceStyle;
        _classicCard = new UiStyleCard(
            UiStyle.ClassicDark, L.T("經典深色"), L.T("低亮度專業工作區\n藍色重點操作"));
        _classicCard.SetBounds(20, 74, 248, 112);
        _paperCard = new UiStyleCard(
            UiStyle.WarmPaper, L.T("暖白相紙"), L.T("明亮暖灰工作區\n陶土橘重點操作"));
        _paperCard.SetBounds(280, 74, 248, 112);
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
        _language.SetBounds(20, 234, 260, 28);
        _language.SelectedIndex = System.Math.Clamp((int)AppSettings.Current.UiLanguage, 0, 7);
        var languageHint = new Label
        {
            Text = "變更語言後將自動重新啟動程式",
            ForeColor = Theme.TextFaint, Font = Theme.Small,
            Left = 294, Top = 237, Width = 234, Height = 22
        };

        var optionsLabel = SectionLabel("一般選項", 280);
        _useLibRaw = NewCheck("使用 LibRaw", AppSettings.Current.UseLibRaw, 306);
        _highPrecision = NewCheck("高精度 RAW 處理流程 (16-bit / float)", AppSettings.Current.UseHighPrecisionRawPipeline, 336);
        _showNumber = NewCheck("在縮圖左上顯示編號 (#1, #2 …)", AppSettings.Current.ShowThumbnailNumber, 366);
        _showScrollBars = NewCheck("顯示捲軸（視窗過矮時左右欄可捲動）", AppSettings.Current.ShowColumnScrollBars, 396);

        string libState = LibRawInterop.Available ? "libraw.dll 已載入" : "libraw.dll 未找到（將退回 WIC / 嵌入預覽）";
        var note = new Label { Text = libState, ForeColor = Theme.TextFaint, Font = Theme.Small, Left = 20, Top = 432, Width = 500, Height = 20 };
        string exifState = Exif.ExifReader.ExifToolAvailable ? "exiftool.exe 已載入" : "exiftool.exe 未找到（將退回 WIC metadata）";
        var note2 = new Label { Text = exifState, ForeColor = Theme.TextFaint, Font = Theme.Small, Left = 20, Top = 452, Width = 500, Height = 20 };

        var bottom = new Panel { Dock = DockStyle.Bottom, Height = 60, BackColor = Theme.PanelBg2 };
        var ok = new FlatButton { Text = "套用", Primary = true, Left = 340, Top = 14, Width = 88, Height = 32 };
        var cancel = new FlatButton { Text = "取消", Left = 436, Top = 14, Width = 96, Height = 32 };
        void ApplyAndClose()
        {
            AppSettings.Current.InterfaceStyle = _selectedStyle;
            AppSettings.Current.UiLanguage = (AppLanguage)Math.Max(0, _language.SelectedIndex);
            AppSettings.Current.UseLibRaw = _useLibRaw.Checked;
            AppSettings.Current.UseHighPrecisionRawPipeline = _highPrecision.Checked;
            AppSettings.Current.ShowThumbnailNumber = _showNumber.Checked;
            AppSettings.Current.ShowColumnScrollBars = _showScrollBars.Checked;
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
        bottom.Controls.AddRange(new Control[] { ok, cancel });

        Controls.AddRange(new Control[]
        {
            header, themeLabel, themeHint, _classicCard, _paperCard,
            languageLabel, _language, languageHint, optionsLabel,
            _useLibRaw, _highPrecision, _showNumber, _showScrollBars, note, note2, bottom
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

    private static Label SectionLabel(string text, int top) => new()
    {
        Text = text, ForeColor = Theme.Accent, Font = Theme.UI(9f, FontStyle.Bold),
        Left = 20, Top = top, Width = 180, Height = 20
    };

    private static DarkCheckBox NewCheck(string text, bool value, int top)
    {
        var check = new DarkCheckBox
        {
            Text = text, BackColor = Theme.WindowBg, Checked = value
        };
        check.SetBounds(20, top, 500, 24);
        return check;
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
            PaintHelpers.FillRounded(g, card, 8, fill);
            PaintHelpers.DrawRounded(g, card, 8, border, Selected ? 2f : 1f);

            TextRenderer.DrawText(g, _title, Theme.Header, new Rectangle(12, 8, Width - 50, 22),
                Theme.Text, TextFormatFlags.Left | TextFormatFlags.VerticalCenter);

            // Selection indicator.
            var ring = new RectangleF(Width - 29, 10, 16, 16);
            using (var ringFill = new SolidBrush(Theme.PanelBg3)) g.FillEllipse(ringFill, ring);
            using (var ringPen = new Pen(Selected ? Theme.Accent : Theme.BorderLight, Selected ? 2f : 1f))
                g.DrawEllipse(ringPen, ring);
            if (Selected)
            {
                using var dot = new SolidBrush(Theme.Accent);
                g.FillEllipse(dot, Width - 24.5f, 14.5f, 7, 7);
            }

            DrawPreview(g, Theme.PaletteFor(Style), new Rectangle(12, 38, 92, 60));
            TextRenderer.DrawText(g, _description, Theme.Small,
                new Rectangle(116, 40, Width - 126, 52), Theme.TextDim,
                TextFormatFlags.Left | TextFormatFlags.WordBreak);
        }

        private static void DrawPreview(Graphics g, Theme.Palette p, Rectangle r)
        {
            PaintHelpers.FillRounded(g, r, 5, p.WindowBg);
            using (var border = new Pen(p.Border)) g.DrawRectangle(border, r);
            using (var bar = new SolidBrush(p.PanelBg2))
                g.FillRectangle(bar, r.X + 1, r.Y + 1, r.Width - 2, 13);
            using (var side = new SolidBrush(p.PanelBg))
            {
                g.FillRectangle(side, r.X + 1, r.Y + 15, 23, r.Height - 16);
                g.FillRectangle(side, r.Right - 20, r.Y + 15, 19, r.Height - 16);
            }
            using (var viewer = new SolidBrush(p.ViewerBg))
                g.FillRectangle(viewer, r.X + 26, r.Y + 17, r.Width - 48, r.Height - 24);
            using (var accent = new SolidBrush(p.Accent))
                g.FillRectangle(accent, r.X + 65, r.Y + 5, 19, 5);
            using (var text = new Pen(p.TextDim, 1))
            {
                g.DrawLine(text, r.X + 5, r.Y + 22, r.X + 19, r.Y + 22);
                g.DrawLine(text, r.X + 5, r.Y + 28, r.X + 17, r.Y + 28);
                g.DrawLine(text, r.Right - 16, r.Y + 23, r.Right - 5, r.Y + 23);
            }
        }
    }
}
