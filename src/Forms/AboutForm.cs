using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;

namespace AwayPhotoRawEditor.Forms;

/// <summary>關於：版本 / 作者(「作者:」與 Email 圖片同一行；Email 以嵌入資源 Assets\email.png 顯示、非文字) /
/// Source Code(GitHub 連結，點擊開瀏覽器) / 編譯時間，各項之間空一行。內文一律 10pt；
/// email.png 以 2×(20pt) 產生，執行期依 DeviceDpi 縮放到 10pt 等效大小與文字對齊。</summary>
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
        ClientSize = new Size(500, 322);

        var body = Theme.UI(10f);

        var header = new Label { Text = "AwayPhotoRawEditor", Font = Theme.UI(14f, FontStyle.Bold), ForeColor = Theme.Text, Left = 16, Top = 16, Width = 460, Height = 30 };

        var version = new Label { Text = L.T("版本：") + AppVersion.Version, Font = body, ForeColor = Theme.Text, Left = 20, Top = 58, Width = 460, Height = 26, TextAlign = ContentAlignment.MiddleLeft };

        // 作者: 與 Email 圖片同一行（圖片接在翻譯後的標題右側）
        var authorCap = new Label { Text = "作者:", Font = body, ForeColor = Theme.Text, AutoSize = true, Left = 20, Top = 108, BackColor = Theme.WindowBg };
        var emailImg = LoadEmailImage();
        var authorPic = new PictureBox { Image = emailImg, SizeMode = PictureBoxSizeMode.Zoom, BackColor = Theme.WindowBg };

        var srcCap = new Label { Text = "Source Code:", Font = body, ForeColor = Theme.Text, Left = 20, Top = 158, Width = 460, Height = 26, TextAlign = ContentAlignment.MiddleLeft };
        var link = new LinkLabel
        {
            Text = RepoUrl, Font = body, Left = 20, Top = 186, AutoSize = true, BackColor = Theme.WindowBg,
            LinkColor = Theme.AccentHover, ActiveLinkColor = Theme.Accent, VisitedLinkColor = Theme.AccentHover,
            LinkBehavior = LinkBehavior.HoverUnderline
        };
        link.LinkClicked += (_, _) =>
        {
            try { System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(RepoUrl) { UseShellExecute = true }); }
            catch { /* 無瀏覽器可開就算了 */ }
        };

        var buildTime = new Label { Text = L.T("編譯時間：") + AppVersion.BuildTime, Font = body, ForeColor = Theme.Text, Left = 20, Top = 234, Width = 460, Height = 26, TextAlign = ContentAlignment.MiddleLeft };

        var ok = new FlatButton { Text = "確定", Primary = true, Left = 404, Top = 274, Width = 80, Height = 32 };
        ok.Click += (_, _) => Close();

        Controls.AddRange(new Control[] { header, version, authorCap, authorPic, srcCap, link, buildTime, ok });
        L.Apply(this);

        // 圖以 2×(20pt) 產生：縮到 10pt 等效大小並跟著 DPI 放大，接在（翻譯後的）「作者:」右側置中
        if (emailImg != null)
        {
            float scale = DeviceDpi / 96f / 2f;
            authorPic.Size = new Size((int)(emailImg.Width * scale), (int)(emailImg.Height * scale));
            authorPic.Left = authorCap.Right + 8;
            authorPic.Top = authorCap.Top + (authorCap.Height - authorPic.Height) / 2;
        }
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
