using System.Xml.Serialization;

namespace AwayPhotoRawEditor.Models;

/// <summary>
/// The camera's colour data as LibRaw reports it, cached inside the adjustment XML so
/// the linear pipeline can build a real white-balance matrix without re-opening the RAW.
/// Multipliers are normalised to G = 1. Null / invalid → the pipeline falls back to the
/// black-body approximation (non-RAW files, LibRaw unavailable, 4-colour sensors).
/// </summary>
public sealed class CameraColorInfo
{
    /// <summary>LibRaw <c>pre_mul</c>: the daylight multipliers its default decode balances to.</summary>
    public double[] PreMul { get; set; } = new double[3];

    /// <summary>LibRaw <c>cam_mul</c>: the as-shot multipliers the camera recorded.</summary>
    public double[] CamMul { get; set; } = new double[3];

    /// <summary>LibRaw <c>rgb_cam</c> (3×3, row-major): pre_mul-scaled camera RGB → linear sRGB.</summary>
    public double[] RgbCam { get; set; } = new double[9];

    [XmlIgnore]
    public bool IsValid =>
        PreMul is { Length: 3 } && CamMul is { Length: 3 } && RgbCam is { Length: 9 } &&
        AllPositive(PreMul) && AllPositive(CamMul) && AllFinite(RgbCam);

    private static bool AllPositive(double[] a)
    {
        foreach (var v in a) if (!(v > 0) || double.IsInfinity(v)) return false;
        return true;
    }

    private static bool AllFinite(double[] a)
    {
        foreach (var v in a) if (double.IsNaN(v) || double.IsInfinity(v)) return false;
        return true;
    }
}
