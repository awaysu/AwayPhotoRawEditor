using System;
using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;

namespace AwayPhotoRawEditor.Forms;

/// <summary>
/// 第一次執行（%AppData% 還沒有 settings.xml）時，在建立任何其他視窗之前跳出的語言選擇畫面。
/// 說明文字固定中英雙語，選項同時列出該語言的自稱與英文名稱，讓任何國家的使用者都能先把
/// 介面切成看得懂的語言。Program 是在這個視窗關閉之後才 L.SetLanguage()，所以不必重新啟動。
/// </summary>
public sealed class FirstRunLanguageForm : Form
{
    // ⚠️ 本檔案刻意完全不引用 Theme。Theme.Small/Normal/Header 是 static readonly 的快取字型，
    // 字型家族取自 L.CurrentLanguage——只要碰到 Theme 一下，字型就會以「使用者還沒選的語言」
    // 定型，之後只能靠 Application.Restart() 重建（見 CLAUDE.md「高 DPI 縮放」）。
    // 所以色盤直接寫死 ClassicDark：第一次執行本來就一定是預設主題。
    private static readonly Color WindowBg = Color.FromArgb(0x1E, 0x1E, 0x1E);
    private static readonly Color PanelBg = Color.FromArgb(0x25, 0x25, 0x25);
    private static readonly Color PanelBg2 = Color.FromArgb(0x2D, 0x2D, 0x2D);
    private static readonly Color PanelBg3 = Color.FromArgb(0x3A, 0x3A, 0x3A);
    private static readonly Color BorderCol = Color.FromArgb(60, 60, 62);
    private static readonly Color BorderLight = Color.FromArgb(82, 82, 86);
    private static readonly Color TextFg = Color.FromArgb(224, 224, 228);
    private static readonly Color TextDim = Color.FromArgb(150, 150, 158);
    private static readonly Color Accent = Color.FromArgb(45, 120, 220);
    private static readonly Color AccentHover = Color.FromArgb(60, 140, 240);
    private static readonly Color AccentDim = Color.FromArgb(38, 90, 160);

    /// <summary>自稱 + 英文名稱。缺字型時（例如德文版 Windows 沒裝 CJK 字型）自稱會顯示成
    /// 方框，英文名稱是唯一還讀得懂的線索，所以兩個都要畫出來。</summary>
    private static readonly (AppLanguage Lang, string Native, string English)[] Choices =
    {
        (AppLanguage.TraditionalChinese, "繁體中文", "Chinese (Traditional)"),
        (AppLanguage.English,            "English",  "English (United States)"),
        (AppLanguage.Japanese,           "日本語",   "Japanese"),
        (AppLanguage.Korean,             "한국어",   "Korean"),
        (AppLanguage.SimplifiedChinese,  "简体中文", "Chinese (Simplified)"),
        (AppLanguage.German,             "Deutsch",  "German"),
        (AppLanguage.French,             "Français", "French"),
        (AppLanguage.Spanish,            "Español",  "Spanish"),
    };

    private readonly LanguageCard[] _cards = new LanguageCard[Choices.Length];
    private int _index;

    /// <summary>使用者選定的語言（直接關掉視窗時就是預選的那個，不會有「沒選」的狀態）。</summary>
    public AppLanguage SelectedLanguage => Choices[_index].Lang;

    public FirstRunLanguageForm(AppLanguage preselect)
    {
        Text = "AwayPhotoRawEditor";
        BackColor = WindowBg;
        ForeColor = TextFg;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false; MinimizeBox = false;
        StartPosition = FormStartPosition.CenterScreen;   // 這時還沒有主視窗可以置中
        ClientSize = Ui.FitWorkArea(520, 424);
        try { Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { }

        // 以下座標一律是 96 DPI 設計值，透過 Ui.Place / Ui.S 縮放。
        var title = new Label
        {
            Text = "選擇語言　/　Select Language",
            Font = Px(20, FontStyle.Bold), ForeColor = TextFg
        };
        Ui.Place(title, 24, 18, 472, 30);

        var hintZh = new Label
        {
            Text = "請選擇介面語言，稍後可在「設定」中變更。",
            Font = Px(14), ForeColor = TextDim
        };
        Ui.Place(hintZh, 24, 54, 472, 22);

        var hintEn = new Label
        {
            Text = "Choose your interface language. You can change it later in Settings.",
            Font = Px(14), ForeColor = TextDim
        };
        Ui.Place(hintEn, 24, 76, 472, 22);

        for (int i = 0; i < Choices.Length; i++)
        {
            var (lang, native, english) = Choices[i];
            var card = new LanguageCard(native, english);
            Ui.Place(card, i % 2 == 0 ? 24 : 264, 110 + (i / 2) * 66, 232, 58);
            int captured = i;
            card.Click += (_, _) => Select(captured);
            card.DoubleClick += (_, _) => { Select(captured); Accept(); };
            card.GotFocus += (_, _) => Select(captured);   // Tab 移動焦點即選取
            card.Activated += () => { Select(captured); Accept(); };  // 卡片上按 Enter / Space
            _cards[i] = card;
            Controls.Add(card);
        }

        var bottom = new Panel { Dock = DockStyle.Bottom, Height = Ui.S(56), BackColor = PanelBg2 };
        var ok = new SimpleButton { Text = "確定　/　OK" };
        Ui.Place(ok, 376, 12, 120, 32);
        ok.Click += (_, _) => Accept();
        bottom.Controls.Add(ok);

        Controls.AddRange(new Control[] { title, hintZh, hintEn, bottom });

        int start = Array.FindIndex(Choices, c => c.Lang == preselect);
        Select(start < 0 ? 1 : start);   // 認不得就落在 English

        KeyPreview = true;
        KeyDown += (_, e) =>
        {
            switch (e.KeyCode)
            {
                case Keys.Enter: Accept(); e.SuppressKeyPress = true; break;
                case Keys.Escape: Accept(); e.SuppressKeyPress = true; break;   // 關掉＝接受目前選取
                case Keys.Left: MoveSelection(-1); e.SuppressKeyPress = true; break;
                case Keys.Right: MoveSelection(1); e.SuppressKeyPress = true; break;
                case Keys.Up: MoveSelection(-2); e.SuppressKeyPress = true; break;
                case Keys.Down: MoveSelection(2); e.SuppressKeyPress = true; break;
            }
        };
    }

    private void MoveSelection(int delta)
    {
        int next = _index + delta;
        if (next >= 0 && next < Choices.Length) { Select(next); _cards[next].Focus(); }
    }

    private void Select(int index)
    {
        if (index < 0 || index >= _cards.Length) return;
        _index = index;
        for (int i = 0; i < _cards.Length; i++) _cards[i].Selected = i == index;
    }

    private void Accept()
    {
        DialogResult = DialogResult.OK;
        Close();
    }

    /// <summary>本視窗專用字型。單位與 Theme.UIPx 相同（100% 下的像素，pt = px × 0.75），
    /// 但字型家族固定 Segoe UI——每台 Windows 都有，而且此時還不知道要用哪個語系字型。</summary>
    private static Font Px(int px, FontStyle style = FontStyle.Regular) =>
        new("Segoe UI", px * 0.75f * Ui.FontScale, style);

    /// <summary>語言選項卡：上排自稱、下排英文名稱、右側選取圓點。</summary>
    private sealed class LanguageCard : Control
    {
        private readonly string _native, _english;
        private bool _hover, _selected;

        /// <summary>卡片上按 Enter / Space（Click 事件在自繪控制項上不會由鍵盤觸發）。</summary>
        public event Action? Activated;

        public bool Selected
        {
            get => _selected;
            set { if (_selected != value) { _selected = value; Invalidate(); } }
        }

        public LanguageCard(string native, string english)
        {
            _native = native;
            _english = english;
            AccessibleName = $"{native} ({english})";
            TabStop = true;
            Cursor = Cursors.Hand;
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                     ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw |
                     ControlStyles.Selectable, true);
        }

        protected override void OnMouseEnter(EventArgs e) { _hover = true; Invalidate(); base.OnMouseEnter(e); }
        protected override void OnMouseLeave(EventArgs e) { _hover = false; Invalidate(); base.OnMouseLeave(e); }
        protected override void OnMouseDown(MouseEventArgs e) { Focus(); base.OnMouseDown(e); }
        protected override void OnGotFocus(EventArgs e) { Invalidate(); base.OnGotFocus(e); }
        protected override void OnLostFocus(EventArgs e) { Invalidate(); base.OnLostFocus(e); }

        protected override void OnKeyDown(KeyEventArgs e)
        {
            if (e.KeyCode is Keys.Enter or Keys.Space) { Activated?.Invoke(); e.Handled = true; }
            base.OnKeyDown(e);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            PaintHelpers.EnableHighQuality(g);
            var r = new RectangleF(1, 1, Width - 2, Height - 2);
            PaintHelpers.FillRounded(g, r, Ui.S(8f), _hover ? PanelBg3 : PanelBg);
            PaintHelpers.DrawRounded(g, r, Ui.S(8f),
                Selected || Focused ? Accent : _hover ? BorderLight : BorderCol,
                Selected ? Ui.S(2f) : Ui.SMin(1));

            TextRenderer.DrawText(g, _native, Px(16),
                new Rectangle(Ui.S(14), Ui.S(7), Width - Ui.S(46), Ui.S(24)), TextFg,
                TextFormatFlags.Left | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
            TextRenderer.DrawText(g, _english, Px(12),
                new Rectangle(Ui.S(14), Ui.S(30), Width - Ui.S(46), Ui.S(20)), TextDim,
                TextFormatFlags.Left | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);

            var ring = new RectangleF(Width - Ui.S(30f), Ui.S(21f), Ui.S(16f), Ui.S(16f));
            using (var fill = new SolidBrush(PanelBg3)) g.FillEllipse(fill, ring);
            using (var pen = new Pen(Selected ? Accent : BorderLight, Selected ? Ui.S(2f) : Ui.SMin(1)))
                g.DrawEllipse(pen, ring);
            if (Selected)
            {
                using var dot = new SolidBrush(Accent);
                g.FillEllipse(dot, Width - Ui.S(25.5f), Ui.S(25.5f), Ui.S(7f), Ui.S(7f));
            }
        }
    }

    /// <summary>等同 FlatButton 的主要按鈕，但不碰 Theme（見檔頭說明）。</summary>
    private sealed class SimpleButton : Control
    {
        private bool _hover, _pressed;

        public SimpleButton()
        {
            Font = Px(15);
            Cursor = Cursors.Hand;
            TabStop = true;
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                     ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
        }

        protected override void OnMouseEnter(EventArgs e) { _hover = true; Invalidate(); base.OnMouseEnter(e); }
        protected override void OnMouseLeave(EventArgs e) { _hover = false; _pressed = false; Invalidate(); base.OnMouseLeave(e); }
        protected override void OnMouseDown(MouseEventArgs e) { _pressed = true; Invalidate(); base.OnMouseDown(e); }
        protected override void OnMouseUp(MouseEventArgs e) { _pressed = false; Invalidate(); base.OnMouseUp(e); }

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            PaintHelpers.EnableHighQuality(g);
            var r = new RectangleF(0.5f, 0.5f, Width - 1, Height - 1);
            PaintHelpers.FillRounded(g, r, Ui.S(4f),
                _pressed ? AccentDim : _hover ? AccentHover : Accent);
            TextRenderer.DrawText(g, Text, Font, new Rectangle(0, 0, Width, Height), Color.White,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
        }
    }
}
