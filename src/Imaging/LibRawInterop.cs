using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Runtime.InteropServices;
using AwayPhotoRawEditor.App;

namespace AwayPhotoRawEditor.Imaging;

/// <summary>
/// Thin P/Invoke wrapper over libraw.dll (LibRaw C API). The DLL is located via
/// AppPaths and loaded through a DllImportResolver so it can live under tools/.
/// Every public method is guarded and returns null on any failure so callers can
/// fall back to WIC / embedded previews without crashing.
/// </summary>
public static class LibRawInterop
{
    private const string Dll = "libraw";

    private static readonly object Gate = new();
    private static bool _initialized;
    private static bool _available;

    public static bool Available
    {
        get { EnsureInit(); return _available; }
    }

    public static string? DllPath { get; private set; }

    private static void EnsureInit()
    {
        if (_initialized) return;
        lock (Gate)
        {
            if (_initialized) return;
            _initialized = true;
            try
            {
                DllPath = AppPaths.FindLibRawDll();
                if (DllPath is null || !File.Exists(DllPath)) { _available = false; return; }

                // Ensure dependent DLLs in the same folder resolve.
                var dir = Path.GetDirectoryName(DllPath)!;
                SetDllDirectory(dir);
                NativeLibrary.SetDllImportResolver(typeof(LibRawInterop).Assembly, Resolver);

                // Probe: init + close.
                var h = libraw_init(0);
                if (h != IntPtr.Zero) { libraw_close(h); _available = true; }
            }
            catch { _available = false; }
        }
    }

    private static IntPtr Resolver(string libraryName, System.Reflection.Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (libraryName == Dll && DllPath is not null && NativeLibrary.TryLoad(DllPath, out var handle))
            return handle;
        return IntPtr.Zero;
    }

    // ---- Native entry points --------------------------------------------

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr libraw_init(uint flags);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Unicode)]
    private static extern int libraw_open_wfile(IntPtr lr, string fname);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern int libraw_unpack(IntPtr lr);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern int libraw_unpack_thumb(IntPtr lr);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern int libraw_dcraw_process(IntPtr lr);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr libraw_dcraw_make_mem_image(IntPtr lr, out int errc);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr libraw_dcraw_make_mem_thumb(IntPtr lr, out int errc);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern void libraw_dcraw_clear_mem(IntPtr img);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern void libraw_close(IntPtr lr);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern void libraw_set_output_bps(IntPtr lr, int value);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern void libraw_set_output_color(IntPtr lr, int value);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern void libraw_set_no_auto_bright(IntPtr lr, int value);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    private static extern void libraw_set_gamma(IntPtr lr, int index, float value);

    [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool SetDllDirectory(string lpPathName);

    // libraw_processed_image_t: int type; ushort h,w,colors,bits; uint data_size; byte data[]
    private const int HeaderSize = 16;
    private const int LIBRAW_IMAGE_JPEG = 1;
    private const int LIBRAW_IMAGE_BITMAP = 2;

    // ---- Public decode API ----------------------------------------------

    /// <summary>Full-resolution 8-bit sRGB decode. Null on failure.</summary>
    public static Bitmap? DecodeToBitmap(string path) => RunLargeStack(() => DecodeProcessed(path, bps: 8) as Bitmap);

    /// <summary>Full-resolution high-precision decode (16-bit → float). Null on failure.</summary>
    public static FloatImageBuffer? DecodeToFloat(string path) => RunLargeStack(() => DecodeProcessed(path, bps: 16) as FloatImageBuffer);

    /// <summary>
    /// LibRaw's full RAW demosaic uses large on-stack buffers and can overflow the
    /// default 1 MB stack of a thread-pool thread (STATUS_STACK_OVERFLOW / 0xC00000FD).
    /// Run native decode on a dedicated thread with a generous 64 MB stack.
    /// </summary>
    private static T RunLargeStack<T>(Func<T> fn)
    {
        T result = default!;
        Exception? error = null;
        var t = new System.Threading.Thread(() =>
        {
            try { result = fn(); }
            catch (Exception ex) { error = ex; }
        }, 64 * 1024 * 1024)
        { IsBackground = true, Name = "LibRawDecode" };
        t.Start();
        t.Join();
        if (error != null) throw error;
        return result;
    }

    private static object? DecodeProcessed(string path, int bps)
    {
        if (!Available) return null;
        IntPtr lr = IntPtr.Zero, img = IntPtr.Zero;
        try
        {
            lr = libraw_init(0);
            if (lr == IntPtr.Zero) return null;
            libraw_set_output_color(lr, 1); // sRGB
            libraw_set_output_bps(lr, bps);
            libraw_set_no_auto_bright(lr, 0);

            if (libraw_open_wfile(lr, path) != 0) return null;
            if (libraw_unpack(lr) != 0) return null;
            if (libraw_dcraw_process(lr) != 0) return null;

            img = libraw_dcraw_make_mem_image(lr, out _);
            if (img == IntPtr.Zero) return null;

            ReadHeader(img, out int type, out int w, out int h, out int colors, out int bits, out int dataSize);
            if (type != LIBRAW_IMAGE_BITMAP || colors < 3 || w <= 0 || h <= 0) return null;
            IntPtr data = img + HeaderSize;

            return bits == 16
                ? BitmapFrom16(data, w, h, colors)
                : BitmapFrom8Bmp(data, w, h, colors);
        }
        catch { return null; }
        finally
        {
            if (img != IntPtr.Zero) libraw_dcraw_clear_mem(img);
            if (lr != IntPtr.Zero) libraw_close(lr);
        }
    }

    /// <summary>Decode the embedded camera thumbnail/preview (fast). Null on failure.</summary>
    public static Bitmap? DecodeThumbnail(string path) => RunLargeStack(() => DecodeThumbnailCore(path));

    private static Bitmap? DecodeThumbnailCore(string path)
    {
        if (!Available) return null;
        IntPtr lr = IntPtr.Zero, img = IntPtr.Zero;
        try
        {
            lr = libraw_init(0);
            if (lr == IntPtr.Zero) return null;
            if (libraw_open_wfile(lr, path) != 0) return null;
            int flip = ReadFlip(lr);   // 內嵌縮圖多為橫躺儲存、不帶方向標籤，要靠相機的 flip 轉正
            if (libraw_unpack_thumb(lr) != 0) return null;
            img = libraw_dcraw_make_mem_thumb(lr, out _);
            if (img == IntPtr.Zero) return null;

            ReadHeader(img, out int type, out int w, out int h, out int colors, out int bits, out int dataSize);
            IntPtr data = img + HeaderSize;

            if (type == LIBRAW_IMAGE_JPEG)
            {
                var bytes = new byte[dataSize];
                Marshal.Copy(data, bytes, 0, dataSize);
                if (flip == 0) return WicDecoder.LoadBytes(bytes);   // 尊重縮圖 JPEG 自帶的方向（若有）
                using var ms = new MemoryStream(bytes);
                using var tmp = new Bitmap(ms);
                var bmp = new Bitmap(tmp);   // detach from stream；只按 flip 轉一次，避免與自帶標籤重複旋轉
                ApplyFlip(bmp, flip);
                return bmp;
            }
            if (type == LIBRAW_IMAGE_BITMAP && colors >= 3)
            {
                var bmp = bits == 16 ? BitmapFrom16(data, w, h, colors).ToBitmap()
                                     : BitmapFrom8Bmp(data, w, h, colors);
                ApplyFlip(bmp, flip);
                return bmp;
            }
            return null;
        }
        catch { return null; }
        finally
        {
            if (img != IntPtr.Zero) libraw_dcraw_clear_mem(img);
            if (lr != IntPtr.Zero) libraw_close(lr);
        }
    }

    // libraw 0.22.1 的 C API 沒有 flip getter。libraw_data_t 開頭為 image 指標(x64=8B)，
    // 接著 sizes{8×ushort=16 + uint raw_pitch=4 + padding=4 + double pixel_aspect=8}，
    // 故 sizes.flip 位於位移 40（已以實檔驗證：直幅 ARW=5、橫幅=0）。
    // ⚠️ DLL 版本固定在 tools/，升級 LibRaw 時要重新驗證此位移。
    private const int SizesFlipOffset = 40;

    private static int ReadFlip(IntPtr lr)
    {
        try
        {
            int f = Marshal.ReadInt32(lr, SizesFlipOffset);
            return f is 3 or 5 or 6 ? f : 0;   // 位移不符預期時讀到垃圾值 → 一律視為不旋轉
        }
        catch { return 0; }
    }

    /// <summary>依 libraw sizes.flip 轉正（3=180°、5=逆時針90°、6=順時針90°）。</summary>
    private static void ApplyFlip(Bitmap bmp, int flip)
    {
        var op = flip switch
        {
            3 => RotateFlipType.Rotate180FlipNone,
            5 => RotateFlipType.Rotate270FlipNone,
            6 => RotateFlipType.Rotate90FlipNone,
            _ => RotateFlipType.RotateNoneFlipNone
        };
        if (op != RotateFlipType.RotateNoneFlipNone) bmp.RotateFlip(op);
    }

    // ---- Buffer conversion ----------------------------------------------

    private static void ReadHeader(IntPtr img, out int type, out int w, out int h,
        out int colors, out int bits, out int dataSize)
    {
        type = Marshal.ReadInt32(img, 0);
        h = (ushort)Marshal.ReadInt16(img, 4);
        w = (ushort)Marshal.ReadInt16(img, 6);
        colors = (ushort)Marshal.ReadInt16(img, 8);
        bits = (ushort)Marshal.ReadInt16(img, 10);
        dataSize = Marshal.ReadInt32(img, 12);
    }

    private static unsafe Bitmap BitmapFrom8Bmp(IntPtr data, int w, int h, int colors)
    {
        var bmp = new Bitmap(w, h, PixelFormat.Format32bppArgb);
        var bd = bmp.LockBits(new Rectangle(0, 0, w, h), ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
        try
        {
            byte* src = (byte*)data;
            for (int y = 0; y < h; y++)
            {
                byte* dst = (byte*)bd.Scan0 + (long)y * bd.Stride;
                byte* s = src + (long)y * w * colors;
                for (int x = 0; x < w; x++)
                {
                    byte r = s[x * colors + 0];
                    byte g = s[x * colors + 1];
                    byte b = s[x * colors + 2];
                    dst[x * 4 + 0] = b; dst[x * 4 + 1] = g; dst[x * 4 + 2] = r; dst[x * 4 + 3] = 255;
                }
            }
        }
        finally { bmp.UnlockBits(bd); }
        return bmp;
    }

    private static unsafe FloatImageBuffer BitmapFrom16(IntPtr data, int w, int h, int colors)
    {
        var buf = new FloatImageBuffer(w, h);
        const float inv = 1f / 65535f;
        ushort* src = (ushort*)data;
        fixed (float* dstBase = buf.Data)
        {
            for (int y = 0; y < h; y++)
            {
                ushort* s = src + (long)y * w * colors;
                float* dst = dstBase + (long)y * w * 4;
                for (int x = 0; x < w; x++)
                {
                    dst[x * 4 + 0] = s[x * colors + 0] * inv;
                    dst[x * 4 + 1] = s[x * colors + 1] * inv;
                    dst[x * 4 + 2] = s[x * colors + 2] * inv;
                    dst[x * 4 + 3] = 1f;
                }
            }
        }
        return buf;
    }
}
