//! Full-resolution export: settings (`export.xml`), file naming, the encoders (with the
//! sRGB ICC profile, DPI and EXIF) and the per-photo flow. Port of the C# `ExportSettings`
//! / `Exporter`, plus 16-bit TIFF / PNG and embedded colour profiles.

use crate::loader::{self, LoaderOptions};
use crate::paths;
use crate::store;
use crate::tiff::{self as tiffmeta, TiffMeta};
use crate::watermark::{self, WatermarkColor, WatermarkPosition, WatermarkSpec};
use crate::xml::{self, XmlNode, XmlStyle};
use awpr_core::buffer::round_half_even;
use awpr_core::{libraw, resize, FloatImage, ImageAdjustments, ProcessContext};
use image::ImageEncoder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportLocation {
    Desktop,
    SameAsSource,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Jpeg,
    Bmp,
    Tiff,
    Png,
}

/// File naming: keep the original name / date-time / a running number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameMode {
    Original,
    DateTime,
    Sequence,
}

/// An existing file of the same name: append `_n` / overwrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictMode {
    AppendNumber,
    Overwrite,
}

macro_rules! xml_enum {
    ($t:ty, $($v:ident),+) => {
        impl $t {
            pub fn xml_name(self) -> &'static str {
                match self { $(Self::$v => stringify!($v)),+ }
            }
            pub fn from_xml(s: &str) -> Option<Self> {
                match s { $(stringify!($v) => Some(Self::$v),)+ _ => None }
            }
        }
    };
}
xml_enum!(ExportLocation, Desktop, SameAsSource, Custom);
xml_enum!(ExportFormat, Jpeg, Bmp, Tiff, Png);
xml_enum!(RenameMode, Original, DateTime, Sequence);
xml_enum!(ConflictMode, AppendNumber, Overwrite);

impl ExportFormat {
    pub const ALL: [ExportFormat; 4] = [Self::Jpeg, Self::Bmp, Self::Tiff, Self::Png];

    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => ".jpg",
            Self::Bmp => ".bmp",
            Self::Tiff => ".tif",
            Self::Png => ".png",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Jpeg => "JPEG",
            Self::Bmp => "BMP",
            Self::Tiff => "TIFF",
            Self::Png => "PNG",
        }
    }
}

/// Persistent export configuration. Field order = the C# property order, which is the
/// element order of export.xml.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportSettings {
    pub location: ExportLocation,
    pub custom_path: String,
    pub use_sub_folder: bool,
    pub sub_folder: String,
    pub rename: RenameMode,
    pub conflict: ConflictMode,
    pub format: ExportFormat,
    /// Long-edge limit in pixels; 0 = full size. Never upscales.
    pub max_long_edge: i64,
    /// Pixels per inch.
    pub resolution: i64,
    pub jpeg_quality: i64,
    pub preserve_exif: bool,
    pub open_explorer_after: bool,
    pub watermark_enabled: bool,
    pub watermark_text: String,
    pub watermark_font_name: String,
    pub watermark_font_size: f64,
    pub watermark_transparency: i64,
    pub watermark_color: WatermarkColor,
    pub watermark_position: WatermarkPosition,
    pub watermark_margin: i64,
}

/// The platform's stand-in for the C# default "Arial".
pub fn default_font_name() -> &'static str {
    if cfg!(windows) {
        "Arial"
    } else if cfg!(target_os = "macos") {
        "Helvetica"
    } else {
        "DejaVu Sans"
    }
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            location: ExportLocation::Desktop,
            custom_path: String::new(),
            use_sub_folder: true,
            sub_folder: "NEW_IMAGE".into(),
            rename: RenameMode::Original,
            conflict: ConflictMode::AppendNumber,
            format: ExportFormat::Jpeg,
            max_long_edge: 2400,
            resolution: 300,
            jpeg_quality: 100,
            preserve_exif: true,
            open_explorer_after: true,
            watermark_enabled: false,
            watermark_text: "Watermark".into(),
            watermark_font_name: default_font_name().into(),
            watermark_font_size: 150.0,
            watermark_transparency: 20,
            watermark_color: WatermarkColor::White,
            watermark_position: WatermarkPosition::BottomRight,
            watermark_margin: 30,
        }
    }
}

impl ExportSettings {
    pub fn watermark(&self) -> WatermarkSpec {
        WatermarkSpec {
            enabled: self.watermark_enabled,
            text: self.watermark_text.clone(),
            font_name: self.watermark_font_name.clone(),
            font_size: self.watermark_font_size,
            transparency: self.watermark_transparency as i32,
            color: self.watermark_color,
            position: self.watermark_position,
            margin: self.watermark_margin as i32,
        }
    }

    pub fn from_xml(r: &XmlNode) -> Self {
        let d = Self::default();
        Self {
            location: r.string("Location").and_then(ExportLocation::from_xml).unwrap_or(d.location),
            custom_path: r.string_or("CustomPath", ""),
            use_sub_folder: r.bool_or("UseSubFolder", d.use_sub_folder),
            sub_folder: r.string_or("SubFolder", &d.sub_folder),
            rename: r.string("Rename").and_then(RenameMode::from_xml).unwrap_or(d.rename),
            conflict: r.string("Conflict").and_then(ConflictMode::from_xml).unwrap_or(d.conflict),
            format: r.string("Format").and_then(ExportFormat::from_xml).unwrap_or(d.format),
            max_long_edge: r.i64_or("MaxLongEdge", d.max_long_edge),
            resolution: r.i64_or("Resolution", d.resolution),
            jpeg_quality: r.i64_or("JpegQuality", d.jpeg_quality),
            preserve_exif: r.bool_or("PreserveExif", d.preserve_exif),
            open_explorer_after: r.bool_or("OpenExplorerAfter", d.open_explorer_after),
            watermark_enabled: r.bool_or("WatermarkEnabled", d.watermark_enabled),
            watermark_text: r.string_or("WatermarkText", &d.watermark_text),
            watermark_font_name: r.string_or("WatermarkFontName", &d.watermark_font_name),
            watermark_font_size: r.f64_or("WatermarkFontSize", d.watermark_font_size),
            watermark_transparency: r.i64_or("WatermarkTransparency", d.watermark_transparency),
            watermark_color: r.string("WatermarkColor").and_then(WatermarkColor::from_xml).unwrap_or(d.watermark_color),
            watermark_position: r.string("WatermarkPosition").and_then(WatermarkPosition::from_xml).unwrap_or(d.watermark_position),
            watermark_margin: r.i64_or("WatermarkMargin", d.watermark_margin),
        }
    }

    pub fn to_xml(&self, style: XmlStyle) -> String {
        let mut n = XmlNode::new("ExportSettings");
        n.add_str("Location", self.location.xml_name());
        n.add_str("CustomPath", &self.custom_path);
        n.add_bool("UseSubFolder", self.use_sub_folder);
        n.add_str("SubFolder", &self.sub_folder);
        n.add_str("Rename", self.rename.xml_name());
        n.add_str("Conflict", self.conflict.xml_name());
        n.add_str("Format", self.format.xml_name());
        n.add_i64("MaxLongEdge", self.max_long_edge);
        n.add_i64("Resolution", self.resolution);
        n.add_i64("JpegQuality", self.jpeg_quality);
        n.add_bool("PreserveExif", self.preserve_exif);
        n.add_bool("OpenExplorerAfter", self.open_explorer_after);
        n.add_bool("WatermarkEnabled", self.watermark_enabled);
        n.add_str("WatermarkText", &self.watermark_text);
        n.add_str("WatermarkFontName", &self.watermark_font_name);
        n.add_f64("WatermarkFontSize", self.watermark_font_size);
        n.add_i64("WatermarkTransparency", self.watermark_transparency);
        n.add_str("WatermarkColor", self.watermark_color.xml_name());
        n.add_str("WatermarkPosition", self.watermark_position.xml_name());
        n.add_i64("WatermarkMargin", self.watermark_margin);
        n.to_document(style)
    }

    /// The Rust build's own `export.rust.xml`; the first run starts from the C# / Swift
    /// `export.xml` beside it (never rewritten, like `settings.xml`).
    pub fn load() -> Self {
        let dir = paths::app_data_dir();
        [dir.join("export.rust.xml"), dir.join("export.xml")]
            .iter()
            .find_map(|p| std::fs::read(p).ok().and_then(|b| xml::parse(&String::from_utf8_lossy(&b))))
            .map(|r| Self::from_xml(&r))
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let dir = paths::app_data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let _ = paths::write_atomic(&dir.join("export.rust.xml").to_string_lossy(), self.to_xml(XmlStyle::native()).as_bytes());
    }

    /// The destination folder for a photo (sub-folder created).
    pub fn resolve_output_dir(&self, source: &str) -> std::io::Result<PathBuf> {
        let mut base = match self.location {
            ExportLocation::Desktop => desktop_dir(),
            ExportLocation::SameAsSource => Path::new(source).parent().map(Path::to_path_buf).unwrap_or_else(desktop_dir),
            ExportLocation::Custom if self.custom_path.trim().is_empty() => desktop_dir(),
            ExportLocation::Custom => PathBuf::from(self.custom_path.trim()),
        };
        if self.use_sub_folder && !self.sub_folder.trim().is_empty() {
            base = base.join(self.sub_folder.trim());
        }
        std::fs::create_dir_all(&base)?;
        Ok(base)
    }
}

/// The user's desktop folder.
pub fn desktop_dir() -> PathBuf {
    let home = PathBuf::from(std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap_or_default());
    if cfg!(windows) {
        // OneDrive "backup" moves the desktop; take it when the plain one is gone.
        let plain = home.join("Desktop");
        let onedrive = home.join("OneDrive").join("Desktop");
        return if !plain.is_dir() && onedrive.is_dir() { onedrive } else { plain };
    }
    if cfg!(target_os = "linux") {
        if let Ok(dirs) = std::fs::read_to_string(home.join(".config/user-dirs.dirs")) {
            for line in dirs.lines() {
                if let Some(v) = line.trim().strip_prefix("XDG_DESKTOP_DIR=") {
                    let v = v.trim_matches('"').replace("$HOME", &home.to_string_lossy());
                    return PathBuf::from(v);
                }
            }
        }
    }
    home.join("Desktop")
}

// ---- naming ------------------------------------------------------------------------------

/// Capture time as (year, month, day, hour, minute, second).
pub type Stamp = (i32, u32, u32, u32, u32, u32);

/// "yyyy:MM:dd HH:mm:ss" (EXIF) → a stamp.
pub fn parse_exif_date(s: &str) -> Option<Stamp> {
    let s = s.trim();
    let (d, t) = s.split_once(' ')?;
    let mut dp = d.split([':', '-', '/']).map(|v| v.parse::<u32>().ok());
    let mut tp = t.split(':').map(|v| v.parse::<u32>().ok());
    let (y, mo, da) = (dp.next()??, dp.next()??, dp.next()??);
    let (h, mi, se) = (tp.next()??, tp.next()??, tp.next().flatten().unwrap_or(0));
    ((1..=12).contains(&mo) && (1..=31).contains(&da) && h < 24 && mi < 60 && se < 61).then_some((y as i32, mo, da, h, mi, se))
}

/// When the photo was taken: the cached EXIF date, else the file's modification time.
pub fn capture_time(path: &str, copy: i32) -> Stamp {
    let (_, exif, _) = store::load_all(path, copy);
    if let Some(s) = exif.and_then(|e| parse_exif_date(&e.date_taken)) {
        return s;
    }
    use chrono::{Datelike, Timelike};
    match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(t) => {
            let l: chrono::DateTime<chrono::Local> = t.into();
            (l.year(), l.month(), l.day(), l.hour(), l.minute(), l.second())
        }
        Err(_) => (1, 1, 1, 0, 0, 0),
    }
}

/// Running state of one export batch (sequence number, photos per second).
#[derive(Default)]
pub struct NameState {
    seq: u32,
    per_second: HashMap<String, u32>,
}

/// The output name without extension (`BuildBaseName`).
pub fn base_name(rename: RenameMode, path: &str, copy: i32, stamp: impl FnOnce() -> Stamp, st: &mut NameState) -> String {
    match rename {
        RenameMode::DateTime => {
            let (y, mo, d, h, mi, s) = stamp();
            let ts = format!("{:02}{mo:02}{d:02}{h:02}{mi:02}{s:02}", y.rem_euclid(100));
            let c = st.per_second.entry(ts.clone()).or_insert(0);
            *c += 1;
            // The last two digits tell photos taken in the same second apart.
            format!("IMG{ts}{c:02}")
        }
        RenameMode::Sequence => {
            st.seq += 1;
            format!("IMG{:05}", st.seq)
        }
        RenameMode::Original => {
            let stem = Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if copy > 0 {
                format!("{stem}_copy{copy}_edited")
            } else {
                format!("{stem}_edited")
            }
        }
    }
}

/// `name.ext`, or the first free `name_1.ext`, `name_2.ext`… (`UniquePath`).
pub fn unique_path(dir: &Path, name: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{name}{ext}"));
    let mut n = 1;
    while p.exists() {
        p = dir.join(format!("{name}_{n}{ext}"));
        n += 1;
    }
    p
}

/// Output size for a long-edge limit; never upscales; `Math.Round` (half to even).
pub fn long_edge_size(w: usize, h: usize, max_long_edge: i64) -> (usize, usize) {
    let longest = w.max(h);
    if max_long_edge <= 0 || longest as i64 <= max_long_edge {
        return (w, h);
    }
    let scale = max_long_edge as f64 / longest as f64;
    ((round_half_even(w as f64 * scale) as usize).max(1), (round_half_even(h as f64 * scale) as usize).max(1))
}

// ---- ICC ---------------------------------------------------------------------------------

/// A compact ICC v2 sRGB display profile (D50 PCS, 1024-entry sRGB tone curves), built
/// here so no third-party profile file has to be shipped.
pub fn srgb_icc() -> &'static [u8] {
    static ICC: OnceLock<Vec<u8>> = OnceLock::new();
    ICC.get_or_init(build_srgb_icc)
}

fn s15(v: f64) -> [u8; 4] {
    ((v * 65536.0).round() as i32).to_be_bytes()
}

fn build_srgb_icc() -> Vec<u8> {
    let xyz = |x: f64, y: f64, z: f64| [b"XYZ ".as_slice(), &[0; 4], &s15(x), &s15(y), &s15(z)].concat();
    let desc = {
        let text = b"sRGB IEC61966-2.1 (AwayPhotoRawEditor)\0";
        let mut v = [b"desc".as_slice(), &[0; 4], &(text.len() as u32).to_be_bytes(), text].concat();
        v.extend_from_slice(&[0; 4]); // Unicode language
        v.extend_from_slice(&[0; 4]); // Unicode count
        v.extend_from_slice(&[0; 2]); // ScriptCode code
        v.push(0); // ScriptCode count
        v.extend_from_slice(&[0; 67]);
        v
    };
    let cprt = [b"text".as_slice(), &[0; 4], b"No copyright, use freely\0"].concat();
    let curve = {
        let n = 1024u32;
        let mut v = [b"curv".as_slice(), &[0; 4], &n.to_be_bytes()].concat();
        for i in 0..n {
            let x = i as f64 / (n - 1) as f64;
            let y = if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) };
            v.extend_from_slice(&((y * 65535.0).round() as u16).to_be_bytes());
        }
        v
    };
    // sRGB primaries chromatically adapted (Bradford) to the D50 PCS.
    let data: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", desc),
        (b"cprt", cprt),
        (b"wtpt", xyz(0.9642, 1.0, 0.8249)),
        (b"rXYZ", xyz(0.4360747, 0.2225045, 0.0139322)),
        (b"gXYZ", xyz(0.3850649, 0.7168786, 0.0971045)),
        (b"bXYZ", xyz(0.1430804, 0.0606169, 0.7141733)),
        (b"rTRC", curve),
    ];
    // gTRC / bTRC share the red curve's data.
    let n_tags = data.len() + 2;
    let mut body = Vec::new();
    let mut table = Vec::new();
    let mut offset = 128 + 4 + 12 * n_tags;
    let mut trc = (0, 0);
    for (sig, d) in &data {
        table.extend_from_slice(*sig);
        table.extend_from_slice(&(offset as u32).to_be_bytes());
        table.extend_from_slice(&(d.len() as u32).to_be_bytes());
        if *sig == b"rTRC" {
            trc = (offset, d.len());
        }
        body.extend_from_slice(d);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 128 + 4 + 12 * n_tags + body.len();
    }
    for sig in [b"gTRC", b"bTRC"] {
        table.extend_from_slice(sig);
        table.extend_from_slice(&(trc.0 as u32).to_be_bytes());
        table.extend_from_slice(&(trc.1 as u32).to_be_bytes());
    }
    let size = 128 + 4 + table.len() + body.len();
    let mut h = Vec::with_capacity(size);
    h.extend_from_slice(&(size as u32).to_be_bytes());
    h.extend_from_slice(&[0; 4]); // CMM
    h.extend_from_slice(&0x0210_0000u32.to_be_bytes()); // version 2.1
    h.extend_from_slice(b"mntr");
    h.extend_from_slice(b"RGB ");
    h.extend_from_slice(b"XYZ ");
    for v in [2026u16, 1, 1, 0, 0, 0] {
        h.extend_from_slice(&v.to_be_bytes());
    }
    h.extend_from_slice(b"acsp");
    h.extend_from_slice(&[0; 4 * 4 + 8]); // platform, flags, manufacturer, model, attributes
    h.extend_from_slice(&0u32.to_be_bytes()); // perceptual intent
    for v in [0.9642, 1.0, 0.8249] {
        h.extend_from_slice(&s15(v));
    }
    h.extend_from_slice(&[0; 4]); // creator
    h.resize(128, 0);
    h.extend_from_slice(&(n_tags as u32).to_be_bytes());
    h.extend_from_slice(&table);
    h.extend_from_slice(&body);
    h
}

// ---- EXIF --------------------------------------------------------------------------------

/// What the exported file's EXIF says about the shot.
#[derive(Debug, Clone, Default)]
pub struct ShotInfo {
    pub make: String,
    pub model: String,
    pub lens: String,
    pub date: String,
    pub exposure_time: f64,
    pub f_number: f64,
    pub iso: u32,
    pub focal_length: f64,
    pub exposure_bias: f64,
    pub metering_mode: u32,
}

impl ShotInfo {
    /// From the source file's own tags, with LibRaw filling the gaps for RAWs.
    pub fn read(path: &str) -> Self {
        let t: TiffMeta = tiffmeta::read(path).unwrap_or_default();
        let raw = if paths::is_raw(path) { libraw::read_meta(path) } else { None };
        let r = raw.as_ref();
        let date = [&t.date_original, &t.date_digitized, &t.date_time].into_iter().flatten().find(|s| !s.trim().is_empty()).cloned().unwrap_or_default();
        Self {
            make: t.make.clone().or_else(|| r.map(|r| r.make.clone())).unwrap_or_default(),
            model: t.model.clone().or_else(|| r.map(|r| r.model.clone())).unwrap_or_default(),
            lens: t.lens_model.clone().or_else(|| r.map(|r| r.lens.clone())).unwrap_or_default(),
            date,
            exposure_time: t.exposure_time.or_else(|| r.map(|r| r.shutter as f64)).unwrap_or(0.0),
            f_number: t.f_number.or_else(|| r.map(|r| r.aperture as f64)).unwrap_or(0.0),
            iso: t.iso.or_else(|| r.map(|r| r.iso_speed.round() as u32)).unwrap_or(0),
            focal_length: t.focal_length.or_else(|| r.map(|r| r.focal_len as f64)).unwrap_or(0.0),
            exposure_bias: t.exposure_bias.unwrap_or(0.0),
            metering_mode: t.metering_mode.unwrap_or(0),
        }
    }
}

struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    data: Vec<u8>,
}

fn ascii(tag: u16, s: &str) -> Option<Entry> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut d: Vec<u8> = s.bytes().filter(|b| b.is_ascii() && *b != 0).collect();
    d.push(0);
    Some(Entry { tag, kind: 2, count: d.len() as u32, data: d })
}

fn short(tag: u16, v: u16) -> Entry {
    Entry { tag, kind: 3, count: 1, data: v.to_le_bytes().to_vec() }
}

fn long(tag: u16, v: u32) -> Entry {
    Entry { tag, kind: 4, count: 1, data: v.to_le_bytes().to_vec() }
}

fn rational(tag: u16, n: u32, d: u32) -> Entry {
    Entry { tag, kind: 5, count: 1, data: [n.to_le_bytes(), d.to_le_bytes()].concat() }
}

fn srational(tag: u16, n: i32, d: i32) -> Entry {
    Entry { tag, kind: 10, count: 1, data: [n.to_le_bytes(), d.to_le_bytes()].concat() }
}

/// One IFD at `at`: entries (sorted), next-IFD 0, then the values that do not fit.
fn write_ifd(mut entries: Vec<Entry>, at: usize) -> Vec<u8> {
    entries.sort_by_key(|e| e.tag);
    let mut out = Vec::new();
    let mut extra = Vec::new();
    let data_at = at + 2 + entries.len() * 12 + 4;
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for e in &entries {
        out.extend_from_slice(&e.tag.to_le_bytes());
        out.extend_from_slice(&e.kind.to_le_bytes());
        out.extend_from_slice(&e.count.to_le_bytes());
        if e.data.len() <= 4 {
            let mut v = e.data.clone();
            v.resize(4, 0);
            out.extend_from_slice(&v);
        } else {
            out.extend_from_slice(&((data_at + extra.len()) as u32).to_le_bytes());
            extra.extend_from_slice(&e.data);
            if extra.len() % 2 == 1 {
                extra.push(0);
            }
        }
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&extra);
    out
}

fn ifd_len(entries: &[Entry]) -> usize {
    2 + entries.len() * 12 + 4 + entries.iter().filter(|e| e.data.len() > 4).map(|e| e.data.len().div_ceil(2) * 2).sum::<usize>()
}

/// A TIFF-structured EXIF block (what JPEG APP1 / PNG eXIf carry, without the
/// "Exif\0\0" prefix): camera, lens and shot data, orientation normal (the pixels are
/// already upright), the output size and DPI, sRGB.
pub fn build_exif(info: &ShotInfo, width: usize, height: usize, dpi: i64) -> Vec<u8> {
    let software = format!("AwayPhotoRawEditor {}", env!("CARGO_PKG_VERSION"));
    let dpi = dpi.max(1) as u32;
    let mut ifd0: Vec<Entry> = [ascii(271, &info.make), ascii(272, &info.model), ascii(305, &software), ascii(306, &info.date)].into_iter().flatten().collect();
    ifd0.push(short(274, 1));
    ifd0.push(rational(282, dpi, 1));
    ifd0.push(rational(283, dpi, 1));
    ifd0.push(short(296, 2));
    let mut exif: Vec<Entry> = Vec::new();
    if info.exposure_time > 0.0 {
        exif.push(if info.exposure_time < 1.0 {
            rational(33434, 1, (1.0 / info.exposure_time).round().max(1.0) as u32)
        } else {
            rational(33434, (info.exposure_time * 10.0).round() as u32, 10)
        });
    }
    if info.f_number > 0.0 {
        exif.push(rational(33437, (info.f_number * 10.0).round() as u32, 10));
    }
    if info.iso > 0 {
        exif.push(short(34855, info.iso.min(65535) as u16));
    }
    exif.push(Entry { tag: 36864, kind: 7, count: 4, data: b"0230".to_vec() });
    exif.extend(ascii(36867, &info.date));
    exif.push(srational(37380, (info.exposure_bias * 100.0).round() as i32, 100));
    if info.metering_mode > 0 {
        exif.push(short(37383, info.metering_mode.min(65535) as u16));
    }
    if info.focal_length > 0.0 {
        exif.push(rational(37386, (info.focal_length * 10.0).round() as u32, 10));
    }
    exif.push(short(40961, 1)); // sRGB
    exif.push(long(40962, width as u32));
    exif.push(long(40963, height as u32));
    exif.extend(ascii(42036, &info.lens));

    // IFD0 gets a pointer to the EXIF IFD, which follows it.
    ifd0.push(long(34665, 0));
    let exif_at = 8 + ifd_len(&ifd0);
    ifd0.last_mut().unwrap().data = (exif_at as u32).to_le_bytes().to_vec();
    let mut out = b"II*\0".to_vec();
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend(write_ifd(ifd0, 8));
    debug_assert_eq!(out.len(), exif_at);
    out.extend(write_ifd(exif, exif_at));
    out
}

// ---- encoding ----------------------------------------------------------------------------

fn to_rgb16(buf: &FloatImage) -> Vec<u16> {
    let mut out = vec![0u16; buf.width * buf.height * 3];
    out.par_chunks_mut(buf.width * 3).enumerate().for_each(|(y, row)| {
        let s = &buf.data[y * buf.width * 4..(y + 1) * buf.width * 4];
        for x in 0..buf.width {
            for k in 0..3 {
                row[x * 3 + k] = (s[x * 4 + k] * 65535.0 + 0.5).clamp(0.0, 65535.0) as u16;
            }
        }
    });
    out
}

fn crc32(data: &[u8]) -> u32 {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (n, v) in t.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *v = c;
        }
        t
    });
    !data.iter().fold(!0u32, |c, &b| t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8))
}

/// Insert a pHYs chunk (pixels per metre) right after IHDR.
fn png_add_dpi(png: &mut Vec<u8>, dpi: i64) {
    let ppm = (dpi as f64 / 0.0254).round() as u32;
    let mut chunk = Vec::with_capacity(21);
    chunk.extend_from_slice(&9u32.to_be_bytes());
    let mut body = b"pHYs".to_vec();
    body.extend_from_slice(&ppm.to_be_bytes());
    body.extend_from_slice(&ppm.to_be_bytes());
    body.push(1); // unit: metre
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    // Signature (8) + IHDR (4 length + 4 type + 13 data + 4 CRC).
    let at = 8 + 25;
    if png.len() > at && &png[12..16] == b"IHDR" {
        png.splice(at..at, chunk);
    }
}

fn ioe<E: Into<Box<dyn std::error::Error + Send + Sync>>>(e: E) -> std::io::Error {
    std::io::Error::other(e)
}

/// Bytes of the output file. `sixteen` applies to TIFF / PNG only.
pub fn encode(img: &FloatImage, format: ExportFormat, sixteen: bool, quality: i64, dpi: i64, exif: Option<&[u8]>, info: &ShotInfo) -> std::io::Result<Vec<u8>> {
    let (w, h) = (img.width as u32, img.height as u32);
    let mut out = Vec::new();
    match format {
        ExportFormat::Jpeg => {
            let mut e = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(50, 100) as u8);
            if dpi > 0 {
                e.set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(dpi.min(65535) as u16));
            }
            let _ = e.set_icc_profile(srgb_icc().to_vec());
            if let Some(x) = exif {
                let _ = e.set_exif_metadata(x.to_vec());
            }
            e.write_image(&crate::codec::to_rgb8(img), w, h, image::ExtendedColorType::Rgb8).map_err(ioe)?;
        }
        ExportFormat::Png => {
            let mut e = image::codecs::png::PngEncoder::new(&mut out);
            let _ = e.set_icc_profile(srgb_icc().to_vec());
            if let Some(x) = exif {
                let _ = e.set_exif_metadata(x.to_vec());
            }
            if sixteen {
                let px = to_rgb16(img);
                let bytes: Vec<u8> = px.iter().flat_map(|v| v.to_ne_bytes()).collect();
                e.write_image(&bytes, w, h, image::ExtendedColorType::Rgb16).map_err(ioe)?;
            } else {
                e.write_image(&crate::codec::to_rgb8(img), w, h, image::ExtendedColorType::Rgb8).map_err(ioe)?;
            }
            if dpi > 0 {
                png_add_dpi(&mut out, dpi);
            }
        }
        ExportFormat::Tiff => {
            use tiff::encoder::{colortype, Compression, Rational, TiffEncoder};
            use tiff::tags::{ResolutionUnit, Tag};
            let mut cur = std::io::Cursor::new(&mut out);
            let mut enc = TiffEncoder::new(&mut cur).map_err(ioe)?.with_compression(Compression::Lzw);
            macro_rules! write {
                ($ct:ty, $data:expr) => {{
                    let mut im = enc.new_image::<$ct>(w, h).map_err(ioe)?;
                    if dpi > 0 {
                        im.resolution(ResolutionUnit::Inch, Rational { n: dpi as u32, d: 1 });
                    }
                    let d = im.encoder();
                    d.write_tag(Tag::IccProfile, srgb_icc()).map_err(ioe)?;
                    if exif.is_some() {
                        // Baseline tags only; the EXIF sub-IFD is not written for TIFF.
                        for (tag, v) in [(Tag::Make, &info.make), (Tag::Model, &info.model), (Tag::DateTime, &info.date)] {
                            if !v.trim().is_empty() {
                                d.write_tag(tag, v.trim()).map_err(ioe)?;
                            }
                        }
                    }
                    d.write_tag(Tag::Software, format!("AwayPhotoRawEditor {}", env!("CARGO_PKG_VERSION")).as_str()).map_err(ioe)?;
                    im.write_data($data).map_err(ioe)?;
                }};
            }
            if sixteen {
                write!(colortype::RGB16, &to_rgb16(img));
            } else {
                write!(colortype::RGB8, &crate::codec::to_rgb8(img));
            }
        }
        ExportFormat::Bmp => {
            image::codecs::bmp::BmpEncoder::new(&mut out).write_image(&crate::codec::to_rgb8(img), w, h, image::ExtendedColorType::Rgb8).map_err(ioe)?;
            // BITMAPINFOHEADER biXPelsPerMeter / biYPelsPerMeter.
            if dpi > 0 && out.len() > 46 {
                let ppm = ((dpi as f64 / 0.0254).round() as u32).to_le_bytes();
                out[38..42].copy_from_slice(&ppm);
                out[42..46].copy_from_slice(&ppm);
            }
        }
    }
    Ok(out)
}

// ---- the flow ----------------------------------------------------------------------------

/// One photo to export.
#[derive(Debug, Clone)]
pub struct ExportItem {
    pub path: String,
    pub copy: i32,
}

/// The pipeline the caller provides (GPU when it can, else `apply_to_float`).
pub type Render<'a> = dyn Fn(&FloatImage, &ImageAdjustments, &ProcessContext) -> FloatImage + Sync + 'a;

/// The written file and how it was made (for reports).
#[derive(Debug, Clone)]
pub struct Written {
    pub path: PathBuf,
    pub width: usize,
    pub height: usize,
    pub sixteen: bool,
    pub bytes: usize,
}

/// Export one photo under `base` (`ExportOne`): full decode → pipeline → watermark at
/// full resolution → long-edge resize → encode → write.
pub fn export_one(item: &ExportItem, s: &ExportSettings, opt: LoaderOptions, base: &str, render: &Render) -> Result<Written, String> {
    let (full, source) = loader::decode_full(&item.path, opt).ok_or("無法解碼影像")?;
    let (adj, exif, _) = store::load_all(&item.path, item.copy);
    let adj = adj.unwrap_or_default();
    let mut exif = exif.unwrap_or_else(|| crate::exif::read(&item.path));
    // Old XMLs without the camera colour data: add it (and keep it for next time).
    if exif.camera.as_ref().is_none_or(|c| !c.is_valid()) && paths::is_raw(&item.path) && opt.use_libraw {
        if let Some(c) = libraw::read_camera_color(&item.path) {
            exif.camera = Some(c);
            let _ = store::save(&item.path, &adj, item.copy, Some(&exif));
        }
    }
    let ctx = ProcessContext { camera: exif.camera.clone(), white_balance_reference: source.white_balance_reference(), ..Default::default() };
    let mut out = render(&full, &adj, &ctx);
    drop(full);
    watermark::apply(&mut out, &s.watermark(), 1.0);
    let (w, h) = long_edge_size(out.width, out.height, s.max_long_edge);
    if (w, h) != (out.width, out.height) {
        out = resize::resize(&out, w, h);
    }

    let dir = s.resolve_output_dir(&item.path).map_err(|e| format!("無法建立資料夾：{e}"))?;
    let ext = s.format.extension();
    let path = match s.conflict {
        ConflictMode::Overwrite => dir.join(format!("{base}{ext}")),
        ConflictMode::AppendNumber => unique_path(&dir, base, ext),
    };
    let sixteen = opt.high_precision && matches!(s.format, ExportFormat::Tiff | ExportFormat::Png);
    let info = ShotInfo::read(&item.path);
    let exif_block = (s.preserve_exif && s.format != ExportFormat::Bmp).then(|| build_exif(&info, out.width, out.height, s.resolution));
    let bytes = encode(&out, s.format, sixteen, s.jpeg_quality, s.resolution, exif_block.as_deref(), &info).map_err(|e| e.to_string())?;
    std::fs::write(&path, &bytes).map_err(|e| format!("無法寫入 {}：{e}", path.display()))?;
    Ok(Written { path, width: out.width, height: out.height, sixteen, bytes: bytes.len() })
}

/// Export a batch. `progress(done, total, name)` is called before and after each photo;
/// returning false cancels before the next one.
pub fn export_all(items: &[ExportItem], s: &ExportSettings, opt: LoaderOptions, render: &Render, mut progress: impl FnMut(usize, usize, &str) -> bool) -> Result<Vec<Written>, String> {
    let mut st = NameState::default();
    let mut written = Vec::new();
    let total = items.len();
    for (done, item) in items.iter().enumerate() {
        let name = paths::file_name(&item.path);
        if !progress(done, total, &name) {
            break;
        }
        let base = base_name(s.rename, &item.path, item.copy, || capture_time(&item.path, item.copy), &mut st);
        let w = export_one(item, s, opt, &base, render).map_err(|e| format!("匯出「{name}」失敗：{e}"))?;
        written.push(w);
        progress(done + 1, total, &name);
    }
    if s.open_explorer_after {
        if let Some(first) = written.first() {
            reveal(&first.path);
        }
    }
    Ok(written)
}

/// Show a file in the platform's file manager.
pub fn reveal(file: &Path) {
    let r = if cfg!(windows) {
        std::process::Command::new("explorer.exe").arg(format!("/select,{}", file.display())).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg("-R").arg(file).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(file.parent().unwrap_or(Path::new("."))).spawn()
    };
    let _ = r;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_edge_never_upscales_and_rounds_half_even() {
        assert_eq!(long_edge_size(6000, 4000, 2400), (2400, 1600));
        assert_eq!(long_edge_size(4000, 6000, 2400), (1600, 2400));
        assert_eq!(long_edge_size(2000, 1000, 2400), (2000, 1000)); // no upscale
        assert_eq!(long_edge_size(6000, 4000, 0), (6000, 4000)); // 0 = full size
        // 5 × 0.5 = 2.5 → 2 (Math.Round is half to even), 3 × 0.5 = 1.5 → 2.
        assert_eq!(long_edge_size(10, 5, 5), (5, 2));
        assert_eq!(long_edge_size(10, 3, 5), (5, 2));
    }

    #[test]
    fn names() {
        let mut st = NameState::default();
        let stamp = || (2021, 5, 10, 16, 2, 58);
        assert_eq!(base_name(RenameMode::DateTime, "a.ARW", 0, stamp, &mut st), "IMG21051016025801");
        assert_eq!(base_name(RenameMode::DateTime, "b.ARW", 0, stamp, &mut st), "IMG21051016025802");
        assert_eq!(base_name(RenameMode::DateTime, "c.ARW", 0, || (2021, 5, 10, 16, 2, 59), &mut st), "IMG21051016025901");
        assert_eq!(base_name(RenameMode::Sequence, "a.ARW", 0, stamp, &mut st), "IMG00001");
        assert_eq!(base_name(RenameMode::Sequence, "a.ARW", 0, stamp, &mut st), "IMG00002");
        // Paths are local ones: backslashes only separate on Windows.
        #[cfg(windows)]
        assert_eq!(base_name(RenameMode::Original, r"C:\p\DSC0001.ARW", 0, stamp, &mut st), "DSC0001_edited");
        assert_eq!(base_name(RenameMode::Original, "/p/DSC0001.ARW", 0, stamp, &mut st), "DSC0001_edited");
        assert_eq!(base_name(RenameMode::Original, "/p/DSC0001.ARW", 2, stamp, &mut st), "DSC0001_copy2_edited");
        assert_eq!(parse_exif_date("2021:05:10 16:02:58"), Some((2021, 5, 10, 16, 2, 58)));
        assert_eq!(parse_exif_date("0000:00:00 00:00:00"), None);
        assert_eq!(parse_exif_date(""), None);
    }

    #[test]
    fn unique_paths_append_numbers() {
        let dir = std::env::temp_dir().join(format!("awpr_unique_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_path(&dir, "x", ".jpg"), dir.join("x.jpg"));
        std::fs::write(dir.join("x.jpg"), b"1").unwrap();
        assert_eq!(unique_path(&dir, "x", ".jpg"), dir.join("x_1.jpg"));
        std::fs::write(dir.join("x_1.jpg"), b"1").unwrap();
        assert_eq!(unique_path(&dir, "x", ".jpg"), dir.join("x_2.jpg"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_xml_round_trip_and_csharp_defaults() {
        let d = ExportSettings::default();
        assert_eq!((d.max_long_edge, d.resolution, d.jpeg_quality, d.sub_folder.as_str()), (2400, 300, 100, "NEW_IMAGE"));
        let s = ExportSettings {
            location: ExportLocation::Custom,
            custom_path: r"D:\out".into(),
            rename: RenameMode::DateTime,
            conflict: ConflictMode::Overwrite,
            format: ExportFormat::Tiff,
            watermark_enabled: true,
            watermark_text: "© 測試".into(),
            watermark_font_size: 72.5,
            watermark_color: WatermarkColor::Orange,
            watermark_position: WatermarkPosition::TopLeft,
            ..Default::default()
        };
        let x = s.to_xml(XmlStyle::DotNet);
        assert!(x.contains("<Location>Custom</Location>") && x.contains("<Format>Tiff</Format>") && x.contains("<WatermarkColor>Orange</WatermarkColor>"));
        // Element order = the C# property order.
        let order = ["Location", "CustomPath", "UseSubFolder", "SubFolder", "Rename", "Conflict", "Format", "MaxLongEdge", "Resolution", "JpegQuality", "PreserveExif", "OpenExplorerAfter", "WatermarkEnabled", "WatermarkText", "WatermarkFontName", "WatermarkFontSize", "WatermarkTransparency", "WatermarkColor", "WatermarkPosition", "WatermarkMargin"];
        let pos: Vec<usize> = order.iter().map(|t| x.find(&format!("<{t}>")).unwrap_or_else(|| panic!("{t}"))).collect();
        assert!(pos.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(ExportSettings::from_xml(&xml::parse(&x).unwrap()), s);
        // A C# file with unknown / missing values keeps the defaults.
        let partial = xml::parse("<?xml version=\"1.0\"?><ExportSettings><Format>Png</Format><Rename>Bogus</Rename></ExportSettings>").unwrap();
        let p = ExportSettings::from_xml(&partial);
        assert_eq!((p.format, p.rename, p.max_long_edge), (ExportFormat::Png, RenameMode::Original, 2400));
    }

    #[test]
    fn icc_profile_is_well_formed() {
        let p = srgb_icc();
        assert_eq!(u32::from_be_bytes(p[0..4].try_into().unwrap()) as usize, p.len());
        assert_eq!(&p[36..40], b"acsp");
        assert_eq!(&p[12..20], b"mntrRGB ");
        let n = u32::from_be_bytes(p[128..132].try_into().unwrap()) as usize;
        assert_eq!(n, 9);
        let mut sigs = Vec::new();
        for i in 0..n {
            let e = &p[132 + i * 12..144 + i * 12];
            let (off, len) = (u32::from_be_bytes(e[4..8].try_into().unwrap()) as usize, u32::from_be_bytes(e[8..12].try_into().unwrap()) as usize);
            assert!(off % 4 == 0 && off + len <= p.len());
            sigs.push(String::from_utf8_lossy(&e[0..4]).into_owned());
            // Each tag's data starts with its type signature.
            let ty = &p[off..off + 4];
            assert!([b"desc".as_slice(), b"text", b"XYZ ", b"curv"].contains(&ty));
        }
        for s in ["desc", "cprt", "wtpt", "rXYZ", "gXYZ", "bXYZ", "rTRC", "gTRC", "bTRC"] {
            assert!(sigs.iter().any(|x| x == s), "{s}");
        }
    }

    #[test]
    fn exif_block_parses_back() {
        let info = ShotInfo { make: "SONY".into(), model: "ILCE-7M3".into(), lens: "FE 85mm F1.8".into(), date: "2021:05:10 16:02:58".into(), exposure_time: 1.0 / 250.0, f_number: 2.8, iso: 400, focal_length: 85.0, exposure_bias: -0.7, metering_mode: 5 };
        let b = build_exif(&info, 2400, 1600, 300);
        let t = crate::tiff::read_bytes(&b).expect("our TIFF reader understands it");
        assert_eq!(t.make.as_deref(), Some("SONY"));
        assert_eq!(t.model.as_deref(), Some("ILCE-7M3"));
        assert_eq!(t.orientation, Some(1));
        assert_eq!(t.iso, Some(400));
        assert_eq!(t.date_original.as_deref(), Some("2021:05:10 16:02:58"));
        assert!((t.exposure_time.unwrap() - 0.004).abs() < 1e-9);
        assert!((t.f_number.unwrap() - 2.8).abs() < 1e-9);
        assert!((t.focal_length.unwrap() - 85.0).abs() < 1e-9);
        assert!((t.exposure_bias.unwrap() + 0.7).abs() < 1e-9);
        assert_eq!((t.pixel_x, t.pixel_y), (Some(2400), Some(1600)));
        assert_eq!(t.lens_model.as_deref(), Some("FE 85mm F1.8"));
    }

    #[test]
    fn encoders_embed_icc_dpi_and_depth() {
        let mut img = FloatImage::new(40, 30);
        for (i, v) in img.data.iter_mut().enumerate() {
            *v = (i % 97) as f32 / 96.0;
        }
        let info = ShotInfo { make: "TEST".into(), ..Default::default() };
        let exif = build_exif(&info, 40, 30, 300);
        for (fmt, sixteen) in [(ExportFormat::Jpeg, false), (ExportFormat::Png, false), (ExportFormat::Png, true), (ExportFormat::Tiff, false), (ExportFormat::Tiff, true), (ExportFormat::Bmp, false)] {
            let bytes = encode(&img, fmt, sixteen, 90, 300, Some(&exif), &info).unwrap();
            let fmt_img = match fmt {
                ExportFormat::Jpeg => image::ImageFormat::Jpeg,
                ExportFormat::Png => image::ImageFormat::Png,
                ExportFormat::Tiff => image::ImageFormat::Tiff,
                ExportFormat::Bmp => image::ImageFormat::Bmp,
            };
            let reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), fmt_img);
            let mut dec = reader.into_decoder().unwrap();
            use image::ImageDecoder;
            assert_eq!(dec.dimensions(), (40, 30), "{fmt:?}");
            let bits16 = matches!(dec.color_type(), image::ColorType::Rgb16 | image::ColorType::Rgba16);
            assert_eq!(bits16, sixteen, "{fmt:?} {:?}", dec.color_type());
            match fmt {
                // image's TIFF decoder does not hand the tag back; read it with tiff itself.
                ExportFormat::Tiff => {
                    let mut t = tiff::decoder::Decoder::new(std::io::Cursor::new(&bytes)).unwrap();
                    assert_eq!(t.get_tag_u8_vec(tiff::tags::Tag::IccProfile).unwrap(), srgb_icc(), "TIFF ICC");
                    assert_eq!(t.get_tag_ascii_string(tiff::tags::Tag::Make).unwrap(), "TEST");
                }
                ExportFormat::Bmp => {}
                _ => assert_eq!(dec.icc_profile().unwrap().as_deref(), Some(srgb_icc()), "{fmt:?} ICC"),
            }
            match fmt {
                ExportFormat::Jpeg => assert_eq!(&bytes[13..17], &[1, 0x01, 0x2C, 0x01]), // JFIF units = inches, Xdensity 300
                ExportFormat::Png => {
                    let at = bytes.windows(4).position(|w| w == b"pHYs").expect("pHYs");
                    assert_eq!(u32::from_be_bytes(bytes[at + 4..at + 8].try_into().unwrap()), 11811);
                }
                ExportFormat::Bmp => assert_eq!(u32::from_le_bytes(bytes[38..42].try_into().unwrap()), 11811),
                ExportFormat::Tiff => {}
            }
            if matches!(fmt, ExportFormat::Jpeg | ExportFormat::Png) {
                assert!(dec.exif_metadata().unwrap().is_some_and(|e| e.windows(4).any(|w| w == b"TEST")), "{fmt:?} EXIF");
            }
        }
    }
}

