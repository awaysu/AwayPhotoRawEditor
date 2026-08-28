import Foundation

/// Central knowledge of supported formats, the RAW_TEMP cache folder, cache-file naming
/// (thumb / proxy / adjustment XML incl. virtual copies) and application data locations.
public enum AppPaths {
    public static let rawTempFolderName = "RAW_TEMP"
    public static let previewListFileName = "preview_list.xml"

    /// Camera RAW formats.
    public static let rawExtensions: Set<String> = [
        "arw", "sr2", "srf", "cr2", "cr3", "crw", "nef", "nrw",
        "raf", "rw2", "orf", "pef", "dng"
    ]

    /// Regular bitmap formats. HEIC is added on macOS, where ImageIO reads it natively
    /// and photos coming off an iPhone are the obvious case the Windows build could not
    /// cover with WIC.
    public static let regularExtensions: Set<String> = [
        "jpg", "jpeg", "png", "tif", "tiff", "bmp", "heic", "heif"
    ]

    public static let supportedExtensions: Set<String> =
        rawExtensions.union(regularExtensions)

    public static func ext(_ path: String) -> String {
        (path as NSString).pathExtension.lowercased()
    }

    public static func isRaw(_ path: String) -> Bool { rawExtensions.contains(ext(path)) }
    public static func isSupported(_ path: String) -> Bool { supportedExtensions.contains(ext(path)) }

    /// True when the path is (or lives inside) a RAW_TEMP cache folder.
    public static func isRawTemp(_ path: String) -> Bool {
        let name = ((path as NSString).lastPathComponent)
        if name.caseInsensitiveCompare(rawTempFolderName) == .orderedSame { return true }
        return path.range(of: "/" + rawTempFolderName + "/", options: .caseInsensitive) != nil
    }

    // ---- RAW_TEMP cache locations ---------------------------------------

    public static func rawTempDir(_ imageFolder: String) -> String {
        (imageFolder as NSString).appendingPathComponent(rawTempFolderName)
    }

    @discardableResult
    public static func ensureRawTempDir(_ imageFolder: String) -> String {
        let dir = rawTempDir(imageFolder)
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        return dir
    }

    static func cacheDir(for imagePath: String) -> String {
        let full = (imagePath as NSString).standardizingPath
        return rawTempDir((full as NSString).deletingLastPathComponent)
    }

    /// RAW_TEMP/{file}_thumb.jpg
    public static func thumbnailPath(_ imagePath: String) -> String {
        (cacheDir(for: imagePath) as NSString)
            .appendingPathComponent((imagePath as NSString).lastPathComponent + "_thumb.jpg")
    }

    /// RAW_TEMP/{file}.rawpipe.png
    public static func proxyPath(_ imagePath: String) -> String {
        (cacheDir(for: imagePath) as NSString)
            .appendingPathComponent((imagePath as NSString).lastPathComponent + ".rawpipe.png")
    }

    /// RAW_TEMP/{file}.rawpipe.xml for the original, or
    /// RAW_TEMP/{file}.copyN.rawpipe.xml for a virtual copy (index >= 1).
    public static func adjustmentXmlPath(_ imagePath: String, virtualCopyIndex: Int = 0) -> String {
        let name = (imagePath as NSString).lastPathComponent
        let suffix = virtualCopyIndex <= 0 ? ".rawpipe.xml" : ".copy\(virtualCopyIndex).rawpipe.xml"
        return (cacheDir(for: imagePath) as NSString).appendingPathComponent(name + suffix)
    }

    public static func previewListPath(_ imageFolder: String) -> String {
        (rawTempDir(imageFolder) as NSString).appendingPathComponent(previewListFileName)
    }

    /// Every cache artefact belonging to one photo — used when a photo is removed or its
    /// cache invalidated.
    public static func cacheFiles(_ imagePath: String) -> [String] {
        [thumbnailPath(imagePath), proxyPath(imagePath),
         proxyPath(imagePath) + ".f16", proxyPath(imagePath) + ".f32",
         proxyPath(imagePath) + ".src"]
    }

    // ---- Application data / settings ------------------------------------

    /// ~/Library/Application Support/AwayPhotoRawEditor — the macOS equivalent of the
    /// Windows build's %AppData%\AwayPhotoRawEditor. Settings and presets are
    /// deliberately kept in the same XML formats so they can be copied across.
    public static var appDataDir: String {
        let root = FileManager.default.urls(for: .applicationSupportDirectory,
                                            in: .userDomainMask).first!
        let dir = root.appendingPathComponent("AwayPhotoRawEditor").path
        if !FileManager.default.fileExists(atPath: dir) {
            try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        }
        return dir
    }

    public static var settingsPath: String {
        (appDataDir as NSString).appendingPathComponent("settings.xml")
    }
    public static var presetsPath: String {
        (appDataDir as NSString).appendingPathComponent("presets.xml")
    }
    public static var exportSettingsPath: String {
        (appDataDir as NSString).appendingPathComponent("export.xml")
    }

    /// Where a diagnostic trace goes when AWPR_TRACE=1.
    public static var tracePath: String {
        (NSTemporaryDirectory() as NSString).appendingPathComponent("awpr_trace.txt")
    }

    // ---- folder scanning -------------------------------------------------

    /// Every supported image in a folder, sorted the way the strip shows them,
    /// skipping the RAW_TEMP cache directory.
    public static func imagesInFolder(_ folder: String) -> [String] {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: folder) else { return [] }
        return names
            .filter { !$0.hasPrefix(".") && isSupported($0) }
            .map { (folder as NSString).appendingPathComponent($0) }
            .filter { !isRawTemp($0) }
            .sorted { $0.localizedStandardCompare($1) == .orderedAscending }
    }
}
