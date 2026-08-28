import Foundation

/// Parameters for a geometry resample. Deliberately `Double`: the geometry maths must
/// stay bit-identical to the reference implementation, and float would drift.
public struct ResampleParams: Sendable {
    public var mode: Int       // 0 = radial distortion, 1 = rotate about (cx,cy)
    public var k: Double
    public var cx: Double, cy: Double
    public var sinA: Double, cosA: Double
    public var ox: Double, oy: Double

    public init(mode: Int, k: Double, cx: Double, cy: Double,
                sinA: Double, cosA: Double, ox: Double, oy: Double) {
        self.mode = mode; self.k = k
        self.cx = cx; self.cy = cy
        self.sinA = sinA; self.cosA = cosA
        self.ox = ox; self.oy = oy
    }
}

/// One blur-based stage: box blur of `radius`, then either blended back (mode 0 —
/// noise reduction / softening) or used as the unsharp base (mode 1 — sharpening).
public struct BlurOp: Sendable {
    public var radius: Int
    public var mode: Int
    public var amount: Float
    public init(radius: Int, mode: Int, amount: Float) {
        self.radius = radius; self.mode = mode; self.amount = amount
    }
}

/// The non-destructive rendering pipeline. Steps (per spec):
/// 1 white balance, 2 exposure, 3 tone LUT, 4 vibrance/saturation, 5 noise reduction,
/// 6 sharpen/soften, 7 gradient, 8 heal, 9 distortion, 10 rotation/crop/straighten,
/// 10c vignette, 11 watermark. Pixel work is parallelized per row.
///
/// The step order lives in exactly one place — `runPipeline` — driven through
/// `StageTarget`. Today the only target is the CPU; a Metal target slots in beside it
/// without the order being written twice.
public enum ImageProcessor {

    // ---- Public entry points --------------------------------------------

    /// Run steps 1-10 producing a new float buffer (geometry may change).
    ///
    /// Tries the GPU when it is enabled and the image fits, and falls back to the CPU on
    /// any failure. The GPU path only ever reads `src`, so a failure part-way through
    /// leaves nothing to undo — the CPU path simply starts again from the original.
    public static func applyToFloat(_ src: FloatImageBuffer, _ adj: ImageAdjustments,
                                    _ ctx: ProcessContext) throws -> FloatImageBuffer {
        if ctx.useGpu, !ctx.forceCpu, MetalPipeline.shared.canHost(width: src.width, height: src.height) {
            do {
                let target = try MetalTarget(src: src, adj: adj, ctx: ctx)
                let result = try runPipeline(target, adj, ctx)
                MetalPipeline.shared.reportSuccess()
                ctx.usedGpu = true
                return result
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                MetalPipeline.shared.reportFailure(error)
            }
        }
        ctx.usedGpu = false
        return try runPipeline(CPUTarget(buf: src.clone(), adj: adj, ctx: ctx), adj, ctx)
    }

    // ---- the pipeline, written once -------------------------------------

    /// Where the pixels live while the pipeline runs. Each call mutates the target in
    /// place (geometry calls may change its size).
    protocol StageTarget {
        var width: Int { get }
        var height: Int { get }
        func pixel(_ p: PixelStageParams) throws
        func blur(_ nr: BlurOp?, _ sh: BlurOp?) throws
        func heal() throws
        func resample(_ p: ResampleParams, _ outW: Int, _ outH: Int) throws
        func rotate(_ rot: Rotation) throws
        func result() throws -> FloatImageBuffer
    }

    static func runPipeline(_ t: StageTarget, _ adj: ImageAdjustments,
                            _ ctx: ProcessContext) throws -> FloatImageBuffer {
        let blurStages = adj.noiseReduction > 0 || adj.sharpening != 0
        let gradActive = adj.hasActiveGradient

        // 1-4 (with no blur stage in between, 7 gradient folds into the same pass)
        try t.pixel(buildColorParams(adj, ctx, withGradients: gradActive && !blurStages))

        if blurStages {
            try t.blur(adj.noiseReduction > 0 ? noiseReductionOp(adj) : nil,   // 5
                       adj.sharpening != 0 ? sharpenOp(adj) : nil)             // 6
            if gradActive { try t.pixel(buildGradientParams(adj)) }            // 7
        }

        if !adj.healSpots.isEmpty { try t.heal() }                             // 8

        // 10c vignette — post-crop, so it always hugs the frame actually shown/exported.
        func finish() throws {
            if adj.vignette != 0 {
                try t.pixel(buildVignetteParams(adj, t.width, t.height))
            }
        }

        if ctx.skipGeometry { try finish(); return try t.result() }  // gradient/heal overlay

        if adj.distortion != 0 {                                               // 9
            try t.resample(ResampleParams(mode: 0, k: adj.distortion / 100.0 * 0.35,
                                          cx: 0, cy: 0, sinA: 0, cosA: 0, ox: 0, oy: 0),
                           t.width, t.height)
        }

        try t.rotate(adj.rotation)          // 10a: discrete 90°, even under the crop overlay

        // Crop overlay: show the full distorted/rotated frame; the crop box is applied
        // only in the final (no-tool) view, so the white frame stays aligned while dragging.
        if ctx.skipCropRect {
            // Live straighten preview: the whole frame rotates about the crop centre
            // (sampled exactly as the crop extraction does), so what shows inside the
            // white box is what the final crop will be; the box itself does not rotate.
            if adj.cropAngle != 0 {
                let W = t.width, H = t.height
                let cx = (adj.cropX + adj.cropWidth / 2) * Double(W)
                let cy = (adj.cropY + adj.cropHeight / 2) * Double(H)
                let a = adj.cropAngle * Double.pi / 180.0
                try t.resample(ResampleParams(mode: 1, k: 0, cx: cx, cy: cy,
                                              sinA: sin(a), cosA: cos(a), ox: cx, oy: cy), W, H)
            }
            try finish()
            return try t.result()
        }

        // 10b: crop rectangle (with straighten angle); a full-frame crop is a no-op.
        let fullCrop = adj.cropX <= 0 && adj.cropY <= 0 &&
                       adj.cropWidth >= 1 && adj.cropHeight >= 1 && adj.cropAngle == 0
        if !fullCrop {
            let W = t.width, H = t.height
            let outW = max(1, Int((min(max(adj.cropWidth, 0.02), 1.0) * Double(W)).roundedHalfEven))
            let outH = max(1, Int((min(max(adj.cropHeight, 0.02), 1.0) * Double(H)).roundedHalfEven))
            let cx = (adj.cropX + adj.cropWidth / 2) * Double(W)
            let cy = (adj.cropY + adj.cropHeight / 2) * Double(H)
            let a = adj.cropAngle * Double.pi / 180.0
            try t.resample(ResampleParams(mode: 1, k: 0, cx: cx, cy: cy,
                                          sinA: sin(a), cosA: cos(a),
                                          ox: Double(outW) / 2.0, oy: Double(outH) / 2.0),
                           outW, outH)
        }
        try finish()
        return try t.result()
    }

    // ---- pixel-stage description ----------------------------------------

    /// Which sub-stages one fused pixel pass performs.
    public struct PixelFlags: OptionSet, Sendable {
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }
        public static let legacyWb       = PixelFlags(rawValue: 1 << 0)
        public static let linearMul      = PixelFlags(rawValue: 1 << 1)
        public static let linearMatrix   = PixelFlags(rawValue: 1 << 2)
        public static let toneLut        = PixelFlags(rawValue: 1 << 3)
        public static let vibSat         = PixelFlags(rawValue: 1 << 4)
        public static let gradients      = PixelFlags(rawValue: 1 << 5)
        public static let gradientLinear = PixelFlags(rawValue: 1 << 6)
        public static let vignette       = PixelFlags(rawValue: 1 << 7)
    }

    public struct PixelStageParams {
        public var flags: PixelFlags = []
        public var toneLut: [Float]?
        public var wbMul: (Float, Float, Float) = (1, 1, 1)
        public var m: [Float]?           // 3×3, exposure already folded in
        public var sat: Float = 0
        public var vib: Float = 0
        public var gradients: [LinearGradient] = []
        public var vigAmount: Float = 0
        public var vigCx: Float = 0, vigCy: Float = 0, vigInvMax: Float = 1
        public init() {}
    }

    // ---- parameter builders ---------------------------------------------

    static func buildColorParams(_ adj: ImageAdjustments, _ ctx: ProcessContext,
                                 withGradients: Bool) -> PixelStageParams {
        var p = PixelStageParams()
        p.flags = .toneLut
        p.toneLut = ToneCurve.buildLut(adj)
        let exp = Float(pow(2.0, adj.exposure))

        if adj.isLegacyPipeline {
            let (mr, mg, mb) = whiteBalanceMultipliers(temperature: adj.temperature, tint: adj.tint)
            p.flags.insert(.legacyWb)
            p.wbMul = (Float(mr) * exp, Float(mg) * exp, Float(mb) * exp)
        } else {
            let matrix: [Float]? = {
                guard let cam = ctx.camera, cam.isValid else { return nil }
                return ColorScience.whiteBalanceMatrix(cam, kelvin: adj.temperature,
                                                       tint: adj.tint,
                                                       reference: ctx.whiteBalanceReference)
            }()
            if let m = matrix {
                p.flags.insert(.linearMatrix)
                p.m = m.map { $0 * exp }
            } else {
                let (mr, mg, mb) = whiteBalanceMultipliers(temperature: adj.temperature, tint: adj.tint)
                p.flags.insert(.linearMul)
                p.wbMul = (Float(mr) * exp, Float(mg) * exp, Float(mb) * exp)
            }
        }

        let sat = Float(adj.saturation / 100.0)
        let vib = Float(adj.vibrance / 100.0)
        if sat != 0 || vib != 0 {
            p.flags.insert(.vibSat); p.sat = sat; p.vib = vib
        }
        if withGradients { addGradients(&p, adj) }
        return p
    }

    static func buildGradientParams(_ adj: ImageAdjustments) -> PixelStageParams {
        var p = PixelStageParams()
        addGradients(&p, adj)
        return p
    }

    static func addGradients(_ p: inout PixelStageParams, _ adj: ImageAdjustments) {
        let list = adj.gradients.filter { $0.hasEffect }
        guard !list.isEmpty else { return }
        p.flags.insert(.gradients)
        if !adj.isLegacyPipeline { p.flags.insert(.gradientLinear) }
        p.gradients = list
    }

    static func buildVignetteParams(_ adj: ImageAdjustments, _ width: Int, _ height: Int) -> PixelStageParams {
        var p = PixelStageParams()
        let cx = Float(width - 1) * 0.5, cy = Float(height - 1) * 0.5
        p.flags = .vignette
        p.vigAmount = Float(-adj.vignette / 100.0)
        p.vigCx = cx; p.vigCy = cy
        p.vigInvMax = 1 / (cx * cx + cy * cy).squareRoot()
        return p
    }

    static func noiseReductionOp(_ adj: ImageAdjustments) -> BlurOp {
        let strength = Float(adj.noiseReduction / 100.0)
        let radius = 1 + Int((strength * 2).roundedHalfEven)
        return BlurOp(radius: radius, mode: 0, amount: min(max(strength * 0.8, 0), 1))
    }

    static func sharpenOp(_ adj: ImageAdjustments) -> BlurOp {
        let amt = Float(adj.sharpening / 100.0)
        if amt > 0 { return BlurOp(radius: 1, mode: 1, amount: amt * 1.5) }
        let radius = 1 + Int((-amt * 2).roundedHalfEven)
        return BlurOp(radius: radius, mode: 0, amount: min(max(-amt, 0), 1))
    }

    // ---- 1 + 2: white balance & exposure --------------------------------

    /// RGB multipliers for a temperature (K) and tint, neutral at 5200 K / 0 tint.
    /// Black-body approximation — the legacy pipeline's only white balance, and v1's
    /// fallback when no camera colour data is available.
    public static func whiteBalanceMultipliers(temperature: Double, tint: Double) -> (r: Double, g: Double, b: Double) {
        let (r, g, b) = kelvinToRgb(temperature)
        let (r0, g0, b0) = kelvinToRgb(5200)
        var mr = r0 / max(1e-3, r)
        var mg = g0 / max(1e-3, g)
        var mb = b0 / max(1e-3, b)
        mr /= mg; mb /= mg; mg = 1.0                        // normalize on green
        mg *= 1.0 - min(max(tint / 100.0, -1), 1) * 0.30    // +tint = magenta (less green)
        return (mr, mg, mb)
    }

    static func kelvinToRgb(_ kelvin: Double) -> (r: Double, g: Double, b: Double) {
        let t = min(max(kelvin, 1000), 40000) / 100.0
        let r = t <= 66 ? 255 : 329.698727446 * pow(t - 60, -0.1332047592)
        let g = t <= 66 ? 99.4708025861 * log(t) - 161.1195681661
                        : 288.1221695283 * pow(t - 60, -0.0755148492)
        let b = t >= 66 ? 255 : (t <= 19 ? 0 : 138.5177312231 * log(t - 10) - 305.0447927307)
        return (min(max(r, 0), 255) / 255.0,
                min(max(g, 0), 255) / 255.0,
                min(max(b, 0), 255) / 255.0)
    }

    // ---- shared helpers --------------------------------------------------

    @inline(__always) static func clamp0(_ v: Float) -> Float { v < 0 ? 0 : v }
    @inline(__always) static func smooth(_ x: Double) -> Double {
        let x = min(max(x, 0), 1); return x * x * (3 - 2 * x)
    }
    @inline(__always) static func inBounds(_ x: Int, _ y: Int, _ w: Int, _ h: Int) -> Bool {
        x >= 0 && y >= 0 && x < w && y < h
    }
}
