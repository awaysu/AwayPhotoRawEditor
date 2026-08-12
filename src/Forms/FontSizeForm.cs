using System;
using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;

namespace AwayPhotoRawEditor.Forms;

/// <summary>
/// 字體大小微調：列出程式用到的每一種字級，單位是「100%（96 DPI）下的像素」——
/// 刻意用整數像素而不是點數，使用者看到與輸入的都不會有小數。
/// 每列右側即時預覽該字級的樣子；最下方「恢復預設」回到原本比例。
///
/// 值寫進 <see cref="Result"/>，由呼叫端（SettingsForm）在按下確定後取用；
/// MainForm 比對 <see cref="FontSizes.Signature"/> 決定是否 <c>Application.Restart()</c>
/// ——`Theme.Small/Normal/Header/Mono` 是快取的靜態欄位，重啟才能全部重建。
/// </summary>
public sealed class FontSizeForm : Form
{
    /// <summary>編輯中的副本；按「確定」才由呼叫端取用。</summary>
    public FontSizes Result { get; }

    private sealed record Row(string Caption, string Detail, FontStyle Style, bool IsMono,
        Func<FontSizes, int> Get, Action<FontSizes, int> Set);

    // 顯示順序由小到大，與實際字級一致，方便一眼看出比例。
    private static readonly Row[] Rows =
    {
        new("小字", "縮圖檔名、EXIF 欄位名、提示文字", FontStyle.Regular, false,
            f => f.Small, (f, v) => f.Small = v),
        new("等寬數值", "直方圖下方 RGB 平均值", FontStyle.Regular, true,
            f => f.Mono, (f, v) => f.Mono = v),
        new("一般（預設）", "滑桿標籤與數值、下拉、輸入框、按鈕", FontStyle.Regular, false,
            f => f.Normal, (f, v) => f.Normal = v),
        new("區塊標題", "基本調整／色彩／工具 等區塊標題", FontStyle.Bold, false,
            f => f.SectionTitle, (f, v) => f.SectionTitle = v),
        new("關於內文", "關於視窗的內文", FontStyle.Regular, false,
            f => f.AboutBody, (f, v) => f.AboutBody = v),
        new("資料夾圖示", "選擇資料夾清單的圖示", FontStyle.Regular, false,
            f => f.FolderGlyph, (f, v) => f.FolderGlyph = v),
        new("小圖示按鈕", "白平衡滴管等圖示鈕", FontStyle.Regular, false,
            f => f.IconGlyph, (f, v) => f.IconGlyph = v),
        new("進度視窗標題", "產生快取／轉存進度視窗", FontStyle.Bold, false,
            f => f.ProgressTitle, (f, v) => f.ProgressTitle = v),
        new("對話框標題", "匯出照片／設定 視窗標題", FontStyle.Bold, false,
            f => f.DialogTitle, (f, v) => f.DialogTitle = v),
        new("關於標題", "關於視窗的標題", FontStyle.Bold, false,
            f => f.AboutTitle, (f, v) => f.AboutTitle = v),
        new("左上程式名稱", "頂端「AwayPhotoRawEditor」", FontStyle.Bold, false,
            f => f.Logo, (f, v) => f.Logo = v),
        new("選單圖示", "左上角的 ☰ 選單鈕", FontStyle.Regular, false,
            f => f.MenuGlyph, (f, v) => f.MenuGlyph = v),
    };

    /// <summary>一列的執行期狀態。<see cref="Owned"/> 是本視窗自己建立的預覽字型，
    /// 只有它可以被 Dispose——直接 Dispose <c>Label.Font</c> 會毀掉繼承來的
    /// <c>Theme.Normal</c>（那是全程式共用的快取字型）。</summary>
    private sealed class Editor
    {
        public required Row Row;
        public required NumericUpDown Num;
        public required Label Preview;
        public Font? Owned;
    }

    private readonly List<Editor> _editors = new();
    private bool _suppress;   // ApplyValues 期間避免 ValueChanged 重入

    // 96 DPI 設計座標
    private const int DlgW = 660, RowH = 38, FirstRowY = 96;
    private const int LabelX = 20, LabelW = 130, DetailX = 156, DetailW = 246,
                      NumX = 410, NumW = 64, PreviewX = 486, PreviewW = 154;

    public FontSizeForm(FontSizes current)
    {
        Result = current.Clone();

        Text = "字體大小";
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false; MinimizeBox = false;
        StartPosition = FormStartPosition.CenterParent;

        int footerY = FirstRowY + Rows.Length * RowH + 16;
        ClientSize = Ui.FitWorkArea(DlgW, footerY + 52);
        AutoScroll = true;   // 螢幕不夠高時仍可捲到所有列與按鈕

        var header = new Label
        {
            Text = "字體大小", Font = Theme.UIPx(Theme.Sizes.DialogTitle, FontStyle.Bold),
            ForeColor = Theme.Text
        };
        Ui.Place(header, LabelX, 16, 400, 28);

        var hint = new Label
        {
            Text = "數值為 100% 顯示比例下的像素；介面大小會再等比縮放。調太大時部分固定寬度的標籤可能被截字",
            ForeColor = Theme.TextFaint, Font = Theme.Small
        };
        Ui.Place(hint, LabelX, 50, DlgW - 2 * LabelX, 20);

        var colNum = new Label
        {
            Text = "像素", ForeColor = Theme.Accent, Font = Theme.UIPx(Theme.Sizes.Small, FontStyle.Bold),
            TextAlign = ContentAlignment.MiddleCenter
        };
        Ui.Place(colNum, NumX, 74, NumW, 18);
        var colPreview = new Label
        {
            Text = "預覽", ForeColor = Theme.Accent, Font = Theme.UIPx(Theme.Sizes.Small, FontStyle.Bold)
        };
        Ui.Place(colPreview, PreviewX, 74, PreviewW, 18);

        Controls.AddRange(new Control[] { header, hint, colNum, colPreview });

        for (int i = 0; i < Rows.Length; i++) AddRow(Rows[i], FirstRowY + i * RowH);

        var reset = new FlatButton { Text = "恢復預設" };
        Ui.Place(reset, LabelX, footerY, 110, 32);
        reset.Click += (_, _) => ApplyValues(new FontSizes());

        var ok = new FlatButton { Text = "確定", Primary = true };
        Ui.Place(ok, DlgW - 20 - 92 - 8 - 88, footerY, 88, 32);
        ok.Click += (_, _) => { DialogResult = DialogResult.OK; Close(); };

        var cancel = new FlatButton { Text = "取消" };
        Ui.Place(cancel, DlgW - 20 - 92, footerY, 92, 32);
        cancel.Click += (_, _) => { DialogResult = DialogResult.Cancel; Close(); };

        Controls.AddRange(new Control[] { reset, ok, cancel });
        L.Apply(this);
        RefreshPreviews();   // L.Apply 會覆寫文字，預覽在它之後才設定
    }

    private void AddRow(Row row, int y)
    {
        var caption = new Label { Text = row.Caption, ForeColor = Theme.Text, TextAlign = ContentAlignment.MiddleLeft };
        Ui.Place(caption, LabelX, y, LabelW, 26);

        var detail = new Label
        {
            Text = row.Detail, ForeColor = Theme.TextFaint, Font = Theme.Small,
            TextAlign = ContentAlignment.MiddleLeft, AutoEllipsis = true
        };
        Ui.Place(detail, DetailX, y, DetailW, 26);

        var num = UiFactory.Numeric(FontSizes.MinPx, FontSizes.MaxPx, row.Get(Result));
        Ui.Place(num, NumX, y, NumW, 26);

        var preview = new Label
        {
            ForeColor = Theme.Text, TextAlign = ContentAlignment.MiddleLeft,
            BackColor = Theme.WindowBg, AutoEllipsis = true
        };
        Ui.Place(preview, PreviewX, y - 6, PreviewW, 38);

        var ed = new Editor { Row = row, Num = num, Preview = preview };
        num.ValueChanged += (_, _) =>
        {
            if (_suppress) return;
            row.Set(Result, (int)num.Value);
            UpdatePreview(ed);
        };

        Controls.AddRange(new Control[] { caption, detail, num, preview });
        _editors.Add(ed);
    }

    /// <summary>預覽用中英混排的短字串，字級差異一眼可見。</summary>
    private void UpdatePreview(Editor ed)
    {
        int px = ed.Row.Get(Result);
        var font = ed.Row.IsMono
            ? new Font("Consolas", px * 0.75f * Ui.FontScale)
            : Theme.UIPx(px, ed.Row.Style);
        ed.Preview.Font = font;
        ed.Owned?.Dispose();      // 只釋放上一個「自己建立的」字型
        ed.Owned = font;
        // 不重複顯示數字（左邊的輸入框已經有了）——27px 時會把預覽欄撐爆
        ed.Preview.Text = "樣本 Ag";
    }

    private void RefreshPreviews()
    {
        foreach (var ed in _editors) UpdatePreview(ed);
    }

    private void ApplyValues(FontSizes values)
    {
        _suppress = true;
        try
        {
            foreach (var ed in _editors)
            {
                int v = Math.Clamp(ed.Row.Get(values), FontSizes.MinPx, FontSizes.MaxPx);
                ed.Num.Value = v;
                ed.Row.Set(Result, v);
            }
        }
        finally { _suppress = false; }
        RefreshPreviews();
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
            foreach (var ed in _editors) { ed.Preview.Font = null; ed.Owned?.Dispose(); ed.Owned = null; }
        base.Dispose(disposing);
    }
}
