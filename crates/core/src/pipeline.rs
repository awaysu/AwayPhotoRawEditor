//! The non-destructive rendering pipeline. Port of `ImageProcessor.swift` and
//! `ImageProcessor+CPU.swift`, which mirror the C# reference implementation.
//!
//! Steps: 1 white balance, 2 exposure, 3 tone LUT, 4 vibrance/saturation, 5 noise
//! reduction, 6 sharpen/soften, 7 gradient, 8 heal, 9 distortion, 10 rotation/crop,
//! 10c vignette.
//!
//! The step order lives in exactly one place, `run_pipeline`, driven through
//! `StageTarget`: `CpuTarget` here (the reference) and the wgpu target in `awpr-gpu`.
//!
//! ⚠️ Floating-point order matters: every expression keeps the operand order and the
//! f32/f64 split of the Swift code, so `hashtest` stays comparable. Rust does not contract
//! `a * b + c` into FMA unless asked, which is the same default as Swift and C#.

use crate::buffer::{round_half_even, round_half_even_f32, FloatImage};
use crate::color::{self, WhiteBalanceReference};
use crate::model::{CameraColorInfo, ImageAdjustments, LinearGradient, Rotation};
use crate::tone;
use std::f64::consts::PI;

/// Per-render context.
#[derive(Debug, Clone, Default)]
pub struct ProcessContext {
    /// Skip distortion / rotation / crop (gradient and heal overlays).
    pub skip_geometry: bool,
    /// Apply distortion + 90° rotation but not the crop rectangle (crop overlay).
    pub skip_crop_rect: bool,
    /// Camera colour data for the white-balance matrix; None → black-body multipliers.
    pub camera: Option<CameraColorInfo>,
    pub white_balance_reference: WhiteBalanceReference,
}

/// Parameters for a geometry resample. f64 on purpose: the CPU geometry must not drift
/// (the GPU, which has no f64, narrows them — the accepted difference between the two).
#[derive(Debug, Clone, Copy)]
pub struct ResampleParams {
    /// 0 = radial distortion, 1 = rotate about (cx,cy)
    pub mode: i32,
    pub k: f64,
    pub cx: f64,
    pub cy: f64,
    pub sin_a: f64,
    pub cos_a: f64,
    pub ox: f64,
    pub oy: f64,
}

/// Box blur of `radius`, then blended back (mode 0) or used as unsharp base (mode 1).
#[derive(Debug, Clone, Copy)]
pub struct BlurOp {
    pub radius: i32,
    pub mode: i32,
    pub amount: f32,
}

/// What one fused pointwise pass does (the Swift `PixelStageParams` / `PixelFlags`).
#[derive(Debug, Clone, Default)]
pub struct PixelStageParams {
    pub legacy_wb: bool,
    pub linear_mul: bool,
    pub linear_matrix: bool,
    pub tone_lut: Option<Vec<f32>>,
    pub vib_sat: bool,
    pub gradients: Vec<LinearGradient>,
    pub gradient_linear: bool,
    pub vignette: bool,
    pub wb_mul: (f32, f32, f32),
    /// 3×3, exposure already folded in.
    pub m: Option<[f32; 9]>,
    pub sat: f32,
    pub vib: f32,
    pub vig_amount: f32,
    pub vig_cx: f32,
    pub vig_cy: f32,
    pub vig_inv_max: f32,
}

pub type StageError = Box<dyn std::error::Error + Send + Sync>;

/// Where the pixels live while the pipeline runs. Each call mutates the target in place
/// (geometry may change its size). The CPU target never fails; a GPU target may, and the
/// caller then falls back to the CPU from the untouched source.
pub trait StageTarget {
    /// What the finished render is: CPU pixels, or (GPU) a frame still on the device so
    /// the editor can draw it without a readback.
    type Output;
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn pixel(&mut self, p: &PixelStageParams) -> Result<(), StageError>;
    fn blur(&mut self, op: BlurOp) -> Result<(), StageError>;
    fn heal(&mut self, adj: &ImageAdjustments) -> Result<(), StageError>;
    fn resample(&mut self, p: ResampleParams, out_w: usize, out_h: usize) -> Result<(), StageError>;
    fn rotate(&mut self, rot: Rotation) -> Result<(), StageError>;
    fn result(self) -> Result<Self::Output, StageError>;
}

/// Run steps 1-10 on the CPU, producing a new buffer (geometry may change the size).
pub fn apply_to_float(src: &FloatImage, adj: &ImageAdjustments, ctx: &ProcessContext) -> FloatImage {
    run_pipeline(CpuTarget::new(src), adj, ctx).expect("the CPU target cannot fail")
}

/// The pipeline, written once.
pub fn run_pipeline<T: StageTarget>(mut t: T, adj: &ImageAdjustments, ctx: &ProcessContext) -> Result<T::Output, StageError> {
    let blur_stages = adj.noise_reduction > 0.0 || adj.sharpening != 0.0;
    let grad_active = adj.has_active_gradient();

    // 1-4 (with no blur stage in between, 7 gradient folds into the same pass)
    t.pixel(&build_color_params(adj, ctx, grad_active && !blur_stages))?;

    if blur_stages {
        if adj.noise_reduction > 0.0 {
            t.blur(noise_reduction_op(adj))?; // 5
        }
        if adj.sharpening != 0.0 {
            t.blur(sharpen_op(adj))?; // 6
        }
        if grad_active {
            t.pixel(&build_gradient_params(adj))?; // 7
        }
    }

    if !adj.heal_spots.is_empty() {
        t.heal(adj)?; // 8
    }

    // 10c vignette — post-crop, so it always hugs the frame actually shown/exported.
    let finish = |mut t: T| -> Result<T::Output, StageError> {
        if adj.vignette != 0.0 {
            let p = build_vignette_params(adj, t.width(), t.height());
            t.pixel(&p)?;
        }
        t.result()
    };

    if ctx.skip_geometry {
        return finish(t);
    }

    if adj.distortion != 0.0 {
        // 9
        let p = ResampleParams {
            mode: 0,
            k: adj.distortion / 100.0 * 0.35,
            cx: 0.0,
            cy: 0.0,
            sin_a: 0.0,
            cos_a: 0.0,
            ox: 0.0,
            oy: 0.0,
        };
        let (w, h) = (t.width(), t.height());
        t.resample(p, w, h)?;
    }

    // 10a: discrete 90°, even under the crop overlay
    if adj.rotation != Rotation::R0 {
        t.rotate(adj.rotation)?;
    }

    if ctx.skip_crop_rect {
        if adj.crop_angle != 0.0 {
            let (w, h) = (t.width(), t.height());
            let cx = (adj.crop_x + adj.crop_width / 2.0) * w as f64;
            let cy = (adj.crop_y + adj.crop_height / 2.0) * h as f64;
            let a = adj.crop_angle * PI / 180.0;
            let p = ResampleParams { mode: 1, k: 0.0, cx, cy, sin_a: a.sin(), cos_a: a.cos(), ox: cx, oy: cy };
            t.resample(p, w, h)?;
        }
        return finish(t);
    }

    // 10b: crop rectangle (with straighten angle); a full-frame crop is a no-op.
    let full_crop = adj.crop_x <= 0.0
        && adj.crop_y <= 0.0
        && adj.crop_width >= 1.0
        && adj.crop_height >= 1.0
        && adj.crop_angle == 0.0;
    if !full_crop {
        let (w, h) = (t.width(), t.height());
        let out_w = (round_half_even(adj.crop_width.max(0.02).min(1.0) * w as f64) as i64).max(1) as usize;
        let out_h = (round_half_even(adj.crop_height.max(0.02).min(1.0) * h as f64) as i64).max(1) as usize;
        let cx = (adj.crop_x + adj.crop_width / 2.0) * w as f64;
        let cy = (adj.crop_y + adj.crop_height / 2.0) * h as f64;
        let a = adj.crop_angle * PI / 180.0;
        let p = ResampleParams {
            mode: 1,
            k: 0.0,
            cx,
            cy,
            sin_a: a.sin(),
            cos_a: a.cos(),
            ox: out_w as f64 / 2.0,
            oy: out_h as f64 / 2.0,
        };
        t.resample(p, out_w, out_h)?;
    }
    finish(t)
}

// ---- the CPU target (reference implementation) -------------------------------

/// The image lives in CPU memory and every stage mutates it in place. Until the first
/// stage runs it is only a borrow of the source: the first pixel pass does the copy
/// itself, so the source is read once instead of cloned and then read again.
pub struct CpuTarget<'a> {
    src: &'a FloatImage,
    buf: Option<FloatImage>,
}

impl<'a> CpuTarget<'a> {
    pub fn new(src: &'a FloatImage) -> Self {
        Self { src, buf: None }
    }

    fn buf(&mut self) -> &mut FloatImage {
        let src = self.src;
        self.buf.get_or_insert_with(|| src.clone())
    }
}

impl StageTarget for CpuTarget<'_> {
    type Output = FloatImage;

    fn width(&self) -> usize {
        self.buf.as_ref().map_or(self.src.width, |b| b.width)
    }

    fn height(&self) -> usize {
        self.buf.as_ref().map_or(self.src.height, |b| b.height)
    }

    fn pixel(&mut self, p: &PixelStageParams) -> Result<(), StageError> {
        match &mut self.buf {
            Some(buf) => pixel(buf, p),
            None => {
                let src = self.src;
                let mut buf = FloatImage::new(src.width, src.height);
                let stride = src.width * 4;
                let k = PixelKernel::new(p, src.width, src.height);
                buf.par_rows_mut(|y, row| {
                    row.copy_from_slice(&src.data[y * stride..(y + 1) * stride]);
                    k.row(y, row);
                });
                self.buf = Some(buf);
            }
        }
        Ok(())
    }

    fn blur(&mut self, op: BlurOp) -> Result<(), StageError> {
        apply_blur_op(self.buf(), op);
        Ok(())
    }

    fn heal(&mut self, adj: &ImageAdjustments) -> Result<(), StageError> {
        heal(self.buf(), adj);
        Ok(())
    }

    fn resample(&mut self, p: ResampleParams, out_w: usize, out_h: usize) -> Result<(), StageError> {
        let out = resample(self.buf(), p, out_w, out_h);
        self.buf = Some(out);
        Ok(())
    }

    fn rotate(&mut self, rot: Rotation) -> Result<(), StageError> {
        if rot != Rotation::R0 {
            let out = rotate_discrete(self.buf(), rot);
            self.buf = Some(out);
        }
        Ok(())
    }

    fn result(mut self) -> Result<FloatImage, StageError> {
        self.buf();
        Ok(self.buf.take().unwrap())
    }
}

// ---- parameter builders ------------------------------------------------------

fn build_color_params(adj: &ImageAdjustments, ctx: &ProcessContext, with_gradients: bool) -> PixelStageParams {
    let mut p = PixelStageParams { tone_lut: Some(tone::build_lut(adj)), ..Default::default() };
    let exp = 2.0f64.powf(adj.exposure) as f32;

    if adj.is_legacy_pipeline() {
        let (mr, mg, mb) = white_balance_multipliers(adj.temperature, adj.tint);
        p.legacy_wb = true;
        p.wb_mul = (mr as f32 * exp, mg as f32 * exp, mb as f32 * exp);
    } else {
        let matrix = ctx.camera.as_ref().filter(|c| c.is_valid()).and_then(|cam| {
            color::white_balance_matrix(cam, adj.temperature, adj.tint, ctx.white_balance_reference)
        });
        if let Some(m) = matrix {
            p.linear_matrix = true;
            p.m = Some(m.map(|v| v * exp));
        } else {
            let (mr, mg, mb) = white_balance_multipliers(adj.temperature, adj.tint);
            p.linear_mul = true;
            p.wb_mul = (mr as f32 * exp, mg as f32 * exp, mb as f32 * exp);
        }
    }

    let sat = (adj.saturation / 100.0) as f32;
    let vib = (adj.vibrance / 100.0) as f32;
    if sat != 0.0 || vib != 0.0 {
        p.vib_sat = true;
        p.sat = sat;
        p.vib = vib;
    }
    if with_gradients {
        add_gradients(&mut p, adj);
    }
    p
}

fn build_gradient_params(adj: &ImageAdjustments) -> PixelStageParams {
    let mut p = PixelStageParams::default();
    add_gradients(&mut p, adj);
    p
}

fn add_gradients(p: &mut PixelStageParams, adj: &ImageAdjustments) {
    let list: Vec<LinearGradient> = adj.gradients.iter().filter(|g| g.has_effect()).cloned().collect();
    if list.is_empty() {
        return;
    }
    p.gradient_linear = !adj.is_legacy_pipeline();
    p.gradients = list;
}

fn build_vignette_params(adj: &ImageAdjustments, width: usize, height: usize) -> PixelStageParams {
    let cx = (width - 1) as f32 * 0.5;
    let cy = (height - 1) as f32 * 0.5;
    PixelStageParams {
        vignette: true,
        vig_amount: (-adj.vignette / 100.0) as f32,
        vig_cx: cx,
        vig_cy: cy,
        vig_inv_max: 1.0 / (cx * cx + cy * cy).sqrt(),
        ..Default::default()
    }
}

fn noise_reduction_op(adj: &ImageAdjustments) -> BlurOp {
    let strength = (adj.noise_reduction / 100.0) as f32;
    let radius = 1 + round_half_even_f32(strength * 2.0) as i32;
    BlurOp { radius, mode: 0, amount: (strength * 0.8).max(0.0).min(1.0) }
}

fn sharpen_op(adj: &ImageAdjustments) -> BlurOp {
    let amt = (adj.sharpening / 100.0) as f32;
    if amt > 0.0 {
        return BlurOp { radius: 1, mode: 1, amount: amt * 1.5 };
    }
    let radius = 1 + round_half_even_f32(-amt * 2.0) as i32;
    BlurOp { radius, mode: 0, amount: (-amt).max(0.0).min(1.0) }
}

// ---- 1 + 2: white balance & exposure (black-body fallback) -------------------

/// RGB multipliers for a temperature (K) and tint, neutral at 5200 K / 0 tint.
pub fn white_balance_multipliers(temperature: f64, tint: f64) -> (f64, f64, f64) {
    let (r, g, b) = kelvin_to_rgb(temperature);
    let (r0, g0, b0) = kelvin_to_rgb(5200.0);
    let mut mr = r0 / r.max(1e-3);
    let mut mg = g0 / g.max(1e-3);
    let mut mb = b0 / b.max(1e-3);
    mr /= mg;
    mb /= mg;
    mg = 1.0;
    mg *= 1.0 - (tint / 100.0).max(-1.0).min(1.0) * 0.30;
    (mr, mg, mb)
}

fn kelvin_to_rgb(kelvin: f64) -> (f64, f64, f64) {
    let t = kelvin.max(1000.0).min(40000.0) / 100.0;
    let r = if t <= 66.0 { 255.0 } else { 329.698727446 * (t - 60.0).powf(-0.1332047592) };
    let g = if t <= 66.0 {
        99.4708025861 * t.ln() - 161.1195681661
    } else {
        288.1221695283 * (t - 60.0).powf(-0.0755148492)
    };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.5177312231 * (t - 10.0).ln() - 305.0447927307
    };
    (r.max(0.0).min(255.0) / 255.0, g.max(0.0).min(255.0) / 255.0, b.max(0.0).min(255.0) / 255.0)
}

// ---- fused pixel pass --------------------------------------------------------
//
// The Swift/C# code runs each pointwise stage over the whole buffer in turn. Every stage
// here is pointwise (it reads only the pixel itself and its x/y), so running them
// back-to-back per pixel performs exactly the same operations in the same order — the
// output is bit-identical — while touching memory once instead of once per stage.

#[inline(always)]
fn clamp0(v: f32) -> f32 {
    if v < 0.0 {
        0.0
    } else {
        v
    }
}

/// Per-gradient constants hoisted out of the pixel loop (as the Swift code does).
struct GradientK {
    sin_a: f64,
    cos_a: f64,
    center_x: f64,
    center_y: f64,
    range: f64,
    g_exp: f32,
    g_con: f32,
    g_hi: f32,
    g_sh: f32,
    g_sat: f32,
}

impl GradientK {
    fn new(gr: &LinearGradient) -> Self {
        let a = gr.angle * PI / 180.0;
        Self {
            sin_a: a.sin(),
            cos_a: a.cos(),
            center_x: gr.center_x,
            center_y: gr.center_y,
            range: gr.range.max(1e-3),
            g_exp: gr.exposure as f32,
            g_con: (gr.contrast / 100.0) as f32,
            g_hi: (gr.highlights / 100.0) as f32,
            g_sh: (gr.shadows / 100.0) as f32,
            g_sat: (gr.saturation / 100.0) as f32,
        }
    }
}

struct PixelKernel<'a> {
    p: &'a PixelStageParams,
    grads: Vec<GradientK>,
    w: usize,
    h: usize,
    dl: &'static [f32],
    el: &'static [f32],
}

impl<'a> PixelKernel<'a> {
    fn new(p: &'a PixelStageParams, w: usize, h: usize) -> Self {
        Self {
            p,
            grads: p.gradients.iter().filter(|g| g.has_effect()).map(GradientK::new).collect(),
            w,
            h,
            dl: color::decode_lut(),
            el: color::encode_lut(),
        }
    }

    #[inline(always)]
    fn row(&self, y: usize, row: &mut [f32]) {
        let p = self.p;
        let (dl, el) = (self.dl, self.el);
        let lut = p.tone_lut.as_deref();
        let ny = y as f64 / self.h as f64;
        let vig_dy = (y as f32 - p.vig_cy) * p.vig_inv_max;
        for (x, px) in row.chunks_exact_mut(4).enumerate() {
            // 1 + 2: white balance & exposure
            if p.legacy_wb {
                let (fr, fg, fb) = p.wb_mul;
                px[0] *= fr;
                px[1] *= fg;
                px[2] *= fb;
            } else if let Some(m) = &p.m {
                // v1: decode → 3×3 (exposure folded in) → re-encode
                let r = color::linearize(dl, px[0]);
                let g = color::linearize(dl, px[1]);
                let b = color::linearize(dl, px[2]);
                px[0] = color::encode(el, m[0] * r + m[1] * g + m[2] * b);
                px[1] = color::encode(el, m[3] * r + m[4] * g + m[5] * b);
                px[2] = color::encode(el, m[6] * r + m[7] * g + m[8] * b);
            } else if p.linear_mul {
                let (fr, fg, fb) = p.wb_mul;
                px[0] = color::encode(el, color::linearize(dl, px[0]) * fr);
                px[1] = color::encode(el, color::linearize(dl, px[1]) * fg);
                px[2] = color::encode(el, color::linearize(dl, px[2]) * fb);
            }

            // 3: tone LUT
            if let Some(lut) = lut {
                px[0] = tone::sample(lut, px[0]);
                px[1] = tone::sample(lut, px[1]);
                px[2] = tone::sample(lut, px[2]);
            }

            // 4: vibrance / saturation
            if p.vib_sat {
                vibrance_saturation_px(px, p.sat, p.vib);
            }

            // 7: graduated filters, stacked in order
            if !self.grads.is_empty() {
                let nx = x as f64 / self.w as f64;
                for g in &self.grads {
                    gradient_px(px, g, nx, ny, p.gradient_linear, dl, el);
                }
            }

            // 10c: vignette
            if p.vignette {
                let dx = (x as f32 - p.vig_cx) * p.vig_inv_max;
                let r = (dx * dx + vig_dy * vig_dy).sqrt(); // 0 centre .. 1 corner
                let mut m = ((r - 0.35) / 0.65).max(0.0).min(1.0); // start ~1/3 out
                m = m * m * (3.0 - 2.0 * m);
                if m > 0.0 {
                    let g = (1.0 + p.vig_amount * m).max(0.0);
                    px[0] *= g;
                    px[1] *= g;
                    px[2] *= g;
                }
            }
        }
    }
}

/// A later pointwise pass (gradient after blur, vignette after crop), in place.
fn pixel(buf: &mut FloatImage, p: &PixelStageParams) {
    let k = PixelKernel::new(p, buf.width, buf.height);
    buf.par_rows_mut(|y, row| k.row(y, row));
}

#[inline(always)]
fn vibrance_saturation_px(px: &mut [f32], sat: f32, vib: f32) {
    let (r, g, b) = (px[0], px[1], px[2]);
    let luma = 0.299f32 * r + 0.587f32 * g + 0.114f32 * b;
    let mx = r.max(g.max(b));
    let mn = r.min(g.min(b));
    let cur_sat = if mx <= 1e-4 { 0.0 } else { (mx - mn) / mx };
    let f = (1.0 + vib * (1.0 - cur_sat)) * (1.0 + sat);
    px[0] = clamp0(luma + (r - luma) * f);
    px[1] = clamp0(luma + (g - luma) * f);
    px[2] = clamp0(luma + (b - luma) * f);
}

/// `linear` (v1): the gradient's exposure multiplies light like the global exposure;
/// contrast / highlights / shadows / saturation stay in the encoded domain.
#[inline(always)]
fn gradient_px(px: &mut [f32], k: &GradientK, nx: f64, ny: f64, linear: bool, dl: &[f32], el: &[f32]) {
    let dist = (nx - k.center_x) * k.sin_a + (ny - k.center_y) * k.cos_a;
    let mut m = (dist / (2.0 * k.range) + 0.5).max(0.0).min(1.0) as f32;
    m = m * m * (3.0 - 2.0 * m); // smoothstep
    if m <= 0.0 {
        return;
    }

    let (mut r, mut g, mut b);
    if k.g_exp == 0.0 {
        r = px[0];
        g = px[1];
        b = px[2];
    } else {
        let exp_mul = 2.0f64.powf((k.g_exp * m) as f64) as f32;
        if linear {
            r = color::encode(el, color::linearize(dl, px[0]) * exp_mul);
            g = color::encode(el, color::linearize(dl, px[1]) * exp_mul);
            b = color::encode(el, color::linearize(dl, px[2]) * exp_mul);
        } else {
            r = px[0] * exp_mul;
            g = px[1] * exp_mul;
            b = px[2] * exp_mul;
        }
    }

    let luma = 0.299f32 * r + 0.587f32 * g + 0.114f32 * b;
    if k.g_sat != 0.0 {
        let f = 1.0 + k.g_sat * m;
        r = luma + (r - luma) * f;
        g = luma + (g - luma) * f;
        b = luma + (b - luma) * f;
    }
    if k.g_con != 0.0 {
        let c = k.g_con * m;
        r = 0.5 + (r - 0.5) * (1.0 + c);
        g = 0.5 + (g - 0.5) * (1.0 + c);
        b = 0.5 + (b - 0.5) * (1.0 + c);
    }
    if k.g_hi != 0.0 {
        let w_h = luma * luma * k.g_hi * 0.5 * m;
        r += w_h;
        g += w_h;
        b += w_h;
    }
    if k.g_sh != 0.0 {
        let w_s = (1.0 - luma) * (1.0 - luma) * k.g_sh * 0.5 * m;
        r += w_s;
        g += w_s;
        b += w_s;
    }

    px[0] = clamp0(r);
    px[1] = clamp0(g);
    px[2] = clamp0(b);
}

// ---- 5 / 6: noise reduction and sharpen / soften -----------------------------

fn apply_blur_op(buf: &mut FloatImage, op: BlurOp) {
    let blurred = box_blur(buf, op.radius);
    if op.mode == 1 {
        let k = op.amount;
        let bd = &blurred.data;
        let stride = buf.width * 4;
        buf.par_rows_mut(|y, row| {
            let b = &bd[y * stride..(y + 1) * stride];
            for (d, s) in row.chunks_exact_mut(4).zip(b.chunks_exact(4)) {
                d[0] = clamp0(d[0] + k * (d[0] - s[0]));
                d[1] = clamp0(d[1] + k * (d[1] - s[1]));
                d[2] = clamp0(d[2] + k * (d[2] - s[2]));
            }
        });
    } else {
        blend(buf, &blurred, op.amount);
    }
}

fn box_blur(src: &FloatImage, radius: i32) -> FloatImage {
    use rayon::prelude::*;
    let (w, h) = (src.width, src.height);
    let r = radius as usize;
    let mut tmp = FloatImage::new(w, h);
    let mut dst = FloatImage::new(w, h);
    let norm = 1.0 / (radius * 2 + 1) as f32;
    let stride = w * 4;
    let sd = &src.data;

    // horizontal. Each output sums taps k = -r..=r in order, starting from 0: the same
    // additions in the same order as the reference; only the edge clamp is hoisted out
    // of the interior.
    tmp.par_rows_mut(|y, trow| {
        let srow = &sd[y * stride..(y + 1) * stride];
        for x in 0..w {
            let (mut sr, mut sg, mut sb) = (0.0f32, 0.0f32, 0.0f32);
            if x >= r && x + r < w {
                for xx in x - r..=x + r {
                    let i = xx * 4;
                    sr += srow[i];
                    sg += srow[i + 1];
                    sb += srow[i + 2];
                }
            } else {
                for k in -radius..=radius {
                    let xx = (x as i64 + k as i64).clamp(0, w as i64 - 1) as usize;
                    let i = xx * 4;
                    sr += srow[i];
                    sg += srow[i + 1];
                    sb += srow[i + 2];
                }
            }
            let o = x * 4;
            trow[o] = sr * norm;
            trow[o + 1] = sg * norm;
            trow[o + 2] = sb * norm;
            trow[o + 3] = srow[o + 3];
        }
    });

    // vertical: accumulate whole source rows into a per-row accumulator, k in order, so
    // each pixel still sees 0 + v(-r) + ... + v(r).
    let td = &tmp.data;
    dst.data.par_chunks_mut(stride).enumerate().for_each_init(
        || vec![0.0f32; w * 3],
        |acc, (y, drow)| {
            acc.fill(0.0);
            for k in -radius..=radius {
                let yy = (y as i64 + k as i64).clamp(0, h as i64 - 1) as usize;
                let srow = &td[yy * stride..(yy + 1) * stride];
                for (a, s) in acc.chunks_exact_mut(3).zip(srow.chunks_exact(4)) {
                    a[0] += s[0];
                    a[1] += s[1];
                    a[2] += s[2];
                }
            }
            let trow = &td[y * stride..(y + 1) * stride];
            for ((d, a), t) in drow.chunks_exact_mut(4).zip(acc.chunks_exact(3)).zip(trow.chunks_exact(4)) {
                d[0] = a[0] * norm;
                d[1] = a[1] * norm;
                d[2] = a[2] * norm;
                d[3] = t[3];
            }
        },
    );
    dst
}

fn blend(dst: &mut FloatImage, other: &FloatImage, amount: f32) {
    let amount = amount.max(0.0).min(1.0);
    let od = &other.data;
    let stride = dst.width * 4;
    dst.par_rows_mut(|y, row| {
        let b = &od[y * stride..(y + 1) * stride];
        for (a, s) in row.chunks_exact_mut(4).zip(b.chunks_exact(4)) {
            a[0] += (s[0] - a[0]) * amount;
            a[1] += (s[1] - a[1]) * amount;
            a[2] += (s[2] - a[2]) * amount;
        }
    });
}

// ---- 8: heal / clone ---------------------------------------------------------

#[inline(always)]
fn smooth(x: f64) -> f64 {
    let x = x.max(0.0).min(1.0);
    x * x * (3.0 - 2.0 * x)
}

#[inline(always)]
fn in_bounds(x: i64, y: i64, w: i64, h: i64) -> bool {
    x >= 0 && y >= 0 && x < w && y < h
}

/// Step 8 on CPU memory. Public because the GPU target downloads, heals here, and
/// uploads again (a few small discs are cheaper than a kernel).
pub fn heal(buf: &mut FloatImage, adj: &ImageAdjustments) {
    let (w, h) = (buf.width, buf.height);
    let max_dim = w.max(h) as f64;
    let src = buf.clone(); // read from a snapshot so spots don't feed each other
    for spot in &adj.heal_spots {
        let radius = (round_half_even(spot.radius_norm * max_dim) as i64).max(1);
        let tx = round_half_even(spot.target_x * w as f64) as i64;
        let ty = round_half_even(spot.target_y * h as f64) as i64;
        if spot.use_inpaint {
            inpaint_spot(buf, &src, tx, ty, radius);
        } else {
            let sx = round_half_even(spot.source_x * w as f64) as i64;
            let sy = round_half_even(spot.source_y * h as f64) as i64;
            clone_spot(buf, &src, tx, ty, sx, sy, radius);
        }
    }
}

fn clone_spot(dst: &mut FloatImage, src: &FloatImage, tx: i64, ty: i64, sx: i64, sy: i64, radius: i64) {
    let (w, h) = (dst.width as i64, dst.height as i64);
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let dist = ((dx * dx + dy * dy) as f64).sqrt();
            if dist > radius as f64 {
                continue;
            }
            let (px, py, qx, qy) = (tx + dx, ty + dy, sx + dx, sy + dy);
            if !in_bounds(px, py, w, h) || !in_bounds(qx, qy, w, h) {
                continue;
            }
            let a = (1.0 - smooth(dist / radius as f64)) as f32;
            let di = ((py * w + px) * 4) as usize;
            let si = ((qy * w + qx) * 4) as usize;
            for c in 0..3 {
                dst.data[di + c] = dst.data[di + c] * (1.0 - a) + src.data[si + c] * a;
            }
        }
    }
}

fn inpaint_spot(dst: &mut FloatImage, src: &FloatImage, tx: i64, ty: i64, radius: i64) {
    let (w, h) = (dst.width as i64, dst.height as i64);
    // Average colour of the surrounding ring.
    let (mut sr, mut sg, mut sb) = (0.0f64, 0.0f64, 0.0f64);
    let mut n = 0;
    let ring = radius + (radius / 2).max(2);
    let mut ang = 0.0f64;
    while ang < PI * 2.0 {
        let qx = tx + (ang.cos() * ring as f64) as i64;
        let qy = ty + (ang.sin() * ring as f64) as i64;
        ang += 0.3;
        if !in_bounds(qx, qy, w, h) {
            continue;
        }
        let si = ((qy * w + qx) * 4) as usize;
        sr += src.data[si] as f64;
        sg += src.data[si + 1] as f64;
        sb += src.data[si + 2] as f64;
        n += 1;
    }
    if n == 0 {
        return;
    }
    let fr = (sr / n as f64) as f32;
    let fg = (sg / n as f64) as f32;
    let fb = (sb / n as f64) as f32;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let dist = ((dx * dx + dy * dy) as f64).sqrt();
            if dist > radius as f64 {
                continue;
            }
            let (px, py) = (tx + dx, ty + dy);
            if !in_bounds(px, py, w, h) {
                continue;
            }
            let a = (1.0 - smooth(dist / radius as f64)) as f32;
            let di = ((py * w + px) * 4) as usize;
            dst.data[di] = dst.data[di] * (1.0 - a) + fr * a;
            dst.data[di + 1] = dst.data[di + 1] * (1.0 - a) + fg * a;
            dst.data[di + 2] = dst.data[di + 2] * (1.0 - a) + fb * a;
        }
    }
}

// ---- 9 / 10: geometry --------------------------------------------------------

/// Inverse-map + bilinear resample. Mode 0: radial distortion. Mode 1: rotate about
/// (cx,cy) with the output origin at (ox,oy).
fn resample(src: &FloatImage, p: ResampleParams, out_w: usize, out_h: usize) -> FloatImage {
    let (w, h) = (src.width, src.height);
    let mut dst = FloatImage::new(out_w, out_h);

    if p.mode == 0 {
        let k = p.k;
        dst.par_rows_mut(|y, row| {
            let ny = (y as f64 / h as f64 - 0.5) * 2.0;
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let nx = (x as f64 / w as f64 - 0.5) * 2.0;
                let r2 = nx * nx + ny * ny;
                let f = 1.0 + k * r2;
                let (sx_n, sy_n) = (nx * f, ny * f);
                let sx = (sx_n / 2.0 + 0.5) * w as f64;
                let sy = (sy_n / 2.0 + 0.5) * h as f64;
                px.copy_from_slice(&sample_bilinear(src, sx, sy));
            }
        });
    } else {
        let ResampleParams { cx, cy, sin_a, cos_a, ox, oy, .. } = p;
        dst.par_rows_mut(|y, row| {
            let ry = y as f64 - oy;
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let rx = x as f64 - ox;
                let sx = cx + (rx * cos_a - ry * sin_a);
                let sy = cy + (rx * sin_a + ry * cos_a);
                px.copy_from_slice(&sample_bilinear(src, sx, sy));
            }
        });
    }
    dst
}

pub fn rotate_discrete(src: &FloatImage, rot: Rotation) -> FloatImage {
    let (w, h) = (src.width, src.height);
    let swap = matches!(rot, Rotation::R90 | Rotation::R270);
    let (dw, dh) = if swap { (h, w) } else { (w, h) };
    let mut dst = FloatImage::new(dw, dh);
    let s = &src.data;
    // Gather form of the Swift scatter: for each destination pixel find its source.
    dst.par_rows_mut(|ny, row| {
        for nx in 0..dw {
            let (x, y) = match rot {
                Rotation::R90 => (ny, h - 1 - nx),
                Rotation::R180 => (w - 1 - nx, h - 1 - ny),
                Rotation::R270 => (w - 1 - ny, nx),
                Rotation::R0 => (nx, ny),
            };
            let si = (y * w + x) * 4;
            row[nx * 4..nx * 4 + 4].copy_from_slice(&s[si..si + 4]);
        }
    });
    dst
}

#[inline(always)]
fn sample_bilinear(buf: &FloatImage, fx0: f64, fy0: f64) -> [f32; 4] {
    let (w, h) = (buf.width, buf.height);
    let mut fx = fx0;
    let mut fy = fy0;
    if fx < 0.0 {
        fx = 0.0;
    } else if fx > (w - 1) as f64 {
        fx = (w - 1) as f64;
    }
    if fy < 0.0 {
        fy = 0.0;
    } else if fy > (h - 1) as f64 {
        fy = (h - 1) as f64;
    }
    let x0 = fx as usize;
    let y0 = fy as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let tx = (fx - x0 as f64) as f32;
    let ty = (fy - y0 as f64) as f32;
    let i00 = (y0 * w + x0) * 4;
    let i10 = (y0 * w + x1) * 4;
    let i01 = (y1 * w + x0) * 4;
    let i11 = (y1 * w + x1) * 4;
    let d = &buf.data;
    let mut out = [0.0f32; 4];
    for c in 0..4 {
        out[c] = lerp2(d[i00 + c], d[i10 + c], d[i01 + c], d[i11 + c], tx, ty);
    }
    out
}

#[inline(always)]
fn lerp2(v00: f32, v10: f32, v01: f32, v11: f32, tx: f32, ty: f32) -> f32 {
    let top = v00 + (v10 - v00) * tx;
    let bot = v01 + (v11 - v01) * tx;
    top + (bot - top) * ty
}
