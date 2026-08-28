import Foundation

/// A look preset ("風格檔"). Applying a preset resets the tonal/colour/detail group and
/// then applies the preset's values, leaving crop / gradient / heal / watermark
/// untouched. "預設時設定" performs a full reset instead.
public struct PresetProfile: Sendable {
    public let name: String
    public let fullReset: Bool
    private let apply: (@Sendable (inout ImageAdjustments) -> Void)?

    public init(name: String, fullReset: Bool = false,
                apply: (@Sendable (inout ImageAdjustments) -> Void)? = nil) {
        self.name = name
        self.fullReset = fullReset
        self.apply = apply
    }

    public func applyTo(_ a: inout ImageAdjustments) {
        if fullReset { a.resetAll(); return }
        a.resetTonal()
        apply?(&a)
    }

    // ---- Built-in registry (order = dropdown order) ---------------------

    /// Storage keys stay in Traditional Chinese across every UI language — only the
    /// display layer is translated.
    public static let defaultName = "預設時設定"

    public static let builtInNames: [String] = [
        defaultName, "風景", "人像", "鮮豔", "黑白", "柔和", "自訂1", "自訂2", "自訂3"
    ]

    public static let builtIn: [String: PresetProfile] = [
        defaultName: PresetProfile(name: defaultName, fullReset: true),

        "風景": PresetProfile(name: "風景") { a in
            a.contrast = 18; a.highlights = -20; a.shadows = 12; a.whites = 8; a.blacks = -10
            a.vibrance = 28; a.saturation = 8
            a.sharpening = 25; a.noiseReduction = 5
        },

        "人像": PresetProfile(name: "人像") { a in
            // Exposure values are authored for the v1 (true-EV) pipeline; the same number
            // on a legacy photo lands about 2.2× stronger, which is accepted.
            a.exposure = 0.2; a.contrast = -6; a.highlights = -12; a.shadows = 18
            a.vibrance = 10; a.saturation = -4
            a.sharpening = 5; a.noiseReduction = 12
        },

        "鮮豔": PresetProfile(name: "鮮豔") { a in
            a.contrast = 14; a.highlights = -10; a.whites = 8; a.blacks = -8
            a.vibrance = 38; a.saturation = 16; a.sharpening = 18
        },

        "黑白": PresetProfile(name: "黑白") { a in
            a.contrast = 22; a.highlights = -15; a.shadows = 8; a.whites = 10; a.blacks = -16
            a.saturation = -100; a.sharpening = 20
        },

        "柔和": PresetProfile(name: "柔和") { a in
            a.exposure = 0.3; a.contrast = -14; a.highlights = -18; a.shadows = 24; a.blacks = 6
            a.vibrance = 6; a.saturation = -6
            a.sharpening = -10; a.noiseReduction = 15
        },

        // 自訂1..3 are user-defined; overridden from PresetStore.
        "自訂1": PresetProfile(name: "自訂1"),
        "自訂2": PresetProfile(name: "自訂2"),
        "自訂3": PresetProfile(name: "自訂3"),
    ]

    public static func get(_ name: String) -> PresetProfile? { builtIn[name] }

    /// The adjustment values a built-in preset produces, starting from defaults —
    /// used by the preset editor to show what a built-in contains.
    public static func builtInValues(_ name: String) -> ImageAdjustments? {
        guard let p = builtIn[name] else { return nil }
        var a = ImageAdjustments()
        p.applyTo(&a)
        return a
    }
}
