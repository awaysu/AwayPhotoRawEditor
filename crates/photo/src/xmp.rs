//! XMP sidecars (`{photo stem}.xmp` next to the photo): 匯出 XMP／匯入 XMP.
//!
//! The structure follows lightcraft's `crates/meta/src/xmp.rs` (MIT / Apache-2.0,
//! https://github.com/storytold/lightcraft): every `rdf:Description` is collected into a
//! flat `prefix:name → values` map — attribute form, element form, `rdf:Seq` / `rdf:Bag` /
//! `rdf:Alt` arrays — with prefixes canonicalised by namespace URI, so a packet that binds
//! the camera-raw namespace to another prefix still reads.
//!
//! Writing puts what has a standard meaning into Adobe's `crs:` (camera-raw-settings)
//! fields — exposure, the tone sliders, white balance, vibrance / saturation, sharpening,
//! noise reduction, vignette, distortion, crop and angle, HSL, tone curves — and every
//! adjustment, exactly, into this app's namespace (`awpr:Adjustments`, the rawpipe.xml
//! `<Adjustments>` element as text, plus `awpr:ProcessVersion`). Reading prefers the
//! exact copy; a sidecar written by another program is mapped from its `crs:` fields.

use crate::store::{self, ExifData};
use crate::xml::{self, format_double, XmlNode, XmlStyle};
use awpr_core::{ImageAdjustments, Rotation};
use std::collections::BTreeMap;
use std::path::Path;

pub const AWPR_NS: &str = "http://ns.awaysu.cc/awayphotoraweditor/1.0/";
pub const CRS_NS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

/// Canonical prefixes by namespace URI.
const NAMESPACES: &[(&str, &str)] = &[
    ("x", "adobe:ns:meta/"),
    ("rdf", RDF_NS),
    ("crs", CRS_NS),
    ("awpr", AWPR_NS),
    ("xmp", "http://ns.adobe.com/xap/1.0/"),
    ("tiff", "http://ns.adobe.com/tiff/1.0/"),
    ("exif", "http://ns.adobe.com/exif/1.0/"),
];

/// The 8 HSL bands as Camera Raw names them (our band order).
const CRS_BANDS: [&str; 8] = ["Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta"];

/// `DSC_0001.ARW` → `DSC_0001.xmp` (Lightroom's convention); a virtual copy N →
/// `DSC_0001.copyN.xmp`.
pub fn sidecar_path(image: &str, copy_index: i32) -> String {
    let p = Path::new(image);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = if copy_index <= 0 { format!("{stem}.xmp") } else { format!("{stem}.copy{copy_index}.xmp") };
    p.with_file_name(name).to_string_lossy().into_owned()
}

// ---- reading -----------------------------------------------------------------------

fn canonical(qname: &str, scopes: &[Vec<(String, String)>]) -> String {
    let (prefix, local) = qname.split_once(':').unwrap_or(("", qname));
    if prefix == "xml" || prefix == "xmlns" {
        return qname.to_string();
    }
    let uri = scopes.iter().rev().flat_map(|s| s.iter().rev()).find(|(p, _)| p == prefix).map(|(_, u)| u.as_str());
    match uri.and_then(|u| NAMESPACES.iter().find(|(_, nu)| *nu == u)) {
        Some((p, _)) => format!("{p}:{local}"),
        None => qname.to_string(),
    }
}

/// The tree with element and attribute names canonicalised (xmlns attributes dropped).
fn canonicalise(n: &XmlNode, scopes: &mut Vec<Vec<(String, String)>>) -> XmlNode {
    let mut scope = Vec::new();
    for (k, v) in &n.attributes {
        if k == "xmlns" {
            scope.push((String::new(), v.clone()));
        } else if let Some(p) = k.strip_prefix("xmlns:") {
            scope.push((p.to_string(), v.clone()));
        }
    }
    scopes.push(scope);
    let out = XmlNode {
        name: canonical(&n.name, scopes),
        text: n.text.clone(),
        attributes: n.attributes.iter().filter(|(k, _)| !k.starts_with("xmlns")).map(|(k, v)| (canonical(k, scopes), v.clone())).collect(),
        children: n.children.iter().map(|c| canonicalise(c, scopes)).collect(),
    };
    scopes.pop();
    out
}

fn is_rdf_meta(k: &str) -> bool {
    matches!(k, "rdf:about" | "rdf:ID" | "rdf:nodeID" | "xml:lang" | "rdf:parseType" | "rdf:resource" | "rdf:datatype")
}

fn attr<'a>(n: &'a XmlNode, k: &str) -> Option<&'a str> {
    n.attributes.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
}

fn collect_description(d: &XmlNode, prefix: &str, props: &mut BTreeMap<String, Vec<String>>) {
    for (k, v) in &d.attributes {
        if !is_rdf_meta(k) {
            props.entry(format!("{prefix}{k}")).or_default().push(v.clone());
        }
    }
    for c in &d.children {
        collect_property(c, prefix, props);
    }
}

fn collect_property(p: &XmlNode, prefix: &str, props: &mut BTreeMap<String, Vec<String>>) {
    let key = format!("{prefix}{}", p.name);
    if let Some(arr) = p.children.iter().find(|c| matches!(c.name.as_str(), "rdf:Seq" | "rdf:Bag" | "rdf:Alt")) {
        let items = arr.children.iter().filter(|li| li.name == "rdf:li").map(|li| li.text.clone().unwrap_or_default().trim().to_string());
        props.entry(key).or_default().extend(items);
        return;
    }
    let struct_prefix = format!("{key}/");
    if attr(p, "rdf:parseType") == Some("Resource") {
        for c in &p.children {
            collect_property(c, &struct_prefix, props);
        }
        return;
    }
    if let Some(d) = p.children.iter().find(|c| c.name == "rdf:Description") {
        collect_description(d, &struct_prefix, props);
        return;
    }
    let v = attr(p, "rdf:resource").map(str::to_string).unwrap_or_else(|| p.text.clone().unwrap_or_default().trim().to_string());
    props.entry(key).or_default().push(v);
}

fn find_descriptions<'a>(n: &'a XmlNode, out: &mut Vec<&'a XmlNode>) {
    if n.name == "rdf:Description" {
        out.push(n);
        return;
    }
    for c in &n.children {
        find_descriptions(c, out);
    }
}

/// Every property of every `rdf:Description`, keyed `prefix:name`.
pub fn properties(src: &str) -> Option<BTreeMap<String, Vec<String>>> {
    let root = xml::parse(src)?;
    let root = canonicalise(&root, &mut Vec::new());
    let mut descs = Vec::new();
    find_descriptions(&root, &mut descs);
    let mut props = BTreeMap::new();
    for d in descs {
        collect_description(d, "", &mut props);
    }
    Some(props)
}

/// Adjustments from an XMP packet. `base` supplies everything a sidecar from another
/// program does not describe (and its geometry when it has no crop). None when the packet
/// carries no development settings at all.
pub fn read_xmp(src: &str, base: &ImageAdjustments) -> Option<ImageAdjustments> {
    let props = properties(src)?;
    let one = |k: &str| props.get(k).and_then(|v| v.first()).map(|s| s.trim());
    let num = |k: &str| one(k).and_then(|s| s.trim_start_matches('+').parse::<f64>().ok());

    // This app's exact copy.
    if let Some(text) = one("awpr:Adjustments") {
        if let Some(n) = xml::parse(text) {
            let mut a = store::adjustments_from_node(&n);
            a.pipeline_version = num("awpr:ProcessVersion").map(|v| (v as i32 - 1).clamp(0, ImageAdjustments::CURRENT_PIPELINE_VERSION)).unwrap_or(ImageAdjustments::CURRENT_PIPELINE_VERSION);
            return Some(a);
        }
    }

    // Another program's camera-raw settings.
    if !props.keys().any(|k| k.starts_with("crs:")) {
        return None;
    }
    let mut a = base.clone();
    a.pipeline_version = ImageAdjustments::CURRENT_PIPELINE_VERSION;
    let set = |k: &str, f: &mut f64, lo: f64, hi: f64| {
        if let Some(v) = num(k) {
            *f = v.clamp(lo, hi);
        }
    };
    set("crs:Exposure2012", &mut a.exposure, -5.0, 5.0);
    set("crs:Contrast2012", &mut a.contrast, -100.0, 100.0);
    set("crs:Highlights2012", &mut a.highlights, -100.0, 100.0);
    set("crs:Shadows2012", &mut a.shadows, -100.0, 100.0);
    set("crs:Whites2012", &mut a.whites, -100.0, 100.0);
    set("crs:Blacks2012", &mut a.blacks, -100.0, 100.0);
    set("crs:Temperature", &mut a.temperature, awpr_core::color::MIN_KELVIN, awpr_core::color::MAX_KELVIN);
    set("crs:Tint", &mut a.tint, -100.0, 100.0);
    set("crs:Vibrance", &mut a.vibrance, -100.0, 100.0);
    set("crs:Saturation", &mut a.saturation, -100.0, 100.0);
    set("crs:Sharpness", &mut a.sharpening, -100.0, 100.0);
    set("crs:LuminanceSmoothing", &mut a.noise_reduction, 0.0, 100.0);
    set("crs:LensManualDistortionAmount", &mut a.distortion, -100.0, 100.0);
    if let Some(v) = num("crs:PostCropVignetteAmount") {
        a.vignette = (-v).clamp(-100.0, 100.0);
    }
    for (i, band) in CRS_BANDS.iter().enumerate() {
        set(&format!("crs:HueAdjustment{band}"), &mut a.hsl_hue[i], -100.0, 100.0);
        set(&format!("crs:SaturationAdjustment{band}"), &mut a.hsl_saturation[i], -100.0, 100.0);
        set(&format!("crs:LuminanceAdjustment{band}"), &mut a.hsl_luminance[i], -100.0, 100.0);
    }
    let curve = |k: &str| -> Option<Vec<(f64, f64)>> {
        let pts: Vec<(f64, f64)> = props
            .get(k)?
            .iter()
            .filter_map(|s| {
                let (x, y) = s.split_once(',')?;
                Some((x.trim().parse::<f64>().ok()? / 255.0, y.trim().parse::<f64>().ok()? / 255.0))
            })
            .collect();
        (pts.len() >= 2).then(|| if awpr_core::v3::curve_is_identity(&pts) { Vec::new() } else { pts })
    };
    if let Some(c) = curve("crs:ToneCurvePV2012") {
        a.curve_rgb = c;
    }
    if let Some(c) = curve("crs:ToneCurvePV2012Red") {
        a.curve_red = c;
    }
    if let Some(c) = curve("crs:ToneCurvePV2012Green") {
        a.curve_green = c;
    }
    if let Some(c) = curve("crs:ToneCurvePV2012Blue") {
        a.curve_blue = c;
    }
    if one("crs:HasCrop").is_some_and(|v| v.eq_ignore_ascii_case("true")) {
        if let (Some(l), Some(t), Some(r), Some(b)) = (num("crs:CropLeft"), num("crs:CropTop"), num("crs:CropRight"), num("crs:CropBottom")) {
            if r > l && b > t {
                (a.crop_x, a.crop_y, a.crop_width, a.crop_height) = (l, t, r - l, b - t);
                a.crop_angle = num("crs:CropAngle").unwrap_or(0.0);
            }
        }
    }
    Some(a)
}

// ---- writing -----------------------------------------------------------------------

fn esc(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn signed(v: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, v);
    if v >= 0.0 && !s.starts_with('-') {
        format!("+{s}")
    } else {
        s
    }
}

/// A complete sidecar packet for these adjustments.
pub fn write_xmp(a: &ImageAdjustments, exif: Option<&ExifData>) -> String {
    let mut attrs: Vec<(String, String)> = vec![
        ("crs:Version".into(), "15.0".into()),
        ("crs:ProcessVersion".into(), "11.0".into()),
        ("crs:WhiteBalance".into(), "Custom".into()),
        ("crs:Temperature".into(), format!("{:.0}", a.temperature)),
        ("crs:Tint".into(), signed(a.tint, 0)),
        ("crs:Exposure2012".into(), signed(a.exposure, 2)),
        ("crs:Contrast2012".into(), signed(a.contrast, 0)),
        ("crs:Highlights2012".into(), signed(a.highlights, 0)),
        ("crs:Shadows2012".into(), signed(a.shadows, 0)),
        ("crs:Whites2012".into(), signed(a.whites, 0)),
        ("crs:Blacks2012".into(), signed(a.blacks, 0)),
        ("crs:Vibrance".into(), signed(a.vibrance, 0)),
        ("crs:Saturation".into(), signed(a.saturation, 0)),
        ("crs:Sharpness".into(), format!("{:.0}", a.sharpening.max(0.0))),
        ("crs:LuminanceSmoothing".into(), format!("{:.0}", a.noise_reduction)),
        ("crs:PostCropVignetteAmount".into(), signed(-a.vignette, 0)),
        ("crs:LensManualDistortionAmount".into(), signed(a.distortion, 0)),
    ];
    for (i, band) in CRS_BANDS.iter().enumerate() {
        attrs.push((format!("crs:HueAdjustment{band}"), signed(a.hsl_hue[i], 0)));
        attrs.push((format!("crs:SaturationAdjustment{band}"), signed(a.hsl_saturation[i], 0)));
        attrs.push((format!("crs:LuminanceAdjustment{band}"), signed(a.hsl_luminance[i], 0)));
    }
    // Camera Raw's crop is in the unrotated frame; ours follows the 90° rotation, so only
    // an unrotated crop maps (the exact copy below always has it).
    let full = a.crop_x <= 0.0 && a.crop_y <= 0.0 && a.crop_width >= 1.0 && a.crop_height >= 1.0 && a.crop_angle == 0.0;
    if a.rotation == Rotation::R0 && !full {
        attrs.push(("crs:HasCrop".into(), "True".into()));
        attrs.push(("crs:CropLeft".into(), format_double(a.crop_x)));
        attrs.push(("crs:CropTop".into(), format_double(a.crop_y)));
        attrs.push(("crs:CropRight".into(), format_double(a.crop_x + a.crop_width)));
        attrs.push(("crs:CropBottom".into(), format_double(a.crop_y + a.crop_height)));
        attrs.push(("crs:CropAngle".into(), format_double(a.crop_angle)));
    } else {
        attrs.push(("crs:HasCrop".into(), "False".into()));
    }
    attrs.push(("awpr:ProcessVersion".into(), (a.pipeline_version + 1).to_string()));
    if let Some(e) = exif {
        if !e.camera_model.is_empty() {
            attrs.push(("tiff:Model".into(), e.camera_model.clone()));
        }
    }

    let curve = |name: &str, pts: &[(f64, f64)]| -> String {
        let pts: Vec<(f64, f64)> = if pts.len() >= 2 { pts.to_vec() } else { vec![(0.0, 0.0), (1.0, 1.0)] };
        let items: String = pts.iter().map(|&(x, y)| format!("     <rdf:li>{:.0}, {:.0}</rdf:li>\n", x * 255.0, y * 255.0)).collect();
        format!("   <{name}>\n    <rdf:Seq>\n{items}    </rdf:Seq>\n   </{name}>\n")
    };
    let exact = store::adjustments_node(a).to_document(XmlStyle::Swift);
    // Only the element itself, without the XML declaration.
    let exact = exact.find("<Adjustments").map(|i| exact[i..].trim_end().to_string()).unwrap_or_default();

    let mut s = String::new();
    s.push_str("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    s.push_str(&format!("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"AwayPhotoRawEditor {}\">\n", env!("CARGO_PKG_VERSION")));
    s.push_str(&format!(" <rdf:RDF xmlns:rdf=\"{RDF_NS}\">\n"));
    s.push_str(&format!("  <rdf:Description rdf:about=\"\"\n    xmlns:crs=\"{CRS_NS}\"\n    xmlns:tiff=\"http://ns.adobe.com/tiff/1.0/\"\n    xmlns:awpr=\"{AWPR_NS}\""));
    for (k, v) in &attrs {
        s.push_str(&format!("\n    {k}=\"{}\"", esc(v)));
    }
    s.push_str(">\n");
    s.push_str(&curve("crs:ToneCurvePV2012", &a.curve_rgb));
    s.push_str(&curve("crs:ToneCurvePV2012Red", &a.curve_red));
    s.push_str(&curve("crs:ToneCurvePV2012Green", &a.curve_green));
    s.push_str(&curve("crs:ToneCurvePV2012Blue", &a.curve_blue));
    s.push_str(&format!("   <awpr:Adjustments>{}</awpr:Adjustments>\n", esc(&exact)));
    s.push_str("  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>\n");
    s
}

/// 匯出 XMP for one photo: its stored adjustments to its sidecar. Returns the path.
pub fn export_sidecar(image: &str, copy_index: i32, adj: &ImageAdjustments, exif: Option<&ExifData>) -> std::io::Result<String> {
    let path = sidecar_path(image, copy_index);
    crate::paths::write_atomic(&path, write_xmp(adj, exif).as_bytes())?;
    Ok(path)
}

/// 匯入 XMP for one photo: the sidecar's adjustments on top of `base`. None when there
/// is no readable sidecar or it has no development settings.
pub fn import_sidecar(image: &str, copy_index: i32, base: &ImageAdjustments) -> Option<ImageAdjustments> {
    let bytes = std::fs::read(sidecar_path(image, copy_index)).ok()?;
    read_xmp(&String::from_utf8_lossy(&bytes), base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use awpr_core::{HealSpot, LinearGradient};

    fn edited() -> ImageAdjustments {
        let mut a = ImageAdjustments { exposure: 0.75, contrast: -12.0, temperature: 4800.0, tint: 7.0, vibrance: 20.0, highlight_recovery: 35.0, vignette: 25.0, ..Default::default() };
        a.hsl_hue[2] = -15.0;
        a.hsl_saturation[5] = 40.0;
        a.hsl_luminance[0] = -22.0;
        a.curve_rgb = vec![(0.0, 0.0), (0.25, 0.2), (0.75, 0.8), (1.0, 1.0)];
        a.curve_blue = vec![(0.0, 0.1), (1.0, 0.9)];
        (a.crop_x, a.crop_y, a.crop_width, a.crop_height, a.crop_angle) = (0.1, 0.05, 0.8, 0.7, 2.5);
        a.gradients.push(LinearGradient { exposure: -0.5, ..Default::default() });
        a.heal_spots.push(HealSpot { target_x: 0.3, source_x: 0.4, ..Default::default() });
        a
    }

    #[test]
    fn exact_round_trip_through_our_namespace() {
        let a = edited();
        let s = write_xmp(&a, None);
        let back = read_xmp(&s, &ImageAdjustments::default()).unwrap();
        assert_eq!(back, a);
        // An older-version photo keeps its version.
        let old = ImageAdjustments { pipeline_version: 1, exposure: 1.0, ..Default::default() };
        assert_eq!(read_xmp(&write_xmp(&old, None), &ImageAdjustments::default()).unwrap(), old);
    }

    #[test]
    fn camera_raw_fields_map_both_ways() {
        let a = edited();
        let s = write_xmp(&a, None);
        let p = properties(&s).unwrap();
        assert_eq!(p["crs:Exposure2012"], vec!["+0.75"]);
        assert_eq!(p["crs:Temperature"], vec!["4800"]);
        assert_eq!(p["crs:HueAdjustmentYellow"], vec!["-15"]);
        assert_eq!(p["crs:PostCropVignetteAmount"], vec!["-25"]);
        assert_eq!(p["crs:ToneCurvePV2012"].len(), 4);
        // Without our namespace (another program's sidecar) the crs fields alone rebuild
        // what they can describe.
        let foreign = s.split("   <awpr:Adjustments>").next().unwrap().to_string() + "  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>\n";
        let b = read_xmp(&foreign, &ImageAdjustments::default()).unwrap();
        assert_eq!((b.exposure, b.contrast, b.temperature, b.tint, b.vibrance, b.vignette), (0.75, -12.0, 4800.0, 7.0, 20.0, 25.0));
        assert_eq!((b.hsl_hue[2], b.hsl_saturation[5], b.hsl_luminance[0]), (-15.0, 40.0, -22.0));
        assert_eq!(b.curve_rgb.len(), 4);
        assert!((b.crop_x - 0.1).abs() < 1e-12 && (b.crop_width - 0.8).abs() < 1e-12 && b.crop_angle == 2.5);
        assert!(b.heal_spots.is_empty()); // not describable in crs
    }

    #[test]
    fn prefixes_are_resolved_by_namespace() {
        let s = format!(
            "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"{RDF_NS}\"><rdf:Description xmlns:cr=\"{CRS_NS}\" cr:Exposure2012=\"-1.50\"><cr:Contrast2012>30</cr:Contrast2012></rdf:Description></rdf:RDF></x:xmpmeta>"
        );
        let a = read_xmp(&s, &ImageAdjustments::default()).unwrap();
        assert_eq!((a.exposure, a.contrast), (-1.5, 30.0));
        assert!(read_xmp("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>", &ImageAdjustments::default()).is_none());
    }

    #[test]
    fn sidecar_names() {
        let p = if cfg!(windows) { r"D:\p\DSC_1.NEF" } else { "/p/DSC_1.NEF" };
        assert!(sidecar_path(p, 0).ends_with("DSC_1.xmp"));
        assert!(sidecar_path(p, 2).ends_with("DSC_1.copy2.xmp"));
    }
}
