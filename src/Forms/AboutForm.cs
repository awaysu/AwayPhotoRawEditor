using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;

namespace AwayPhotoRawEditor.Forms;

/// <summary>關於：作者(名字+Email 以嵌入資源圖片顯示，Assets\email.png，非文字) /
/// Source Code(GitHub 連結，點擊開瀏覽器) / 版本(取自 <see cref="AppVersion.Version"/>)。</summary>
public sealed class AboutForm : Form
{
    private const string RepoUrl = "https://github.com/awaysu/AwayPhotoRawEditor";

    public AboutForm()
    {
        Text = "關於";
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false; MinimizeBox = false;
        StartPosition = FormStartPosition.CenterParent;
        ClientSize = new Size(380, 290);

        var header = new Label { Text = "Away PhotoRaw Editor", Font = Theme.UI(14f, FontStyle.Bold), ForeColor = Theme.Text, Left = 16, Top = 16, Width = 340, Height = 28 };

        var authorCap = new Label { Text = "作者:", ForeColor = Theme.Text, Left = 20, Top = 60, Width = 340, Height = 24, TextAlign = ContentAlignment.MiddleLeft };
        var authorPic = new PictureBox { Image = LoadEmailImage(), SizeMode = PictureBoxSizeMode.AutoSize, Left = 20, Top = 86, BackColor = Theme.WindowBg };

        var srcCap = new Label { Text = "Source Code:", ForeColor = Theme.Text, Left = 20, Top = 124, Width = 340, Height = 24, TextAlign = ContentAlignment.MiddleLeft };
        var link = new LinkLabel
        {
            Text = RepoUrl, Left = 20, Top = 150, AutoSize = true, BackColor = Theme.WindowBg,
            LinkColor = Theme.AccentHover, ActiveLinkColor = Color.FromArgb(120, 180, 255), VisitedLinkColor = Theme.AccentHover,
            LinkBehavior = LinkBehavior.HoverUnderline
        };
        link.LinkClicked += (_, _) =>
        {
            try { System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(RepoUrl) { UseShellExecute = true }); }
            catch { /* 無瀏覽器可開就算了 */ }
        };

        var version = new Label { Text = "版本：" + AppVersion.Version, ForeColor = Theme.Text, Left = 20, Top = 192, Width = 340, Height = 26, TextAlign = ContentAlignment.MiddleLeft };

        var ok = new FlatButton { Text = "確定", Primary = true, Left = 284, Top = 242, Width = 80, Height = 32 };
        ok.Click += (_, _) => Close();

        Controls.AddRange(new Control[] { header, authorCap, authorPic, srcCap, link, version, ok });
    }

    /// <summary>從嵌入資源載入 Email 圖片（AwayPhotoRawEditor.Assets.email.png）。</summary>
    private static Image? LoadEmailImage()
    {
        try
        {
            var s = typeof(AboutForm).Assembly.GetManifestResourceStream("AwayPhotoRawEditor.Assets.email.png");
            return s is null ? null : Image.FromStream(s);
        }
        catch { return null; }
    }
}
