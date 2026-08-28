import Foundation

/// RGB histogram (256 bins per channel).
public struct Histogram: Sendable {
    public var r = [Int](repeating: 0, count: 256)
    public var g = [Int](repeating: 0, count: 256)
    public var b = [Int](repeating: 0, count: 256)
    public var max = 1
    public init() {}
}

public enum ImageStats {

    /// Per-channel 256-bin histogram of a rendered buffer.
    public static func computeHistogram(_ buf: FloatImageBuffer) -> Histogram {
        var h = Histogram()
        let d = buf.data
        var i = 0
        let n = buf.width * buf.height
        for _ in 0..<n {
            h.r[bin(d[i])] += 1
            h.g[bin(d[i + 1])] += 1
            h.b[bin(d[i + 2])] += 1
            i += 4
        }
        // Ignore the pure 0/255 spikes when scaling, so a clipped sky does not flatten
        // the rest of the curve.
        var mx = 0
        for i in 1..<255 {
            if h.r[i] > mx { mx = h.r[i] }
            if h.g[i] > mx { mx = h.g[i] }
            if h.b[i] > mx { mx = h.b[i] }
        }
        h.max = mx <= 0 ? 1 : mx
        return h
    }

    @inline(__always)
    static func bin(_ v: Float) -> Int {
        let i = Int(v * 255 + 0.5)
        return i < 0 ? 0 : (i > 255 ? 255 : i)
    }

    /// Average colour of the whole buffer, in 0..255 units to match the panel's display.
    public static func meanColor(_ buf: FloatImageBuffer) -> (r: Double, g: Double, b: Double) {
        var sr = 0.0, sg = 0.0, sb = 0.0
        let d = buf.data
        let n = buf.width * buf.height
        guard n > 0 else { return (0, 0, 0) }
        var i = 0
        for _ in 0..<n {
            sr += Double(d[i]); sg += Double(d[i + 1]); sb += Double(d[i + 2])
            i += 4
        }
        let c = Double(n)
        return (sr / c * 255, sg / c * 255, sb / c * 255)
    }

    /// Mean linear-sRGB colour of a small patch, for the white-balance eyedropper.
    public static func patchMean(_ buf: FloatImageBuffer, x: Int, y: Int, radius: Int = 3)
        -> (r: Double, g: Double, b: Double)? {
        var sr = 0.0, sg = 0.0, sb = 0.0
        var n = 0
        for yy in (y - radius)...(y + radius) {
            for xx in (x - radius)...(x + radius) {
                guard xx >= 0, yy >= 0, xx < buf.width, yy < buf.height else { continue }
                let i = (yy * buf.width + xx) * 4
                sr += Double(ColorScience.linearize(buf.data[i]))
                sg += Double(ColorScience.linearize(buf.data[i + 1]))
                sb += Double(ColorScience.linearize(buf.data[i + 2]))
                n += 1
            }
        }
        guard n > 0 else { return nil }
        return (sr / Double(n), sg / Double(n), sb / Double(n))
    }
}
