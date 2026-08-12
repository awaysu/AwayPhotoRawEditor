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
    /// <param name="expectedVisible">可見區尺寸（EXIF FullImageSize）。LibRaw 對不支援的機型
    /// 會連遮罩邊一起輸出，有這個值就能精準裁掉；給 <c>Size.Empty</c> 則退回黑邊掃描。</param>
    public static Bitmap? DecodeToBitmap(string path, Size expectedVisible = default) =>
        RunLargeStack(() => DecodeProcessed(path, bps: 8, expectedVisible) as Bitmap);

    /// <summary>Full-resolution high-precision decode (16-bit → float). Null on failure.</summary>
    public static FloatImageBuffer? DecodeToFloat(string path, Size expectedVisible = default) =>
        RunLargeStack(() => DecodeProcessed(path, bps: 16, expectedVisible) as FloatImageBuffer);

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

    private static object? DecodeProcessed(string path, int bps, Size expectedVisible)
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

            // LibRaw 不認識的機型會連遮罩邊一起交出來（見 VisibleRect），先算出可見區再複製
            var rect = VisibleRect(data, w, h, colors, bits, expectedVisible);

            return bits == 16
                ? BitmapFrom16(data, w, h, colors, rect)
                : BitmapFrom8Bmp(data, w, h, colors, rect);
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
                // 縮圖是相機自帶的預覽，本來就不含遮罩邊，整塊複製
                var whole = new Rectangle(0, 0, w, h);
                var bmp = bits == 16 ? BitmapFrom16(data, w, h, colors, whole).ToBitmap()
                                     : BitmapFrom8Bmp(data, w, h, colors, whole);
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

    // 同一個 libraw_image_sizes_t（起點＝libraw_data_t + 8）的前 8 個 ushort：
    //   raw_height, raw_width, height, width, top_margin, left_margin, iheight, iwidth
    private const int SizesOffset = 8;

    /// <summary>libraw 回報的尺寸。`Width`/`Height` 是它認定的可見區，
    /// `RawWidth`/`RawHeight` 含遮罩邊；機型不被支援時兩者會相同。</summary>
    public readonly record struct RawSizes(
        int RawWidth, int RawHeight, int Width, int Height,
        int LeftMargin, int TopMargin, int IWidth, int IHeight, int Flip);

    /// <summary>診斷用：讀出 libraw 的尺寸欄位（`--selftest` 會印出來）。</summary>
    public static RawSizes? ReadSizes(string path) => RunLargeStack(() => ReadSizesCore(path));

    private static RawSizes? ReadSizesCore(string path)
    {
        if (!Available) return null;
        IntPtr lr = IntPtr.Zero;
        try
        {
            lr = libraw_init(0);
            if (lr == IntPtr.Zero) return null;
            if (libraw_open_wfile(lr, path) != 0) return null;
            if (libraw_unpack(lr) != 0) return null;
            ushort U(int i) => (ushort)Marshal.ReadInt16(lr, SizesOffset + i * 2);
            return new RawSizes(
                RawWidth: U(1), RawHeight: U(0), Width: U(3), Height: U(2),
                LeftMargin: U(5), TopMargin: U(4), IWidth: U(7), IHeight: U(6),
                Flip: Marshal.ReadInt32(lr, SizesFlipOffset));
        }
        catch { return null; }
        finally { if (lr != IntPtr.Zero) libraw_close(lr); }
    }

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

    // ---- 遮罩邊裁切 -------------------------------------------------------

    /// <summary>常見的可見區長寬比（連同其倒數，涵蓋直幅）。</summary>
    private static readonly double[] CommonAspects = { 3.0 / 2, 4.0 / 3, 16.0 / 9, 1.0, 5.0 / 4, 7.0 / 5 };

    private static bool PlausibleAspect(int w, int h)
    {
        if (w <= 0 || h <= 0) return false;
        double a = (double)w / h;
        foreach (double t in CommonAspects)
        {
            if (Math.Abs(a - t) / t < 0.005) return true;
            double inv = 1 / t;
            if (Math.Abs(a - inv) / inv < 0.005) return true;
        }
        return false;
    }

    /// <summary>
    /// LibRaw 對它不認識的機型拿不到可見區裁切表，會把整塊感光元件緩衝區交出來，
    /// 遮罩邊在輸出上就是純黑：ILCE-7RM6 配 LibRaw 0.22.1 解出 10240×7168，
    /// 實際可見區只有 9984×6656（3:2），右邊多 256 欄、下面多 512 列全黑。
    ///
    /// 這裡量出四邊「整列／整欄都是純黑」的範圍再裁掉，並用兩道保險避免誤裁：
    ///  1. 解出來的長寬比已經是常見比例 → 直接原樣返回（正常支援的機型零成本）
    ///  2. 每邊最多裁 12.5%，而且**裁完必須落在常見比例上**才採用
    ///     （夜景照整欄純黑也可能被量到，但裁完的比例不會剛好是 3:2/4:3/…）
    /// </summary>
    private static unsafe Rectangle VisibleRect(IntPtr data, int w, int h, int colors, int bits, Size expected)
    {
        var full = new Rectangle(0, 0, w, h);

        int thr = bits == 16 ? 2 * 257 : 2;          // 0..255 的門檻換算到 0..65535
        long rowStride = (long)w * colors;
        byte* p8 = (byte*)data;
        ushort* p16 = (ushort*)data;

        bool Dark(long index)                        // index = 像素序號 × colors
        {
            if (bits == 16)
                return p16[index] <= thr && p16[index + 1] <= thr && p16[index + 2] <= thr;
            return p8[index] <= thr && p8[index + 1] <= thr && p8[index + 2] <= thr;
        }
        bool ColBlack(int x)
        {
            for (int y = 0; y < h; y++) if (!Dark(y * rowStride + (long)x * colors)) return false;
            return true;
        }
        bool RowBlack(int y)
        {
            long b = y * rowStride;
            for (int x = 0; x < w; x++) if (!Dark(b + (long)x * colors)) return false;
            return true;
        }

        // ---- 1) 有 EXIF 的可見尺寸就用它（最準；遮罩邊界有去馬賽克過渡帶，純黑掃描會少裁幾十像素）
        int ew = expected.Width, eh = expected.Height;
        if (ew > 0 && eh > 0)
        {
            if ((w < h) != (ew < eh)) (ew, eh) = (eh, ew);   // libraw 已內部套用 flip，直/橫幅對調
            if (ew <= w && eh <= h && (ew < w || eh < h))
            {
                // 位置由「哪一邊是黑的」決定：旋轉過的緩衝區遮罩邊不一定在右下角
                int x = (w > ew && ColBlack(0)) ? w - ew : 0;
                int y = (h > eh && RowBlack(0)) ? h - eh : 0;
                return new Rectangle(x, y, ew, eh);
            }
            return full;   // 期望尺寸與解出來的一致（或不合理）→ 不動
        }

        // ---- 2) 沒有 EXIF 尺寸時的退路：純黑邊掃描 + 長寬比驗證
        if (PlausibleAspect(w, h)) return full;      // 已是常見比例，不用掃

        int maxTrimX = w / 8, maxTrimY = h / 8;
        int right = 0, left = 0, bottom = 0, top = 0;
        while (right < maxTrimX && ColBlack(w - 1 - right)) right++;
        while (left < maxTrimX && left < w - right - 1 && ColBlack(left)) left++;
        while (bottom < maxTrimY && RowBlack(h - 1 - bottom)) bottom++;
        while (top < maxTrimY && top < h - bottom - 1 && RowBlack(top)) top++;

        int nw = w - left - right, nh = h - top - bottom;
        if (nw <= 0 || nh <= 0) return full;
        if (nw == w && nh == h) return full;
        if (!PlausibleAspect(nw, nh)) return full;   // 裁完不像正常比例 → 不敢動
        return new Rectangle(left, top, nw, nh);
    }

    // ---- native buffer → managed ----------------------------------------

    private static unsafe Bitmap BitmapFrom8Bmp(IntPtr data, int srcW, int srcH, int colors, Rectangle rect)
    {
        int w = rect.Width, h = rect.Height;
        long rowStride = (long)srcW * colors;
        var bmp = new Bitmap(w, h, PixelFormat.Format32bppArgb);
        var bd = bmp.LockBits(new Rectangle(0, 0, w, h), ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
        try
        {
            byte* src = (byte*)data;
            for (int y = 0; y < h; y++)
            {
                byte* dst = (byte*)bd.Scan0 + (long)y * bd.Stride;
                byte* s = src + (y + rect.Y) * rowStride + (long)rect.X * colors;
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

    private static unsafe FloatImageBuffer BitmapFrom16(IntPtr data, int srcW, int srcH, int colors, Rectangle rect)
    {
        int w = rect.Width, h = rect.Height;
        long rowStride = (long)srcW * colors;
        var buf = new FloatImageBuffer(w, h);
        const float inv = 1f / 65535f;
        ushort* src = (ushort*)data;
        fixed (float* dstBase = buf.Data)
        {
            for (int y = 0; y < h; y++)
            {
                ushort* s = src + (y + rect.Y) * rowStride + (long)rect.X * colors;
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
