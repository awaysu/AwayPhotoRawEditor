import Foundation

/// Builds a 1-D tone response LUT (input 0..1 → output 0..1) composing Blacks/Whites
/// end points, Shadows/Highlights region shifts and Contrast. Applied per channel by
/// `ImageProcessor`.
public enum ToneCurve {
    public static let lutSize = 1024

    public static func buildLut(_ a: ImageAdjustments) -> [Float] {
        var lut = [Float](repeating: 0, count: lutSize)

        let blacks = a.blacks / 100.0      // -1..1
        let whites = a.whites / 100.0      // -1..1
        let contrast = a.contrast / 100.0  // -1..1
        let hi = a.highlights / 100.0      // -1..1
        let sh = a.shadows / 100.0         // -1..1

        // End points: negative Blacks deepens blacks; positive Whites lifts whites.
        let bl = min(max(-blacks * 0.12, -0.1), 0.4)        // black input pivot
        let wl = min(max(1.0 - whites * 0.12, 0.6), 1.1)    // white input pivot
        let span = max(1e-3, wl - bl)

        for i in 0..<lutSize {
            let x = Double(i) / Double(lutSize - 1)

            // 1) black / white point remap
            var v = (x - bl) / span
            v = min(max(v, 0.0), 1.0)

            // 2) shadows / highlights region shift
            let wH = v * v                  // emphasis on brights
            let wS = (1 - v) * (1 - v)      // emphasis on darks
            v += hi * 0.28 * wH + sh * 0.28 * wS
            v = min(max(v, 0.0), 1.0)

            // 3) contrast S-curve around mid grey
            let t = v - 0.5
            v = 0.5 + t * (1.0 + contrast) + contrast * 0.6 * t * (0.25 - t * t)
            lut[i] = Float(min(max(v, 0.0), 1.0))
        }
        return lut
    }

    /// Sample a LUT with linear interpolation; input clamped to 0..1.
    @inline(__always)
    public static func sample(_ lut: UnsafePointer<Float>, _ x: Float) -> Float {
        if x <= 0 { return lut[0] }
        if x >= 1 { return lut[lutSize - 1] }
        let f = x * Float(lutSize - 1)
        let i = Int(f)
        let frac = f - Float(i)
        return lut[i] + (lut[i + 1] - lut[i]) * frac
    }
}
