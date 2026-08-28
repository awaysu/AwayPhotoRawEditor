import Foundation
import CoreGraphics
import ImageIO

/// Result of a thumbnail load with the extra information the status line shows.
public struct ThumbnailInfo {
    public var buffer: FloatImageBuffer?
    public var width = 0
    public var height = 0
    public var fromCache = false
    public var usedLibRaw = false
}

/// Central image acquisition + cache orchestration. Decodes RAW via LibRaw (falling back
/// to the embedded preview, then ImageIO), decodes regular formats via ImageIO, and
/// manages the RAW_TEMP thumbnail / proxy caches.
public final class RawLoader: @unchecked Sendable {

    public var useLibRaw: Bool
    public var useHighPrecisionRawPipeline: Bool

    public var libRawAvailable: Bool { LibRawBridge.available }

    /// Best-effort record of whether the last full decode actually used LibRaw.
    public private(set) var lastFullDecodeUsedLibRaw = false

    public static let defaultProxyMaxDim = 2560

    public init(settings: AppSettings = .current) {
        useLibRaw = settings.useLibRaw
        useHighPrecisionRawPipeline = settings.useHighPrecisionRawPipeline
    }

    public func syncFromSettings(_ settings: AppSettings = .current) {
        useLibRaw = settings.useLibRaw
        useHighPrecisionRawPipeline = settings.useHighPrecisionRawPipeline
    }

    /// Attach LibRaw's colour data to `exif` for a RAW file (metadata-only open, cheap).
    /// No-op for non-RAW, when LibRaw is off/unavailable, or when it is already there —
    /// XMLs written before v1.0.15 cache EXIF without it, so this back-fills them on the
    /// next load. Returns true when data was added (the caller should persist).
    public func enrichCameraColor(path: String, exif: inout ExifData?) -> Bool {
        guard var e = exif else { return false }
        if let c = e.camera, c.isValid { return false }
        guard AppPaths.isRaw(path), useLibRaw, LibRawBridge.available else { return false }
        guard let cam = LibRawBridge.readCameraColor(path) else { return false }
        e.camera = cam
        exif = e
        return true
    }

    // ---- Full-resolution decode (no adjustments) ------------------------

    /// Full-resolution, orientation-corrected buffer. Nil only on total failure.
    public func decodeFull(path: String) -> FloatImageBuffer? {
        if AppPaths.isRaw(path) {
            if useLibRaw, LibRawBridge.available {
                // Hand LibRaw the visible size from metadata: on models it has no crop
                // table for it emits the mask border as black edges.
                let vis = ExifReader.readVisibleSize(path: path)
                let bps = useHighPrecisionRawPipeline ? 16 : 8
                if let b = LibRawBridge.decodeFull(path, bps: bps, expectedVisible: vis) {
                    lastFullDecodeUsedLibRaw = true
                    return b
                }
            }
            lastFullDecodeUsedLibRaw = false
            // Fallback: the camera's embedded preview.
            if let preview = ExifReader.extractPreview(path: path),
               let b = ImageIOCodec.loadFloat(data: preview) {
                return b
            }
            // Last resort: let ImageIO decode the RAW itself.
            return ImageIOCodec.loadFloat(path: path)
        }
        lastFullDecodeUsedLibRaw = false
        return ImageIOCodec.loadFloat(path: path)
    }

    /// Full-resolution high-precision decode (for export / high-precision preview).
    public func decodeFullFloat(path: String) -> FloatImageBuffer? {
        if AppPaths.isRaw(path), useLibRaw, useHighPrecisionRawPipeline, LibRawBridge.available {
            let vis = ExifReader.readVisibleSize(path: path)
            if let f = LibRawBridge.decodeFull(path, bps: 16, expectedVisible: vis) {
                lastFullDecodeUsedLibRaw = true
                return f
            }
        }
        return decodeFull(path: path)
    }

    // ---- Thumbnails ------------------------------------------------------

    public func loadThumbnail(path: String, maxW: Int, maxH: Int) -> FloatImageBuffer? {
        loadThumbnailWithInfo(path: path, maxW: maxW, maxH: maxH,
                              useCache: true, materialize: true).buffer
    }

    public func loadThumbnailWithInfo(path: String, maxW: Int, maxH: Int,
                                      useCache: Bool, materialize: Bool) -> ThumbnailInfo {
        var info = ThumbnailInfo()

        // 1) cache hit
        if useCache {
            let cachePath = AppPaths.thumbnailPath(path)
            if FileManager.default.fileExists(atPath: cachePath) {
                if !materialize {
                    info.fromCache = true
                    return info
                }
                if let cached = CacheManager.load(cachePath) {
                    info.buffer = cached
                    info.width = cached.width; info.height = cached.height
                    info.fromCache = true
                    return info
                }
            }
        }

        // 2) generate
        var full: FloatImageBuffer?
        if AppPaths.isRaw(path), useLibRaw, LibRawBridge.available,
           let t = LibRawBridge.decodeThumbnail(path) {
            switch t {
            case .jpeg(let data, let flip):
                // The preview JPEG may carry its own orientation tag; ImageIO honours it.
                // Only when LibRaw reports a flip and the JPEG had no tag does the camera's
                // flip need applying, so double rotation is avoided by checking the tag.
                if var b = ImageIOCodec.loadFloat(data: data) {
                    if flip != 0, orientationOfJPEG(data) <= 1 {
                        b = applyFlip(b, flip)
                    }
                    full = b
                    info.usedLibRaw = true
                }
            case .pixels(let b, let flip):
                full = flip != 0 ? applyFlip(b, flip) : b
                info.usedLibRaw = true
            }
        }
        if full == nil { full = decodeFull(path: path) }
        guard let source = full else { return info }

        let thumb = CacheManager.resizeToFit(source, maxW: maxW, maxH: maxH)
        if useCache {
            CacheManager.saveJpeg(thumb, to: AppPaths.thumbnailPath(path))
        }
        if !materialize { return info }
        info.buffer = thumb
        info.width = thumb.width; info.height = thumb.height
        return info
    }

    @discardableResult
    public func ensureThumbnailCache(path: String, maxW: Int, maxH: Int) -> Bool {
        let cachePath = AppPaths.thumbnailPath(path)
        if FileManager.default.fileExists(atPath: cachePath) { return true }
        let info = loadThumbnailWithInfo(path: path, maxW: maxW, maxH: maxH,
                                        useCache: true, materialize: false)
        return FileManager.default.fileExists(atPath: cachePath) || info.fromCache
    }

    public func loadThumbnailCache(path: String) -> FloatImageBuffer? {
        let cachePath = AppPaths.thumbnailPath(path)
        guard FileManager.default.fileExists(atPath: cachePath) else { return nil }
        return CacheManager.load(cachePath)
    }

    /// LibRaw's `sizes.flip` (3 = 180°, 5 = ccw 90°, 6 = cw 90°) as a rotation of the
    /// decoded preview.
    func applyFlip(_ buf: FloatImageBuffer, _ flip: Int) -> FloatImageBuffer {
        switch flip {
        case 3: return ImageProcessor.rotateDiscrete(buf, .r180)
        case 5: return ImageProcessor.rotateDiscrete(buf, .r270)
        case 6: return ImageProcessor.rotateDiscrete(buf, .r90)
        default: return buf
        }
    }

    func orientationOfJPEG(_ data: Data) -> Int {
        guard let src = CGImageSourceCreateWithData(data as CFData, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any],
              let o = props[kCGImagePropertyOrientation] as? Int else { return 1 }
        return o
    }

    // ---- Proxy -----------------------------------------------------------

    // .f16 (16-bit integer) replaces the older .f32: those files actually held 8-bit
    // quantised values, so the different extension lets them lapse on their own.
    static func proxyFloatPath(_ path: String) -> String { AppPaths.proxyPath(path) + ".f16" }

    @discardableResult
    public func ensureProxyCache(path: String, maxDim: Int = RawLoader.defaultProxyMaxDim) -> Bool {
        let proxyPath = AppPaths.proxyPath(path)
        let needPng = !FileManager.default.fileExists(atPath: proxyPath)
        let needFloat = useHighPrecisionRawPipeline &&
                        !FileManager.default.fileExists(atPath: Self.proxyFloatPath(path))
        if !needPng && !needFloat { return true }

        if useHighPrecisionRawPipeline {
            guard let full = decodeFullFloat(path: path) else { return false }
            let scaled = CacheManager.resizeFloatToMaxDim(full, maxDim: maxDim)
            CacheManager.savePng(scaled, to: proxyPath)
            CacheManager.saveHalf(scaled, to: Self.proxyFloatPath(path))
            return true
        } else {
            guard let full = decodeFull(path: path) else { return false }
            let scaled = CacheManager.resizeToMaxDim(full, maxDim: maxDim)
            CacheManager.savePng(scaled, to: proxyPath)
            return true
        }
    }

    /// Load the 8-bit proxy (generating it if necessary).
    public func loadProxy(path: String, maxDim: Int = RawLoader.defaultProxyMaxDim) -> FloatImageBuffer? {
        ensureProxyCache(path: path, maxDim: maxDim)
        let proxyPath = AppPaths.proxyPath(path)
        if FileManager.default.fileExists(atPath: proxyPath),
           let b = CacheManager.load(proxyPath) { return b }
        return decodeFull(path: path)
    }

    /// Load the high-precision float proxy (falling back to the 8-bit proxy).
    public func loadProxyFloat(path: String, maxDim: Int = RawLoader.defaultProxyMaxDim) -> FloatImageBuffer? {
        ensureProxyCache(path: path, maxDim: maxDim)
        if useHighPrecisionRawPipeline,
           let f = CacheManager.loadHalf(Self.proxyFloatPath(path)) { return f }
        return loadProxy(path: path, maxDim: maxDim)
    }
}
