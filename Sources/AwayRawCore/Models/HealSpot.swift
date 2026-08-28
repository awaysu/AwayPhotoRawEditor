import Foundation

/// A single heal / clone point. Positions are stored in normalized image
/// coordinates (0..1) so they survive proxy vs full-resolution scaling.
public struct HealSpot: Equatable, Sendable {
    /// Where the fix is painted (normalized 0..1).
    public var targetX: Double = 0
    public var targetY: Double = 0

    /// Where the source pixels are sampled from (normalized 0..1). Clone mode.
    public var sourceX: Double = 0
    public var sourceY: Double = 0

    /// Radius in pixels at the resolution it was authored on (informational).
    public var radius: Double = 10

    /// Radius as a fraction of the image's larger dimension (resolution independent).
    public var radiusNorm: Double = 0.02

    /// True = inpaint from surrounding pixels; false = clone from source.
    public var useInpaint: Bool = false

    public init() {}
}
