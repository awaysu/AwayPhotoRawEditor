//! Step 11, the watermark (標誌): text drawn straight onto the output pixels, the same
//! on every platform (the C# build used GDI+ `DrawString`, the Swift build Core Text).
//!
//! Fonts come from the system font folders by family name; a glyph the chosen font lacks
//! falls back to the platform's CJK UI font and then to the Ubuntu font egui ships with,
//! so any text renders somewhere.

use ab_glyph::{Font, FontArc, GlyphId, PxScale, ScaleFont};
use awpr_core::buffer::round_half_even;
use awpr_core::FloatImage;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Text colour (order = the export dialog list and the C# enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatermarkColor {
    White,
    Black,
    Blue,
    Yellow,
    Green,
    Red,
    Gray,
    Orange,
}

impl WatermarkColor {
    pub const ALL: [WatermarkColor; 8] = [Self::White, Self::Black, Self::Blue, Self::Yellow, Self::Green, Self::Red, Self::Gray, Self::Orange];

    /// The .NET enum name used in export.xml.
    pub fn xml_name(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Black => "Black",
            Self::Blue => "Blue",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Red => "Red",
            Self::Gray => "Gray",
            Self::Orange => "Orange",
        }
    }

    pub fn from_xml(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.xml_name() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::White => "白色",
            Self::Black => "黑色",
            Self::Blue => "藍色",
            Self::Yellow => "黃色",
            Self::Green => "綠色",
            Self::Red => "紅色",
            Self::Gray => "灰色",
            Self::Orange => "橙色",
        }
    }

    /// `ImageProcessor.ResolveWatermarkColor`.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Self::White => [255, 255, 255],
            Self::Black => [0, 0, 0],
            Self::Blue => [40, 120, 240],
            Self::Yellow => [245, 210, 40],
            Self::Green => [60, 190, 90],
            Self::Red => [230, 50, 50],
            Self::Gray => [150, 150, 150],
            Self::Orange => [245, 150, 40],
        }
    }
}

/// Anchor corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatermarkPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl WatermarkPosition {
    pub const ALL: [WatermarkPosition; 4] = [Self::TopLeft, Self::TopRight, Self::BottomLeft, Self::BottomRight];

    pub fn xml_name(self) -> &'static str {
        match self {
            Self::TopLeft => "TopLeft",
            Self::TopRight => "TopRight",
            Self::BottomLeft => "BottomLeft",
            Self::BottomRight => "BottomRight",
        }
    }

    pub fn from_xml(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.xml_name() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "左上",
            Self::TopRight => "右上",
            Self::BottomLeft => "左下",
            Self::BottomRight => "右下",
        }
    }
}

/// The watermark settings for one render (`WatermarkSpec`). Sizes are authored at full
/// resolution; a render at another size passes its scale.
#[derive(Debug, Clone, PartialEq)]
pub struct WatermarkSpec {
    pub enabled: bool,
    pub text: String,
    pub font_name: String,
    /// Pixels at full resolution, 6–300.
    pub font_size: f64,
    /// 0 (opaque) – 100 (invisible).
    pub transparency: i32,
    pub color: WatermarkColor,
    pub position: WatermarkPosition,
    /// Pixels at full resolution, 0–9999.
    pub margin: i32,
}

impl WatermarkSpec {
    pub fn is_visible(&self) -> bool {
        self.enabled && !self.text.trim().is_empty()
    }

    /// Text opacity 0..1 (`255 * (1 − transparency / 100)`, rounded like the C# int).
    pub fn opacity(&self) -> f32 {
        let a = (255.0 * (1.0 - self.transparency.clamp(0, 100) as f64 / 100.0)).round();
        (a / 255.0) as f32
    }
}

/// Rasterised text: coverage 0..1 over a canvas, with the layout box (advance width ×
/// line height) the position is computed from at (`box_x`, `box_y`) inside it.
pub struct TextMask {
    pub width: usize,
    pub height: usize,
    pub coverage: Vec<f32>,
    pub box_x: usize,
    pub box_y: usize,
    pub box_w: f64,
    pub box_h: f64,
}

/// Top-left of the text's layout box for a corner anchor (`ApplyWatermark`).
pub fn layout_origin(img_w: usize, img_h: usize, box_w: f64, box_h: f64, position: WatermarkPosition, margin: f64) -> (f64, f64) {
    let x = match position {
        WatermarkPosition::TopLeft | WatermarkPosition::BottomLeft => margin,
        _ => img_w as f64 - box_w - margin,
    };
    let y = match position {
        WatermarkPosition::TopLeft | WatermarkPosition::TopRight => margin,
        _ => img_h as f64 - box_h - margin,
    };
    (x, y)
}

/// Draw the watermark onto `img` (display-encoded RGBA float). `scale` = this render's
/// size relative to full resolution (1 for export).
pub fn apply(img: &mut FloatImage, spec: &WatermarkSpec, scale: f64) {
    if !spec.is_visible() {
        return;
    }
    let size = (spec.font_size * scale).max(4.0);
    let margin = round_half_even(spec.margin as f64 * scale);
    let Some(mask) = render_text(&spec.text, &spec.font_name, size) else { return };
    let (x, y) = layout_origin(img.width, img.height, mask.box_w, mask.box_h, spec.position, margin);
    let ox = x.round() as i64 - mask.box_x as i64;
    let oy = y.round() as i64 - mask.box_y as i64;
    let alpha = spec.opacity();
    let c = spec.color.rgb().map(|v| v as f32 / 255.0);
    for my in 0..mask.height {
        let py = oy + my as i64;
        if py < 0 || py >= img.height as i64 {
            continue;
        }
        for mx in 0..mask.width {
            let px = ox + mx as i64;
            if px < 0 || px >= img.width as i64 {
                continue;
            }
            let a = mask.coverage[my * mask.width + mx] * alpha;
            if a <= 0.0 {
                continue;
            }
            let i = img.index(px as usize, py as usize);
            for k in 0..3 {
                img.data[i + k] = img.data[i + k] * (1.0 - a) + c[k] * a;
            }
        }
    }
}

/// Lay out and rasterise one line of `text` at an em size of `size` pixels.
pub fn render_text(text: &str, font_name: &str, size: f64) -> Option<TextMask> {
    let primary = load_font(font_name);
    let mut fonts: Vec<FontArc> = primary.into_iter().collect();
    fonts.extend(fallback_fonts().iter().cloned());
    let first = fonts.first()?.clone();
    // ab_glyph's PxScale is the ascent−descent height; GDI's pixel size is the em.
    let px = |f: &FontArc| {
        let upem = f.units_per_em().unwrap_or(1000.0);
        PxScale::from((size * (f.height_unscaled() as f64) / upem as f64) as f32)
    };
    let main = first.as_scaled(px(&first));
    let (ascent, descent) = (main.ascent() as f64, main.descent() as f64);

    // Each character from the first font that has it.
    let mut glyphs: Vec<(usize, GlyphId, f64)> = Vec::new();
    let mut pen = 0.0f64;
    let mut prev: Option<(usize, GlyphId)> = None;
    for ch in text.chars().filter(|c| !c.is_control()) {
        let fi = fonts.iter().position(|f| f.glyph_id(ch).0 != 0).unwrap_or(0);
        let f = fonts[fi].as_scaled(px(&fonts[fi]));
        let id = f.glyph_id(ch);
        if let Some((pf, pid)) = prev {
            if pf == fi {
                pen += f.kern(pid, id) as f64;
            }
        }
        glyphs.push((fi, id, pen));
        pen += f.h_advance(id) as f64;
        prev = Some((fi, id));
    }
    let box_w = pen;
    let box_h = ascent - descent;
    if box_w <= 0.0 {
        return None;
    }
    // Room for strokes that overhang the layout box.
    let pad = (size * 0.3).ceil() as usize + 2;
    let (w, h) = (box_w.ceil() as usize + 2 * pad, box_h.ceil() as usize + 2 * pad);
    let mut coverage = vec![0f32; w * h];
    for (fi, id, x) in glyphs {
        let f = &fonts[fi];
        let g = id.with_scale_and_position(px(f), ab_glyph::point((pad as f64 + x) as f32, (pad as f64 + ascent) as f32));
        if let Some(o) = f.outline_glyph(g) {
            let b = o.px_bounds();
            o.draw(|gx, gy, c| {
                let (xx, yy) = (b.min.x as i64 + gx as i64, b.min.y as i64 + gy as i64);
                if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                    let v = &mut coverage[yy as usize * w + xx as usize];
                    *v = (*v + c).min(1.0);
                }
            });
        }
    }
    Some(TextMask { width: w, height: h, coverage, box_x: pad, box_y: pad, box_w, box_h })
}

// ---- fonts ------------------------------------------------------------------------------

/// One installed font face.
#[derive(Debug, Clone)]
pub struct FontFace {
    /// The English (or first) family name — what the list shows and export.xml stores.
    pub family: String,
    /// Every family name in the file (localised ones too), lower-cased, for lookups.
    pub aliases: Vec<String>,
    pub regular: bool,
    pub path: PathBuf,
    pub index: u32,
}

fn font_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    if cfg!(windows) {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        v.push(win.join("Fonts"));
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(l).join(r"Microsoft\Windows\Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        v.extend(["/System/Library/Fonts", "/System/Library/Fonts/Supplemental", "/Library/Fonts"].map(PathBuf::from));
        if let Some(h) = &home {
            v.push(h.join("Library/Fonts"));
        }
    } else {
        v.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(PathBuf::from));
        if let Some(h) = &home {
            v.push(h.join(".local/share/fonts"));
            v.push(h.join(".fonts"));
        }
    }
    v
}

fn collect_font_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                collect_font_files(&p, depth - 1, out);
            }
            continue;
        }
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
            out.push(p);
        }
    }
}

/// Every installed face (scanned once; only the name tables are read).
pub fn system_fonts() -> &'static [FontFace] {
    static FACES: OnceLock<Vec<FontFace>> = OnceLock::new();
    FACES.get_or_init(|| {
        let mut files = Vec::new();
        for d in font_dirs() {
            collect_font_files(&d, 4, &mut files);
        }
        let mut faces = Vec::new();
        for f in files {
            faces.extend(read_faces(&f).unwrap_or_default());
        }
        faces
    })
}

/// Family names for the font list, sorted and unique.
pub fn font_families() -> Vec<String> {
    let mut v: Vec<String> = system_fonts().iter().map(|f| f.family.clone()).collect();
    v.sort_by_key(|s| s.to_lowercase());
    v.dedup();
    v
}

fn face_for(name: &str) -> Option<&'static FontFace> {
    let key = name.trim().to_lowercase();
    let all = system_fonts();
    let matching = || all.iter().filter(|f| f.aliases.iter().any(|a| *a == key));
    matching().find(|f| f.regular).or_else(|| matching().next())
}

fn open_face(path: &Path, index: u32) -> Option<FontArc> {
    let bytes = std::fs::read(path).ok()?;
    ab_glyph::FontVec::try_from_vec_and_index(bytes, index).ok().map(FontArc::new)
}

/// The installed font of that family name (cached).
fn load_font(name: &str) -> Option<FontArc> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<FontArc>>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().unwrap();
    c.entry(name.to_lowercase()).or_insert_with(|| face_for(name).and_then(|f| open_face(&f.path, f.index))).clone()
}

/// The platform CJK UI font, then the bundled Ubuntu Light.
fn fallback_fonts() -> &'static [FontArc] {
    static F: OnceLock<Vec<FontArc>> = OnceLock::new();
    F.get_or_init(|| {
        let mut v = Vec::new();
        let cjk: &[(&str, u32)] = if cfg!(windows) {
            &[(r"C:\Windows\Fonts\msjh.ttc", 1), (r"C:\Windows\Fonts\msjh.ttf", 0), (r"C:\Windows\Fonts\mingliu.ttc", 0)]
        } else if cfg!(target_os = "macos") {
            &[("/System/Library/Fonts/PingFang.ttc", 2), ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0), ("/System/Library/Fonts/STHeiti Medium.ttc", 0)]
        } else {
            &[
                ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 3),
                ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 3),
                ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", 3),
                ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
            ]
        };
        if let Some(f) = cjk.iter().find_map(|&(p, i)| open_face(Path::new(p), i)) {
            v.push(f);
        }
        if let Ok(f) = FontArc::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT) {
            v.push(f);
        }
        v
    })
}

// ---- name tables --------------------------------------------------------------------

fn be16(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|s| u16::from_be_bytes([s[0], s[1]]))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_at(f: &mut std::fs::File, offset: u64, len: usize) -> Option<Vec<u8>> {
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// The faces in a .ttf / .otf / .ttc, reading only the headers and `name` tables.
fn read_faces(path: &Path) -> Option<Vec<FontFace>> {
    let mut f = std::fs::File::open(path).ok()?;
    let head = read_at(&mut f, 0, 12)?;
    let offsets: Vec<u32> = if &head[0..4] == b"ttcf" {
        let n = be32(&head, 8)?.min(64) as usize;
        let t = read_at(&mut f, 12, n * 4)?;
        (0..n).filter_map(|i| be32(&t, i * 4)).collect()
    } else {
        vec![0]
    };
    let mut out = Vec::new();
    for (index, off) in offsets.into_iter().enumerate() {
        let dir = read_at(&mut f, off as u64, 12)?;
        let n = be16(&dir, 4)? as usize;
        let recs = read_at(&mut f, off as u64 + 12, n * 16)?;
        let Some(i) = (0..n).find(|i| &recs[i * 16..i * 16 + 4] == b"name") else { continue };
        let (t_off, t_len) = (be32(&recs, i * 16 + 8)?, be32(&recs, i * 16 + 12)?);
        let Some(table) = read_at(&mut f, t_off as u64, (t_len as usize).min(1 << 20)) else { continue };
        if let Some(face) = parse_name_table(&table, path, index as u32) {
            out.push(face);
        }
    }
    Some(out)
}

fn parse_name_table(t: &[u8], path: &Path, index: u32) -> Option<FontFace> {
    let count = be16(t, 2)? as usize;
    let strings = be16(t, 4)? as usize;
    let (mut english, mut first, mut aliases, mut regular) = (None, None, Vec::new(), false);
    for r in 0..count {
        let at = 6 + r * 12;
        let (platform, encoding, lang, name_id) = (be16(t, at)?, be16(t, at + 2)?, be16(t, at + 4)?, be16(t, at + 6)?);
        let (len, off) = (be16(t, at + 8)? as usize, be16(t, at + 10)? as usize);
        let Some(raw) = t.get(strings + off..strings + off + len) else { continue };
        let s = match (platform, encoding) {
            (3, 0 | 1 | 10) | (0, _) => {
                let u: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&u)
            }
            (1, 0) => raw.iter().map(|&b| b as char).collect(),
            _ => continue,
        };
        let s = s.trim().to_string();
        if s.is_empty() {
            continue;
        }
        match name_id {
            1 => {
                if (platform == 3 && lang == 0x409) || (platform == 1 && lang == 0) {
                    english.get_or_insert_with(|| s.clone());
                }
                first.get_or_insert_with(|| s.clone());
                let l = s.to_lowercase();
                if !aliases.contains(&l) {
                    aliases.push(l);
                }
            }
            2 => regular |= matches!(s.to_lowercase().as_str(), "regular" | "normal" | "book" | "roman"),
            _ => {}
        }
    }
    let family = english.or(first)?;
    Some(FontFace { family, aliases, regular, path: path.to_path_buf(), index })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(pos: WatermarkPosition) -> WatermarkSpec {
        WatermarkSpec {
            enabled: true,
            text: "AwayPhoto 測試".into(),
            font_name: "NoSuchFontAnywhere".into(),
            font_size: 40.0,
            transparency: 0,
            color: WatermarkColor::Red,
            position: pos,
            margin: 30,
        }
    }

    #[test]
    fn corner_origins() {
        let (w, h) = (1000, 800);
        assert_eq!(layout_origin(w, h, 200.0, 50.0, WatermarkPosition::TopLeft, 30.0), (30.0, 30.0));
        assert_eq!(layout_origin(w, h, 200.0, 50.0, WatermarkPosition::TopRight, 30.0), (770.0, 30.0));
        assert_eq!(layout_origin(w, h, 200.0, 50.0, WatermarkPosition::BottomLeft, 30.0), (30.0, 720.0));
        assert_eq!(layout_origin(w, h, 200.0, 50.0, WatermarkPosition::BottomRight, 30.0), (770.0, 720.0));
    }

    #[test]
    fn opacity_and_xml_names() {
        let mut s = spec(WatermarkPosition::TopLeft);
        assert_eq!(s.opacity(), 1.0);
        s.transparency = 20; // 255 × 0.8 = 204
        assert_eq!(s.opacity(), 204.0 / 255.0);
        s.transparency = 150;
        assert_eq!(s.opacity(), 0.0);
        for c in WatermarkColor::ALL {
            assert_eq!(WatermarkColor::from_xml(c.xml_name()), Some(c));
        }
        for p in WatermarkPosition::ALL {
            assert_eq!(WatermarkPosition::from_xml(p.xml_name()), Some(p));
        }
    }

    #[test]
    fn draws_into_the_right_corner() {
        // An unknown font falls back to the bundled one, so this runs on any machine.
        for (pos, left, top) in [(WatermarkPosition::BottomRight, false, false), (WatermarkPosition::TopLeft, true, true)] {
            let mut img = FloatImage::new(600, 300);
            apply(&mut img, &spec(pos), 1.0);
            let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
            for y in 0..img.height {
                for x in 0..img.width {
                    let r = img.data[img.index(x, y)];
                    if r > 0.0 {
                        sx += x as f64 * r as f64;
                        sy += y as f64 * r as f64;
                        n += r as f64;
                    }
                }
            }
            assert!(n > 100.0, "nothing drawn");
            let (cx, cy) = (sx / n, sy / n);
            assert_eq!(cx < 300.0, left, "{pos:?} x {cx}");
            assert_eq!(cy < 150.0, top, "{pos:?} y {cy}");
            // The C# red (230, 50, 50) blended over black.
            assert!(img.data.chunks_exact(4).all(|p| p[1] == p[2] && (p[1] * 230.0 - p[0] * 50.0).abs() < 1e-3));
        }
        // Disabled or blank: untouched.
        let mut img = FloatImage::new(64, 64);
        let mut s = spec(WatermarkPosition::TopLeft);
        s.text = "  ".into();
        apply(&mut img, &s, 1.0);
        assert!(img.data.iter().all(|&v| v == 0.0));
    }
}
