//! Bitmap I/O for regular formats and the RAW_TEMP cache files: JPEG / PNG / TIFF / BMP
//! through the `image` crate (pure Rust, identical on every platform), plus the 16-bit
//! `.f16` proxy in the Swift / C# format.

use awpr_core::FloatImage;
use image::{DynamicImage, ImageDecoder, ImageReader};
use rayon::prelude::*;
use std::io::Cursor;

/// HEIC needs libheif, which this build does not link yet.
pub fn heic_supported() -> bool {
    false
}

fn to_float(img: DynamicImage) -> FloatImage {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut out = FloatImage::new(w, h);
    match img {
        // 8-bit sources: exactly k/255, like the native builds' 8-bit decode.
        DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_) | DynamicImage::ImageLuma8(_) | DynamicImage::ImageLumaA8(_) => {
            let rgb = img.into_rgb8();
            let src = rgb.as_raw();
            out.data.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
                for x in 0..w {
                    let s = (y * w + x) * 3;
                    row[x * 4] = src[s] as f32 / 255.0;
                    row[x * 4 + 1] = src[s + 1] as f32 / 255.0;
                    row[x * 4 + 2] = src[s + 2] as f32 / 255.0;
                    row[x * 4 + 3] = 1.0;
                }
            });
        }
        _ => {
            let rgb = img.into_rgb32f();
            let src = rgb.as_raw();
            out.data.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
                for x in 0..w {
                    let s = (y * w + x) * 3;
                    row[x * 4] = src[s];
                    row[x * 4 + 1] = src[s + 1];
                    row[x * 4 + 2] = src[s + 2];
                    row[x * 4 + 3] = 1.0;
                }
            });
        }
    }
    out
}

/// Decode a regular image with its EXIF orientation applied.
pub fn load_float(path: &str) -> Option<FloatImage> {
    let reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    decode(reader)
}

pub fn load_float_bytes(bytes: &[u8]) -> Option<FloatImage> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?;
    decode(reader)
}

fn decode<R: std::io::BufRead + std::io::Seek>(reader: ImageReader<R>) -> Option<FloatImage> {
    let mut dec = reader.into_decoder().ok()?;
    let orientation = dec.orientation().ok();
    let mut img = DynamicImage::from_decoder(dec).ok()?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Some(to_float(img))
}

/// The EXIF orientation of an encoded image (1 when absent).
pub fn orientation_of(bytes: &[u8]) -> u32 {
    crate::tiff::read_bytes(bytes).and_then(|m| m.orientation).unwrap_or(1)
}

fn to_u8(v: f32) -> u8 {
    (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// 8-bit RGB bytes, the way every cache writer quantises.
pub fn to_rgb8(buf: &FloatImage) -> Vec<u8> {
    let mut out = vec![0u8; buf.width * buf.height * 3];
    out.par_chunks_mut(buf.width * 3).enumerate().for_each(|(y, row)| {
        let s = &buf.data[y * buf.width * 4..(y + 1) * buf.width * 4];
        for x in 0..buf.width {
            row[x * 3] = to_u8(s[x * 4]);
            row[x * 3 + 1] = to_u8(s[x * 4 + 1]);
            row[x * 3 + 2] = to_u8(s[x * 4 + 2]);
        }
    });
    out
}

/// Round every sample to 8 bits in place — what reading the saved PNG back would give.
pub fn quantize8(buf: &mut FloatImage) {
    buf.data.par_iter_mut().for_each(|v| *v = to_u8(*v) as f32 / 255.0);
}

pub fn save_jpeg(buf: &FloatImage, path: &str, quality: u8) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode(&to_rgb8(buf), buf.width as u32, buf.height as u32, image::ExtendedColorType::Rgb8)
        .map_err(std::io::Error::other)?;
    crate::paths::write_atomic(path, &bytes)
}

pub fn save_png(buf: &FloatImage, path: &str) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(&to_rgb8(buf), buf.width as u32, buf.height as u32, image::ExtendedColorType::Rgb8)
        .map_err(std::io::Error::other)?;
    crate::paths::write_atomic(path, &bytes)
}

use image::ImageEncoder;

/// A cache bitmap, or None when missing or truncated (a crash mid-write leaves a file
/// that decodes "fine" with garbage at the bottom, so it is checked first).
pub fn load_cache(path: &str) -> Option<FloatImage> {
    if !is_complete(path) {
        return None;
    }
    load_float(path)
}

/// A PNG ends with its IEND chunk, a JPEG with EOI.
pub fn is_complete(path: &str) -> bool {
    use std::io::{Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    let Ok(len) = f.seek(SeekFrom::End(0)) else { return false };
    if len < 12 || f.seek(SeekFrom::End(-8)).is_err() {
        return false;
    }
    let mut b = [0u8; 8];
    if std::io::Read::read_exact(&mut f, &mut b).is_err() {
        return false;
    }
    b == [0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82] || (b[6] == 0xFF && b[7] == 0xD9) || (b[5] == 0xFF && b[6] == 0xD9)
}

// ---- 16-bit proxy (.f16) -----------------------------------------------------------
// "AP16", i32 width, i32 height (little-endian), then RGBA u16 samples. LibRaw's
// high-precision output is 16-bit integer, so this is lossless.

const HALF_MAGIC: u32 = 0x3631_5041;

pub fn save_half(buf: &FloatImage, path: &str) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(12 + buf.data.len() * 2);
    out.extend_from_slice(&HALF_MAGIC.to_le_bytes());
    out.extend_from_slice(&(buf.width as i32).to_le_bytes());
    out.extend_from_slice(&(buf.height as i32).to_le_bytes());
    let samples: Vec<u8> = buf
        .data
        .par_iter()
        .flat_map_iter(|&v| {
            let s: u16 = if v <= 0.0 { 0 } else if v >= 1.0 { 65535 } else { (v * 65535.0 + 0.5) as u16 };
            s.to_le_bytes()
        })
        .collect();
    out.extend_from_slice(&samples);
    crate::paths::write_atomic(path, &out)
}

pub fn load_half(path: &str) -> Option<FloatImage> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 12 || u32::from_le_bytes(data[0..4].try_into().ok()?) != HALF_MAGIC {
        return None;
    }
    let w = i32::from_le_bytes(data[4..8].try_into().ok()?);
    let h = i32::from_le_bytes(data[8..12].try_into().ok()?);
    if w <= 0 || h <= 0 || (w as i64) * (h as i64) > 64 * 1024 * 1024 {
        return None;
    }
    let (w, h) = (w as usize, h as usize);
    let n = w * h * 4;
    let src = data.get(12..12 + n * 2)?;
    let mut buf = FloatImage::new(w, h);
    let inv = 1.0f32 / 65535.0;
    buf.data.par_iter_mut().enumerate().for_each(|(i, d)| {
        *d = u16::from_le_bytes([src[i * 2], src[i * 2 + 1]]) as f32 * inv;
    });
    Some(buf)
}
