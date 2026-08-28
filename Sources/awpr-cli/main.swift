import Foundation
import AwayRawCore
import CoreGraphics

// Headless diagnostics for the rendering engine — the macOS counterpart of the Windows
// build's --selftest / --exporttest / --shot switches. Everything the GUI does to a photo
// goes through the same AwayRawCore entry points, so these commands exercise the real
// pipeline rather than a parallel copy of it.

let args = Array(CommandLine.arguments.dropFirst())

func usage() -> Never {
    print("""
    awpr-cli — AwayPhotoRawEditor engine diagnostics

      info <image>
          LibRaw availability and sizes, EXIF, camera colour data.

      selftest <image> [report.txt]
          End-to-end: decode → EXIF → cache → pipeline → histogram → XML round-trip.

      render <image> <out.png> [--exposure N] [--contrast N] [--temp K] [--tint N]
                               [--vibrance N] [--saturation N] [--sharpen N] [--nr N]
                               [--vignette N] [--rotate 0|90|180|270] [--preset NAME]
          Render one photo through the full pipeline and write a PNG.

      exporttest <image> <outDir> [report.txt]
          Run the export pipeline end to end.

      bench <image>
          Time each pipeline stage at proxy and full resolution.
    """)
    exit(2)
}

guard let command = args.first else { usage() }

// MARK: - helpers

func fmt(_ d: Double, _ places: Int = 4) -> String {
    String(format: "%.\(places)f", d)
}

func elapsed(_ block: () throws -> Void) rethrows -> Double {
    let t0 = Date()
    try block()
    return Date().timeIntervalSince(t0) * 1000
}

final class Report {
    var lines: [String] = []
    func add(_ s: String = "") {
        print(s)
        lines.append(s)
    }
    func write(to path: String?) {
        guard let path else { return }
        try? lines.joined(separator: "\n").write(toFile: path, atomically: true, encoding: .utf8)
        print("\n報告已寫入 \(path)")
    }
}

func describeCamera(_ c: CameraColorInfo) -> [String] {
    var out: [String] = []
    out.append("  pre_mul : " + c.preMul.map { fmt($0) }.joined(separator: ", "))
    out.append("  cam_mul : " + c.camMul.map { fmt($0) }.joined(separator: ", "))
    out.append("  rgb_cam : ")
    for r in 0..<3 {
        out.append("      " + (0..<3).map { fmt(c.rgbCam[r * 3 + $0]) }.joined(separator: ", "))
    }
    if let shot = ColorScience.asShot(c) {
        // Full precision: this is the number the adjustment XML stores, so it is what a
        // cross-implementation comparison actually needs.
        out.append("  as-shot : \(DotNetXml.string(shot.kelvin)) K, tint \(DotNetXml.string(shot.tint))")
    } else {
        out.append("  as-shot : （無法換算）")
    }
    return out
}

// MARK: - commands

switch command {

case "info":
    guard args.count >= 2 else { usage() }
    let path = args[1]
    let r = Report()
    r.add("檔案      : \(path)")
    r.add("LibRaw    : \(LibRawBridge.available ? LibRawBridge.version : "不可用")")
    r.add("是 RAW    : \(AppPaths.isRaw(path))")

    if AppPaths.isRaw(path), let s = LibRawBridge.readSizes(path) {
        r.add("libraw sizes: raw \(s.rawWidth)x\(s.rawHeight) / visible \(s.width)x\(s.height)"
              + " / margin L\(s.leftMargin) T\(s.topMargin) / flip \(s.flip)")
        if s.rawWidth == s.width && s.rawHeight == s.height {
            r.add("  ⚠️ 可見區等於整塊感光元件 — 此機型沒有裁切表，解出來可能帶遮罩黑邊")
        }
    }
    if let v = ExifReader.readVisibleSize(path: path) {
        r.add("ImageIO 可見尺寸: \(v.width)x\(v.height)")
    }

    let exif = ExifReader.read(path: path)
    r.add("")
    r.add("EXIF")
    r.add("  相機    : \(exif.cameraMake) \(exif.cameraModel)")
    r.add("  鏡頭    : \(exif.lens)")
    r.add("  ISO     : \(exif.iso)   光圈: \(exif.aperture)   快門: \(exif.shutter)")
    r.add("  焦距    : \(exif.focalLength)   曝光補償: \(exif.exposureBias)")
    r.add("  白平衡  : \(exif.whiteBalance)   測光: \(exif.meteringMode)")
    r.add("  拍攝時間: \(exif.dateTaken)")
    r.add("  尺寸    : \(exif.dimensionsDisplay)   檔案: \(exif.fileSizeDisplay)")

    if let cam = LibRawBridge.readCameraColor(path) {
        r.add("")
        r.add("相機色彩資料（線性管線的白平衡矩陣來源）")
        for l in describeCamera(cam) { r.add(l) }
    } else if AppPaths.isRaw(path) {
        r.add("")
        r.add("相機色彩資料: 無（會退回黑體近似）")
    }

case "selftest":
    guard args.count >= 2 else { usage() }
    let path = args[1]
    let reportPath = args.count >= 3 ? args[2] : nil
    let r = Report()
    var failures = 0
    func check(_ name: String, _ ok: Bool, _ detail: String = "") {
        r.add("  [\(ok ? "OK" : "失敗")] \(name)\(detail.isEmpty ? "" : " — " + detail)")
        if !ok { failures += 1 }
    }

    r.add("AwayPhotoRawEditor 引擎自我測試")
    r.add("檔案: \(path)")
    r.add("LibRaw: \(LibRawBridge.available ? LibRawBridge.version : "不可用")")
    r.add("")

    let loader = RawLoader()
    loader.useLibRaw = true
    loader.useHighPrecisionRawPipeline = true

    r.add("[1] 解碼")
    var full: FloatImageBuffer?
    let decodeMs = elapsed { full = loader.decodeFullFloat(path: path) }
    check("全解析度解碼", full != nil,
          full.map { "\($0.width)x\($0.height)，\(Int(decodeMs)) ms，來源=\(loader.lastDecodeSource.rawValue)" } ?? "")
    guard let fullBuf = full else {
        r.add("\n解碼失敗，中止。")
        r.write(to: reportPath)
        exit(1)
    }

    r.add("")
    r.add("[2] EXIF 與相機色彩")
    var exif: ExifData? = ExifReader.read(path: path)
    // Not every file carries a camera model — a converted or stripped DNG legitimately
    // has none — so this only checks that metadata was read at all.
    let gotExif = exif != nil && (!(exif!.cameraModel.isEmpty) || exif!.width > 0)
    check("EXIF 讀取", gotExif,
          exif.map { "\($0.cameraMake) \($0.cameraModel) \($0.dimensionsDisplay)" } ?? "")
    let enriched = loader.enrichCameraColor(path: path, exif: &exif)
    check("相機色彩資料", !AppPaths.isRaw(path) || exif?.camera != nil,
          enriched ? "本次補上" : (exif?.camera != nil ? "已存在" : "無（退回黑體近似）"))

    r.add("")
    r.add("[3] 快取")
    let proxyMs = elapsed { _ = loader.ensureProxyCache(path: path) }
    check("proxy 快取", FileManager.default.fileExists(atPath: AppPaths.proxyPath(path)),
          "\(Int(proxyMs)) ms")
    let thumbMs = elapsed { _ = loader.ensureThumbnailCache(path: path, maxW: 240, maxH: 160) }
    check("縮圖快取", FileManager.default.fileExists(atPath: AppPaths.thumbnailPath(path)),
          "\(Int(thumbMs)) ms")
    let proxy = loader.loadProxyFloat(path: path)
    check("讀回 proxy", proxy != nil, proxy.map { "\($0.width)x\($0.height)" } ?? "")

    r.add("")
    r.add("[4] 管線")
    var adj = ImageAdjustments()
    adj.exposure = 0.5
    adj.contrast = 20
    adj.highlights = -30
    adj.shadows = 25
    adj.temperature = 4200
    adj.tint = 10
    adj.vibrance = 25
    adj.saturation = 10
    adj.sharpening = 30
    adj.noiseReduction = 20
    adj.vignette = 25
    adj.cropX = 0.05; adj.cropY = 0.05; adj.cropWidth = 0.9; adj.cropHeight = 0.9
    adj.cropAngle = 2
    var g = LinearGradient()
    g.exposure = -0.6; g.saturation = 15
    adj.gradients = [g]
    var spot = HealSpot()
    spot.targetX = 0.4; spot.targetY = 0.4
    spot.sourceX = 0.5; spot.sourceY = 0.5
    spot.radiusNorm = 0.02
    adj.healSpots = [spot]

    let ctx = ProcessContext()
    ctx.camera = exif?.camera
    ctx.whiteBalanceReference = .decode

    let source = proxy ?? fullBuf
    var rendered: FloatImageBuffer?
    let renderMs = try elapsed { rendered = try ImageProcessor.applyToFloat(source, adj, ctx) }
    check("完整管線（proxy）", rendered != nil,
          rendered.map { "\($0.width)x\($0.height)，\(Int(renderMs)) ms" } ?? "")

    // Every stage in isolation, so a failure points at one step.
    for (name, mutate) in [
        ("僅曝光/白平衡", { (a: inout ImageAdjustments) in a.exposure = 1; a.temperature = 3200 }),
        ("僅色調曲線", { a in a.contrast = 40; a.highlights = -50; a.shadows = 40 }),
        ("僅降噪+銳利化", { a in a.noiseReduction = 50; a.sharpening = 60 }),
        ("僅漸層", { a in var g = LinearGradient(); g.exposure = -1; a.gradients = [g] }),
        ("僅暗角", { a in a.vignette = 60 }),
        ("僅裁切+角度", { a in a.cropX = 0.1; a.cropWidth = 0.8; a.cropAngle = 5 }),
        ("僅廣角變形", { a in a.distortion = 40 }),
        ("僅旋轉 90°", { a in a.rotation = .r90 }),
    ] as [(String, (inout ImageAdjustments) -> Void)] {
        var one = ImageAdjustments()
        mutate(&one)
        var out: FloatImageBuffer?
        let ms = try elapsed { out = try ImageProcessor.applyToFloat(source, one, ctx) }
        check(name, out != nil, out.map { "\($0.width)x\($0.height)，\(Int(ms)) ms" } ?? "")
    }

    r.add("")
    r.add("[5] 直方圖與均值")
    if let rendered {
        let h = ImageStats.computeHistogram(rendered)
        let sum = h.r.reduce(0, +)
        check("直方圖", sum == rendered.width * rendered.height,
              "峰值 \(h.max)，樣本 \(sum)")
        let m = ImageStats.meanColor(rendered)
        r.add("  平均 RGB: \(fmt(m.r, 1)), \(fmt(m.g, 1)), \(fmt(m.b, 1))")
    }

    r.add("")
    r.add("[6] XML 往返（與 Windows 版同格式）")
    let tmpDir = NSTemporaryDirectory() + "awpr_selftest_\(UUID().uuidString)"
    try? FileManager.default.createDirectory(atPath: tmpDir, withIntermediateDirectories: true)
    let tmpImage = (tmpDir as NSString).appendingPathComponent((path as NSString).lastPathComponent)
    FileManager.default.createFile(atPath: tmpImage, contents: Data())
    AdjustmentXmlStore.save(imagePath: tmpImage, adjustments: adj, exif: exif)
    let reloaded = AdjustmentXmlStore.load(imagePath: tmpImage)
    check("調整值往返", reloaded?.valueEquals(adj) ?? false)
    let reloadedExif = AdjustmentXmlStore.loadExif(imagePath: tmpImage)
    check("EXIF 往返", reloadedExif?.cameraModel == exif?.cameraModel)
    check("相機色彩往返", {
        guard let a = reloadedExif?.camera, let b = exif?.camera else { return exif?.camera == nil }
        return zip(a.rgbCam, b.rgbCam).allSatisfy { abs($0 - $1) < 1e-9 }
    }())
    if let xml = try? String(contentsOfFile: AppPaths.adjustmentXmlPath(tmpImage), encoding: .utf8) {
        r.add("  XML 片段:")
        for line in xml.split(separator: "\n").prefix(8) { r.add("    " + line) }
    }
    try? FileManager.default.removeItem(atPath: tmpDir)

    r.add("")
    r.add("[7] 風格檔")
    for name in PresetProfile.builtInNames {
        var a = ImageAdjustments()
        let ok = PresetStore.apply(name: name, to: &a)
        check("套用「\(name)」", ok, "曝光 \(fmt(a.exposure, 2)) 對比 \(fmt(a.contrast, 0))")
    }

    r.add("")
    r.add(failures == 0 ? "全部通過 ✅" : "有 \(failures) 項失敗 ❌")
    r.write(to: reportPath)
    exit(failures == 0 ? 0 : 1)

case "render":
    guard args.count >= 3 else { usage() }
    let path = args[1]
    let outPath = args[2]
    var adj = ImageAdjustments()
    var i = 3
    while i < args.count {
        let flag = args[i]
        let value = i + 1 < args.count ? args[i + 1] : ""
        let d = Double(value) ?? 0
        switch flag {
        case "--exposure":   adj.exposure = d
        case "--contrast":   adj.contrast = d
        case "--highlights": adj.highlights = d
        case "--shadows":    adj.shadows = d
        case "--whites":     adj.whites = d
        case "--blacks":     adj.blacks = d
        case "--temp":       adj.temperature = d
        case "--tint":       adj.tint = d
        case "--vibrance":   adj.vibrance = d
        case "--saturation": adj.saturation = d
        case "--sharpen":    adj.sharpening = d
        case "--nr":         adj.noiseReduction = d
        case "--vignette":   adj.vignette = d
        case "--distortion": adj.distortion = d
        case "--rotate":     adj.rotation = Rotation(rawValue: Int(d)) ?? .r0
        case "--preset":     _ = PresetStore.apply(name: value, to: &adj)
        default:
            print("未知參數: \(flag)")
            usage()
        }
        i += 2
    }

    let loader = RawLoader()
    loader.useHighPrecisionRawPipeline = true
    guard let src = loader.decodeFullFloat(path: path) else {
        print("無法解碼 \(path)")
        exit(1)
    }
    var exif: ExifData? = ExifReader.read(path: path)
    _ = loader.enrichCameraColor(path: path, exif: &exif)
    let ctx = ProcessContext()
    ctx.camera = exif?.camera
    let t0 = Date()
    let out = try ImageProcessor.applyToFloat(src, adj, ctx)
    let ms = Date().timeIntervalSince(t0) * 1000
    guard ImageIOCodec.write(out, to: outPath, format: .png) else {
        print("寫入失敗")
        exit(1)
    }
    print("已寫入 \(outPath) — \(out.width)x\(out.height)，\(Int(ms)) ms")

case "exporttest":
    guard args.count >= 3 else { usage() }
    let path = args[1]
    let outDir = args[2]
    let reportPath = args.count >= 4 ? args[3] : nil
    let r = Report()
    r.add("匯出測試")
    r.add("來源: \(path)")

    var settings = ExportSettings()
    settings.location = .custom
    settings.customPath = outDir
    settings.useSubFolder = false
    settings.openFinderAfter = false
    settings.maxLongEdge = 2400
    settings.format = .jpeg
    settings.jpegQuality = 92

    let loader = RawLoader()
    loader.useHighPrecisionRawPipeline = true
    let item = PhotoItem(sourcePath: path)

    // Give it something to actually apply.
    var adj = AdjustmentXmlStore.load(imagePath: path) ?? ImageAdjustments()
    adj.exposure = 0.3
    adj.contrast = 15
    adj.vibrance = 20
    var exif: ExifData? = ExifReader.read(path: path)
    _ = loader.enrichCameraColor(path: path, exif: &exif)
    AdjustmentXmlStore.save(imagePath: path, adjustments: adj, exif: exif)

    do {
        let t0 = Date()
        let written = try Exporter.export(items: [item], settings: settings, loader: loader) { p in
            r.add("  \(p.done)/\(p.total) \(p.message)")
        }
        let ms = Date().timeIntervalSince(t0) * 1000
        for w in written {
            let size = (try? FileManager.default.attributesOfItem(atPath: w)[.size] as? NSNumber)??.int64Value ?? 0
            r.add("寫出: \(w)  (\(size / 1024) KB)")
            if let img = ImageIOCodec.loadCGImage(path: w) {
                r.add("  尺寸: \(img.width)x\(img.height)")
            }
            let e = ExifReader.read(path: w)
            r.add("  保留 EXIF: \(e.cameraModel.isEmpty ? "否" : "是（\(e.cameraMake) \(e.cameraModel)）")")
        }
        r.add("耗時 \(Int(ms)) ms")

        // Second pass exercises the _1 de-duplication.
        let again = try Exporter.export(items: [item], settings: settings, loader: loader)
        r.add("再次匯出（測試同名處理）: \(again.first ?? "")")
        r.add("")
        r.add("通過 ✅")
    } catch {
        r.add("失敗: \(error.localizedDescription)")
        r.write(to: reportPath)
        exit(1)
    }
    r.write(to: reportPath)

case "bench":
    guard args.count >= 2 else { usage() }
    let path = args[1]
    let loader = RawLoader()
    loader.useHighPrecisionRawPipeline = true
    print("解碼中…")
    let t0 = Date()
    guard let full = loader.decodeFullFloat(path: path) else { print("無法解碼"); exit(1) }
    print("全解析度解碼 \(full.width)x\(full.height): \(Int(Date().timeIntervalSince(t0) * 1000)) ms")

    var exif: ExifData? = ExifReader.read(path: path)
    _ = loader.enrichCameraColor(path: path, exif: &exif)
    let ctx = ProcessContext()
    ctx.camera = exif?.camera
    let proxy = CacheManager.resizeFloatToMaxDim(full, maxDim: 2560)
    print("proxy \(proxy.width)x\(proxy.height)")

    let cases: [(String, ImageAdjustments)] = {
        var out: [(String, ImageAdjustments)] = []
        func mk(_ n: String, _ f: (inout ImageAdjustments) -> Void) {
            var a = ImageAdjustments(); f(&a); out.append((n, a))
        }
        mk("白平衡+曝光") { $0.exposure = 1; $0.temperature = 3200 }
        mk("色調曲線") { $0.contrast = 40; $0.highlights = -50; $0.shadows = 40 }
        mk("降噪+銳利化") { $0.noiseReduction = 50; $0.sharpening = 60 }
        mk("漸層") { var g = LinearGradient(); g.exposure = -1; $0.gradients = [g] }
        mk("暗角") { $0.vignette = 60 }
        mk("裁切+角度") { $0.cropX = 0.1; $0.cropWidth = 0.8; $0.cropAngle = 5 }
        mk("廣角變形") { $0.distortion = 40 }
        mk("綜合") {
            $0.exposure = 0.5; $0.contrast = 20; $0.highlights = -30; $0.shadows = 25
            $0.temperature = 4200; $0.vibrance = 25; $0.sharpening = 30; $0.noiseReduction = 20
            $0.vignette = 25; $0.cropX = 0.05; $0.cropWidth = 0.9; $0.cropAngle = 2
        }
        return out
    }()

    // Pad in characters, not bytes: %-18s counts UTF-8 bytes and would split the CJK
    // labels mid-character.
    func pad(_ s: String, _ width: Int) -> String {
        // CJK glyphs occupy two terminal columns.
        let cols = s.unicodeScalars.reduce(0) { $0 + ($1.value > 0x2E80 ? 2 : 1) }
        return s + String(repeating: " ", count: max(0, width - cols))
    }
    func padLeft(_ s: String, _ width: Int) -> String {
        String(repeating: " ", count: max(0, width - s.count)) + s
    }

    print("")
    print(pad("階段", 20) + padLeft("proxy", 12) + padLeft("全解析度", 12))
    for (name, adj) in cases {
        let pms = try elapsed { _ = try ImageProcessor.applyToFloat(proxy, adj, ctx) }
        let fms = try elapsed { _ = try ImageProcessor.applyToFloat(full, adj, ctx) }
        print(pad(name, 20) + padLeft("\(Int(pms)) ms", 12) + padLeft("\(Int(fms)) ms", 12))
    }

default:
    usage()
}
