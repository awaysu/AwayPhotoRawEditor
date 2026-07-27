using System.Collections.Generic;
using System.Drawing;
using System.Windows.Forms;
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
        Size = new Size(290, 158);

        _hist = new HistogramControl();
        _hist.SetBounds(11, 4, 268, 98);
        _mean = new Label
        {
            ForeColor = Theme.TextDim, Font = Theme.Mono, TextAlign = ContentAlignment.MiddleLeft,
            Text = "R 0.0   G 0.0   B 0.0"
        };
        _mean.SetBounds(11, 104, 268, 22);

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
        Size = new Size(290, 285);
        _exif = new ExifView();
        _exif.SetBounds(11, 4, 268, 245);
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
            TextRenderer.DrawText(g, "—", Theme.Normal, new Rectangle(0, 2, Width, 20), Theme.TextFaint, TextFormatFlags.Left);
            return;
        }

        var rows = new List<(string k, string v)>
        {
            ("相機", $"{_data.CameraMake} {_data.CameraModel}".Trim()),
            ("鏡頭", _data.Lens),
            ("ISO", _data.ISO),
            ("光圈", _data.Aperture),
            ("快門", _data.Shutter),
            ("焦段", _data.FocalLength),
            ("曝光補償", _data.ExposureBias),
            ("白平衡", _data.WhiteBalance),
            ("測光", _data.MeteringMode),
            ("日期", _data.DateTaken),
            ("尺寸", _data.DimensionsDisplay),
            ("檔案大小", _data.FileSizeDisplay),
        };

        int y = 2;
        const int keyW = 64, lineH = 20;
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
