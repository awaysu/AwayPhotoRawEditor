import Foundation

/// Per-render context (watermark scaling, cancellation).
public final class ProcessContext: @unchecked Sendable {
    /// Multiplier applied to watermark font size and margin (output/full-res ratio).
    public var watermarkScale: Double = 1.0
    public var forExport: Bool = false

    /// Global watermark overlay to draw on top of the result (nil / disabled = none).
    public var watermark: WatermarkSpec?

    /// When true, skip distortion / rotation / crop (steps 9-10) so the viewer shows the
    /// original full frame — used while the gradient / heal overlay is active (their
    /// effects live in the pre-geometry frame) so overlay coordinates stay aligned.
    public var skipGeometry: Bool = false

    /// When true, apply distortion + 90° rotation but skip only the crop-rectangle
    /// extraction — used while the crop tool overlay is active so the distortion slider
    /// and rotate buttons take visible effect, yet the white crop box still frames the
    /// full (distorted/rotated) image.
    public var skipCropRect: Bool = false

    /// Camera colour data for the linear pipeline's white-balance matrix. Nil →
    /// black-body multipliers (non-RAW, LibRaw unavailable, legacy XML without the data).
    public var camera: CameraColorInfo?

    /// What the source pixels are already balanced to: LibRaw proxies/decodes → `.decode`
    /// (pre_mul); the camera's embedded preview (strip thumbnails) → `.asShot`.
    public var whiteBalanceReference: WhiteBalanceReference = .decode

    /// Render on the GPU when one is usable. Follows AppSettings.useGpu.
    public var useGpu: Bool = AppSettings.current.useGpu

    /// Diagnostics: force the CPU path for this render regardless of `useGpu`.
    public var forceCpu: Bool = false

    /// Set by the pipeline: this render actually ran on the GPU.
    public var usedGpu: Bool = false

    public var token: CancelToken = .none

    public init() {}
}
