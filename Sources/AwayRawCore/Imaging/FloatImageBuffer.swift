import Foundation
import Accelerate

/// High-precision RGBA image buffer (float per channel, nominally 0..1 but may exceed
/// during processing). Backs the high-precision RAW pipeline and the 16-bit proxy cache.
/// Channel order is R, G, B, A.
///
/// A class rather than a struct: the pipeline mutates buffers in place from several
/// threads at once, and copy-on-write on a 60-megapixel array would be ruinous.
public final class FloatImageBuffer: @unchecked Sendable {
    public let width: Int
    public let height: Int

    /// Length = width * height * 4, row-major as R,G,B,A.
    public let data: UnsafeMutablePointer<Float>
    public let count: Int

    public init(width: Int, height: Int, zeroed: Bool = true) {
        precondition(width > 0 && height > 0, "Invalid image size")
        self.width = width
        self.height = height
        self.count = width * height * 4
        self.data = UnsafeMutablePointer<Float>.allocate(capacity: count)
        if zeroed { self.data.initialize(repeating: 0, count: count) }
        else { self.data.initialize(repeating: 0, count: count) }
    }

    deinit {
        data.deinitialize(count: count)
        data.deallocate()
    }

    @inline(__always)
    public func index(_ x: Int, _ y: Int) -> Int { (y * width + x) * 4 }

    public func clone() -> FloatImageBuffer {
        let c = FloatImageBuffer(width: width, height: height, zeroed: false)
        c.data.update(from: data, count: count)
        return c
    }

    /// A read-only view over the samples.
    public var buffer: UnsafeMutableBufferPointer<Float> {
        UnsafeMutableBufferPointer(start: data, count: count)
    }
}

// MARK: - Parallel row helpers

/// Runs `body` for every row in [y0, y1) across all cores. The row range exists so a
/// stage that was partially completed elsewhere can hand the remainder over.
@inline(__always)
func parallelRows(_ y0: Int, _ y1: Int, _ body: (Int) -> Void) {
    let n = y1 - y0
    guard n > 0 else { return }
    if n < 8 {
        for y in y0..<y1 { body(y) }
        return
    }
    // Chunk so each concurrentPerform unit is worth its dispatch overhead.
    let cores = max(1, ProcessInfo.processInfo.activeProcessorCount)
    let chunks = min(n, cores * 4)
    let per = (n + chunks - 1) / chunks
    DispatchQueue.concurrentPerform(iterations: chunks) { c in
        let start = y0 + c * per
        let end = min(y1, start + per)
        var y = start
        while y < end { body(y); y += 1 }
    }
}

/// Cancellation is cooperative: a flag the UI flips when a render is superseded.
/// Mirrors the CancellationToken threaded through the C# pipeline.
public final class CancelToken: @unchecked Sendable {
    private var flag: Int32 = 0
    public init() {}

    public func cancel() { OSAtomicOrFlag(&flag) }
    public var isCancelled: Bool { flag != 0 }

    private func OSAtomicOrFlag(_ p: UnsafeMutablePointer<Int32>) { p.pointee = 1 }

    /// Throws `CancellationError` when the render has been superseded.
    public func check() throws {
        if flag != 0 { throw CancellationError() }
    }

    public static let none = CancelToken()
}

// MARK: - .NET-compatible rounding

extension Double {
    /// `Math.Round(x)` as .NET defines it: **banker's rounding**, half to even.
    ///
    /// Swift's `rounded()` is half-away-from-zero, so a plain port silently disagrees on
    /// exact halves. That is not academic — a 1705 px frame cropped to 0.9 gives exactly
    /// 1534.5, and the two rules produce 1534 vs 1535, an off-by-one in the output image.
    /// Every site ported from a C# `Math.Round` uses this.
    @inline(__always)
    public var roundedHalfEven: Double { rounded(.toNearestOrEven) }
}

extension Float {
    /// See `Double.roundedHalfEven`.
    @inline(__always)
    public var roundedHalfEven: Float { rounded(.toNearestOrEven) }
}
