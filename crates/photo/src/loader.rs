//! Image acquisition and the RAW_TEMP thumbnail / proxy caches. Port of `RawLoader.swift`:
//! RAW through LibRaw (falling back to the embedded preview), everything else through the
//! `image` crate.

use crate::{codec, exif, paths};
use awpr_core::color::WhiteBalanceReference;
use awpr_core::libraw::{self, Thumbnail};
use awpr_core::pipeline::rotate_discrete;
use awpr_core::resize::{resize_to_fit, resize_to_max_dim};
use awpr_core::pipeline::{ProcessContext, SourceKind};
use awpr_core::{v3, CameraColorInfo, FloatImage, ImageAdjustments, Rotation};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub const PROXY_MAX_DIM: usize = 2560;
pub const THUMB_MAX_W: usize = 240;
pub const THUMB_MAX_H: usize = 160;
const THUMB_QUALITY: u8 = 88;

/// How a photo's pixels were produced — decides what white balance is already baked in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeSource {
    /// LibRaw's own decode: balanced with `pre_mul` (daylight).
    LibRaw,
    /// A camera-rendered image (regular file or embedded preview): `cam_mul` baked in.
    Rendered,
}

impl DecodeSource {
    pub fn white_balance_reference(self) -> WhiteBalanceReference {
        match self {
            Self::LibRaw => WhiteBalanceReference::Decode,
            Self::Rendered => WhiteBalanceReference::AsShot,
        }
    }

    /// The word in the `.src` marker (shared with the Swift build).
    fn marker(self) -> &'static str {
        match self {
            Self::LibRaw => "libraw",
            Self::Rendered => "imageio",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LoaderOptions {
    pub use_libraw: bool,
    /// 16-bit decode and the `.f16` proxy (Settings → RAW 處理精度).
    pub high_precision: bool,
}

impl Default for LoaderOptions {
    fn default() -> Self {
        Self { use_libraw: true, high_precision: false }
    }
}

fn apply_flip(buf: FloatImage, flip: i32) -> FloatImage {
    match flip {
        3 => rotate_discrete(&buf, Rotation::R180),
        5 => rotate_discrete(&buf, Rotation::R270),
        6 => rotate_discrete(&buf, Rotation::R90),
        _ => buf,
    }
}

/// The camera's embedded preview, upright.
pub fn embedded_preview(path: &str) -> Option<FloatImage> {
    match libraw::decode_thumb(path)? {
        Thumbnail::Jpeg(bytes, flip) => {
            let b = codec::load_float_bytes(&bytes)?;
            // A preview that carries its own orientation tag is already upright.
            Some(if flip != 0 && codec::orientation_of(&bytes) <= 1 { apply_flip(b, flip) } else { b })
        }
        Thumbnail::Pixels(b, flip) => Some(apply_flip(b, flip)),
    }
}

/// Full-resolution, upright pixels and how they were made. None only on total failure.
pub fn decode_full(path: &str, opt: LoaderOptions) -> Option<(FloatImage, DecodeSource)> {
    if paths::is_raw(path) {
        if opt.use_libraw && libraw::available() {
            // The visible size trims the mask border LibRaw leaves on models it has no
            // crop table for (and matches what the native builds output).
            let vis = exif::visible_size(path);
            let bps = if opt.high_precision { 16 } else { 8 };
            if let Some(b) = libraw::decode_full(path, bps, vis) {
                return Some((b, DecodeSource::LibRaw));
            }
        }
        return embedded_preview(path).map(|b| (b, DecodeSource::Rendered));
    }
    codec::load_float(path).map(|b| (b, DecodeSource::Rendered))
}

// ---- per-file generation lock -------------------------------------------------------

fn path_lock(key: &str) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(Default::default).lock().unwrap().entry(key.to_string()).or_default().clone()
}

// ---- thumbnails ------------------------------------------------------------------------

/// Make sure `RAW_TEMP/{file}_thumb.jpg` exists (camera preview for RAW, else a downscale).
pub fn ensure_thumbnail_cache(path: &str, opt: LoaderOptions) -> bool {
    let cache = paths::thumbnail_path(path);
    let lock = path_lock(&cache);
    let _g = lock.lock().unwrap();
    if codec::is_complete(&cache) {
        return true;
    }
    let base = if paths::is_raw(path) && opt.use_libraw && libraw::available() {
        embedded_preview(path)
    } else {
        None
    };
    let Some(base) = base.or_else(|| decode_full(path, opt).map(|(b, _)| b)) else { return false };
    codec::save_jpeg(&resize_to_fit(&base, THUMB_MAX_W, THUMB_MAX_H), &cache, THUMB_QUALITY).is_ok()
}

/// What a strip thumbnail is rendered from: the proxy-cut thumbnail once it exists (the
/// editor's own pixels), else the camera preview / regular-file thumbnail.
pub struct ThumbnailBase {
    pub buffer: FloatImage,
    /// None = the camera preview (cam_mul baked in) or a regular file.
    pub proxy_source: Option<DecodeSource>,
}

pub fn load_thumbnail_base(path: &str) -> Option<ThumbnailBase> {
    if paths::is_raw(path) {
        if let Some(b) = codec::load_cache(&paths::proxy_thumbnail_path(path)) {
            return Some(ThumbnailBase { buffer: b, proxy_source: Some(proxy_source(path)) });
        }
    }
    codec::load_cache(&paths::thumbnail_path(path)).map(|b| ThumbnailBase { buffer: b, proxy_source: None })
}

// ---- proxy -----------------------------------------------------------------------------

/// What the cached proxy's pixels are already balanced to.
pub fn proxy_source(path: &str) -> DecodeSource {
    if !paths::is_raw(path) {
        return DecodeSource::Rendered;
    }
    match std::fs::read_to_string(paths::proxy_source_path(path)).map(|s| s.trim().to_string()) {
        Ok(s) if s == "imageio" => DecodeSource::Rendered,
        // No marker (older caches) means LibRaw, which is what they were.
        _ => DecodeSource::LibRaw,
    }
}

/// Build the proxy (and its strip thumbnail) unless it is already cached.
pub fn ensure_proxy_cache(path: &str, opt: LoaderOptions) -> bool {
    let png = paths::proxy_path(path);
    let lock = path_lock(&png);
    let _g = lock.lock().unwrap();
    let need_png = !codec::is_complete(&png);
    let need_half = opt.high_precision && !std::path::Path::new(&paths::proxy_f16_path(path)).exists();
    if !need_png && !need_half {
        if paths::is_raw(path) && !std::path::Path::new(&paths::proxy_thumbnail_path(path)).exists() {
            if let Some(p) = codec::load_cache(&png) {
                write_proxy_thumbnail(path, &p);
            }
        }
        return true;
    }
    let Some((full, source)) = decode_full(path, opt) else { return false };
    write_proxy(path, full, source, opt)
}

/// The proxy files from a full decode: PNG, `.f16` in high precision, the source marker
/// and the strip thumbnail.
fn write_proxy(path: &str, full: FloatImage, source: DecodeSource, opt: LoaderOptions) -> bool {
    let scaled = resize_to_max_dim(full, PROXY_MAX_DIM);
    let ok = codec::save_png(&scaled, &paths::proxy_path(path)).is_ok();
    if opt.high_precision {
        let _ = codec::save_half(&scaled, &paths::proxy_f16_path(path));
    }
    let _ = std::fs::write(paths::proxy_source_path(path), source.marker());
    write_proxy_thumbnail(path, &scaled);
    ok
}

fn write_proxy_thumbnail(path: &str, proxy: &FloatImage) {
    if paths::is_raw(path) {
        let _ = codec::save_jpeg(&resize_to_fit(proxy, THUMB_MAX_W, THUMB_MAX_H), &paths::proxy_thumbnail_path(path), THUMB_QUALITY);
    }
}

/// The editing proxy (generating it if needed): the `.f16` in high-precision mode, else
/// the 8-bit PNG.
pub fn load_proxy(path: &str, opt: LoaderOptions) -> Option<(FloatImage, DecodeSource)> {
    ensure_proxy_cache(path, opt);
    let source = proxy_source(path);
    if opt.high_precision {
        if let Some(f) = codec::load_half(&paths::proxy_f16_path(path)) {
            return Some((f, source));
        }
    }
    if let Some(b) = codec::load_cache(&paths::proxy_path(path)) {
        return Some((b, source));
    }
    // An unreadable cache: decode directly rather than failing the photo.
    decode_full(path, opt).map(|(b, s)| (resize_to_max_dim(b, PROXY_MAX_DIM), s))
}

// ---- 處理版本 3: the linear camera source ---------------------------------------------

/// Whether a photo can have a linear camera source: a RAW LibRaw reads with a 3×3 camera
/// matrix. Everything else renders version 3 from its gamma-encoded pixels.
pub fn linear_capable(path: &str, camera: Option<&CameraColorInfo>, opt: LoaderOptions) -> bool {
    paths::is_raw(path) && opt.use_libraw && libraw::available() && camera.is_some_and(|c| c.is_valid())
}

/// Full-resolution linear camera RGB, highlights reconstructed, and its auto-bright gain.
pub fn decode_linear_full(path: &str, camera: &CameraColorInfo) -> Option<(FloatImage, f32)> {
    let mut full = libraw::decode_linear(path, exif::visible_size(path))?;
    let gain = v3::prepare_linear_source(&mut full, camera);
    Some((full, gain))
}

fn read_gain(path: &str) -> Option<f32> {
    let s = std::fs::read_to_string(paths::proxy_v3_meta_path(path)).ok()?;
    s.lines().find_map(|l| l.trim().strip_prefix("gain=")).and_then(|v| v.trim().parse::<f32>().ok()).filter(|g| g.is_finite() && *g > 0.0)
}

/// Build the version-3 proxy unless it is cached: `RAW_TEMP/{file}.rawpipe.v3.png` and
/// its gain. The gain is measured on the full decode, as LibRaw's auto-bright is.
pub fn ensure_proxy_v3(path: &str, camera: &CameraColorInfo) -> bool {
    let png = paths::proxy_v3_path(path);
    let lock = path_lock(&png);
    let _g = lock.lock().unwrap();
    if codec::is_complete(&png) && read_gain(path).is_some() {
        return true;
    }
    let Some(full) = libraw::decode_linear(path, exif::visible_size(path)) else { return false };
    write_proxy_v3(path, full, camera)
}

/// The version-3 proxy files from LibRaw's linear decode: reconstruct, measure the gain,
/// shrink, write the PNG, then the gain.
fn write_proxy_v3(path: &str, mut full: FloatImage, camera: &CameraColorInfo) -> bool {
    let gain = v3::prepare_linear_source(&mut full, camera);
    let scaled = resize_to_max_dim(full, PROXY_MAX_DIM);
    if codec::save_linear_png(&scaled, &paths::proxy_v3_path(path)).is_err() {
        return false;
    }
    paths::write_atomic(&paths::proxy_v3_meta_path(path), format!("gain={gain}\n").as_bytes()).is_ok()
}

fn proxy_v3_ready(path: &str) -> bool {
    codec::is_complete(&paths::proxy_v3_path(path)) && read_gain(path).is_some()
}

/// The folder cache builder's entry: the usual proxy, and with `camera` (a 處理版本 3
/// RAW) the linear one too — when both are missing, from a single LibRaw open + unpack
/// (`libraw::with_unpacked`), so the second proxy costs a second `dcraw_process` instead of
/// a second decode.
pub fn ensure_proxy_caches(path: &str, opt: LoaderOptions, camera: Option<&CameraColorInfo>) -> bool {
    let need_v2 = !codec::is_complete(&paths::proxy_path(path)) || (opt.high_precision && !std::path::Path::new(&paths::proxy_f16_path(path)).exists());
    let v3_cam = camera.filter(|c| linear_capable(path, Some(c), opt));
    if let (true, Some(cam)) = (need_v2, v3_cam) {
        if !proxy_v3_ready(path) {
            let (png, v3png) = (paths::proxy_path(path), paths::proxy_v3_path(path));
            let (l1, l2) = (path_lock(&png), path_lock(&v3png));
            let g1 = l1.lock().unwrap();
            let g2 = l2.lock().unwrap();
            let bps = if opt.high_precision { 16 } else { 8 };
            let vis = exif::visible_size(path);
            let (p, cam) = (path.to_string(), cam.clone());
            // Linear first: its reconstruction, gain and 16-bit PNG then run on other cores
            // while LibRaw renders the encoded decode from the same unpack.
            libraw::with_unpacked(path, move |u| {
                let Some(u) = u else { return };
                let lin = u.linear(vis);
                std::thread::scope(|s| {
                    if let Some(l) = lin {
                        s.spawn(|| write_proxy_v3(&p, l, &cam));
                    }
                    if let Some(b) = u.encoded(bps, vis) {
                        write_proxy(&p, b, DecodeSource::LibRaw, opt);
                    }
                });
            });
            drop((g2, g1));
        }
    }
    // Whatever is still missing (a failed half, a camera preview fallback, the strip
    // thumbnail of an existing proxy) the single-purpose builders finish.
    let ok = ensure_proxy_cache(path, opt);
    if let Some(cam) = v3_cam {
        ensure_proxy_v3(path, cam);
    }
    ok
}

/// The version-3 editing proxy and its gain (building it if needed).
pub fn load_proxy_v3(path: &str, camera: &CameraColorInfo) -> Option<(FloatImage, f32)> {
    ensure_proxy_v3(path, camera);
    Some((codec::load_linear_png(&paths::proxy_v3_path(path))?, read_gain(path)?))
}

/// The editing proxy for these adjustments: the linear camera proxy for a version-3 RAW,
/// otherwise the usual one. Falls back to the usual proxy when the linear one fails.
pub fn load_proxy_for(path: &str, adj: &ImageAdjustments, camera: Option<&CameraColorInfo>, opt: LoaderOptions) -> Option<(FloatImage, DecodeSource, SourceKind)> {
    if adj.is_v3() && linear_capable(path, camera, opt) {
        if let Some((p, gain)) = camera.and_then(|c| load_proxy_v3(path, c)) {
            return Some((p, DecodeSource::LibRaw, SourceKind::LinearCamera { gain }));
        }
    }
    load_proxy(path, opt).map(|(p, s)| (p, s, SourceKind::Encoded))
}

/// The full-resolution source to render `adj` from, with its context (export).
pub fn decode_for_render(path: &str, adj: &ImageAdjustments, camera: Option<&CameraColorInfo>, opt: LoaderOptions) -> Option<(FloatImage, ProcessContext)> {
    if adj.is_v3() && linear_capable(path, camera, opt) {
        if let Some(cam) = camera {
            if let Some((full, gain)) = decode_linear_full(path, cam) {
                let ctx = ProcessContext { camera: Some(cam.clone()), source_kind: SourceKind::LinearCamera { gain }, ..Default::default() };
                return Some((full, ctx));
            }
        }
    }
    let (full, source) = decode_full(path, opt)?;
    let ctx = ProcessContext { camera: camera.cloned(), white_balance_reference: source.white_balance_reference(), ..Default::default() };
    Some((full, ctx))
}

