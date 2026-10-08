//! Image acquisition and the RAW_TEMP thumbnail / proxy caches. Port of `RawLoader.swift`:
//! RAW through LibRaw (falling back to the embedded preview), everything else through the
//! `image` crate.

use crate::{codec, exif, paths};
use awpr_core::color::WhiteBalanceReference;
use awpr_core::libraw::{self, Thumbnail};
use awpr_core::pipeline::rotate_discrete;
use awpr_core::resize::{resize_to_fit, resize_to_max_dim};
use awpr_core::{FloatImage, Rotation};
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
    let scaled = resize_to_max_dim(full, PROXY_MAX_DIM);
    let ok = codec::save_png(&scaled, &png).is_ok();
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
