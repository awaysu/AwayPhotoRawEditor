import Foundation
import CryptoKit
import AwayRawCore

/// `hashtest <img> <report>` — a cross-implementation fingerprint of the colour pipeline.
///
/// The 14 cases, the source preparation, the SHA input and the number formatting all
/// mirror the C# port's `Diagnostics/PipelineHash.cs` (which in turn mirrors the Windows
/// build's `GpuParity.Cases()`), so the two reports can be diffed line for line.
///
/// ⚠️ **Byte-identical SHAs are not the pass criterion.** The pipeline calls
/// `pow`/`sin`/`cos`/`log` in about 18 places, and the C and Swift runtimes hand those to
/// the platform libm; only + - * / and sqrt are required to be correctly rounded. The
/// thresholds carried over from GpuParity are:
///
/// * SHA equal → bit-identical, ideal
/// * SHA differs but samples agree to < 1e-5 → libm ULP noise, acceptable
/// * differences of 1e-3 or more, or an 8-bit channel差 >= 2 → the maths has diverged
enum PipelineHash {

    /// `gputest <img> [report]` — CPU vs GPU on the same 14 cases, plus three heal
    /// cases that only exist here (see `healCases`).
    ///
    /// Byte-identical output is *not* expected: the geometry stages compute in `double`
    /// on the CPU and Metal has no double, and libm and the Metal standard library round
    /// transcendentals differently.
    ///
    /// The pass criterion is the **8-bit** one the Windows build's GpuParity settled on,
    /// because that is what actually reaches a file:
    ///
    /// * no 8-bit channel may differ by 2 or more — that would mean the maths diverged
    /// * at most 0.07% of channels may differ by 1
    ///
    /// The float difference is reported for information but is not itself the test. A
    /// resample coordinate of magnitude ~2000 carries about 1e-4 of float rounding, and
    /// bilinear interpolation turns that into a similar difference in value; the Windows
    /// build measured 2.9e-4 on the same stage at full resolution. Judging that by a
    /// float threshold would fail a difference no one can see.
    static func gpuParity(imagePath: String, reportPath: String?) -> Int32 {
        var lines: [String] = []
        func line(_ s: String = "") { print(s); lines.append(s) }

        line("=== CPU / GPU 對照 ===")
        line("Metal   : \(MetalPipeline.shared.statusText)")
        line("可用    : \(MetalPipeline.shared.available)")
        line("尺寸上限: \(MetalPipeline.shared.maxPixels / 1_000_000) MP")
        line("目標檔案: \((imagePath as NSString).lastPathComponent)")

        guard MetalPipeline.shared.available else {
            line("!! Metal 不可用，無法比較")
            write(lines, to: reportPath)
            return 1
        }

        guard let (source, cam) = loadSource(imagePath, line) else {
            write(lines, to: reportPath)
            return 1
        }
        line("來源    : \(source.width) x \(source.height)")
        line("")

        var failures = 0
        var worstDiff = 0.0
        // The 14 shared cases, plus the heal cases that exist only here: hashtest's list
        // must stay identical to the C# one, but the GPU path hands healing to the CPU on
        // a shared buffer (download → fix in place → continue), and that hand-off is
        // exactly what needs a check.
        for (name, adj) in cases() + healCases() {
            var ranOnGpu = false
            func render(gpu: Bool) -> FloatImageBuffer? {
                let ctx = ProcessContext()
                ctx.camera = cam
                ctx.whiteBalanceReference = .decode
                ctx.useGpu = gpu
                ctx.forceCpu = !gpu
                let out = try? ImageProcessor.applyToFloat(source, adj, ctx)
                if gpu { ranOnGpu = ctx.usedGpu }
                return out
            }
            let t0 = Date()
            guard let cpu = render(gpu: false) else { line("[\(name)] CPU 失敗"); failures += 1; continue }
            let cpuMs = Date().timeIntervalSince(t0) * 1000
            let t1 = Date()
            guard let gpu = render(gpu: true) else { line("[\(name)] GPU 失敗"); failures += 1; continue }
            let gpuMs = Date().timeIntervalSince(t1) * 1000

            guard ranOnGpu else {
                // Above the size cap the GPU path is declined, and comparing CPU with CPU
                // would pass vacuously.
                line("[\(name)] ⚠️ 未實際使用 GPU（超過尺寸上限或裝置拒絕），略過比較")
                continue
            }
            guard cpu.width == gpu.width, cpu.height == gpu.height else {
                line("[\(name)] ❌ 尺寸不同 CPU=\(cpu.width)x\(cpu.height) GPU=\(gpu.width)x\(gpu.height)")
                failures += 1
                continue
            }

            // A heal case where the spots changed nothing would agree trivially — both
            // paths run the same CPU routine — so prove the spots actually touched pixels.
            var healedPx = -1
            if !adj.healSpots.isEmpty {
                var bare = adj
                bare.healSpots = []
                let ctx = ProcessContext()
                ctx.camera = cam
                ctx.whiteBalanceReference = .decode
                ctx.forceCpu = true
                if let plain = try? ImageProcessor.applyToFloat(source, bare, ctx),
                   plain.width == cpu.width, plain.height == cpu.height {
                    healedPx = 0
                    for px in 0..<(cpu.width * cpu.height) {
                        let i = px * 4
                        if plain.data[i] != cpu.data[i] || plain.data[i + 1] != cpu.data[i + 1]
                            || plain.data[i + 2] != cpu.data[i + 2] { healedPx += 1 }
                    }
                }
            }

            var maxDiff: Float = 0
            var byteDiff2 = 0            // channels differing by >= 2 after 8-bit quantisation
            var byteDiff1 = 0
            for i in 0..<(cpu.width * cpu.height * 4) where i % 4 != 3 {
                let d = abs(cpu.data[i] - gpu.data[i])
                if d > maxDiff { maxDiff = d }
                let bd = abs(Int(toByte(cpu.data[i])) - Int(toByte(gpu.data[i])))
                if bd >= 2 { byteDiff2 += 1 } else if bd == 1 { byteDiff1 += 1 }
            }
            worstDiff = max(worstDiff, Double(maxDiff))
            let total = cpu.width * cpu.height * 3
            let pct1 = Double(byteDiff1) / Double(total) * 100
            let healOk = adj.healSpots.isEmpty || healedPx > 0
            let ok = byteDiff2 == 0 && pct1 <= 0.07 && healOk
            if !ok { failures += 1 }
            line("[\(name)]")
            line("  \(ok ? "✅" : "❌")  最大差 \(String(format: "%.2e", Double(maxDiff)))  " +
                 "8-bit 差1 \(String(format: "%.3f", pct1))%  差≥2 \(byteDiff2)")
            if !adj.healSpots.isEmpty {
                line("  修護實際改動 \(healedPx < 0 ? "（無法比對）" : "\(healedPx) px")" +
                     (healOk ? "" : "  ❌ 修護沒有改到任何像素"))
            }
            line("  CPU \(Int(cpuMs)) ms  →  GPU \(Int(gpuMs)) ms  " +
                 "(\(String(format: "%.1f", cpuMs / max(gpuMs, 0.001)))×)")
        }

        line("")
        line("整體最大差: \(String(format: "%.2e", worstDiff))")
        line(failures == 0 ? "全部通過 ✅" : "有 \(failures) 項超出容許範圍 ❌")
        write(lines, to: reportPath)
        return failures == 0 ? 0 : 1
    }

    /// Shared source preparation for both fingerprint modes.
    static func loadSource(_ imagePath: String,
                           _ line: (String) -> Void) -> (FloatImageBuffer, CameraColorInfo?)? {
        if AppPaths.isRaw(imagePath) {
            let cam = LibRawBridge.readCameraColor(imagePath)
            guard let full = LibRawBridge.decodeFull(imagePath, bps: 16) else {
                line("!! RAW 解碼失敗")
                return nil
            }
            return (CacheManager.resizeFloatToMaxDim(full, maxDim: 2560), cam)
        }
        guard let px = ImageIOCodec.loadFloat(path: imagePath) else {
            line("!! 影像載入失敗")
            return nil
        }
        return (px, nil)
    }

    static func run(imagePath: String, reportPath: String?) -> Int32 {
        var lines: [String] = []
        func line(_ s: String = "") { print(s); lines.append(s) }

        line("=== AwayPhotoRawEditor 色彩管線指紋 ===")
        line("平台    : \(ProcessInfo.processInfo.operatingSystemVersionString) arm64/x86_64 universal")
        line("執行階段: Swift / AwayRawCore")
        line("目標檔案: \((imagePath as NSString).lastPathComponent)")

        var cam: CameraColorInfo?
        var src: FloatImageBuffer?

        if AppPaths.isRaw(imagePath) {
            cam = LibRawBridge.readCameraColor(imagePath)
            // Deliberately no expectedVisible here, matching the C# reference's
            // DecodeToFloat(path) call. The mask-border trim is a real improvement in the
            // app, but for this comparison both sides must start from identical pixels.
            guard let full = LibRawBridge.decodeFull(imagePath, bps: 16) else {
                line("!! RAW 解碼失敗")
                write(lines, to: reportPath)
                return 1
            }
            src = CacheManager.resizeFloatToMaxDim(full, maxDim: 2560)
        } else {
            guard let px = ImageIOCodec.loadFloat(path: imagePath) else {
                line("!! 影像載入失敗")
                write(lines, to: reportPath)
                return 1
            }
            src = px
        }

        guard let source = src else { return 1 }
        line("來源    : \(source.width) x \(source.height)")
        let valid = cam?.isValid ?? false
        line("相機色彩: \(valid ? "有（矩陣白平衡）" : "無（黑體近似）")")
        if let c = cam, valid {
            line("          pre_mul[\(f4(c.preMul[0])),\(f4(c.preMul[1])),\(f4(c.preMul[2]))] " +
                 "cam_mul[\(f4(c.camMul[0])),\(f4(c.camMul[1])),\(f4(c.camMul[2]))]")
        }
        line("")

        for (name, adj) in cases() {
            let ctx = ProcessContext()
            ctx.camera = cam
            ctx.whiteBalanceReference = .decode
            // The fingerprint is of the CPU reference: it is what gets compared against
            // the other implementation, and the GPU is checked separately by `gputest`.
            ctx.forceCpu = true
            guard let out = try? ImageProcessor.applyToFloat(source, adj, ctx) else {
                line("[\(name)]")
                line("  !! 算圖失敗")
                continue
            }
            line("[\(name)]")
            line("  尺寸    : \(out.width) x \(out.height)")
            line("  8bitSHA : \(sha8(out))")
            let (mr, mg, mb) = means(out)
            line("  通道均值: R=\(f9(mr)) G=\(f9(mg)) B=\(f9(mb))")
            line("  取樣點  : \(samples(out))")
            line("")
        }

        write(lines, to: reportPath)
        return 0
    }

    static func write(_ lines: [String], to path: String?) {
        guard let path else { return }
        try? lines.joined(separator: "\n").appending("\n")
            .write(toFile: path, atomically: true, encoding: .utf8)
        print("報告已寫出: \(path)")
    }

    // ---- formatting (matched to .NET's F4 / F9 / G9) ----------------------

    static func f4(_ v: Double) -> String { String(format: "%.4f", v) }
    static func f9(_ v: Double) -> String { String(format: "%.9f", v) }

    /// .NET's "G9": up to 9 significant digits, trailing zeros trimmed.
    static func g9(_ v: Float) -> String {
        var s = String(format: "%.9g", Double(v))
        if let i = s.firstIndex(of: "e") { s.replaceSubrange(i...i, with: "E") }
        return s
    }

    /// SHA-256 of the 8-bit output, over the same BGRA bytes the exporter would write.
    static func sha8(_ buf: FloatImageBuffer) -> String {
        var bytes = [UInt8](repeating: 0, count: buf.width * buf.height * 4)
        let d = buf.data
        bytes.withUnsafeMutableBufferPointer { bp in
            let b = bp.baseAddress!
            for i in stride(from: 0, to: buf.width * buf.height * 4, by: 4) {
                b[i]     = toByte(d[i + 2])   // B
                b[i + 1] = toByte(d[i + 1])   // G
                b[i + 2] = toByte(d[i])       // R
                b[i + 3] = toByte(d[i + 3])   // A
            }
        }
        let digest = SHA256.hash(data: Data(bytes))
        return String(digest.map { String(format: "%02X", $0) }.joined().prefix(32))
    }

    @inline(__always)
    static func toByte(_ v: Float) -> UInt8 {
        let i = Int(v * 255 + 0.5)
        return UInt8(i < 0 ? 0 : (i > 255 ? 255 : i))
    }

    static func means(_ b: FloatImageBuffer) -> (Double, Double, Double) {
        var sr = 0.0, sg = 0.0, sb = 0.0
        let d = b.data
        let n = b.width * b.height
        for i in stride(from: 0, to: n * 4, by: 4) {
            sr += Double(d[i]); sg += Double(d[i + 1]); sb += Double(d[i + 2])
        }
        return (sr / Double(n), sg / Double(n), sb / Double(n))
    }

    /// Fixed normalized sample positions at full float precision. When the SHAs differ,
    /// these are what separate ULP noise from a genuine divergence.
    static func samples(_ b: FloatImageBuffer) -> String {
        let pts: [(Double, Double)] = [(0.13, 0.21), (0.5, 0.5), (0.77, 0.34), (0.29, 0.86)]
        return pts.map { (u, v) in
            let x = min(max(Int(u * Double(b.width)), 0), b.width - 1)
            let y = min(max(Int(v * Double(b.height)), 0), b.height - 1)
            let o = b.index(x, y)
            return "(\(g9(b.data[o])),\(g9(b.data[o + 1])),\(g9(b.data[o + 2])))"
        }.joined(separator: " ")
    }

    // ---- the 14 cases ----------------------------------------------------
    // ⚠️ Must stay word for word identical to the C# port's Cases() and the Windows
    // build's GpuParity.Cases(), or the comparison loses its meaning.

    static func cases() -> [(String, ImageAdjustments)] {
        var out: [(String, ImageAdjustments)] = []

        out.append(("v1 預設（LUT 恆等 + 白平衡中性）", ImageAdjustments()))

        var legacy = ImageAdjustments()
        legacy.exposure = 0.6; legacy.temperature = 4200; legacy.tint = 15; legacy.contrast = 20
        legacy.pipelineVersion = 0
        out.append(("舊版 曝光/色溫/色調/對比", legacy))

        var a3 = ImageAdjustments()
        a3.exposure = 1.2; a3.temperature = 3600; a3.tint = -20
        out.append(("v1 曝光/色溫/色調", a3))

        var a4 = ImageAdjustments()
        a4.contrast = 35; a4.highlights = -40; a4.shadows = 30; a4.whites = 15; a4.blacks = -20
        out.append(("v1 色調曲線全開", a4))

        var a5 = ImageAdjustments()
        a5.vibrance = 40; a5.saturation = -25
        out.append(("鮮豔度/飽和度", a5))

        var a6 = ImageAdjustments()
        a6.noiseReduction = 60; a6.sharpening = 70
        out.append(("降噪 + 銳利化", a6))

        var a7 = ImageAdjustments()
        a7.sharpening = -50
        out.append(("柔化（負銳利度）", a7))

        out.append(("漸層 ×2", withGradients(ImageAdjustments())))

        var a9 = ImageAdjustments()
        a9.sharpening = 30
        out.append(("漸層 + 銳利化（漸層獨立 pass）", withGradients(a9)))

        var a10 = ImageAdjustments()
        a10.vignette = 60
        out.append(("暗角", a10))

        var a11 = ImageAdjustments()
        a11.cropX = 0.1; a11.cropY = 0.15; a11.cropWidth = 0.7; a11.cropHeight = 0.6
        a11.cropAngle = 7
        out.append(("裁切 + 角度", a11))

        var a12 = ImageAdjustments()
        a12.distortion = 40
        out.append(("廣角變形", a12))

        var a13 = ImageAdjustments()
        a13.rotation = .r90
        a13.cropX = 0.05; a13.cropY = 0.05; a13.cropWidth = 0.9; a13.cropHeight = 0.8
        out.append(("旋轉 90 + 裁切", a13))

        out.append(("綜合", combined()))
        return out
    }

    // ---- gputest-only: heal ----------------------------------------------
    // Not part of the 14 (hashtest must match the C# list), so they are appended only
    // by gpuParity. The spots sit where a 2560-long-edge proxy still has content, and the
    // radii are large enough (0.03 × long edge ≈ 77 px) that a mistake would show.

    static func healCases() -> [(String, ImageAdjustments)] {
        var out: [(String, ImageAdjustments)] = []

        var clone = HealSpot()
        clone.targetX = 0.35; clone.targetY = 0.45
        clone.sourceX = 0.55; clone.sourceY = 0.40
        clone.radiusNorm = 0.03

        var inpaint = HealSpot()
        inpaint.targetX = 0.65; inpaint.targetY = 0.6
        inpaint.radiusNorm = 0.025
        inpaint.useInpaint = true

        var h1 = ImageAdjustments()
        h1.healSpots = [clone]
        out.append(("修護 複製（僅 GPU 對照）", h1))

        var h2 = ImageAdjustments()
        h2.healSpots = [inpaint]
        out.append(("修護 填補（僅 GPU 對照）", h2))

        // Heal in the middle of a full pipeline: stages before it must be flushed to the
        // CPU, stages after it must see the edited pixels.
        var h3 = combined()
        h3.healSpots = [clone, inpaint]
        out.append(("綜合 + 修護 ×2（僅 GPU 對照）", h3))
        return out
    }

    static func combined() -> ImageAdjustments {
        var a = ImageAdjustments()
        a.exposure = 0.4; a.temperature = 6200; a.tint = 8; a.contrast = 15
        a.highlights = -25; a.shadows = 20
        a.vibrance = 20; a.saturation = 5
        a.noiseReduction = 30; a.sharpening = 40; a.vignette = 35
        a.cropX = 0.05; a.cropY = 0.05; a.cropWidth = 0.85; a.cropHeight = 0.85
        a.cropAngle = -3; a.distortion = -20
        return withGradients(a)
    }

    static func withGradients(_ a: ImageAdjustments) -> ImageAdjustments {
        var a = a
        var g1 = LinearGradient()
        g1.centerX = 0.5; g1.centerY = 0.3; g1.angle = 0; g1.range = 0.25
        g1.exposure = -0.8; g1.contrast = 10; g1.saturation = -20
        var g2 = LinearGradient()
        g2.centerX = 0.4; g2.centerY = 0.7; g2.angle = 35; g2.range = 0.2
        g2.exposure = 0.5; g2.highlights = -30; g2.shadows = 20
        a.gradients = [g1, g2]
        return a
    }
}
