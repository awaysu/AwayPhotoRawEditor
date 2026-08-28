import Foundation

/// The camera's colour data as LibRaw reports it, cached inside the adjustment XML so
/// the linear pipeline can build a real white-balance matrix without re-opening the RAW.
/// Multipliers are normalised to G = 1. Nil / invalid → the pipeline falls back to the
/// black-body approximation (non-RAW files, LibRaw unavailable, 4-colour sensors).
public struct CameraColorInfo: Equatable, Sendable {
    /// LibRaw `pre_mul`: the daylight multipliers its default decode balances to.
    public var preMul: [Double] = [0, 0, 0]

    /// LibRaw `cam_mul`: the as-shot multipliers the camera recorded.
    public var camMul: [Double] = [0, 0, 0]

    /// LibRaw `rgb_cam` (3×3, row-major): pre_mul-scaled camera RGB → linear sRGB.
    public var rgbCam: [Double] = Array(repeating: 0, count: 9)

    public init() {}

    public init(preMul: [Double], camMul: [Double], rgbCam: [Double]) {
        self.preMul = preMul
        self.camMul = camMul
        self.rgbCam = rgbCam
    }

    public var isValid: Bool {
        preMul.count == 3 && camMul.count == 3 && rgbCam.count == 9 &&
        Self.allPositive(preMul) && Self.allPositive(camMul) && Self.allFinite(rgbCam)
    }

    private static func allPositive(_ a: [Double]) -> Bool {
        for v in a where !(v > 0) || v.isInfinite { return false }
        return true
    }

    private static func allFinite(_ a: [Double]) -> Bool {
        for v in a where v.isNaN || v.isInfinite { return false }
        return true
    }
}
