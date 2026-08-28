import Foundation

public struct NamedPreset {
    public var name: String = ""
    public var adjustments = ImageAdjustments()
    public init(name: String, adjustments: ImageAdjustments) {
        self.name = name; self.adjustments = adjustments
    }
}

/// Stores user-defined presets (自訂1..3, any saved custom looks, and overrides of the
/// built-ins) in presets.xml — same format as the Windows build, so a presets.xml can be
/// carried across. Applying only copies tonal/colour/detail fields onto the target,
/// matching PresetProfile semantics.
public enum PresetStore {

    public static func load() -> [NamedPreset] {
        guard let root = DotNetXml.parse(contentsOf: URL(fileURLWithPath: AppPaths.presetsPath)),
              let items = root.child("Items") else { return [] }
        return items.childrenNamed("NamedPreset").compactMap { n in
            guard let name = n.string("Name"), !name.isEmpty else { return nil }
            let adj = n.child("Adjustments").map { AdjustmentXmlStore.decodeAdjustments($0) }
                      ?? ImageAdjustments()
            return NamedPreset(name: name, adjustments: adj)
        }
    }

    public static func saveAll(_ presets: [NamedPreset]) {
        let root = XmlNode("Presets")
        let items = root.add(XmlNode("Items"))
        for p in presets {
            let n = items.add(XmlNode("NamedPreset"))
            n.add("Name", p.name)
            let a = AdjustmentXmlStore.encode(p.adjustments)
            a.name = "Adjustments"
            n.add(a)
        }
        try? root.documentData().write(to: URL(fileURLWithPath: AppPaths.presetsPath), options: .atomic)
    }

    /// Save (or overwrite) a named preset from a full adjustment set.
    public static func save(name: String, source: ImageAdjustments) {
        var col = load()
        col.removeAll { $0.name == name }
        col.append(NamedPreset(name: name, adjustments: source))
        saveAll(col)
    }

    /// Remove a preset's custom override so it reverts to its built-in default.
    public static func remove(name: String) {
        var col = load()
        let before = col.count
        col.removeAll { $0.name == name }
        if col.count != before { saveAll(col) }
    }

    /// True when the named preset has a user-saved override.
    public static func hasOverride(name: String) -> Bool {
        load().contains { $0.name == name }
    }

    /// Names in presets.xml that are not built-ins — the custom presets added in the
    /// preset editor.
    public static func customNames() -> [String] {
        load().map(\.name).filter { PresetProfile.builtIn[$0] == nil }
    }

    /// Every preset name the picker should show: the built-ins in their fixed order,
    /// then any custom ones.
    public static func allNames() -> [String] {
        PresetProfile.builtInNames + customNames()
    }

    /// Restore defaults: drop every custom preset and every override of a built-in.
    public static func resetAllToDefaults() { saveAll([]) }

    /// Back up the whole preset configuration (i.e. presets.xml) to `path`.
    public static func exportTo(path: String) throws {
        let root = XmlNode("Presets")
        let items = root.add(XmlNode("Items"))
        for p in load() {
            let n = items.add(XmlNode("NamedPreset"))
            n.add("Name", p.name)
            let a = AdjustmentXmlStore.encode(p.adjustments)
            a.name = "Adjustments"
            n.add(a)
        }
        try root.documentData().write(to: URL(fileURLWithPath: path), options: .atomic)
    }

    /// Restore: read a backup and replace presets.xml wholesale.
    /// Returns false when the file parses but is not a preset collection.
    @discardableResult
    public static func importFrom(path: String) throws -> Bool {
        guard let root = DotNetXml.parse(contentsOf: URL(fileURLWithPath: path)),
              root.name == "Presets" else { return false }
        let items = root.child("Items")?.childrenNamed("NamedPreset") ?? []
        let col = items.compactMap { n -> NamedPreset? in
            guard let name = n.string("Name"), !name.isEmpty else { return nil }
            let adj = n.child("Adjustments").map { AdjustmentXmlStore.decodeAdjustments($0) }
                      ?? ImageAdjustments()
            return NamedPreset(name: name, adjustments: adj)
        }
        saveAll(col)
        return true
    }

    public static func get(name: String) -> ImageAdjustments? {
        load().first { $0.name == name }?.adjustments
    }

    /// Apply a stored preset's tonal fields onto `target`. True if found.
    ///
    /// Temperature and Tint are deliberately excluded: a preset never touches white
    /// balance (2026-08-23). Kelvin values left over in an old presets.xml are ignored.
    @discardableResult
    public static func applyCustom(name: String, to target: inout ImageAdjustments) -> Bool {
        guard let p = get(name: name) else { return false }
        target.resetTonal()
        target.exposure = p.exposure; target.contrast = p.contrast; target.highlights = p.highlights
        target.shadows = p.shadows; target.whites = p.whites; target.blacks = p.blacks
        target.vibrance = p.vibrance; target.saturation = p.saturation
        target.sharpening = p.sharpening; target.noiseReduction = p.noiseReduction
        target.vignette = p.vignette; target.distortion = p.distortion
        return true
    }

    /// Apply any preset by name: a user override wins over the built-in of the same name.
    @discardableResult
    public static func apply(name: String, to target: inout ImageAdjustments) -> Bool {
        // "預設時設定" is a full reset and is never overridable.
        if name == PresetProfile.defaultName {
            PresetProfile.builtIn[name]?.applyTo(&target)
            return true
        }
        if hasOverride(name: name) { return applyCustom(name: name, to: &target) }
        guard let p = PresetProfile.get(name) else { return false }
        p.applyTo(&target)
        return true
    }

    /// The values a preset currently resolves to (override if any, else built-in).
    public static func resolvedValues(name: String) -> ImageAdjustments? {
        if let o = get(name: name) { return o }
        return PresetProfile.builtInValues(name)
    }
}
