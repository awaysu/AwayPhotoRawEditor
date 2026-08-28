import Foundation

/// One entry in the preview list / thumbnail strip. A virtual copy shares the
/// original file on disk and is distinguished only by `virtualCopyIndex`; its
/// identity string is "path|copy:N".
public final class PhotoItem: @unchecked Sendable {
    public let sourcePath: String

    /// 0 = the original; 1,2,3… = virtual copies.
    public let virtualCopyIndex: Int

    public init(sourcePath: String, virtualCopyIndex: Int = 0) {
        self.sourcePath = sourcePath
        self.virtualCopyIndex = virtualCopyIndex
    }

    /// Stable identity used as a dictionary key and in preview_list.xml.
    public var key: String {
        virtualCopyIndex <= 0 ? sourcePath : "\(sourcePath)|copy:\(virtualCopyIndex)"
    }

    public var isVirtualCopy: Bool { virtualCopyIndex > 0 }

    public var fileName: String { (sourcePath as NSString).lastPathComponent }

    public var displayName: String {
        virtualCopyIndex <= 0 ? fileName : "\(fileName)  (copy \(virtualCopyIndex))"
    }

    // ---- Transient UI state (not persisted here) ------------------------

    /// Photo has non-default adjustments (shows the "edited" badge).
    public var isEdited: Bool = false

    /// #編號: the position in the folder's full list *including* hidden photos, so a
    /// hidden #2 still holds its number and the strip shows #1, #3.
    /// 0 = unassigned (the strip falls back to index + 1).
    public var displayNumber: Int = 0

    /// Hidden from the preview (persisted in preview_list.xml).
    public var isHidden: Bool = false

    /// This item is the current "copy settings" source (shows a marker).
    public var isCopySettingsSource: Bool = false

    // ---- Key parsing ----------------------------------------------------

    public static func parseKey(_ key: String) -> (path: String, copyIndex: Int) {
        guard let r = key.range(of: "|copy:", options: .backwards) else { return (key, 0) }
        let path = String(key[key.startIndex..<r.lowerBound])
        let n = Int(key[r.upperBound...]) ?? 0
        return (path, n)
    }

    public static func fromKey(_ key: String) -> PhotoItem {
        let (path, n) = parseKey(key)
        return PhotoItem(sourcePath: path, virtualCopyIndex: n)
    }
}

extension PhotoItem: Equatable, Hashable {
    public static func == (a: PhotoItem, b: PhotoItem) -> Bool { a.key == b.key }
    public func hash(into hasher: inout Hasher) { hasher.combine(key) }
}
