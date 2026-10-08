//! Colour maths for the linear-light pipeline (pipelineVersion ≥ 1). Port of
//! `ColorScience.swift`; see that file and the C# CLAUDE.md for the reasoning.

use crate::model::CameraColorInfo;
use std::sync::OnceLock;

/// Which white balance the source pixels were already balanced to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteBalanceReference {
    /// LibRaw proxy / full decode: balanced with `pre_mul`.
    #[default]
    Decode,
    /// Camera-rendered preview: the as-shot `cam_mul` is baked in.
    AsShot,
}

// ---- transfer curve (LibRaw gamm = {0.45, 4.5}, BT.709 OETF) ------------------

pub const DECODE_LUT_SIZE: usize = 4096;
pub const ENCODE_LUT_SIZE: usize = 8192;

fn build_lut(n: usize, f: fn(f64) -> f64) -> Vec<f32> {
    (0..=n).map(|i| f(i as f64 / n as f64) as f32).collect()
}

pub fn decode_lut() -> &'static [f32] {
    static L: OnceLock<Vec<f32>> = OnceLock::new();
    L.get_or_init(|| build_lut(DECODE_LUT_SIZE, decode_exact))
}

pub fn encode_lut() -> &'static [f32] {
    static L: OnceLock<Vec<f32>> = OnceLock::new();
    L.get_or_init(|| build_lut(ENCODE_LUT_SIZE, encode_exact))
}

pub fn encode_exact(l: f64) -> f64 {
    if l <= 0.0 {
        0.0
    } else if l < 0.018 {
        4.5 * l
    } else {
        1.099 * l.powf(0.45) - 0.099
    }
}

pub fn decode_exact(v: f64) -> f64 {
    if v <= 0.0 {
        0.0
    } else if v < 0.081 {
        v / 4.5
    } else {
        ((v + 0.099) / 1.099).powf(1.0 / 0.45)
    }
}

#[inline(always)]
pub fn sample(lut: &[f32], n: usize, x: f32) -> f32 {
    if !(x > 0.0) {
        return 0.0; // also catches NaN
    }
    if x >= 1.0 {
        return lut[n];
    }
    let f = x * n as f32;
    let i = f as usize;
    let t = f - i as f32;
    // 0 < x < 1 here, so i <= n - 1 and i + 1 <= n: inside the n + 1 entries. This is the
    // hottest load in the pipeline (six per pixel), so the bounds check is skipped.
    debug_assert!(lut.len() == n + 1 && i < n);
    unsafe {
        let a = *lut.get_unchecked(i);
        let b = *lut.get_unchecked(i + 1);
        a + (b - a) * t
    }
}

/// Encoded (display) value → linear light.
#[inline(always)]
pub fn linearize(lut: &[f32], v: f32) -> f32 {
    sample(lut, DECODE_LUT_SIZE, v)
}

/// Linear light → encoded value (highlights clip here).
#[inline(always)]
pub fn encode(lut: &[f32], l: f32) -> f32 {
    sample(lut, ENCODE_LUT_SIZE, l)
}

// ---- XYZ ↔ linear sRGB (D65) -------------------------------------------------

const XYZ_TO_SRGB: [f64; 9] = [
    3.2404542, -1.5371385, -0.4985314, //
    -0.9692660, 1.8760108, 0.0415560, //
    0.0556434, -0.2040259, 1.0572252,
];

const SRGB_TO_XYZ: [f64; 9] = [
    0.4124564, 0.3575761, 0.1804375, //
    0.2126729, 0.7151522, 0.0721750, //
    0.0193339, 0.1191920, 0.9503041,
];

// ---- Planckian locus / CCT (Kang et al. 2002) --------------------------------

pub const MIN_KELVIN: f64 = 2000.0;
pub const MAX_KELVIN: f64 = 12000.0;
/// 1 tint unit = this much Duv.
pub const DUV_PER_TINT_UNIT: f64 = 0.0002;

fn planckian_xy(t_in: f64) -> (f64, f64) {
    let t_k = t_in.clamp(1667.0, 25000.0);
    let t = 1e3 / t_k;
    let t2 = t * t;
    let t3 = t2 * t;
    let x = if t_k <= 4000.0 {
        -0.2661239 * t3 - 0.2343589 * t2 + 0.8776956 * t + 0.179910
    } else {
        -3.0258469 * t3 + 2.1070379 * t2 + 0.2226347 * t + 0.240390
    };
    let x2 = x * x;
    let x3 = x2 * x;
    let y = if t_k <= 2222.0 {
        -1.1063814 * x3 - 1.34811020 * x2 + 2.18555832 * x - 0.20219683
    } else if t_k <= 4000.0 {
        -0.9549476 * x3 - 1.37418593 * x2 + 2.09137015 * x - 0.16748867
    } else {
        3.0817580 * x3 - 5.87338670 * x2 + 3.75112997 * x - 0.37001483
    };
    (x, y)
}

fn xy_to_uv(x: f64, y: f64) -> (f64, f64) {
    let d = -2.0 * x + 12.0 * y + 3.0;
    (4.0 * x / d, 6.0 * y / d)
}

fn uv_to_xy(u: f64, v: f64) -> (f64, f64) {
    let d = 2.0 * u - 8.0 * v + 4.0;
    (3.0 * u / d, 2.0 * v / d)
}

fn planckian_uv(t: f64) -> (f64, f64) {
    let (x, y) = planckian_xy(t);
    xy_to_uv(x, y)
}

/// Unit normal to the locus at T pointing to the green side (+Duv).
fn green_normal(t: f64) -> (f64, f64) {
    let (u0, v0) = planckian_uv(t - 10.0);
    let (u1, v1) = planckian_uv(t + 10.0);
    let mut du = u1 - u0;
    let mut dv = v1 - v0;
    let len = (du * du + dv * dv).sqrt();
    if len < 1e-12 {
        return (0.0, 1.0);
    }
    du /= len;
    dv /= len;
    (dv, -du)
}

/// XYZ (Y = 1) of the illuminant at (K, tint); +tint = magenta side of the locus.
fn illuminant_xyz(kelvin: f64, tint: f64) -> (f64, f64, f64) {
    let k = kelvin.clamp(MIN_KELVIN, MAX_KELVIN);
    let (mut u, mut v) = planckian_uv(k);
    let (nu, nv) = green_normal(k);
    let duv = -tint * DUV_PER_TINT_UNIT;
    u += nu * duv;
    v += nv * duv;
    let (x, y0) = uv_to_xy(u, v);
    let y = if y0 <= 1e-6 { 1e-6 } else { y0 };
    (x / y, 1.0, (1.0 - x - y) / y)
}

/// Closest locus point (CCT) and signed Duv (+ = green) for a chromaticity.
fn uv_to_kelvin_duv(u: f64, v: f64) -> (f64, f64) {
    let dist2 = |t: f64| {
        let (pu, pv) = planckian_uv(t);
        (pu - u) * (pu - u) + (pv - v) * (pv - v)
    };
    let mut best = MIN_KELVIN;
    let mut best_d = f64::MAX;
    let mut t = MIN_KELVIN;
    while t <= MAX_KELVIN {
        let d = dist2(t);
        if d < best_d {
            best_d = d;
            best = t;
        }
        t += 100.0;
    }
    let mut lo = MIN_KELVIN.max(best - 100.0);
    let mut hi = MAX_KELVIN.min(best + 100.0);
    for _ in 0..40 {
        let m1 = lo + (hi - lo) / 3.0;
        let m2 = hi - (hi - lo) / 3.0;
        if dist2(m1) < dist2(m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    let k = (lo + hi) / 2.0;
    let (u0, v0) = planckian_uv(k);
    let (nu, nv) = green_normal(k);
    let duv = (u - u0) * nu + (v - v0) * nv;
    (k, duv)
}

// ---- camera white balance ----------------------------------------------------

fn neutral_camera_response(cc: &CameraColorInfo, kelvin: f64, tint: f64) -> Option<[f64; 3]> {
    let (x, y, z) = illuminant_xyz(kelvin, tint);
    let srgb = mul3(&XYZ_TO_SRGB, x, y, z);
    let inv = invert3(&cc.rgb_cam)?;
    let cam_scaled = mul3(&inv, srgb[0], srgb[1], srgb[2]);
    let mut raw = [0.0; 3];
    for i in 0..3 {
        raw[i] = cam_scaled[i] / cc.pre_mul[i];
        if !(raw[i] > 1e-9) {
            return None;
        }
    }
    Some(normalize_green(raw))
}

/// Camera multipliers (G = 1) that neutralise the illuminant at (K, tint).
pub fn kelvin_tint_to_cam_mul(cc: &CameraColorInfo, kelvin: f64, tint: f64) -> Option<[f64; 3]> {
    let raw = neutral_camera_response(cc, kelvin, tint)?;
    Some(normalize_green([1.0 / raw[0], 1.0 / raw[1], 1.0 / raw[2]]))
}

/// (K, tint) of the illuminant that a set of camera multipliers neutralises.
pub fn cam_mul_to_kelvin_tint(cc: &CameraColorInfo, mul: &[f64; 3]) -> Option<(f64, f64)> {
    if !(mul[0] > 0.0 && mul[1] > 0.0 && mul[2] > 0.0) {
        return None;
    }
    let mut cam_scaled = [0.0; 3];
    for i in 0..3 {
        cam_scaled[i] = cc.pre_mul[i] / mul[i];
    }
    let srgb = mul3(&cc.rgb_cam, cam_scaled[0], cam_scaled[1], cam_scaled[2]);
    let xyz = mul3(&SRGB_TO_XYZ, srgb[0], srgb[1], srgb[2]);
    let sum = xyz[0] + xyz[1] + xyz[2];
    if !(sum > 1e-9 && xyz[1] > 0.0) {
        return None;
    }
    let (u, v) = xy_to_uv(xyz[0] / sum, xyz[1] / sum);
    let (k, duv) = uv_to_kelvin_duv(u, v);
    Some((k, (-duv / DUV_PER_TINT_UNIT).clamp(-100.0, 100.0)))
}

/// As-shot (K, tint) from the camera's recorded multipliers.
pub fn as_shot(cc: &CameraColorInfo) -> Option<(f64, f64)> {
    cam_mul_to_kelvin_tint(cc, &cc.cam_mul)
}

/// 3×3 matrix (row-major) that re-balances linear sRGB pixels — already balanced to
/// `reference` — to (K, tint): M = rgb_cam · diag(target / reference) · rgb_cam⁻¹,
/// normalised so a neutral grey keeps its luminance.
pub fn white_balance_matrix(
    cc: &CameraColorInfo,
    kelvin: f64,
    tint: f64,
    reference: WhiteBalanceReference,
) -> Option<[f32; 9]> {
    if !cc.is_valid() {
        return None;
    }
    let target = kelvin_tint_to_cam_mul(cc, kelvin, tint)?;
    let ref_mul = if reference == WhiteBalanceReference::AsShot { cc.cam_mul } else { cc.pre_mul };
    let inv = invert3(&cc.rgb_cam)?;

    let mut gains = [0.0; 3];
    for i in 0..3 {
        if !(ref_mul[i] > 0.0) {
            return None;
        }
        gains[i] = target[i] / ref_mul[i];
    }
    let mut md = [0.0f64; 9];
    for r in 0..3 {
        for c in 0..3 {
            let mut s = 0.0;
            for k in 0..3 {
                s += cc.rgb_cam[r * 3 + k] * gains[k] * inv[k * 3 + c];
            }
            md[r * 3 + c] = s;
        }
    }

    let (yr, yg, yb) = (0.2126729, 0.7151522, 0.0721750);
    let gr = md[0] + md[1] + md[2];
    let gg = md[3] + md[4] + md[5];
    let gb = md[6] + md[7] + md[8];
    let y = yr * gr + yg * gg + yb * gb;
    if !(y > 1e-6) {
        return None;
    }
    Some(md.map(|v| (v / y) as f32))
}

/// Camera multipliers that would make a sampled linear-sRGB pixel neutral (WB picker).
pub fn neutralizing_cam_mul(
    cc: &CameraColorInfo,
    r: f64,
    g: f64,
    b: f64,
    reference: WhiteBalanceReference,
) -> Option<[f64; 3]> {
    if !cc.is_valid() {
        return None;
    }
    let inv = invert3(&cc.rgb_cam)?;
    let patch = mul3(&inv, r, g, b);
    let white = mul3(&inv, 1.0, 1.0, 1.0);
    let ref_mul = if reference == WhiteBalanceReference::AsShot { cc.cam_mul } else { cc.pre_mul };
    let mut mul = [0.0; 3];
    for i in 0..3 {
        if !(patch[i] > 1e-9 && white[i] > 1e-9) {
            return None;
        }
        mul[i] = ref_mul[i] * white[i] / patch[i];
    }
    Some(normalize_green(mul))
}

// ---- small linear algebra ----------------------------------------------------

fn mul3(m: &[f64; 9], a: f64, b: f64, c: f64) -> [f64; 3] {
    [
        m[0] * a + m[1] * b + m[2] * c,
        m[3] * a + m[4] * b + m[5] * c,
        m[6] * a + m[7] * b + m[8] * c,
    ]
}

pub fn invert3(m: &[f64; 9]) -> Option<[f64; 9]> {
    let (a, b, c, d, e, f, g, h, i) = (m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8]);
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 || det.is_nan() {
        return None;
    }
    let s = 1.0 / det;
    Some([
        (e * i - f * h) * s,
        (c * h - b * i) * s,
        (b * f - c * e) * s,
        (f * g - d * i) * s,
        (a * i - c * g) * s,
        (c * d - a * f) * s,
        (d * h - e * g) * s,
        (b * g - a * h) * s,
        (a * e - b * d) * s,
    ])
}

pub fn normalize_green(v: [f64; 3]) -> [f64; 3] {
    let g = if v[1] > 1e-12 { v[1] } else { 1.0 };
    [v[0] / g, 1.0, v[2] / g]
}
