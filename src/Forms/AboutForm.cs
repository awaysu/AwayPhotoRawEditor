using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;

namespace AwayPhotoRawEditor.Forms;

/// <summary>關於：版本 / 編譯時間 / 作者(「作者:」與 Email 圖片同一行；Email 以嵌入資源
/// Assets\email.png 顯示、非文字) / 下載 / Source Code（都是可點的連結）/
/// 第三方元件（版本為執行期讀取，不寫死）/ 授權 / 二次開發說明，各項之間空一行。
///
/// 內文字級用 <c>Theme.Sizes.AboutBody</c>（設定 →「字體大小」可調）；email.png 是以
/// Segoe UI 20pt（≈26.67px @96dpi）產生的圖，所以縮放倍率要跟著 AboutBody 走，
/// 否則調大內文時它會變成唯一沒放大的東西。</summary>
public sealed class AboutForm : Form
{
    private const string RepoUrl = "https://github.com/awaysu/AwayPhotoRawEditor";
    private const string DownloadUrl = "https://www.awaysu.cc/software/awayphotoraweditor";

    /// <summary>email.png 產生時的字級（Segoe UI 20pt → 96 DPI 下約 26.67px）。</summary>
    private const float EmailImageRenderedPx = 20f * 96f / 72f;

    // 96 DPI 設計座標
    private const int DlgW = 560, TextX = 20, TextW = 520;

    public AboutForm()
    {
        Text = "關於";
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false; MinimizeBox = false;
        StartPosition = FormStartPosition.CenterParent;

        var body = Theme.UIPx(Theme.Sizes.AboutBody);
        int lineH = 28, gap = 44;   // 一行高度 / 各項之間（含空一行）的間距

        var header = new Label { Text = "AwayPhotoRawEditor", Font = Theme.UIPx(Theme.Sizes.AboutTitle, FontStyle.Bold), ForeColor = Theme.Text };
        Ui.Place(header, 16, 16, TextW, 34);

        int y = 62;
        var version = Text0(L.T("版本：") + AppVersion.Version, body, y, lineH);
        // 「檢查更新」擺在版本那一行的右端：它問的就是「我這版是不是最新的」，
        // 貼著版本號才看得懂。實際寬度與位置在下方量測區依翻譯後的字串校正。
        var checkBtn = new FlatButton { Text = "檢查更新" };
        int checkRowY = y;
        y += gap;

        // 編譯時間緊接版本：兩者都是「這份執行檔是哪一版」的資訊，放一起才好對照
        var buildTime = Text0(L.T("編譯時間：") + AppVersion.BuildTime, body, y, lineH);
        y += gap;

        // 作者: 與 Email 圖片同一行（圖片接在翻譯後的標題右側）
        var authorCap = new Label { Text = "作者:", Font = body, ForeColor = Theme.Text, AutoSize = true, Left = Ui.S(TextX), Top = Ui.S(y), BackColor = Theme.WindowBg };
        var emailImg = LoadEmailImage();
        var authorPic = new PictureBox { Image = emailImg, SizeMode = PictureBoxSizeMode.Zoom, BackColor = Theme.WindowBg };
        y += gap;

        var dlCap = Text0("下載:", body, y, lineH);
        y += lineH;
        var dlLink = MakeLink(DownloadUrl, body, y);
        y += gap;

        var srcCap = Text0("Source Code:", body, y, lineH);
        y += lineH;
        var srcLink = MakeLink(RepoUrl, body, y);
        y += gap;

        // 第三方元件聲明（LGPL/Artistic 署名；元件名稱與授權為專有名詞，不翻譯）
        // 版本在執行期讀取：升級 tools/ 之後這裡不會變成過期資訊。
        var thirdCap = Text0("第三方元件:", body, y, lineH);
        y += lineH;
        var thirdList = Text0(ThirdPartyText(), body, y, lineH * 2 + 4);
        thirdList.TextAlign = ContentAlignment.TopLeft;
        y += lineH * 2 + 4 + 16;

        // 本程式自己的授權。不是法律義務（授權約束的是再散布的人，不是著作權人），
        // 但緊接在下方「二次開發」的請求上面，剛好示範：聲明就是放這個位置。
        var license = Text0(L.T("授權：") + "BSD 3-Clause　© 2026 Chih-Wei Su (Awaysu)", body, y, lineH);
        y += gap;

        // 二次開發說明（與 README 同一段文字）
        var modifyNote = Text0(
            "歡迎自由修改成你自己的版本，只希望你能在你的「關於」視窗中提及來源是這裡（AwayPhotoRawEditor / Awaysu）。",
            body, y, lineH * 3);   // 高度稍後依實際翻譯量測校正
        modifyNote.TextAlign = ContentAlignment.TopLeft;
        modifyNote.ForeColor = Theme.TextDim;

        var ok = new FlatButton { Text = "確定", Primary = true };
        ok.Size = Ui.Sz(80, 32);
        ok.Click += (_, _) => Close();

        AutoScroll = true;   // 螢幕不夠高 / 字級調很大時仍可捲到底

        Controls.AddRange(new Control[]
        {
            header, version, buildTime, authorCap, authorPic, dlCap, dlLink,
            srcCap, srcLink, thirdCap, thirdList, license, modifyNote, ok, checkBtn
        });
        L.Apply(this);
        checkBtn.Click += async (_, _) => await CheckForUpdatesAsync(checkBtn);

        // 「檢查更新」的寬度依翻譯後的字串量測（法/西文比中文長一倍以上），再靠右擺到版本那一行；
        // 版本標籤要跟著縮短——它的背景不透明又橫跨整列，不縮就會蓋住按鈕（後加入的控制項 z-order 在後面）。
        int cbW = TextRenderer.MeasureText(checkBtn.Text, checkBtn.Font).Width + Ui.S(24);
        checkBtn.Size = new Size(Math.Max(Ui.S(96), cbW), Ui.S(30));
        checkBtn.Left = Ui.S(DlgW - 20) - checkBtn.Width;
        checkBtn.Top = Ui.S(checkRowY) + (Ui.S(lineH) - checkBtn.Height) / 2;
        version.Width = Math.Max(Ui.S(80), checkBtn.Left - version.Left - Ui.S(12));

        // 說明文字換行後的高度只有量過才知道（德/法/西文比中文長不少），
        // 據此擺「確定」並決定視窗高度，中文版下方才不會留一片空白。
        int noteH = TextRenderer.MeasureText(modifyNote.Text, modifyNote.Font,
            new Size(modifyNote.Width, 0), TextFormatFlags.WordBreak).Height;
        modifyNote.Height = Math.Max(Ui.S(lineH), noteH + Ui.S(6));
        ok.Left = Ui.S(DlgW - 20) - ok.Width;
        ok.Top = modifyNote.Bottom + Ui.S(18);
        ClientSize = Ui.FitWorkAreaPx(Ui.S(DlgW), ok.Bottom + Ui.S(20));

        // email.png 縮到與內文同字級（見 EmailImageRenderedPx），接在翻譯後的「作者:」右側置中
        if (emailImg != null)
        {
            float scale = Ui.Scale * Theme.Sizes.AboutBody / EmailImageRenderedPx;
            authorPic.Size = new Size((int)(emailImg.Width * scale), (int)(emailImg.Height * scale));
            authorPic.Left = authorCap.Right + Ui.S(8);
            authorPic.Top = authorCap.Top + (authorCap.Height - authorPic.Height) / 2;
        }
    }

    /// <summary>按「檢查更新」：問網站現在的最新版本，有新版就顯示更新說明並問要不要開下載頁。
    ///
    /// <para>只做「查詢＋開頁面」，不自動下載安裝：這個程式同時發行安裝檔與攜帶版 zip，
    /// 程式無從知道使用者手上是哪一種（攜帶版跑安裝檔會裝出第二份），讓使用者自己挑比較安全。</para>
    ///
    /// <para>這是 <c>async void</c> 事件處理常式，例外逸出會直接讓行程當掉，所以整段包 try/catch。</para></summary>
    private async Task CheckForUpdatesAsync(FlatButton btn)
    {
        string caption = btn.Text;          // 已經是翻譯後的字（L.Apply 在建構式就跑過了）
        btn.Enabled = false;
        btn.Text = L.T("檢查中…");
        btn.Invalidate();                   // FlatButton 沒有覆寫 OnTextChanged/OnEnabledChanged
        try
        {
            var info = await UpdateCheck.FetchAsync();

            if (IsDisposed || Disposing) return;      // 查詢期間使用者把視窗關了

            if (info is null)
                MessageBox.Show(this, L.T("無法連線到更新伺服器，請稍後再試。"), L.T("檢查更新"),
                    MessageBoxButtons.OK, MessageBoxIcon.Warning);
            else if (!info.UpdateAvailable)
                MessageBox.Show(this, L.F("目前已是最新版本（{0}）。", AppVersion.Version), L.T("檢查更新"),
                    MessageBoxButtons.OK, MessageBoxIcon.Information);
            else
            {
                string msg = L.F("有新版本可以下載。\n\n目前版本：{0}\n最新版本：v{1}",
                                 AppVersion.Version, info.LatestVersion);
                if (info.Notes.Length > 0) msg += "\n\n" + info.Notes;
                msg += "\n\n" + L.T("要開啟下載頁面嗎？");

                if (MessageBox.Show(this, msg, L.T("檢查更新"),
                        MessageBoxButtons.YesNo, MessageBoxIcon.Information) == DialogResult.Yes)
                    OpenUrl(info.PageUrl);
            }
        }
        catch { /* async void：例外絕不能逸出，查不到就當作沒查到 */ }
        finally
        {
            if (!IsDisposed)
            {
                btn.Text = caption;
                btn.Enabled = true;
                btn.Invalidate();
            }
        }
    }

    /// <summary>用系統預設瀏覽器開網址（開不起來就算了，不要為此跳錯誤）。</summary>
    private static void OpenUrl(string url)
    {
        try { System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(url) { UseShellExecute = true }); }
        catch { /* 無瀏覽器可開就算了 */ }
    }

    /// <summary>靠左的一段內文（x/寬固定，只給 y 與高度）。</summary>
    private static Label Text0(string text, Font font, int y, int h)
    {
        var l = new Label { Text = text, Font = font, ForeColor = Theme.Text, TextAlign = ContentAlignment.MiddleLeft };
        Ui.Place(l, TextX, y, TextW, h);
        return l;
    }

    private static LinkLabel MakeLink(string url, Font font, int y)
    {
        var link = new LinkLabel
        {
            Text = url, Font = font, Left = Ui.S(TextX), Top = Ui.S(y), AutoSize = true, BackColor = Theme.WindowBg,
            LinkColor = Theme.AccentHover, ActiveLinkColor = Theme.Accent, VisitedLinkColor = Theme.AccentHover,
            LinkBehavior = LinkBehavior.HoverUnderline
        };
        link.LinkClicked += (_, _) => OpenUrl(url);
        return link;
    }

    /// <summary>第三方元件三行，版本於執行期讀取（讀不到就只顯示名稱）。</summary>
    private static string ThirdPartyText()
    {
        string lib = Imaging.LibRawInterop.Version;
        string et = Exif.ExifReader.ExifToolVersion;
        string cs = Imaging.Gpu.GpuPipeline.LibraryVersion;
        string l1 = "LibRaw" + (lib.Length > 0 ? " " + lib : "") + " (LGPL 2.1)";
        string l2 = "ExifTool" + (et.Length > 0 ? " " + et : "") + " by Phil Harvey (Perl Artistic License)";
        string l3 = "ComputeSharp" + (cs.Length > 0 ? " " + cs : "") + " by Sergio Pedri (MIT) — GPU";
        return l1 + "\n" + l2 + "\n" + l3;
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
