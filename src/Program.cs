using System;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;
using AwayPhotoRawEditor.Forms;

namespace AwayPhotoRawEditor;

internal static class Program
{
    [STAThread]
    private static void Main(string[] args)
    {
        // DPI 必須最先決定：Theme 的靜態字型與所有版面常數都依賴 Ui.Scale，
        // 而 SetHighDpiMode 也必須在建立任何視窗之前呼叫。
        ApplicationConfiguration.Initialize(); // high-DPI + visual styles from csproj
        AppSettings.Load();                    // 只讀 XML，不碰 Theme/UI，可以放在 Ui.Init 之前
        Imaging.Gpu.GpuPipeline.Enabled = AppSettings.Current.UseGpu;
        Ui.Init(AppSettings.Current.UiScalePercent);
        if (float.TryParse(Environment.GetEnvironmentVariable("AWPR_UI_SCALE"),
                System.Globalization.NumberStyles.Float,
                System.Globalization.CultureInfo.InvariantCulture, out var forcedScale))
            Ui.ForceScale(forcedScale);   // 診斷：不改系統設定就能截出各種縮放的畫面

        var language = AppSettings.Current.UiLanguage;
        if (Enum.TryParse<AppLanguage>(Environment.GetEnvironmentVariable("AWPR_UI_LANGUAGE"), true, out var diagnosticLanguage))
        {
            language = diagnosticLanguage;
            AppSettings.Current.UiLanguage = diagnosticLanguage; // in-memory only
        }
        // 第一次執行（還沒有 settings.xml）先問語言，讓非中文使用者一開始就看得懂介面。
        // ⚠️ 一定要在 L.SetLanguage / Theme 之前：Theme 的快取靜態字型是依 L.CurrentLanguage
        // 決定字型家族的，先碰到 Theme 就得靠 Application.Restart() 才能換回來。
        // FirstRunLanguageForm 為此完全不引用 Theme（見該檔說明）。
        else if (AppSettings.IsFirstRun && !(args.Length > 0 && args[0].StartsWith("--")))
        {
            using var picker = new FirstRunLanguageForm(L.GuessFromSystem());
            picker.ShowDialog();
            language = picker.SelectedLanguage;
            AppSettings.Current.UiLanguage = language;
            AppSettings.Current.Save();   // 存起來才不會每次啟動都問
        }
        L.SetLanguage(language);

        var style = AppSettings.Current.InterfaceStyle;
        // Visual diagnostics can exercise both palettes without modifying settings.xml.
        if (Enum.TryParse<UiStyle>(Environment.GetEnvironmentVariable("AWPR_UI_STYLE"), true, out var diagnosticStyle))
        {
            style = diagnosticStyle;
            AppSettings.Current.InterfaceStyle = diagnosticStyle; // in-memory only; never saved by diagnostics
        }
        Theme.Apply(style);

        // Headless engine self-test: --selftest <imagePath> <reportPath>
        // GPU 對照：--gputest <img> <report>（CPU 與 GPU 各算一次，量差異與耗時）
        if (args.Length >= 3 && args[0] == "--gputest")
        {
            try { Diagnostics.GpuParity.Run(args[1], args[2]); }
            catch (Exception ex) { System.IO.File.WriteAllText(args[2] + ".err.txt", ex.ToString()); }
            return;
        }
        if (args.Length >= 3 && args[0] == "--selftest")
        {
            AwayPhotoRawEditor.Diagnostics.SelfTest.Run(args[1], args[2]);
            return;
        }

        // Export pipeline test: --exporttest <imagePath> <outDir> <reportPath>
        if (args.Length >= 4 && args[0] == "--exporttest")
        {
            AwayPhotoRawEditor.Diagnostics.SelfTest.RunExport(args[1], args[2], args[3]);
            return;
        }

        // Main window screenshot: --shot <folder> <outPng> [waitMs] [WxH]
        if (args.Length >= 3 && args[0] == "--shot")
        {
            try
            {
                Headless = true;   // the off-screen window can steal keyboard focus — ignore shortcuts
                var mf = new MainForm();
                mf.WindowState = FormWindowState.Normal;
                mf.StartPosition = FormStartPosition.Manual;
                mf.Location = new System.Drawing.Point(-4000, -4000);
                // 預設尺寸是 96 DPI 設計值（要縮放才裝得下同樣的版面）；
                // 明確指定的 WxH 則視為實際像素，方便測矮視窗捲軸。
                var size = Ui.Sz(1500, 1040);
                if (args.Length >= 5)
                {
                    var parts = args[4].Split('x', 'X');
                    if (parts.Length == 2 && int.TryParse(parts[0], out var pw) && int.TryParse(parts[1], out var ph))
                        size = new System.Drawing.Size(pw, ph);
                }
                mf.MinimumSize = System.Drawing.Size.Empty;   // 允許矮於 1100x700 以測試欄位捲軸
                mf.ClientSize = size;
                mf.Show();
                mf.TestOpen(args[1]);
                int waitMs = args.Length >= 4 && int.TryParse(args[3], out var w) ? w : 9000;
                var sw = System.Diagnostics.Stopwatch.StartNew();
                while (sw.ElapsedMilliseconds < waitMs) { Application.DoEvents(); System.Threading.Thread.Sleep(40); }
                using var bmp = new System.Drawing.Bitmap(mf.Width, mf.Height);
                mf.DrawToBitmap(bmp, new System.Drawing.Rectangle(0, 0, mf.Width, mf.Height));
                bmp.Save(args[2]);
                mf.Close();
            }
            catch (Exception ex)
            {
                System.IO.File.WriteAllText(args[2] + ".err.txt", ex.ToString());
            }
            return;
        }

        // Dialog screenshot: --dlgshot <export|settings|...> <outPng>（folder 已移除：開啟資料夾改用系統對話框）
        if (args.Length >= 3 && args[0] == "--dlgshot")
        {
            Form dlg = args[1] switch
            {
                "export" => new Forms.ExportForm(new Export.ExportSettings(), 12),
                "progress" => new Forms.ProgressForm("產生快取（縮圖＋預覽）",
                    async (prog, ct) => { prog.Report((7, 12, "DSCF1234.RAF")); await System.Threading.Tasks.Task.Delay(8000, ct); },
                    subtitle: "第一次產生快取與縮圖檔案需要一些時間\n請稍等...",
                    doneMessage: "完成，可以開始編輯"),
                "gradient" => BuildGradientRibbonProbe(),
                "gradoverlay" => BuildGradientOverlayProbe(),
                "presets" => new Forms.PresetEditorForm(),
                "about" => new Forms.AboutForm(),
                "firstrun" => new Forms.FirstRunLanguageForm(L.GuessFromSystem()),
                "fonts" => new Forms.FontSizeForm(AppSettings.Current.FontSizes),
                _ => new Forms.SettingsForm()
            };
            dlg.StartPosition = FormStartPosition.Manual;
            dlg.Location = new System.Drawing.Point(-4000, -4000);
            dlg.Show();
            for (int i = 0; i < 10; i++) { Application.DoEvents(); System.Threading.Thread.Sleep(40); }
            using (var bmp = new System.Drawing.Bitmap(dlg.Width, dlg.Height))
            {
                dlg.DrawToBitmap(bmp, new System.Drawing.Rectangle(0, 0, dlg.Width, dlg.Height));
                bmp.Save(args[2]);
            }
            dlg.Close();
            return;
        }

        // Controls gallery screenshot: --gallery <outPng> [sampleImage]
        if (args.Length >= 2 && args[0] == "--gallery")
        {
            var gf = new Diagnostics.GalleryForm(args.Length >= 3 ? args[2] : null);
            gf.StartPosition = FormStartPosition.Manual;
            gf.Location = new System.Drawing.Point(-3000, -3000);
            gf.Show();
            for (int i = 0; i < 12; i++) { Application.DoEvents(); System.Threading.Thread.Sleep(50); }
            using (var bmp = new System.Drawing.Bitmap(gf.Width, gf.Height))
            {
                gf.DrawToBitmap(bmp, new System.Drawing.Rectangle(0, 0, gf.Width, gf.Height));
                bmp.Save(args[1]);
            }
            gf.Close();
            return;
        }

        Application.ThreadException += (_, e) =>
            MessageBox.Show(e.Exception.Message, "AwayPhotoRawEditor",
                MessageBoxButtons.OK, MessageBoxIcon.Error);
        AppDomain.CurrentDomain.UnhandledException += (_, e) =>
            MessageBox.Show((e.ExceptionObject as Exception)?.Message ?? "Unknown error",
                "AwayPhotoRawEditor", MessageBoxButtons.OK, MessageBoxIcon.Error);

        Application.Run(new MainForm());
    }

    /// <summary>True in off-screen diagnostic modes (--shot): the invisible window still gets
    /// keyboard focus, so MainForm must ignore global shortcuts (Del would hide photos!).</summary>
    public static bool Headless { get; private set; }

    // Diagnostic: a form hosting the tools panel switched to the gradient tab, with one
    // gradient added, so --dlgshot gradient can screenshot the 新增線性漸層 ribbon.
    private static Form BuildGradientRibbonProbe()
    {
        var adj = new Models.ImageAdjustments();
        adj.Gradients.Add(new Models.LinearGradient { Exposure = -1.2 });
        adj.ActiveGradientIndex = 0;
        var tools = new Panels.ToolsPanel();
        tools.Location = Ui.Pt(12, 12);
        tools.Bind(adj);
        tools.SelectTool(Models.ToolMode.Gradient);
        var f = new Form
        {
            FormBorderStyle = FormBorderStyle.FixedDialog,
            ClientSize = new System.Drawing.Size(tools.Width + Ui.S(24), tools.Height + Ui.S(24)),
            BackColor = Controls.Theme.WindowBg, Text = "漸層 ribbon"
        };
        f.Controls.Add(tools);
        return f;
    }

    // Diagnostic: an ImageViewer showing the gradient overlay so --dlgshot gradoverlay can
    // verify the 白(位置)/黃(範圍)/藍(角度，右側) handle geometry.
    private static Form BuildGradientOverlayProbe()
    {
        var img = new System.Drawing.Bitmap(1200, 800);
        using (var g = System.Drawing.Graphics.FromImage(img))
            g.Clear(System.Drawing.Color.FromArgb(90, 100, 110));
        var adj = new Models.ImageAdjustments();
        adj.Gradients.Add(new Models.LinearGradient { CenterX = 0.5, CenterY = 0.45, Angle = 20, Range = 0.22, Exposure = -1 });
        adj.ActiveGradientIndex = 0;
        var viewer = new Controls.ImageViewer { Dock = DockStyle.Fill, Tool = Models.ToolMode.Gradient };
        viewer.Adjustments = adj;
        var f = new Form
        {
            FormBorderStyle = FormBorderStyle.FixedDialog,
            ClientSize = Ui.Sz(760, 520),
            BackColor = Controls.Theme.WindowBg, Text = "漸層 overlay"
        };
        f.Controls.Add(viewer);
        f.Shown += (_, _) => viewer.SetImage(img, true);
        return f;
    }
}
