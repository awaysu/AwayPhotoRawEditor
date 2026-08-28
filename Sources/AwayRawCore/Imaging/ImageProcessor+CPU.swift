import Foundation

extension ImageProcessor {

    /// The reference implementation: the image lives in CPU memory and every stage
    /// mutates it in place. A future Metal target must match this maths exactly.
    final class CPUTarget: StageTarget {
        var buf: FloatImageBuffer
        let adj: ImageAdjustments
        let ctx: ProcessContext

        init(buf: FloatImageBuffer, adj: ImageAdjustments, ctx: ProcessContext) {
            self.buf = buf; self.adj = adj; self.ctx = ctx
        }

        var width: Int { buf.width }
        var height: Int { buf.height }

        func pixel(_ p: PixelStageParams) throws {
            try ctx.token.check()
            let H = buf.height
            if p.flags.contains(.legacyWb) {
                whiteBalanceExposureLegacy(buf, p, 0, H)
            } else if p.flags.contains(.linearMul) || p.flags.contains(.linearMatrix) {
                whiteBalanceExposureLinear(buf, p, 0, H)
            }
            if p.flags.contains(.toneLut), let lut = p.toneLut {
                applyLut(buf, lut, 0, H)
            }
            if p.flags.contains(.vibSat) {
                vibranceSaturation(buf, p.sat, p.vib, 0, H)
            }
            if p.flags.contains(.gradients) {
                gradient(buf, p.gradients, linearExposure: p.flags.contains(.gradientLinear), 0, H)
            }
            if p.flags.contains(.vignette) {
                vignette(buf, p, 0, H)
            }
        }

        func blur(_ nr: BlurOp?, _ sh: BlurOp?) throws {
            try ctx.token.check()
            if let nr { applyBlurOp(buf, nr) }
            try ctx.token.check()
            if let sh { applyBlurOp(buf, sh) }
        }

        func heal() throws {
            try ctx.token.check()
            ImageProcessor.heal(buf, adj, ctx.token)
        }

        func resample(_ p: ResampleParams, _ outW: Int, _ outH: Int) throws {
            try ctx.token.check()
            buf = cpuResample(buf, p, outW, outH)
        }

        func rotate(_ rot: Rotation) throws {
            try ctx.token.check()
            buf = rotateDiscrete(buf, rot)
        }

        func result() throws -> FloatImageBuffer {
            try ctx.token.check()
            return buf
        }
    }

    // ---- 1 + 2: white balance & exposure --------------------------------

    static func whiteBalanceExposureLegacy(_ buf: FloatImageBuffer, _ p: PixelStageParams,
                                           _ y0: Int, _ y1: Int) {
        let (fr, fg, fb) = p.wbMul
        let d = buf.data, W = buf.width
        parallelRows(y0, y1) { y in
            var i = y * W * 4
            for _ in 0..<W {
                d[i] *= fr; d[i + 1] *= fg; d[i + 2] *= fb
                i += 4
            }
        }
    }

    /// v1: decode the transfer curve → white balance → exposure → re-encode, so both
    /// gains act on light rather than on gamma-encoded values. White balance is a 3×3
    /// matrix in camera space when LibRaw gave us the camera's colour data; otherwise the
    /// black-body multipliers (now applied in linear, the only correct place for them).
    static func whiteBalanceExposureLinear(_ buf: FloatImageBuffer, _ p: PixelStageParams,
                                           _ y0: Int, _ y1: Int) {
        let d = buf.data, W = buf.width
        if let m = p.m {
            let m0 = m[0], m1 = m[1], m2 = m[2]
            let m3 = m[3], m4 = m[4], m5 = m[5]
            let m6 = m[6], m7 = m[7], m8 = m[8]
            parallelRows(y0, y1) { y in
                var i = y * W * 4
                for _ in 0..<W {
                    let r = ColorScience.linearize(d[i])
                    let g = ColorScience.linearize(d[i + 1])
                    let b = ColorScience.linearize(d[i + 2])
                    d[i]     = ColorScience.encode(m0 * r + m1 * g + m2 * b)
                    d[i + 1] = ColorScience.encode(m3 * r + m4 * g + m5 * b)
                    d[i + 2] = ColorScience.encode(m6 * r + m7 * g + m8 * b)
                    i += 4
                }
            }
        } else {
            let (fr, fg, fb) = p.wbMul
            parallelRows(y0, y1) { y in
                var i = y * W * 4
                for _ in 0..<W {
                    d[i]     = ColorScience.encode(ColorScience.linearize(d[i]) * fr)
                    d[i + 1] = ColorScience.encode(ColorScience.linearize(d[i + 1]) * fg)
                    d[i + 2] = ColorScience.encode(ColorScience.linearize(d[i + 2]) * fb)
                    i += 4
                }
            }
        }
    }

    // ---- 3: tone LUT -----------------------------------------------------

    static func applyLut(_ buf: FloatImageBuffer, _ lut: [Float], _ y0: Int, _ y1: Int) {
        let d = buf.data, W = buf.width
        lut.withUnsafeBufferPointer { lp in
            let l = lp.baseAddress!
            parallelRows(y0, y1) { y in
                var i = y * W * 4
                for _ in 0..<W {
                    d[i]     = ToneCurve.sample(l, d[i])
                    d[i + 1] = ToneCurve.sample(l, d[i + 1])
                    d[i + 2] = ToneCurve.sample(l, d[i + 2])
                    i += 4
                }
            }
        }
    }

    // ---- 4: vibrance / saturation ---------------------------------------

    static func vibranceSaturation(_ buf: FloatImageBuffer, _ sat: Float, _ vib: Float,
                                   _ y0: Int, _ y1: Int) {
        if sat == 0 && vib == 0 { return }
        let d = buf.data, W = buf.width
        parallelRows(y0, y1) { y in
            var i = y * W * 4
            for _ in 0..<W {
                let r = d[i], g = d[i + 1], b = d[i + 2]
                let luma = 0.299 * r + 0.587 * g + 0.114 * b
                let mx = max(r, max(g, b))
                let mn = min(r, min(g, b))
                let curSat: Float = mx <= 1e-4 ? 0 : (mx - mn) / mx
                let f = (1 + vib * (1 - curSat)) * (1 + sat)
                d[i]     = clamp0(luma + (r - luma) * f)
                d[i + 1] = clamp0(luma + (g - luma) * f)
                d[i + 2] = clamp0(luma + (b - luma) * f)
                i += 4
            }
        }
    }

    // ---- 5 / 6: noise reduction and sharpen / soften ---------------------

    static func applyBlurOp(_ buf: FloatImageBuffer, _ op: BlurOp) {
        let blurred = boxBlur(buf, op.radius)
        if op.mode == 1 {
            // unsharp mask
            let d = buf.data, b = blurred.data, W = buf.width
            let k = op.amount
            parallelRows(0, buf.height) { y in
                var i = y * W * 4
                for _ in 0..<W {
                    d[i]     = clamp0(d[i]     + k * (d[i]     - b[i]))
                    d[i + 1] = clamp0(d[i + 1] + k * (d[i + 1] - b[i + 1]))
                    d[i + 2] = clamp0(d[i + 2] + k * (d[i + 2] - b[i + 2]))
                    i += 4
                }
            }
        } else {
            blend(buf, blurred, op.amount)
        }
    }

    // ---- 7: graduated filter --------------------------------------------

    static func gradient(_ buf: FloatImageBuffer, _ grads: [LinearGradient],
                         linearExposure: Bool, _ y0: Int, _ y1: Int) {
        // Stack every linear gradient, each over the running buffer.
        for gr in grads where gr.hasEffect {
            applyGradient(buf, gr, linearExposure: linearExposure, y0, y1)
        }
    }

    /// - Parameter linearExposure: v1 — the gradient's exposure multiplies light
    ///   (decode → × → encode) like the global exposure does; its contrast / highlights /
    ///   shadows / saturation stay in the encoded domain, where they were designed.
    static func applyGradient(_ buf: FloatImageBuffer, _ gr: LinearGradient,
                              linearExposure: Bool, _ y0: Int, _ y1: Int) {
        let a = gr.angle * Double.pi / 180.0
        let sinA = sin(a), cosA = cos(a)
        let centerX = gr.centerX, centerY = gr.centerY
        let range = max(1e-3, gr.range)
        let gExp = Float(gr.exposure)
        let gCon = Float(gr.contrast / 100.0)
        let gHi  = Float(gr.highlights / 100.0)
        let gSh  = Float(gr.shadows / 100.0)
        let gSat = Float(gr.saturation / 100.0)
        let W = buf.width, H = buf.height
        let d = buf.data

        parallelRows(y0, y1) { y in
            let ny = Double(y) / Double(H)
            var i = y * W * 4
            for x in 0..<W {
                defer { i += 4 }
                let nx = Double(x) / Double(W)
                let dist = (nx - centerX) * sinA + (ny - centerY) * cosA
                var m = Float(min(max(dist / (2 * range) + 0.5, 0.0), 1.0))
                m = m * m * (3 - 2 * m)             // smoothstep
                if m <= 0 { continue }

                var r: Float, g: Float, b: Float
                if gExp == 0 {
                    r = d[i]; g = d[i + 1]; b = d[i + 2]
                } else {
                    let expMul = Float(pow(2.0, Double(gExp * m)))
                    if linearExposure {
                        r = ColorScience.encode(ColorScience.linearize(d[i]) * expMul)
                        g = ColorScience.encode(ColorScience.linearize(d[i + 1]) * expMul)
                        b = ColorScience.encode(ColorScience.linearize(d[i + 2]) * expMul)
                    } else {
                        r = d[i] * expMul; g = d[i + 1] * expMul; b = d[i + 2] * expMul
                    }
                }

                let luma = 0.299 * r + 0.587 * g + 0.114 * b
                if gSat != 0 {
                    let f = 1 + gSat * m
                    r = luma + (r - luma) * f; g = luma + (g - luma) * f; b = luma + (b - luma) * f
                }
                if gCon != 0 {
                    let c = gCon * m
                    r = 0.5 + (r - 0.5) * (1 + c)
                    g = 0.5 + (g - 0.5) * (1 + c)
                    b = 0.5 + (b - 0.5) * (1 + c)
                }
                if gHi != 0 {
                    let wH = luma * luma * gHi * 0.5 * m
                    r += wH; g += wH; b += wH
                }
                if gSh != 0 {
                    let wS = (1 - luma) * (1 - luma) * gSh * 0.5 * m
                    r += wS; g += wS; b += wS
                }

                d[i] = clamp0(r); d[i + 1] = clamp0(g); d[i + 2] = clamp0(b)
            }
        }
    }

    // ---- 10c: vignette (暗角) -------------------------------------------

    /// Radial corner shading applied post-crop: positive darkens the corners, negative
    /// brightens them. Ramps smoothly from ~1/3 radius outward.
    static func vignette(_ buf: FloatImageBuffer, _ p: PixelStageParams, _ y0: Int, _ y1: Int) {
        let amount = p.vigAmount     // slider + → darken (gain < 1)
        let W = buf.width
        let cx = p.vigCx, cy = p.vigCy, invMax = p.vigInvMax
        let d = buf.data

        parallelRows(y0, y1) { y in
            let dy = (Float(y) - cy) * invMax
            var i = y * W * 4
            for x in 0..<W {
                defer { i += 4 }
                let dx = (Float(x) - cx) * invMax
                let r = (dx * dx + dy * dy).squareRoot()        // 0 centre .. 1 corner
                var m = min(max((r - 0.35) / 0.65, 0), 1)       // start ~1/3 out
                m = m * m * (3 - 2 * m)                          // smoothstep
                if m <= 0 { continue }
                let g = max(0, 1 + amount * m)
                d[i] *= g; d[i + 1] *= g; d[i + 2] *= g
            }
        }
    }

    // ---- 8: heal / clone ------------------------------------------------

    static func heal(_ buf: FloatImageBuffer, _ adj: ImageAdjustments, _ token: CancelToken) {
        let W = buf.width, H = buf.height
        let maxDim = Double(max(W, H))
        let src = buf.clone()   // read from a snapshot so spots don't feed each other
        for spot in adj.healSpots {
            if token.isCancelled { return }
            let radius = max(1, Int((spot.radiusNorm * maxDim).roundedHalfEven))
            let tx = Int((spot.targetX * Double(W)).roundedHalfEven)
            let ty = Int((spot.targetY * Double(H)).roundedHalfEven)
            if spot.useInpaint {
                inpaintSpot(buf, src, tx, ty, radius)
            } else {
                cloneSpot(buf, src, tx, ty,
                          Int((spot.sourceX * Double(W)).roundedHalfEven),
                          Int((spot.sourceY * Double(H)).roundedHalfEven), radius)
            }
        }
    }

    static func cloneSpot(_ dst: FloatImageBuffer, _ src: FloatImageBuffer,
                          _ tx: Int, _ ty: Int, _ sx: Int, _ sy: Int, _ radius: Int) {
        let W = dst.width, H = dst.height
        for dy in -radius...radius {
            for dx in -radius...radius {
                let dist = (Double(dx * dx + dy * dy)).squareRoot()
                if dist > Double(radius) { continue }
                let px = tx + dx, py = ty + dy, qx = sx + dx, qy = sy + dy
                if !inBounds(px, py, W, H) || !inBounds(qx, qy, W, H) { continue }
                let a = Float(1.0 - smooth(dist / Double(radius)))   // feather edges
                let di = (py * W + px) * 4, si = (qy * W + qx) * 4
                for c in 0..<3 {
                    dst.data[di + c] = dst.data[di + c] * (1 - a) + src.data[si + c] * a
                }
            }
        }
    }

    static func inpaintSpot(_ dst: FloatImageBuffer, _ src: FloatImageBuffer,
                            _ tx: Int, _ ty: Int, _ radius: Int) {
        let W = dst.width, H = dst.height
        // Average colour of the surrounding ring.
        var sr = 0.0, sg = 0.0, sb = 0.0
        var n = 0
        let ring = radius + max(2, radius / 2)
        var ang = 0.0
        while ang < Double.pi * 2 {
            defer { ang += 0.3 }
            let qx = tx + Int(cos(ang) * Double(ring))
            let qy = ty + Int(sin(ang) * Double(ring))
            if !inBounds(qx, qy, W, H) { continue }
            let si = (qy * W + qx) * 4
            sr += Double(src.data[si]); sg += Double(src.data[si + 1]); sb += Double(src.data[si + 2])
            n += 1
        }
        if n == 0 { return }
        let fr = Float(sr / Double(n)), fg = Float(sg / Double(n)), fb = Float(sb / Double(n))
        for dy in -radius...radius {
            for dx in -radius...radius {
                let dist = (Double(dx * dx + dy * dy)).squareRoot()
                if dist > Double(radius) { continue }
                let px = tx + dx, py = ty + dy
                if !inBounds(px, py, W, H) { continue }
                let a = Float(1.0 - smooth(dist / Double(radius)))
                let di = (py * W + px) * 4
                dst.data[di]     = dst.data[di]     * (1 - a) + fr * a
                dst.data[di + 1] = dst.data[di + 1] * (1 - a) + fg * a
                dst.data[di + 2] = dst.data[di + 2] * (1 - a) + fb * a
            }
        }
    }

    // ---- 9 / 10: geometry resampling -------------------------------------

    /// Inverse-map + bilinear resample.
    /// Mode 0: radial distortion (step 9). Mode 1: rotate about (cx,cy) with the output
    /// origin at (ox,oy) — both the crop extraction (origin = output centre) and the
    /// full-frame straighten preview (origin = crop centre) are this one formula.
    static func cpuResample(_ src: FloatImageBuffer, _ p: ResampleParams,
                            _ outW: Int, _ outH: Int) -> FloatImageBuffer {
        let W = src.width, H = src.height
        let dst = FloatImageBuffer(width: outW, height: outH, zeroed: false)
        let dd = dst.data

        if p.mode == 0 {
            let k = p.k
            parallelRows(0, outH) { y in
                let ny = (Double(y) / Double(H) - 0.5) * 2
                var i = y * outW * 4
                for x in 0..<outW {
                    defer { i += 4 }
                    let nx = (Double(x) / Double(W) - 0.5) * 2
                    let r2 = nx * nx + ny * ny
                    let f = 1 + k * r2
                    let sxN = nx * f, syN = ny * f
                    let sx = (sxN / 2 + 0.5) * Double(W), sy = (syN / 2 + 0.5) * Double(H)
                    let s = sampleBilinear(src, sx, sy)
                    dd[i] = s.0; dd[i + 1] = s.1; dd[i + 2] = s.2; dd[i + 3] = s.3
                }
            }
        } else {
            let cx = p.cx, cy = p.cy, sinA = p.sinA, cosA = p.cosA, ox = p.ox, oy = p.oy
            parallelRows(0, outH) { y in
                let ry = Double(y) - oy
                var i = y * outW * 4
                for x in 0..<outW {
                    defer { i += 4 }
                    let rx = Double(x) - ox
                    let sx = cx + (rx * cosA - ry * sinA)
                    let sy = cy + (rx * sinA + ry * cosA)
                    let s = sampleBilinear(src, sx, sy)
                    dd[i] = s.0; dd[i + 1] = s.1; dd[i + 2] = s.2; dd[i + 3] = s.3
                }
            }
        }
        return dst
    }

    static func rotateDiscrete(_ src: FloatImageBuffer, _ rot: Rotation) -> FloatImageBuffer {
        if rot == .r0 { return src }
        let W = src.width, H = src.height
        let swap = (rot == .r90 || rot == .r270)
        let dst = FloatImageBuffer(width: swap ? H : W, height: swap ? W : H, zeroed: false)
        let DW = dst.width
        let s = src.data, d = dst.data
        parallelRows(0, H) { y in
            for x in 0..<W {
                let nx: Int, ny: Int
                switch rot {
                case .r90:  nx = H - 1 - y; ny = x
                case .r180: nx = W - 1 - x; ny = H - 1 - y
                default:    nx = y;         ny = W - 1 - x     // r270
                }
                let si = (y * W + x) * 4, di = (ny * DW + nx) * 4
                d[di] = s[si]; d[di + 1] = s[si + 1]
                d[di + 2] = s[si + 2]; d[di + 3] = s[si + 3]
            }
        }
        return dst
    }

    // ---- shared pixel helpers -------------------------------------------

    static func boxBlur(_ src: FloatImageBuffer, _ radius: Int) -> FloatImageBuffer {
        let W = src.width, H = src.height
        let tmp = FloatImageBuffer(width: W, height: H, zeroed: false)
        let dst = FloatImageBuffer(width: W, height: H, zeroed: false)
        let norm = 1 / Float(radius * 2 + 1)
        let sd = src.data, td = tmp.data, dd = dst.data

        // horizontal
        parallelRows(0, H) { y in
            let row = y * W * 4
            for x in 0..<W {
                var r: Float = 0, g: Float = 0, b: Float = 0
                for k in -radius...radius {
                    let xx = min(max(x + k, 0), W - 1)
                    let i = row + xx * 4
                    r += sd[i]; g += sd[i + 1]; b += sd[i + 2]
                }
                let o = row + x * 4
                td[o] = r * norm; td[o + 1] = g * norm; td[o + 2] = b * norm
                td[o + 3] = sd[o + 3]
            }
        }
        // vertical
        parallelRows(0, H) { y in
            for x in 0..<W {
                var r: Float = 0, g: Float = 0, b: Float = 0
                for k in -radius...radius {
                    let yy = min(max(y + k, 0), H - 1)
                    let i = (yy * W + x) * 4
                    r += td[i]; g += td[i + 1]; b += td[i + 2]
                }
                let o = (y * W + x) * 4
                dd[o] = r * norm; dd[o + 1] = g * norm; dd[o + 2] = b * norm
                dd[o + 3] = td[o + 3]
            }
        }
        return dst
    }

    static func blend(_ dst: FloatImageBuffer, _ other: FloatImageBuffer, _ amount: Float) {
        let amount = min(max(amount, 0), 1)
        let a = dst.data, b = other.data, W = dst.width
        parallelRows(0, dst.height) { y in
            var i = y * W * 4
            for _ in 0..<W {
                a[i]     += (b[i]     - a[i])     * amount
                a[i + 1] += (b[i + 1] - a[i + 1]) * amount
                a[i + 2] += (b[i + 2] - a[i + 2]) * amount
                i += 4
            }
        }
    }

    @inline(__always)
    static func sampleBilinear(_ buf: FloatImageBuffer, _ fx0: Double, _ fy0: Double)
        -> (Float, Float, Float, Float) {
        let W = buf.width, H = buf.height
        var fx = fx0, fy = fy0
        if fx < 0 { fx = 0 } else if fx > Double(W - 1) { fx = Double(W - 1) }
        if fy < 0 { fy = 0 } else if fy > Double(H - 1) { fy = Double(H - 1) }
        let x0 = Int(fx), y0 = Int(fy)
        let x1 = min(x0 + 1, W - 1), y1 = min(y0 + 1, H - 1)
        let tx = Float(fx - Double(x0)), ty = Float(fy - Double(y0))
        let i00 = (y0 * W + x0) * 4, i10 = (y0 * W + x1) * 4
        let i01 = (y1 * W + x0) * 4, i11 = (y1 * W + x1) * 4
        let d = buf.data
        return (lerp2(d[i00],     d[i10],     d[i01],     d[i11],     tx, ty),
                lerp2(d[i00 + 1], d[i10 + 1], d[i01 + 1], d[i11 + 1], tx, ty),
                lerp2(d[i00 + 2], d[i10 + 2], d[i01 + 2], d[i11 + 2], tx, ty),
                lerp2(d[i00 + 3], d[i10 + 3], d[i01 + 3], d[i11 + 3], tx, ty))
    }

    @inline(__always)
    static func lerp2(_ v00: Float, _ v10: Float, _ v01: Float, _ v11: Float,
                      _ tx: Float, _ ty: Float) -> Float {
        let top = v00 + (v10 - v00) * tx
        let bot = v01 + (v11 - v01) * tx
        return top + (bot - top) * ty
    }
}
