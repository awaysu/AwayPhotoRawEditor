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
