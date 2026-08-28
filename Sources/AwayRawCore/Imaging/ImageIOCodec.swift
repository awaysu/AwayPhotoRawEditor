import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

/// Decoding and encoding of ordinary image formats through ImageIO — the macOS
/// counterpart of the Windows build's WIC decoder. Everything is normalised to
/// straight-alpha 8-bit sRGB before it becomes a `FloatImageBuffer`, so the pipeline
/// sees exactly the value range it was written against.
public enum ImageIOCodec {

    nonisolated(unsafe) static let srgb = CGColorSpace(name: CGColorSpace.sRGB)!

    // ---- decode ----------------------------------------------------------

    public static func loadCGImage(path: String) -> CGImage? {
        let url = URL(fileURLWithPath: path) as CFURL
        guard let src = CGImageSourceCreateWithURL(url, nil) else { return nil }
        return makeOriented(src)
    }

    public static func loadCGImage(data: Data) -> CGImage? {
        guard let src = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        return makeOriented(src)
    }

    /// Decodes frame 0 and bakes in the EXIF orientation, so callers never have to think
    /// about it again.
    static func makeOriented(_ src: CGImageSource) -> CGImage? {
        let opts: [CFString: Any] = [kCGImageSourceShouldCache: false]
        guard let img = CGImageSourceCreateImageAtIndex(src, 0, opts as CFDictionary) else { return nil }
        let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any]
        let orientation = (props?[kCGImagePropertyOrientation] as? Int) ?? 1
        return orientation > 1 ? applyOrientation(img, orientation) : img
    }

    /// Apply an EXIF orientation (1..8) to a CGImage.
    public static func applyOrientation(_ img: CGImage, _ orientation: Int) -> CGImage {
        let w = img.width, h = img.height
        let swapsAxes = orientation >= 5
        let outW = swapsAxes ? h : w
        let outH = swapsAxes ? w : h

        guard let ctx = CGContext(data: nil, width: outW, height: outH,
                                  bitsPerComponent: 8, bytesPerRow: 0,
                                  space: srgb,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return img }

        // CGContext's origin is bottom-left, so each case is the EXIF transform composed
        // with the flip between the two coordinate systems.
        switch orientation {
        case 2: ctx.translateBy(x: CGFloat(outW), y: 0); ctx.scaleBy(x: -1, y: 1)
        case 3: ctx.translateBy(x: CGFloat(outW), y: CGFloat(outH)); ctx.rotate(by: .pi)
        case 4: ctx.translateBy(x: 0, y: CGFloat(outH)); ctx.scaleBy(x: 1, y: -1)
        case 5: ctx.rotate(by: -.pi / 2); ctx.translateBy(x: CGFloat(-outH), y: 0)
                ctx.translateBy(x: CGFloat(outH), y: 0); ctx.scaleBy(x: -1, y: 1)
        case 6: ctx.translateBy(x: CGFloat(outW), y: 0); ctx.rotate(by: .pi / 2)
        case 7: ctx.translateBy(x: CGFloat(outW), y: 0); ctx.rotate(by: .pi / 2)
                ctx.translateBy(x: CGFloat(w), y: 0); ctx.scaleBy(x: -1, y: 1)
        case 8: ctx.translateBy(x: 0, y: CGFloat(outH)); ctx.rotate(by: -.pi / 2)
        default: break
        }
        ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))
        return ctx.makeImage() ?? img
    }

    /// Convert a CGImage into the pipeline's float buffer, going through an sRGB
    /// 8-bit context so colour-managed sources land on the same numbers the Windows
    /// build's 32bppArgb conversion produced.
    public static func toFloatBuffer(_ img: CGImage) -> FloatImageBuffer? {
        let w = img.width, h = img.height
        guard w > 0, h > 0 else { return nil }
        let bytesPerRow = w * 4
        var bytes = [UInt8](repeating: 0, count: bytesPerRow * h)
        let ok: Bool = bytes.withUnsafeMutableBytes { raw -> Bool in
            guard let ctx = CGContext(data: raw.baseAddress, width: w, height: h,
                                      bitsPerComponent: 8, bytesPerRow: bytesPerRow,
                                      space: srgb,
                                      bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue |
                                                  CGBitmapInfo.byteOrder32Big.rawValue)
            else { return false }
            ctx.setFillColor(red: 0, green: 0, blue: 0, alpha: 1)
            ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))
            return true
        }
        guard ok else { return nil }

        let buf = FloatImageBuffer(width: w, height: h, zeroed: false)
        let d = buf.data
        let inv: Float = 1.0 / 255.0
        bytes.withUnsafeBufferPointer { bp in
            let b = bp.baseAddress!
            parallelRows(0, h) { y in
                var s = y * bytesPerRow
                var o = y * w * 4
                for _ in 0..<w {
                    d[o]     = Float(b[s])     * inv
                    d[o + 1] = Float(b[s + 1]) * inv
                    d[o + 2] = Float(b[s + 2]) * inv
                    d[o + 3] = 1
                    s += 4; o += 4
                }
            }
        }
        return buf
    }

    public static func loadFloat(path: String) -> FloatImageBuffer? {
        guard let img = loadCGImage(path: path) else { return nil }
        return toFloatBuffer(img)
    }

    public static func loadFloat(data: Data) -> FloatImageBuffer? {
        guard let img = loadCGImage(data: data) else { return nil }
        return toFloatBuffer(img)
    }

    // ---- encode ----------------------------------------------------------

    /// Materialise a float buffer as an 8-bit sRGB CGImage (values clamped to 0..1).
    public static func toCGImage(_ buf: FloatImageBuffer) -> CGImage? {
        let w = buf.width, h = buf.height
        let bytesPerRow = w * 4
        var bytes = [UInt8](repeating: 0, count: bytesPerRow * h)
        let d = buf.data
        bytes.withUnsafeMutableBufferPointer { bp in
            let b = bp.baseAddress!
            parallelRows(0, h) { y in
                var s = y * w * 4
                var o = y * bytesPerRow
                for _ in 0..<w {
                    b[o]     = toByte(d[s])
                    b[o + 1] = toByte(d[s + 1])
                    b[o + 2] = toByte(d[s + 2])
                    b[o + 3] = 255
                    s += 4; o += 4
                }
            }
        }
        guard let provider = CGDataProvider(data: Data(bytes) as CFData) else { return nil }
        return CGImage(width: w, height: h, bitsPerComponent: 8, bitsPerPixel: 32,
                       bytesPerRow: bytesPerRow, space: srgb,
                       bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue |
                                                          CGBitmapInfo.byteOrder32Big.rawValue),
                       provider: provider, decode: nil, shouldInterpolate: false,
                       intent: .defaultIntent)
    }

    @inline(__always)
    static func toByte(_ v: Float) -> UInt8 {
        let i = Int(v * 255 + 0.5)
        return UInt8(i < 0 ? 0 : (i > 255 ? 255 : i))
    }

    public enum OutputFormat {
        case jpeg(quality: Double)      // 0..1
        case png
        case tiff
        case bmp

        var utType: CFString {
            switch self {
            case .jpeg: return UTType.jpeg.identifier as CFString
            case .png:  return UTType.png.identifier as CFString
            case .tiff: return UTType.tiff.identifier as CFString
            case .bmp:  return UTType.bmp.identifier as CFString
            }
        }
    }

    /// Write a CGImage to disk, optionally stamping a DPI and copying source metadata.
    @discardableResult
    public static func write(_ img: CGImage, to path: String, format: OutputFormat,
                             dpi: Int? = nil,
                             metadataFrom sourcePath: String? = nil) -> Bool {
        let url = URL(fileURLWithPath: path) as CFURL
        guard let dest = CGImageDestinationCreateWithURL(url, format.utType, 1, nil) else { return false }

        var props: [CFString: Any] = [:]
        if case .jpeg(let q) = format {
            props[kCGImageDestinationLossyCompressionQuality] = q
        }
        if let dpi, dpi > 0 {
            props[kCGImagePropertyDPIWidth] = dpi
            props[kCGImagePropertyDPIHeight] = dpi
            // TIFF and JPEG record resolution in their own dictionaries too.
            props[kCGImagePropertyTIFFDictionary] = [
                kCGImagePropertyTIFFXResolution: dpi,
                kCGImagePropertyTIFFYResolution: dpi,
                kCGImagePropertyTIFFResolutionUnit: 2
            ] as CFDictionary
            props[kCGImagePropertyJFIFDictionary] = [
                kCGImagePropertyJFIFXDensity: dpi,
                kCGImagePropertyJFIFYDensity: dpi,
                kCGImagePropertyJFIFDensityUnit: 1
            ] as CFDictionary
        }

        // Carry the source's EXIF/GPS/IPTC across when asked. The orientation is dropped
        // because the exported pixels are already upright — keeping it would rotate the
        // image a second time in any viewer.
        if let sourcePath,
           let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: sourcePath) as CFURL, nil),
           let srcProps = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any] {
            for key in [kCGImagePropertyExifDictionary, kCGImagePropertyGPSDictionary,
                        kCGImagePropertyIPTCDictionary, kCGImagePropertyExifAuxDictionary] {
                if let v = srcProps[key] { props[key] = v }
            }
            if var tiff = srcProps[kCGImagePropertyTIFFDictionary] as? [CFString: Any] {
                tiff.removeValue(forKey: kCGImagePropertyTIFFOrientation)
                if let dpi, dpi > 0 {
                    tiff[kCGImagePropertyTIFFXResolution] = dpi
                    tiff[kCGImagePropertyTIFFYResolution] = dpi
                    tiff[kCGImagePropertyTIFFResolutionUnit] = 2
                }
                props[kCGImagePropertyTIFFDictionary] = tiff as CFDictionary
            }
            props[kCGImagePropertyOrientation] = 1
        }

        CGImageDestinationAddImage(dest, img, props as CFDictionary)
        return CGImageDestinationFinalize(dest)
    }

    @discardableResult
    public static func write(_ buf: FloatImageBuffer, to path: String, format: OutputFormat,
                             dpi: Int? = nil, metadataFrom sourcePath: String? = nil) -> Bool {
        guard let img = toCGImage(buf) else { return false }
        return write(img, to: path, format: format, dpi: dpi, metadataFrom: sourcePath)
    }
}
