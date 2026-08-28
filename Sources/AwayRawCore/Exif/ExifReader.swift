import Foundation
import ImageIO
import CoreGraphics

/// Reads photo metadata through ImageIO. This is where the macOS port diverges most
/// visibly from Windows: that build shells out to ExifTool for every field. Bundling
/// ExifTool here would mean shipping a Perl distribution — several hundred unsigned
/// Mach-O objects — through notarization, so ImageIO does the job instead. It reads RAW
/// makernotes natively, which covers everything the panel displays.
public enum ExifReader {

    /// The formatted string for a value the panel shows, or "" when absent.
    public static func read(path: String) -> ExifData {
        var data = ExifData()
        data.filePath = path
        if let attrs = try? FileManager.default.attributesOfItem(atPath: path),
           let size = attrs[.size] as? NSNumber {
            data.fileSize = size.int64Value
        }

        guard let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any]
        else { return data }

        let tiff = props[kCGImagePropertyTIFFDictionary] as? [CFString: Any] ?? [:]
        let exif = props[kCGImagePropertyExifDictionary] as? [CFString: Any] ?? [:]
        let aux  = props[kCGImagePropertyExifAuxDictionary] as? [CFString: Any] ?? [:]

        data.cameraMake = str(tiff[kCGImagePropertyTIFFMake])
        data.cameraModel = str(tiff[kCGImagePropertyTIFFModel])
        data.lens = firstNonEmpty(str(exif[kCGImagePropertyExifLensModel]),
                                  str(aux[kCGImagePropertyExifAuxLensModel]),
                                  str(exif[kCGImagePropertyExifLensMake]))

        if let isoArr = exif[kCGImagePropertyExifISOSpeedRatings] as? [NSNumber], let iso = isoArr.first {
            data.iso = String(iso.intValue)
        } else if let iso = exif[kCGImagePropertyExifISOSpeedRatings] as? NSNumber {
            data.iso = String(iso.intValue)
        }

        let fn = num(exif[kCGImagePropertyExifFNumber])
        data.aperture = fn > 0 ? "f/" + trimZeros(fn) : ""

        data.shutter = formatShutter(num(exif[kCGImagePropertyExifExposureTime]))

        let fl = num(exif[kCGImagePropertyExifFocalLength])
        data.focalLength = fl > 0 ? trimZeros(fl) + " mm" : ""

        let ec = num(exif[kCGImagePropertyExifExposureBiasValue])
        data.exposureBias = ec == 0 ? "0 EV" : String(format: "%+.1f EV", ec)

        data.whiteBalance = whiteBalanceName(exif[kCGImagePropertyExifWhiteBalance])
        data.meteringMode = meteringModeName(exif[kCGImagePropertyExifMeteringMode])
        data.dateTaken = firstNonEmpty(str(exif[kCGImagePropertyExifDateTimeOriginal]),
                                       str(exif[kCGImagePropertyExifDateTimeDigitized]),
                                       str(tiff[kCGImagePropertyTIFFDateTime]))

        data.width = int(props[kCGImagePropertyPixelWidth])
        data.height = int(props[kCGImagePropertyPixelHeight])

        // Some RAWs report the whole sensor buffer (an ILCE-7RM6 ARW is 10240×7168 when
        // the visible area is 9984×6656). Prefer the visible size so the info panel agrees
        // with what actually gets exported.
        if let vis = readVisibleSize(path: path),
           vis.width <= data.width, vis.height <= data.height,
           vis.width < data.width || vis.height < data.height {
            data.width = vis.width
            data.height = vis.height
        }
        return data
    }

    /// The visible (cropped) frame size, used both for the info panel and to trim the mask
    /// border LibRaw leaves on models it has no crop table for.
    ///
    /// ImageIO decodes RAW natively and reports the visible frame, so it plays the role
    /// ExifTool's `FullImageSize` plays on Windows. LibRaw's own `sizes.width/height` is
    /// the cross-check: when the two disagree, ImageIO is the one that knows the crop.
    public static func readVisibleSize(path: String) -> (width: Int, height: Int)? {
        guard let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any]
        else { return nil }
        let w = int(props[kCGImagePropertyPixelWidth])
        let h = int(props[kCGImagePropertyPixelHeight])
        guard w > 0, h > 0 else { return nil }
        return (w, h)
    }

    /// The file's EXIF orientation (1..8; 1 when unreadable).
    public static func readOrientation(path: String) -> Int {
        guard let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any],
              let o = props[kCGImagePropertyOrientation] as? Int
        else { return 1 }
        return o
    }

    /// The camera's embedded preview as encoded bytes — the fallback when LibRaw cannot
    /// decode a file.
    public static func extractPreview(path: String) -> Data? {
        guard let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil) else { return nil }
        // Index 0 is the primary image; RAW containers expose the preview as a later index.
        let count = CGImageSourceGetCount(src)
        guard count > 0 else { return nil }
        let opts: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: 4096
        ]
        guard let img = CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary) else { return nil }
        let out = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(out, "public.jpeg" as CFString, 1, nil)
        else { return nil }
        CGImageDestinationAddImage(dest, img, [kCGImageDestinationLossyCompressionQuality: 0.95] as CFDictionary)
        guard CGImageDestinationFinalize(dest) else { return nil }
        return out as Data
    }

    // ---- formatting ------------------------------------------------------

    static func str(_ v: Any?) -> String {
        guard let v else { return "" }
        if let s = v as? String { return s.trimmingCharacters(in: .whitespaces) }
        if let n = v as? NSNumber { return n.stringValue }
        return ""
    }

    static func num(_ v: Any?) -> Double {
        if let n = v as? NSNumber { return n.doubleValue }
        if let s = v as? String, let d = Double(s) { return d }
        if let a = v as? [NSNumber], let f = a.first { return f.doubleValue }
        return 0
    }

    static func int(_ v: Any?) -> Int { Int(num(v)) }

    static func firstNonEmpty(_ vals: String...) -> String {
        for v in vals where !v.trimmingCharacters(in: .whitespaces).isEmpty { return v }
        return ""
    }

    /// "0.#" in .NET: at most one decimal place, no trailing zero.
    static func trimZeros(_ v: Double) -> String {
        let s = String(format: "%.1f", v)
        return s.hasSuffix(".0") ? String(s.dropLast(2)) : s
    }

    static func formatShutter(_ seconds: Double) -> String {
        if seconds <= 0 { return "" }
        if seconds >= 1 { return trimZeros(seconds) + " s" }
        return "1/" + String(Int((1.0 / seconds).rounded())) + " s"
    }

    /// EXIF WhiteBalance is 0 = auto, 1 = manual. The Windows build keeps ExifTool's
    /// friendly string, so the same words are produced here.
    static func whiteBalanceName(_ v: Any?) -> String {
        guard let n = v as? NSNumber else { return "" }
        return n.intValue == 0 ? "Auto" : "Manual"
    }

    static func meteringModeName(_ v: Any?) -> String {
        guard let n = v as? NSNumber else { return "" }
        switch n.intValue {
        case 0: return "Unknown"
        case 1: return "Average"
        case 2: return "Center-weighted average"
        case 3: return "Spot"
        case 4: return "Multi-spot"
        case 5: return "Multi-segment"
        case 6: return "Partial"
        case 255: return "Other"
        default: return ""
        }
    }

    /// Copy the source file's metadata onto an exported file (JPEG/TIFF/PNG carry EXIF;
    /// BMP does not). Rewrites the file in place through ImageIO.
    @discardableResult
    public static func copyMetadata(from sourcePath: String, to destPath: String) -> Bool {
        guard let destSrc = CGImageSourceCreateWithURL(URL(fileURLWithPath: destPath) as CFURL, nil),
              let type = CGImageSourceGetType(destSrc),
              let img = CGImageSourceCreateImageAtIndex(destSrc, 0, nil),
              var destProps = CGImageSourceCopyPropertiesAtIndex(destSrc, 0, nil) as? [CFString: Any]
        else { return false }

        guard let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: sourcePath) as CFURL, nil),
              let srcProps = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any]
        else { return false }

        for key in [kCGImagePropertyExifDictionary, kCGImagePropertyGPSDictionary,
                    kCGImagePropertyIPTCDictionary, kCGImagePropertyExifAuxDictionary] {
            if let v = srcProps[key] { destProps[key] = v }
        }
        if var tiff = srcProps[kCGImagePropertyTIFFDictionary] as? [CFString: Any] {
            // The exported pixels are already upright; keeping the source orientation
            // would rotate them a second time in any viewer.
            tiff.removeValue(forKey: kCGImagePropertyTIFFOrientation)
            if let existing = destProps[kCGImagePropertyTIFFDictionary] as? [CFString: Any] {
                for (k, v) in existing where tiff[k] == nil { tiff[k] = v }
            }
            destProps[kCGImagePropertyTIFFDictionary] = tiff as CFDictionary
        }
        destProps[kCGImagePropertyOrientation] = 1

        let out = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(out, type, 1, nil) else { return false }
        CGImageDestinationAddImage(dest, img, destProps as CFDictionary)
        guard CGImageDestinationFinalize(dest) else { return false }
        return (try? (out as Data).write(to: URL(fileURLWithPath: destPath), options: .atomic)) != nil
    }
}
