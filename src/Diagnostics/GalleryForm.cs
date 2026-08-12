using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;
using AwayPhotoRawEditor.Imaging;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Diagnostics;

/// <summary>
/// Visual gallery of the Phase 3 custom controls (invoked with --gallery &lt;png&gt;
/// &lt;sampleImage&gt;). Renders every control with sample data and saves a screenshot.
/// </summary>
public sealed class GalleryForm : Form
{
    public GalleryForm(string? sampleImage)
    {
        Text = "AwayPhotoRawEditor — Controls Gallery";
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        ClientSize = Ui.Sz(1180, 760);
        StartPosition = FormStartPosition.CenterScreen;

        // ---- Left column: sections with sliders + buttons ----
        var left = new Panel { BackColor = Theme.PanelBg, Dock = DockStyle.Left, Width = Ui.S(320), AutoScroll = true };
        Controls.Add(left);

        var presetPanel = new Panel { Dock = DockStyle.Top, Height = Ui.S(96), BackColor = Theme.PanelBg };
        var combo = new ComboBox { Dock = DockStyle.Top, DropDownStyle = ComboBoxStyle.DropDownList, FlatStyle = FlatStyle.Flat, BackColor = Theme.PanelBg3, ForeColor = Theme.Text };
        combo.Items.AddRange(new object[] { L.T("預設時設定"), L.T("風景"), L.T("人像"), L.T("鮮豔"), L.T("黑白") });
        combo.SelectedIndex = 1;
        presetPanel.Controls.Add(new FlatButton { Text = "儲存風格檔", Dock = DockStyle.Bottom, Height = Ui.S(28) });
        presetPanel.Controls.Add(combo);

        var color = new SectionPanel("色彩");
        color.AddContent(MakeSlider("色溫", 2000, 12000, 5200, 5600, "0", false));
        color.AddContent(MakeSlider("色調", -100, 100, 0, 4, "0", true));
        color.AddContent(MakeSlider("鮮豔度", -100, 100, 0, 28, "0", true));
        color.AddContent(MakeSlider("飽和度", -100, 100, 0, 8, "0", true));

        var basic = new SectionPanel("基本調整");
        basic.AddContent(MakeSlider("曝光", -5, 5, 0, 0.35, "0.00", true));
        basic.AddContent(MakeSlider("對比", -100, 100, 0, 20, "0", true));
        basic.AddContent(MakeSlider("亮部", -100, 100, 0, -30, "0", true));
        basic.AddContent(MakeSlider("暗部", -100, 100, 0, 25, "0", true));
        basic.AddContent(MakeSlider("白色", -100, 100, 0, 8, "0", true));
        basic.AddContent(MakeSlider("黑色", -100, 100, 0, -10, "0", true));

        left.Controls.Add(presetPanel);
        left.Controls.Add(color);
        left.Controls.Add(basic);

        // ---- Right column: histogram, tabs, ratings, buttons ----
        var right = new Panel { BackColor = Theme.PanelBg, Dock = DockStyle.Right, Width = Ui.S(300), Padding = Ui.Pad(12) };
        Controls.Add(right);

        var hist = new HistogramControl { Dock = DockStyle.Top, Histogram = SampleHistogram() };
        var tab = new TopTab { Dock = DockStyle.Top, Tabs = new[] { "裁切", "漸層", "修護", "標誌" }, SelectedIndex = 0, Margin = Ui.Pad(0, 8, 0, 0) };
        var dropper = new IconButton { Glyph = "🖉", Checkable = true, Dock = DockStyle.Top, Height = Ui.S(32) };
        var primaryBtn = new FlatButton { Text = "匯出全部照片", Primary = true, Dock = DockStyle.Top, Height = Ui.S(32) };
        var normalBtn = new FlatButton { Text = "恢復上一步", Dock = DockStyle.Top, Height = Ui.S(30) };
        foreach (var c in new Control[] { normalBtn, primaryBtn, dropper, tab, hist })
        {
            c.Margin = Ui.Pad(0, 6, 0, 6);
            right.Controls.Add(c);
            c.BringToFront();
        }

        // ---- Bottom: thumbnail strip ----
        var strip = new ThumbnailStrip { Dock = DockStyle.Bottom, Height = Ui.S(108) };
        Controls.Add(strip);
        var items = new List<PhotoItem>();
        for (int i = 0; i < 8; i++)
        {
            var it = new PhotoItem($"C:\\photos\\DSC{1000 + i}.RAF", i == 5 ? 1 : 0)
            {
                IsEdited = i is 1 or 2 or 5,
                IsCopySettingsSource = i == 2
            };
            items.Add(it);
        }
        strip.SetItems(items, 1);
        for (int i = 0; i < items.Count; i++)
            strip.SetImage(items[i].Key, SampleThumb(i));

        // ---- Center: image viewer with crop overlay ----
        var viewer = new ImageViewer { Dock = DockStyle.Fill, Tool = ToolMode.Crop };
        Controls.Add(viewer);
        viewer.BringToFront();
        var adj = new ImageAdjustments { CropX = 0.12, CropY = 0.1, CropWidth = 0.7, CropHeight = 0.72 };
        viewer.Adjustments = adj;
        Bitmap? sample = null;
        if (sampleImage is not null && System.IO.File.Exists(sampleImage))
            sample = WicDecoder.LoadFile(sampleImage) is { } b ? CacheManager.ResizeToMaxDim(b, 1400) : null;
        sample ??= SamplePhoto();
        viewer.SetImage(sample, true);
        L.Apply(this);
    }

    private static AdjustmentSlider MakeSlider(string label, double min, double max, double def, double val, string fmt, bool bipolar)
        => new()
        {
            Label = label, Min = min, Max = max, DefaultValue = def, Format = fmt, Bipolar = bipolar,
            Value = val, Height = Ui.S(40)
        };

    private static Histogram SampleHistogram()
    {
        var h = new Histogram();
        for (int i = 0; i < 256; i++)
        {
            double t = i / 255.0;
            h.R[i] = (int)(8000 * Math.Exp(-Math.Pow((t - 0.45) * 3, 2)) + 500);
            h.G[i] = (int)(7000 * Math.Exp(-Math.Pow((t - 0.5) * 3, 2)) + 400);
            h.B[i] = (int)(6000 * Math.Exp(-Math.Pow((t - 0.55) * 3, 2)) + 300);
        }
        h.Max = 8500;
        return h;
    }

    private static Bitmap SampleThumb(int i)
    {
        var bmp = new Bitmap(120, 80);
        using var g = Graphics.FromImage(bmp);
        var c = Color.FromArgb(60 + i * 18 % 180, 90, 140 - i * 10 % 100);
        g.Clear(c);
        g.FillEllipse(Brushes.White, 40, 24, 40, 32);
        return bmp;
    }

    private static Bitmap SamplePhoto()
    {
        var bmp = new Bitmap(1200, 800);
        using var g = Graphics.FromImage(bmp);
        using var lg = new System.Drawing.Drawing2D.LinearGradientBrush(
            new Rectangle(0, 0, 1200, 800), Color.FromArgb(120, 150, 90), Color.FromArgb(60, 80, 110), 60f);
        g.FillRectangle(lg, 0, 0, 1200, 800);
        return bmp;
    }
}
