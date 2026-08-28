import Foundation

/// Which white balance the source pixels were already balanced to.
public enum WhiteBalanceReference: Sendable {
    /// LibRaw proxy / full decode: balanced with `pre_mul` (daylight matrix multipliers).
    case decode
    /// Camera-rendered preview (embedded JPEG): the as-shot `cam_mul` is baked in.
    case asShot
}

/// Colour math for the linear-light pipeline (pipelineVersion ≥ 1): the transfer curve
/// LibRaw bakes into its output, CCT ↔ chromaticity on the Planckian locus, and the
/// camera-space white-balance matrix built from LibRaw's `pre_mul` / `cam_mul` / `rgb_cam`.
public enum ColorScience {

    // ---- transfer curve --------------------------------------------------
    // LibRaw's default gamm = {0.45, 4.5} (init_close_utils.cpp:32) = BT.709 OETF:
    //   V = 4.5 L                  (L < 0.018)
    //   V = 1.099 L^0.45 − 0.099   (otherwise)
    // JPEGs decoded by ImageIO are sRGB, which differs only by a few percent in the toe;
    // for "adjust white balance on an already-baked JPEG" — approximate by nature — one
    // shared curve matters more than the difference.

    public static let decodeLutSize = 4096
    public static let encodeLutSize = 8192

    /// n + 1 entries: the extra endpoint lets `sample` interpolate without a bounds test.
    /// Held as raw pointers because the pipeline samples them once per channel per pixel —
    /// re-entering `withUnsafeBufferPointer` tens of millions of times per render is not free.
    nonisolated(unsafe) public static let decodeLut: UnsafePointer<Float> =
        buildLut(decodeLutSize) { decodeExact($0) }
    nonisolated(unsafe) public static let encodeLut: UnsafePointer<Float> =
        buildLut(encodeLutSize) { encodeExact($0) }

    private static func buildLut(_ n: Int, _ f: (Double) -> Double) -> UnsafePointer<Float> {
        let p = UnsafeMutablePointer<Float>.allocate(capacity: n + 1)
        for i in 0...n { p[i] = Float(f(Double(i) / Double(n))) }
        return UnsafePointer(p)
    }

    /// The tables as arrays, for callers that need to copy them (diagnostics, GPU upload).
    public static var decodeTable: [Float] { Array(UnsafeBufferPointer(start: decodeLut, count: decodeLutSize + 1)) }
    public static var encodeTable: [Float] { Array(UnsafeBufferPointer(start: encodeLut, count: encodeLutSize + 1)) }

    public static func encodeExact(_ l: Double) -> Double {
        l <= 0 ? 0 : (l < 0.018 ? 4.5 * l : 1.099 * pow(l, 0.45) - 0.099)
    }

    public static func decodeExact(_ v: Double) -> Double {
        v <= 0 ? 0 : (v < 0.081 ? v / 4.5 : pow((v + 0.099) / 1.099, 1.0 / 0.45))
    }

    /// Encoded (display) value → linear light. Input clamped to 0..1.
    @inline(__always)
    public static func linearize(_ v: Float) -> Float {
        sample(decodeLut, decodeLutSize, v)
    }

    /// Linear light → encoded value. Input clamped to 0..1 (highlights clip here,
    /// exactly where the legacy path's tone LUT clipped them).
    @inline(__always)
    public static func encode(_ l: Float) -> Float {
        sample(encodeLut, encodeLutSize, l)
    }

    @inline(__always)
    static func sample(_ lut: UnsafePointer<Float>, _ n: Int, _ x: Float) -> Float {
        if !(x > 0) { return 0 }          // also catches NaN
        if x >= 1 { return lut[n] }
        let f = x * Float(n)
        let i = Int(f)
        let t = f - Float(i)
        return lut[i] + (lut[i + 1] - lut[i]) * t
    }

    // ---- XYZ ↔ linear sRGB (D65) ------------------------------------------

    static let xyzToSrgb: [Double] = [
         3.2404542, -1.5371385, -0.4985314,
        -0.9692660,  1.8760108,  0.0415560,
         0.0556434, -0.2040259,  1.0572252
    ]

    static let srgbToXyz: [Double] = [
        0.4124564, 0.3575761, 0.1804375,
        0.2126729, 0.7151522, 0.0721750,
        0.0193339, 0.1191920, 0.9503041
    ]

    // ---- Planckian locus / CCT ---------------------------------------------
    // Colour temperature always runs along the black-body locus (Kang et al. 2002,
    // 1667–25000K). The daylight locus sits slightly green of it above 4000K
    // (Duv ≈ +0.003), but as-shot and the slider must share one locus to be
    // self-consistent — the offset is absorbed by tint.

    public static let minKelvin = 2000.0
    public static let maxKelvin = 12000.0

    /// 1 tint unit = this much Duv. ±100 ≈ ±0.02 Duv, the same visual strength as the
    /// legacy ±30% green multiplier.
    public static let duvPerTintUnit = 0.0002

    static func planckianXy(_ T: Double) -> (x: Double, y: Double) {
        let T = min(max(T, 1667), 25000)
        let t = 1e3 / T, t2 = t * t, t3 = t2 * t
        let x = T <= 4000
            ? -0.2661239 * t3 - 0.2343589 * t2 + 0.8776956 * t + 0.179910
            : -3.0258469 * t3 + 2.1070379 * t2 + 0.2226347 * t + 0.240390
        let x2 = x * x, x3 = x2 * x
        let y: Double
        if T <= 2222 {
            y = -1.1063814 * x3 - 1.34811020 * x2 + 2.18555832 * x - 0.20219683
        } else if T <= 4000 {
            y = -0.9549476 * x3 - 1.37418593 * x2 + 2.09137015 * x - 0.16748867
        } else {
            y =  3.0817580 * x3 - 5.87338670 * x2 + 3.75112997 * x - 0.37001483
        }
        return (x, y)
    }

    static func xyToUv(_ x: Double, _ y: Double) -> (u: Double, v: Double) {
        let d = -2 * x + 12 * y + 3
        return (4 * x / d, 6 * y / d)
    }

    static func uvToXy(_ u: Double, _ v: Double) -> (x: Double, y: Double) {
        let d = 2 * u - 8 * v + 4
        return (3 * u / d, 2 * v / d)
    }

    static func planckianUv(_ T: Double) -> (u: Double, v: Double) {
        let (x, y) = planckianXy(T)
        return xyToUv(x, y)
    }

    /// Unit normal to the locus at T pointing to the green side (+Duv).
    static func greenNormal(_ T: Double) -> (nu: Double, nv: Double) {
        let (u0, v0) = planckianUv(T - 10)
        let (u1, v1) = planckianUv(T + 10)
        var du = u1 - u0, dv = v1 - v0
        let len = (du * du + dv * dv).squareRoot()
        if len < 1e-12 { return (0, 1) }
        du /= len; dv /= len
        // The tangent toward higher T runs to lower u / lower v; green lies at lower u
        // and higher v.
        return (dv, -du)
    }

    /// XYZ (Y = 1) of the illuminant at (K, tint); +tint = magenta side of the locus.
    static func illuminantXyz(kelvin: Double, tint: Double) -> (X: Double, Y: Double, Z: Double) {
        let k = min(max(kelvin, minKelvin), maxKelvin)
        var (u, v) = planckianUv(k)
        let (nu, nv) = greenNormal(k)
        let duv = -tint * duvPerTintUnit
        u += nu * duv; v += nv * duv
        let (x, y0) = uvToXy(u, v)
        let y = y0 <= 1e-6 ? 1e-6 : y0
        return (x / y, 1.0, (1 - x - y) / y)
    }

    /// Closest locus point (CCT) and signed Duv (+ = green) for a chromaticity.
    static func uvToKelvinDuv(_ u: Double, _ v: Double) -> (kelvin: Double, duv: Double) {
        func dist2(_ T: Double) -> Double {
            let (pu, pv) = planckianUv(T)
            return (pu - u) * (pu - u) + (pv - v) * (pv - v)
        }
        var best = minKelvin, bestD = Double.greatestFiniteMagnitude
        var T = minKelvin
        while T <= maxKelvin {
            let d = dist2(T)
            if d < bestD { bestD = d; best = T }
            T += 100
        }
        // Refine by ternary search inside the winning ±100 K bracket.
        var lo = max(minKelvin, best - 100), hi = min(maxKelvin, best + 100)
        for _ in 0..<40 {
            let m1 = lo + (hi - lo) / 3, m2 = hi - (hi - lo) / 3
            if dist2(m1) < dist2(m2) { hi = m2 } else { lo = m1 }
        }
        let K = (lo + hi) / 2
        let (u0, v0) = planckianUv(K)
        let (nu, nv) = greenNormal(K)
        let duv = (u - u0) * nu + (v - v0) * nv
        return (K, duv)
    }

    // ---- camera white balance --------------------------------------------

    /// Raw camera channel response (R,G,B, G = 1) to a neutral patch lit by (K, tint).
    /// Inverse of the multipliers LibRaw would need to neutralise that light. Nil when
    /// the matrix throws the illuminant out of the camera's gamut (degenerate profile).
    static func neutralCameraResponse(_ cc: CameraColorInfo, kelvin: Double, tint: Double) -> [Double]? {
        let (X, Y, Z) = illuminantXyz(kelvin: kelvin, tint: tint)
        let srgb = mul3(xyzToSrgb, X, Y, Z)
        guard let inv = invert3(cc.rgbCam) else { return nil }
        let camScaled = mul3(inv, srgb[0], srgb[1], srgb[2])
        var raw = [Double](repeating: 0, count: 3)
        for i in 0..<3 {
            raw[i] = camScaled[i] / cc.preMul[i]
            if !(raw[i] > 1e-9) { return nil }
        }
        return normalizeGreen(raw)
    }

    /// Camera multipliers (G = 1) that neutralise the illuminant at (K, tint).
    public static func kelvinTintToCamMul(_ cc: CameraColorInfo, kelvin: Double, tint: Double) -> [Double]? {
        guard let raw = neutralCameraResponse(cc, kelvin: kelvin, tint: tint) else { return nil }
        return normalizeGreen([1 / raw[0], 1 / raw[1], 1 / raw[2]])
    }

    /// (K, tint) of the illuminant that a set of camera multipliers neutralises —
    /// as-shot from `cam_mul`, or a picked neutral patch.
    public static func camMulToKelvinTint(_ cc: CameraColorInfo, mul: [Double]) -> (kelvin: Double, tint: Double)? {
        guard mul.count >= 3, mul[0] > 0, mul[1] > 0, mul[2] > 0 else { return nil }
        // A neutral patch's raw response ∝ 1/mul; LibRaw scales by pre_mul then rgb_cam
        // to reach linear sRGB.
        var camScaled = [Double](repeating: 0, count: 3)
        for i in 0..<3 { camScaled[i] = cc.preMul[i] / mul[i] }
        let srgb = mul3(cc.rgbCam, camScaled[0], camScaled[1], camScaled[2])
        let xyz = mul3(srgbToXyz, srgb[0], srgb[1], srgb[2])
        let sum = xyz[0] + xyz[1] + xyz[2]
        guard sum > 1e-9, xyz[1] > 0 else { return nil }
        let (u, v) = xyToUv(xyz[0] / sum, xyz[1] / sum)
        let (K, duv) = uvToKelvinDuv(u, v)
        return (K, min(max(-duv / duvPerTintUnit, -100), 100))
    }

    /// As-shot (K, tint) from the camera's recorded multipliers.
    public static func asShot(_ cc: CameraColorInfo) -> (kelvin: Double, tint: Double)? {
        camMulToKelvinTint(cc, mul: cc.camMul)
    }

    /// 3×3 matrix (row-major) that re-balances **linear sRGB** pixels — already balanced
    /// to `reference` — to (K, tint). Built in camera space:
    /// M = rgb_cam · diag(mul_target / mul_reference) · rgb_cam⁻¹.
    /// Nil → caller falls back to the black-body multipliers.
    public static func whiteBalanceMatrix(_ cc: CameraColorInfo, kelvin: Double, tint: Double,
                                          reference: WhiteBalanceReference) -> [Float]? {
        guard cc.isValid else { return nil }
        guard let target = kelvinTintToCamMul(cc, kelvin: kelvin, tint: tint) else { return nil }
        let refMul = reference == .asShot ? cc.camMul : cc.preMul
        guard let inv = invert3(cc.rgbCam) else { return nil }

        var gains = [Double](repeating: 0, count: 3)
        for i in 0..<3 {
            guard refMul[i] > 0 else { return nil }
            gains[i] = target[i] / refMul[i]
        }
        // M = rgbCam · diag(g) · inv
        var md = [Double](repeating: 0, count: 9)
        for r in 0..<3 {
            for c in 0..<3 {
                var s = 0.0
                for k in 0..<3 { s += cc.rgbCam[r * 3 + k] * gains[k] * inv[k * 3 + c] }
                md[r * 3 + c] = s
            }
        }

        // Luminance normalisation: a neutral grey must keep its Y. Camera-space gains
        // with G = 1 leave sRGB luminance drifting (6500→3200K loses about 20%), and the
        // white-balance slider has no business changing exposure — the legacy path kept
        // sRGB G = 1 for the same reason.
        let yr = 0.2126729, yg = 0.7151522, yb = 0.0721750
        let gr = md[0] + md[1] + md[2]
        let gg = md[3] + md[4] + md[5]
        let gb = md[6] + md[7] + md[8]
        let Y = yr * gr + yg * gg + yb * gb
        guard Y > 1e-6 else { return nil }
        return md.map { Float($0 / Y) }
    }

    /// Given a sampled linear-sRGB pixel that should be neutral (WB picker), the camera
    /// multipliers that would make it so — feed to `camMulToKelvinTint`.
    public static func neutralizingCamMul(_ cc: CameraColorInfo, r: Double, g: Double, b: Double,
                                          reference: WhiteBalanceReference) -> [Double]? {
        guard cc.isValid, let inv = invert3(cc.rgbCam) else { return nil }
        let patch = mul3(inv, r, g, b)      // camera-scaled response of the patch
        let white = mul3(inv, 1, 1, 1)      // camera-scaled response of a true neutral
        let refMul = reference == .asShot ? cc.camMul : cc.preMul
        var mul = [Double](repeating: 0, count: 3)
        for i in 0..<3 {
            guard patch[i] > 1e-9, white[i] > 1e-9 else { return nil }
            mul[i] = refMul[i] * white[i] / patch[i]
        }
        return normalizeGreen(mul)
    }

    // ---- small linear algebra ---------------------------------------------

    static func mul3(_ m: [Double], _ a: Double, _ b: Double, _ c: Double) -> [Double] {
        [m[0] * a + m[1] * b + m[2] * c,
         m[3] * a + m[4] * b + m[5] * c,
         m[6] * a + m[7] * b + m[8] * c]
    }

    public static func invert3(_ m: [Double]) -> [Double]? {
        guard m.count == 9 else { return nil }
        let a = m[0], b = m[1], c = m[2], d = m[3], e = m[4], f = m[5], g = m[6], h = m[7], i = m[8]
        let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g)
        if abs(det) < 1e-12 || det.isNaN { return nil }
        let s = 1 / det
        return [
            (e * i - f * h) * s, (c * h - b * i) * s, (b * f - c * e) * s,
            (f * g - d * i) * s, (a * i - c * g) * s, (c * d - a * f) * s,
            (d * h - e * g) * s, (b * g - a * h) * s, (a * e - b * d) * s
        ]
    }

    public static func normalizeGreen(_ v: [Double]) -> [Double] {
        let g = v[1] > 1e-12 ? v[1] : 1
        return [v[0] / g, 1.0, v[2] / g]
    }
}
