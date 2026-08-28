import Foundation

public enum ExportLocation: String, Sendable, CaseIterable {
    case desktop = "Desktop"
    case sameAsSource = "SameAsSource"
    case custom = "Custom"
}

public enum ExportFormat: String, Sendable, CaseIterable {
    case jpeg = "Jpeg", bmp = "Bmp", tiff = "Tiff", png = "Png"
}

/// File-naming rule: keep the original name / date-time / a running number.
public enum RenameMode: String, Sendable, CaseIterable {
    case original = "Original", dateTime = "DateTime", sequence = "Sequence"
}

/// What to do when the output name already exists.
public enum ConflictMode: String, Sendable, CaseIterable {
    case appendNumber = "AppendNumber", overwrite = "Overwrite"
}

/// Persistent export configuration (export.xml under the app data folder).
public struct ExportSettings: Sendable {
    public var location: ExportLocation = .desktop
    public var customPath: String = ""

    public var useSubFolder: Bool = true
    public var subFolder: String = "NEW_IMAGE"

    public var rename: RenameMode = .original
    public var conflict: ConflictMode = .appendNumber

    public var format: ExportFormat = .jpeg
    /// Output long-edge limit (px): the longer of width/height is scaled to this,
    /// preserving aspect. Default 2400.
    public var maxLongEdge: Int = 2400
    /// Output resolution (pixels per inch). Default 300.
    public var resolution: Int = 300
    public var jpegQuality: Int = 100
    public var preserveExif: Bool = true
    public var openFinderAfter: Bool = true

    // ---- Watermark (標誌) — a global overlay, applied on export and (when enabled)
    //      to the live preview.
    public var watermarkEnabled: Bool = false
    public var watermarkText: String = "Watermark"
    public var watermarkFontName: String = "Helvetica"
    public var watermarkFontSize: Double = 150     // 6 .. 300
    public var watermarkTransparency: Int = 20     // 0 .. 100
    public var watermarkColor: WatermarkColor = .white
    public var watermarkPosition: WatermarkPosition = .bottomRight
    public var watermarkMargin: Int = 30           // 0 .. 9999 px

    public init() {}

    /// Snapshot the watermark fields for the render pipeline.
    public func buildWatermark() -> WatermarkSpec {
        var w = WatermarkSpec()
        w.enabled = watermarkEnabled
        w.text = watermarkText
        w.fontName = watermarkFontName
        w.fontSize = watermarkFontSize
        w.transparency = watermarkTransparency
        w.color = watermarkColor
        w.position = watermarkPosition
        w.margin = watermarkMargin
        return w
    }

    public var fileExtension: String {
        switch format {
        case .bmp: return ".bmp"
        case .tiff: return ".tif"
        case .png: return ".png"
        case .jpeg: return ".jpg"
        }
    }

    public var outputFormat: ImageIOCodec.OutputFormat {
        switch format {
        case .jpeg: return .jpeg(quality: Double(min(max(jpegQuality, 50), 100)) / 100.0)
        case .png:  return .png
        case .tiff: return .tiff
        case .bmp:  return .bmp
        }
    }

    // ---- persistence -----------------------------------------------------

    public static func load() -> ExportSettings {
        var s = ExportSettings()
        guard let root = DotNetXml.parse(contentsOf: URL(fileURLWithPath: AppPaths.exportSettingsPath))
        else { return s }
        if let v = root.string("Location"), let l = ExportLocation(rawValue: v) { s.location = l }
        s.customPath = root.string("CustomPath", default: "")
        s.useSubFolder = root.bool("UseSubFolder", default: s.useSubFolder)
        s.subFolder = root.string("SubFolder", default: s.subFolder)
        if let v = root.string("Rename"), let r = RenameMode(rawValue: v) { s.rename = r }
        if let v = root.string("Conflict"), let c = ConflictMode(rawValue: v) { s.conflict = c }
        if let v = root.string("Format"), let f = ExportFormat(rawValue: v) { s.format = f }
        s.maxLongEdge = root.int("MaxLongEdge", default: s.maxLongEdge)
        s.resolution = root.int("Resolution", default: s.resolution)
        s.jpegQuality = root.int("JpegQuality", default: s.jpegQuality)
        s.preserveExif = root.bool("PreserveExif", default: s.preserveExif)
        s.openFinderAfter = root.bool("OpenExplorerAfter", default: s.openFinderAfter)
        s.watermarkEnabled = root.bool("WatermarkEnabled", default: s.watermarkEnabled)
        s.watermarkText = root.string("WatermarkText", default: s.watermarkText)
        s.watermarkFontName = root.string("WatermarkFontName", default: s.watermarkFontName)
        s.watermarkFontSize = root.double("WatermarkFontSize", default: s.watermarkFontSize)
        s.watermarkTransparency = root.int("WatermarkTransparency", default: s.watermarkTransparency)
        if let v = root.string("WatermarkColor"), let c = WatermarkColor(xmlName: v) { s.watermarkColor = c }
        if let v = root.string("WatermarkPosition"), let p = WatermarkPosition(xmlName: v) { s.watermarkPosition = p }
        s.watermarkMargin = root.int("WatermarkMargin", default: s.watermarkMargin)
        return s
    }

    public func save() {
        let root = XmlNode("ExportSettings")
        root.add("Location", location.rawValue)
        root.add("CustomPath", customPath)
        root.add("UseSubFolder", useSubFolder)
        root.add("SubFolder", subFolder)
        root.add("Rename", rename.rawValue)
        root.add("Conflict", conflict.rawValue)
        root.add("Format", format.rawValue)
        root.add("MaxLongEdge", maxLongEdge)
        root.add("Resolution", resolution)
        root.add("JpegQuality", jpegQuality)
        root.add("PreserveExif", preserveExif)
        // Kept under the Windows element name so one export.xml works on both platforms.
        root.add("OpenExplorerAfter", openFinderAfter)
        root.add("WatermarkEnabled", watermarkEnabled)
        root.add("WatermarkText", watermarkText)
        root.add("WatermarkFontName", watermarkFontName)
        root.add("WatermarkFontSize", watermarkFontSize)
        root.add("WatermarkTransparency", watermarkTransparency)
        root.add("WatermarkColor", watermarkColor.xmlName)
        root.add("WatermarkPosition", watermarkPosition.xmlName)
        root.add("WatermarkMargin", watermarkMargin)
        try? root.documentData().write(to: URL(fileURLWithPath: AppPaths.exportSettingsPath),
                                       options: .atomic)
    }

    /// Resolve the destination directory (creating the sub-folder if requested).
    public func resolveOutputDir(sourceFilePath: String) -> String {
        var baseDir: String
        switch location {
        case .desktop:
            baseDir = FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask)
                        .first?.path ?? NSHomeDirectory()
        case .sameAsSource:
            baseDir = (sourceFilePath as NSString).deletingLastPathComponent
        case .custom:
            baseDir = customPath.trimmingCharacters(in: .whitespaces).isEmpty
                ? (FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask)
                     .first?.path ?? NSHomeDirectory())
                : customPath
        }
        if useSubFolder, !subFolder.trimmingCharacters(in: .whitespaces).isEmpty {
            baseDir = (baseDir as NSString).appendingPathComponent(subFolder)
        }
        try? FileManager.default.createDirectory(atPath: baseDir, withIntermediateDirectories: true)
        return baseDir
    }
}
