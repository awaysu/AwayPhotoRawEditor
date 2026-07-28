using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.Globalization;
using System.IO;
using System.Text;
using System.Text.Json;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Exif;

/// <summary>
/// Reads EXIF metadata. Prefers the bundled exiftool.exe; falls back to GDI+/WIC
/// property items when ExifTool is unavailable. Also extracts embedded JPEG
/// previews from RAW files (used as a decode fallback).
/// </summary>
public static class ExifReader
{
    public static bool ExifToolAvailable => AppPaths.FindExifTool() is not null;

    public static ExifData Read(string path)
    {
        var data = new ExifData { FilePath = path };
        try
        {
            var fi = new FileInfo(path);
            if (fi.Exists) data.FileSize = fi.Length;
        }
        catch { }

        if (ExifToolAvailable && TryReadWithExifTool(path, data))
            return data;

        TryReadWithGdi(path, data);
        return data;
    }

    // ---- ExifTool path ---------------------------------------------------

    private static bool TryReadWithExifTool(string path, ExifData data)
    {
        try
        {
            string exe = AppPaths.FindExifTool()!;
            // Numeric fields use the '#' suffix to force a raw number; text fields
            // (WhiteBalance, MeteringMode) keep ExifTool's friendly formatting.
            string[] args =
            {
                "-json", "-fast2",
                "-Make", "-Model", "-LensModel", "-LensID", "-Lens",
                "-ISO#", "-FNumber#", "-ExposureTime#", "-FocalLength#",
                "-ExposureCompensation#", "-WhiteBalance", "-MeteringMode",
                "-ColorTemperature#", "-DateTimeOriginal", "-CreateDate",
                "-ImageWidth#", "-ImageHeight#",
                path
            };
            string json = RunExifTool(exe, args, out _);
            if (string.IsNullOrWhiteSpace(json)) return false;

            using var doc = JsonDocument.Parse(json);
            if (doc.RootElement.ValueKind != JsonValueKind.Array || doc.RootElement.GetArrayLength() == 0)
                return false;
            var o = doc.RootElement[0];

            data.CameraMake = Str(o, "Make");
            data.CameraModel = Str(o, "Model");
            data.Lens = FirstNonEmpty(Str(o, "LensModel"), Str(o, "Lens"), Str(o, "LensID"));
            data.ISO = Str(o, "ISO");
            double fn = Num(o, "FNumber");
            data.Aperture = fn > 0 ? "f/" + fn.ToString("0.#", CultureInfo.InvariantCulture) : "";
            data.Shutter = FormatShutter(Num(o, "ExposureTime"));
            double fl = Num(o, "FocalLength");
            data.FocalLength = fl > 0 ? fl.ToString("0.#", CultureInfo.InvariantCulture) + " mm" : "";
            double ec = Num(o, "ExposureCompensation");
            data.ExposureBias = ec == 0 ? "0 EV" : ec.ToString("+0.0;-0.0", CultureInfo.InvariantCulture) + " EV";
            data.WhiteBalance = Str(o, "WhiteBalance");
            data.MeteringMode = Str(o, "MeteringMode");
            data.ColorTemperature = Num(o, "ColorTemperature");
            data.DateTaken = FirstNonEmpty(Str(o, "DateTimeOriginal"), Str(o, "CreateDate"));
            data.Width = (int)Num(o, "ImageWidth");
            data.Height = (int)Num(o, "ImageHeight");
            return true;
        }
        catch { return false; }
    }

    /// <summary>讀取檔案的 EXIF Orientation（1..8；讀不到回 1）。RAW 內嵌預覽轉正用。</summary>
    public static int ReadOrientation(string path)
    {
        string? exe = AppPaths.FindExifTool();
        if (exe is null) return 1;
        try
        {
            string outp = RunExifTool(exe, new[] { "-Orientation#", "-s3", "-fast2", path }, out _);
            return int.TryParse(outp.Trim(), out int v) && v is >= 1 and <= 8 ? v : 1;
        }
        catch { return 1; }
    }

    /// <summary>Extract an embedded preview JPEG from a RAW file (largest available). Null on failure.</summary>
    public static byte[]? ExtractPreview(string path)
    {
        if (!ExifToolAvailable) return null;
        foreach (var tag in new[] { "-JpgFromRaw", "-PreviewImage", "-ThumbnailImage" })
        {
            try
            {
                string exe = AppPaths.FindExifTool()!;
                byte[] bytes = RunExifToolBinary(exe, new[] { "-b", tag, path });
                if (bytes.Length > 1024) return bytes;
            }
            catch { }
        }
        return null;
    }

    /// <summary>
    /// Copy EXIF / XMP / IPTC metadata from <paramref name="sourcePath"/> onto an
    /// already-exported file, normalising Orientation to 1 (the pixels are baked
    /// upright on export). No-op when ExifTool is unavailable or the target format
    /// cannot carry metadata. Returns true on success.
    /// </summary>
    public static bool CopyMetadata(string sourcePath, string destPath)
    {
        string? exe = AppPaths.FindExifTool();
        if (exe is null || !File.Exists(sourcePath) || !File.Exists(destPath)) return false;
        try
        {
            string[] args =
            {
                "-overwrite_original", "-m", "-q",
                "-TagsFromFile", sourcePath,
                "-EXIF:all", "-XMP:all", "-IPTC:all",
                "-Orientation#=1",
                destPath
            };
            RunExifTool(exe, args, out _);
            return true;
        }
        catch { return false; }
    }

    private static string RunExifTool(string exe, string[] args, out string stderr)
    {
        var psi = new ProcessStartInfo(exe)
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
            StandardOutputEncoding = Encoding.UTF8
        };
        foreach (var a in args) psi.ArgumentList.Add(a);
        using var p = Process.Start(psi)!;
        string outp = p.StandardOutput.ReadToEnd();
        stderr = p.StandardError.ReadToEnd();
        p.WaitForExit(15000);
        return outp;
    }

    private static byte[] RunExifToolBinary(string exe, string[] args)
    {
        var psi = new ProcessStartInfo(exe)
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true
        };
        foreach (var a in args) psi.ArgumentList.Add(a);
        using var p = Process.Start(psi)!;
        using var ms = new MemoryStream();
        p.StandardOutput.BaseStream.CopyTo(ms);
        p.WaitForExit(15000);
        return ms.ToArray();
    }

    // ---- GDI+/WIC fallback ----------------------------------------------

    private static void TryReadWithGdi(string path, ExifData data)
    {
        try
        {
            using var img = Image.FromStream(File.OpenRead(path), false, false);
            data.Width = img.Width;
            data.Height = img.Height;
            foreach (var pi in img.PropertyItems)
            {
                switch (pi.Id)
                {
                    case 0x010F: data.CameraMake = AsciiVal(pi); break;
                    case 0x0110: data.CameraModel = AsciiVal(pi); break;
                    case 0x8827: data.ISO = ShortVal(pi).ToString(); break;
                    case 0x829D: data.Aperture = "f/" + RationalVal(pi).ToString("0.#"); break;
                    case 0x829A: data.Shutter = FormatShutter(RationalVal(pi)); break;
                    case 0x920A: data.FocalLength = RationalVal(pi).ToString("0.#") + " mm"; break;
                    case 0x9003: data.DateTaken = AsciiVal(pi); break;
                    case 0xA434: data.Lens = AsciiVal(pi); break;
                }
            }
        }
        catch { }
    }

    // ---- helpers ---------------------------------------------------------

    private static string Str(JsonElement o, string name) =>
        o.TryGetProperty(name, out var v)
            ? v.ValueKind == JsonValueKind.String ? v.GetString() ?? "" : v.ToString()
            : "";

    private static double Num(JsonElement o, string name)
    {
        if (!o.TryGetProperty(name, out var v)) return 0;
        if (v.ValueKind == JsonValueKind.Number && v.TryGetDouble(out var d)) return d;
        if (v.ValueKind == JsonValueKind.String &&
            double.TryParse(v.GetString(), NumberStyles.Any, CultureInfo.InvariantCulture, out var ds)) return ds;
        return 0;
    }

    private static string FirstNonEmpty(params string[] vals)
    {
        foreach (var v in vals) if (!string.IsNullOrWhiteSpace(v)) return v;
        return "";
    }

    private static string FormatShutter(double seconds)
    {
        if (seconds <= 0) return "";
        if (seconds >= 1) return seconds.ToString("0.#", CultureInfo.InvariantCulture) + " s";
        return "1/" + Math.Round(1.0 / seconds).ToString(CultureInfo.InvariantCulture) + " s";
    }

    private static string AsciiVal(PropertyItem pi) =>
        pi.Value is null ? "" : Encoding.ASCII.GetString(pi.Value).TrimEnd('\0', ' ');

    private static int ShortVal(PropertyItem pi) =>
        pi.Value is { Length: >= 2 } ? BitConverter.ToUInt16(pi.Value, 0) : 0;

    private static double RationalVal(PropertyItem pi)
    {
        if (pi.Value is { Length: >= 8 })
        {
            uint num = BitConverter.ToUInt32(pi.Value, 0);
            uint den = BitConverter.ToUInt32(pi.Value, 4);
            return den == 0 ? 0 : (double)num / den;
        }
        return 0;
    }
}
