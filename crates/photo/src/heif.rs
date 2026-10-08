//! The HEIF container (ISO/IEC 23008-12 on ISO-BMFF), read just far enough for what the
//! decoders do not hand back uniformly: the primary image's size (`ispe`), its
//! orientation (`irot` / `imir`, applied in the order listed), its colour primaries (`colr`
//! nclx, or an ICC profile) and the EXIF block (an `Exif` item). Pure Rust, so the three
//! platforms' system decoders (ImageIO, WIC, libheif) all get the same metadata, and the
//! info panel works even where no decoder is installed.

use awpr_core::pipeline::{rotate_discrete, SourcePrimaries};
use awpr_core::{FloatImage, Rotation};

/// One transformative property, in file order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    /// `irot`: quarter turns anticlockwise.
    Rotate(u8),
    /// `imir`: 0 = mirror about the vertical axis (left ↔ right), 1 = about the horizontal.
    Mirror(u8),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeifInfo {
    /// The coded size (`ispe`), before the transforms.
    pub width: u32,
    pub height: u32,
    pub transforms: Vec<Transform>,
    pub primaries: SourcePrimaries,
    /// The EXIF block as a TIFF stream (`II*\0…`), for `tiff::read_bytes`.
    pub exif_tiff: Option<Vec<u8>>,
}

impl HeifInfo {
    /// The size once the transforms are applied (what the viewer shows).
    pub fn display_size(&self) -> (u32, u32) {
        let turns: u32 = self.transforms.iter().map(|t| if let Transform::Rotate(n) = t { *n as u32 } else { 0 }).sum();
        if turns % 2 == 1 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

pub fn is_heif_path(path: &str) -> bool {
    matches!(crate::paths::ext(path).as_str(), "heic" | "heif" | "hif")
}

fn be16(d: &[u8], o: usize) -> Option<u32> {
    Some(u16::from_be_bytes(d.get(o..o + 2)?.try_into().ok()?) as u32)
}

fn be32(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

fn be_n(d: &[u8], o: usize, n: usize) -> Option<u64> {
    let b = d.get(o..o + n)?;
    Some(b.iter().fold(0u64, |a, &x| (a << 8) | x as u64))
}

/// The boxes directly inside `d`: (type, payload).
fn boxes(d: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut o = 0usize;
    while o + 8 <= d.len() {
        let Some(size32) = be32(d, o) else { break };
        let kind: [u8; 4] = d[o + 4..o + 8].try_into().unwrap();
        let (header, size) = match size32 {
            1 => match be_n(d, o + 8, 8) {
                Some(s) => (16usize, s as usize),
                None => break,
            },
            0 => (8, d.len() - o),
            s => (8, s as usize),
        };
        if size < header || o + size > d.len() {
            break;
        }
        out.push((kind, &d[o + header..o + size]));
        o += size;
    }
    out
}

fn child<'a>(d: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(d).into_iter().find(|(k, _)| k == kind).map(|(_, p)| p)
}

/// The item that holds an ICC profile describes Display P3?
fn icc_is_display_p3(icc: &[u8]) -> bool {
    let ascii = icc.windows(10).any(|w| w == b"Display P3");
    let utf16: Vec<u8> = "Display P3".encode_utf16().flat_map(|c| c.to_be_bytes()).collect();
    ascii || icc.windows(utf16.len()).any(|w| w == utf16.as_slice())
}

struct Extent {
    offset: u64,
    length: u64,
}

/// Parse a whole HEIF file in memory.
pub fn parse(file: &[u8]) -> Option<HeifInfo> {
    let top = boxes(file);
    let ftyp = top.iter().find(|(k, _)| k == b"ftyp")?.1;
    let brands: Vec<&[u8]> = std::iter::once(ftyp.get(0..4)?).chain(ftyp.get(8..)?.chunks(4)).collect();
    if !brands.iter().any(|b| matches!(*b, b"heic" | b"heix" | b"mif1" | b"msf1" | b"heim" | b"heis" | b"hevc" | b"hevx")) {
        return None;
    }
    let meta = top.iter().find(|(k, _)| k == b"meta")?.1.get(4..)?; // FullBox
    let mut info = HeifInfo::default();

    // Primary item.
    let pitm = child(meta, b"pitm")?;
    let primary = if pitm[0] == 0 { be16(pitm, 4)? } else { be32(pitm, 4)? };

    // Item types: find the Exif item(s).
    let mut exif_items = Vec::new();
    if let Some(iinf) = child(meta, b"iinf") {
        let start = if iinf[0] == 0 { 4 + 2 } else { 4 + 4 };
        for (k, infe) in boxes(iinf.get(start..)?) {
            if &k != b"infe" || infe.len() < 4 {
                continue;
            }
            let v = infe[0];
            if v < 2 {
                continue;
            }
            let (id, o) = if v == 2 { (be16(infe, 4)?, 6) } else { (be32(infe, 4)?, 8) };
            let item_type = infe.get(o + 2..o + 6)?;
            if item_type == b"Exif" {
                exif_items.push(id);
            }
        }
    }

    // Item locations.
    let mut locations: Vec<(u32, u8, Vec<Extent>)> = Vec::new();
    if let Some(iloc) = child(meta, b"iloc") {
        let v = iloc[0];
        let mut o = 4;
        let sizes = *iloc.get(o)?;
        let sizes2 = *iloc.get(o + 1)?;
        let (off_sz, len_sz, base_sz) = ((sizes >> 4) as usize, (sizes & 15) as usize, (sizes2 >> 4) as usize);
        let idx_sz = if v >= 1 { (sizes2 & 15) as usize } else { 0 };
        o += 2;
        let count = if v < 2 {
            o += 2;
            be16(iloc, o - 2)?
        } else {
            o += 4;
            be32(iloc, o - 4)?
        };
        for _ in 0..count {
            let id = if v < 2 {
                o += 2;
                be16(iloc, o - 2)?
            } else {
                o += 4;
                be32(iloc, o - 4)?
            };
            let method = if v >= 1 {
                o += 2;
                (be16(iloc, o - 2)? & 15) as u8
            } else {
                0
            };
            o += 2; // data_reference_index
            let base = be_n(iloc, o, base_sz)?;
            o += base_sz;
            let n = be16(iloc, o)?;
            o += 2;
            let mut ext = Vec::new();
            for _ in 0..n {
                o += idx_sz;
                let off = be_n(iloc, o, off_sz)?;
                o += off_sz;
                let len = be_n(iloc, o, len_sz)?;
                o += len_sz;
                ext.push(Extent { offset: base + off, length: len });
            }
            locations.push((id, method, ext));
        }
    }
    let idat = child(meta, b"idat");
    let item_data = |id: u32| -> Option<Vec<u8>> {
        let (_, method, ext) = locations.iter().find(|(i, _, _)| *i == id)?;
        let src: &[u8] = match method {
            0 => file,
            1 => idat?,
            _ => return None,
        };
        let mut out = Vec::new();
        for e in ext {
            let s = e.offset as usize;
            let len = if e.length == 0 { src.len().saturating_sub(s) } else { e.length as usize };
            out.extend_from_slice(src.get(s..s.checked_add(len)?)?);
        }
        Some(out)
    };
    for id in exif_items {
        if let Some(d) = item_data(id) {
            // 4-byte offset to the TIFF header, then (usually "Exif\0\0" and) the TIFF.
            let skip = be32(&d, 0)? as usize;
            if let Some(t) = d.get(4 + skip..) {
                let t = if t.starts_with(b"Exif\0\0") { &t[6..] } else { t };
                if t.starts_with(b"II*\0") || t.starts_with(b"MM\0*") {
                    info.exif_tiff = Some(t.to_vec());
                    break;
                }
            }
        }
    }

    // Properties of the primary item, in association order.
    let iprp = child(meta, b"iprp")?;
    let ipco: Vec<([u8; 4], &[u8])> = boxes(child(iprp, b"ipco")?);
    let ipma = child(iprp, b"ipma")?;
    let (v, flags) = (ipma[0], be32(ipma, 0)? & 0xFF_FFFF);
    let mut o = 4;
    let entries = be32(ipma, o)?;
    o += 4;
    let mut assoc = Vec::new();
    for _ in 0..entries {
        let id = if v < 1 {
            o += 2;
            be16(ipma, o - 2)?
        } else {
            o += 4;
            be32(ipma, o - 4)?
        };
        let n = *ipma.get(o)? as usize;
        o += 1;
        for _ in 0..n {
            let idx = if flags & 1 != 0 {
                o += 2;
                be16(ipma, o - 2)? & 0x7FFF
            } else {
                o += 1;
                (*ipma.get(o - 1)? & 0x7F) as u32
            };
            if id == primary {
                assoc.push(idx as usize);
            }
        }
    }
    for idx in assoc {
        let Some((kind, p)) = idx.checked_sub(1).and_then(|i| ipco.get(i)) else { continue };
        match kind {
            b"ispe" => {
                info.width = be32(p, 4)?;
                info.height = be32(p, 8)?;
            }
            b"irot" => info.transforms.push(Transform::Rotate(p.first()? & 3)),
            b"imir" => info.transforms.push(Transform::Mirror(p.first()? & 1)),
            b"colr" => match p.get(0..4)? {
                b"nclx" => {
                    // 12 = SMPTE EG 432-1 (Display P3), 11 = DCI-P3 (treated as P3 too).
                    if matches!(be16(p, 4)?, 11 | 12) {
                        info.primaries = SourcePrimaries::DisplayP3;
                    }
                }
                b"prof" | b"rICC" => {
                    if icc_is_display_p3(&p[4..]) {
                        info.primaries = SourcePrimaries::DisplayP3;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    Some(info)
}

pub fn read(path: &str) -> Option<HeifInfo> {
    parse(&std::fs::read(path).ok()?)
}

/// Apply the container's transforms to a decoder's unrotated output.
pub fn apply_transforms(mut img: FloatImage, transforms: &[Transform]) -> FloatImage {
    for t in transforms {
        img = match *t {
            Transform::Rotate(0) => img,
            Transform::Rotate(1) => rotate_discrete(&img, Rotation::R270), // 90° anticlockwise
            Transform::Rotate(2) => rotate_discrete(&img, Rotation::R180),
            Transform::Rotate(_) => rotate_discrete(&img, Rotation::R90),
            Transform::Mirror(axis) => mirror(&img, axis),
        };
    }
    img
}

fn mirror(src: &FloatImage, axis: u8) -> FloatImage {
    let (w, h) = (src.width, src.height);
    let mut out = FloatImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = if axis == 0 { (w - 1 - x, y) } else { (x, h - 1 - y) };
            let (d, s) = (out.index(x, y), src.index(sx, sy));
            out.data[d..d + 4].copy_from_slice(&src.data[s..s + 4]);
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn bx(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(payload);
        v
    }

    fn full(kind: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
        let mut p = vec![version, (flags >> 16) as u8, (flags >> 8) as u8, flags as u8];
        p.extend_from_slice(payload);
        bx(kind, &p)
    }

    /// A minimal HEIF: item 1 = the image (ispe, irot 1, colr nclx P3), item 2 = Exif.
    pub(crate) fn synthetic(tiff: &[u8], irot: Option<u8>, primaries: u16) -> Vec<u8> {
        let ftyp = bx(b"ftyp", b"heic\0\0\0\0mif1heic");
        let pitm = full(b"pitm", 0, 0, &1u16.to_be_bytes());
        let infe = |id: u16, ty: &[u8; 4]| {
            let mut p = id.to_be_bytes().to_vec();
            p.extend_from_slice(&0u16.to_be_bytes());
            p.extend_from_slice(ty);
            p.push(0);
            full(b"infe", 2, 0, &p)
        };
        let mut iinf_p = 2u16.to_be_bytes().to_vec();
        iinf_p.extend(infe(1, b"hvc1"));
        iinf_p.extend(infe(2, b"Exif"));
        let iinf = full(b"iinf", 0, 0, &iinf_p);
        let mut ispe_p = 0u32.to_be_bytes().to_vec();
        ispe_p.extend_from_slice(&600u32.to_be_bytes());
        ispe_p.extend_from_slice(&400u32.to_be_bytes());
        let ispe = bx(b"ispe", &ispe_p);
        let mut colr_p = b"nclx".to_vec();
        colr_p.extend_from_slice(&primaries.to_be_bytes());
        colr_p.extend_from_slice(&13u16.to_be_bytes());
        colr_p.extend_from_slice(&6u16.to_be_bytes());
        colr_p.push(0x80);
        let colr = bx(b"colr", &colr_p);
        let mut ipco_p = ispe.clone();
        ipco_p.extend(colr);
        let mut assoc = vec![1u8, 2];
        if let Some(r) = irot {
            ipco_p.extend(bx(b"irot", &[r]));
            assoc.push(3);
        }
        let ipco = bx(b"ipco", &ipco_p);
        let mut ipma_p = 1u32.to_be_bytes().to_vec();
        ipma_p.extend_from_slice(&1u16.to_be_bytes());
        ipma_p.push(assoc.len() as u8);
        ipma_p.extend(assoc.iter().map(|a| 0x80 | a));
        let ipma = full(b"ipma", 0, 0, &ipma_p);
        let mut iprp_p = ipco;
        iprp_p.extend(ipma);
        let iprp = bx(b"iprp", &iprp_p);
        // Exif payload: offset 6 ("Exif\0\0") then the TIFF; placed in mdat after meta.
        let mut exif = 6u32.to_be_bytes().to_vec();
        exif.extend_from_slice(b"Exif\0\0");
        exif.extend_from_slice(tiff);
        let build = |exif_off: u32| {
            let mut iloc_p = vec![0x44, 0x00];
            iloc_p.extend_from_slice(&1u16.to_be_bytes());
            iloc_p.extend_from_slice(&2u16.to_be_bytes()); // item 2
            iloc_p.extend_from_slice(&0u16.to_be_bytes()); // data ref
            iloc_p.extend_from_slice(&1u16.to_be_bytes()); // extents
            iloc_p.extend_from_slice(&exif_off.to_be_bytes());
            iloc_p.extend_from_slice(&(exif.len() as u32).to_be_bytes());
            let iloc = full(b"iloc", 0, 0, &iloc_p);
            let mut meta_p = Vec::new();
            for b in [&full(b"hdlr", 0, 0, b"\0\0\0\0pict\0\0\0\0\0\0\0\0\0\0\0\0\0"), &pitm, &iloc, &iinf, &iprp] {
                meta_p.extend_from_slice(b);
            }
            let meta = full(b"meta", 0, 0, &meta_p);
            let mut file = ftyp.clone();
            file.extend(meta);
            file
        };
        let head = build(0);
        let off = (head.len() + 8) as u32;
        let mut file = build(off);
        file.extend(bx(b"mdat", &exif));
        file
    }

    fn tiff_with(make: &str, orientation: u16) -> Vec<u8> {
        // II*, IFD at 8: Make (ASCII), Orientation (SHORT).
        let mut t = b"II*\0".to_vec();
        t.extend_from_slice(&8u32.to_le_bytes());
        t.extend_from_slice(&2u16.to_le_bytes());
        let make_off = 8 + 2 + 2 * 12 + 4;
        let mk = format!("{make}\0");
        t.extend_from_slice(&0x010Fu16.to_le_bytes());
        t.extend_from_slice(&2u16.to_le_bytes());
        t.extend_from_slice(&(mk.len() as u32).to_le_bytes());
        t.extend_from_slice(&(make_off as u32).to_le_bytes());
        t.extend_from_slice(&0x0112u16.to_le_bytes());
        t.extend_from_slice(&3u16.to_le_bytes());
        t.extend_from_slice(&1u32.to_le_bytes());
        t.extend_from_slice(&(orientation as u32).to_le_bytes());
        t.extend_from_slice(&0u32.to_le_bytes());
        t.extend_from_slice(mk.as_bytes());
        t
    }

    #[test]
    fn reads_size_rotation_primaries_and_exif() {
        let file = synthetic(&tiff_with("Apple", 6), Some(3), 12);
        let info = parse(&file).expect("parses");
        assert_eq!((info.width, info.height), (600, 400));
        assert_eq!(info.transforms, vec![Transform::Rotate(3)]);
        assert_eq!(info.display_size(), (400, 600));
        assert_eq!(info.primaries, SourcePrimaries::DisplayP3);
        let t = crate::tiff::read_bytes(info.exif_tiff.as_ref().unwrap()).unwrap();
        assert_eq!(t.make.as_deref(), Some("Apple"));
        assert_eq!(t.orientation, Some(6));
        let plain = parse(&synthetic(&tiff_with("X", 1), None, 1)).unwrap();
        assert!(plain.transforms.is_empty() && plain.primaries == SourcePrimaries::Srgb);
        assert!(parse(b"not a heif file at all, sorry").is_none());
    }

    #[test]
    fn transforms_turn_the_right_way() {
        // 2 × 1: left red, right green.
        let mut img = FloatImage::new(2, 1);
        img.data.copy_from_slice(&[1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0]);
        // irot 3 = 270° anticlockwise = 90° clockwise: red ends on top.
        let r = apply_transforms(img.clone(), &[Transform::Rotate(3)]);
        assert_eq!((r.width, r.height), (1, 2));
        assert_eq!(&r.data[0..3], &[1.0, 0.0, 0.0]);
        // irot 1 = 90° anticlockwise: green on top.
        let l = apply_transforms(img.clone(), &[Transform::Rotate(1)]);
        assert_eq!(&l.data[0..3], &[0.0, 1.0, 0.0]);
        // imir axis 0: left ↔ right.
        let m = apply_transforms(img, &[Transform::Mirror(0)]);
        assert_eq!(&m.data[0..3], &[0.0, 1.0, 0.0]);
    }
}
