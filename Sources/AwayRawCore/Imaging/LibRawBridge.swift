import Foundation
import CLibRawShim

/// LibRaw's reported sizes. `width`/`height` are what it considers the visible area;
/// `rawWidth`/`rawHeight` include the mask borders. On a camera model LibRaw has no crop
/// table for the two are identical — which is exactly the black-border bug below.
public struct RawSizes: Sendable {
    public var rawWidth = 0, rawHeight = 0
    public var width = 0, height = 0
    public var leftMargin = 0, topMargin = 0
    public var iWidth = 0, iHeight = 0
    public var flip = 0
}

/// RAW decoding through LibRaw (0.22.x), the same library and the same settings the
/// Windows build uses, so a photo renders identically on both platforms.
/// Carries a decode's result off the dedicated large-stack thread.
final class ResultBox<T>: @unchecked Sendable {
    var value: T?
}

public enum LibRawBridge {

    nonisolated(unsafe) private static var _available: Bool?

    public static var available: Bool {
        if let a = _available { return a }
        let a = awpr_libraw_available() != 0
        _available = a
        return a
    }

    public static var version: String {
        guard available, let v = awpr_libraw_version() else { return "" }
        return String(cString: v)
    }

    /// LibRaw's full demosaic uses large on-stack buffers and overflows the 512 KB a
    /// default secondary thread gets (the Windows build hit the same wall with the 1 MB
    /// thread-pool stack). Run native decodes on a dedicated 64 MB-stack thread.
    static func runLargeStack<T>(_ fn: @escaping () -> T) -> T {
        let box = ResultBox<T>()
        let sem = DispatchSemaphore(value: 0)
        let thread = Thread {
            box.value = fn()
            sem.signal()
        }
        thread.stackSize = 64 * 1024 * 1024
        thread.name = "LibRawDecode"
        thread.start()
        sem.wait()
        return box.value!
    }

    // ---- metadata --------------------------------------------------------

    /// Diagnostics: LibRaw's size fields (printed by `awpr-cli selftest`). A model whose
    /// visible area equals the raw area has no crop table — the black-border case.
    public static func readSizes(_ path: String) -> RawSizes? {
        guard available else { return nil }
        return runLargeStack {
            var s = awpr_sizes()
            guard awpr_read_sizes(path, &s) != 0 else { return nil }
            var r = RawSizes()
            r.rawWidth = Int(s.raw_width); r.rawHeight = Int(s.raw_height)
            r.width = Int(s.width); r.height = Int(s.height)
            r.leftMargin = Int(s.left_margin); r.topMargin = Int(s.top_margin)
            r.iWidth = Int(s.iwidth); r.iHeight = Int(s.iheight)
            r.flip = Int(s.flip)
            return r
        }
    }

    /// The camera's colour data (pre_mul / cam_mul / rgb_cam) for the linear pipeline's
    /// white-balance matrix. Metadata-only open, no unpack. Nil when LibRaw cannot open
    /// the file, the numbers are degenerate, or the sensor has four colour channels —
    /// those fall back to the black-body approximation.
    public static func readCameraColor(_ path: String) -> CameraColorInfo? {
        guard available else { return nil }
        return runLargeStack {
            var pre = [Double](repeating: 0, count: 3)
            var cam = [Double](repeating: 0, count: 3)
            var rgb = [Double](repeating: 0, count: 9)
            let ok = pre.withUnsafeMutableBufferPointer { p in
                cam.withUnsafeMutableBufferPointer { c in
                    rgb.withUnsafeMutableBufferPointer { r in
                        awpr_read_camera_color(path, p.baseAddress, c.baseAddress, r.baseAddress)
                    }
                }
            }
            guard ok != 0 else { return nil }

            var info = CameraColorInfo(preMul: pre, camMul: cam, rgbCam: rgb)
            // Some files record cam_mul as all zeros: fall back to pre_mul as the as-shot
            // setting rather than discarding the whole set.
            if !(info.camMul[0] > 0 && info.camMul[1] > 0 && info.camMul[2] > 0) {
                info.camMul = info.preMul
            }
            guard info.isValid else { return nil }
            info.preMul = ColorScience.normalizeGreen(info.preMul)
            info.camMul = ColorScience.normalizeGreen(info.camMul)
            guard info.isValid, ColorScience.invert3(info.rgbCam) != nil else { return nil }
            return info
        }
    }

    // ---- decode ----------------------------------------------------------

    /// Full-resolution 8-bit sRGB decode into a float buffer. Nil on failure.
    /// - Parameter expectedVisible: the visible size from EXIF. LibRaw hands back the
    ///   mask border as black pixels on models it has no crop table for; with this value
    ///   the border can be trimmed exactly.
    public static func decodeFull(_ path: String, bps: Int,
                                  expectedVisible: (width: Int, height: Int)? = nil) -> FloatImageBuffer? {
        guard available else { return nil }
        return runLargeStack {
            var img = awpr_image()
            guard awpr_decode_full(path, Int32(bps), &img) != 0 else { return nil }
            defer { awpr_free_image(&img) }
            guard let data = img.data else { return nil }

            let w = Int(img.width), h = Int(img.height)
            let colors = Int(img.colors), bits = Int(img.bits)
            let rect = visibleRect(data: data, w: w, h: h, colors: colors, bits: bits,
                                   expected: expectedVisible)
            return bits == 16
                ? bufferFrom16(data, srcW: w, colors: colors, rect: rect)
                : bufferFrom8(data, srcW: w, colors: colors, rect: rect)
        }
    }

    /// A decoded embedded preview: either raw RGB samples or the camera's JPEG blob,
    /// plus the flip needed to stand it upright.
    public enum ThumbResult {
        case jpeg(Data, flip: Int)
        case pixels(FloatImageBuffer, flip: Int)
    }

    /// Decode the embedded camera thumbnail/preview (fast — no demosaic). Nil on failure.
    public static func decodeThumbnail(_ path: String) -> ThumbResult? {
        guard available else { return nil }
        return runLargeStack {
            var img = awpr_image()
            var flip: Int32 = 0
            guard awpr_decode_thumb(path, &img, &flip) != 0 else { return nil }
            defer { awpr_free_image(&img) }
            guard let data = img.data else { return nil }

            if img.type == 1 {          // LIBRAW_IMAGE_JPEG
                let bytes = Data(bytes: data, count: Int(img.data_size))
                return .jpeg(bytes, flip: Int(flip))
            }
            let w = Int(img.width), h = Int(img.height)
            guard img.type == 2, img.colors >= 3, w > 0, h > 0 else { return nil }
            // The preview comes from the camera and never carries a mask border, so the
            // whole thing is copied.
            let rect = (x: 0, y: 0, w: w, h: h)
            let buf = Int(img.bits) == 16
                ? bufferFrom16(data, srcW: w, colors: Int(img.colors), rect: rect)
                : bufferFrom8(data, srcW: w, colors: Int(img.colors), rect: rect)
            guard let buf else { return nil }
            return .pixels(buf, flip: Int(flip))
        }
    }

    // ---- mask-border trimming --------------------------------------------

    /// Common visible aspect ratios, together with their reciprocals (portrait).
    static let commonAspects: [Double] = [3.0 / 2, 4.0 / 3, 16.0 / 9, 1.0, 5.0 / 4, 7.0 / 5]

    static func plausibleAspect(_ w: Int, _ h: Int) -> Bool {
        guard w > 0, h > 0 else { return false }
        let a = Double(w) / Double(h)
        for t in commonAspects {
            if abs(a - t) / t < 0.005 { return true }
            let inv = 1 / t
            if abs(a - inv) / inv < 0.005 { return true }
        }
        return false
    }

    /// On a camera model LibRaw has no visible-area crop table for, it hands back the whole
    /// sensor buffer and the mask border shows up as pure black: an ILCE-7RM6 on LibRaw
    /// 0.22.x decodes to 10240×7168 when the visible area is only 9984×6656 (3:2) — 256
    /// extra columns on the right, 512 extra rows at the bottom.
    ///
    /// Two safeguards keep this from trimming a legitimate frame:
    ///  1. an aspect ratio that is already a common one is returned untouched (no cost on
    ///     properly supported models),
    ///  2. each edge may lose at most 12.5%, and the result must land on a common aspect
    ///     ratio to be accepted (a night shot can have genuinely black columns, but
    ///     trimming them will not happen to produce exactly 3:2 or 4:3).
    ///
    /// A pure-black scan alone is not enough: demosaicing leaves a transition band at the
    /// mask boundary, so scanning stops dozens of pixels short (measured 10017×6673
    /// against the correct 9984×6656, still leaving a thin black line). The EXIF size is
    /// authoritative; the scan only decides *which* edge the padding is on — LibRaw has
    /// already applied flip, so a portrait shot's mask border is not at the bottom right.
    static func visibleRect(data: UnsafeRawPointer, w: Int, h: Int, colors: Int, bits: Int,
                            expected: (width: Int, height: Int)?) -> (x: Int, y: Int, w: Int, h: Int) {
        let full = (x: 0, y: 0, w: w, h: h)
        let thr = bits == 16 ? 2 * 257 : 2      // a 0..255 threshold scaled to 0..65535
        let rowStride = w * colors
        let p8 = data.assumingMemoryBound(to: UInt8.self)
        let p16 = data.assumingMemoryBound(to: UInt16.self)

        func dark(_ index: Int) -> Bool {
            if bits == 16 {
                return Int(p16[index]) <= thr && Int(p16[index + 1]) <= thr && Int(p16[index + 2]) <= thr
            }
            return Int(p8[index]) <= thr && Int(p8[index + 1]) <= thr && Int(p8[index + 2]) <= thr
        }
        func colBlack(_ x: Int) -> Bool {
            for y in 0..<h where !dark(y * rowStride + x * colors) { return false }
            return true
        }
        func rowBlack(_ y: Int) -> Bool {
            let b = y * rowStride
            for x in 0..<w where !dark(b + x * colors) { return false }
            return true
        }

        // 1) the EXIF visible size, when we have it, is the accurate answer
        if var (ew, eh) = expected, ew > 0, eh > 0 {
            // LibRaw has already applied flip internally, so swap for portrait.
            if (w < h) != (ew < eh) { swap(&ew, &eh) }
            if ew <= w && eh <= h && (ew < w || eh < h) {
                let x = (w > ew && colBlack(0)) ? w - ew : 0
                let y = (h > eh && rowBlack(0)) ? h - eh : 0
                return (x, y, ew, eh)
            }
            return full     // matches the decode (or is nonsense) → leave it alone
        }

        // 2) fallback without an EXIF size: black-edge scan plus an aspect-ratio check
        if plausibleAspect(w, h) { return full }

        let maxTrimX = w / 8, maxTrimY = h / 8
        var right = 0, left = 0, bottom = 0, top = 0
        while right < maxTrimX && colBlack(w - 1 - right) { right += 1 }
        while left < maxTrimX && left < w - right - 1 && colBlack(left) { left += 1 }
        while bottom < maxTrimY && rowBlack(h - 1 - bottom) { bottom += 1 }
        while top < maxTrimY && top < h - bottom - 1 && rowBlack(top) { top += 1 }

        let nw = w - left - right, nh = h - top - bottom
        if nw <= 0 || nh <= 0 { return full }
        if nw == w && nh == h { return full }
        if !plausibleAspect(nw, nh) { return full }   // does not look like a real frame
        return (left, top, nw, nh)
    }

    // ---- native buffer → float buffer ------------------------------------

    static func bufferFrom8(_ data: UnsafeRawPointer, srcW: Int, colors: Int,
                            rect: (x: Int, y: Int, w: Int, h: Int)) -> FloatImageBuffer? {
        guard rect.w > 0, rect.h > 0 else { return nil }
        let buf = FloatImageBuffer(width: rect.w, height: rect.h, zeroed: false)
        let src = data.assumingMemoryBound(to: UInt8.self)
        let rowStride = srcW * colors
        let inv: Float = 1.0 / 255.0
        let d = buf.data
        parallelRows(0, rect.h) { y in
            let s = src + (y + rect.y) * rowStride + rect.x * colors
            var o = y * rect.w * 4
            for x in 0..<rect.w {
                d[o]     = Float(s[x * colors + 0]) * inv
                d[o + 1] = Float(s[x * colors + 1]) * inv
                d[o + 2] = Float(s[x * colors + 2]) * inv
                d[o + 3] = 1
                o += 4
            }
        }
        return buf
    }

    static func bufferFrom16(_ data: UnsafeRawPointer, srcW: Int, colors: Int,
                             rect: (x: Int, y: Int, w: Int, h: Int)) -> FloatImageBuffer? {
        guard rect.w > 0, rect.h > 0 else { return nil }
        let buf = FloatImageBuffer(width: rect.w, height: rect.h, zeroed: false)
        let src = data.assumingMemoryBound(to: UInt16.self)
        let rowStride = srcW * colors
        let inv: Float = 1.0 / 65535.0
        let d = buf.data
        parallelRows(0, rect.h) { y in
            let s = src + (y + rect.y) * rowStride + rect.x * colors
            var o = y * rect.w * 4
            for x in 0..<rect.w {
                d[o]     = Float(s[x * colors + 0]) * inv
                d[o + 1] = Float(s[x * colors + 1]) * inv
                d[o + 2] = Float(s[x * colors + 2]) * inv
                d[o + 3] = 1
                o += 4
            }
        }
        return buf
    }
}
