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
        ClientSize = Ui.FitWorkArea(500, 420);

        var body = Theme.UIPx(Theme.Sizes.AboutBody);

        // 座標一律是 96 DPI 設計值。
        var header = new Label { Text = "AwayPhotoRawEditor", Font = Theme.UIPx(Theme.Sizes.AboutTitle, FontStyle.Bold), ForeColor = Theme.Text };
        Ui.Place(header, 16, 16, 460, 30);

        var version = new Label { Text = L.T("版本：") + AppVersion.Version, Font = body, ForeColor = Theme.Text, TextAlign = ContentAlignment.MiddleLeft };
        Ui.Place(version, 20, 58, 460, 26);

        // 作者: 與 Email 圖片同一行（圖片接在翻譯後的標題右側）
        var authorCap = new Label { Text = "作者:", Font = body, ForeColor = Theme.Text, AutoSize = true, Left = Ui.S(20), Top = Ui.S(108), BackColor = Theme.WindowBg };
        var emailImg = LoadEmailImage();
        var authorPic = new PictureBox { Image = emailImg, SizeMode = PictureBoxSizeMode.Zoom, BackColor = Theme.WindowBg };

        var srcCap = new Label { Text = "Source Code:", Font = body, ForeColor = Theme.Text, TextAlign = ContentAlignment.MiddleLeft };
        Ui.Place(srcCap, 20, 158, 460, 26);
        var link = new LinkLabel
        {
            Text = RepoUrl, Font = body, Left = Ui.S(20), Top = Ui.S(186), AutoSize = true, BackColor = Theme.WindowBg,
            LinkColor = Theme.AccentHover, ActiveLinkColor = Theme.Accent, VisitedLinkColor = Theme.AccentHover,
            LinkBehavior = LinkBehavior.HoverUnderline
        };
        link.LinkClicked += (_, _) =>
        {
            try { System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(RepoUrl) { UseShellExecute = true }); }
            catch { /* 無瀏覽器可開就算了 */ }
        };

        var buildTime = new Label { Text = L.T("編譯時間：") + AppVersion.BuildTime, Font = body, ForeColor = Theme.Text, TextAlign = ContentAlignment.MiddleLeft };
        Ui.Place(buildTime, 20, 234, 460, 26);

        // 第三方元件聲明（LGPL/Artistic 署名；元件名稱與授權為專有名詞，不翻譯）
        var thirdCap = new Label { Text = "第三方元件:", Font = body, ForeColor = Theme.Text, TextAlign = ContentAlignment.MiddleLeft };
        Ui.Place(thirdCap, 20, 282, 460, 26);
        var thirdList = new Label { Text = "LibRaw (LGPL 2.1)\nExifTool by Phil Harvey (Perl Artistic License)", Font = body, ForeColor = Theme.Text, TextAlign = ContentAlignment.TopLeft };
        Ui.Place(thirdList, 20, 310, 460, 48);

        var ok = new FlatButton { Text = "確定", Primary = true }; Ui.Place(ok, 404, 372, 80, 32);
        ok.Click += (_, _) => Close();

        Controls.AddRange(new Control[] { header, version, authorCap, authorPic, srcCap, link, buildTime, thirdCap, thirdList, ok });
        L.Apply(this);

        // 圖以 2×(20pt) 產生：縮到 10pt 等效大小並跟著 DPI 放大，接在（翻譯後的）「作者:」右側置中
        if (emailImg != null)
        {
            float scale = Ui.Scale / 2f;   // 圖是 2×(20pt) 產生的，縮回 10pt 等效再乘 DPI
            authorPic.Size = new Size((int)(emailImg.Width * scale), (int)(emailImg.Height * scale));
            authorPic.Left = authorCap.Right + Ui.S(8);
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
