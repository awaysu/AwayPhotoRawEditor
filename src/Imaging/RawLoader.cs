using System.Drawing;
using System.IO;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Exif;

namespace AwayPhotoRawEditor.Imaging;

/// <summary>Result of a thumbnail load with extra info for the UI/status line.</summary>
public readonly record struct ThumbnailInfo(Bitmap? Bitmap, int Width, int Height, bool FromCache, bool UsedLibRaw);

/// <summary>
/// Central image acquisition + cache orchestration. Decodes RAW via LibRaw
/// (falling back to embedded-preview via ExifTool, then WIC), decodes regular
/// formats via WIC, and manages RAW_TEMP thumbnail / proxy caches.
/// </summary>
public sealed class RawLoader
{
    public bool UseLibRaw { get; set; } = AppSettings.Current.UseLibRaw;
    public bool UseHighPrecisionRawPipeline { get; set; } = AppSettings.Current.UseHighPrecisionRawPipeline;

    public bool LibRawAvailable => LibRawInterop.Available;

    /// <summary>Best-effort record of whether the last full decode actually used LibRaw.</summary>
    public volatile bool LastFullDecodeUsedLibRaw;

    public const int DefaultProxyMaxDim = 2560;

    public void SyncFromSettings()
    {
        UseLibRaw = AppSettings.Current.UseLibRaw;
        UseHighPrecisionRawPipeline = AppSettings.Current.UseHighPrecisionRawPipeline;
    }

    // ---- Full-resolution decode (no adjustments) ------------------------

    /// <summary>Full-resolution, orientation-corrected bitmap. Throws only on total failure.</summary>
    public Bitmap LoadFull(string path)
    {
        var bmp = DecodeFullBitmap(path);
        if (bmp is null) throw new IOException($"無法讀取影像：{Path.GetFileName(path)}");
        return bmp;
    }

    public Bitmap? DecodeFullBitmap(string path)
    {
        if (AppPaths.IsRaw(path))
        {
            if (UseLibRaw && LibRawInterop.Available)
            {
                var b = LibRawInterop.DecodeToBitmap(path);
                if (b is not null) { LastFullDecodeUsedLibRaw = true; return b; }
            }
            LastFullDecodeUsedLibRaw = false;
            // Fallback: embedded preview via ExifTool.
            var preview = ExifReader.ExtractPreview(path);
            if (preview is not null)
            {
                var b = WicDecoder.LoadBytes(preview);
                if (b is not null) return b;
            }
            // Last resort: let WIC try the file directly (may work if OS RAW codec present).
            return WicDecoder.LoadFile(path);
        }

        LastFullDecodeUsedLibRaw = false;
        return WicDecoder.LoadFile(path);
    }

    /// <summary>Full-resolution high-precision float buffer (for export / high-precision preview).</summary>
    public FloatImageBuffer? DecodeFullFloat(string path)
    {
        if (AppPaths.IsRaw(path) && UseLibRaw && UseHighPrecisionRawPipeline && LibRawInterop.Available)
        {
            var f = LibRawInterop.DecodeToFloat(path);
            if (f is not null) { LastFullDecodeUsedLibRaw = true; return f; }
        }
        var bmp = DecodeFullBitmap(path);
        return bmp is null ? null : FloatImageBuffer.FromBitmap(bmp);
    }

    // ---- Thumbnails ------------------------------------------------------

    public Bitmap? LoadThumbnail(string path, int maxW, int maxH) =>
        LoadThumbnailWithInfo(path, maxW, maxH, useCache: true, materializeBitmap: true).Bitmap;

    public ThumbnailInfo LoadThumbnailWithInfo(string path, int maxW, int maxH, bool useCache, bool materializeBitmap)
    {
        // 1) cache hit
        if (useCache)
        {
            var cachePath = AppPaths.ThumbnailPath(path);
            if (File.Exists(cachePath))
            {
                if (!materializeBitmap) return new ThumbnailInfo(null, 0, 0, true, false);
                var cached = CacheManager.LoadBitmap(cachePath);
                if (cached is not null) return new ThumbnailInfo(cached, cached.Width, cached.Height, true, false);
            }
        }

        // 2) generate
        bool usedLibRaw = false;
        Bitmap? full = null;
        if (AppPaths.IsRaw(path) && UseLibRaw && LibRawInterop.Available)
        {
            full = LibRawInterop.DecodeThumbnail(path);
            if (full is not null) usedLibRaw = true;
        }
        full ??= DecodeFullBitmap(path);
        if (full is null) return new ThumbnailInfo(null, 0, 0, false, usedLibRaw);

        using (full)
        {
            var thumb = CacheManager.ResizeToFit(full, maxW, maxH);
            if (useCache)
            {
                try { CacheManager.SaveJpeg(thumb, AppPaths.ThumbnailPath(path)); } catch { }
            }
            if (!materializeBitmap) { thumb.Dispose(); return new ThumbnailInfo(null, 0, 0, false, usedLibRaw); }
            return new ThumbnailInfo(thumb, thumb.Width, thumb.Height, false, usedLibRaw);
        }
    }

    public bool EnsureThumbnailCache(string path, int maxW, int maxH)
    {
        var cachePath = AppPaths.ThumbnailPath(path);
        if (File.Exists(cachePath)) return true;
        var info = LoadThumbnailWithInfo(path, maxW, maxH, useCache: true, materializeBitmap: false);
        return File.Exists(cachePath) || info.FromCache;
    }

    public Bitmap? LoadThumbnailCache(string path)
    {
        var cachePath = AppPaths.ThumbnailPath(path);
        return File.Exists(cachePath) ? CacheManager.LoadBitmap(cachePath) : null;
    }

    // ---- Proxy -----------------------------------------------------------

    private static string ProxyFloatPath(string path) => AppPaths.ProxyPath(path) + ".f32";

    public bool EnsureProxyCache(string path, int maxDim = DefaultProxyMaxDim)
    {
        var proxyPath = AppPaths.ProxyPath(path);
        bool needPng = !File.Exists(proxyPath);
        bool needFloat = UseHighPrecisionRawPipeline && !File.Exists(ProxyFloatPath(path));
        if (!needPng && !needFloat) return true;

        if (UseHighPrecisionRawPipeline)
        {
            var full = DecodeFullFloat(path);
            if (full is null) return false;
            var scaled = ScaleFloat(full, maxDim);
            try
            {
                using var bmp = scaled.ToBitmap();
                CacheManager.SavePng(bmp, proxyPath);
                CacheManager.SaveFloat(scaled, ProxyFloatPath(path));
            }
            catch { return false; }
            return true;
        }
        else
        {
            var full = DecodeFullBitmap(path);
            if (full is null) return false;
            using (full)
            using (var scaled = CacheManager.ResizeToMaxDim(full, maxDim))
            {
                try { CacheManager.SavePng(scaled, proxyPath); } catch { return false; }
            }
            return true;
        }
    }

    /// <summary>Load the 8-bit proxy bitmap (generating it if necessary).</summary>
    public Bitmap? LoadProxyBitmap(string path, int maxDim = DefaultProxyMaxDim)
    {
        EnsureProxyCache(path, maxDim);
        var proxyPath = AppPaths.ProxyPath(path);
        return File.Exists(proxyPath) ? CacheManager.LoadBitmap(proxyPath) : DecodeFullBitmap(path);
    }

    /// <summary>Load the high-precision float proxy (falling back to the 8-bit proxy).</summary>
    public FloatImageBuffer? LoadProxyFloat(string path, int maxDim = DefaultProxyMaxDim)
    {
        EnsureProxyCache(path, maxDim);
        if (UseHighPrecisionRawPipeline)
        {
            var f = CacheManager.LoadFloat(ProxyFloatPath(path));
            if (f is not null) return f;
        }
        var bmp = LoadProxyBitmap(path, maxDim);
        return bmp is null ? null : FloatImageBuffer.FromBitmap(bmp);
    }

    private static FloatImageBuffer ScaleFloat(FloatImageBuffer src, int maxDim)
    {
        int longSide = Math.Max(src.Width, src.Height);
        if (longSide <= maxDim) return src;
        // Downscale via a bitmap round-trip (adequate for proxy generation).
        using var bmp = src.ToBitmap();
        using var scaled = CacheManager.ResizeToMaxDim(bmp, maxDim);
        return FloatImageBuffer.FromBitmap(scaled);
    }
}
