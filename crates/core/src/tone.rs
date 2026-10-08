//! 1-D tone response LUT: Blacks/Whites end points, Shadows/Highlights region shifts and
//! Contrast. Port of `ToneCurve.swift`.

use crate::model::ImageAdjustments;

pub const LUT_SIZE: usize = 1024;

pub fn build_lut(a: &ImageAdjustments) -> Vec<f32> {
    let blacks = a.blacks / 100.0;
    let whites = a.whites / 100.0;
    let contrast = a.contrast / 100.0;
    let hi = a.highlights / 100.0;
    let sh = a.shadows / 100.0;

    let bl = (-blacks * 0.12).max(-0.1).min(0.4);
    let wl = (1.0 - whites * 0.12).max(0.6).min(1.1);
    let span = (wl - bl).max(1e-3);

    (0..LUT_SIZE)
        .map(|i| {
            let x = i as f64 / (LUT_SIZE - 1) as f64;

            // 1) black / white point remap
            let mut v = (x - bl) / span;
            v = v.max(0.0).min(1.0);

            // 2) shadows / highlights region shift
            let w_h = v * v;
            let w_s = (1.0 - v) * (1.0 - v);
            v += hi * 0.28 * w_h + sh * 0.28 * w_s;
            v = v.max(0.0).min(1.0);

            // 3) contrast S-curve around mid grey
            let t = v - 0.5;
            v = 0.5 + t * (1.0 + contrast) + contrast * 0.6 * t * (0.25 - t * t);
            v.max(0.0).min(1.0) as f32
        })
        .collect()
}

/// Sample a LUT with linear interpolation; input clamped to 0..1.
#[inline(always)]
pub fn sample(lut: &[f32], x: f32) -> f32 {
    if x <= 0.0 {
        return lut[0];
    }
    if x >= 1.0 {
        return lut[LUT_SIZE - 1];
    }
    let f = x * (LUT_SIZE - 1) as f32;
    let i = f as usize;
    let frac = f - i as f32;
    // 0 < x < 1 here, so i <= LUT_SIZE - 2.
    debug_assert!(lut.len() == LUT_SIZE && i + 1 < LUT_SIZE);
    unsafe {
        let a = *lut.get_unchecked(i);
        let b = *lut.get_unchecked(i + 1);
        a + (b - a) * frac
    }
}
