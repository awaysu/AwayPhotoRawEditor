import Foundation

/// Root of a per-photo .rawpipe.xml document.
public struct RawPipeDocument {
    /// True when written by `ensureDefault` and never edited (a placeholder).
    public var isPlaceholder: Bool = false
    /// Rendering maths for this photo (see `ImageAdjustments.pipelineVersion`).
    /// Absent in XMLs written before v1.0.15 → reads back as 0 = legacy.
    public var pipelineVersion: Int = 0
    public var adjustments = ImageAdjustments()
    public var exif: ExifData?
    public init() {}
}

/// Persists `ImageAdjustments` (and cached EXIF) to RAW_TEMP/{file}.rawpipe.xml
/// (or .copyN.rawpipe.xml for virtual copies), in the same format the Windows build
/// reads and writes.
public enum AdjustmentXmlStore {

    // ---- serialization ---------------------------------------------------

    /// Element order mirrors the C# property declaration order — XmlSerializer is
    /// order-sensitive when it reads, so this has to stay in step.
    static func encode(_ a: ImageAdjustments) -> XmlNode {
        let n = XmlNode("Adjustments")
        n.add("Exposure", a.exposure)
        n.add("Contrast", a.contrast)
        n.add("Highlights", a.highlights)
        n.add("Shadows", a.shadows)
        n.add("Whites", a.whites)
        n.add("Blacks", a.blacks)
        n.add("Temperature", a.temperature)
        n.add("Tint", a.tint)
        n.add("Vibrance", a.vibrance)
        n.add("Saturation", a.saturation)
        n.add("Sharpening", a.sharpening)
        n.add("NoiseReduction", a.noiseReduction)
        n.add("Vignette", a.vignette)
        n.add("Distortion", a.distortion)
        n.add("CropAspectRatio", a.cropAspectRatio)
        n.add("CropAngle", a.cropAngle)
        n.add("CropX", a.cropX)
        n.add("CropY", a.cropY)
        n.add("CropWidth", a.cropWidth)
        n.add("CropHeight", a.cropHeight)
        n.add("Rotation", a.rotation.xmlName)

        let g = n.add(XmlNode("Gradients"))
        for gr in a.gradients {
            let e = g.add(XmlNode("LinearGradient"))
            e.add("CenterX", gr.centerX)
            e.add("CenterY", gr.centerY)
            e.add("Angle", gr.angle)
            e.add("Range", gr.range)
            e.add("Exposure", gr.exposure)
            e.add("Contrast", gr.contrast)
            e.add("Highlights", gr.highlights)
            e.add("Shadows", gr.shadows)
            e.add("Saturation", gr.saturation)
        }

        n.add("HealSize", a.healSize)

        let h = n.add(XmlNode("HealSpots"))
        for s in a.healSpots {
            let e = h.add(XmlNode("HealSpot"))
            e.add("TargetX", s.targetX)
            e.add("TargetY", s.targetY)
            e.add("SourceX", s.sourceX)
            e.add("SourceY", s.sourceY)
            e.add("Radius", s.radius)
            e.add("RadiusNorm", s.radiusNorm)
            e.add("UseInpaint", s.useInpaint)
        }
        return n
    }

    static func decodeAdjustments(_ n: XmlNode) -> ImageAdjustments {
        var a = ImageAdjustments()
        a.exposure = n.double("Exposure", default: a.exposure)
        a.contrast = n.double("Contrast", default: a.contrast)
        a.highlights = n.double("Highlights", default: a.highlights)
        a.shadows = n.double("Shadows", default: a.shadows)
        a.whites = n.double("Whites", default: a.whites)
        a.blacks = n.double("Blacks", default: a.blacks)
        a.temperature = n.double("Temperature", default: a.temperature)
        a.tint = n.double("Tint", default: a.tint)
        a.vibrance = n.double("Vibrance", default: a.vibrance)
        a.saturation = n.double("Saturation", default: a.saturation)
        a.sharpening = n.double("Sharpening", default: a.sharpening)
        a.noiseReduction = n.double("NoiseReduction", default: a.noiseReduction)
        a.vignette = n.double("Vignette", default: a.vignette)
        a.distortion = n.double("Distortion", default: a.distortion)
        a.cropAspectRatio = n.string("CropAspectRatio", default: a.cropAspectRatio)
        a.cropAngle = n.double("CropAngle", default: a.cropAngle)
        a.cropX = n.double("CropX", default: a.cropX)
        a.cropY = n.double("CropY", default: a.cropY)
        a.cropWidth = n.double("CropWidth", default: a.cropWidth)
        a.cropHeight = n.double("CropHeight", default: a.cropHeight)
        if let r = n.string("Rotation"), let rot = Rotation(xmlName: r) { a.rotation = rot }

        if let g = n.child("Gradients") {
            a.gradients = g.childrenNamed("LinearGradient").map { e in
                var gr = LinearGradient()
                gr.centerX = e.double("CenterX", default: gr.centerX)
                gr.centerY = e.double("CenterY", default: gr.centerY)
                gr.angle = e.double("Angle", default: gr.angle)
                gr.range = e.double("Range", default: gr.range)
                gr.exposure = e.double("Exposure", default: gr.exposure)
                gr.contrast = e.double("Contrast", default: gr.contrast)
                gr.highlights = e.double("Highlights", default: gr.highlights)
                gr.shadows = e.double("Shadows", default: gr.shadows)
                gr.saturation = e.double("Saturation", default: gr.saturation)
                return gr
            }
        }

        a.healSize = n.double("HealSize", default: a.healSize)

        if let h = n.child("HealSpots") {
            a.healSpots = h.childrenNamed("HealSpot").map { e in
                var s = HealSpot()
                s.targetX = e.double("TargetX", default: s.targetX)
                s.targetY = e.double("TargetY", default: s.targetY)
                s.sourceX = e.double("SourceX", default: s.sourceX)
                s.sourceY = e.double("SourceY", default: s.sourceY)
                s.radius = e.double("Radius", default: s.radius)
                s.radiusNorm = e.double("RadiusNorm", default: s.radiusNorm)
                s.useInpaint = e.bool("UseInpaint", default: s.useInpaint)
                return s
            }
        }
        return a
    }

    static func encode(_ e: ExifData) -> XmlNode {
        let n = XmlNode("Exif")
        n.add("CameraMake", e.cameraMake)
        n.add("CameraModel", e.cameraModel)
        n.add("Lens", e.lens)
        n.add("ISO", e.iso)
        n.add("Aperture", e.aperture)
        n.add("Shutter", e.shutter)
        n.add("FocalLength", e.focalLength)
        n.add("ExposureBias", e.exposureBias)
        n.add("WhiteBalance", e.whiteBalance)
        n.add("MeteringMode", e.meteringMode)
        n.add("ColorTemperature", e.colorTemperature)
        n.add("Tint", e.tint)
        if let c = e.camera {
            let cn = n.add(XmlNode("Camera"))
            cn.addDoubleArray("PreMul", c.preMul)
            cn.addDoubleArray("CamMul", c.camMul)
            cn.addDoubleArray("RgbCam", c.rgbCam)
        }
        n.add("DateTaken", e.dateTaken)
        n.add("Width", e.width)
        n.add("Height", e.height)
        n.add("FileSize", String(e.fileSize))
        n.add("FilePath", e.filePath)
        return n
    }

    static func decodeExif(_ n: XmlNode) -> ExifData {
        var e = ExifData()
        e.cameraMake = n.string("CameraMake", default: "")
        e.cameraModel = n.string("CameraModel", default: "")
        e.lens = n.string("Lens", default: "")
        e.iso = n.string("ISO", default: "")
        e.aperture = n.string("Aperture", default: "")
        e.shutter = n.string("Shutter", default: "")
        e.focalLength = n.string("FocalLength", default: "")
        e.exposureBias = n.string("ExposureBias", default: "")
        e.whiteBalance = n.string("WhiteBalance", default: "")
        e.meteringMode = n.string("MeteringMode", default: "")
        e.colorTemperature = n.double("ColorTemperature", default: 0)
        e.tint = n.double("Tint", default: 0)
        if let cn = n.child("Camera") {
            var c = CameraColorInfo()
            c.preMul = cn.doubleArray("PreMul", count: 3) ?? c.preMul
            c.camMul = cn.doubleArray("CamMul", count: 3) ?? c.camMul
            c.rgbCam = cn.doubleArray("RgbCam", count: 9) ?? c.rgbCam
            if c.isValid { e.camera = c }
        }
        e.dateTaken = n.string("DateTaken", default: "")
        e.width = n.int("Width", default: 0)
        e.height = n.int("Height", default: 0)
        e.fileSize = Int64(n.string("FileSize", default: "0")) ?? 0
        e.filePath = n.string("FilePath", default: "")
        return e
    }

    // ---- document I/O ----------------------------------------------------

    static func loadDocument(_ path: String) -> RawPipeDocument? {
        guard FileManager.default.fileExists(atPath: path),
              let root = DotNetXml.parse(contentsOf: URL(fileURLWithPath: path)),
              root.name == "RawPipe" else { return nil }
        var doc = RawPipeDocument()
        doc.isPlaceholder = root.bool("IsPlaceholder", default: false)
        doc.pipelineVersion = root.int("PipelineVersion", default: 0)
        if let a = root.child("Adjustments") { doc.adjustments = decodeAdjustments(a) }
        if let e = root.child("Exif") { doc.exif = decodeExif(e) }
        // The version rides on the document; hand it to the adjustments so the pipeline
        // (which only ever sees ImageAdjustments) knows which maths to use.
        doc.adjustments.pipelineVersion = doc.pipelineVersion
        return doc
    }

    static func writeDocument(_ doc: RawPipeDocument, to path: String) throws {
        let root = XmlNode("RawPipe")
        root.add("IsPlaceholder", doc.isPlaceholder)
        root.add("PipelineVersion", doc.pipelineVersion)
        root.add(encode(doc.adjustments))
        if let e = doc.exif { root.add(encode(e)) }
        let url = URL(fileURLWithPath: path)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(),
                                                withIntermediateDirectories: true)
        try root.documentData().write(to: url, options: .atomic)
    }

    // ---- public API ------------------------------------------------------

    public static func save(imagePath: String, adjustments: ImageAdjustments,
                            copyIndex: Int = 0, exif: ExifData? = nil) {
        let path = AppPaths.adjustmentXmlPath(imagePath, virtualCopyIndex: copyIndex)
        // Preserve previously stored EXIF if none supplied.
        let keep = exif ?? loadDocument(path)?.exif
        var doc = RawPipeDocument()
        doc.isPlaceholder = false
        doc.pipelineVersion = adjustments.pipelineVersion
        doc.adjustments = adjustments
        doc.exif = keep
        try? writeDocument(doc, to: path)
    }

    public static func load(imagePath: String, copyIndex: Int = 0) -> ImageAdjustments? {
        loadDocument(AppPaths.adjustmentXmlPath(imagePath, virtualCopyIndex: copyIndex))?.adjustments
    }

    public static func loadExif(imagePath: String, copyIndex: Int = 0) -> ExifData? {
        loadDocument(AppPaths.adjustmentXmlPath(imagePath, virtualCopyIndex: copyIndex))?.exif
    }

    /// Everything in the document in a single read — `load` / `loadExif` /
    /// `isDefaultPlaceholder` each re-parse the file, which adds up over a whole folder.
    public static func loadAll(imagePath: String, copyIndex: Int = 0)
        -> (adjustments: ImageAdjustments?, exif: ExifData?, isPlaceholder: Bool) {
        guard let doc = loadDocument(AppPaths.adjustmentXmlPath(imagePath, virtualCopyIndex: copyIndex))
        else { return (nil, nil, false) }
        return (doc.adjustments, doc.exif, doc.isPlaceholder)
    }

    /// Returns the stored adjustments, or creates a default placeholder (marked
    /// isPlaceholder) and returns fresh defaults if no XML exists yet.
    public static func ensureDefault(imagePath: String, exif: ExifData? = nil,
                                     copyIndex: Int = 0) -> ImageAdjustments {
        let path = AppPaths.adjustmentXmlPath(imagePath, virtualCopyIndex: copyIndex)
        if let existing = loadDocument(path) { return existing.adjustments }

        var adj = ImageAdjustments()
        // Seed as-shot white balance: from the camera multipliers when LibRaw gave us the
        // colour data (the linear pipeline's own Kelvin scale), else the EXIF-reported K.
        if let cam = exif?.camera, cam.isValid, let shot = ColorScience.asShot(cam) {
            adj.temperature = min(max(shot.kelvin, ColorScience.minKelvin), ColorScience.maxKelvin)
            adj.tint = shot.tint
        } else if let e = exif, e.hasAsShotWhiteBalance {
            adj.temperature = e.colorTemperature
        }

        var doc = RawPipeDocument()
        doc.isPlaceholder = true
        doc.pipelineVersion = adj.pipelineVersion
        doc.adjustments = adj
        doc.exif = exif
        try? writeDocument(doc, to: path)      // non-fatal
        return adj
    }

    public static func isDefaultPlaceholder(imagePath: String, copyIndex: Int = 0) -> Bool {
        loadDocument(AppPaths.adjustmentXmlPath(imagePath, virtualCopyIndex: copyIndex))?.isPlaceholder ?? false
    }

    public static func exists(imagePath: String, copyIndex: Int = 0) -> Bool {
        FileManager.default.fileExists(
            atPath: AppPaths.adjustmentXmlPath(imagePath, virtualCopyIndex: copyIndex))
    }

    public static func delete(imagePath: String, copyIndex: Int = 0) {
        try? FileManager.default.removeItem(
            atPath: AppPaths.adjustmentXmlPath(imagePath, virtualCopyIndex: copyIndex))
    }
}
