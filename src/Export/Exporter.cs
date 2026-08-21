using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.Linq;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Imaging;
using AwayPhotoRawEditor.Models;
using AwayPhotoRawEditor.Storage;

namespace AwayPhotoRawEditor.Export;

/// <summary>Full-resolution non-destructive export pipeline.</summary>
public static class Exporter
{
    /// <summary>Export the given photos; returns the list of written file paths.</summary>
    public static async Task<List<string>> Export(
        IReadOnlyList<PhotoItem> items, ExportSettings settings, RawLoader loader,
        IProgress<(int done, int total, string msg)> progress, CancellationToken token)
    {
        var written = new List<string>();
        int total = items.Count, done = 0;
        int seq = 0;                                        // 「數字開始」流水號
        var dtCounts = new Dictionary<string, int>();       // 「日期時間」每秒計數（同秒數以末兩碼區別）

        await Task.Run(() =>
        {
            foreach (var item in items)
            {
                token.ThrowIfCancellationRequested();
                progress.Report((done, total, item.DisplayName));
                try
                {
                    string baseName = BuildBaseName(item, settings, ref seq, dtCounts);
                    ExportOne(item, settings, loader, written, baseName);
                }
                catch (Exception ex)
                {
                    throw new Exception(L.F("匯出「{0}」失敗：{1}", item.FileName, ex.Message), ex);
                }
                done++;
                progress.Report((done, total, item.DisplayName));
            }
        }, token);

        if (settings.OpenExplorerAfter && written.Count > 0)
            OpenInExplorer(written[0]);

        return written;
    }

    /// <summary>依重新命名規則產生輸出檔名（不含副檔名）。</summary>
    private static string BuildBaseName(PhotoItem item, ExportSettings s, ref int seq, Dictionary<string, int> dtCounts)
    {
        switch (s.Rename)
        {
            case RenameMode.DateTime:
            {
                DateTime dt = CaptureTime(item);
                string ts = dt.ToString("yyMMddHHmmss");
                dtCounts.TryGetValue(ts, out int c);
                c++;
                dtCounts[ts] = c;
                return $"IMG{ts}{c:00}";      // 末兩碼區別同秒數的照片
            }
            case RenameMode.Sequence:
                return $"IMG{++seq:00000}";
            default:                          // 按照原始檔案（沿用 _edited 命名）
            {
                string stem = Path.GetFileNameWithoutExtension(item.SourcePath);
                return item.IsVirtualCopy ? $"{stem}_copy{item.VirtualCopyIndex}_edited" : $"{stem}_edited";
            }
        }
    }

    /// <summary>拍攝時間：優先用快取 EXIF 的 DateTaken，取不到退回檔案修改時間。</summary>
    private static DateTime CaptureTime(PhotoItem item)
    {
        var exif = AdjustmentXmlStore.LoadExif(item.SourcePath, item.VirtualCopyIndex);
        if (exif is not null && !string.IsNullOrWhiteSpace(exif.DateTaken))
        {
            if (DateTime.TryParseExact(exif.DateTaken, "yyyy:MM:dd HH:mm:ss",
                    CultureInfo.InvariantCulture, DateTimeStyles.None, out var dt))
                return dt;
            if (DateTime.TryParse(exif.DateTaken, out dt))
                return dt;
        }
        try { return File.GetLastWriteTime(item.SourcePath); } catch { return DateTime.MinValue; }
    }

    private static void ExportOne(PhotoItem item, ExportSettings s, RawLoader loader, List<string> written, string baseName)
    {
        // 1) full-resolution decode — stays float: in high-precision mode the 16-bit decode
        //    used to be collapsed to a Bitmap right here, before the pipeline ever saw it.
        using FloatImageBuffer full = DecodeFull(item, loader);

        // 2) adjustments + the cached EXIF (carries the camera colour data for the WB matrix)
        var (adj, exif, _) = AdjustmentXmlStore.LoadAll(item.SourcePath, item.VirtualCopyIndex);
        adj ??= new ImageAdjustments();
        if (exif is not null && loader.EnrichCameraColor(item.SourcePath, exif))
        {
            // 舊 XML 沒有相機色彩資料：補上並存回，下次就不用再開一次檔
            try { AdjustmentXmlStore.Save(item.SourcePath, adj, item.VirtualCopyIndex, exif); } catch { }
        }

        // 3) apply the complete pipeline at full resolution (watermark authored at full res)
        using Bitmap processed = ImageProcessor.Apply(full, adj, new ProcessContext
        {
            ForExport = true, WatermarkScale = 1.0, Watermark = s.BuildWatermark(),
            Camera = exif?.Camera, WhiteBalanceReference = WhiteBalanceReference.Decode
        });

        // 4) resize so the longest edge (寬長最大) equals the target, preserving aspect
        using Bitmap final = ResizeToLongEdge(processed, s.MaxLongEdge);

        // 5) 解析度（像素/英寸）
        if (s.Resolution > 0)
            final.SetResolution(s.Resolution, s.Resolution);

        // 6) 檔名 + 存檔遇到相同檔名的處理
        string dir = s.ResolveOutputDir(item.SourcePath);
        string outPath = s.Conflict == ConflictMode.Overwrite
            ? Path.Combine(dir, baseName + s.Extension)    // 直接覆蓋
            : UniquePath(dir, baseName, s.Extension);       // 檔名接續 _數字

        SaveAs(final, outPath, s);

        // 7) copy source metadata onto the export (JPEG/TIFF/PNG carry EXIF; BMP does not)
        if (s.PreserveExif && s.Format != ExportFormat.Bmp)
            Exif.ExifReader.CopyMetadata(item.SourcePath, outPath);

        written.Add(outPath);
    }

    private static FloatImageBuffer DecodeFull(PhotoItem item, RawLoader loader)
    {
        if (loader.UseHighPrecisionRawPipeline)
        {
            var f = loader.DecodeFullFloat(item.SourcePath);
            if (f is not null) return f;
        }
        using var bmp = loader.DecodeFullBitmap(item.SourcePath)
                        ?? throw new IOException(L.T("無法解碼影像"));
        return FloatImageBuffer.FromBitmap(bmp);
    }

    /// <summary>Scale the image so its longest edge equals <paramref name="maxLongEdge"/>,
    /// preserving aspect ratio. Never upscales beyond the source resolution.</summary>
    private static Bitmap ResizeToLongEdge(Bitmap src, int maxLongEdge)
    {
        if (maxLongEdge <= 0) return (Bitmap)src.Clone();
        int longest = Math.Max(src.Width, src.Height);
        if (longest <= maxLongEdge) return (Bitmap)src.Clone();   // 不放大

        double scale = (double)maxLongEdge / longest;
        int w = Math.Max(1, (int)Math.Round(src.Width * scale));
        int h = Math.Max(1, (int)Math.Round(src.Height * scale));
        return CacheManager.ResizeTo(src, w, h);
    }

    private static string UniquePath(string dir, string name, string ext)
    {
        string p = Path.Combine(dir, name + ext);
        int n = 1;
        while (File.Exists(p)) p = Path.Combine(dir, $"{name}_{n++}{ext}");
        return p;
    }

    private static void SaveAs(Bitmap bmp, string path, ExportSettings s)
    {
        switch (s.Format)
        {
            case ExportFormat.Jpeg:
                var enc = ImageCodecInfo.GetImageEncoders().First(c => c.FormatID == ImageFormat.Jpeg.Guid);
                using (var ep = new EncoderParameters(1))
                {
                    ep.Param[0] = new EncoderParameter(Encoder.Quality, (long)Math.Clamp(s.JpegQuality, 50, 100));
                    bmp.Save(path, enc, ep);
                }
                break;
            case ExportFormat.Bmp: bmp.Save(path, ImageFormat.Bmp); break;
            case ExportFormat.Tiff: bmp.Save(path, ImageFormat.Tiff); break;
            case ExportFormat.Png: bmp.Save(path, ImageFormat.Png); break;
        }
    }

    private static void OpenInExplorer(string file)
    {
        try { Process.Start(new ProcessStartInfo("explorer.exe", $"/select,\"{file}\"") { UseShellExecute = true }); }
        catch { }
    }
}
