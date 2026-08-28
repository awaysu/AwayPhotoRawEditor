import Foundation

/// Standalone watermark (標誌) configuration. The watermark is a global export-time
/// overlay (stored in ExportSettings) rather than a per-photo edit; when enabled it is
/// also drawn live onto the main preview. Sizes are authored at full resolution and
/// scaled per render via `ProcessContext.watermarkScale`.
public struct WatermarkSpec: Equatable, Sendable {
    public var enabled: Bool = false
    public var text: String = ""
    public var fontName: String = "Helvetica"
    public var fontSize: Double = 150       // 6 .. 300
    public var transparency: Int = 20       // 0 .. 100
    public var color: WatermarkColor = .white
    public var position: WatermarkPosition = .bottomRight
    public var margin: Int = 30             // 0 .. 9999 px

    public init() {}
}
