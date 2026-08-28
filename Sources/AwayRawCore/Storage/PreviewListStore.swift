import Foundation

public struct VirtualCopyEntry: Equatable, Sendable {
    public var path: String = ""
    public var index: Int = 0
    public init(path: String, index: Int) { self.path = path; self.index = index }
}

/// Contents of RAW_TEMP/preview_list.xml.
public struct PreviewList: Sendable {
    /// Item keys hidden from the preview (PhotoItem.key values).
    public var hidden: [String] = []
    /// Virtual copies (original path + copy index) to recreate on load.
    public var virtualCopies: [VirtualCopyEntry] = []
    public init() {}
}

/// Loads / saves the per-folder preview list (hidden items + virtual copies).
public enum PreviewListStore {

    public static func load(imageFolder: String) -> PreviewList {
        var list = PreviewList()
        guard let root = DotNetXml.parse(
                contentsOf: URL(fileURLWithPath: AppPaths.previewListPath(imageFolder)))
        else { return list }
        if let h = root.child("Hidden") {
            list.hidden = h.childrenNamed("string").compactMap { $0.text }
        }
        if let v = root.child("VirtualCopies") {
            list.virtualCopies = v.childrenNamed("VirtualCopyEntry").compactMap { n in
                guard let p = n.string("Path"), !p.isEmpty else { return nil }
                return VirtualCopyEntry(path: p, index: n.int("Index", default: 0))
            }
        }
        return list
    }

    public static func save(imageFolder: String, list: PreviewList) {
        let root = XmlNode("PreviewList")
        let h = root.add(XmlNode("Hidden"))
        for k in list.hidden { h.add("string", k) }
        let v = root.add(XmlNode("VirtualCopies"))
        for e in list.virtualCopies {
            let n = v.add(XmlNode("VirtualCopyEntry"))
            n.add("Path", e.path)
            n.add("Index", e.index)
        }
        let path = AppPaths.previewListPath(imageFolder)
        let url = URL(fileURLWithPath: path)
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(),
                                                 withIntermediateDirectories: true)
        try? root.documentData().write(to: url, options: .atomic)   // non-fatal
    }
}
