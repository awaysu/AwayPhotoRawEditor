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
    reset_v3(a);
}

/// The 處理版本 3 values (高光復原, HSL, curves) back to neutral.
pub fn reset_v3(a: &mut ImageAdjustments) {
    let d = ImageAdjustments::default();
    a.highlight_recovery = d.highlight_recovery;
    (a.hsl_hue, a.hsl_saturation, a.hsl_luminance) = (d.hsl_hue, d.hsl_saturation, d.hsl_luminance);
    (a.curve_rgb, a.curve_red, a.curve_green, a.curve_blue) = (d.curve_rgb, d.curve_red, d.curve_green, d.curve_blue);
}

/// 基本／色彩／細節 重設 (white balance included, geometry and local edits kept).
pub fn reset_basic_color_detail(a: &mut ImageAdjustments) {
    let d = ImageAdjustments::default();
    (a.exposure, a.contrast, a.highlights, a.shadows, a.whites, a.blacks) = (d.exposure, d.contrast, d.highlights, d.shadows, d.whites, d.blacks);
    (a.temperature, a.tint, a.vibrance, a.saturation) = (d.temperature, d.tint, d.vibrance, d.saturation);
    (a.sharpening, a.noise_reduction, a.vignette) = (d.sharpening, d.noise_reduction, d.vignette);
    reset_v3(a);
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
        highlight_recovery, hsl_hue, hsl_saturation, hsl_luminance, curve_rgb, curve_red, curve_green, curve_blue,
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

/// 處理版本 2 → 3. Exposure and white balance mean the same thing in both — version 3
/// folds version 2's decode and white-balance matrix into one (`v3::camera_to_srgb`) and
/// reproduces LibRaw's auto-bright as a gain — so they stay as they are; tone, colour and
/// detail keep their values; the version-3-only tools start neutral.
pub fn convert_linear_to_v3(a: &mut ImageAdjustments) {
    reset_v3(a);
}

/// 升級處理版本 for one photo, from any older version straight to the current one;
/// false when it already uses the current maths.
pub fn upgrade(a: &mut ImageAdjustments, exif: Option<&ExifData>) -> bool {
    if a.pipeline_version >= ImageAdjustments::CURRENT_PIPELINE_VERSION {
        return false;
    }
    if a.is_legacy_pipeline() {
        convert_legacy_to_linear(a, exif);
    }
    convert_linear_to_v3(a);
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
        assert_eq!((a.temperature, a.tint, a.pipeline_version), (5600.0, 5.0, ImageAdjustments::CURRENT_PIPELINE_VERSION));
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
    /// 升級處理版本 2 → 3 on a synthetic photo: version 2 renders LibRaw's decode
    /// (BT.709 of rgb_cam · pre_mul · raw · auto-bright), version 3 the linear camera data
    /// with the same gain. With exposure and white balance set (and no version-3-only
    /// values) the two must agree to 8-bit rounding.
    #[test]
    fn upgrade_2_to_3_keeps_exposure_and_white_balance() {
        use awpr_core::color::WhiteBalanceReference;
        use awpr_core::{apply_to_float, v3, FloatImage, ProcessContext, SourceKind};
        let cam = CameraColorInfo { pre_mul: [2.1792, 1.0, 1.2902], cam_mul: [1.9463, 1.0, 1.5488], rgb_cam: [1.7, -0.6, -0.1, -0.2, 1.5, -0.3, 0.05, -0.45, 1.4] };
        let gain = 1.6f32;
        let (w, h) = (48usize, 24usize);
        let mut raw = FloatImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                // Greys and mild colours under daylight (raw ≈ 1 / pre_mul for a neutral).
                let level = 0.01 + 0.13 * x as f32 / w as f32;
                let tilt = [1.0 + 0.25 * ((y % 3) as f32 - 1.0), 1.0, 1.0 - 0.2 * ((y % 4) as f32 - 1.5) / 1.5];
                let o = raw.index(x, y);
                for c in 0..3 {
                    raw.data[o + c] = level * tilt[c] / cam.pre_mul[c] as f32;
                }
                raw.data[o + 3] = 1.0;
            }
        }
        let mut decoded = raw.clone();
        for p in decoded.data.chunks_exact_mut(4) {
            let q: Vec<f64> = (0..3).map(|c| p[c] as f64 * cam.pre_mul[c]).collect();
            for r in 0..3 {
                let s: f64 = (0..3).map(|k| cam.rgb_cam[r * 3 + k] * q[k]).sum::<f64>() * gain as f64;
                p[r] = v3::oetf(s.clamp(0.0, 1.0) as f32);
            }
        }
        let v2 = ImageAdjustments { pipeline_version: 1, exposure: 0.4, temperature: 4300.0, tint: 6.0, ..Default::default() };
        let mut up = v2.clone();
        assert!(upgrade(&mut up, None));
        assert_eq!((up.exposure, up.temperature, up.tint, up.pipeline_version), (0.4, 4300.0, 6.0, 2));
        let ctx2 = ProcessContext { camera: Some(cam.clone()), white_balance_reference: WhiteBalanceReference::Decode, ..Default::default() };
        let ctx3 = ProcessContext { camera: Some(cam), source_kind: SourceKind::LinearCamera { gain }, ..Default::default() };
        let a = apply_to_float(&decoded, &v2, &ctx2);
        let b = apply_to_float(&raw, &up, &ctx3);
        let byte = |v: f32| (v * 255.0 + 0.5).clamp(0.0, 255.0) as i32;
        let (mut worst, mut sum) = (0, 0i64);
        for (i, (x, y)) in a.data.iter().zip(&b.data).enumerate() {
            if i % 4 == 3 {
                continue;
            }
            let d = (byte(*x) - byte(*y)).abs();
            worst = worst.max(d);
            sum += d as i64;
        }
        assert!(worst <= 1, "8-bit difference up to {worst}");
        assert!((sum as f64) / ((w * h * 3) as f64) < 0.2, "mean 8-bit difference {}", sum as f64 / (w * h * 3) as f64);
    }
}
