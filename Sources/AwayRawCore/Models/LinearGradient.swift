import Foundation

/// A single linear (graduated) filter. Multiple of these stack on one photo
/// (see `ImageAdjustments.gradients`). Geometry is in normalized image coordinates:
/// `centerX`/`centerY` is the white handle (position), `range` the yellow band
/// (falloff, in image-height units) and `angle` the blue rotate handle (degrees).
public struct LinearGradient: Equatable, Sendable {
    public var centerX: Double = 0.5
    public var centerY: Double = 0.15      // new gradients start near the top
    public var angle: Double = 0           // degrees
    public var range: Double = 0.25

    public var exposure: Double = 0        // -5 .. +5
    public var contrast: Double = 0        // -100 .. +100
    public var highlights: Double = 0      // -100 .. +100
    public var shadows: Double = 0         // -100 .. +100
    public var saturation: Double = 0      // -100 .. +100

    public init() {}

    /// True when this gradient actually changes any pixels.
    public var hasEffect: Bool {
        exposure != 0 || contrast != 0 || highlights != 0 || shadows != 0 || saturation != 0
    }
}
