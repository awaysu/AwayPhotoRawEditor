//! 處理版本 3 (`pipeline_version` 2): the wide-gamut linear colour pipeline.
//!
//! Versions 0 and 1 run in `pipeline.rs` exactly as before (hashtest guards them); nothing
//! here is reachable from them. What changes for version 3:
//!
//! * **Source.** A RAW is decoded once more by LibRaw as *linear camera RGB* — no white
//!   balance, no colour matrix, no gamma, no auto-bright, 16-bit, sensor clip = 1.0
//!   (`libraw::decode_linear`). Highlight reconstruction runs on that, at full resolution,
//!   when the cache is built ([`prepare_linear_source`]); LibRaw's auto-bright is
//!   reproduced as a stored gain so a version-3 photo starts as bright as version 2.
//!   Everything else (JPEG, TIFF, a RAW LibRaw cannot read) is a gamma-encoded source and
//!   is linearised first.
//! * **One pass, linear f32.** Highlight clip → white balance + camera matrix into linear
//!   Rec.2020 (exposure folded in) → tone (the version-2 tone curve, a highlight shoulder
//!   and the point curves, applied in the encoded domain) → vibrance / saturation / HSL in
//!   OkLCh → Rec.2020 → sRGB with gamut compression (desaturate towards the luminance
//!   until in range) → the encoding version 1 / 2 output (LibRaw's BT.709 curve, so the
//!   rest of the pipeline, the display and the export see the same kind of values).
//!
//! The white balance and exposure maths are chosen so that, with no version-3-only
//! values, a photo renders like version 2 before its tone curve: 升級處理版本 2 → 3
//! keeps exposure and white balance as they are.
//!
//! The GPU kernel (`awpr-gpu`, `PIXEL_V3`) transliterates [`pixel`] and reads its
//! parameters from [`V3Params::words`]; change both together.
//!
//! Highlight reconstruction, the OkLCh colour tools, the band weights and the monotone
//! curve follow lightcraft (MIT / Apache-2.0, https://github.com/storytold/lightcraft —
//! `crates/raw/src/highlight.rs`, `crates/pipeline/src/colorops.rs`,
//! `crates/color/src/perceptual.rs`, `crates/color/src/spline.rs`).

use crate::buffer::FloatImage;
use crate::color;
use crate::model::{CameraColorInfo, ImageAdjustments};
use crate::pipeline::{white_balance_multipliers, ProcessContext, SourceKind, SourcePrimaries};
use crate::tone;
use rayon::prelude::*;
use std::f32::consts::{PI, TAU};
use std::sync::OnceLock;

// ---- colour spaces (D65) -----------------------------------------------------------

/// Linear sRGB → linear Rec.2020 (ITU-R BT.2087; rows sum to 1, so white stays white).
const SRGB_TO_2020: [f64; 9] = [
    0.627403895934699, 0.329283038377884, 0.043313065687417, //
    0.069097289358232, 0.919540395075459, 0.011362315566309, //
    0.016391438875150, 0.088013307877226, 0.895595253247624,
];

const REC2020_TO_XYZ: [f64; 9] = [
    0.636958048301291, 0.144616903586208, 0.168880975164172, //
    0.262700212011267, 0.677998071518871, 0.059301716469862, //
    0.000000000000000, 0.028072693049087, 1.060985057710791,
];

/// Björn Ottosson's OkLab, from XYZ (as lightcraft's `perceptual.rs`).
const XYZ_TO_LMS: [f64; 9] = [
    0.8189330101, 0.3618667424, -0.1288597137, //
    0.0329845436, 0.9293118715, 0.0361456387, //
    0.0482003018, 0.2643662691, 0.6338517070,
];

const LMS_TO_LAB: [f64; 9] = [
    0.2104542553, 0.7936177850, -0.0040720468, //
    1.9779984951, -2.4285922050, 0.4505937099, //
    0.0259040371, 0.7827717662, -0.8086757660,
];

/// Linear Display P3 → linear sRGB (D65; P3's primaries, sRGB's white).
pub const P3_TO_SRGB: [f64; 9] = [
    1.224940176, -0.224940176, 0.0, //
    -0.042056955, 1.042056955, 0.0, //
    -0.019637555, -0.078636046, 1.098273600,
];

/// sRGB luminance weights: the gamut compression's grey axis.
const SRGB_Y: [f32; 3] = [0.2126729, 0.7151522, 0.0721750];

fn mat_mul(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut m = [0.0; 9];
    for r in 0..3 {
        for c in 0..3 {
            m[r * 3 + c] = (0..3).map(|k| a[r * 3 + k] * b[k * 3 + c]).sum();
        }
    }
    m
}

fn to_f32(m: &[f64; 9]) -> [f32; 9] {
    m.map(|v| v as f32)
}

/// The constant matrices, computed once in f64.
pub struct Spaces {
    pub srgb_to_2020: [f64; 9],
    pub rec2020_to_srgb: [f64; 9],
    /// Linear Rec.2020 → LMS, LMS → Rec.2020, cube-rooted LMS → Lab, Lab → cube-rooted LMS.
    pub to_lms: [f32; 9],
    pub from_lms: [f32; 9],
    pub to_lab: [f32; 9],
    pub from_lab: [f32; 9],
}

pub fn spaces() -> &'static Spaces {
    static S: OnceLock<Spaces> = OnceLock::new();
    S.get_or_init(|| {
        let srgb_to_2020 = SRGB_TO_2020;
        let rec2020_to_srgb = color::invert3(&srgb_to_2020).expect("sRGB matrix");
        let to_lms = mat_mul(&XYZ_TO_LMS, &REC2020_TO_XYZ);
        Spaces {
            srgb_to_2020,
            rec2020_to_srgb,
            to_lms: to_f32(&to_lms),
            from_lms: to_f32(&color::invert3(&to_lms).expect("LMS matrix")),
            to_lab: to_f32(&LMS_TO_LAB),
            from_lab: to_f32(&color::invert3(&LMS_TO_LAB).expect("Lab matrix")),
        }
    })
}

#[inline(always)]
fn mul3(m: &[f32], v: [f32; 3]) -> [f32; 3] {
    [
        m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
        m[3] * v[0] + m[4] * v[1] + m[5] * v[2],
        m[6] * v[0] + m[7] * v[1] + m[8] * v[2],
    ]
}

// ---- transfer: LibRaw's gamm {0.45, 4.5} (BT.709), extended above 1 ---------------

/// Linear → encoded. Above 1 the power segment continues (the highlight shoulder and the
/// tone LUT see how far over white a value is).
#[inline(always)]
pub fn oetf(l: f32) -> f32 {
    if !(l > 0.0) {
        0.0
    } else if l < 0.018 {
        4.5 * l
    } else {
        1.099 * l.powf(0.45) - 0.099
    }
}

/// Encoded → linear.
#[inline(always)]
pub fn eotf(v: f32) -> f32 {
    if !(v > 0.0) {
        0.0
    } else if v < 0.081 {
        v / 4.5
    } else {
        ((v + 0.099) / 1.099).powf(1.0 / 0.45)
    }
}

// ---- OkLab / OkLCh -------------------------------------------------------------------

/// Signed cube root as the GPU computes it (WGSL has no cbrt).
#[inline(always)]
fn cbrt_s(x: f32) -> f32 {
    if x == 0.0 {
        0.0
    } else if x < 0.0 {
        -(-x).powf(1.0 / 3.0)
    } else {
        x.powf(1.0 / 3.0)
    }
}

/// Linear Rec.2020 → OkLab `[L, a, b]`.
#[inline(always)]
pub fn oklab_from_2020(rgb: [f32; 3]) -> [f32; 3] {
    let s = spaces();
    let lms = mul3(&s.to_lms, rgb).map(cbrt_s);
    mul3(&s.to_lab, lms)
}

/// OkLab → linear Rec.2020.
#[inline(always)]
pub fn oklab_to_2020(lab: [f32; 3]) -> [f32; 3] {
    let s = spaces();
    let lms = mul3(&s.from_lab, lab).map(|v| v * v * v);
    mul3(&s.from_lms, lms)
}

/// Angle wrapped to −π..π, written the way the shader writes it.
#[inline(always)]
fn wrap(a: f32) -> f32 {
    let t = a + PI;
    t - TAU * (t / TAU).floor() - PI
}

/// `a` mod 2π in 0..2π (`rem_euclid`, as the shader writes it).
#[inline(always)]
fn rem_tau(a: f32) -> f32 {
    a - TAU * (a / TAU).floor()
}

/// Band centres as sRGB HSV hues (紅 橙 黃 綠 青 藍 紫 洋紅), lightcraft's `MIXER_HUES`.
pub const BAND_SRGB_HUES: [f64; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 225.0, 270.0, 315.0];

/// OkLCh hue (radians) of a fully saturated sRGB colour with HSV hue `deg`.
pub fn oklch_hue_of_srgb_hue(deg: f64) -> f32 {
    let h = deg.rem_euclid(360.0) / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as i32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    // Display values → linear (the sRGB curve: these are colours as a viewer sees them).
    let lin = |v: f64| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    let s = spaces();
    let m = &s.srgb_to_2020;
    let (r, g, b) = (lin(r), lin(g), lin(b));
    let rec = [
        (m[0] * r + m[1] * g + m[2] * b) as f32,
        (m[3] * r + m[4] * g + m[5] * b) as f32,
        (m[6] * r + m[7] * g + m[8] * b) as f32,
    ];
    let lab = oklab_from_2020(rec);
    lab[2].atan2(lab[1])
}

/// OkLCh hue of each band centre.
pub fn band_hues() -> &'static [f32; 8] {
    static H: OnceLock<[f32; 8]> = OnceLock::new();
    H.get_or_init(|| BAND_SRGB_HUES.map(oklch_hue_of_srgb_hue))
}

/// Partition-of-unity weights of hue `h` over the 8 bands: a raised cosine between the two
/// neighbouring centres, so every hue's weights sum to 1.
#[inline(always)]
pub fn band_weights(h: f32, hues: &[f32; 8]) -> [f32; 8] {
    let mut w = [0.0f32; 8];
    for i in 0..8 {
        let a = hues[i];
        let b = hues[(i + 1) % 8];
        let span = rem_tau(wrap(b - a));
        let d = rem_tau(wrap(h - a));
        if d <= span {
            let t = d / span;
            let s = 0.5 - 0.5 * (t * PI).cos();
            w[i] += 1.0 - s;
            w[(i + 1) % 8] += s;
            break;
        }
    }
    w
}

// ---- curves --------------------------------------------------------------------------

/// A monotone cubic Hermite spline (Fritsch–Carlson) through points in 0..1 × 0..1: it
/// never overshoots between points, so a curve cannot invert tones by accident.
#[derive(Clone, Debug, PartialEq)]
pub struct MonotoneCurve {
    xs: Vec<f64>,
    ys: Vec<f64>,
    ms: Vec<f64>,
}

impl MonotoneCurve {
    /// Sorted by x, duplicates merged, clamped to 0..1; fewer than two points → identity.
    pub fn new(points: &[(f64, f64)]) -> Self {
        let mut p: Vec<(f64, f64)> = points
            .iter()
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .map(|&(x, y)| (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)))
            .collect();
        p.sort_by(|a, b| a.0.total_cmp(&b.0));
        p.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
        if p.len() < 2 {
            p = vec![(0.0, 0.0), (1.0, 1.0)];
        }
        let n = p.len();
        let xs: Vec<f64> = p.iter().map(|q| q.0).collect();
        let ys: Vec<f64> = p.iter().map(|q| q.1).collect();
        let d: Vec<f64> = (0..n - 1).map(|i| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i])).collect();
        let mut ms = vec![0.0; n];
        ms[0] = d[0];
        ms[n - 1] = d[n - 2];
        for i in 1..n - 1 {
            ms[i] = if d[i - 1] * d[i] <= 0.0 { 0.0 } else { (d[i - 1] + d[i]) / 2.0 };
        }
        for i in 0..n - 1 {
            if d[i] == 0.0 {
                ms[i] = 0.0;
                ms[i + 1] = 0.0;
                continue;
            }
            let a = ms[i] / d[i];
            let b = ms[i + 1] / d[i];
            let s = a * a + b * b;
            if s > 9.0 {
                let t = 3.0 / s.sqrt();
                ms[i] = t * a * d[i];
                ms[i + 1] = t * b * d[i];
            }
        }
        Self { xs, ys, ms }
    }

    pub fn eval(&self, x: f64) -> f64 {
        let n = self.xs.len();
        if x <= self.xs[0] {
            return self.ys[0];
        }
        if x >= self.xs[n - 1] {
            return self.ys[n - 1];
        }
        let i = match self.xs.binary_search_by(|v| v.total_cmp(&x)) {
            Ok(i) => return self.ys[i],
            Err(i) => i - 1,
        };
        let h = self.xs[i + 1] - self.xs[i];
        let t = (x - self.xs[i]) / h;
        let (t2, t3) = (t * t, t * t * t);
        let v = (2.0 * t3 - 3.0 * t2 + 1.0) * self.ys[i]
            + (t3 - 2.0 * t2 + t) * h * self.ms[i]
            + (-2.0 * t3 + 3.0 * t2) * self.ys[i + 1]
            + (t3 - t2) * h * self.ms[i + 1];
        v.clamp(0.0, 1.0)
    }
}

/// True for an empty point list or one on the diagonal.
pub fn curve_is_identity(points: &[(f64, f64)]) -> bool {
    points.iter().all(|&(x, y)| (x - y).abs() < 1e-9) && (points.is_empty() || points.len() >= 2)
}

fn curve_of(points: &[(f64, f64)]) -> Option<MonotoneCurve> {
    (!curve_is_identity(points)).then(|| MonotoneCurve::new(points))
}

// ---- tone LUTs -----------------------------------------------------------------------

/// Entries per channel, over encoded values 0..LUT_MAX.
pub const LUT_SIZE: usize = 4096;
/// Encoded 2.0 ≈ 4.7× linear white: headroom for reconstructed highlights.
pub const LUT_MAX: f32 = 2.0;

/// The highlight shoulder: identity below `knee`, then a tanh roll-off that reaches white
/// asymptotically. `knee` = 1 (高光復原 0) is a hard clip at white, as in version 2.
#[inline]
pub fn shoulder(x: f64, knee: f64) -> f64 {
    if x <= knee {
        x
    } else if knee >= 1.0 {
        1.0
    } else {
        knee + (1.0 - knee) * ((x - knee) / (1.0 - knee)).tanh()
    }
}

/// The knee for a 高光復原 value (0..100).
pub fn knee_of(highlight_recovery: f64) -> f64 {
    1.0 - 0.25 * (highlight_recovery / 100.0).clamp(0.0, 1.0)
}

/// R, G, B tables (LUT_SIZE each): shoulder → the version-2 tone curve → the RGB curve →
/// the channel's curve. Composed once per render, sampled once per channel per pixel.
pub fn build_luts(a: &ImageAdjustments) -> Vec<f32> {
    let tone = tone::build_lut(a);
    let knee = knee_of(a.highlight_recovery);
    let master = curve_of(&a.curve_rgb);
    let chans = [curve_of(&a.curve_red), curve_of(&a.curve_green), curve_of(&a.curve_blue)];
    let mut out = vec![0.0f32; LUT_SIZE * 3];
    for (c, chan) in chans.iter().enumerate() {
        for i in 0..LUT_SIZE {
            let x = i as f64 / (LUT_SIZE - 1) as f64 * LUT_MAX as f64;
            let s = shoulder(x, knee);
            let mut v = tone::sample(&tone, s as f32) as f64;
            if let Some(m) = &master {
                v = m.eval(v);
            }
            if let Some(ch) = chan {
                v = ch.eval(v);
            }
            out[c * LUT_SIZE + i] = v.clamp(0.0, 1.0) as f32;
        }
    }
    out
}

/// Linear interpolation over 0..LUT_MAX, clamped at both ends.
#[inline(always)]
pub fn sample_lut(lut: &[f32], x: f32) -> f32 {
    if !(x > 0.0) {
        return lut[0];
    }
    let f = x.min(LUT_MAX) * ((LUT_SIZE - 1) as f32 / LUT_MAX);
    let i = (f as usize).min(LUT_SIZE - 2);
    let t = f - i as f32;
    lut[i] + (lut[i + 1] - lut[i]) * t
}

// ---- per-render parameters -----------------------------------------------------------

#[derive(Debug, Clone)]
pub struct V3Params {
    /// The source is linear camera RGB (else gamma-encoded RGB, linearised first).
    pub linear_camera: bool,
    /// Source (linear) → linear Rec.2020: white balance, camera matrix, exposure, gain.
    pub m_in: [f32; 9],
    /// Camera multipliers of the chosen white balance (the clip-neutral limit).
    pub wb: [f32; 3],
    /// 高光復原 0..1: how much of the reconstructed highlights survive the clip.
    pub hl: f32,
    /// `build_luts`.
    pub luts: Vec<f32>,
    pub vib: f32,
    pub sat: f32,
    /// OkLCh hue of skin (vibrance protects it).
    pub skin: f32,
    /// Per band: hue shift (radians), chroma scale − 1, lightness shift.
    pub hsl_hue: [f32; 8],
    pub hsl_sat: [f32; 8],
    pub hsl_lum: [f32; 8],
    pub band_hues: [f32; 8],
    pub mixer: bool,
    pub color_ops: bool,
    /// Linear Rec.2020 → linear sRGB.
    pub out_m: [f32; 9],
}

impl V3Params {
    pub fn new(adj: &ImageAdjustments, ctx: &ProcessContext) -> Self {
        let sp = spaces();
        let exp = 2.0f64.powf(adj.exposure);
        let cam = ctx.camera.as_ref().filter(|c| c.is_valid());
        let mut linear_camera = false;
        let mut wb = [1.0f32; 3];
        let m_src: [f64; 9] = match (ctx.source_kind, cam) {
            (SourceKind::LinearCamera { gain }, Some(cam)) => {
                linear_camera = true;
                let (m, target) = camera_to_srgb(cam, adj.temperature, adj.tint);
                wb = target.map(|v| v as f32);
                m.map(|v| v * exp * gain as f64)
            }
            (SourceKind::LinearCamera { gain }, None) => {
                // Linear data without camera colour cannot happen from the loader; treat
                // it as linear sRGB rather than render garbage.
                linear_camera = true;
                let k = exp * gain as f64;
                [k, 0.0, 0.0, 0.0, k, 0.0, 0.0, 0.0, k]
            }
            (SourceKind::Encoded, Some(cam)) => {
                match color::white_balance_matrix(cam, adj.temperature, adj.tint, ctx.white_balance_reference) {
                    Some(m) => m.map(|v| v as f64 * exp),
                    None => black_body(adj, exp),
                }
            }
            (SourceKind::Encoded, None) => black_body(adj, exp),
        };
        // A Display P3 source: its linear values go to linear sRGB first, which the white
        // balance (defined on linear sRGB) and the Rec.2020 step then take as usual.
        let m_src = if !linear_camera && ctx.source_primaries == SourcePrimaries::DisplayP3 { mat_mul(&m_src, &P3_TO_SRGB) } else { m_src };
        let m_in = to_f32(&mat_mul(&sp.srgb_to_2020, &m_src));

        let k = |v: f64| (v / 100.0) as f32;
        let hsl_hue = adj.hsl_hue.map(|v| k(v.clamp(-100.0, 100.0)) * 0.5);
        let hsl_sat = adj.hsl_saturation.map(|v| k(v.clamp(-100.0, 100.0)));
        let hsl_lum = adj.hsl_luminance.map(|v| k(v.clamp(-100.0, 100.0)) * 0.18);
        let mixer = hsl_hue.iter().chain(&hsl_sat).chain(&hsl_lum).any(|&v| v != 0.0);
        let vib = k(adj.vibrance);
        let sat = k(adj.saturation);
        Self {
            linear_camera,
            m_in,
            wb,
            hl: k(adj.highlight_recovery).clamp(0.0, 1.0),
            luts: build_luts(adj),
            vib,
            sat,
            skin: oklch_hue_of_srgb_hue(25.0),
            hsl_hue,
            hsl_sat,
            hsl_lum,
            band_hues: *band_hues(),
            mixer,
            color_ops: mixer || vib != 0.0 || sat != 0.0,
            out_m: to_f32(&sp.rec2020_to_srgb),
        }
    }

    /// The parameter block the GPU kernel reads (see `W_*`).
    pub fn words(&self, width: usize, height: usize) -> Vec<f32> {
        let s = spaces();
        let mut w = vec![0.0f32; W_COUNT];
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        w[W_LINEAR] = b(self.linear_camera);
        w[W_COLOR] = b(self.color_ops);
        w[W_MIXER] = b(self.mixer);
        w[W_HL] = self.hl;
        w[W_VIB] = self.vib;
        w[W_SAT] = self.sat;
        w[W_SKIN] = self.skin;
        w[W_WIDTH] = width as f32;
        w[W_HEIGHT] = height as f32;
        w[W_WB..W_WB + 3].copy_from_slice(&self.wb);
        w[W_M_IN..W_M_IN + 9].copy_from_slice(&self.m_in);
        w[W_OUT..W_OUT + 9].copy_from_slice(&self.out_m);
        w[W_OUT_Y..W_OUT_Y + 3].copy_from_slice(&SRGB_Y);
        w[W_TO_LMS..W_TO_LMS + 9].copy_from_slice(&s.to_lms);
        w[W_FROM_LMS..W_FROM_LMS + 9].copy_from_slice(&s.from_lms);
        w[W_TO_LAB..W_TO_LAB + 9].copy_from_slice(&s.to_lab);
        w[W_FROM_LAB..W_FROM_LAB + 9].copy_from_slice(&s.from_lab);
        w[W_BANDS..W_BANDS + 8].copy_from_slice(&self.band_hues);
        w[W_HUE..W_HUE + 8].copy_from_slice(&self.hsl_hue);
        w[W_HSAT..W_HSAT + 8].copy_from_slice(&self.hsl_sat);
        w[W_HLUM..W_HLUM + 8].copy_from_slice(&self.hsl_lum);
        w
    }
}

// Word offsets of `V3Params::words` (the WGSL declares the same numbers).
pub const W_LINEAR: usize = 0;
pub const W_COLOR: usize = 1;
pub const W_MIXER: usize = 2;
pub const W_HL: usize = 3;
pub const W_VIB: usize = 4;
pub const W_SAT: usize = 5;
pub const W_SKIN: usize = 6;
pub const W_WIDTH: usize = 7;
pub const W_HEIGHT: usize = 8;
pub const W_WB: usize = 9;
pub const W_M_IN: usize = 12;
pub const W_OUT: usize = 21;
pub const W_OUT_Y: usize = 30;
pub const W_TO_LMS: usize = 33;
pub const W_FROM_LMS: usize = 42;
pub const W_TO_LAB: usize = 51;
pub const W_FROM_LAB: usize = 60;
pub const W_BANDS: usize = 69;
pub const W_HUE: usize = 77;
pub const W_HSAT: usize = 85;
pub const W_HLUM: usize = 93;
pub const W_COUNT: usize = 101;

fn black_body(adj: &ImageAdjustments, exp: f64) -> [f64; 9] {
    let (r, g, b) = white_balance_multipliers(adj.temperature, adj.tint);
    // Through f32 like version 1's multipliers.
    let (r, g, b) = ((r as f32) as f64 * exp, (g as f32) as f64 * exp, (b as f32) as f64 * exp);
    [r, 0.0, 0.0, 0.0, g, 0.0, 0.0, 0.0, b]
}

/// Linear camera RGB → linear sRGB at (K, tint), and the camera multipliers used.
///
/// Version 2 shows `M · lin` where lin = rgb_cam · diag(pre_mul) · raw (LibRaw's decode)
/// and M = rgb_cam · diag(target / pre_mul) · rgb_cam⁻¹ / y (`color::white_balance_matrix`,
/// y keeping a daylight grey's luminance). Folding the two gives rgb_cam · diag(target) / y:
/// the same pixels, without the round trip through LibRaw's clipped sRGB.
pub fn camera_to_srgb(cam: &CameraColorInfo, kelvin: f64, tint: f64) -> ([f64; 9], [f64; 3]) {
    let target = color::kelvin_tint_to_cam_mul(cam, kelvin, tint).unwrap_or(cam.pre_mul);
    let inv = color::invert3(&cam.rgb_cam).unwrap_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    let gains = [target[0] / cam.pre_mul[0], target[1] / cam.pre_mul[1], target[2] / cam.pre_mul[2]];
    let mut md = [0.0f64; 9];
    for r in 0..3 {
        for c in 0..3 {
            md[r * 3 + c] = (0..3).map(|k| cam.rgb_cam[r * 3 + k] * gains[k] * inv[k * 3 + c]).sum();
        }
    }
    let (yr, yg, yb) = (0.2126729, 0.7151522, 0.0721750);
    let y = yr * (md[0] + md[1] + md[2]) + yg * (md[3] + md[4] + md[5]) + yb * (md[6] + md[7] + md[8]);
    let y = if y > 1e-6 { y } else { 1.0 };
    let mut m = [0.0f64; 9];
    for r in 0..3 {
        for c in 0..3 {
            m[r * 3 + c] = cam.rgb_cam[r * 3 + c] * target[c] / y;
        }
    }
    (m, target)
}

// ---- the pixel ---------------------------------------------------------------------------

#[inline(always)]
fn color_ops(p: &V3Params, rgb: [f32; 3]) -> [f32; 3] {
    let mut lab = oklab_from_2020(rgb);
    if !p.mixer {
        // Chroma only: scale a, b (the OkLCh round trip without its sin / cos).
        let c0 = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
        let mut c = c0;
        if p.vib != 0.0 {
            let low = 1.0 - (c / 0.22).clamp(0.0, 1.0);
            let skin = if p.vib > 0.0 {
                let d = wrap(lab[2].atan2(lab[1]) - p.skin) / 0.35;
                1.0 - 0.6 * (-(d * d)).exp()
            } else {
                1.0
            };
            c *= (1.0 + p.vib * low * low * skin * 1.2).max(0.0);
        }
        if p.sat != 0.0 {
            c *= (1.0 + p.sat).max(0.0);
        }
        if c0 > 0.0 {
            let k = c / c0;
            lab[1] *= k;
            lab[2] *= k;
        }
        return oklab_to_2020(lab);
    }
    let mut l = lab[0];
    let mut c = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    let mut h = lab[2].atan2(lab[1]);
    let w = band_weights(h, &p.band_hues);
    let (mut dh, mut ds, mut dl) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..8 {
        dh += w[i] * p.hsl_hue[i];
        ds += w[i] * p.hsl_sat[i];
        dl += w[i] * p.hsl_lum[i];
    }
    // Near-neutral colours have no meaningful hue: fade the band edits out towards grey.
    let chroma_w = (c / 0.12).min(1.0);
    h += dh * chroma_w;
    c *= (1.0 + ds).max(0.0);
    l += dl * chroma_w * l.max(0.05).sqrt();
    if p.vib != 0.0 {
        let low = 1.0 - (c / 0.22).clamp(0.0, 1.0);
        let skin = if p.vib > 0.0 {
            let d = wrap(h - p.skin) / 0.35;
            1.0 - 0.6 * (-(d * d)).exp()
        } else {
            1.0
        };
        c *= (1.0 + p.vib * low * low * skin * 1.2).max(0.0);
    }
    if p.sat != 0.0 {
        c *= (1.0 + p.sat).max(0.0);
    }
    oklab_to_2020([l, c * h.cos(), c * h.sin()])
}

/// One pixel, in place (RGB; alpha untouched). The output is encoded like version 1 / 2.
#[inline(always)]
pub fn pixel(p: &V3Params, px: &mut [f32]) {
    let mut v = [px[0], px[1], px[2]];
    if p.linear_camera {
        // Clip-neutral at the chosen white balance (each channel limited to the lowest
        // channel's clip level), blended towards the reconstructed values by 高光復原.
        let lim = p.wb[0].min(p.wb[1]).min(p.wb[2]);
        for c in 0..3 {
            let cn = (v[c] * p.wb[c]).min(lim) / p.wb[c];
            v[c] = cn + p.hl * (v[c] - cn);
        }
    } else {
        v = v.map(eotf);
    }
    let mut lin = mul3(&p.m_in, v);

    // Tone, in the encoded domain, per channel.
    for c in 0..3 {
        let e = sample_lut(&p.luts[c * LUT_SIZE..(c + 1) * LUT_SIZE], oetf(lin[c]));
        lin[c] = eotf(e);
    }

    if p.color_ops {
        lin = color_ops(p, lin);
    }

    // Rec.2020 → sRGB, then into the sRGB gamut by desaturating towards the luminance.
    let mut s = mul3(&p.out_m, lin);
    let yy = (SRGB_Y[0] * s[0] + SRGB_Y[1] * s[1] + SRGB_Y[2] * s[2]).clamp(0.0, 1.0);
    let mut tg = 1.0f32;
    for &c in &s {
        if c < 0.0 {
            tg = tg.min(yy / (yy - c).max(1e-9));
        } else if c > 1.0 {
            tg = tg.min((1.0 - yy) / (c - yy).max(1e-9));
        }
    }
    if tg < 1.0 {
        s = s.map(|c| yy + (c - yy) * tg);
    }
    px[0] = oetf(s[0].clamp(0.0, 1.0));
    px[1] = oetf(s[1].clamp(0.0, 1.0));
    px[2] = oetf(s[2].clamp(0.0, 1.0));
}

/// A Display P3 source as sRGB for the older versions (which only read sRGB): decode,
/// convert, clip to the sRGB gamut, encode — the transfer curve the pipeline assumes.
pub fn p3_to_srgb_encoded(img: &mut FloatImage) {
    let m = to_f32(&P3_TO_SRGB);
    img.par_rows_mut(|_, row| {
        for px in row.chunks_exact_mut(4) {
            let s = mul3(&m, [eotf(px[0]), eotf(px[1]), eotf(px[2])]);
            for c in 0..3 {
                px[c] = oetf(s[c].clamp(0.0, 1.0));
            }
        }
    });
}

/// The version-3 colour pass over a whole image, in place.
pub fn apply(buf: &mut FloatImage, p: &V3Params) {
    buf.par_rows_mut(|_, row| {
        for px in row.chunks_exact_mut(4) {
            pixel(p, px);
        }
    });
}

// ---- highlights (linear camera RGB, before white balance, clip = 1.0) ----------------

/// Sensor values at or above this count as clipped (LibRaw's demosaic softens the edge
/// of a clipped area slightly below 1).
pub const CLIP: f32 = 0.99;

/// Each 2 × 2 block of a `w × h` level becomes the mean of its valid entries.
fn downsample(w: usize, h: usize, cv: &mut [[f32; 3]], cval: &mut [bool], at: impl Fn(usize) -> Option<[f32; 3]> + Sync) {
    let cw = w.div_ceil(2);
    cv.par_chunks_mut(cw).zip(cval.par_chunks_mut(cw)).enumerate().for_each(|(y, (vrow, okrow))| {
        for x in 0..cw {
            let (mut s, mut n) = ([0f32; 3], 0);
            for dy in 0..2 {
                for dx in 0..2 {
                    let (sx, sy) = (2 * x + dx, 2 * y + dy);
                    if sx < w && sy < h {
                        if let Some(v) = at(sy * w + sx) {
                            for c in 0..3 {
                                s[c] += v[c];
                            }
                            n += 1;
                        }
                    }
                }
            }
            if n > 0 {
                vrow[x] = s.map(|v| v / n as f32);
                okrow[x] = true;
            }
        }
    });
}

/// Bilinear sample of the `cw × ch` coarse level at fine pixel `(x, y)`.
fn upsample(cv: &[[f32; 3]], cw: usize, ch: usize, x: usize, y: usize) -> [f32; 3] {
    let fx = ((x as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (cw - 1) as f32);
    let fy = ((y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (ch - 1) as f32);
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(cw - 1), (y0 + 1).min(ch - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let g = |xx: usize, yy: usize| cv[yy * cw + xx];
    let mut v = [0f32; 3];
    for c in 0..3 {
        let top = g(x0, y0)[c] + (g(x1, y0)[c] - g(x0, y0)[c]) * tx;
        let bot = g(x0, y1)[c] + (g(x1, y1)[c] - g(x0, y1)[c]) * tx;
        v[c] = top + (bot - top) * ty;
    }
    v
}

/// Fill the invalid entries of a level from coarser levels (a normalised-convolution
/// pyramid).
fn fill_invalid(w: usize, h: usize, values: &mut [[f32; 3]], valid: &mut [bool]) {
    if w == 0 || h == 0 || valid.iter().all(|&v| v) || !valid.iter().any(|&v| v) {
        return;
    }
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut cv = vec![[0f32; 3]; cw * ch];
    let mut cval = vec![false; cw * ch];
    {
        let (values, valid) = (&*values, &*valid);
        downsample(w, h, &mut cv, &mut cval, |i| valid[i].then(|| values[i]));
    }
    if cw * ch < w * h {
        fill_invalid(cw, ch, &mut cv, &mut cval);
    } else {
        return;
    }
    values.par_chunks_mut(w).zip(valid.par_chunks_mut(w)).enumerate().for_each(|(y, (vrow, okrow))| {
        for x in 0..w {
            if !okrow[x] {
                vrow[x] = upsample(&cv, cw, ch, x, y);
                okrow[x] = true;
            }
        }
    });
}

#[inline]
fn px3(img: &FloatImage, i: usize) -> [f32; 3] {
    [img.data[i * 4], img.data[i * 4 + 1], img.data[i * 4 + 2]]
}

/// Rebuild partially clipped channels from the unclipped ones, using the white-balanced
/// chromaticity of nearby unclipped pixels (diffused into the clipped area coarse to
/// fine). Fully clipped pixels become neutral (under `wb`) at the brightest plausible
/// level. Unclipped pixels are untouched. Returns how many pixels had a clipped channel.
/// lightcraft's `reconstruct`, on an RGBA buffer.
pub fn reconstruct(img: &mut FloatImage, wb: [f32; 3], clip: f32) -> usize {
    let (w, h) = (img.width, img.height);
    let is_clipped = |p: [f32; 3]| p[0] >= clip || p[1] >= clip || p[2] >= clip;
    let count = img.data.par_chunks(4).filter(|p| is_clipped([p[0], p[1], p[2]])).count();
    if count == 0 {
        return 0;
    }
    let chroma_of = |p: [f32; 3]| -> Option<[f32; 3]> {
        if is_clipped(p) {
            return None;
        }
        let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
        let s = q[0] + q[1] + q[2];
        (s > 1e-4).then(|| q.map(|v| v.max(0.0) / s))
    };
    // The chromaticity field starts at half resolution: a clipped pixel is never valid at
    // full resolution, so its value is always the half-resolution level's sample.
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut cv = vec![[1f32 / 3.0; 3]; cw * ch];
    let mut cval = vec![false; cw * ch];
    {
        let src = &*img;
        downsample(w, h, &mut cv, &mut cval, |i| chroma_of(px3(src, i)));
    }
    if cw * ch < w * h {
        fill_invalid(cw, ch, &mut cv, &mut cval);
    }
    let any_valid = cw * ch < w * h && cval.iter().any(|&v| v);
    let max_level = wb.iter().cloned().fold(0.0f32, f32::max) * clip;
    img.par_rows_mut(|y, row| {
        for (x, px) in row.chunks_exact_mut(4).enumerate() {
            let p = [px[0], px[1], px[2]];
            let cl = [p[0] >= clip, p[1] >= clip, p[2] >= clip];
            if !cl.iter().any(|&b| b) {
                continue;
            }
            let r = if any_valid { upsample(&cv, cw, ch, x, y) } else { [1f32 / 3.0; 3] };
            let q = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
            let (mut sum, mut k) = (0f32, 0usize);
            for c in 0..3 {
                if !cl[c] && r[c] > 1e-3 {
                    sum += q[c] / r[c];
                    k += 1;
                }
            }
            let mut out = q;
            if k == 0 {
                let v = q.iter().cloned().fold(0.0f32, f32::max).max(max_level);
                out = [v; 3];
            } else {
                let sum = sum / k as f32;
                for c in 0..3 {
                    if cl[c] {
                        out[c] = q[c].max(sum * r[c]);
                    }
                }
            }
            px[0] = out[0] / wb[0];
            px[1] = out[1] / wb[1];
            px[2] = out[2] / wb[2];
        }
    });
    count
}

/// The brightness LibRaw's auto-bright would give this photo (`no_auto_bright = 0`,
/// threshold 1 %): balance with pre_mul / min(pre_mul), convert with rgb_cam, clip, take
/// each channel's 99th percentile on its 8192-bin histogram, and scale the highest of them
/// to white. Computed on the unreconstructed decode, as LibRaw sees it.
pub fn auto_bright_gain(img: &FloatImage, cam: &CameraColorInfo) -> f32 {
    let min = cam.pre_mul.iter().cloned().fold(f64::MAX, f64::min).max(1e-9);
    let pre = cam.pre_mul.map(|v| (v / min) as f32);
    let m = to_f32(&cam.rgb_cam);
    let hist = img
        .data
        .par_chunks(4 * 4096)
        .fold(
            || vec![0u32; 3 * 8192],
            |mut h, chunk| {
                for p in chunk.chunks_exact(4) {
                    let q = [p[0] * pre[0], p[1] * pre[1], p[2] * pre[2]];
                    let s = mul3(&m, q);
                    for c in 0..3 {
                        let v = (s[c].clamp(0.0, 1.0) * 65535.0) as u32;
                        h[c * 8192 + (v >> 3).min(8191) as usize] += 1;
                    }
                }
                h
            },
        )
        .reduce(|| vec![0u32; 3 * 8192], |mut a, b| {
            a.iter_mut().zip(&b).for_each(|(x, y)| *x += y);
            a
        });
    let perc = (img.width * img.height) as f64 * 0.01;
    let mut t_white = 0usize;
    for c in 0..3 {
        let mut total = 0u64;
        let mut val = 0x2000usize;
        loop {
            val -= 1;
            if val <= 32 {
                break;
            }
            total += hist[c * 8192 + val] as u64;
            if total as f64 > perc {
                break;
            }
        }
        t_white = t_white.max(val);
    }
    if t_white <= 32 {
        return 1.0;
    }
    (65535.0 / (t_white as f32 * 8.0)).clamp(1.0, 64.0)
}

/// Turn LibRaw's linear decode into the version-3 source: its auto-bright gain (returned)
/// and highlight reconstruction at the as-shot white balance.
pub fn prepare_linear_source(img: &mut FloatImage, cam: &CameraColorInfo) -> f32 {
    let gain = auto_bright_gain(img, cam);
    reconstruct(img, cam.cam_mul.map(|v| v as f32), CLIP);
    gain
}

/// 白平衡滴管 on a linear camera source: the multipliers that make the patch neutral.
pub fn neutralizing_mul(r: f32, g: f32, b: f32) -> Option<[f64; 3]> {
    if !(r > 1e-6 && g > 1e-6 && b > 1e-6) {
        return None;
    }
    Some(color::normalize_green([1.0 / r as f64, 1.0 / g as f64, 1.0 / b as f64]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam() -> CameraColorInfo {
        CameraColorInfo {
            pre_mul: [2.1792, 1.0, 1.2902],
            cam_mul: [1.9463, 1.0, 1.5488],
            rgb_cam: [1.7, -0.6, -0.1, -0.2, 1.5, -0.3, 0.05, -0.45, 1.4],
        }
    }

    #[test]
    fn oklab_round_trip_and_white() {
        let w = oklab_from_2020([1.0, 1.0, 1.0]);
        assert!((w[0] - 1.0).abs() < 1e-3 && w[1].abs() < 1e-3 && w[2].abs() < 1e-3, "{w:?}");
        for rgb in [[0.2f32, 0.5, 0.8], [0.9, 0.1, 0.05], [0.01, 0.02, 0.015], [0.5, 0.5, 0.5], [1.4, 0.7, 0.2]] {
            let back = oklab_to_2020(oklab_from_2020(rgb));
            for c in 0..3 {
                assert!((back[c] - rgb[c]).abs() < 1e-4 * rgb[c].max(1.0), "{rgb:?} -> {back:?}");
            }
        }
        // OkLCh: hue survives the polar round trip.
        let lab = oklab_from_2020([0.8, 0.3, 0.1]);
        let (c, h) = ((lab[1] * lab[1] + lab[2] * lab[2]).sqrt(), lab[2].atan2(lab[1]));
        let back = oklab_to_2020([lab[0], c * h.cos(), c * h.sin()]);
        assert!((back[0] - 0.8).abs() < 1e-4 && (back[2] - 0.1).abs() < 1e-4);
    }

    #[test]
    fn spaces_are_inverse_and_keep_white() {
        let s = spaces();
        let id = mat_mul(&s.srgb_to_2020, &s.rec2020_to_srgb);
        for r in 0..3 {
            for c in 0..3 {
                assert!((id[r * 3 + c] - if r == c { 1.0 } else { 0.0 }).abs() < 1e-9);
            }
            // D65 white stays white.
            assert!((s.srgb_to_2020[r * 3..r * 3 + 3].iter().sum::<f64>() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn band_weights_sum_to_one() {
        let hues = band_hues();
        for i in 0..720 {
            let h = -PI + i as f32 * TAU / 720.0;
            let w = band_weights(h, hues);
            let sum: f32 = w.iter().sum();
            assert!((sum - 1.0).abs() < 1e-5, "h={h} sum={sum}");
            assert!(w.iter().all(|&v| (0.0..=1.0 + 1e-6).contains(&v)));
        }
        // At a band centre that band has (almost) all the weight.
        for (i, &h) in hues.iter().enumerate() {
            assert!(band_weights(h, hues)[i] > 0.999, "band {i}");
        }
        // The centres go round the circle in order (red → magenta).
        let mut total = 0.0;
        for i in 0..8 {
            let d = wrap(hues[(i + 1) % 8] - hues[i]);
            assert!(d > 0.0, "band {i} → {}", i + 1);
            total += d;
        }
        assert!((total - TAU).abs() < 1e-3);
    }

    #[test]
    fn curve_is_monotone_and_hits_points() {
        let pts = [(0.0, 0.0), (0.25, 0.1), (0.5, 0.5), (0.6, 0.9), (1.0, 1.0)];
        let c = MonotoneCurve::new(&pts);
        for &(x, y) in &pts {
            assert!((c.eval(x) - y).abs() < 1e-12);
        }
        let mut prev = -1.0;
        for i in 0..=1000 {
            let v = c.eval(i as f64 / 1000.0);
            assert!(v >= prev - 1e-12, "not monotone at {i}");
            assert!((0.0..=1.0).contains(&v));
            prev = v;
        }
        // Flat runs stay flat (no overshoot between equal points).
        let flat = MonotoneCurve::new(&[(0.0, 0.2), (0.3, 0.5), (0.7, 0.5), (1.0, 0.9)]);
        for i in 300..=700 {
            assert!((flat.eval(i as f64 / 1000.0) - 0.5).abs() < 1e-12);
        }
        assert!(curve_is_identity(&[]));
        assert!(curve_is_identity(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)]));
        assert!(!curve_is_identity(&[(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)]));
    }

    #[test]
    fn luts_identity_and_shoulder() {
        let a = ImageAdjustments::default();
        let l = build_luts(&a);
        // Default sliders: identity below white, a hard clip above (as version 2).
        for &x in &[0.0f32, 0.1, 0.5, 0.9, 1.0, 1.5, 2.0] {
            let v = sample_lut(&l[..LUT_SIZE], x);
            assert!((v - x.min(1.0)).abs() < 2e-3, "{x} -> {v}");
        }
        let b = ImageAdjustments { highlight_recovery: 100.0, ..Default::default() };
        let l = build_luts(&b);
        let at = |x: f32| sample_lut(&l[..LUT_SIZE], x);
        assert!((at(0.5) - 0.5).abs() < 2e-3); // below the knee
        assert!(at(1.0) < 0.96 && at(1.0) > 0.9); // white rolls off
        assert!(at(1.5) > at(1.0) && at(2.0) <= 1.0); // and keeps separating
    }

    #[test]
    fn neutral_stays_neutral_and_matches_version_2_white_balance() {
        let c = cam();
        let adj = ImageAdjustments { temperature: 5000.0, tint: 3.0, ..Default::default() };
        // A camera-neutral patch under the chosen light renders grey.
        let target = color::kelvin_tint_to_cam_mul(&c, 5000.0, 3.0).unwrap();
        let ctx = ProcessContext { camera: Some(c.clone()), source_kind: SourceKind::LinearCamera { gain: 1.0 }, ..Default::default() };
        let p = V3Params::new(&adj, &ctx);
        let raw = target.map(|t| (0.2 / t) as f32);
        let mut px = [raw[0], raw[1], raw[2], 1.0];
        pixel(&p, &mut px);
        assert!((px[0] - px[1]).abs() < 2e-3 && (px[2] - px[1]).abs() < 2e-3, "{px:?}");

        // Version 2's maths on the same light: decode (rgb_cam · pre_mul) then the WB matrix.
        let m1 = color::white_balance_matrix(&c, 5000.0, 3.0, color::WhiteBalanceReference::Decode).unwrap();
        let raw = [0.11f64, 0.3, 0.17];
        let dec: Vec<f64> = (0..3).map(|r| (0..3).map(|k| c.rgb_cam[r * 3 + k] * c.pre_mul[k] * raw[k]).sum()).collect();
        let v2: Vec<f64> = (0..3).map(|r| (0..3).map(|k| m1[r * 3 + k] as f64 * dec[k]).sum()).collect();
        let (m3, _) = camera_to_srgb(&c, 5000.0, 3.0);
        let v3: Vec<f64> = (0..3).map(|r| (0..3).map(|k| m3[r * 3 + k] * raw[k]).sum()).collect();
        for i in 0..3 {
            assert!((v2[i] - v3[i]).abs() < 1e-5 * v2[i].abs().max(1.0), "{v2:?} vs {v3:?}");
        }
    }

    #[test]
    fn hsl_moves_only_its_band() {
        let mut adj = ImageAdjustments::default();
        adj.hsl_saturation[4] = -100.0; // 青: fully desaturated
        let ctx = ProcessContext::default();
        let p = V3Params::new(&adj, &ctx);
        let enc = |v: f32| oetf(v);
        // A cyan pixel loses its colour, a red one keeps it.
        let mut cyan = [enc(0.05), enc(0.4), enc(0.45), 1.0];
        pixel(&p, &mut cyan);
        assert!((cyan[0] - cyan[1]).abs() < 0.03 && (cyan[1] - cyan[2]).abs() < 0.03, "{cyan:?}");
        let mut red = [enc(0.6), enc(0.05), enc(0.04), 1.0];
        let mut red0 = red;
        pixel(&p, &mut red);
        pixel(&V3Params::new(&ImageAdjustments::default(), &ctx), &mut red0);
        assert!((red[0] - red0[0]).abs() < 1e-3 && (red[1] - red0[1]).abs() < 1e-3, "{red:?} vs {red0:?}");
    }

    /// A synthetic Display P3 colour through a version-3 render equals the formula:
    /// sRGB = P3_TO_SRGB · P3 (linear), with neutral sliders and white balance.
    #[test]
    fn display_p3_source_converts_by_the_formula() {
        let adj = ImageAdjustments::default(); // 5200 K / 0 → the black-body multipliers are 1
        assert_eq!(white_balance_multipliers(adj.temperature, adj.tint), (1.0, 1.0, 1.0));
        let p3 = ProcessContext { source_primaries: SourcePrimaries::DisplayP3, ..Default::default() };
        let srgb = ProcessContext::default();
        for lin in [[0.40f32, 0.30, 0.20], [0.10, 0.50, 0.30], [0.25, 0.25, 0.25]] {
            let mut px = [oetf(lin[0]), oetf(lin[1]), oetf(lin[2]), 1.0];
            pixel(&V3Params::new(&adj, &p3), &mut px);
            let m = to_f32(&P3_TO_SRGB);
            let want = mul3(&m, lin);
            for c in 0..3 {
                assert!((px[c] - oetf(want[c])).abs() < 2e-3, "{lin:?}: {} vs {}", px[c], oetf(want[c]));
            }
            // The same numbers read as sRGB give the plain values back.
            let mut q = [oetf(lin[0]), oetf(lin[1]), oetf(lin[2]), 1.0];
            pixel(&V3Params::new(&adj, &srgb), &mut q);
            assert!((q[0] - oetf(lin[0])).abs() < 2e-3);
        }
        // A P3 primary red is outside sRGB: version 3 keeps it the most saturated red sRGB
        // can show (gamut compression), more than reading the same numbers as sRGB red would.
        let mut red = [1.0f32, 0.0, 0.0, 1.0];
        pixel(&V3Params::new(&adj, &p3), &mut red);
        assert!(red[0] > 0.95 && red[1] < 0.2 && red[2] < 0.2, "{red:?}");
        // The older versions' conversion: P3 grey stays grey, P3 red clips to sRGB red.
        let mut img = FloatImage::new(2, 1);
        img.data.copy_from_slice(&[0.5, 0.5, 0.5, 1.0, 1.0, 0.0, 0.0, 1.0]);
        p3_to_srgb_encoded(&mut img);
        assert!((img.data[0] - 0.5).abs() < 2e-3 && (img.data[1] - 0.5).abs() < 2e-3);
        assert!(img.data[4] > 0.99 && img.data[5] == 0.0 && img.data[6] == 0.0);
    }

    #[test]
    fn out_of_gamut_is_compressed_not_clipped() {
        // A Rec.2020 green far outside sRGB keeps its lightness ordering and stays in range.
        let ctx = ProcessContext::default();
        let p = V3Params::new(&ImageAdjustments::default(), &ctx);
        let mut s = [0.0f32; 3];
        let lin = [0.05f32, 0.6, 0.05];
        let mut v = mul3(&p.out_m, lin);
        let yy = (SRGB_Y[0] * v[0] + SRGB_Y[1] * v[1] + SRGB_Y[2] * v[2]).clamp(0.0, 1.0);
        let mut tg = 1.0f32;
        for &c in &v {
            if c < 0.0 {
                tg = tg.min(yy / (yy - c).max(1e-9));
            } else if c > 1.0 {
                tg = tg.min((1.0 - yy) / (c - yy).max(1e-9));
            }
        }
        if tg < 1.0 {
            v = v.map(|c| yy + (c - yy) * tg);
        }
        s.copy_from_slice(&v);
        assert!(s.iter().all(|&c| (-1e-6..=1.0 + 1e-6).contains(&c)), "{s:?}");
        assert!(s[1] > s[0] && s[1] > s[2]);
    }

    /// Synthetic: a warm gradient whose red clips in the bright half.
    #[test]
    fn reconstruct_restores_a_clipped_channel() {
        let (w, h) = (64usize, 16usize);
        let mut truth = FloatImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = 0.3 + 1.2 * x as f32 / 63.0;
                let o = truth.index(x, y);
                truth.data[o..o + 4].copy_from_slice(&[v, v * 0.6, v * 0.35, 1.0]);
            }
        }
        let mut img = truth.clone();
        img.data.chunks_exact_mut(4).for_each(|p| p[..3].iter_mut().for_each(|v| *v = v.min(1.0)));
        let err = |a: &FloatImage| -> f32 { a.data.chunks_exact(4).zip(truth.data.chunks_exact(4)).map(|(p, t)| (p[0] - t[0]).abs()).sum() };
        let before = err(&img);
        let n = reconstruct(&mut img, [1.0, 1.0, 1.0], 0.999);
        assert!(n > 0);
        let after = err(&img);
        assert!(after < before * 0.2, "before {before} after {after}");
        // Unclipped pixels untouched; the clipped channel now above the clip.
        assert_eq!(img.data[0..4], truth.data[0..4]);
        let o = img.index(w - 1, 0);
        assert!(img.data[o] > 1.3, "{}", img.data[o]);
    }

    #[test]
    fn reconstruct_fully_clipped_and_nothing_clipped() {
        let mut img = FloatImage::new(4, 4);
        img.data.chunks_exact_mut(4).for_each(|p| p.copy_from_slice(&[1.0, 1.0, 1.0, 1.0]));
        assert_eq!(reconstruct(&mut img, [2.0, 1.0, 1.5], 0.99), 16);
        let p = &img.data[0..3];
        assert!((p[0] * 2.0 - p[1]).abs() < 1e-5 && (p[2] * 1.5 - p[1]).abs() < 1e-5);
        let mut img = FloatImage::new(4, 4);
        img.data.iter_mut().for_each(|v| *v = 0.5);
        assert_eq!(reconstruct(&mut img, [2.0, 1.0, 1.5], 0.99), 0);
    }

    #[test]
    fn highlight_strength_blends_clip_neutral_and_reconstruction() {
        // A reconstructed pixel above the clip: 高光復原 0 gives the neutral clip, 100 the
        // reconstruction (which, through the shoulder, stays below white but brighter).
        let c = cam();
        let ctx = ProcessContext { camera: Some(c), source_kind: SourceKind::LinearCamera { gain: 1.0 }, ..Default::default() };
        let mut a0 = [1.6f32, 1.0, 0.7, 1.0];
        let mut a1 = a0;
        pixel(&V3Params::new(&ImageAdjustments::default(), &ctx), &mut a0);
        pixel(&V3Params::new(&ImageAdjustments { highlight_recovery: 100.0, ..Default::default() }, &ctx), &mut a1);
        assert!(a0.iter().take(3).all(|&v| v <= 1.0) && a1.iter().take(3).all(|&v| v <= 1.0));
        assert!((a0[0] - a0[1]).abs() < 0.02 && (a0[1] - a0[2]).abs() < 0.02, "clip-neutral is grey: {a0:?}");
        assert!((a1[0] - a1[2]).abs() > 0.02, "reconstruction keeps colour: {a1:?}");
    }

    #[test]
    fn auto_bright_gain_scales_the_99th_percentile_to_white() {
        let c = CameraColorInfo { pre_mul: [1.0, 1.0, 1.0], cam_mul: [1.0, 1.0, 1.0], rgb_cam: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] };
        let mut img = FloatImage::new(100, 100);
        img.data.chunks_exact_mut(4).for_each(|p| p.copy_from_slice(&[0.25, 0.25, 0.25, 1.0]));
        let g = auto_bright_gain(&img, &c);
        assert!((g - 4.0).abs() < 0.01, "{g}");
        // Already reaching white: no gain.
        img.data.chunks_exact_mut(4).take(500).for_each(|p| p.copy_from_slice(&[1.0, 1.0, 1.0, 1.0]));
        assert!((auto_bright_gain(&img, &c) - 1.0).abs() < 0.01);
    }
}
