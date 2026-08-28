import Foundation

/// The complete non-destructive edit description for one photo (or virtual copy).
/// Serialized to RAW_TEMP/{file}.rawpipe.xml. All ranges match the UI sliders.
public struct ImageAdjustments: Equatable, Sendable {

    /// Newest rendering maths. 0 = legacy (WB/exposure multiplied on gamma-encoded
    /// values, black-body Kelvin); 1 = linear light + camera-matrix white balance.
    public static let currentPipelineVersion = 1

    /// Which maths renders this photo. Lives on the XML document (RawPipeDocument), not
    /// in the serialized adjustment fields — and it is deliberately excluded from
    /// `valueEquals` / `copyFrom` / `applyDelta` so a legacy photo with untouched
    /// sliders never counts as "edited" and a reset never silently upgrades it.
    /// Old XMLs without the element load as 0 and keep rendering exactly as before
    /// until the user upgrades them; fresh instances are current.
    public var pipelineVersion: Int = ImageAdjustments.currentPipelineVersion

    public var isLegacyPipeline: Bool { pipelineVersion < 1 }

    // ---- Basic (基本調整) ------------------------------------------------
    public var exposure: Double = 0      // v1: -5 .. +5 EV (true stops); legacy slider ±2
    public var contrast: Double = 0      // -100 .. +100
    public var highlights: Double = 0    // -100 .. +100
    public var shadows: Double = 0       // -100 .. +100
    public var whites: Double = 0        // -100 .. +100
    public var blacks: Double = 0        // -100 .. +100

    // ---- Colour (色彩) ---------------------------------------------------
    public var temperature: Double = 5200  // 2000 .. 12000 K
    public var tint: Double = 0            // -100 .. +100
    public var vibrance: Double = 0        // -100 .. +100
    public var saturation: Double = 0      // -100 .. +100

    // ---- Detail (細節) ---------------------------------------------------
    public var sharpening: Double = 0      // -100 .. +100 (negative = soften)
    public var noiseReduction: Double = 0  // 0 .. 100
    public var vignette: Double = 0        // -100 .. +100 (positive = darken corners), post-crop

    // ---- Lens / geometry -------------------------------------------------
    public var distortion: Double = 0      // -100 .. +100 (wide-angle correction)

    // ---- Crop ------------------------------------------------------------
    public var cropAspectRatio: String = "Original"  // Original / 3:2 / 4:3 / 16:9 / 1:1 / W:H
    public var cropAngle: Double = 0       // -90 .. +90 (fine straighten)
    public var cropX: Double = 0.0         // normalized
    public var cropY: Double = 0.0
    public var cropWidth: Double = 1.0
    public var cropHeight: Double = 1.0
    public var rotation: Rotation = .r0

    // ---- Gradient (漸層) — a stack of linear gradients, added on demand -----
    public var gradients: [LinearGradient] = []

    /// UI selection: which gradient the sliders / handles currently edit.
    /// Runtime only (never persisted, never compared).
    public var activeGradientIndex: Int = -1

    /// The currently-selected gradient, or nil when there are none.
    /// Reading it clamps a stale index to the last gradient.
    public var activeGradient: LinearGradient? {
        get {
            guard !gradients.isEmpty else { return nil }
            let i = (activeGradientIndex < 0 || activeGradientIndex >= gradients.count)
                  ? gradients.count - 1 : activeGradientIndex
            return gradients[i]
        }
        set {
            guard let newValue, !gradients.isEmpty else { return }
            let i = (activeGradientIndex < 0 || activeGradientIndex >= gradients.count)
                  ? gradients.count - 1 : activeGradientIndex
            gradients[i] = newValue
        }
    }

    /// Index the active-gradient accessors actually resolve to (-1 when empty).
    public var resolvedGradientIndex: Int {
        guard !gradients.isEmpty else { return -1 }
        return (activeGradientIndex < 0 || activeGradientIndex >= gradients.count)
             ? gradients.count - 1 : activeGradientIndex
    }

    // ---- Heal (修護) -----------------------------------------------------
    public var healSize: Double = 10       // brush size 0 .. 50
    public var healSpots: [HealSpot] = []

    public init() {}

    // ---------------------------------------------------------------------

    /// Reset absolutely everything to defaults (used by "預設時設定").
    /// Keeps the pipeline version — a reset must not silently upgrade a legacy photo.
    public mutating func resetAll() {
        let version = pipelineVersion
        self = ImageAdjustments()
        pipelineVersion = version
    }

    /// Reset only tonal / colour / detail fields (basic + colour + detail + distortion),
    /// leaving crop, rotation, gradient, heal and watermark intact. Used when applying a
    /// look preset so a preset doesn't blow away geometry or local edits.
    ///
    /// Temperature/Tint are deliberately untouched: presets never touch white balance
    /// (2026-08-23 decision — a preset holding an absolute Kelvin value wrecks indoor
    /// and overcast shots now that photos are seeded to as-shot).
    public mutating func resetTonal() {
        let d = ImageAdjustments()
        exposure = d.exposure; contrast = d.contrast; highlights = d.highlights
        shadows = d.shadows; whites = d.whites; blacks = d.blacks
        vibrance = d.vibrance; saturation = d.saturation
        sharpening = d.sharpening; noiseReduction = d.noiseReduction
        vignette = d.vignette; distortion = d.distortion
    }

    /// Reset only 基本調整 + 色彩 + 細節 slider values, leaving geometry
    /// (crop/distortion/rotation), gradient, heal and watermark untouched.
    public mutating func resetBasicColorDetail() {
        let d = ImageAdjustments()
        exposure = d.exposure; contrast = d.contrast; highlights = d.highlights
        shadows = d.shadows; whites = d.whites; blacks = d.blacks
        temperature = d.temperature; tint = d.tint
        vibrance = d.vibrance; saturation = d.saturation
        sharpening = d.sharpening; noiseReduction = d.noiseReduction; vignette = d.vignette
    }

    /// Copy into self every scalar field whose value differs between `edited` and
    /// `baseline` — i.e. the fields the user actually changed during a multi-selection
    /// edit. Fields the user did not touch (and healSpots/gradients, which are
    /// position-specific) are left alone, so each photo keeps its own settings apart
    /// from the synced change.
    public mutating func applyDelta(edited: ImageAdjustments, baseline: ImageAdjustments) {
        if edited.exposure       != baseline.exposure       { exposure = edited.exposure }
        if edited.contrast       != baseline.contrast       { contrast = edited.contrast }
        if edited.highlights     != baseline.highlights     { highlights = edited.highlights }
        if edited.shadows        != baseline.shadows        { shadows = edited.shadows }
        if edited.whites         != baseline.whites         { whites = edited.whites }
        if edited.blacks         != baseline.blacks         { blacks = edited.blacks }
        if edited.temperature    != baseline.temperature    { temperature = edited.temperature }
        if edited.tint           != baseline.tint           { tint = edited.tint }
        if edited.vibrance       != baseline.vibrance       { vibrance = edited.vibrance }
        if edited.saturation     != baseline.saturation     { saturation = edited.saturation }
        if edited.sharpening     != baseline.sharpening     { sharpening = edited.sharpening }
        if edited.noiseReduction != baseline.noiseReduction { noiseReduction = edited.noiseReduction }
        if edited.vignette       != baseline.vignette       { vignette = edited.vignette }
        if edited.distortion     != baseline.distortion     { distortion = edited.distortion }
        if edited.cropAspectRatio != baseline.cropAspectRatio { cropAspectRatio = edited.cropAspectRatio }
        if edited.cropAngle      != baseline.cropAngle      { cropAngle = edited.cropAngle }
        if edited.cropX          != baseline.cropX          { cropX = edited.cropX }
        if edited.cropY          != baseline.cropY          { cropY = edited.cropY }
        if edited.cropWidth      != baseline.cropWidth      { cropWidth = edited.cropWidth }
        if edited.cropHeight     != baseline.cropHeight     { cropHeight = edited.cropHeight }
        if edited.rotation       != baseline.rotation       { rotation = edited.rotation }
        if edited.healSize       != baseline.healSize       { healSize = edited.healSize }
    }

    /// Value equality across all persisted fields (for dirty / edited detection).
    /// Runtime-only state (pipelineVersion, activeGradientIndex) is excluded.
    public func valueEquals(_ o: ImageAdjustments) -> Bool {
        exposure == o.exposure && contrast == o.contrast && highlights == o.highlights &&
        shadows == o.shadows && whites == o.whites && blacks == o.blacks &&
        temperature == o.temperature && tint == o.tint &&
        vibrance == o.vibrance && saturation == o.saturation &&
        sharpening == o.sharpening && noiseReduction == o.noiseReduction &&
        vignette == o.vignette && distortion == o.distortion &&
        cropAspectRatio == o.cropAspectRatio && cropAngle == o.cropAngle &&
        cropX == o.cropX && cropY == o.cropY &&
        cropWidth == o.cropWidth && cropHeight == o.cropHeight &&
        rotation == o.rotation && healSize == o.healSize &&
        healSpots == o.healSpots && gradients == o.gradients
    }

    public static func == (a: ImageAdjustments, b: ImageAdjustments) -> Bool { a.valueEquals(b) }

    /// True when equal to a freshly-constructed default (a "no edits" state).
    public var isDefault: Bool { valueEquals(ImageAdjustments()) }

    /// True when at least one gradient actually changes pixels.
    public var hasActiveGradient: Bool { gradients.contains { $0.hasEffect } }
}
