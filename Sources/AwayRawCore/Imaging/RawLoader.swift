import Foundation
import CoreGraphics
import ImageIO

/// How a photo's pixels were produced. This decides what white balance is already
/// baked into them, which the linear pipeline's matrix has to undo.
public enum DecodeSource: String, Sendable {
    /// LibRaw's own decode: balanced with `pre_mul` (daylight).
    case libRaw = "libraw"
    /// ImageIO or the camera's embedded preview: the as-shot `cam_mul` is baked in.
    case imageIO = "imageio"

    public var whiteBalanceReference: WhiteBalanceReference {
        self == .libRaw ? .decode : .asShot
    }
}

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

    /// How the last full decode produced its pixels.
    public private(set) var lastDecodeSource: DecodeSource = .libRaw

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
        decodeFullWithSource(path: path)?.buffer
    }

    /// As `decodeFull`, but the decode source rides along with the pixels. The folder
    /// cache runs two decodes at once, so a "last decode" instance property would race —
    /// callers that need the source (the proxy marker, export) take it from here.
    public func decodeFullWithSource(path: String) -> (buffer: FloatImageBuffer, source: DecodeSource)? {
        if AppPaths.isRaw(path) {
            if useLibRaw, LibRawBridge.available {
                // Hand LibRaw the visible size from metadata: on models it has no crop
                // table for it emits the mask border as black edges.
                let vis = ExifReader.readVisibleSize(path: path)
                let bps = useHighPrecisionRawPipeline ? 16 : 8
                if let b = LibRawBridge.decodeFull(path, bps: bps, expectedVisible: vis) {
                    lastFullDecodeUsedLibRaw = true
                    lastDecodeSource = .libRaw
                    return (b, .libRaw)
                }
            }
            lastFullDecodeUsedLibRaw = false
            lastDecodeSource = .imageIO
            // ImageIO next, at full resolution. It decodes formats LibRaw cannot — a
            // Nikon Z 8's High Efficiency NEF is recognised by LibRaw 0.22
            // (`nikon_he_load_raw`) but not decodable by it, while ImageIO handles it
            // natively. Trying this before the embedded preview is what keeps such files
            // at full resolution instead of dropping to a downsized preview.
            if let b = ImageIOCodec.loadFloat(path: path) { return (b, .imageIO) }
            // Last resort: the camera's embedded preview (reduced resolution).
            if let preview = ExifReader.extractPreview(path: path),
               let b = ImageIOCodec.loadFloat(data: preview) {
                return (b, .imageIO)
            }
            return nil
        }
        lastFullDecodeUsedLibRaw = false
        lastDecodeSource = .imageIO
        guard let b = ImageIOCodec.loadFloat(path: path) else { return nil }
        return (b, .imageIO)
    }

    /// Full-resolution high-precision decode (for export / high-precision preview).
    public func decodeFullFloat(path: String) -> FloatImageBuffer? {
        decodeFullFloatWithSource(path: path)?.buffer
    }

    public func decodeFullFloatWithSource(path: String) -> (buffer: FloatImageBuffer, source: DecodeSource)? {
        if AppPaths.isRaw(path), useLibRaw, useHighPrecisionRawPipeline, LibRawBridge.available {
            let vis = ExifReader.readVisibleSize(path: path)
            if let f = LibRawBridge.decodeFull(path, bps: 16, expectedVisible: vis) {
                lastFullDecodeUsedLibRaw = true
                lastDecodeSource = .libRaw
                return (f, .libRaw)
            }
        }
        return decodeFullWithSource(path: path)
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
        return Self.withPathLock(cachePath) {
            if FileManager.default.fileExists(atPath: cachePath) { return true }
            let info = loadThumbnailWithInfo(path: path, maxW: maxW, maxH: maxH,
                                            useCache: true, materialize: false)
            return FileManager.default.fileExists(atPath: cachePath) || info.fromCache
        }
    }

    public func loadThumbnailCache(path: String) -> FloatImageBuffer? {
        let cachePath = AppPaths.thumbnailPath(path)
        guard FileManager.default.fileExists(atPath: cachePath) else { return nil }
        return CacheManager.load(cachePath)
    }

    /// Strip thumbnail size (points; the strip cell is 240×160).
    public static let thumbnailMaxW = 240
    public static let thumbnailMaxH = 160

    /// The base image a strip thumbnail is rendered from.
    ///
    /// For a RAW file the first thumbnail is the camera's embedded preview — instant, but
    /// it is the *camera's* rendering (its tone curve, picture style, white balance), not
    /// what the editor shows, so with edits applied the two drift apart visibly
    /// (2026-09-01: "縮圖和實際編輯的結果不一樣"). As soon as the proxy exists a thumbnail
    /// is cut from it instead (`AppPaths.proxyThumbnailPath`), so the strip and the editor
    /// start from the same pixels; `proxySource` then says what those pixels are balanced
    /// to, exactly as `proxyDecodeSource` does for the editor. `proxySource == nil` means
    /// the camera preview (or a non-RAW file, whose `_thumb.jpg` already matches the proxy).
    public struct ThumbnailBase {
        public let buffer: FloatImageBuffer
        public let proxySource: DecodeSource?
    }

    public func loadThumbnailBase(path: String) -> ThumbnailBase? {
        if AppPaths.isRaw(path) {
            let p = AppPaths.proxyThumbnailPath(path)
            if FileManager.default.fileExists(atPath: p), let b = CacheManager.load(p) {
                return ThumbnailBase(buffer: b, proxySource: proxyDecodeSource(path: path))
            }
        }
        guard let b = loadThumbnailCache(path: path) else { return nil }
        return ThumbnailBase(buffer: b, proxySource: nil)
    }

    /// Written alongside a freshly generated RAW proxy, from the same scaled pixels.
    private func writeProxyThumbnail(path: String, _ proxy: FloatImageBuffer) {
        guard AppPaths.isRaw(path) else { return }
        let thumb = CacheManager.resizeToFit(proxy, maxW: Self.thumbnailMaxW, maxH: Self.thumbnailMaxH)
        CacheManager.saveJpeg(thumb, to: AppPaths.proxyThumbnailPath(path))
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

    /// A one-word marker recording how the cached proxy was decoded. The renderer needs
    /// it because a proxy that came from ImageIO already has the camera's white balance
    /// baked in, while a LibRaw one is balanced to pre_mul — feeding the matrix the wrong
    /// reference tints the whole photo. Kept as a sidecar rather than a new element in
    /// the shared XML, since it is per-platform cache metadata, not an edit.
    static func proxySourcePath(_ path: String) -> String { AppPaths.proxyPath(path) + ".src" }

    /// What the cached proxy's pixels are already balanced to.
    public func proxyDecodeSource(path: String) -> DecodeSource {
        guard AppPaths.isRaw(path) else { return .imageIO }
        guard let s = try? String(contentsOfFile: Self.proxySourcePath(path), encoding: .utf8),
              let src = DecodeSource(rawValue: s.trimmingCharacters(in: .whitespacesAndNewlines))
        else {
            // No marker (an older cache): assume LibRaw, which is what it was before this
            // marker existed and remains the common case.
            return .libRaw
        }
        return src
    }

    private func writeProxySource(path: String, _ source: DecodeSource) {
        try? source.rawValue.write(toFile: Self.proxySourcePath(path),
                                   atomically: true, encoding: .utf8)
    }

    /// One generator per cache file at a time. The folder-open worker and a click on
    /// that same photo used to decode and write the proxy concurrently; with atomic
    /// writes that is merely wasteful (a second 60 MP decode), but it also means the
    /// click waits for the file instead of racing it.
    private static var pathLocks: [String: NSLock] = [:]
    private static let pathLocksGuard = NSLock()

    static func withPathLock<T>(_ key: String, _ body: () -> T) -> T {
        pathLocksGuard.lock()
        let lock = pathLocks[key] ?? { let l = NSLock(); pathLocks[key] = l; return l }()
        pathLocksGuard.unlock()
        lock.lock(); defer { lock.unlock() }
        return body()
    }

    @discardableResult
    public func ensureProxyCache(path: String, maxDim: Int = RawLoader.defaultProxyMaxDim) -> Bool {
        Self.withPathLock(AppPaths.proxyPath(path)) { ensureProxyCacheLocked(path: path, maxDim: maxDim) }
    }

    private func ensureProxyCacheLocked(path: String, maxDim: Int) -> Bool {
        let proxyPath = AppPaths.proxyPath(path)
        let needPng = !FileManager.default.fileExists(atPath: proxyPath)
        let needFloat = useHighPrecisionRawPipeline &&
                        !FileManager.default.fileExists(atPath: Self.proxyFloatPath(path))
        if !needPng && !needFloat {
            // A proxy from before the proxy-cut thumbnail existed (or a thumbnail that was
            // lost): cut it from the cached proxy now, so an existing folder catches up on
            // open instead of needing its caches deleted (which would take the edits too).
            if AppPaths.isRaw(path),
               !FileManager.default.fileExists(atPath: AppPaths.proxyThumbnailPath(path)),
               let cached = CacheManager.load(proxyPath) {
                writeProxyThumbnail(path: path, cached)
            }
            return true
        }

        if useHighPrecisionRawPipeline {
            guard let (full, source) = decodeFullFloatWithSource(path: path) else { return false }
            let scaled = CacheManager.resizeFloatToMaxDim(full, maxDim: maxDim)
            CacheManager.savePng(scaled, to: proxyPath)
            CacheManager.saveHalf(scaled, to: Self.proxyFloatPath(path))
            writeProxySource(path: path, source)
            writeProxyThumbnail(path: path, scaled)
            return true
        } else {
            guard let (full, source) = decodeFullWithSource(path: path) else { return false }
            let scaled = CacheManager.resizeToMaxDim(full, maxDim: maxDim)
            CacheManager.savePng(scaled, to: proxyPath)
            writeProxySource(path: path, source)
            writeProxyThumbnail(path: path, scaled)
            return true
        }
    }

    /// Load the 8-bit proxy (generating it if necessary).
    public func loadProxy(path: String, maxDim: Int = RawLoader.defaultProxyMaxDim) -> FloatImageBuffer? {
        ensureProxyCache(path: path, maxDim: maxDim)
        let proxyPath = AppPaths.proxyPath(path)
        if let b = CacheManager.load(proxyPath) { return b }
        // `load` discards a half-written file; build it again rather than falling back
        // to a full-resolution decode on every selection from now on.
        ensureProxyCache(path: path, maxDim: maxDim)
        if let b = CacheManager.load(proxyPath) { return b }
        return decodeFull(path: path)
    }

    /// Load the high-precision float proxy (falling back to the 8-bit proxy).
    public func loadProxyFloat(path: String, maxDim: Int = RawLoader.defaultProxyMaxDim) -> FloatImageBuffer? {
        ensureProxyCache(path: path, maxDim: maxDim)
        if useHighPrecisionRawPipeline {
            let fp = Self.proxyFloatPath(path)
            if let f = CacheManager.loadHalf(fp) { return f }
            if FileManager.default.fileExists(atPath: fp) {
                // Present but unreadable (short file, bad magic): regenerate once.
                try? FileManager.default.removeItem(atPath: fp)
                ensureProxyCache(path: path, maxDim: maxDim)
                if let f = CacheManager.loadHalf(fp) { return f }
            }
        }
        return loadProxy(path: path, maxDim: maxDim)
    }
}
