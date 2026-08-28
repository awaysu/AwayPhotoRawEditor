import Foundation

/// EXIF / photo metadata as read by `ExifReader` (ImageIO on macOS, replacing the
/// Windows build's ExifTool) and optionally cached inside the adjustment XML.
public struct ExifData: Equatable, Sendable {
    public var cameraMake: String = ""
    public var cameraModel: String = ""
    public var lens: String = ""
    public var iso: String = ""
    public var aperture: String = ""
    public var shutter: String = ""
    public var focalLength: String = ""
    public var exposureBias: String = ""
    public var whiteBalance: String = ""
    public var meteringMode: String = ""

    /// As-shot colour temperature (K) if the camera recorded it; 0 = unknown.
    public var colorTemperature: Double = 0
    /// As-shot tint if recorded.
    public var tint: Double = 0

    /// LibRaw colour data (pre_mul / cam_mul / rgb_cam) for the linear pipeline's
    /// white-balance matrix. Nil for non-RAW files or when LibRaw could not open the file.
    public var camera: CameraColorInfo?

    public var dateTaken: String = ""
    public var width: Int = 0
    public var height: Int = 0
    public var fileSize: Int64 = 0
    public var filePath: String = ""

    public init() {}

    public var fileSizeDisplay: String {
        guard fileSize > 0 else { return "" }
        let mb = Double(fileSize) / 1024 / 1024
        if fileSize >= 1024 * 1024 { return String(format: "%.1f MB", mb) }
        return String(format: "%.1f KB", Double(fileSize) / 1024)
    }

    public var dimensionsDisplay: String {
        width > 0 && height > 0 ? "\(width) x \(height)" : ""
    }

    public var hasAsShotWhiteBalance: Bool {
        colorTemperature >= 2000 && colorTemperature <= 12000
    }
}
