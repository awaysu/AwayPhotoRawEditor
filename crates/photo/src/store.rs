//! `RAW_TEMP/*.rawpipe.xml` (edits + cached EXIF) and `preview_list.xml` (hidden photos,
//! virtual copies), in the exact shape the C# `XmlSerializer` and the Swift
//! `AdjustmentXmlStore` read and write.

use crate::paths;
use crate::xml::{self, XmlNode, XmlStyle};
use awpr_core::{color, CameraColorInfo, HealSpot, ImageAdjustments, LinearGradient, Rotation};

/// EXIF / photo metadata, cached inside the adjustment XML.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExifData {
    pub camera_make: String,
    pub camera_model: String,
    pub lens: String,
    pub iso: String,
    pub aperture: String,
    pub shutter: String,
    pub focal_length: String,
    pub exposure_bias: String,
    pub white_balance: String,
    pub metering_mode: String,
    /// As-shot colour temperature (K) if the camera recorded it; 0 = unknown.
    pub color_temperature: f64,
    pub tint: f64,
    /// LibRaw colour data for the white-balance matrix; None for non-RAW.
    pub camera: Option<CameraColorInfo>,
    pub date_taken: String,
    pub width: i64,
    pub height: i64,
    pub file_size: i64,
    pub file_path: String,
}

impl ExifData {
    pub fn has_as_shot_white_balance(&self) -> bool {
        (2000.0..=12000.0).contains(&self.color_temperature)
    }

    pub fn file_size_display(&self) -> String {
        if self.file_size <= 0 {
            String::new()
        } else if self.file_size >= 1024 * 1024 {
            format!("{:.1} MB", self.file_size as f64 / 1024.0 / 1024.0)
        } else {
            format!("{:.1} KB", self.file_size as f64 / 1024.0)
        }
    }

    pub fn dimensions_display(&self) -> String {
        if self.width > 0 && self.height > 0 {
            format!("{} x {}", self.width, self.height)
        } else {
            String::new()
        }
    }
}

/// Root of a `.rawpipe.xml`.
#[derive(Debug, Clone, Default)]
pub struct RawPipeDocument {
    /// Written by `ensure_default` and never edited.
    pub is_placeholder: bool,
    /// Absent before v1.0.15 → 0 (legacy maths).
    pub pipeline_version: i32,
    pub adjustments: ImageAdjustments,
    pub exif: Option<ExifData>,
}

/// Value equality ignoring the pipeline version (a legacy photo with untouched sliders
/// is not "edited") — the C# `ValueEquals`.
pub fn value_equals(a: &ImageAdjustments, b: &ImageAdjustments) -> bool {
    let mut b2 = b.clone();
    b2.pipeline_version = a.pipeline_version;
    *a == b2
}

pub fn is_default(a: &ImageAdjustments) -> bool {
    value_equals(a, &ImageAdjustments::default())
}

fn rotation_name(r: Rotation) -> &'static str {
    match r {
        Rotation::R0 => "R0",
        Rotation::R90 => "R90",
        Rotation::R180 => "R180",
        Rotation::R270 => "R270",
    }
}

fn rotation_from(s: &str) -> Option<Rotation> {
    Some(match s.trim() {
        "R0" | "0" => Rotation::R0,
        "R90" | "90" => Rotation::R90,
        "R180" | "180" => Rotation::R180,
        "R270" | "270" => Rotation::R270,
        _ => return None,
    })
}

// ---- encoding (element order = the C# property declaration order) ----------------

fn encode_adjustments(a: &ImageAdjustments) -> XmlNode {
    let mut n = XmlNode::new("Adjustments");
    n.add_f64("Exposure", a.exposure);
    n.add_f64("Contrast", a.contrast);
    n.add_f64("Highlights", a.highlights);
    n.add_f64("Shadows", a.shadows);
    n.add_f64("Whites", a.whites);
    n.add_f64("Blacks", a.blacks);
    n.add_f64("Temperature", a.temperature);
    n.add_f64("Tint", a.tint);
    n.add_f64("Vibrance", a.vibrance);
    n.add_f64("Saturation", a.saturation);
    n.add_f64("Sharpening", a.sharpening);
    n.add_f64("NoiseReduction", a.noise_reduction);
    n.add_f64("Vignette", a.vignette);
    n.add_f64("Distortion", a.distortion);
    n.add_str("CropAspectRatio", &a.crop_aspect_ratio);
    n.add_f64("CropAngle", a.crop_angle);
    n.add_f64("CropX", a.crop_x);
    n.add_f64("CropY", a.crop_y);
    n.add_f64("CropWidth", a.crop_width);
    n.add_f64("CropHeight", a.crop_height);
    n.add_str("Rotation", rotation_name(a.rotation));
    let g = n.add(XmlNode::new("Gradients"));
    for gr in &a.gradients {
        let e = g.add(XmlNode::new("LinearGradient"));
        e.add_f64("CenterX", gr.center_x);
        e.add_f64("CenterY", gr.center_y);
        e.add_f64("Angle", gr.angle);
        e.add_f64("Range", gr.range);
        e.add_f64("Exposure", gr.exposure);
        e.add_f64("Contrast", gr.contrast);
        e.add_f64("Highlights", gr.highlights);
        e.add_f64("Shadows", gr.shadows);
        e.add_f64("Saturation", gr.saturation);
    }
    n.add_f64("HealSize", a.heal_size);
    let h = n.add(XmlNode::new("HealSpots"));
    for s in &a.heal_spots {
        let e = h.add(XmlNode::new("HealSpot"));
        e.add_f64("TargetX", s.target_x);
        e.add_f64("TargetY", s.target_y);
        e.add_f64("SourceX", s.source_x);
        e.add_f64("SourceY", s.source_y);
        e.add_f64("Radius", s.radius);
        e.add_f64("RadiusNorm", s.radius_norm);
        e.add_bool("UseInpaint", s.use_inpaint);
    }
    n
}

fn decode_adjustments(n: &XmlNode) -> ImageAdjustments {
    let mut a = ImageAdjustments::default();
    a.exposure = n.f64_or("Exposure", a.exposure);
    a.contrast = n.f64_or("Contrast", a.contrast);
    a.highlights = n.f64_or("Highlights", a.highlights);
    a.shadows = n.f64_or("Shadows", a.shadows);
    a.whites = n.f64_or("Whites", a.whites);
    a.blacks = n.f64_or("Blacks", a.blacks);
    a.temperature = n.f64_or("Temperature", a.temperature);
    a.tint = n.f64_or("Tint", a.tint);
    a.vibrance = n.f64_or("Vibrance", a.vibrance);
    a.saturation = n.f64_or("Saturation", a.saturation);
    a.sharpening = n.f64_or("Sharpening", a.sharpening);
    a.noise_reduction = n.f64_or("NoiseReduction", a.noise_reduction);
    a.vignette = n.f64_or("Vignette", a.vignette);
    a.distortion = n.f64_or("Distortion", a.distortion);
    a.crop_aspect_ratio = n.string_or("CropAspectRatio", &a.crop_aspect_ratio);
    a.crop_angle = n.f64_or("CropAngle", a.crop_angle);
    a.crop_x = n.f64_or("CropX", a.crop_x);
    a.crop_y = n.f64_or("CropY", a.crop_y);
    a.crop_width = n.f64_or("CropWidth", a.crop_width);
    a.crop_height = n.f64_or("CropHeight", a.crop_height);
    if let Some(r) = n.string("Rotation").and_then(rotation_from) {
        a.rotation = r;
    }
    if let Some(g) = n.child("Gradients") {
        a.gradients = g
            .children_named("LinearGradient")
            .map(|e| {
                let d = LinearGradient::default();
                LinearGradient {
                    center_x: e.f64_or("CenterX", d.center_x),
                    center_y: e.f64_or("CenterY", d.center_y),
                    angle: e.f64_or("Angle", d.angle),
                    range: e.f64_or("Range", d.range),
                    exposure: e.f64_or("Exposure", d.exposure),
                    contrast: e.f64_or("Contrast", d.contrast),
                    highlights: e.f64_or("Highlights", d.highlights),
                    shadows: e.f64_or("Shadows", d.shadows),
                    saturation: e.f64_or("Saturation", d.saturation),
                }
            })
            .collect();
    }
    a.heal_size = n.f64_or("HealSize", a.heal_size);
    if let Some(h) = n.child("HealSpots") {
        a.heal_spots = h
            .children_named("HealSpot")
            .map(|e| {
                let d = HealSpot::default();
                HealSpot {
                    target_x: e.f64_or("TargetX", d.target_x),
                    target_y: e.f64_or("TargetY", d.target_y),
                    source_x: e.f64_or("SourceX", d.source_x),
                    source_y: e.f64_or("SourceY", d.source_y),
                    radius: e.f64_or("Radius", d.radius),
                    radius_norm: e.f64_or("RadiusNorm", d.radius_norm),
                    use_inpaint: e.bool_or("UseInpaint", d.use_inpaint),
                }
            })
            .collect();
    }
    a
}

fn encode_exif(e: &ExifData) -> XmlNode {
    let mut n = XmlNode::new("Exif");
    n.add_str("CameraMake", &e.camera_make);
    n.add_str("CameraModel", &e.camera_model);
    n.add_str("Lens", &e.lens);
    n.add_str("ISO", &e.iso);
    n.add_str("Aperture", &e.aperture);
    n.add_str("Shutter", &e.shutter);
    n.add_str("FocalLength", &e.focal_length);
    n.add_str("ExposureBias", &e.exposure_bias);
    n.add_str("WhiteBalance", &e.white_balance);
    n.add_str("MeteringMode", &e.metering_mode);
    n.add_f64("ColorTemperature", e.color_temperature);
    n.add_f64("Tint", e.tint);
    if let Some(c) = &e.camera {
        let cn = n.add(XmlNode::new("Camera"));
        cn.add_f64_array("PreMul", &c.pre_mul);
        cn.add_f64_array("CamMul", &c.cam_mul);
        cn.add_f64_array("RgbCam", &c.rgb_cam);
    }
    n.add_str("DateTaken", &e.date_taken);
    n.add_i64("Width", e.width);
    n.add_i64("Height", e.height);
    n.add_i64("FileSize", e.file_size);
    n.add_str("FilePath", &e.file_path);
    n
}

fn decode_exif(n: &XmlNode) -> ExifData {
    let camera = n.child("Camera").and_then(|cn| {
        let arr3 = |k| cn.f64_array(k, 3).map(|v| [v[0], v[1], v[2]]);
        let c = CameraColorInfo {
            pre_mul: arr3("PreMul")?,
            cam_mul: arr3("CamMul")?,
            rgb_cam: cn.f64_array("RgbCam", 9)?.try_into().ok()?,
        };
        c.is_valid().then_some(c)
    });
    ExifData {
        camera_make: n.string_or("CameraMake", ""),
        camera_model: n.string_or("CameraModel", ""),
        lens: n.string_or("Lens", ""),
        iso: n.string_or("ISO", ""),
        aperture: n.string_or("Aperture", ""),
        shutter: n.string_or("Shutter", ""),
        focal_length: n.string_or("FocalLength", ""),
        exposure_bias: n.string_or("ExposureBias", ""),
        white_balance: n.string_or("WhiteBalance", ""),
        metering_mode: n.string_or("MeteringMode", ""),
        color_temperature: n.f64_or("ColorTemperature", 0.0),
        tint: n.f64_or("Tint", 0.0),
        camera,
        date_taken: n.string_or("DateTaken", ""),
        width: n.i64_or("Width", 0),
        height: n.i64_or("Height", 0),
        file_size: n.i64_or("FileSize", 0),
        file_path: n.string_or("FilePath", ""),
    }
}

impl RawPipeDocument {
    pub fn parse(src: &str) -> Option<Self> {
        let root = xml::parse(src)?;
        if root.name != "RawPipe" {
            return None;
        }
        let mut doc = RawPipeDocument {
            is_placeholder: root.bool_or("IsPlaceholder", false),
            pipeline_version: root.i64_or("PipelineVersion", 0) as i32,
            adjustments: root.child("Adjustments").map(decode_adjustments).unwrap_or_default(),
            exif: root.child("Exif").map(decode_exif),
        };
        // The version rides on the document; the pipeline only sees the adjustments.
        doc.adjustments.pipeline_version = doc.pipeline_version;
        Some(doc)
    }

    pub fn to_xml(&self, style: XmlStyle) -> String {
        let mut root = XmlNode::new("RawPipe");
        root.add_bool("IsPlaceholder", self.is_placeholder);
        root.add_i64("PipelineVersion", self.pipeline_version as i64);
        root.add(encode_adjustments(&self.adjustments));
        if let Some(e) = &self.exif {
            root.add(encode_exif(e));
        }
        root.to_document(style)
    }

    pub fn load(path: &str) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        Self::parse(&String::from_utf8_lossy(&bytes))
    }

    pub fn write(&self, path: &str) -> std::io::Result<()> {
        paths::write_atomic(path, self.to_xml(XmlStyle::native()).as_bytes())
    }
}

/// Everything in a photo's XML in one read: (adjustments, exif, is_placeholder).
pub fn load_all(image: &str, copy_index: i32) -> (Option<ImageAdjustments>, Option<ExifData>, bool) {
    match RawPipeDocument::load(&paths::adjustment_xml_path(image, copy_index)) {
        Some(d) => (Some(d.adjustments), d.exif, d.is_placeholder),
        None => (None, None, false),
    }
}

/// Save edits, keeping the cached EXIF when none is supplied.
pub fn save(image: &str, adjustments: &ImageAdjustments, copy_index: i32, exif: Option<&ExifData>) -> std::io::Result<()> {
    let path = paths::adjustment_xml_path(image, copy_index);
    let exif = match exif {
        Some(e) => Some(e.clone()),
        None => RawPipeDocument::load(&path).and_then(|d| d.exif),
    };
    RawPipeDocument {
        is_placeholder: false,
        pipeline_version: adjustments.pipeline_version,
        adjustments: adjustments.clone(),
        exif,
    }
    .write(&path)
}

/// The stored adjustments, or fresh defaults seeded with the as-shot white balance and
/// written as a placeholder when the photo has no XML yet.
pub fn ensure_default(image: &str, exif: Option<&ExifData>, copy_index: i32) -> ImageAdjustments {
    let path = paths::adjustment_xml_path(image, copy_index);
    if let Some(d) = RawPipeDocument::load(&path) {
        return d.adjustments;
    }
    let mut adj = ImageAdjustments::default();
    if let Some(shot) = exif.and_then(|e| e.camera.as_ref()).filter(|c| c.is_valid()).and_then(color::as_shot) {
        adj.temperature = shot.0.clamp(color::MIN_KELVIN, color::MAX_KELVIN);
        adj.tint = shot.1;
    } else if let Some(e) = exif.filter(|e| e.has_as_shot_white_balance()) {
        adj.temperature = e.color_temperature;
    }
    let _ = RawPipeDocument {
        is_placeholder: true,
        pipeline_version: adj.pipeline_version,
        adjustments: adj.clone(),
        exif: exif.cloned(),
    }
    .write(&path);
    adj
}

// ---- preview_list.xml ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct VirtualCopyEntry {
    pub path: String,
    pub index: i32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreviewList {
    /// Keys (`path` or `path|copy:N`) hidden from the strip.
    pub hidden: Vec<String>,
    pub virtual_copies: Vec<VirtualCopyEntry>,
}

impl PreviewList {
    pub fn parse(src: &str) -> Option<Self> {
        let root = xml::parse(src)?;
        let mut list = PreviewList::default();
        if let Some(h) = root.child("Hidden") {
            list.hidden = h.children_named("string").filter_map(|n| n.text.clone()).collect();
        }
        if let Some(v) = root.child("VirtualCopies") {
            list.virtual_copies = v
                .children_named("VirtualCopyEntry")
                .filter_map(|n| {
                    let p = n.string("Path")?;
                    (!p.is_empty()).then(|| VirtualCopyEntry { path: p.to_string(), index: n.i64_or("Index", 0) as i32 })
                })
                .collect();
        }
        Some(list)
    }

    pub fn to_xml(&self, style: XmlStyle) -> String {
        let mut root = XmlNode::new("PreviewList");
        let h = root.add(XmlNode::new("Hidden"));
        for k in &self.hidden {
            h.add_str("string", k);
        }
        let v = root.add(XmlNode::new("VirtualCopies"));
        for e in &self.virtual_copies {
            let n = v.add(XmlNode::new("VirtualCopyEntry"));
            n.add_str("Path", &e.path);
            n.add_i64("Index", e.index as i64);
        }
        root.to_document(style)
    }

    pub fn load(folder: &str) -> Self {
        std::fs::read(paths::preview_list_path(folder))
            .ok()
            .and_then(|b| Self::parse(&String::from_utf8_lossy(&b)))
            .unwrap_or_default()
    }

    pub fn save(&self, folder: &str) -> std::io::Result<()> {
        paths::write_atomic(&paths::preview_list_path(folder), self.to_xml(XmlStyle::native()).as_bytes())
    }
}

/// `path|copy:N` → (path, N).
pub fn parse_key(key: &str) -> (String, i32) {
    match key.rfind("|copy:") {
        Some(i) => (key[..i].to_string(), key[i + 6..].parse().unwrap_or(0)),
        None => (key.to_string(), 0),
    }
}

pub fn make_key(path: &str, copy_index: i32) -> String {
    if copy_index <= 0 {
        path.to_string()
    } else {
        format!("{path}|copy:{copy_index}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(rel: &str) -> String {
        let p = format!("{}/../../tests/fixtures/xml/{rel}", env!("CARGO_MANIFEST_DIR"));
        String::from_utf8(std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}"))).unwrap()
    }

    /// Reading a file the C# / Swift build wrote and writing it back in that build's
    /// style must reproduce it byte for byte.
    #[test]
    fn rawpipe_round_trips_byte_identical() {
        let cases = [
            ("csharp/DSC_3579.NEF.rawpipe.xml", XmlStyle::DotNet),
            ("csharp/Canon-eos-r-raw-00001.cr3.rawpipe.xml", XmlStyle::DotNet),
            ("csharp/placeholder_dng.rawpipe.xml", XmlStyle::DotNet),
            ("swift/sony_a7m3.ARW.rawpipe.xml", XmlStyle::Swift),
            ("swift/canon_eosr.CR3.rawpipe.xml", XmlStyle::Swift),
        ];
        for (rel, style) in cases {
            let src = fixture(rel);
            let doc = RawPipeDocument::parse(&src).unwrap_or_else(|| panic!("{rel} did not parse"));
            assert_eq!(doc.to_xml(style), src, "{rel}");
        }
    }

    #[test]
    fn rawpipe_values() {
        let d = RawPipeDocument::parse(&fixture("csharp/Canon-eos-r-raw-00001.cr3.rawpipe.xml")).unwrap();
        assert_eq!(d.pipeline_version, 1);
        assert_eq!(d.adjustments.gradients.len(), 1);
        assert_eq!(d.adjustments.gradients[0].exposure, 1.335877776145935);
        assert_eq!(d.adjustments.crop_y, 0.14088543256123853);
        let e = d.exif.unwrap();
        assert_eq!(e.lens, "RF50mm F1.2 L USM");
        assert_eq!(e.exposure_bias, "+0.7 EV");
        assert!(e.camera.is_some());

        let p = RawPipeDocument::parse(&fixture("csharp/placeholder_dng.rawpipe.xml")).unwrap();
        assert!(p.is_placeholder);
        assert_eq!(p.adjustments.temperature, 6500.111995871754);
    }

    #[test]
    fn edits_with_gradients_and_heal_round_trip() {
        let mut a = ImageAdjustments::default();
        a.exposure = -1.25;
        a.rotation = Rotation::R270;
        a.crop_aspect_ratio = "3:2".into();
        a.gradients.push(LinearGradient { exposure: 0.5, angle: -12.5, ..Default::default() });
        a.heal_spots.push(HealSpot { target_x: 0.25, source_x: 0.3, use_inpaint: true, ..Default::default() });
        let doc = RawPipeDocument { adjustments: a.clone(), pipeline_version: 1, ..Default::default() };
        for style in [XmlStyle::DotNet, XmlStyle::Swift] {
            let back = RawPipeDocument::parse(&doc.to_xml(style)).unwrap();
            assert_eq!(back.adjustments, a);
            assert!(back.exif.is_none());
        }
    }

    #[test]
    fn legacy_xml_without_version_is_pipeline_0() {
        let d = RawPipeDocument::parse("<RawPipe><Adjustments><Exposure>1</Exposure></Adjustments></RawPipe>").unwrap();
        assert_eq!(d.pipeline_version, 0);
        assert!(d.adjustments.is_legacy_pipeline());
        assert_eq!(d.adjustments.exposure, 1.0);
    }

    #[test]
    fn preview_list_round_trips() {
        let src = fixture("csharp/preview_list.xml");
        let l = PreviewList::parse(&src).unwrap();
        assert_eq!(l.to_xml(XmlStyle::DotNet), src);
        let full = PreviewList {
            hidden: vec![r"D:\a\b.ARW".into(), r"D:\a\b.ARW|copy:1".into()],
            virtual_copies: vec![VirtualCopyEntry { path: r"D:\a\b.ARW".into(), index: 1 }],
        };
        assert_eq!(PreviewList::parse(&full.to_xml(XmlStyle::Swift)).unwrap(), full);
        assert_eq!(parse_key(r"D:\a\b.ARW|copy:3"), (r"D:\a\b.ARW".to_string(), 3));
    }
}
