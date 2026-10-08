//! Separable area-average resize. Port of `CacheManager.resizeFloat*`.

use crate::buffer::{round_half_even, FloatImage};

/// Shrink so the long side is at most `max_dim` (never upsizes).
pub fn resize_to_max_dim(src: FloatImage, max_dim: usize) -> FloatImage {
    let long_side = src.width.max(src.height);
    if long_side <= max_dim {
        return src;
    }
    let scale = max_dim as f64 / long_side as f64;
    let dw = (round_half_even(src.width as f64 * scale) as i64).max(1) as usize;
    let dh = (round_half_even(src.height as f64 * scale) as i64).max(1) as usize;
    resize(&src, dw, dh)
}

/// Area-average resize to an exact size.
pub fn resize(src: &FloatImage, dw: usize, dh: usize) -> FloatImage {
    let (sw, sh) = (src.width, src.height);
    if dw == sw && dh == sh {
        return src.clone();
    }
    let sd = &src.data;

    // horizontal pass: src (sw×sh) → tmp (dw×sh)
    let mut tmp = FloatImage::new(dw, sh);
    tmp.par_rows_mut(|y, trow| {
        let srow = y * sw * 4;
        for x in 0..dw {
            let x0 = x as f64 * sw as f64 / dw as f64;
            let x1 = (x + 1) as f64 * sw as f64 / dw as f64;
            let (mut r, mut g, mut b, mut wsum) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            let mut sx = x0 as usize;
            let end = sw.min(x1.ceil() as usize);
            while sx < end {
                let cover = (x1.min((sx + 1) as f64) - x0.max(sx as f64)) as f32;
                if cover > 0.0 {
                    let si = srow + sx * 4;
                    r += sd[si] * cover;
                    g += sd[si + 1] * cover;
                    b += sd[si + 2] * cover;
                    wsum += cover;
                }
                sx += 1;
            }
            let ti = x * 4;
            if wsum > 0.0 {
                trow[ti] = r / wsum;
                trow[ti + 1] = g / wsum;
                trow[ti + 2] = b / wsum;
            }
            trow[ti + 3] = 1.0;
        }
    });

    // vertical pass: tmp (dw×sh) → dst (dw×dh)
    let td = &tmp.data;
    let mut dst = FloatImage::new(dw, dh);
    dst.par_rows_mut(|y, drow| {
        let y0 = y as f64 * sh as f64 / dh as f64;
        let y1 = (y + 1) as f64 * sh as f64 / dh as f64;
        for x in 0..dw {
            let (mut r, mut g, mut b, mut wsum) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            let mut sy = y0 as usize;
            let end = sh.min(y1.ceil() as usize);
            while sy < end {
                let cover = (y1.min((sy + 1) as f64) - y0.max(sy as f64)) as f32;
                if cover > 0.0 {
                    let ti = (sy * dw + x) * 4;
                    r += td[ti] * cover;
                    g += td[ti + 1] * cover;
                    b += td[ti + 2] * cover;
                    wsum += cover;
                }
                sy += 1;
            }
            let di = x * 4;
            if wsum > 0.0 {
                drow[di] = r / wsum;
                drow[di + 1] = g / wsum;
                drow[di + 2] = b / wsum;
            }
            drow[di + 3] = 1.0;
        }
    });
    dst
}

/// Shrink to fit inside `max_w`×`max_h`, keeping the aspect ratio (never upsizes).
pub fn resize_to_fit(src: &FloatImage, max_w: usize, max_h: usize) -> FloatImage {
    let scale = (max_w as f64 / src.width as f64).min(max_h as f64 / src.height as f64).min(1.0);
    let w = (round_half_even(src.width as f64 * scale) as i64).max(1) as usize;
    let h = (round_half_even(src.height as f64 * scale) as i64).max(1) as usize;
    resize(src, w, h)
}
