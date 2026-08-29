import Foundation
import Metal

enum MetalError: Error {
    case unavailable
    case allocationFailed
    case encodingFailed
    case commandFailed(String)
}

// Parameter structs, laid out to match the shader declarations exactly.

struct GPUPixelParams {
    var flags: UInt32 = 0
    var gradientCount: UInt32 = 0
    var wbR: Float = 1, wbG: Float = 1, wbB: Float = 1
    var m0: Float = 1, m1: Float = 0, m2: Float = 0
    var m3: Float = 0, m4: Float = 1, m5: Float = 0
    var m6: Float = 0, m7: Float = 0, m8: Float = 1
    var sat: Float = 0, vib: Float = 0
    var vigAmount: Float = 0, vigCx: Float = 0, vigCy: Float = 0, vigInvMax: Float = 1
    var width: UInt32 = 0, height: UInt32 = 0
}

struct GPUGradient {
    var sinA: Float = 0, cosA: Float = 1
    var centerX: Float = 0.5, centerY: Float = 0.15
    var inv2Range: Float = 2
    var exposure: Float = 0
    var contrast: Float = 0, highlights: Float = 0, shadows: Float = 0, saturation: Float = 0
}

struct GPUBlurParams {
    var width: UInt32 = 0, height: UInt32 = 0
    var radius: Int32 = 1
    var mode: Int32 = 0
    var amount: Float = 0
}

struct GPUResampleParams {
    var srcWidth: UInt32 = 0, srcHeight: UInt32 = 0
    var outWidth: UInt32 = 0, outHeight: UInt32 = 0
    var mode: Int32 = 0
    var k: Float = 0
    var cx: Float = 0, cy: Float = 0, sinA: Float = 0, cosA: Float = 1
    var ox: Float = 0, oy: Float = 0
}

struct GPURotateParams {
    var width: UInt32 = 0, height: UInt32 = 0
    var rot: Int32 = 0
}

extension ImageProcessor {

    /// The GPU renderer. The image is uploaded once, every stage runs on the device, and
    /// it comes back once — the equivalent of the Windows build's "resident" path.
    ///
    /// Healing stays on the CPU: it touches a handful of small discs, so downloading,
    /// fixing and re-uploading costs less than a kernel would.
    final class MetalTarget: StageTarget {
        private let gpu = MetalPipeline.shared
        private let adj: ImageAdjustments
        private let ctx: ProcessContext

        private var image: MTLBuffer
        private(set) var width: Int
        private(set) var height: Int
        /// Buffers taken from the pool, returned when this target goes away. The one
        /// finally handed back to the caller is removed from this list first.
        private var borrowed: [MTLBuffer] = []
        /// Buffers this target is done encoding against but whose command buffer has
        /// not completed. They are reused by later stages of *this* target (Metal
        /// tracks the hazard inside one command buffer) and only go back to the shared
        /// pool after `flush()` — handing them out to another target earlier would let
        /// its CPU upload land in memory the pending kernels still read.
        private var retired: [MTLBuffer] = []

        init(src: FloatImageBuffer, adj: ImageAdjustments, ctx: ProcessContext) throws {
            guard gpu.available,
                  let buf = gpu.borrowBuffer(width: src.width, height: src.height) else {
                throw MetalError.unavailable
            }
            // The pipeline must not modify the caller's source, so the pixels are copied
            // in — the CPU path clones for the same reason.
            buf.contents().assumingMemoryBound(to: Float.self)
                .update(from: src.data, count: src.count)
            self.image = buf
            self.borrowed = [buf]
            self.width = src.width
            self.height = src.height
            self.adj = adj
            self.ctx = ctx
        }

        deinit {
            // An abandoned render (cancelled or failed) must not leave the encoder open.
            // Nothing was committed without a wait, so every buffer is idle here.
            encoder?.endEncoding()
            for b in borrowed { gpu.returnBuffer(b) }
            for b in retired { gpu.returnBuffer(b) }
        }

        private func take(width w: Int, height h: Int) throws -> MTLBuffer {
            let length = w * h * 4 * MemoryLayout<Float>.size
            if let i = retired.firstIndex(where: { $0.length == length }) {
                let b = retired.remove(at: i)
                borrowed.append(b)
                return b
            }
            guard let b = gpu.borrowBuffer(width: w, height: h) else {
                throw MetalError.allocationFailed
            }
            borrowed.append(b)
            return b
        }

        private func release(_ b: MTLBuffer) {
            if let i = borrowed.firstIndex(where: { $0 === b }) { borrowed.remove(at: i) }
            retired.append(b)
        }

        // All stages are encoded into a single command buffer and submitted once. Each
        // stage used to commit and wait on its own, and for a proxy-sized render the
        // per-submission round trip was comparable to the work itself.
        //
        // Ordering and visibility between stages are safe: buffers are created with the
        // default (tracked) hazard mode, so Metal inserts the barriers between dispatches
        // that read what an earlier dispatch wrote. The blur already relied on this for
        // its H → V → combine sequence.
        private var cmd: MTLCommandBuffer?
        private var encoder: MTLComputeCommandEncoder?

        private func run(_ body: (MTLComputeCommandEncoder) throws -> Void) throws {
            if encoder == nil {
                guard let c = gpu.makeCommandBuffer(),
                      let e = c.makeComputeCommandEncoder() else {
                    throw MetalError.encodingFailed
                }
                cmd = c
                encoder = e
            }
            try body(encoder!)
        }

        /// Submit whatever has been encoded and wait for it. Needed before the CPU reads
        /// the pixels — the heal stage and the final result.
        private func flush() throws {
            guard let c = cmd else { return }
            encoder?.endEncoding()
            encoder = nil
            cmd = nil
            c.commit()
            c.waitUntilCompleted()
            // The GPU is done with everything encoded so far: the retired scratch can
            // now safely serve other targets.
            for b in retired { gpu.returnBuffer(b) }
            retired.removeAll()
            if let e = c.error { throw MetalError.commandFailed(e.localizedDescription) }
        }

        // ---- stages ------------------------------------------------------

        func pixel(_ p: PixelStageParams) throws {
            try ctx.token.check()
            guard let state = gpu.state("pixelStage"),
                  let decode = gpu.decodeLut, let encode = gpu.encodeLut else {
                throw MetalError.unavailable
            }

            var gp = GPUPixelParams()
            gp.flags = UInt32(p.flags.rawValue)
            gp.width = UInt32(width); gp.height = UInt32(height)
            (gp.wbR, gp.wbG, gp.wbB) = p.wbMul
            if let m = p.m {
                gp.m0 = m[0]; gp.m1 = m[1]; gp.m2 = m[2]
                gp.m3 = m[3]; gp.m4 = m[4]; gp.m5 = m[5]
                gp.m6 = m[6]; gp.m7 = m[7]; gp.m8 = m[8]
            }
            gp.sat = p.sat; gp.vib = p.vib
            gp.vigAmount = p.vigAmount
            gp.vigCx = p.vigCx; gp.vigCy = p.vigCy; gp.vigInvMax = p.vigInvMax

            // A tone LUT is only meaningful when the flag is set; otherwise pass a stub so
            // the buffer binding is always valid.
            let tone = p.toneLut ?? [Float](repeating: 0, count: ToneCurve.lutSize)
            let gradients: [GPUGradient] = p.gradients.map { gr in
                var g = GPUGradient()
                let a = gr.angle * Double.pi / 180.0
                g.sinA = Float(sin(a)); g.cosA = Float(cos(a))
                g.centerX = Float(gr.centerX); g.centerY = Float(gr.centerY)
                g.inv2Range = Float(1.0 / (2 * max(1e-3, gr.range)))
                g.exposure = Float(gr.exposure)
                g.contrast = Float(gr.contrast / 100.0)
                g.highlights = Float(gr.highlights / 100.0)
                g.shadows = Float(gr.shadows / 100.0)
                g.saturation = Float(gr.saturation / 100.0)
                return g
            }
            gp.gradientCount = UInt32(gradients.count)
            // Metal rejects a zero-length buffer binding, so keep at least one element.
            let gradientData = gradients.isEmpty ? [GPUGradient()] : gradients

            try run { enc in
                enc.setBuffer(image, offset: 0, index: 0)
                enc.setBytes(&gp, length: MemoryLayout<GPUPixelParams>.stride, index: 1)
                enc.setBuffer(decode, offset: 0, index: 2)
                enc.setBuffer(encode, offset: 0, index: 3)
                tone.withUnsafeBytes { enc.setBytes($0.baseAddress!, length: $0.count, index: 4) }
                gradientData.withUnsafeBytes {
                    enc.setBytes($0.baseAddress!, length: $0.count, index: 5)
                }
                gpu.dispatch(enc, state, width: width, height: height)
            }
        }

        func blur(_ nr: BlurOp?, _ sh: BlurOp?) throws {
            if let nr { try applyBlur(nr) }
            if let sh { try applyBlur(sh) }
        }

        private func applyBlur(_ op: BlurOp) throws {
            try ctx.token.check()
            guard let hState = gpu.state("blurH"),
                  let vState = gpu.state("blurV"),
                  let cState = gpu.state("blurCombine") else {
                throw MetalError.allocationFailed
            }
            let tmp = try take(width: width, height: height)
            let blurred = try take(width: width, height: height)
            defer { release(tmp); release(blurred) }
            var bp = GPUBlurParams()
            bp.width = UInt32(width); bp.height = UInt32(height)
            bp.radius = Int32(op.radius); bp.mode = Int32(op.mode); bp.amount = op.amount

            try run { enc in
                enc.setBuffer(image, offset: 0, index: 0)
                enc.setBuffer(tmp, offset: 0, index: 1)
                enc.setBytes(&bp, length: MemoryLayout<GPUBlurParams>.stride, index: 2)
                gpu.dispatch(enc, hState, width: width, height: height)

                enc.setBuffer(tmp, offset: 0, index: 0)
                enc.setBuffer(blurred, offset: 0, index: 1)
                enc.setBytes(&bp, length: MemoryLayout<GPUBlurParams>.stride, index: 2)
                gpu.dispatch(enc, vState, width: width, height: height)

                enc.setBuffer(image, offset: 0, index: 0)
                enc.setBuffer(blurred, offset: 0, index: 1)
                enc.setBytes(&bp, length: MemoryLayout<GPUBlurParams>.stride, index: 2)
                gpu.dispatch(enc, cState, width: width, height: height)
            }
        }

        func heal() throws {
            try ctx.token.check()
            try flush()          // the CPU is about to read the pixels
            // Down, fix on the CPU, back up.
            // The image buffer is shared storage, so the CPU can edit it in place and
            // there is nothing to upload afterwards.
            let view = FloatImageBuffer(
                width: width, height: height,
                borrowing: image.contents().assumingMemoryBound(to: Float.self),
                owner: image)
            ImageProcessor.heal(view, adj, ctx.token)
        }

        func resample(_ p: ResampleParams, _ outW: Int, _ outH: Int) throws {
            try ctx.token.check()
            guard let state = gpu.state("resample") else { throw MetalError.allocationFailed }
            let dst = try take(width: outW, height: outH)
            var rp = GPUResampleParams()
            rp.srcWidth = UInt32(width); rp.srcHeight = UInt32(height)
            rp.outWidth = UInt32(outW); rp.outHeight = UInt32(outH)
            rp.mode = Int32(p.mode)
            rp.k = Float(p.k)
            rp.cx = Float(p.cx); rp.cy = Float(p.cy)
            rp.sinA = Float(p.sinA); rp.cosA = Float(p.cosA)
            rp.ox = Float(p.ox); rp.oy = Float(p.oy)

            try run { enc in
                enc.setBuffer(image, offset: 0, index: 0)
                enc.setBuffer(dst, offset: 0, index: 1)
                enc.setBytes(&rp, length: MemoryLayout<GPUResampleParams>.stride, index: 2)
                gpu.dispatch(enc, state, width: outW, height: outH)
            }
            release(image)
            image = dst
            width = outW
            height = outH
        }

        func rotate(_ rot: Rotation) throws {
            guard rot != .r0 else { return }
            try ctx.token.check()
            let swap = (rot == .r90 || rot == .r270)
            let outW = swap ? height : width
            let outH = swap ? width : height
            guard let state = gpu.state("rotate90") else { throw MetalError.allocationFailed }
            let dst = try take(width: outW, height: outH)
            var rp = GPURotateParams()
            rp.width = UInt32(width); rp.height = UInt32(height)
            rp.rot = Int32(rot.rawValue)

            try run { enc in
                enc.setBuffer(image, offset: 0, index: 0)
                enc.setBuffer(dst, offset: 0, index: 1)
                enc.setBytes(&rp, length: MemoryLayout<GPURotateParams>.stride, index: 2)
                gpu.dispatch(enc, state, width: width, height: height)
            }
            release(image)
            image = dst
            width = outW
            height = outH
        }

        private func download() throws -> FloatImageBuffer {
            let out = FloatImageBuffer(width: width, height: height, zeroed: false)
            let src = image.contents().assumingMemoryBound(to: Float.self)
            out.data.update(from: src, count: out.count)
            return out
        }

        func result() throws -> FloatImageBuffer {
            try ctx.token.check()
            try flush()
            // Hand the shared-storage buffer straight over instead of copying it out.
            // The returned FloatImageBuffer retains the MTLBuffer, and it leaves the
            // borrowed list so deinit does not recycle memory the caller still holds.
            let out = image
            if let i = borrowed.firstIndex(where: { $0 === out }) { borrowed.remove(at: i) }
            return FloatImageBuffer(
                width: width, height: height,
                borrowing: out.contents().assumingMemoryBound(to: Float.self),
                owner: out)
        }
    }
}
