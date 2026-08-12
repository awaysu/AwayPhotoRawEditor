using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;
using AwayPhotoRawEditor.Imaging;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Panels;

/// <summary>直方圖 (290x158): RGB histogram + R/G/B 平均值.</summary>
public sealed class HistogramPanel : SectionPanel
{
    private readonly HistogramControl _hist;
    private readonly Label _mean;

    public HistogramPanel() : base("直方圖")
    {
        ManualLayout = true;
        Size = Ui.Sz(290, 158);

        _hist = new HistogramControl();
        Ui.Place(_hist, 11, 4, 268, 98);
        _mean = new Label
        {
            ForeColor = Theme.TextDim, Font = Theme.Mono, TextAlign = ContentAlignment.MiddleLeft,
            Text = "R 0.0   G 0.0   B 0.0"
        };
        Ui.Place(_mean, 11, 104, 268, 22);

        ContentArea.Controls.Add(_hist);
        ContentArea.Controls.Add(_mean);
    }

    public void SetHistogram(Histogram? h) => _hist.Histogram = h;
    public void SetMean(double r, double g, double b) => _mean.Text = $"R {r,5:0.0}   G {g,5:0.0}   B {b,5:0.0}";
}

/// <summary>照片資訊 (290x285): EXIF + 檔案資訊.</summary>
public sealed class PhotoInfoPanel : SectionPanel
{
    private readonly ExifView _exif;

    public PhotoInfoPanel() : base("照片資訊")
    {
        ManualLayout = true;
        Size = Ui.Sz(290, 285);
        _exif = new ExifView();
        Ui.Place(_exif, 11, 4, 268, 245);
        ContentArea.Controls.Add(_exif);
    }

    public void SetExif(ExifData? exif) => _exif.Data = exif;
}

/// <summary>Custom-painted EXIF key/value list.</summary>
public sealed class ExifView : Control
{
    private ExifData? _data;

    public ExifData? Data { get => _data; set { _data = value; Invalidate(); } }

    public ExifView()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
        BackColor = Theme.PanelBg;
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        g.Clear(Theme.PanelBg);
        if (_data is null)
        {
            TextRenderer.DrawText(g, "—", Theme.Normal, new Rectangle(0, Ui.S(2), Width, Ui.S(20)), Theme.TextFaint, TextFormatFlags.Left);
            return;
        }

        var rows = new List<(string k, string v)>
        {
            (L.T("相機"), $"{_data.CameraMake} {_data.CameraModel}".Trim()),
            (L.T("鏡頭"), _data.Lens),
            ("ISO", _data.ISO),
            (L.T("光圈"), _data.Aperture),
            (L.T("快門"), _data.Shutter),
            (L.T("焦段"), _data.FocalLength),
            (L.T("曝光補償"), _data.ExposureBias),
            (L.T("白平衡"), _data.WhiteBalance),
            (L.T("測光"), _data.MeteringMode),
            (L.T("日期"), _data.DateTaken),
            (L.T("尺寸"), _data.DimensionsDisplay),
            (L.T("檔案大小"), _data.FileSizeDisplay),
        };

        int y = Ui.S(2);
        // 欄位名欄寬與列高都用實測值：字級可由使用者調整（設定 →「字體大小」），
        // 寫死 64/94 與 20 在大字級下會讓「曝光補償」貼上右邊的數值、或讓上下列相黏。
        int keyW = Ui.S(L.CurrentLanguage is AppLanguage.TraditionalChinese or AppLanguage.SimplifiedChinese ? 64 : 94);
        foreach (var (k, _) in rows)
            keyW = Math.Max(keyW, TextRenderer.MeasureText(g, k, Theme.Small).Width + Ui.S(8));
        keyW = Math.Min(keyW, Width * 45 / 100);   // 別把數值欄擠掉
        // 列高刻意維持 20：12 列 × 20 = 240 剛好放進 ExifView 的 245 高。
        // 若改成隨字級長高（15px → 23），12 列變 276 就會把最後幾列裁掉——
        // 而放大 ExifView 又會讓右欄超過螢幕。字要塞進固定的框，不是反過來。
        int lineH = Ui.S(20);
        foreach (var (k, v) in rows)
        {
            if (string.IsNullOrWhiteSpace(v)) continue;
            TextRenderer.DrawText(g, k, Theme.Small, new Rectangle(0, y, keyW, lineH), Theme.TextFaint,
                TextFormatFlags.Left | TextFormatFlags.VerticalCenter);
            TextRenderer.DrawText(g, v, Theme.Normal, new Rectangle(keyW, y, Width - keyW, lineH), Theme.Text,
                TextFormatFlags.Left | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
            y += lineH;
        }
    }
}
