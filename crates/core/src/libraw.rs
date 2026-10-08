//! RAW decoding through LibRaw 0.22.2 with the same settings as the Windows and macOS
//! builds (output_color = sRGB, auto-bright on). Port of `LibRawBridge.swift`.

use crate::buffer::FloatImage;
use crate::color;
use crate::model::CameraColorInfo;
use awpr_libraw_sys as sys;
use rayon::prelude::*;
use std::ffi::{CStr, CString};

/// LibRaw's reported sizes.
#[derive(Debug, Clone, Copy, Default)]
pub struct RawSizes {
    pub raw_width: i32,
    pub raw_height: i32,
    pub width: i32,
    pub height: i32,
    pub left_margin: i32,
    pub top_margin: i32,
    pub iwidth: i32,
    pub iheight: i32,
    pub flip: i32,
}

pub fn available() -> bool {
    unsafe { sys::awpr_libraw_available() != 0 }
}

/// Every camera this LibRaw build supports ("Make Model"), in LibRaw's order.
pub fn camera_list() -> Vec<String> {
    unsafe {
        (0..sys::awpr_camera_count())
            .filter_map(|i| {
                let p = sys::awpr_camera_name(i);
                (!p.is_null()).then(|| CStr::from_ptr(p).to_string_lossy().into_owned())
            })
            .collect()
    }
}

pub fn version() -> String {
    unsafe {
        let p = sys::awpr_libraw_version();
        if p.is_null() {
            String::new()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
}

/// LibRaw's demosaic uses large on-stack buffers; the Windows build hit the 1 MB
/// thread-pool stack and the Swift build the 512 KB secondary-thread one. Run every
/// native call on a dedicated 64 MB-stack thread.
fn run_large_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .name("LibRawDecode".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("spawn LibRaw thread")
        .join()
        .expect("LibRaw thread panicked")
}

fn c_path(path: &str) -> Option<CString> {
    CString::new(path.as_bytes()).ok()
}

pub fn read_sizes(path: &str) -> Option<RawSizes> {
    let cp = c_path(path)?;
    run_large_stack(move || {
        let mut s = sys::awpr_sizes::default();
        if unsafe { sys::awpr_read_sizes(cp.as_ptr(), &mut s) } == 0 {
            return None;
        }
        Some(RawSizes {
            raw_width: s.raw_width,
            raw_height: s.raw_height,
            width: s.width,
            height: s.height,
            left_margin: s.left_margin,
            top_margin: s.top_margin,
            iwidth: s.iwidth,
            iheight: s.iheight,
            flip: s.flip,
        })
    })
}

/// The camera's colour data for the white-balance matrix. None when LibRaw cannot open
/// the file, the numbers are degenerate, or the sensor has four colour channels.
pub fn read_camera_color(path: &str) -> Option<CameraColorInfo> {
    let cp = c_path(path)?;
    run_large_stack(move || {
        let mut pre = [0.0f64; 3];
        let mut cam = [0.0f64; 3];
        let mut rgb = [0.0f64; 9];
        let ok = unsafe {
            sys::awpr_read_camera_color(cp.as_ptr(), pre.as_mut_ptr(), cam.as_mut_ptr(), rgb.as_mut_ptr())
        };
        if ok == 0 {
            return None;
        }
        let mut info = CameraColorInfo { pre_mul: pre, cam_mul: cam, rgb_cam: rgb };
        // Some files record cam_mul as all zeros: fall back to pre_mul as the as-shot.
        if !(info.cam_mul[0] > 0.0 && info.cam_mul[1] > 0.0 && info.cam_mul[2] > 0.0) {
            info.cam_mul = info.pre_mul;
        }
        if !info.is_valid() {
            return None;
        }
        info.pre_mul = color::normalize_green(info.pre_mul);
        info.cam_mul = color::normalize_green(info.cam_mul);
        if !info.is_valid() || color::invert3(&info.rgb_cam).is_none() {
            return None;
        }
        Some(info)
    })
}

/// Full-resolution demosaiced sRGB decode. `bps` is 8 or 16.
///
/// `expected_visible` is the EXIF visible size used to trim LibRaw's mask border on
/// models it has no crop table for. hashtest passes None, like the C# and Swift ones.
pub fn decode_full(path: &str, bps: i32, expected_visible: Option<(usize, usize)>) -> Option<FloatImage> {
    decode(path, bps, expected_visible, true)
}

/// LibRaw's output exactly as it comes, mask border included (diagnostics).
pub fn decode_full_untrimmed(path: &str, bps: i32) -> Option<FloatImage> {
    decode(path, bps, None, false)
}

/// Full-resolution linear camera RGB for 處理版本 3: no white balance, no colour matrix,
/// no gamma, no auto-bright, 16-bit, sensor clip = 1.0, flip applied. None for files
/// LibRaw cannot decode and for four-colour sensors. The mask border is trimmed the way
/// `decode_full` trims it, so both decodes cover the same pixels.
pub fn decode_linear(path: &str, expected_visible: Option<(usize, usize)>) -> Option<FloatImage> {
    let cp = c_path(path)?;
    run_large_stack(move || {
        let mut img = sys::awpr_image::default();
        if unsafe { sys::awpr_decode_linear(cp.as_ptr(), &mut img) } == 0 {
            return None;
        }
        let out = (|| {
            if img.data.is_null() || img.bits != 16 {
                return None;
            }
            let (w, h, colors) = (img.width as usize, img.height as usize, img.colors as usize);
            let s = unsafe { std::slice::from_raw_parts(img.data as *const u16, w * h * colors) };
            let rect = visible_rect(&|i| s[i] as u32, w, h, colors, 2 * 257, expected_visible);
            buffer_from(s, w, colors, rect, 1.0f32 / 65535.0f32)
        })();
        unsafe { sys::awpr_free_image(&mut img) };
        out
    })
}

fn decode(path: &str, bps: i32, expected_visible: Option<(usize, usize)>, trim: bool) -> Option<FloatImage> {
    let cp = c_path(path)?;
    run_large_stack(move || {
        let mut img = sys::awpr_image::default();
        if unsafe { sys::awpr_decode_full(cp.as_ptr(), bps, &mut img) } == 0 {
            return None;
        }
        let out = (|| {
            if img.data.is_null() {
                return None;
            }
            let w = img.width as usize;
            let h = img.height as usize;
            let colors = img.colors as usize;
            let bits = img.bits;
            let n = w * h * colors;
            if bits == 16 {
                let s = unsafe { std::slice::from_raw_parts(img.data as *const u16, n) };
                let rect = if trim {
                    visible_rect(&|i| s[i] as u32, w, h, colors, 2 * 257, expected_visible)
                } else {
                    Rect { x: 0, y: 0, w, h }
                };
                buffer_from(s, w, colors, rect, 1.0f32 / 65535.0f32)
            } else {
                let s = unsafe { std::slice::from_raw_parts(img.data as *const u8, n) };
                let rect = if trim {
                    visible_rect(&|i| s[i] as u32, w, h, colors, 2, expected_visible)
                } else {
                    Rect { x: 0, y: 0, w, h }
                };
                buffer_from(s, w, colors, rect, 1.0f32 / 255.0f32)
            }
        })();
        unsafe { sys::awpr_free_image(&mut img) };
        out
    })
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

const COMMON_ASPECTS: [f64; 6] = [3.0 / 2.0, 4.0 / 3.0, 16.0 / 9.0, 1.0, 5.0 / 4.0, 7.0 / 5.0];

fn plausible_aspect(w: usize, h: usize) -> bool {
    if w == 0 || h == 0 {
        return false;
    }
    let a = w as f64 / h as f64;
    COMMON_ASPECTS.iter().any(|&t| {
        let inv = 1.0 / t;
        (a - t).abs() / t < 0.005 || (a - inv).abs() / inv < 0.005
    })
}

/// Trim the mask border LibRaw leaves on models it has no crop table for. See the long
/// comment on `LibRawBridge.visibleRect` in the Swift build for the reasoning.
fn visible_rect(
    px: &dyn Fn(usize) -> u32,
    w: usize,
    h: usize,
    colors: usize,
    thr: u32,
    expected: Option<(usize, usize)>,
) -> Rect {
    let full = Rect { x: 0, y: 0, w, h };
    let row_stride = w * colors;
    let dark = |i: usize| px(i) <= thr && px(i + 1) <= thr && px(i + 2) <= thr;
    let col_black = |x: usize| (0..h).all(|y| dark(y * row_stride + x * colors));
    let row_black = |y: usize| (0..w).all(|x| dark(y * row_stride + x * colors));

    if let Some((mut ew, mut eh)) = expected {
        if ew > 0 && eh > 0 {
            // LibRaw has already applied flip, so swap for portrait.
            if (w < h) != (ew < eh) {
                std::mem::swap(&mut ew, &mut eh);
            }
            if ew <= w && eh <= h && (ew < w || eh < h) {
                let x = if w > ew && col_black(0) { w - ew } else { 0 };
                let y = if h > eh && row_black(0) { h - eh } else { 0 };
                return Rect { x, y, w: ew, h: eh };
            }
            return full;
        }
    }

    if plausible_aspect(w, h) {
        return full;
    }
    let (max_trim_x, max_trim_y) = (w / 8, h / 8);
    let (mut right, mut left, mut bottom, mut top) = (0, 0, 0, 0);
    while right < max_trim_x && col_black(w - 1 - right) {
        right += 1;
    }
    while left < max_trim_x && left < w - right - 1 && col_black(left) {
        left += 1;
    }
    while bottom < max_trim_y && row_black(h - 1 - bottom) {
        bottom += 1;
    }
    while top < max_trim_y && top < h - bottom - 1 && row_black(top) {
        top += 1;
    }
    let nw = w - left - right;
    let nh = h - top - bottom;
    if nw == 0 || nh == 0 || (nw == w && nh == h) || !plausible_aspect(nw, nh) {
        return full;
    }
    Rect { x: left, y: top, w: nw, h: nh }
}

fn buffer_from<T: Copy + Into<f32> + Sync>(s: &[T], src_w: usize, colors: usize, rect: Rect, inv: f32) -> Option<FloatImage> {
    if rect.w == 0 || rect.h == 0 {
        return None;
    }
    let mut buf = FloatImage::new(rect.w, rect.h);
    let row_stride = src_w * colors;
    buf.data.par_chunks_mut(rect.w * 4).enumerate().for_each(|(y, row)| {
        let base = (y + rect.y) * row_stride + rect.x * colors;
        for x in 0..rect.w {
            let si = base + x * colors;
            let o = x * 4;
            row[o] = s[si].into() * inv;
            row[o + 1] = s[si + 1].into() * inv;
            row[o + 2] = s[si + 2].into() * inv;
            row[o + 3] = 1.0;
        }
    });
    Some(buf)
}

/// What LibRaw parsed out of the file's headers (the info panel's source on all
/// platforms; the Windows build reads the same fields through ExifTool, Swift through
/// ImageIO).
#[derive(Debug, Clone, Default)]
pub struct RawMeta {
    pub make: String,
    pub model: String,
    pub lens: String,
    pub iso_speed: f32,
    /// Seconds.
    pub shutter: f32,
    pub aperture: f32,
    /// Millimetres.
    pub focal_len: f32,
    /// Unix time of DateTimeOriginal as LibRaw stores it (local time), 0 = unknown.
    pub timestamp: i64,
    /// Visible size after the camera's flip.
    pub width: i32,
    pub height: i32,
    pub flip: i32,
}

fn c_string(buf: &[std::os::raw::c_char]) -> String {
    let bytes: Vec<u8> = buf.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

pub fn read_meta(path: &str) -> Option<RawMeta> {
    let cp = c_path(path)?;
    run_large_stack(move || {
        let mut m = sys::awpr_meta::default();
        if unsafe { sys::awpr_read_meta(cp.as_ptr(), &mut m) } == 0 {
            return None;
        }
        Some(RawMeta {
            make: c_string(&m.make),
            model: c_string(&m.model),
            lens: c_string(&m.lens),
            iso_speed: m.iso_speed,
            shutter: m.shutter,
            aperture: m.aperture,
            focal_len: m.focal_len,
            timestamp: m.timestamp,
            width: m.width,
            height: m.height,
            flip: m.flip,
        })
    })
}

/// The camera's embedded preview.
pub enum Thumbnail {
    /// An encoded JPEG, plus LibRaw's flip (3 = 180°, 5 = ccw 90°, 6 = cw 90°).
    Jpeg(Vec<u8>, i32),
    /// Decoded pixels, plus the flip.
    Pixels(FloatImage, i32),
}

/// The embedded preview, fast (no demosaic). None when the file has none LibRaw can read.
pub fn decode_thumb(path: &str) -> Option<Thumbnail> {
    let cp = c_path(path)?;
    run_large_stack(move || {
        let mut img = sys::awpr_image::default();
        let mut flip = 0;
        if unsafe { sys::awpr_decode_thumb(cp.as_ptr(), &mut img, &mut flip) } == 0 {
            return None;
        }
        let out = (|| {
            if img.data.is_null() || img.data_size <= 0 {
                return None;
            }
            if img.type_ == 1 {
                let s = unsafe { std::slice::from_raw_parts(img.data as *const u8, img.data_size as usize) };
                return Some(Thumbnail::Jpeg(s.to_vec(), flip));
            }
            let (w, h, colors) = (img.width as usize, img.height as usize, img.colors as usize);
            if w == 0 || h == 0 || colors < 3 {
                return None;
            }
            let rect = Rect { x: 0, y: 0, w, h };
            let buf = if img.bits == 16 {
                let s = unsafe { std::slice::from_raw_parts(img.data as *const u16, w * h * colors) };
                buffer_from(s, w, colors, rect, 1.0f32 / 65535.0f32)
            } else {
                let s = unsafe { std::slice::from_raw_parts(img.data as *const u8, w * h * colors) };
                buffer_from(s, w, colors, rect, 1.0f32 / 255.0f32)
            }?;
            Some(Thumbnail::Pixels(buf, flip))
        })();
        unsafe { sys::awpr_free_image(&mut img) };
        out
    })
}
