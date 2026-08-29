import Foundation
import CoreGraphics

public struct ExportProgress: Sendable {
    public var done: Int
    public var total: Int
    public var message: String
}

public enum ExportError: LocalizedError {
    case decodeFailed(String)
    case writeFailed(String)
    case itemFailed(String, String)

    public var errorDescription: String? {
        switch self {
        case .decodeFailed(let f): return L.t("無法解碼影像") + "：" + f
        case .writeFailed(let f): return L.t("無法開啟：") + f
        case .itemFailed(let f, let m): return L.f("匯出「{0}」失敗：{1}", f, m)
        }
    }
}

/// Full-resolution non-destructive export pipeline.
public enum Exporter {

    /// Export the given photos; returns the list of written file paths.
    public static func export(items: [PhotoItem], settings: ExportSettings, loader: RawLoader,
                              progress: ((ExportProgress) -> Void)? = nil,
                              token: CancelToken = .none) throws -> [String] {
        var written: [String] = []
        let total = items.count
        var done = 0
        var seq = 0                              // running number for "數字開始"
        var dtCounts: [String: Int] = [:]        // per-second count for "日期時間"

        for item in items {
            try token.check()
            progress?(ExportProgress(done: done, total: total, message: item.displayName))
            do {
                let baseName = buildBaseName(item, settings, &seq, &dtCounts)
                let path = try exportOne(item, settings, loader, baseName)
                written.append(path)
            } catch let e {
                throw ExportError.itemFailed(item.fileName, e.localizedDescription)
            }
            done += 1
            progress?(ExportProgress(done: done, total: total, message: item.displayName))
        }

        if settings.openFinderAfter, let first = written.first {
            revealInFinder(first)
        }
        return written
    }

    /// The output filename (without extension) for the chosen rename rule.
    static func buildBaseName(_ item: PhotoItem, _ s: ExportSettings,
                              _ seq: inout Int, _ dtCounts: inout [String: Int]) -> String {
        switch s.rename {
        case .dateTime:
            let dt = captureTime(item)
            let fmt = DateFormatter()
            fmt.locale = Locale(identifier: "en_US_POSIX")
            fmt.dateFormat = "yyMMddHHmmss"
            let ts = fmt.string(from: dt)
            let c = (dtCounts[ts] ?? 0) + 1
            dtCounts[ts] = c
            return String(format: "IMG%@%02d", ts, c)   // last two digits separate same-second shots
        case .sequence:
            seq += 1
            return String(format: "IMG%05d", seq)
        case .original:
            let stem = ((item.sourcePath as NSString).lastPathComponent as NSString)
                        .deletingPathExtension
            return item.isVirtualCopy
                ? "\(stem)_copy\(item.virtualCopyIndex)_edited"
                : "\(stem)_edited"
        }
    }

    /// Capture time: the cached EXIF DateTaken, falling back to the file's modification date.
    static func captureTime(_ item: PhotoItem) -> Date {
        if let exif = AdjustmentXmlStore.loadExif(imagePath: item.sourcePath,
                                                  copyIndex: item.virtualCopyIndex),
           !exif.dateTaken.trimmingCharacters(in: .whitespaces).isEmpty {
            let fmt = DateFormatter()
            fmt.locale = Locale(identifier: "en_US_POSIX")
            fmt.dateFormat = "yyyy:MM:dd HH:mm:ss"
            if let d = fmt.date(from: exif.dateTaken) { return d }
            fmt.dateFormat = "yyyy-MM-dd HH:mm:ss"
            if let d = fmt.date(from: exif.dateTaken) { return d }
        }
        if let attrs = try? FileManager.default.attributesOfItem(atPath: item.sourcePath),
           let d = attrs[.modificationDate] as? Date { return d }
        return Date(timeIntervalSince1970: 0)
    }

    static func exportOne(_ item: PhotoItem, _ s: ExportSettings, _ loader: RawLoader,
                          _ baseName: String) throws -> String {
        // 1) full-resolution decode — stays float the whole way
        guard let (full, source) = decodeFull(item, loader) else {
            throw ExportError.decodeFailed(item.fileName)
        }
        // Whatever the decode actually used decides what white balance is baked in.
        let wbReference = source.whiteBalanceReference

        // 2) adjustments + cached EXIF (which carries the camera colour data for the WB matrix)
        var (adjOpt, exif, _) = AdjustmentXmlStore.loadAll(imagePath: item.sourcePath,
                                                           copyIndex: item.virtualCopyIndex)
        let adj = adjOpt ?? ImageAdjustments()
        if loader.enrichCameraColor(path: item.sourcePath, exif: &exif) {
            // An older XML without camera colour data: fill it in and store it back so the
            // file does not have to be reopened next time.
            AdjustmentXmlStore.save(imagePath: item.sourcePath, adjustments: adj,
                                    copyIndex: item.virtualCopyIndex, exif: exif)
        }

        // 3) the complete pipeline at full resolution (watermark authored at full res)
        let ctx = ProcessContext()
        ctx.forExport = true
        ctx.watermarkScale = 1.0
        ctx.watermark = s.buildWatermark()
        ctx.camera = exif?.camera
        ctx.whiteBalanceReference = wbReference
        let processed = try ImageProcessor.applyToFloat(full, adj, ctx)

        // 4) resize so the longest edge equals the target, preserving aspect (never upscales)
        let resized = resizeToLongEdge(processed, s.maxLongEdge)

        // 5) watermark. It is authored at full resolution (the Windows build draws it
        //    before the resize), so when drawn onto the resized frame its size and margin
        //    scale down by the same factor — otherwise a 2400 px export of a 66 MP photo
        //    would carry a watermark four times too large.
        guard var img = ImageIOCodec.toCGImage(resized) else {
            throw ExportError.writeFailed(item.fileName)
        }
        ctx.watermarkScale = Double(max(resized.width, resized.height))
                           / Double(max(processed.width, processed.height))
        img = Watermark.apply(img, ctx)

        // 6) filename + same-name handling
        let dir = s.resolveOutputDir(sourceFilePath: item.sourcePath)
        let outPath = s.conflict == .overwrite
            ? (dir as NSString).appendingPathComponent(baseName + s.fileExtension)
            : uniquePath(dir: dir, name: baseName, ext: s.fileExtension)

        // 7) write, carrying source metadata when asked (BMP cannot hold EXIF)
        let copyFrom = (s.preserveExif && s.format != .bmp) ? item.sourcePath : nil
        guard ImageIOCodec.write(img, to: outPath, format: s.outputFormat,
                                 dpi: s.resolution > 0 ? s.resolution : nil,
                                 metadataFrom: copyFrom) else {
            throw ExportError.writeFailed(item.fileName)
        }
        return outPath
    }

    static func decodeFull(_ item: PhotoItem, _ loader: RawLoader)
        -> (buffer: FloatImageBuffer, source: DecodeSource)? {
        if loader.useHighPrecisionRawPipeline,
           let r = loader.decodeFullFloatWithSource(path: item.sourcePath) { return r }
        return loader.decodeFullWithSource(path: item.sourcePath)
    }

    /// Scale so the longest edge equals `maxLongEdge`, preserving aspect. Never upscales.
    static func resizeToLongEdge(_ src: FloatImageBuffer, _ maxLongEdge: Int) -> FloatImageBuffer {
        guard maxLongEdge > 0 else { return src }
        let longest = max(src.width, src.height)
        guard longest > maxLongEdge else { return src }
        return CacheManager.resizeFloatToMaxDim(src, maxDim: maxLongEdge)
    }

    static func uniquePath(dir: String, name: String, ext: String) -> String {
        var p = (dir as NSString).appendingPathComponent(name + ext)
        var n = 1
        while FileManager.default.fileExists(atPath: p) {
            p = (dir as NSString).appendingPathComponent("\(name)_\(n)\(ext)")
            n += 1
        }
        return p
    }

    static func revealInFinder(_ file: String) {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        p.arguments = ["-R", file]
        try? p.run()
    }
}
