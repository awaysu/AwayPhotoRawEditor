import Foundation
import Metal

/// Device, shader library and pipeline states for the GPU renderer.
///
/// The whole image lives on the GPU for the duration of a render (Apple Silicon has
/// unified memory, so "upload" is a shared-storage write rather than a copy across PCIe),
/// and comes back once at the end. Anything that fails — no device, a shader that will not
/// compile, an image larger than the device's buffer limit — leaves `available` false or
/// throws, and the caller silently uses the CPU path.
public final class MetalPipeline: @unchecked Sendable {

    public static let shared = MetalPipeline()

    public let device: MTLDevice?
    private let queue: MTLCommandQueue?
    private var states: [String: MTLComputePipelineState] = [:]

    /// The transfer curves and their sizes, uploaded once and reused by every render.
    private var decodeLutBuffer: MTLBuffer?
    private var encodeLutBuffer: MTLBuffer?

    /// Set once a render has failed in a way that suggests the device is unusable, so the
    /// app stops paying the cost of trying. Mirrors the Windows build's `_broken`.
    private var broken = false
    private var consecutiveFailures = 0

    /// Diagnostics / settings display.
    public private(set) var statusText: String = "未初始化"

    public var available: Bool { device != nil && queue != nil && !broken && !states.isEmpty }

    /// Largest image (in pixels) the GPU path will take on.
    ///
    /// Two limits apply and the smaller wins. The device's buffer limit is the hard one:
    /// three full copies coexist at the worst moment (image, geometry destination, blur
    /// scratch), so a third of one buffer's capacity.
    ///
    /// The practical limit is much lower, and it is a measured one. At 66 megapixels each
    /// copy is about 1 GB; the stages are then purely memory-bound, the GPU stops winning,
    /// and once the working set approaches physical memory the timings fall apart
    /// (a single vignette pass measured 8.4 s against 0.43 s on the CPU — that is paging,
    /// not compute). Below the cap the GPU is a consistent 1.5-6x. The cap sits well above
    /// any proxy (a 2560-long-edge proxy is about 4.4 MP) so interactive editing — the
    /// case that actually needs the speed — always qualifies, while a full-resolution
    /// export quietly uses the CPU, where it is competitive and predictable.
    public private(set) var maxPixels: Int = 0

    /// The measured ceiling described on `maxPixels`.
    public static let practicalPixelCap = 16_000_000

    private init() {
        guard let dev = MTLCreateSystemDefaultDevice() else {
            device = nil; queue = nil
            statusText = "找不到 Metal 裝置"
            return
        }
        device = dev
        queue = dev.makeCommandQueue()
        guard queue != nil else {
            statusText = "無法建立 Metal 命令佇列"
            return
        }

        do {
            let opts = MTLCompileOptions()
            // The shaders mirror CPU maths exactly; fast-math would licence the compiler
            // to reassociate and contract operations and quietly break that.
            // mathMode arrived in macOS 15, and fastMathEnabled is its deprecated
            // predecessor — set whichever this OS understands.
            if #available(macOS 15.0, *) {
                opts.mathMode = .safe
            } else {
                opts.fastMathEnabled = false
            }
            let library = try dev.makeLibrary(source: MetalShaders.source, options: opts)
            for name in ["pixelStage", "blurH", "blurV", "blurCombine", "resample", "rotate90"] {
                guard let fn = library.makeFunction(name: name) else {
                    statusText = "找不到 kernel：\(name)"
                    states.removeAll()
                    return
                }
                states[name] = try dev.makeComputePipelineState(function: fn)
            }
        } catch {
            statusText = "Metal shader 編譯失敗：\(error.localizedDescription)"
            states.removeAll()
            return
        }

        let deviceCap = Int(dev.maxBufferLength) / MemoryLayout<Float>.size / 4 / 3
        maxPixels = max(0, min(deviceCap, Self.practicalPixelCap))

        decodeLutBuffer = makeLutBuffer(ColorScience.decodeTable)
        encodeLutBuffer = makeLutBuffer(ColorScience.encodeTable)
        if decodeLutBuffer == nil || encodeLutBuffer == nil {
            statusText = "無法建立轉換曲線緩衝區"
            states.removeAll()
            return
        }

        statusText = "\(dev.name)（\(dev.hasUnifiedMemory ? "統一記憶體" : "獨立記憶體")）"
    }

    private func makeLutBuffer(_ values: [Float]) -> MTLBuffer? {
        values.withUnsafeBytes { raw in
            device?.makeBuffer(bytes: raw.baseAddress!, length: raw.count,
                               options: .storageModeShared)
        }
    }

    var decodeLut: MTLBuffer? { decodeLutBuffer }
    var encodeLut: MTLBuffer? { encodeLutBuffer }

    func state(_ name: String) -> MTLComputePipelineState? { states[name] }

    func makeCommandBuffer() -> MTLCommandBuffer? { queue?.makeCommandBuffer() }

    /// Can this image be rendered on the GPU at all?
    public func canHost(width: Int, height: Int) -> Bool {
        available && maxPixels > 0 && width * height <= maxPixels
    }

    /// A shared-storage buffer holding the image, so the CPU can read the result back
    /// without an explicit blit.
    func makeImageBuffer(_ buf: FloatImageBuffer) -> MTLBuffer? {
        device?.makeBuffer(bytes: buf.data,
                           length: buf.count * MemoryLayout<Float>.size,
                           options: .storageModeShared)
    }

    func makeEmptyBuffer(width: Int, height: Int) -> MTLBuffer? {
        device?.makeBuffer(length: width * height * 4 * MemoryLayout<Float>.size,
                           options: .storageModeShared)
    }

    // ---- scratch pool ----------------------------------------------------
    // A 66-megapixel frame is a 1 GB buffer, and a render can want three of them at once
    // (image, geometry destination, blur scratch). Allocating those per stage dominated
    // full-resolution timings — first touch of fresh pages costs more than the kernels do.
    // They are recycled by byte length instead.

    private var pool: [Int: [MTLBuffer]] = [:]
    private let poolLock = NSLock()
    /// Keep at most this much memory parked between renders. Sized for a handful of
    /// proxy-scale buffers, not for full-resolution frames.
    private let poolByteLimit = 512 * 1024 * 1024

    func borrowBuffer(width: Int, height: Int) -> MTLBuffer? {
        let length = width * height * 4 * MemoryLayout<Float>.size
        poolLock.lock()
        if var list = pool[length], let b = list.popLast() {
            pool[length] = list
            poolLock.unlock()
            return b
        }
        poolLock.unlock()
        return device?.makeBuffer(length: length, options: .storageModeShared)
    }

    func returnBuffer(_ buffer: MTLBuffer) {
        poolLock.lock()
        defer { poolLock.unlock() }
        var held = 0
        for (len, list) in pool { held += len * list.count }
        guard held + buffer.length <= poolByteLimit else { return }
        pool[buffer.length, default: []].append(buffer)
    }

    /// Drop everything parked in the pool (settings change, or memory pressure).
    public func flushPool() {
        poolLock.lock()
        pool.removeAll()
        poolLock.unlock()
    }

    /// Dispatch a 2-D kernel over `width` x `height`.
    func dispatch(_ encoder: MTLComputeCommandEncoder, _ state: MTLComputePipelineState,
                  width: Int, height: Int) {
        encoder.setComputePipelineState(state)
        let w = min(state.threadExecutionWidth, width)
        let h = min(max(state.maxTotalThreadsPerThreadgroup / max(1, w), 1), height)
        let threadsPerGroup = MTLSize(width: max(1, w), height: max(1, h), depth: 1)
        // Non-uniform threadgroups are supported on every Metal 2 device we target, so
        // the kernels' own bounds checks are belt-and-braces rather than load-bearing.
        encoder.dispatchThreads(MTLSize(width: width, height: height, depth: 1),
                                threadsPerThreadgroup: threadsPerGroup)
    }

    func reportSuccess() { consecutiveFailures = 0 }

    /// Three failures in a row is treated as a broken device and the app stays on the CPU
    /// for the rest of the session.
    func reportFailure(_ error: Error) {
        consecutiveFailures += 1
        if consecutiveFailures >= 3 {
            broken = true
            statusText = "GPU 連續失敗，本次執行改用 CPU：\(error.localizedDescription)"
        }
    }
}
