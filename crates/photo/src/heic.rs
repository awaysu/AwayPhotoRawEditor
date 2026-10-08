//! HEIC / HEIF decoding through the operating system (docs/HEIC-EVAL.md): ImageIO on
//! macOS, WIC on Windows (needs Microsoft's HEIF + HEVC extensions), and on Linux the
//! system's libheif, loaded at run time so a machine without it still starts. Nothing of
//! libheif / libde265 is compiled into the program.
//!
//! Every decoder returns the primary image's pixels as stored (gamma-encoded, in the
//! file's own primaries — Display P3 stays P3, see `heif::HeifInfo::primaries`), 16-bit
//! when the file is deeper than 8 bits. Orientation comes from the container's
//! `irot` / `imir`, applied exactly once: libheif applies them itself, ImageIO and WIC
//! hand back the coded image, which `heif::apply_transforms` then turns. The EXIF
//! Orientation tag is never applied to a HEIF — iPhones write it *and* `irot`, and both
//! describe the same turn.

use crate::heif;
use awpr_core::FloatImage;

#[derive(Debug, Clone, PartialEq)]
pub enum HeicError {
    /// No decoder on this machine; the text says what to install.
    Unavailable(String),
    /// A decoder exists but this file did not decode.
    Failed(String),
}

impl std::fmt::Display for HeicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(s) | Self::Failed(s) => f.write_str(s),
        }
    }
}

/// What a platform decoder gives back.
pub(crate) struct Raw {
    pub image: FloatImage,
    /// The container's transforms are already applied.
    pub oriented: bool,
}

/// The decoded, upright primary image.
pub fn decode(path: &str) -> Result<FloatImage, HeicError> {
    let info = heif::read(path);
    let raw = platform::decode(path)?;
    let transforms = info.as_ref().map(|i| i.transforms.clone()).unwrap_or_default();
    if raw.oriented || transforms.is_empty() {
        return Ok(raw.image);
    }
    // WIC: a quarter turn shows in the size, so a decoder that already turned it is
    // caught here even if it says otherwise.
    if let Some(i) = &info {
        let (w, h) = (raw.image.width as u32, raw.image.height as u32);
        if (w, h) == i.display_size() && (w, h) != (i.width, i.height) {
            return Ok(raw.image);
        }
    }
    Ok(heif::apply_transforms(raw.image, &transforms))
}

/// Whether this machine can decode HEIC at all (Err: why not, for the info panel).
pub fn availability() -> Result<(), String> {
    platform::availability()
}

fn to_float_u16(w: usize, h: usize, src: &[u16], stride: usize, channels: usize, max: f32) -> FloatImage {
    let mut out = FloatImage::new(w, h);
    let inv = 1.0 / max;
    for y in 0..h {
        let row = &src[y * stride..];
        for x in 0..w {
            let o = out.index(x, y);
            for c in 0..3 {
                out.data[o + c] = row[x * channels + c] as f32 * inv;
            }
            out.data[o + 3] = 1.0;
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn to_float_u8(w: usize, h: usize, src: &[u8], stride: usize, channels: usize) -> FloatImage {
    let mut out = FloatImage::new(w, h);
    for y in 0..h {
        let row = &src[y * stride..];
        for x in 0..w {
            let o = out.index(x, y);
            for c in 0..3 {
                out.data[o + c] = row[x * channels + c] as f32 / 255.0;
            }
            out.data[o + 3] = 1.0;
        }
    }
    out
}

// ---- Linux: libheif, dlopen'ed -------------------------------------------------------

#[cfg(target_os = "linux")]
mod platform {
    use super::{to_float_u16, to_float_u8, HeicError, Raw};
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::sync::OnceLock;

    #[repr(C)]
    struct HeifError {
        code: c_int,
        subcode: c_int,
        message: *const c_char,
    }

    // libheif's enum values (heif_image.h; stable since 1.x).
    const COLORSPACE_RGB: c_int = 1;
    const CHROMA_INTERLEAVED_RGB: c_int = 10;
    const CHROMA_INTERLEAVED_RRGGBB_LE: c_int = 14;
    const CHANNEL_INTERLEAVED: c_int = 10;

    struct Api {
        _lib: libloading::Library,
        context_alloc: unsafe extern "C" fn() -> *mut c_void,
        context_free: unsafe extern "C" fn(*mut c_void),
        read_from_file: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_void) -> HeifError,
        primary_handle: unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> HeifError,
        handle_release: unsafe extern "C" fn(*mut c_void),
        luma_bits: unsafe extern "C" fn(*const c_void) -> c_int,
        decode_image: unsafe extern "C" fn(*const c_void, *mut *mut c_void, c_int, c_int, *const c_void) -> HeifError,
        plane: unsafe extern "C" fn(*const c_void, c_int, *mut c_int) -> *const u8,
        width: unsafe extern "C" fn(*const c_void, c_int) -> c_int,
        height: unsafe extern "C" fn(*const c_void, c_int) -> c_int,
        image_release: unsafe extern "C" fn(*mut c_void),
    }

    fn api() -> Result<&'static Api, String> {
        static API: OnceLock<Result<Api, String>> = OnceLock::new();
        API.get_or_init(|| unsafe {
            let lib = ["libheif.so.1", "libheif.so"]
                .iter()
                .find_map(|n| libloading::Library::new(n).ok())
                .ok_or_else(|| "libheif is not installed (Debian / Ubuntu: libheif1 + libheif-plugin-libde265; Fedora: libheif + libheif-freeworld)".to_string())?;
            macro_rules! sym {
                ($n:literal) => {
                    *lib.get(concat!($n, "\0").as_bytes()).map_err(|e| format!("libheif: {}: {e}", $n))?
                };
            }
            // Plugins (the HEVC decoder is one on most distributions) load in heif_init.
            if let Ok(init) = lib.get::<unsafe extern "C" fn(*const c_void) -> HeifError>(b"heif_init\0") {
                let _ = init(std::ptr::null());
            }
            Ok(Api {
                context_alloc: sym!("heif_context_alloc"),
                context_free: sym!("heif_context_free"),
                read_from_file: sym!("heif_context_read_from_file"),
                primary_handle: sym!("heif_context_get_primary_image_handle"),
                handle_release: sym!("heif_image_handle_release"),
                luma_bits: sym!("heif_image_handle_get_luma_bits_per_pixel"),
                decode_image: sym!("heif_decode_image"),
                plane: sym!("heif_image_get_plane_readonly"),
                width: sym!("heif_image_get_width"),
                height: sym!("heif_image_get_height"),
                image_release: sym!("heif_image_release"),
                _lib: lib,
            })
        })
        .as_ref()
        .map_err(Clone::clone)
    }

    fn check(e: HeifError) -> Result<(), String> {
        if e.code == 0 {
            return Ok(());
        }
        let msg = if e.message.is_null() { String::new() } else { unsafe { CStr::from_ptr(e.message) }.to_string_lossy().into_owned() };
        Err(format!("libheif: {msg} ({}/{})", e.code, e.subcode))
    }

    pub fn availability() -> Result<(), String> {
        api().map(|_| ())
    }

    pub fn decode(path: &str) -> Result<Raw, HeicError> {
        let a = api().map_err(HeicError::Unavailable)?;
        let cpath = CString::new(path).map_err(|e| HeicError::Failed(e.to_string()))?;
        unsafe {
            let ctx = (a.context_alloc)();
            let result = (|| -> Result<Raw, String> {
                check((a.read_from_file)(ctx, cpath.as_ptr(), std::ptr::null()))?;
                let mut handle = std::ptr::null_mut();
                check((a.primary_handle)(ctx, &mut handle))?;
                let deep = (a.luma_bits)(handle) > 8;
                let bits = (a.luma_bits)(handle).max(8);
                let mut img = std::ptr::null_mut();
                let chroma = if deep { CHROMA_INTERLEAVED_RRGGBB_LE } else { CHROMA_INTERLEAVED_RGB };
                let r = check((a.decode_image)(handle, &mut img, COLORSPACE_RGB, chroma, std::ptr::null()));
                (a.handle_release)(handle);
                r?;
                let (w, h) = ((a.width)(img, CHANNEL_INTERLEAVED) as usize, (a.height)(img, CHANNEL_INTERLEAVED) as usize);
                let mut stride: c_int = 0;
                let p = (a.plane)(img, CHANNEL_INTERLEAVED, &mut stride);
                if p.is_null() || w == 0 || h == 0 {
                    (a.image_release)(img);
                    return Err("libheif: no pixels".into());
                }
                let image = if deep {
                    let s = std::slice::from_raw_parts(p as *const u16, stride as usize / 2 * h);
                    to_float_u16(w, h, s, stride as usize / 2, 3, ((1u32 << bits) - 1) as f32)
                } else {
                    let s = std::slice::from_raw_parts(p, stride as usize * h);
                    to_float_u8(w, h, s, stride as usize, 3)
                };
                (a.image_release)(img);
                // libheif applies irot / imir unless told not to.
                Ok(Raw { image, oriented: true })
            })();
            (a.context_free)(ctx);
            result.map_err(HeicError::Failed)
        }
    }
}

// ---- Windows: WIC ------------------------------------------------------------------------

#[cfg(windows)]
mod platform {
    use super::{to_float_u16, HeicError, Raw};
    use windows::core::HSTRING;
    use windows::Win32::Foundation::GENERIC_READ;
    use windows::Win32::Graphics::Imaging::*;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};

    /// WINCODEC_ERR_COMPONENTNOTFOUND (no WIC codec for the file: the HEIF extension is
    /// missing) and MF_E_TOPO_CODEC_NOT_FOUND (the HEIF extension is there but the HEVC
    /// one is not: no transform decodes the content).
    const MISSING_CODES: [i32; 2] = [0x88982F50u32 as i32, 0xC00D5212u32 as i32];

    pub const MISSING: &str = "Windows needs the HEIF Image Extensions and the HEVC Video Extensions (Microsoft Store) to read HEIC";

    fn map(e: windows::core::Error) -> HeicError {
        if MISSING_CODES.contains(&e.code().0) {
            HeicError::Unavailable(MISSING.into())
        } else {
            HeicError::Failed(e.to_string())
        }
    }

    pub fn availability() -> Result<(), String> {
        // Only a decode can tell (the codecs are Store packages, registered per format).
        Ok(())
    }

    pub fn decode(path: &str) -> Result<Raw, HeicError> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(map)?;
            let decoder = factory.CreateDecoderFromFilename(&HSTRING::from(path), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).map_err(map)?;
            let frame = decoder.GetFrame(0).map_err(map)?;
            let (mut w, mut h) = (0u32, 0u32);
            frame.GetSize(&mut w, &mut h).map_err(map)?;
            // 16 bits per channel, gamma-encoded, no colour management (the file's
            // primaries are kept; the pipeline converts them).
            let conv = factory.CreateFormatConverter().map_err(map)?;
            conv.Initialize(&frame, &GUID_WICPixelFormat64bppRGBA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).map_err(map)?;
            let stride = w as usize * 4;
            let mut buf = vec![0u16; stride * h as usize];
            let bytes = std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, buf.len() * 2);
            conv.CopyPixels(std::ptr::null(), (stride * 2) as u32, bytes).map_err(map)?;
            Ok(Raw { image: to_float_u16(w as usize, h as usize, &buf, stride, 4, 65535.0), oriented: false })
        }
    }
}

// ---- macOS: ImageIO ----------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod platform {
    use super::{to_float_u16, HeicError, Raw};
    use std::ffi::c_void;

    type CFTypeRef = *const c_void;

    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    struct CGSize {
        w: f64,
        h: f64,
    }
    #[repr(C)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFURLCreateFromFileSystemRepresentation(alloc: CFTypeRef, buf: *const u8, len: isize, dir: bool) -> CFTypeRef;
        fn CFRelease(cf: CFTypeRef);
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGImageGetWidth(img: CFTypeRef) -> usize;
        fn CGImageGetHeight(img: CFTypeRef) -> usize;
        fn CGImageGetColorSpace(img: CFTypeRef) -> CFTypeRef;
        fn CGColorSpaceGetModel(space: CFTypeRef) -> i32;
        fn CGColorSpaceCreateWithName(name: CFTypeRef) -> CFTypeRef;
        fn CGBitmapContextCreate(data: *mut c_void, w: usize, h: usize, bpc: usize, bpr: usize, space: CFTypeRef, info: u32) -> CFTypeRef;
        fn CGBitmapContextGetData(ctx: CFTypeRef) -> *mut c_void;
        fn CGBitmapContextGetBytesPerRow(ctx: CFTypeRef) -> usize;
        fn CGContextDrawImage(ctx: CFTypeRef, rect: CGRect, img: CFTypeRef);
        static kCGColorSpaceSRGB: CFTypeRef;
    }
    #[link(name = "ImageIO", kind = "framework")]
    extern "C" {
        fn CGImageSourceCreateWithURL(url: CFTypeRef, options: CFTypeRef) -> CFTypeRef;
        fn CGImageSourceGetCount(src: CFTypeRef) -> usize;
        fn CGImageSourceCreateImageAtIndex(src: CFTypeRef, index: usize, options: CFTypeRef) -> CFTypeRef;
    }

    const RGB_MODEL: i32 = 1; // kCGColorSpaceModelRGB
    const PREMULTIPLIED_LAST: u32 = 1; // kCGImageAlphaPremultipliedLast
    const BYTE_ORDER_16_LITTLE: u32 = 1 << 12; // kCGBitmapByteOrder16Little

    pub fn availability() -> Result<(), String> {
        Ok(())
    }

    pub fn decode(path: &str) -> Result<Raw, HeicError> {
        let fail = |s: &str| HeicError::Failed(format!("ImageIO: {s}"));
        unsafe {
            let url = CFURLCreateFromFileSystemRepresentation(std::ptr::null(), path.as_ptr(), path.len() as isize, false);
            if url.is_null() {
                return Err(fail("bad path"));
            }
            let src = CGImageSourceCreateWithURL(url, std::ptr::null());
            CFRelease(url);
            if src.is_null() {
                return Err(fail("cannot open"));
            }
            if CGImageSourceGetCount(src) == 0 {
                CFRelease(src);
                return Err(fail("no image"));
            }
            let img = CGImageSourceCreateImageAtIndex(src, 0, std::ptr::null());
            CFRelease(src);
            if img.is_null() {
                return Err(fail("cannot decode"));
            }
            let (w, h) = (CGImageGetWidth(img), CGImageGetHeight(img));
            // Draw into the image's own colour space (no conversion: P3 stays P3).
            let own = CGImageGetColorSpace(img);
            let (space, owned) = if !own.is_null() && CGColorSpaceGetModel(own) == RGB_MODEL { (own, false) } else { (CGColorSpaceCreateWithName(kCGColorSpaceSRGB), true) };
            let ctx = CGBitmapContextCreate(std::ptr::null_mut(), w, h, 16, 0, space, PREMULTIPLIED_LAST | BYTE_ORDER_16_LITTLE);
            if owned {
                CFRelease(space);
            }
            if ctx.is_null() {
                CFRelease(img);
                return Err(fail("cannot create a bitmap"));
            }
            CGContextDrawImage(ctx, CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: CGSize { w: w as f64, h: h as f64 } }, img);
            CFRelease(img);
            let data = CGBitmapContextGetData(ctx) as *const u16;
            let stride = CGBitmapContextGetBytesPerRow(ctx) / 2;
            let image = to_float_u16(w, h, std::slice::from_raw_parts(data, stride * h), stride, 4, 65535.0);
            CFRelease(ctx);
            // CGImageSourceCreateImageAtIndex gives the coded image; the container's
            // transforms turn it.
            Ok(Raw { image, oriented: false })
        }
    }
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
mod platform {
    use super::{HeicError, Raw};
    pub fn availability() -> Result<(), String> {
        Err("no HEIC decoder on this platform".into())
    }
    pub fn decode(_: &str) -> Result<Raw, HeicError> {
        Err(HeicError::Unavailable("no HEIC decoder on this platform".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heif::Transform;

    /// The regression guard for "orientation applied twice": an iPhone-style file has
    /// EXIF Orientation 6 *and* `irot`; only the container's turn counts.
    #[test]
    fn exif_orientation_is_never_applied_to_heif() {
        let file = crate::heif::tests::synthetic(&[], Some(3), 1);
        let info = heif::parse(&file).unwrap();
        assert_eq!(info.transforms, vec![Transform::Rotate(3)]);
        // A decoder that already turned it (libheif): nothing more happens, whatever
        // EXIF says.
        let upright = FloatImage::new(400, 600);
        let raw = Raw { image: upright, oriented: true };
        assert_eq!((raw.image.width, raw.image.height), (400, 600));
        // A decoder that did not (ImageIO / WIC): exactly the container's turn.
        let coded = FloatImage::new(600, 400);
        let turned = heif::apply_transforms(coded, &info.transforms);
        assert_eq!((turned.width, turned.height), (400, 600));
        // There is no code path from the EXIF orientation to a HEIF's pixels: the loader
        // only applies EXIF orientation inside `codec` for the formats `image` decodes.
        assert!(!crate::codec::applies_exif_orientation("x.heic"));
        assert!(crate::codec::applies_exif_orientation("x.jpg"));
    }
}
