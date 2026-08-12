using System;
using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;
using AwayPhotoRawEditor.Export;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Forms;

/// <summary>匯出設定視窗。左欄：儲存位置 / 次資料夾 / 重新命名 / 存檔遇到相同檔名；
/// 右欄：格式與尺寸 / 標誌（浮水印，全域覆蓋，勾選後即時顯示於主影像預覽）。</summary>
public sealed class ExportForm : Form
{
    private readonly ExportSettings _s;
    private readonly DarkRadioButton _rDesktop, _rSame, _rCustom;
    private readonly DarkRadioButton _rAppend, _rOverwrite;
    private readonly TextBox _customPath, _subFolder, _maxEdge;
    private readonly DarkCheckBox _useSub, _openExplorer, _preserveExif;
    private readonly ComboBox _format, _rename, _resolution;
    private readonly AdjustmentSlider _quality;
    // 標誌 (watermark)
    private readonly DarkCheckBox _wmEnable;
    private readonly TextBox _wmText;
    private readonly ComboBox _wmFont, _wmPos, _wmColor;
    private readonly NumericUpDown _wmSize, _wmAlpha, _wmMargin;

    public bool StartRequested { get; private set; }

    /// <summary>Raised whenever a watermark control changes (after writing it into the settings),
    /// so the caller can re-render the live preview.</summary>
    public event Action? WatermarkChanged;

    public ExportForm(ExportSettings settings, int photoCount)
    {
        _s = settings;
        Text = "匯出設定";
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false; MinimizeBox = false;
        StartPosition = FormStartPosition.CenterParent;
        ClientSize = Ui.FitWorkArea(1048, 572);
        AutoScroll = true;   // 小螢幕 / 高縮放時仍可捲到所有欄位

        Controls.Add(BuildHeader(photoCount, 1048));

        // 以下座標一律是 96 DPI 設計值；Ui.Place / NewCard / WmLabel 負責縮放。
        const int dlgW = 1048;
        const int mx = 16, cardW = 500, colGap = 16;
        const int rightX = mx + cardW + colGap;   // 532 — right column x

        // ================= 左欄：儲存位置 / 次資料夾 / 重新命名 / 存檔遇到相同檔名 =================

        // ---- Card 1: 儲存位置 -------------------------------------------
        var card1 = NewCard("儲存位置", mx, 72, cardW, 152);
        _rDesktop = AddRadio(card1, "桌面", 38);
        _rSame = AddRadio(card1, "同原始照片目錄", 66);
        _rCustom = AddRadio(card1, "自己選擇", 94);
        _customPath = UiFactory.Text(_s.CustomPath); Ui.Place(_customPath, 36, 120, 356, 24);
        var browse = new FlatButton { Text = "瀏覽" }; Ui.Place(browse, 400, 119, 84, 26);
        browse.Click += (_, _) =>
        {
            using var d = new FolderPickerForm(_customPath.Text);
            if (d.ShowDialog(this) == DialogResult.OK) { _customPath.Text = d.SelectedPath; _rCustom.Checked = true; }
        };
        card1.Controls.Add(_customPath); card1.Controls.Add(browse);
        Controls.Add(card1);

        // ---- Card 2: 次資料夾 -------------------------------------------
        var card2 = NewCard("次資料夾", mx, 236, cardW, 74);
        _useSub = new DarkCheckBox { Text = "儲存至次資料夾", Checked = _s.UseSubFolder }; Ui.Place(_useSub, 16, 38, 156, 24);
        _subFolder = UiFactory.Text(_s.SubFolder); Ui.Place(_subFolder, 180, 39, 304, 24);
        card2.Controls.Add(_useSub); card2.Controls.Add(_subFolder);
        Controls.Add(card2);

        // ---- Card 3: 重新命名 -------------------------------------------
        var card3 = NewCard("重新命名", mx, 322, cardW, 78);
        _rename = UiFactory.Combo(
            "按照原始檔案",
            "日期時間（IMG 年月日時分秒＋序號）",
            "數字開始（IMG00001）");
        Ui.Place(_rename, 16, 40, 468, 24);
        _rename.SelectedIndex = (int)_s.Rename;
        card3.Controls.Add(_rename);
        Controls.Add(card3);

        // ---- Card 4: 存檔遇到相同檔名 -----------------------------------
        var card4 = NewCard("存檔遇到相同檔名", mx, 412, cardW, 100);
        _rAppend = AddRadio(card4, "檔名接續 \"_數字\"，例如 _1, _2...", 38);
        _rAppend.Width = Ui.S(468);
        _rOverwrite = AddRadio(card4, "直接覆蓋", 66);
        _rAppend.Checked = _s.Conflict == ConflictMode.AppendNumber;
        _rOverwrite.Checked = _s.Conflict == ConflictMode.Overwrite;
        Controls.Add(card4);

        // ================= 右欄：格式與尺寸 / 標誌 =================

        // ---- Card 5: 格式與尺寸 -----------------------------------------
        var card5 = NewCard("格式與尺寸", rightX, 72, cardW, 218);
        _format = UiFactory.Combo("JPEG", "BMP", "TIFF", "PNG"); Ui.Place(_format, 16, 40, 120, 24);
        _format.SelectedIndex = (int)_s.Format;
        var el = WmLabel("符合寬度高度(像素)", 142, 42, 214);
        _maxEdge = UiFactory.Text(_s.MaxLongEdge.ToString()); Ui.Place(_maxEdge, 360, 40, 72, 24);
        card5.Controls.Add(_format); card5.Controls.Add(el); card5.Controls.Add(_maxEdge);
        var rl = WmLabel("解析度（像素/英寸）", 16, 74, 156);
        _resolution = UiFactory.Combo("100", "200", "300", "400", "500", "600"); Ui.Place(_resolution, 176, 72, 80, 24);
        _resolution.SelectedIndex = Math.Clamp(_s.Resolution / 100 - 1, 0, 5);
        card5.Controls.Add(rl); card5.Controls.Add(_resolution);
        _quality = new AdjustmentSlider { Label = "JPEG 品質", Min = 50, Max = 100, DefaultValue = 100, Format = "0" };
        Ui.Place(_quality, 16, 108, 468, 40);
        _quality.SetValueSilent(_s.JpegQuality);
        card5.Controls.Add(_quality);
        _preserveExif = new DarkCheckBox { Text = "保存 EXIF（相機 / 鏡頭 / 拍攝資訊）", Checked = _s.PreserveExif }; Ui.Place(_preserveExif, 16, 156, 468, 24);
        card5.Controls.Add(_preserveExif);
        _openExplorer = new DarkCheckBox { Text = "轉檔完成後開啟檔案總管顯示", Checked = _s.OpenExplorerAfter }; Ui.Place(_openExplorer, 16, 186, 468, 24);
        card5.Controls.Add(_openExplorer);
        Controls.Add(card5);

        // ---- Card 6: 浮水印 --------------------------------------------
        var card6 = NewCard("浮水印", rightX, 302, cardW, 152);
        // Row 1: 啟用浮水印 + 文字
        _wmEnable = new DarkCheckBox { Text = "啟用浮水印", Checked = _s.WatermarkEnabled }; Ui.Place(_wmEnable, 16, 40, 170, 24);
        card6.Controls.Add(_wmEnable);
        card6.Controls.Add(WmLabel("文字", 186, 42, 42));
        _wmText = UiFactory.Text(_s.WatermarkText); Ui.Place(_wmText, 228, 40, 256, 24);
        card6.Controls.Add(_wmText);

        // Row 2: 字體 + 大小 + 顏色
        card6.Controls.Add(WmLabel("字體", 16, 76, 60));
        _wmFont = UiFactory.Combo(FontNames()); Ui.Place(_wmFont, 80, 74, 104, 24);
        SelectFont(_s.WatermarkFontName);
        card6.Controls.Add(_wmFont);
        card6.Controls.Add(WmLabel("大小", 190, 76, 50));
        _wmSize = UiFactory.Numeric(6, 300, (decimal)Math.Clamp(_s.WatermarkFontSize, 6, 300)); Ui.Place(_wmSize, 244, 74, 58, 24);
        card6.Controls.Add(_wmSize);
        card6.Controls.Add(WmLabel("顏色", 306, 76, 46));
        _wmColor = UiFactory.Combo(
            L.Pick("白色", "White", "白", "흰색", "白色", "Weiß", "Blanc", "Blanco"),
            L.Pick("黑色", "Black", "黒", "검은색", "黑色", "Schwarz", "Noir", "Negro"),
            L.Pick("藍色", "Blue", "青", "파란색", "蓝色", "Blau", "Bleu", "Azul"),
            L.Pick("黃色", "Yellow", "黄", "노란색", "黄色", "Gelb", "Jaune", "Amarillo"),
            L.Pick("綠色", "Green", "緑", "초록색", "绿色", "Grün", "Vert", "Verde"),
            L.Pick("紅色", "Red", "赤", "빨간색", "红色", "Rot", "Rouge", "Rojo"),
            L.Pick("灰色", "Gray", "グレー", "회색", "灰色", "Grau", "Gris", "Gris"),
            L.Pick("橙色", "Orange", "オレンジ", "주황색", "橙色", "Orange", "Orange", "Naranja"));
        Ui.Place(_wmColor, 352, 74, 132, 24);
        _wmColor.SelectedIndex = (int)_s.WatermarkColor;
        card6.Controls.Add(_wmColor);

        // Row 3: 透明度 + 位置 + 邊緣
        card6.Controls.Add(WmLabel("透明度", 16, 110, 64));
        _wmAlpha = UiFactory.Numeric(0, 100, Math.Clamp(_s.WatermarkTransparency, 0, 100)); Ui.Place(_wmAlpha, 84, 108, 54, 24);
        card6.Controls.Add(_wmAlpha);
        card6.Controls.Add(WmLabel("位置", 142, 110, 66));
        _wmPos = UiFactory.Combo("左上", "右上", "左下", "右下"); Ui.Place(_wmPos, 210, 108, 125, 24);
        _wmPos.SelectedIndex = (int)_s.WatermarkPosition;
        card6.Controls.Add(_wmPos);
        card6.Controls.Add(WmLabel("邊緣", 340, 110, 58));
        _wmMargin = UiFactory.Numeric(0, 9999, Math.Clamp(_s.WatermarkMargin, 0, 9999)); Ui.Place(_wmMargin, 402, 108, 62, 24);
        card6.Controls.Add(_wmMargin);
        Controls.Add(card6);

        // Watermark controls edit the settings live and refresh the main preview.
        _wmEnable.CheckedChanged += (_, _) => WmChanged();
        _wmText.TextChanged += (_, _) => WmChanged();
        _wmFont.SelectedIndexChanged += (_, _) => WmChanged();
        _wmColor.SelectedIndexChanged += (_, _) => WmChanged();
        _wmPos.SelectedIndexChanged += (_, _) => WmChanged();
        _wmSize.ValueChanged += (_, _) => WmChanged();
        _wmAlpha.ValueChanged += (_, _) => WmChanged();
        _wmMargin.ValueChanged += (_, _) => WmChanged();

        // ---- Footer buttons ---------------------------------------------
        const int footerY = 524, cancelX = dlgW - 16 - 92, startX = cancelX - 8 - 168;
        var save = new FlatButton { Text = "儲存設定" }; Ui.Place(save, mx, footerY, 100, 32);
        var cancel = new FlatButton { Text = "取消" }; Ui.Place(cancel, cancelX, footerY, 92, 32);
        // 168（原 150）：德/法/西文的「儲存設定並開始轉存」較長，放寬避免截字
        var start = new FlatButton { Text = "儲存設定並開始轉存", Primary = true }; Ui.Place(start, startX, footerY, 168, 32);
        save.Click += (_, _) => { Commit(); DialogResult = DialogResult.OK; Close(); };
        start.Click += (_, _) => { Commit(); StartRequested = true; DialogResult = DialogResult.OK; Close(); };
        cancel.Click += (_, _) => { DialogResult = DialogResult.Cancel; Close(); };
        Controls.AddRange(new Control[] { save, start, cancel });

        _rDesktop.Checked = _s.Location == ExportLocation.Desktop;
        _rSame.Checked = _s.Location == ExportLocation.SameAsSource;
        _rCustom.Checked = _s.Location == ExportLocation.Custom;
        L.Apply(this);
    }

    // ---- layout helpers -----------------------------------------------------

    // 以下 helper 的 x/y/w/h 都是 96 DPI 設計座標，一律在這裡乘上 Ui.Scale。

    private static Panel BuildHeader(int photoCount, int width)
    {
        var header = new Panel { BackColor = Theme.PanelBg2 };
        Ui.Place(header, 0, 0, width, 60);
        header.Paint += (_, e) =>
        {
            var g = e.Graphics;
            PaintHelpers.EnableHighQuality(g);
            using (var accent = new SolidBrush(Theme.Accent))
                g.FillRectangle(accent, 0, 0, Ui.S(4), header.Height);            // accent bar
            using (var line = new Pen(Color.FromArgb(60, Theme.Accent), Ui.SMin(1)))
                g.DrawLine(line, 0, header.Height - Ui.SMin(1), header.Width, header.Height - Ui.SMin(1));
            TextRenderer.DrawText(g, L.T("匯出照片"), Theme.UIPx(Theme.Sizes.DialogTitle, FontStyle.Bold),
                Ui.Rect(20, 8, 440, 26), Theme.Text, TextFormatFlags.Left);
            TextRenderer.DrawText(g, L.F("共 {0} 張相片將被轉存", photoCount), Theme.Normal,
                Ui.Rect(20, 34, 440, 20), Theme.TextDim, TextFormatFlags.Left);
        };
        return header;
    }

    private static DarkGroupBox NewCard(string title, int x, int y, int w, int h)
    {
        var c = new DarkGroupBox { Text = title };
        Ui.Place(c, x, y, w, h);
        return c;
    }

    private static Label WmLabel(string text, int x, int y, int w)
    {
        var l = new Label { Text = text, ForeColor = Theme.TextDim, BackColor = Theme.PanelBg, TextAlign = ContentAlignment.MiddleLeft };
        Ui.Place(l, x, y, w, 22);
        return l;
    }

    private static DarkRadioButton AddRadio(Control parent, string text, int y)
    {
        var r = new DarkRadioButton { Text = text };
        Ui.Place(r, 16, y, 300, 24);
        parent.Controls.Add(r);
        return r;
    }

    private static string[] FontNames()
    {
        var list = new List<string>();
        foreach (var f in FontFamily.Families)
            if (!string.IsNullOrWhiteSpace(f.Name)) list.Add(f.Name);
        return list.ToArray();
    }

    private void SelectFont(string name)
    {
        int i = _wmFont.Items.IndexOf(name);
        if (i < 0) i = _wmFont.Items.IndexOf("Arial");
        _wmFont.SelectedIndex = Math.Max(0, i);
    }

    private void WmChanged() { CommitWatermark(); WatermarkChanged?.Invoke(); }

    private void CommitWatermark()
    {
        _s.WatermarkEnabled = _wmEnable.Checked;
        _s.WatermarkText = _wmText.Text;
        if (_wmFont.SelectedItem is string f) _s.WatermarkFontName = f;
        _s.WatermarkFontSize = (float)_wmSize.Value;
        _s.WatermarkTransparency = (int)_wmAlpha.Value;
        _s.WatermarkColor = (WatermarkColor)Math.Max(0, _wmColor.SelectedIndex);
        _s.WatermarkPosition = (WatermarkPosition)Math.Max(0, _wmPos.SelectedIndex);
        _s.WatermarkMargin = (int)_wmMargin.Value;
    }

    private void Commit()
    {
        _s.Location = _rCustom.Checked ? ExportLocation.Custom : _rSame.Checked ? ExportLocation.SameAsSource : ExportLocation.Desktop;
        _s.CustomPath = _customPath.Text.Trim();
        _s.UseSubFolder = _useSub.Checked;
        _s.SubFolder = string.IsNullOrWhiteSpace(_subFolder.Text) ? "NEW_IMAGE" : _subFolder.Text.Trim();
        _s.Rename = (RenameMode)Math.Max(0, _rename.SelectedIndex);
        _s.Conflict = _rOverwrite.Checked ? ConflictMode.Overwrite : ConflictMode.AppendNumber;
        _s.Format = (ExportFormat)Math.Max(0, _format.SelectedIndex);
        if (int.TryParse(_maxEdge.Text, out var edge) && edge > 0) _s.MaxLongEdge = edge;
        if (int.TryParse(_resolution.Text, out var dpi) && dpi > 0) _s.Resolution = dpi;
        _s.JpegQuality = (int)Math.Clamp(_quality.Value, 50, 100);
        _s.PreserveExif = _preserveExif.Checked;
        _s.OpenExplorerAfter = _openExplorer.Checked;
        CommitWatermark();
        _s.Save();
    }
}
