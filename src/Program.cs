using System;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Forms;

namespace AwayPhotoRawEditor;

internal static class Program
{
    [STAThread]
    private static void Main(string[] args)
    {
        AppSettings.Load();

        // Headless engine self-test: --selftest <imagePath> <reportPath>
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

        ApplicationConfiguration.Initialize(); // high-DPI + visual styles from csproj

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
                var size = new System.Drawing.Size(1500, 1040);
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

        // Dialog screenshot: --dlgshot <folder|export|settings> <outPng>
        if (args.Length >= 3 && args[0] == "--dlgshot")
        {
            Form dlg = args[1] switch
            {
                "folder" => new Forms.FolderPickerForm(Environment.GetFolderPath(Environment.SpecialFolder.Desktop)),
                "export" => new Forms.ExportForm(new Export.ExportSettings(), 12),
                "progress" => new Forms.ProgressForm("產生快取（縮圖＋預覽）",
                    async (prog, ct) => { prog.Report((7, 12, "DSCF1234.RAF")); await System.Threading.Tasks.Task.Delay(8000, ct); },
                    subtitle: "第一次產生快取與縮圖檔案需要一些時間\n請稍等...",
                    doneMessage: "完成，可以開始編輯"),
                "gradient" => BuildGradientRibbonProbe(),
                "gradoverlay" => BuildGradientOverlayProbe(),
                "presets" => new Forms.PresetEditorForm(),
                "about" => new Forms.AboutForm(),
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
        tools.Location = new System.Drawing.Point(12, 12);
        tools.Bind(adj);
        tools.SelectTool(Models.ToolMode.Gradient);
        var f = new Form
        {
            FormBorderStyle = FormBorderStyle.FixedDialog,
            ClientSize = new System.Drawing.Size(tools.Width + 24, tools.Height + 24),
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
            ClientSize = new System.Drawing.Size(760, 520),
            BackColor = Controls.Theme.WindowBg, Text = "漸層 overlay"
        };
        f.Controls.Add(viewer);
        f.Shown += (_, _) => viewer.SetImage(img, true);
        return f;
    }
}
