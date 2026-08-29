import Foundation

/// Overall interface style, switchable from Settings.
public enum UiStyle: String, Sendable, CaseIterable {
    case classicDark = "ClassicDark"
    case warmPaper = "WarmPaper"
}

/// Supported application UI languages. New values are only ever appended — settings.xml
/// stores the name, but the existing order must not be rearranged.
public enum AppLanguage: String, Sendable, CaseIterable {
    case traditionalChinese = "TraditionalChinese"
    case english = "English"
    case japanese = "Japanese"
    case korean = "Korean"
    case simplifiedChinese = "SimplifiedChinese"
    case german = "German"
    case french = "French"
    case spanish = "Spanish"

    /// The best guess for a first run, from the system's preferred languages.
    /// Unrecognised locales fall back to English.
    public static func guessFromSystem() -> AppLanguage {
        for id in Locale.preferredLanguages {
            let l = id.lowercased()
            if l.hasPrefix("zh") {
                if l.contains("hant") || l.contains("tw") || l.contains("hk") || l.contains("mo") {
                    return .traditionalChinese
                }
                return .simplifiedChinese
            }
            if l.hasPrefix("ja") { return .japanese }
            if l.hasPrefix("ko") { return .korean }
            if l.hasPrefix("de") { return .german }
            if l.hasPrefix("fr") { return .french }
            if l.hasPrefix("es") { return .spanish }
            if l.hasPrefix("en") { return .english }
        }
        return .english
    }
}

/// Every UI font size, in points. macOS lays out in points and scales for Retina itself,
/// so unlike the Windows build there is no manual DPI factor to apply — but the sizes stay
/// user-adjustable because that was the point of the Windows "字體大小…" dialog.
///
/// New fields are only ever appended; a settings.xml missing a field keeps the default here.
public struct FontSizes: Equatable, Sendable {
    /// Small: thumbnail filename, #index / copy marker, EXIF field names, grey hints, progress %.
    public var small: Int = 11
    /// Monospaced: the R/G/B means under the histogram.
    public var mono: Int = 10
    /// Normal (the default): slider labels and values, EXIF values, popups, fields, buttons, tabs.
    public var normal: Int = 12
    /// Section title (bold): 基本調整 / 色彩 / 細節 / 直方圖 / 照片資訊 / 工具.
    public var sectionTitle: Int = 13
    /// About window body text.
    public var aboutBody: Int = 12
    /// The 📁 / 💽 glyphs in the folder list.
    public var folderGlyph: Int = 12
    /// Small icon buttons (the white-balance eyedropper and friends).
    public var iconGlyph: Int = 13
    /// Progress window title (bold).
    public var progressTitle: Int = 13
    /// Dialog title (bold): 匯出照片 / 設定.
    public var dialogTitle: Int = 14
    /// About window title (bold).
    public var aboutTitle: Int = 18
    /// The top-left "AwayPhotoRawEditor" logo (bold).
    public var logo: Int = 17
    /// The top-left ☰ menu glyph.
    public var menuGlyph: Int = 20

    /// Adjustable range. Too small to read, or wide enough to burst a fixed-width box.
    public static let minPt = 7
    public static let maxPt = 36

    public init() {}

    /// Clamp every field back into range, so a hand-edited settings.xml cannot make the
    /// interface unusable.
    public mutating func clamp() {
        func c(_ v: Int) -> Int { min(max(v, Self.minPt), Self.maxPt) }
        small = c(small); mono = c(mono); normal = c(normal); sectionTitle = c(sectionTitle)
        aboutBody = c(aboutBody); folderGlyph = c(folderGlyph); iconGlyph = c(iconGlyph)
        progressTitle = c(progressTitle); dialogTitle = c(dialogTitle); aboutTitle = c(aboutTitle)
        logo = c(logo); menuGlyph = c(menuGlyph)
    }

    /// Ordered (label key, keypath) pairs so the 字體大小 dialog can build itself.
    public static let fields: [(key: String, path: WritableKeyPath<FontSizes, Int>)] = [
        ("小字", \.small), ("等寬（直方圖數值）", \.mono), ("一般", \.normal),
        ("區塊標題", \.sectionTitle), ("關於內文", \.aboutBody), ("資料夾圖示", \.folderGlyph),
        ("圖示按鈕", \.iconGlyph), ("進度標題", \.progressTitle), ("對話框標題", \.dialogTitle),
        ("關於標題", \.aboutTitle), ("Logo", \.logo), ("選單圖示", \.menuGlyph),
    ]
}

/// Persistent application settings (settings.xml under
/// ~/Library/Application Support/AwayPhotoRawEditor).
public final class AppSettings: @unchecked Sendable {

    /// Use LibRaw for RAW decoding (falls back to ImageIO when false or unavailable).
    public var useLibRaw = true

    /// Use the float RGBA high-precision RAW pipeline (16-bit proxy cache).
    public var useHighPrecisionRawPipeline = false

    /// Render on the GPU. Reserved for the Metal phase; the CPU path is the only one
    /// implemented today, so this is stored but not yet acted on.
    public var useGpu = true

    /// Show the #1.. index number on each thumbnail (top-left).
    public var showThumbnailNumber = true

    /// Preview strip "show everything" mode: true = include hidden (non-exported) photos
    /// with a hidden badge; false = the default, hidden photos do not appear.
    public var showHiddenPhotos = false

    /// Overall interface style; a settings.xml without the field keeps classic dark.
    public var interfaceStyle: UiStyle = .classicDark

    /// UI language; a settings.xml without the field keeps Traditional Chinese.
    public var uiLanguage: AppLanguage = .traditionalChinese

    /// Interface size (%). 0 = automatic (follow the system). macOS scales for Retina on
    /// its own, so this only stretches the point sizes.
    public var uiScalePercent = 0

    /// Every UI font size, in points.
    public var fontSizes = FontSizes()

    /// Last used folder — restored at startup.
    public var lastFolder = ""

    /// Folders opened before, most recent first. Same element as the Windows build
    /// (`<RecentFolders><string>…</string></RecentFolders>`), so the list survives a copy.
    public var recentFolders: [String] = []

    /// Add a folder to the history: de-duplicated, most recent first, capped at 20.
    public func pushRecentFolder(_ folder: String) {
        let f = folder.trimmingCharacters(in: .whitespaces)
        guard !f.isEmpty else { return }
        recentFolders.removeAll { $0.caseInsensitiveCompare(f) == .orderedSame }
        recentFolders.insert(f, at: 0)
        if recentFolders.count > 20 { recentFolders.removeLast(recentFolders.count - 20) }
    }

    /// True when settings.xml did not exist at startup — drives the first-run language pick.
    public private(set) var isFirstRun = false

    public init() {}

    // ---- persistence -----------------------------------------------------

    nonisolated(unsafe) public static var current = AppSettings.load()

    public static func load() -> AppSettings {
        let s = AppSettings()
        guard let root = DotNetXml.parse(contentsOf: URL(fileURLWithPath: AppPaths.settingsPath)) else {
            s.isFirstRun = true
            return s
        }
        s.useLibRaw = root.bool("UseLibRaw", default: s.useLibRaw)
        s.useHighPrecisionRawPipeline = root.bool("UseHighPrecisionRawPipeline",
                                                  default: s.useHighPrecisionRawPipeline)
        s.useGpu = root.bool("UseGpu", default: s.useGpu)
        s.showThumbnailNumber = root.bool("ShowThumbnailNumber", default: s.showThumbnailNumber)
        s.showHiddenPhotos = root.bool("ShowHiddenPhotos", default: s.showHiddenPhotos)
        if let v = root.string("InterfaceStyle"), let st = UiStyle(rawValue: v) { s.interfaceStyle = st }
        if let v = root.string("UiLanguage"), let l = AppLanguage(rawValue: v) { s.uiLanguage = l }
        s.uiScalePercent = root.int("UiScalePercent", default: s.uiScalePercent)
        s.lastFolder = root.string("LastFolder", default: "")
        if let r = root.child("RecentFolders") {
            s.recentFolders = r.childrenNamed("string").compactMap { $0.text }.filter { !$0.isEmpty }
        }
        if let f = root.child("FontSizes") {
            var fs = FontSizes()
            fs.small = f.int("Small", default: fs.small)
            fs.mono = f.int("Mono", default: fs.mono)
            fs.normal = f.int("Normal", default: fs.normal)
            fs.sectionTitle = f.int("SectionTitle", default: fs.sectionTitle)
            fs.aboutBody = f.int("AboutBody", default: fs.aboutBody)
            fs.folderGlyph = f.int("FolderGlyph", default: fs.folderGlyph)
            fs.iconGlyph = f.int("IconGlyph", default: fs.iconGlyph)
            fs.progressTitle = f.int("ProgressTitle", default: fs.progressTitle)
            fs.dialogTitle = f.int("DialogTitle", default: fs.dialogTitle)
            fs.aboutTitle = f.int("AboutTitle", default: fs.aboutTitle)
            fs.logo = f.int("Logo", default: fs.logo)
            fs.menuGlyph = f.int("MenuGlyph", default: fs.menuGlyph)
            fs.clamp()
            s.fontSizes = fs
        }
        return s
    }

    public func save() {
        let root = XmlNode("AppSettings")
        root.add("UseLibRaw", useLibRaw)
        root.add("UseHighPrecisionRawPipeline", useHighPrecisionRawPipeline)
        root.add("UseGpu", useGpu)
        root.add("ShowThumbnailNumber", showThumbnailNumber)
        root.add("ShowHiddenPhotos", showHiddenPhotos)
        root.add("InterfaceStyle", interfaceStyle.rawValue)
        root.add("UiLanguage", uiLanguage.rawValue)
        root.add("UiScalePercent", uiScalePercent)
        let f = root.add(XmlNode("FontSizes"))
        f.add("Small", fontSizes.small)
        f.add("Mono", fontSizes.mono)
        f.add("Normal", fontSizes.normal)
        f.add("SectionTitle", fontSizes.sectionTitle)
        f.add("AboutBody", fontSizes.aboutBody)
        f.add("FolderGlyph", fontSizes.folderGlyph)
        f.add("IconGlyph", fontSizes.iconGlyph)
        f.add("ProgressTitle", fontSizes.progressTitle)
        f.add("DialogTitle", fontSizes.dialogTitle)
        f.add("AboutTitle", fontSizes.aboutTitle)
        f.add("Logo", fontSizes.logo)
        f.add("MenuGlyph", fontSizes.menuGlyph)
        root.add("LastFolder", lastFolder)
        let r = root.add(XmlNode("RecentFolders"))
        for f in recentFolders { r.add("string", f) }
        try? root.documentData().write(to: URL(fileURLWithPath: AppPaths.settingsPath), options: .atomic)
    }
}
