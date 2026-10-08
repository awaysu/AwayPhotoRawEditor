//! A small TIFF/EXIF tag reader for the fields the info panel shows and the RAW visible
//! size. One reader for every container, so the three platforms agree:
//!
//! * TIFF-based RAWs (ARW, NEF, DNG, CR2, PEF, SRW…) and RW2 / ORF (TIFF with their own
//!   magic numbers), walking IFD0, its SubIFDs and the EXIF IFD;
//! * CR3: the TIFF blocks in the `CMT1` (IFD0) and `CMT2` (EXIF) boxes;
//! * RAF and JPEG: the EXIF block of the (embedded) JPEG.
//!
//! The Windows build gets the same values from ExifTool, the Swift build from ImageIO.

use std::collections::HashMap;
use std::io::Read;

/// Tags collected across all the IFDs walked.
#[derive(Debug, Default, Clone)]
pub struct TiffMeta {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens_model: Option<String>,
    pub lens_make: Option<String>,
    pub orientation: Option<u32>,
    pub exposure_time: Option<f64>,
    pub f_number: Option<f64>,
    pub iso: Option<u32>,
    pub date_original: Option<String>,
    pub date_digitized: Option<String>,
    pub date_time: Option<String>,
    pub exposure_bias: Option<f64>,
    pub metering_mode: Option<u32>,
    pub focal_length: Option<f64>,
    pub white_balance: Option<u32>,
    pub pixel_x: Option<u32>,
    pub pixel_y: Option<u32>,
    /// DNG `DefaultCropSize` (also written by Sony ARW in the raw SubIFD) — the visible
    /// frame of the raw data.
    pub default_crop: Option<(u32, u32)>,
}

impl TiffMeta {
    /// The visible frame size the native builds trim LibRaw's output to: the raw IFD's
    /// DefaultCropSize, else the EXIF pixel dimensions.
    pub fn visible_size(&self) -> Option<(usize, usize)> {
        if let Some((w, h)) = self.default_crop.filter(|&(w, h)| w > 0 && h > 0) {
            return Some((w as usize, h as usize));
        }
        match (self.pixel_x, self.pixel_y) {
            (Some(w), Some(h)) if w > 0 && h > 0 => Some((w as usize, h as usize)),
            _ => None,
        }
    }
}

/// Read up to this much of the file: every container here keeps its metadata near the
/// front (Sony's raw SubIFD sits ~150 KB in, CR3's CMT boxes in the first few KB).
const HEAD_BYTES: u64 = 4 * 1024 * 1024;

pub fn read(path: &str) -> Option<TiffMeta> {
    let mut head = Vec::new();
    std::fs::File::open(path).ok()?.take(HEAD_BYTES).read_to_end(&mut head).ok()?;
    read_bytes(&head)
}

pub fn read_bytes(data: &[u8]) -> Option<TiffMeta> {
    let mut m = TiffMeta::default();
    if data.len() < 16 {
        return None;
    }
    if is_tiff(data) {
        walk_tiff(data, &mut m);
        return Some(m);
    }
    if data.starts_with(&[0xFF, 0xD8]) {
        return jpeg_exif(data).map(|t| {
            walk_tiff(t, &mut m);
            m
        });
    }
    if data.starts_with(b"FUJIFILM") && data.len() > 92 {
        // RAF: big-endian offset/length of the embedded JPEG at byte 84.
        let off = u32::from_be_bytes(data[84..88].try_into().ok()?) as usize;
        let len = u32::from_be_bytes(data[88..92].try_into().ok()?) as usize;
        let jpeg = data.get(off..off.checked_add(len)?.min(data.len()))?;
        let t = jpeg_exif(jpeg)?;
        walk_tiff(t, &mut m);
        return Some(m);
    }
    if &data[4..8] == b"ftyp" {
        // CR3 / ISO-BMFF: CMT1 = IFD0, CMT2 = EXIF IFD, each a complete TIFF block.
        let mut found = false;
        for name in [b"CMT1", b"CMT2"] {
            if let Some(t) = find_box(data, name) {
                if name == b"CMT2" {
                    walk_ifd_chain(t, read_u32(t, 4, t[0] == b'I').unwrap_or(0) as usize, &mut m, Ifd::Exif);
                } else {
                    walk_tiff(t, &mut m);
                }
                found = true;
            }
        }
        return found.then_some(m);
    }
    None
}

fn is_tiff(d: &[u8]) -> bool {
    // Standard TIFF, Panasonic RW2 ("IIU\0") and Olympus ORF ("IIRO" / "IIRS" / "MMOR").
    matches!(&d[..4], b"II*\0" | b"MM\0*" | b"IIU\0" | b"IIRO" | b"IIRS" | b"MMOR")
}

/// The TIFF block inside a JPEG's `Exif\0\0` APP1 segment.
fn jpeg_exif(d: &[u8]) -> Option<&[u8]> {
    let mut i = 2;
    while i + 4 <= d.len() {
        if d[i] != 0xFF {
            return None;
        }
        let marker = d[i + 1];
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
        let seg = d.get(i + 4..i + 2 + len)?;
        if marker == 0xE1 && seg.starts_with(b"Exif\0\0") {
            return Some(&seg[6..]);
        }
        i += 2 + len;
    }
    None
}

/// Depth-first search for a box by type (CR3 nests CMT* inside moov/uuid).
fn find_box<'a>(d: &'a [u8], name: &[u8; 4]) -> Option<&'a [u8]> {
    fn search<'a>(d: &'a [u8], name: &[u8; 4], depth: u32) -> Option<&'a [u8]> {
        let mut i = 0;
        while i + 8 <= d.len() {
            let size = u32::from_be_bytes(d[i..i + 4].try_into().ok()?) as usize;
            let ty = &d[i + 4..i + 8];
            let (hdr, size) = if size == 1 {
                (16, u64::from_be_bytes(d.get(i + 8..i + 16)?.try_into().ok()?) as usize)
            } else if size == 0 {
                (8, d.len() - i)
            } else {
                (8, size)
            };
            if size < hdr {
                return None;
            }
            let body = d.get(i + hdr..(i + size).min(d.len()))?;
            if ty == name {
                return Some(body);
            }
            if depth < 4 && (ty == b"moov" || ty == b"uuid") {
                // Canon's uuid box carries a 16-byte UUID before its children.
                let inner = if ty == b"uuid" { body.get(16..).unwrap_or(&[]) } else { body };
                if let Some(r) = search(inner, name, depth + 1) {
                    return Some(r);
                }
            }
            i += size;
        }
        None
    }
    search(d, name, 0)
}

#[derive(Clone, Copy, PartialEq)]
enum Ifd {
    Main,
    Exif,
}

fn read_u16(d: &[u8], o: usize, le: bool) -> Option<u16> {
    let b: [u8; 2] = d.get(o..o + 2)?.try_into().ok()?;
    Some(if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
}

fn read_u32(d: &[u8], o: usize, le: bool) -> Option<u32> {
    let b: [u8; 4] = d.get(o..o + 4)?.try_into().ok()?;
    Some(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
}

fn walk_tiff(d: &[u8], m: &mut TiffMeta) {
    if d.len() < 8 {
        return;
    }
    let le = d[0] == b'I';
    if let Some(first) = read_u32(d, 4, le) {
        walk_ifd_chain(d, first as usize, m, Ifd::Main);
    }
}

/// One tag's value(s), decoded enough for the fields read here.
enum Val {
    Ints(Vec<u32>),
    Rationals(Vec<f64>),
    Text(String),
    Other,
}

fn walk_ifd_chain(d: &[u8], mut off: usize, m: &mut TiffMeta, kind: Ifd) {
    let le = d.first() == Some(&b'I');
    let mut visited: HashMap<usize, ()> = HashMap::new();
    let mut count = 0;
    while off != 0 && off + 2 <= d.len() && visited.insert(off, ()).is_none() && count < 8 {
        count += 1;
        let Some(n) = read_u16(d, off, le) else { return };
        let n = n as usize;
        for i in 0..n {
            let e = off + 2 + 12 * i;
            let (Some(tag), Some(ty), Some(cnt)) = (read_u16(d, e, le), read_u16(d, e + 2, le), read_u32(d, e + 4, le)) else {
                return;
            };
            if tag == 0x002E && ty == 7 && kind == Ifd::Main {
                // Panasonic RW2 `JpgFromRaw`: the camera's full EXIF (lens, white
                // balance, output size) lives in this embedded JPEG, not in the RW2 IFDs.
                let at = read_u32(d, e + 8, le).unwrap_or(u32::MAX) as usize;
                if let Some(jpeg) = at.checked_add(cnt as usize).and_then(|end| d.get(at..end.min(d.len()))) {
                    if let Some(t) = jpeg_exif(jpeg) {
                        // Its pixel dimensions are the preview's, not the photo's.
                        let (px, py) = (m.pixel_x, m.pixel_y);
                        walk_tiff(t, m);
                        (m.pixel_x, m.pixel_y) = (px, py);
                    }
                }
                continue;
            }
            let v = value(d, e, ty, cnt as usize, le);
            match (kind, tag) {
                (_, 0x014A) | (_, 0x8769) => {
                    // SubIFDs / EXIF IFD pointers.
                    if let Val::Ints(offs) = v {
                        for o in offs.into_iter().take(4) {
                            walk_ifd_chain(d, o as usize, m, if tag == 0x8769 { Ifd::Exif } else { Ifd::Main });
                        }
                    }
                }
                _ => apply(m, tag, v),
            }
        }
        off = read_u32(d, off + 2 + 12 * n, le).unwrap_or(0) as usize;
    }
}

fn value(d: &[u8], entry: usize, ty: u16, cnt: usize, le: bool) -> Val {
    let unit = match ty {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 => 8,
        _ => return Val::Other,
    };
    let size = unit * cnt;
    let at = if size <= 4 { entry + 8 } else { read_u32(d, entry + 8, le).unwrap_or(u32::MAX) as usize };
    let Some(raw) = at.checked_add(size).and_then(|end| d.get(at..end)) else { return Val::Other };
    match ty {
        2 => Val::Text(String::from_utf8_lossy(raw.split(|&b| b == 0).next().unwrap_or(&[])).trim().to_string()),
        3 => Val::Ints((0..cnt.min(16)).filter_map(|i| read_u16(raw, i * 2, le).map(u32::from)).collect()),
        4 | 13 => Val::Ints((0..cnt.min(16)).filter_map(|i| read_u32(raw, i * 4, le)).collect()),
        5 | 10 => Val::Rationals(
            (0..cnt.min(4))
                .filter_map(|i| {
                    let (n, dd) = (read_u32(raw, i * 8, le)?, read_u32(raw, i * 8 + 4, le)?);
                    let (n, dd) = if ty == 10 { (n as i32 as f64, dd as i32 as f64) } else { (n as f64, dd as f64) };
                    (dd != 0.0).then(|| n / dd)
                })
                .collect(),
        ),
        _ => Val::Other,
    }
}

fn apply(m: &mut TiffMeta, tag: u16, v: Val) {
    let text = |v: &Val| match v {
        Val::Text(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    };
    let int = |v: &Val| match v {
        Val::Ints(x) => x.first().copied(),
        _ => None,
    };
    let rat = |v: &Val| match v {
        Val::Rationals(x) => x.first().copied(),
        Val::Ints(x) => x.first().map(|&i| i as f64),
        _ => None,
    };
    // First writer wins: IFD0 comes before the SubIFDs and embedded previews.
    fn set<T>(slot: &mut Option<T>, v: Option<T>) {
        if slot.is_none() {
            *slot = v;
        }
    }
    match tag {
        0x010F => set(&mut m.make, text(&v)),
        0x0110 => set(&mut m.model, text(&v)),
        0x0112 => set(&mut m.orientation, int(&v)),
        0x0132 => set(&mut m.date_time, text(&v)),
        0x829A => set(&mut m.exposure_time, rat(&v)),
        0x829D => set(&mut m.f_number, rat(&v)),
        0x8827 => set(&mut m.iso, int(&v)),
        0x9003 => set(&mut m.date_original, text(&v)),
        0x9004 => set(&mut m.date_digitized, text(&v)),
        0x9204 => set(&mut m.exposure_bias, rat(&v)),
        0x9207 => set(&mut m.metering_mode, int(&v)),
        0x920A => set(&mut m.focal_length, rat(&v)),
        0xA002 => set(&mut m.pixel_x, int(&v)),
        0xA003 => set(&mut m.pixel_y, int(&v)),
        0xA403 => set(&mut m.white_balance, int(&v)),
        0xA433 => set(&mut m.lens_make, text(&v)),
        0xA434 => set(&mut m.lens_model, text(&v)),
        0xC620 => {
            let wh = match &v {
                Val::Ints(x) if x.len() >= 2 => Some((x[0], x[1])),
                Val::Rationals(x) if x.len() >= 2 => Some((x[0].round() as u32, x[1].round() as u32)),
                _ => None,
            };
            // Keep the largest: a DNG may also crop its previews.
            if let Some((w, h)) = wh {
                if m.default_crop.is_none_or(|(ow, oh)| w as u64 * h as u64 > ow as u64 * oh as u64) {
                    m.default_crop = Some((w, h));
                }
            }
        }
        _ => {}
    }
}
