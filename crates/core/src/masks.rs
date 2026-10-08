//! 遮罩 (處理版本 3): radial and brush local adjustments.
//!
//! A mask is first turned into a weight per pixel (0..1) at the size of the image being
//! rendered — the proxy in the editor, the full decode on export — then the linear
//! gradient's adjustment set is applied through that weight, with the gradient's
//! formulas (exposure in linear light, the rest on encoded values). The weights are
//! computed on the CPU for both renderers and cached per mask shape and size (moving an
//! adjustment slider does not rebuild them), so the GPU only applies them: CPU and GPU
//! cannot disagree about where a mask is.
//!
//! Coordinates are normalized image coordinates before crop and rotation, like the
//! gradients': the pass runs before the geometry stages, so a mask stays on the same
//! scene content whatever the crop or the 90° rotation.

use crate::buffer::FloatImage;
use crate::color;
use crate::model::{BrushStroke, ImageAdjustments, LocalMask, MaskKind};
use rayon::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

#[inline]
fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 1 inside, 0 outside, a smooth fall-off across the outer `feather` % of the radius.
/// `d` is the distance in units of the radius.
#[inline]
pub fn falloff(d: f64, feather: f64) -> f64 {
    let f = (feather / 100.0).clamp(0.0, 1.0);
    let inner = 1.0 - f;
    if d >= 1.0 {
        0.0
    } else if d <= inner {
        1.0
    } else {
        1.0 - smoothstep((d - inner) / f.max(1e-9))
    }
}

/// The radial mask's ellipse distance (1 = on the edge) at normalized (nx, ny) of a
/// `w × h` image.
#[inline]
pub fn radial_distance(m: &LocalMask, nx: f64, ny: f64, w: usize, h: usize) -> f64 {
    let long = w.max(h) as f64;
    let u = (nx - m.center_x) * w as f64 / long;
    let v = (ny - m.center_y) * h as f64 / long;
    let (s, c) = m.angle.to_radians().sin_cos();
    let ur = u * c + v * s;
    let vr = -u * s + v * c;
    ((ur / m.radius_x.max(1e-6)).powi(2) + (vr / m.radius_y.max(1e-6)).powi(2)).sqrt()
}

/// Brush stamps are laid every this many radii along a stroke, so coverage does not
/// depend on how densely the pointer was sampled.
const STAMP_SPACING: f64 = 0.25;

/// A stroke's stamp centres in pixels.
fn stamps(s: &BrushStroke, w: usize, h: usize, r: f64) -> Vec<(f64, f64)> {
    let px = |p: &(f64, f64)| (p.0 * w as f64, p.1 * h as f64);
    let mut prev = px(&s.points[0]);
    let mut pts = vec![prev];
    for p in &s.points[1..] {
        let q = px(p);
        let d = ((q.0 - prev.0).powi(2) + (q.1 - prev.1).powi(2)).sqrt();
        let n = (d / (r * STAMP_SPACING)).ceil().max(1.0) as usize;
        for k in 1..=n {
            let t = k as f64 / n as f64;
            pts.push((prev.0 + (q.0 - prev.0) * t, prev.1 + (q.1 - prev.1) * t));
        }
        prev = q;
    }
    pts
}

/// Paint the strokes into `out` (which starts at 0). A stroke's coverage is the max of its
/// stamps — one pass of the brush; strokes then build up with their flow (or erase).
fn paint_brush(m: &LocalMask, w: usize, h: usize, out: &mut [f32]) {
    let long = w.max(h) as f64;
    for s in m.strokes.iter().filter(|s| !s.points.is_empty()) {
        let r = (s.radius * long).max(0.5);
        let pts = stamps(s, w, h, r);
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in &pts {
            (x0, y0, x1, y1) = (x0.min(x - r), y0.min(y - r), x1.max(x + r), y1.max(y + r));
        }
        let bx0 = x0.floor().max(0.0) as usize;
        let by0 = y0.floor().max(0.0) as usize;
        let bx1 = (x1.ceil().max(0.0) as usize).min(w);
        let by1 = (y1.ceil().max(0.0) as usize).min(h);
        if bx0 >= bx1 || by0 >= by1 {
            continue;
        }
        let flow = (s.flow / 100.0).clamp(0.0, 1.0) as f32;
        let (feather, erase) = (s.feather, s.erase);
        out.par_chunks_mut(w).enumerate().skip(by0).take(by1 - by0).for_each(|(y, row)| {
            let fy = y as f64;
            let mut cover = vec![0f32; bx1 - bx0];
            for &(cx, cy) in &pts {
                let dy = fy - cy;
                if dy.abs() > r {
                    continue;
                }
                let xa = (cx - r).floor().max(bx0 as f64) as usize;
                let xb = ((cx + r).ceil() as usize).min(bx1);
                for x in xa..xb {
                    let dx = x as f64 - cx;
                    let c = falloff((dx * dx + dy * dy).sqrt() / r, feather) as f32;
                    let slot = &mut cover[x - bx0];
                    if c > *slot {
                        *slot = c;
                    }
                }
            }
            for (x, &c) in cover.iter().enumerate() {
                if c <= 0.0 {
                    continue;
                }
                let v = &mut row[bx0 + x];
                let a = flow * c;
                *v = if erase { *v * (1.0 - a) } else { *v + a * (1.0 - *v) };
            }
        });
    }
}

/// The mask's weight per pixel of a `w × h` image (row-major), inversion applied.
pub fn rasterize(m: &LocalMask, w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0f32; w * h];
    match m.kind {
        MaskKind::Radial => out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let ny = y as f64 / h as f64;
            for (x, v) in row.iter_mut().enumerate() {
                *v = falloff(radial_distance(m, x as f64 / w as f64, ny, w, h), m.feather) as f32;
            }
        }),
        MaskKind::Brush => paint_brush(m, w, h, &mut out),
    }
    if m.invert {
        out.par_iter_mut().for_each(|v| *v = 1.0 - *v);
    }
    out
}

/// Hash of everything that decides the weights (not the adjustments).
pub fn shape_key(m: &LocalMask, w: usize, h: usize) -> u64 {
    let mut hs = DefaultHasher::new();
    (w, h, m.kind == MaskKind::Radial, m.invert).hash(&mut hs);
    for v in [m.feather, m.center_x, m.center_y, m.radius_x, m.radius_y, m.angle] {
        v.to_bits().hash(&mut hs);
    }
    if m.kind == MaskKind::Brush {
        for s in &m.strokes {
            (s.radius.to_bits(), s.feather.to_bits(), s.flow.to_bits(), s.erase, s.points.len()).hash(&mut hs);
            for p in &s.points {
                (p.0.to_bits(), p.1.to_bits()).hash(&mut hs);
            }
        }
    }
    hs.finish()
}

/// `rasterize`, through a small cache (the last few shapes and sizes).
pub fn weights(m: &LocalMask, w: usize, h: usize) -> Arc<Vec<f32>> {
    static CACHE: OnceLock<Mutex<Vec<(u64, Arc<Vec<f32>>)>>> = OnceLock::new();
    const KEEP: usize = 8;
    let key = shape_key(m, w, h);
    let cache = CACHE.get_or_init(Default::default);
    {
        let mut c = cache.lock().unwrap();
        if let Some(i) = c.iter().position(|(k, _)| *k == key) {
            let e = c.remove(i);
            let out = e.1.clone();
            c.push(e);
            return out;
        }
    }
    let out = Arc::new(rasterize(m, w, h));
    let mut c = cache.lock().unwrap();
    c.push((key, out.clone()));
    if c.len() > KEEP {
        c.remove(0);
    }
    out
}

// ---- applying ------------------------------------------------------------------------

/// One mask's adjustments, scaled like a gradient's (`pipeline::GradientK`).
#[derive(Debug, Clone, Copy)]
pub struct MaskAdjust {
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub saturation: f32,
}

/// The mask pass: each active mask's adjustments and weights at the pass's size.
#[derive(Debug, Clone)]
pub struct MaskParams {
    pub masks: Vec<(MaskAdjust, Arc<Vec<f32>>)>,
    pub width: usize,
    pub height: usize,
}

impl MaskParams {
    pub fn new(adj: &ImageAdjustments, width: usize, height: usize) -> Self {
        let masks = adj
            .masks
            .iter()
            .filter(|m| m.has_effect())
            .map(|m| {
                let a = MaskAdjust {
                    exposure: m.exposure as f32,
                    contrast: (m.contrast / 100.0) as f32,
                    highlights: (m.highlights / 100.0) as f32,
                    shadows: (m.shadows / 100.0) as f32,
                    saturation: (m.saturation / 100.0) as f32,
                };
                (a, weights(m, width, height))
            })
            .collect();
        Self { masks, width, height }
    }
}

/// Adjust one pixel through weight `m` — the linear gradient's formulas (`gradient_px` with
/// linear exposure), with the weight given instead of computed from a line.
#[inline(always)]
pub fn mask_px(px: &mut [f32], k: &MaskAdjust, m: f32, dl: &[f32], el: &[f32]) {
    if m <= 0.0 {
        return;
    }
    let (mut r, mut g, mut b);
    if k.exposure == 0.0 {
        r = px[0];
        g = px[1];
        b = px[2];
    } else {
        let exp_mul = 2.0f32.powf(k.exposure * m);
        r = color::encode(el, color::linearize(dl, px[0]) * exp_mul);
        g = color::encode(el, color::linearize(dl, px[1]) * exp_mul);
        b = color::encode(el, color::linearize(dl, px[2]) * exp_mul);
    }
    let luma = 0.299f32 * r + 0.587f32 * g + 0.114f32 * b;
    if k.saturation != 0.0 {
        let f = 1.0 + k.saturation * m;
        r = luma + (r - luma) * f;
        g = luma + (g - luma) * f;
        b = luma + (b - luma) * f;
    }
    if k.contrast != 0.0 {
        let c = k.contrast * m;
        r = 0.5 + (r - 0.5) * (1.0 + c);
        g = 0.5 + (g - 0.5) * (1.0 + c);
        b = 0.5 + (b - 0.5) * (1.0 + c);
    }
    if k.highlights != 0.0 {
        let w_h = luma * luma * k.highlights * 0.5 * m;
        r += w_h;
        g += w_h;
        b += w_h;
    }
    if k.shadows != 0.0 {
        let w_s = (1.0 - luma) * (1.0 - luma) * k.shadows * 0.5 * m;
        r += w_s;
        g += w_s;
        b += w_s;
    }
    px[0] = r.max(0.0);
    px[1] = g.max(0.0);
    px[2] = b.max(0.0);
}

/// The mask pass over a whole image, in place (masks stacked in order).
pub fn apply(buf: &mut FloatImage, p: &MaskParams) {
    debug_assert!(buf.width == p.width && buf.height == p.height);
    let (dl, el) = (color::decode_lut(), color::encode_lut());
    let w = buf.width;
    buf.par_rows_mut(|y, row| {
        for (x, px) in row.chunks_exact_mut(4).enumerate() {
            let i = y * w + x;
            for (k, wts) in &p.masks {
                mask_px(px, k, wts[i], dl, el);
            }
        }
    });
}

/// The parameter words the GPU kernel reads: count, width, height, then 5 per mask.
pub fn words(p: &MaskParams) -> Vec<f32> {
    let mut v = vec![p.masks.len() as f32, p.width as f32, p.height as f32, 0.0];
    for (k, _) in &p.masks {
        v.extend_from_slice(&[k.exposure, k.contrast, k.highlights, k.shadows, k.saturation]);
    }
    v
}

/// All masks' weights back to back (the GPU's weight buffer).
pub fn packed_weights(p: &MaskParams) -> Vec<f32> {
    let mut v = Vec::with_capacity(p.masks.len() * p.width * p.height);
    for (_, w) in &p.masks {
        v.extend_from_slice(w);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BrushStroke;

    #[test]
    fn falloff_is_one_inside_zero_outside_and_monotone() {
        for feather in [0.0, 25.0, 50.0, 100.0] {
            assert_eq!(falloff(0.0, feather), 1.0);
            assert_eq!(falloff(1.0, feather), 0.0);
            assert_eq!(falloff(1.5, feather), 0.0);
            let mut prev = 1.0;
            for i in 0..=200 {
                let v = falloff(i as f64 / 200.0, feather);
                assert!(v <= prev + 1e-12 && (0.0..=1.0).contains(&v));
                prev = v;
            }
        }
        // Hard edge without feather, a soft one with.
        assert_eq!(falloff(0.99, 0.0), 1.0);
        let soft = falloff(0.75, 50.0);
        assert!(soft > 0.0 && soft < 1.0, "{soft}");
    }

    #[test]
    fn radial_weights_and_inversion() {
        let m = LocalMask { radius_x: 0.2, radius_y: 0.2, feather: 40.0, ..Default::default() };
        let (w, h) = (300, 200);
        let r = rasterize(&m, w, h);
        assert_eq!(r[100 * w + 150], 1.0); // centre
        assert_eq!(r[5 * w + 5], 0.0); // corner
        // Round in pixels whatever the aspect: same weight 50 px right and 50 px down.
        assert!((r[100 * w + 200] - r[150 * w + 150]).abs() < 1e-6);
        let inv = rasterize(&LocalMask { invert: true, ..m.clone() }, w, h);
        for (a, b) in r.iter().zip(&inv) {
            assert!((a + b - 1.0).abs() < 1e-6);
        }
        // Rotated ellipse: the long axis follows the angle.
        let e = LocalMask { radius_x: 0.3, radius_y: 0.05, angle: 90.0, feather: 0.0, ..Default::default() };
        let re = rasterize(&e, 200, 200);
        assert_eq!(re[(100 + 40) * 200 + 100], 1.0); // along y now
        assert_eq!(re[100 * 200 + 140], 0.0);
    }

    #[test]
    fn brush_covers_its_stroke_and_erase_removes() {
        let stroke = BrushStroke { radius: 0.05, feather: 0.0, flow: 100.0, erase: false, points: vec![(0.2, 0.5), (0.8, 0.5)] };
        let mut m = LocalMask { kind: MaskKind::Brush, strokes: vec![stroke.clone()], ..Default::default() };
        let (w, h) = (200, 100);
        let r = rasterize(&m, w, h);
        assert_eq!(r[50 * w + 100], 1.0); // on the stroke
        assert_eq!(r[10 * w + 100], 0.0); // away from it
        // Coverage does not depend on how many points were sampled along the line.
        let dense = BrushStroke { points: (0..=60).map(|i| (0.2 + i as f64 * 0.01, 0.5)).collect(), ..stroke.clone() };
        let rd = rasterize(&LocalMask { strokes: vec![dense], ..m.clone() }, w, h);
        let covered = r.iter().filter(|&&v| v > 0.0).count();
        let differ = r.iter().zip(&rd).filter(|(a, b)| (*a - *b).abs() > 1e-6).count();
        assert!(covered > 1000 && differ * 50 < covered, "{differ} of {covered} differ"); // only edge pixels
        // Half flow twice builds up; an erase stroke clears.
        m.strokes = vec![BrushStroke { flow: 50.0, ..stroke.clone() }, BrushStroke { flow: 50.0, ..stroke.clone() }];
        assert!((rasterize(&m, w, h)[50 * w + 100] - 0.75).abs() < 1e-6);
        m.strokes.push(BrushStroke { erase: true, ..stroke });
        assert_eq!(rasterize(&m, w, h)[50 * w + 100], 0.0);
    }

    #[test]
    fn weights_are_cached_by_shape_not_adjustment() {
        let a = LocalMask { exposure: 1.0, ..Default::default() };
        let b = LocalMask { exposure: -2.0, contrast: 30.0, ..a.clone() };
        assert!(Arc::ptr_eq(&weights(&a, 64, 48), &weights(&b, 64, 48)));
        let c = LocalMask { center_x: 0.4, ..a.clone() };
        assert!(!Arc::ptr_eq(&weights(&a, 64, 48), &weights(&c, 64, 48)));
    }

    /// A mask sits on the scene, not on the frame: rendering with a 90° rotation or a crop
    /// gives the rotated / cropped pixels of the unrotated, uncropped render.
    #[test]
    fn masks_follow_the_scene_under_rotation_and_crop() {
        use crate::pipeline::{apply_to_float, rotate_discrete, ProcessContext};
        use crate::Rotation;
        let (w, h) = (64usize, 48usize);
        let mut src = FloatImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let o = src.index(x, y);
                src.data[o..o + 4].copy_from_slice(&[0.2 + 0.5 * x as f32 / w as f32, 0.3 + 0.4 * y as f32 / h as f32, 0.35, 1.0]);
            }
        }
        let base = ImageAdjustments {
            masks: vec![
                LocalMask { center_x: 0.3, center_y: 0.6, radius_x: 0.2, radius_y: 0.1, angle: 30.0, exposure: 1.0, saturation: 40.0, ..Default::default() },
                LocalMask { kind: MaskKind::Brush, contrast: 30.0, strokes: vec![BrushStroke { radius: 0.05, feather: 50.0, flow: 100.0, erase: false, points: vec![(0.6, 0.2), (0.9, 0.4)] }], ..Default::default() },
            ],
            ..Default::default()
        };
        assert!(base.has_active_mask());
        let ctx = ProcessContext::default();
        let plain = apply_to_float(&src, &base, &ctx);
        let rotated = apply_to_float(&src, &ImageAdjustments { rotation: Rotation::R90, ..base.clone() }, &ctx);
        assert_eq!(rotated.data, rotate_discrete(&plain, Rotation::R90).data);
        let crop = ImageAdjustments { crop_x: 0.25, crop_y: 0.25, crop_width: 0.5, crop_height: 0.5, ..base.clone() };
        let cropped = apply_to_float(&src, &crop, &ctx);
        assert_eq!((cropped.width, cropped.height), (32, 24));
        for y in 0..24 {
            for x in 0..32 {
                let (a, b) = (cropped.index(x, y), plain.index(x + 16, y + 12));
                assert_eq!(cropped.data[a..a + 3], plain.data[b..b + 3], "({x},{y})");
            }
        }
        // And the masks did something.
        let none = apply_to_float(&src, &ImageAdjustments { masks: Vec::new(), ..base }, &ctx);
        assert!(none.data != plain.data);
    }

    #[test]
    fn zero_weight_changes_nothing_and_full_weight_is_the_gradient_maths() {
        let (dl, el) = (color::decode_lut(), color::encode_lut());
        let k = MaskAdjust { exposure: 1.0, contrast: 0.2, highlights: -0.3, shadows: 0.25, saturation: -0.4 };
        let mut px = [0.4f32, 0.3, 0.2, 1.0];
        let orig = px;
        mask_px(&mut px, &k, 0.0, dl, el);
        assert_eq!(px, orig);
        mask_px(&mut px, &k, 1.0, dl, el);
        assert!(px[0] > orig[0]); // +1 EV brightens
    }
}
