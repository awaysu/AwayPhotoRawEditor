//! Whole-adjustment operations the editor and the batch tools share: the resets, the
//! multi-selection delta copy and the 升級處理版本 conversion. Ports of the C#
//! `ImageAdjustments.ResetAll / ResetTonal / ResetBasicColorDetail / ApplyDelta` and
//! `MainForm.ConvertLegacyToLinear`.

use crate::store::ExifData;
use awpr_core::{color, ImageAdjustments};

/// 預設時設定 / 全部重設: every value back to default, gradients and heal spots removed.
/// The processing version stays (it is not an edit).
pub fn reset_all(a: &mut ImageAdjustments) {
    let v = a.pipeline_version;
    *a = ImageAdjustments { pipeline_version: v, ..Default::default() };
}

/// Before a look preset: tone, colour and detail plus distortion back to default.
/// Temperature / tint are left alone — presets do not touch white balance (v1.0.16).
pub fn reset_tonal(a: &mut ImageAdjustments) {
    let d = ImageAdjustments::default();
    (a.exposure, a.contrast, a.highlights, a.shadows, a.whites, a.blacks) = (d.exposure, d.contrast, d.highlights, d.shadows, d.whites, d.blacks);
    (a.vibrance, a.saturation) = (d.vibrance, d.saturation);
    (a.sharpening, a.noise_reduction, a.vignette, a.distortion) = (d.sharpening, d.noise_reduction, d.vignette, d.distortion);
}

/// 基本／色彩／細節 重設 (white balance included, geometry and local edits kept).
pub fn reset_basic_color_detail(a: &mut ImageAdjustments) {
    let d = ImageAdjustments::default();
    (a.exposure, a.contrast, a.highlights, a.shadows, a.whites, a.blacks) = (d.exposure, d.contrast, d.highlights, d.shadows, d.whites, d.blacks);
    (a.temperature, a.tint, a.vibrance, a.saturation) = (d.temperature, d.tint, d.vibrance, d.saturation);
    (a.sharpening, a.noise_reduction, a.vignette) = (d.sharpening, d.noise_reduction, d.vignette);
}

/// Copy onto `target` every scalar field that differs between `edited` and `baseline` —
/// the fields the user changed during a multi-selection edit. Gradients and heal spots
/// (position-specific) and the processing version are never copied.
pub fn apply_delta(target: &mut ImageAdjustments, edited: &ImageAdjustments, baseline: &ImageAdjustments) {
    macro_rules! sync {
        ($($f:ident),+ $(,)?) => {
            $(if edited.$f != baseline.$f {
                target.$f = edited.$f.clone();
            })+
        };
    }
    sync!(
        exposure, contrast, highlights, shadows, whites, blacks, temperature, tint, vibrance, saturation, sharpening, noise_reduction, vignette, distortion,
        crop_aspect_ratio, crop_angle, crop_x, crop_y, crop_width, crop_height, rotation, heal_size,
    );
}

/// Rewrite a legacy photo's values so it looks as close as possible under the linear
/// pipeline: exposure × 1/0.45 (the old one multiplied gamma-encoded values), and the
/// white balance's offset from as-shot moved onto the camera-matrix as-shot.
pub fn convert_legacy_to_linear(a: &mut ImageAdjustments, exif: Option<&ExifData>) {
    const GAMMA_POWER: f64 = 1.0 / 0.45;
    a.exposure = (a.exposure * GAMMA_POWER).clamp(-5.0, 5.0);
    for g in &mut a.gradients {
        g.exposure = (g.exposure * GAMMA_POWER).clamp(-5.0, 5.0);
    }
    let legacy_as_shot = exif.filter(|e| e.has_as_shot_white_balance()).map(|e| e.color_temperature).unwrap_or(5200.0);
    let delta = a.temperature - legacy_as_shot;
    if let Some((k, t)) = exif.and_then(|e| e.camera.as_ref()).filter(|c| c.is_valid()).and_then(color::as_shot) {
        a.temperature = (k + delta).clamp(color::MIN_KELVIN, color::MAX_KELVIN);
        a.tint = (t + a.tint).clamp(-100.0, 100.0);
    }
    // No camera data (non-RAW, LibRaw could not read it): the black-body scale did not
    // change, so the Kelvin value stays.
}

/// 升級處理版本 for one photo; false when it already uses the current maths.
pub fn upgrade(a: &mut ImageAdjustments, exif: Option<&ExifData>) -> bool {
    if !a.is_legacy_pipeline() {
        return false;
    }
    convert_legacy_to_linear(a, exif);
    a.pipeline_version = ImageAdjustments::CURRENT_PIPELINE_VERSION;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use awpr_core::{CameraColorInfo, HealSpot, LinearGradient, Rotation};

    #[test]
    fn resets() {
        let mut a = ImageAdjustments { pipeline_version: 0, exposure: 1.0, temperature: 3000.0, tint: 12.0, saturation: 40.0, distortion: 20.0, crop_x: 0.1, ..Default::default() };
        a.gradients.push(LinearGradient::default());
        let mut t = a.clone();
        reset_tonal(&mut t);
        assert_eq!((t.exposure, t.saturation, t.distortion), (0.0, 0.0, 0.0));
        assert_eq!((t.temperature, t.tint, t.crop_x, t.gradients.len()), (3000.0, 12.0, 0.1, 1)); // WB and geometry kept
        let mut b = a.clone();
        reset_basic_color_detail(&mut b);
        assert_eq!((b.temperature, b.tint, b.distortion, b.crop_x), (5200.0, 0.0, 20.0, 0.1));
        let mut c = a.clone();
        reset_all(&mut c);
        assert_eq!(c, ImageAdjustments { pipeline_version: 0, ..Default::default() });
    }

    #[test]
    fn delta_copies_only_changed_scalars() {
        let baseline = ImageAdjustments { exposure: 0.5, contrast: 10.0, ..Default::default() };
        let mut edited = baseline.clone();
        edited.contrast = 35.0;
        edited.rotation = Rotation::R90;
        edited.crop_aspect_ratio = "4:3".into();
        edited.heal_spots.push(HealSpot::default());
        edited.gradients.push(LinearGradient::default());
        let mut other = ImageAdjustments { exposure: -1.0, contrast: 0.0, temperature: 4000.0, pipeline_version: 0, ..Default::default() };
        apply_delta(&mut other, &edited, &baseline);
        assert_eq!(other.contrast, 35.0); // changed → synced
        assert_eq!(other.exposure, -1.0); // untouched → kept
        assert_eq!(other.temperature, 4000.0);
        assert_eq!((other.rotation, other.crop_aspect_ratio.as_str()), (Rotation::R90, "4:3"));
        assert!(other.heal_spots.is_empty() && other.gradients.is_empty());
        assert_eq!(other.pipeline_version, 0);
    }

    #[test]
    fn upgrade_conversion_matches_csharp() {
        // C#: Exposure × (1 / 0.45), clamp ±5; gradients the same.
        let mut a = ImageAdjustments { pipeline_version: 0, exposure: 1.0, temperature: 5600.0, tint: 5.0, ..Default::default() };
        a.gradients.push(LinearGradient { exposure: -3.0, ..Default::default() });
        // No camera data, no EXIF Kelvin: Kelvin kept.
        assert!(upgrade(&mut a, None));
        assert_eq!(a.exposure, 1.0 / 0.45);
        assert_eq!(a.gradients[0].exposure, -5.0); // −6.67 clamped
        assert_eq!((a.temperature, a.tint, a.pipeline_version), (5600.0, 5.0, 1));
        assert!(!upgrade(&mut a, None)); // already current

        // With camera data: offset from the EXIF as-shot Kelvin moved onto the matrix as-shot.
        let cam = CameraColorInfo { pre_mul: [2.1792, 1.0, 1.2902], cam_mul: [1.9463, 1.0, 1.5488], rgb_cam: [1.7, -0.6, -0.1, -0.2, 1.5, -0.3, 0.05, -0.45, 1.4] };
        let (k, t) = color::as_shot(&cam).unwrap();
        let exif = ExifData { color_temperature: 5000.0, camera: Some(cam), ..Default::default() };
        let mut b = ImageAdjustments { pipeline_version: 0, exposure: 0.0, temperature: 5300.0, tint: -4.0, ..Default::default() };
        assert!(upgrade(&mut b, Some(&exif)));
        assert_eq!(b.temperature, (k + 300.0).clamp(2000.0, 12000.0));
        assert_eq!(b.tint, (t - 4.0).clamp(-100.0, 100.0));
    }
}
