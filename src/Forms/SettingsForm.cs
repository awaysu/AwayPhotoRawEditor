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

    public SettingsForm()
    {
        Text = "設定";
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false; MinimizeBox = false;
        StartPosition = FormStartPosition.CenterParent;
        ClientSize = new Size(420, 282);

        var header = new Label { Text = "設定", Font = Theme.Header, ForeColor = Theme.Text, Left = 16, Top = 14, Width = 380, Height = 24 };

        _useLibRaw = new DarkCheckBox { Text = "使用 LibRaw", BackColor = Theme.WindowBg, Checked = AppSettings.Current.UseLibRaw };
        _useLibRaw.SetBounds(20, 50, 360, 24);
        _highPrecision = new DarkCheckBox { Text = "高精度 RAW 處理流程 (16-bit / float)", BackColor = Theme.WindowBg, Checked = AppSettings.Current.UseHighPrecisionRawPipeline };
        _highPrecision.SetBounds(20, 80, 360, 24);
        _showNumber = new DarkCheckBox { Text = "在縮圖左上顯示編號 (#1, #2 …)", BackColor = Theme.WindowBg, Checked = AppSettings.Current.ShowThumbnailNumber };
        _showNumber.SetBounds(20, 110, 360, 24);
        _showScrollBars = new DarkCheckBox { Text = "顯示捲軸（視窗過矮時左右欄可捲動）", BackColor = Theme.WindowBg, Checked = AppSettings.Current.ShowColumnScrollBars };
        _showScrollBars.SetBounds(20, 140, 360, 24);

        string libState = LibRawInterop.Available ? "libraw.dll 已載入" : "libraw.dll 未找到（將退回 WIC / 嵌入預覽）";
        var note = new Label { Text = libState, ForeColor = Theme.TextFaint, Font = Theme.Small, Left = 20, Top = 178, Width = 380, Height = 20 };
        string exifState = Exif.ExifReader.ExifToolAvailable ? "exiftool.exe 已載入" : "exiftool.exe 未找到（將退回 WIC metadata）";
        var note2 = new Label { Text = exifState, ForeColor = Theme.TextFaint, Font = Theme.Small, Left = 20, Top = 198, Width = 380, Height = 20 };

        var ok = new FlatButton { Text = "確定", Primary = true, Left = 236, Top = 234, Width = 80, Height = 32 };
        var cancel = new FlatButton { Text = "取消", Left = 324, Top = 234, Width = 80, Height = 32 };
        ok.Click += (_, _) =>
        {
            AppSettings.Current.UseLibRaw = _useLibRaw.Checked;
            AppSettings.Current.UseHighPrecisionRawPipeline = _highPrecision.Checked;
            AppSettings.Current.ShowThumbnailNumber = _showNumber.Checked;
            AppSettings.Current.ShowColumnScrollBars = _showScrollBars.Checked;
            AppSettings.Current.Save();
            DialogResult = DialogResult.OK;
            Close();
        };
        cancel.Click += (_, _) => { DialogResult = DialogResult.Cancel; Close(); };

        Controls.AddRange(new Control[] { header, _useLibRaw, _highPrecision, _showNumber, _showScrollBars, note, note2, ok, cancel });
    }
}
