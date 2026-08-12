using System.Diagnostics;
using System.IO;
using System.Text;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Exif;
using AwayPhotoRawEditor.Imaging;
using AwayPhotoRawEditor.Models;
using AwayPhotoRawEditor.Storage;

namespace AwayPhotoRawEditor.Diagnostics;

/// <summary>
/// End-to-end engine smoke test (invoked with --selftest &lt;imagePath&gt; &lt;reportPath&gt;).
/// Exercises tool detection, EXIF, RAW decode, cache, the processing pipeline,
/// histogram/mean and XML round-trip, writing a human-readable report.
/// </summary>
public static class SelfTest
{
    /// <summary>Export pipeline test (--exporttest &lt;image&gt; &lt;outDir&gt; &lt;report&gt;).</summary>
    public static void RunExport(string imagePath, string outDir, string reportPath)
    {
        var sb = new StringBuilder();
        void Line(string s) => sb.AppendLine(s);
        try
        {
            System.IO.Directory.CreateDirectory(outDir);
            Line("=== 匯出流程測試 ===");
            var item = new PhotoItem(imagePath);
            var exif = ExifReader.Read(imagePath);

            // Author a vivid adjustment so the output visibly differs.
            var adj = new ImageAdjustments { Contrast = 30, Vibrance = 40, Saturation = 12, Exposure = 0.3, Temperature = 6500 };
            PresetProfile.Get("鮮豔")!.ApplyTo(adj);
            AdjustmentXmlStore.Save(imagePath, adj, 0, exif);

            var loader = new RawLoader { UseLibRaw = true };
            var settings = new Export.ExportSettings
            {
                Location = Export.ExportLocation.Custom, CustomPath = outDir, UseSubFolder = false,
                Format = Export.ExportFormat.Jpeg, MaxLongEdge = 2000, JpegQuality = 90, OpenExplorerAfter = false
            };

            var prog = new Progress<(int, int, string)>();
            var written = Export.Exporter.Export(new[] { item }, settings, loader, prog, System.Threading.CancellationToken.None)
                .GetAwaiter().GetResult();

            Line($"輸出檔案數 : {written.Count}");
            foreach (var f in written)
            {
                using var b = new System.Drawing.Bitmap(f);
                Line($"  {f}  ->  {b.Width}x{b.Height}, {new System.IO.FileInfo(f).Length / 1024} KB");
            }

            // export again to verify de-dupe (_1 suffix)
            var w2 = Export.Exporter.Export(new[] { item }, settings, loader, prog, System.Threading.CancellationToken.None)
                .GetAwaiter().GetResult();
            Line($"第二次匯出（測試去重）: {string.Join(", ", w2.ConvertAll(System.IO.Path.GetFileName))}");
            Line("=== 完成 (成功) ===");
        }
        catch (Exception ex) { Line("!!! 失敗 !!!"); Line(ex.ToString()); }
        finally { System.IO.File.WriteAllText(reportPath, sb.ToString(), Encoding.UTF8); }
    }

    public static void Run(string imagePath, string reportPath)
    {
        var sb = new StringBuilder();
        void Line(string s) => sb.AppendLine(s);
        var sw = Stopwatch.StartNew();

        try
        {
            Line("=== AwayPhotoRawEditor 引擎自我測試 ===");
            Line($"目標檔案: {imagePath}");
            Line("");

            Line("[工具偵測]");
            Line($"  libraw.dll 路徑 : {AppPaths.FindLibRawDll() ?? "(未找到)"}");
            Line($"  LibRaw 可用     : {LibRawInterop.Available}");
            Line($"  exiftool.exe    : {AppPaths.FindExifTool() ?? "(未找到)"}");
            Line($"  ExifTool 可用   : {ExifReader.ExifToolAvailable}");
            if (AppPaths.IsRaw(imagePath) && LibRawInterop.ReadSizes(imagePath) is { } rs)
            {
                // 機型不被 LibRaw 支援時 Width/Height 會等於 RawWidth/RawHeight（拿不到可見區裁切表），
                // 遮罩邊就會變成輸出上的黑邊——這幾個數字是判斷的依據。
                Line($"  libraw sizes    : raw {rs.RawWidth}x{rs.RawHeight} / visible {rs.Width}x{rs.Height}" +
                     $" / i {rs.IWidth}x{rs.IHeight} / margin L{rs.LeftMargin} T{rs.TopMargin} / flip {rs.Flip}");
            }
            Line("");

            Line("[EXIF]");
            var exif = ExifReader.Read(imagePath);
            Line($"  相機   : {exif.CameraMake} {exif.CameraModel}");
            Line($"  鏡頭   : {exif.Lens}");
            Line($"  ISO/光圈/快門 : {exif.ISO} / {exif.Aperture} / {exif.Shutter}");
            Line($"  焦段/EV : {exif.FocalLength} / {exif.ExposureBias}");
            Line($"  白平衡/色溫 : {exif.WhiteBalance} / {exif.ColorTemperature}K");
            Line($"  日期   : {exif.DateTaken}");
            Line($"  尺寸(EXIF) : {exif.DimensionsDisplay}   檔案大小: {exif.FileSizeDisplay}");
            Line("");

            var loader = new RawLoader { UseLibRaw = true, UseHighPrecisionRawPipeline = false };
            Line("[全解析度解碼]");
            var t0 = sw.ElapsedMilliseconds;
            using var full = loader.LoadFull(imagePath);
            Line($"  尺寸 : {full.Width} x {full.Height}");
            Line($"  使用 LibRaw : {loader.LastFullDecodeUsedLibRaw}");
            Line($"  耗時 : {sw.ElapsedMilliseconds - t0} ms");
            Line("");

            Line("[縮圖 + Proxy 快取]");
            t0 = sw.ElapsedMilliseconds;
            using (var thumb = loader.LoadThumbnail(imagePath, 320, 240))
                Line($"  縮圖 : {thumb?.Width}x{thumb?.Height}  ({sw.ElapsedMilliseconds - t0} ms)");
            t0 = sw.ElapsedMilliseconds;
            loader.EnsureProxyCache(imagePath);
            using var proxy = loader.LoadProxyBitmap(imagePath);
            Line($"  Proxy : {proxy?.Width}x{proxy?.Height}  ({sw.ElapsedMilliseconds - t0} ms)");
            Line($"  快取檔 : {File.Exists(AppPaths.ThumbnailPath(imagePath))} / {File.Exists(AppPaths.ProxyPath(imagePath))}");
            Line("");

            Line("[處理管線]");
            var adj = new ImageAdjustments
            {
                Exposure = 0.4, Contrast = 20, Highlights = -30, Shadows = 25,
                Temperature = 6200, Vibrance = 25, Saturation = 8, Sharpening = 30
            };
            var wm = new Models.WatermarkSpec { Enabled = true, Text = "AwayPhotoRawEditor", FontSize = 40 };
            t0 = sw.ElapsedMilliseconds;
            using var rendered = ImageProcessor.Apply(proxy!, adj, new ProcessContext { Watermark = wm });
            Line($"  輸出尺寸 : {rendered.Width}x{rendered.Height}  ({sw.ElapsedMilliseconds - t0} ms)");
            var hist = ImageStats.ComputeHistogram(rendered);
            var (mr, mg, mb) = ImageStats.MeanColor(rendered);
            Line($"  RGB 平均 : {mr:0.0} / {mg:0.0} / {mb:0.0}   histogram max bin: {hist.Max}");

            string outJpg = Path.Combine(Path.GetDirectoryName(reportPath)!, "selftest_output.jpg");
            CacheManager.SaveJpeg(rendered, outJpg, 92);
            Line($"  已輸出處理後影像 : {outJpg}");
            Line("");

            Line("[XML 儲存往返]");
            AdjustmentXmlStore.Save(imagePath, adj, 0, exif);
            var reloaded = AdjustmentXmlStore.Load(imagePath);
            Line($"  儲存後可重新載入 : {reloaded is not null}");
            Line($"  值相等 : {reloaded?.ValueEquals(adj)}");
            Line($"  是預設佔位 : {AdjustmentXmlStore.IsDefaultPlaceholder(imagePath)}");
            Line("");

            Line($"總耗時 : {sw.ElapsedMilliseconds} ms");
            Line("=== 測試完成 (成功) ===");
        }
        catch (Exception ex)
        {
            Line("");
            Line("!!! 測試失敗 !!!");
            Line(ex.ToString());
        }
        finally
        {
            File.WriteAllText(reportPath, sb.ToString(), Encoding.UTF8);
        }
    }
}
