import Foundation
import CoreGraphics

/// Reads / writes the RAW_TEMP cache artifacts: thumbnail JPEGs, 8-bit proxy PNGs and a
/// lossless 16-bit proxy. Also hosts the high-quality resize helpers.
public enum CacheManager {

    // ---- bitmap cache files ---------------------------------------------

    public static func saveJpeg(_ buf: FloatImageBuffer, to path: String, quality: Double = 0.88) {
        ensureDir(path)
        ImageIOCodec.write(buf, to: path, format: .jpeg(quality: quality))
    }

    public static func savePng(_ buf: FloatImageBuffer, to path: String) {
        ensureDir(path)
        ImageIOCodec.write(buf, to: path, format: .png)
    }

    public static func load(_ path: String) -> FloatImageBuffer? {
        guard FileManager.default.fileExists(atPath: path) else { return nil }
        return ImageIOCodec.loadFloat(path: path)
    }

    static func ensureDir(_ path: String) {
        let dir = (path as NSString).deletingLastPathComponent
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    }

    // ---- 16-bit proxy (.f16) ---------------------------------------------
    // LibRaw's high-precision output is 16-bit integer to begin with, so storing UInt16
    // loses nothing and halves the file against float32 (2560×1707 ≈ 35 MB). The older
    // .f32 files actually held 8-bit-quantised values; the different extension lets them
    // lapse without a version check.

    static let halfMagic: UInt32 = 0x36315041   // "AP16"

    public static func saveHalf(_ buf: FloatImageBuffer, to path: String) {
        ensureDir(path)
        var out = Data()
        out.reserveCapacity(12 + buf.count * 2)
        var magic = halfMagic.littleEndian
        var w = Int32(buf.width).littleEndian
        var h = Int32(buf.height).littleEndian
        withUnsafeBytes(of: &magic) { out.append(contentsOf: $0) }
        withUnsafeBytes(of: &w) { out.append(contentsOf: $0) }
        withUnsafeBytes(of: &h) { out.append(contentsOf: $0) }

        var samples = [UInt16](repeating: 0, count: buf.count)
        let d = buf.data
        samples.withUnsafeMutableBufferPointer { sp in
            let s = sp.baseAddress!
            parallelRows(0, buf.height) { y in
                var i = y * buf.width * 4
                for _ in 0..<(buf.width * 4) {
                    let v = d[i]
                    s[i] = v <= 0 ? 0 : (v >= 1 ? 65535 : UInt16(v * 65535 + 0.5))
                    i += 1
                }
            }
        }
        samples.withUnsafeBufferPointer { sp in
            out.append(UnsafeBufferPointer(start: UnsafeRawPointer(sp.baseAddress!)
                        .assumingMemoryBound(to: UInt8.self), count: sp.count * 2))
        }
        try? out.write(to: URL(fileURLWithPath: path), options: .atomic)
    }

    public static func loadHalf(_ path: String) -> FloatImageBuffer? {
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: path)),
              data.count >= 12 else { return nil }
        return data.withUnsafeBytes { raw -> FloatImageBuffer? in
            let base = raw.baseAddress!
            let magic = base.loadUnaligned(fromByteOffset: 0, as: UInt32.self)
            guard UInt32(littleEndian: magic) == halfMagic else { return nil }
            let w = Int(Int32(littleEndian: base.loadUnaligned(fromByteOffset: 4, as: Int32.self)))
            let h = Int(Int32(littleEndian: base.loadUnaligned(fromByteOffset: 8, as: Int32.self)))
            guard w > 0, h > 0, w * h <= 64 * 1024 * 1024 else { return nil }
            let count = w * h * 4
            guard data.count >= 12 + count * 2 else { return nil }

            let buf = FloatImageBuffer(width: w, height: h, zeroed: false)
            let d = buf.data
            let inv: Float = 1.0 / 65535.0
            let src = base + 12
            parallelRows(0, h) { y in
                var i = y * w * 4
                for _ in 0..<(w * 4) {
                    let v = src.loadUnaligned(fromByteOffset: i * 2, as: UInt16.self)
                    d[i] = Float(UInt16(littleEndian: v)) * inv
                    i += 1
                }
            }
            return buf
        }
    }

    // ---- float resize ----------------------------------------------------

    /// Downscale a float buffer so its long edge is ≤ `maxDim`, by separable area
    /// averaging — staying in float the whole way. Returns the input itself when no
    /// scaling is needed. Only ever downscales.
    public static func resizeFloatToMaxDim(_ src: FloatImageBuffer, maxDim: Int) -> FloatImageBuffer {
        let longSide = max(src.width, src.height)
        if longSide <= maxDim { return src }
        let scale = Double(maxDim) / Double(longSide)
        let dw = max(1, Int((Double(src.width) * scale).rounded()))
        let dh = max(1, Int((Double(src.height) * scale).rounded()))
        return resizeFloat(src, dw, dh)
    }

    /// Separable area-average resize to an exact size.
    public static func resizeFloat(_ src: FloatImageBuffer, _ dw: Int, _ dh: Int) -> FloatImageBuffer {
        let sw = src.width, sh = src.height
        if dw == sw && dh == sh { return src.clone() }

        // horizontal pass: src (sw×sh) → tmp (dw×sh)
        let tmp = UnsafeMutablePointer<Float>.allocate(capacity: dw * sh * 4)
        tmp.initialize(repeating: 0, count: dw * sh * 4)
        defer { tmp.deinitialize(count: dw * sh * 4); tmp.deallocate() }
        let sd = src.data

        parallelRows(0, sh) { y in
            let srow = y * sw * 4, trow = y * dw * 4
            for x in 0..<dw {
                let x0 = Double(x) * Double(sw) / Double(dw)
                let x1 = Double(x + 1) * Double(sw) / Double(dw)
                var r: Float = 0, g: Float = 0, b: Float = 0, wsum: Float = 0
                var sx = Int(x0)
                let end = min(sw, Int(x1.rounded(.up)))
                while sx < end {
                    let cover = Float(min(x1, Double(sx + 1)) - max(x0, Double(sx)))
                    if cover > 0 {
                        let si = srow + sx * 4
                        r += sd[si] * cover; g += sd[si + 1] * cover; b += sd[si + 2] * cover
                        wsum += cover
                    }
                    sx += 1
                }
                let ti = trow + x * 4
                if wsum > 0 { tmp[ti] = r / wsum; tmp[ti + 1] = g / wsum; tmp[ti + 2] = b / wsum }
                tmp[ti + 3] = 1
            }
        }

        // vertical pass: tmp (dw×sh) → dst (dw×dh)
        let dst = FloatImageBuffer(width: dw, height: dh, zeroed: false)
        let dd = dst.data
        parallelRows(0, dh) { y in
            let y0 = Double(y) * Double(sh) / Double(dh)
            let y1 = Double(y + 1) * Double(sh) / Double(dh)
            let drow = y * dw * 4
            for x in 0..<dw {
                var r: Float = 0, g: Float = 0, b: Float = 0, wsum: Float = 0
                var sy = Int(y0)
                let end = min(sh, Int(y1.rounded(.up)))
                while sy < end {
                    let cover = Float(min(y1, Double(sy + 1)) - max(y0, Double(sy)))
                    if cover > 0 {
                        let ti = (sy * dw + x) * 4
                        r += tmp[ti] * cover; g += tmp[ti + 1] * cover; b += tmp[ti + 2] * cover
                        wsum += cover
                    }
                    sy += 1
                }
                let di = drow + x * 4
                if wsum > 0 { dd[di] = r / wsum; dd[di + 1] = g / wsum; dd[di + 2] = b / wsum }
                dd[di + 3] = 1
            }
        }
        return dst
    }

    /// Resize preserving aspect ratio so the result fits within maxW×maxH (never upsizes).
    public static func resizeToFit(_ src: FloatImageBuffer, maxW: Int, maxH: Int) -> FloatImageBuffer {
        var scale = min(Double(maxW) / Double(src.width), Double(maxH) / Double(src.height))
        if scale >= 1.0 { scale = 1.0 }
        let w = max(1, Int((Double(src.width) * scale).rounded()))
        let h = max(1, Int((Double(src.height) * scale).rounded()))
        return resizeFloat(src, w, h)
    }

    /// Resize preserving aspect ratio so the longest side equals maxDim (never upsizes).
    public static func resizeToMaxDim(_ src: FloatImageBuffer, maxDim: Int) -> FloatImageBuffer {
        let longSide = max(src.width, src.height)
        if longSide <= maxDim { return src.clone() }
        return resizeFloatToMaxDim(src, maxDim: maxDim)
    }

    /// Remove every cache artefact belonging to one photo.
    public static func deleteCacheFiles(_ imagePath: String) {
        for f in AppPaths.cacheFiles(imagePath) {
            try? FileManager.default.removeItem(atPath: f)
        }
    }
}
