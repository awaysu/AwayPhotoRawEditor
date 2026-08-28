import Foundation

/// Moving a photo from the legacy maths (pipelineVersion 0) to the linear pipeline (1).
/// Lightroom-style: an old photo keeps rendering exactly as it always did until the user
/// asks for the upgrade, at which point the slider values are rewritten so the picture
/// looks as close as possible to what it looked like before.
public enum PipelineUpgrade {

    /// The neutral Kelvin the legacy pipeline was built around.
    public static let neutralKelvin = 5200.0

    /// Rewrite legacy slider values so the photo looks as close as possible under v1.
    public static func convertLegacyToLinear(_ a: inout ImageAdjustments, exif: ExifData?) {
        // Legacy exposure multiplied the 709-encoded value: ×2 is about 2^2.22 of light.
        // The toe has a different slope, so this is an approximation — close enough for
        // "looks about the same".
        let gammaPower = 1.0 / 0.45
        a.exposure = min(max(a.exposure * gammaPower, -5), 5)
        for i in a.gradients.indices {
            a.gradients[i].exposure = min(max(a.gradients[i].exposure * gammaPower, -5), 5)
        }

        // Colour temperature: the legacy as-shot reference was the EXIF Kelvin (or neutral
        // 5200 with none); v1's comes from cam_mul. Keep the offset the user dialled in and
        // move it onto the new reference.
        let legacyAsShot = (exif?.hasAsShotWhiteBalance ?? false)
            ? exif!.colorTemperature : neutralKelvin
        let delta = a.temperature - legacyAsShot
        if let cam = exif?.camera, cam.isValid, let shot = ColorScience.asShot(cam) {
            a.temperature = min(max(shot.kelvin + delta, ColorScience.minKelvin), ColorScience.maxKelvin)
            a.tint = min(max(shot.tint + a.tint, -100), 100)
        }
        // Without camera colour data (non-RAW, or LibRaw could not read it) the black-body
        // scale is unchanged, so the Kelvin value carries over as is.

        a.pipelineVersion = ImageAdjustments.currentPipelineVersion
    }

    /// Upgrade one stored photo in place. Returns false when it was already current.
    @discardableResult
    public static func upgrade(imagePath: String, copyIndex: Int = 0, loader: RawLoader) -> Bool {
        var (adjOpt, exif, _) = AdjustmentXmlStore.loadAll(imagePath: imagePath, copyIndex: copyIndex)
        guard var adj = adjOpt else { return false }
        guard adj.isLegacyPipeline else { return false }
        _ = loader.enrichCameraColor(path: imagePath, exif: &exif)
        convertLegacyToLinear(&adj, exif: exif)
        AdjustmentXmlStore.save(imagePath: imagePath, adjustments: adj,
                                copyIndex: copyIndex, exif: exif)
        return true
    }
}
