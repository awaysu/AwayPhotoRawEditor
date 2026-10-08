//! Look presets (風格檔): the built-in list (`PresetProfile.BuiltIn`) and the user's
//! overrides / custom presets in `presets.xml` (`PresetStore`), same format as the C# and
//! Swift builds. The Rust build keeps its own `presets.rust.xml` and starts from theirs.

use crate::edits;
use crate::paths;
use crate::store;
use crate::xml::{self, XmlNode, XmlStyle};
use awpr_core::ImageAdjustments;
use std::path::Path;

/// 預設時設定: a full reset, not editable.
pub const DEFAULT_NAME: &str = "預設時設定";

/// Built-in presets, in list order.
pub const BUILT_IN_NAMES: [&str; 9] = [DEFAULT_NAME, "風景", "人像", "鮮豔", "黑白", "柔和", "自訂1", "自訂2", "自訂3"];

pub fn is_builtin(name: &str) -> bool {
    BUILT_IN_NAMES.contains(&name)
}

/// A built-in preset's own values onto `a` (`PresetProfile.ApplyTo`). False for an
/// unknown name.
pub fn apply_builtin(name: &str, a: &mut ImageAdjustments) -> bool {
    if name == DEFAULT_NAME {
        edits::reset_all(a);
        return true;
    }
    if !is_builtin(name) {
        return false;
    }
    edits::reset_tonal(a);
    match name {
        "風景" => {
            (a.contrast, a.highlights, a.shadows, a.whites, a.blacks) = (18.0, -20.0, 12.0, 8.0, -10.0);
            (a.vibrance, a.saturation) = (28.0, 8.0);
            (a.sharpening, a.noise_reduction) = (25.0, 5.0);
        }
        "人像" => {
            // Exposure in true EV (the linear pipeline).
            (a.exposure, a.contrast, a.highlights, a.shadows) = (0.2, -6.0, -12.0, 18.0);
            (a.vibrance, a.saturation) = (10.0, -4.0);
            (a.sharpening, a.noise_reduction) = (5.0, 12.0);
        }
        "鮮豔" => {
            (a.contrast, a.highlights, a.whites, a.blacks) = (14.0, -10.0, 8.0, -8.0);
            (a.vibrance, a.saturation, a.sharpening) = (38.0, 16.0, 18.0);
        }
        "黑白" => {
            (a.contrast, a.highlights, a.shadows, a.whites, a.blacks) = (22.0, -15.0, 8.0, 10.0, -16.0);
            (a.saturation, a.sharpening) = (-100.0, 20.0);
        }
        "柔和" => {
            (a.exposure, a.contrast, a.highlights, a.shadows, a.blacks) = (0.3, -14.0, -18.0, 24.0, 6.0);
            (a.vibrance, a.saturation) = (6.0, -6.0);
            (a.sharpening, a.noise_reduction) = (-10.0, 15.0);
        }
        _ => {} // 自訂1–3: empty until the user saves them
    }
    true
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamedPreset {
    pub name: String,
    pub adjustments: ImageAdjustments,
}

/// The contents of presets.xml.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PresetCollection {
    pub items: Vec<NamedPreset>,
}

impl PresetCollection {
    /// None when the document is not a preset collection.
    pub fn parse(src: &str) -> Option<Self> {
        let root = xml::parse(src)?;
        if root.name != "Presets" {
            return None;
        }
        let items = root
            .child("Items")
            .map(|items| {
                items
                    .children_named("NamedPreset")
                    .filter_map(|n| {
                        let name = n.string("Name").filter(|s| !s.is_empty())?.to_string();
                        let adjustments = n.child("Adjustments").map(store::adjustments_from_node).unwrap_or_default();
                        Some(NamedPreset { name, adjustments })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self { items })
    }

    pub fn to_xml(&self, style: XmlStyle) -> String {
        let mut root = XmlNode::new("Presets");
        let items = root.add(XmlNode::new("Items"));
        for p in &self.items {
            let n = items.add(XmlNode::new("NamedPreset"));
            n.add_str("Name", &p.name);
            n.add(store::adjustments_node(&p.adjustments));
        }
        root.to_document(style)
    }

    /// From `dir`: presets.rust.xml, else the C# / Swift presets.xml, else empty.
    pub fn load_from(dir: &Path) -> Self {
        [dir.join("presets.rust.xml"), dir.join("presets.xml")]
            .iter()
            .find_map(|p| std::fs::read(p).ok().and_then(|b| Self::parse(&String::from_utf8_lossy(&b))))
            .unwrap_or_default()
    }

    pub fn save_to(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        paths::write_atomic(&dir.join("presets.rust.xml").to_string_lossy(), self.to_xml(XmlStyle::native()).as_bytes())
    }

    pub fn load() -> Self {
        Self::load_from(&paths::app_data_dir())
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&paths::app_data_dir())
    }

    pub fn get(&self, name: &str) -> Option<&ImageAdjustments> {
        self.items.iter().find(|p| p.name == name).map(|p| &p.adjustments)
    }

    /// Save (or overwrite) a preset from a full adjustment set.
    pub fn set(&mut self, name: &str, adjustments: &ImageAdjustments) {
        self.items.retain(|p| p.name != name);
        self.items.push(NamedPreset { name: name.to_string(), adjustments: adjustments.clone() });
    }

    /// Drop a preset (a built-in's override reverts it to its default). True if it existed.
    pub fn remove(&mut self, name: &str) -> bool {
        let n = self.items.len();
        self.items.retain(|p| p.name != name);
        self.items.len() != n
    }

    /// Names that are not built-ins (the editor's own presets), in file order.
    pub fn custom_names(&self) -> Vec<String> {
        self.items.iter().filter(|p| !is_builtin(&p.name)).map(|p| p.name.clone()).collect()
    }

    /// A stored preset's tonal fields onto `target` (`PresetStore.ApplyCustom`). The
    /// white balance is never copied.
    pub fn apply_stored(&self, name: &str, target: &mut ImageAdjustments) -> bool {
        let Some(p) = self.get(name) else { return false };
        edits::reset_tonal(target);
        (target.exposure, target.contrast, target.highlights, target.shadows, target.whites, target.blacks) = (p.exposure, p.contrast, p.highlights, p.shadows, p.whites, p.blacks);
        (target.vibrance, target.saturation) = (p.vibrance, p.saturation);
        (target.sharpening, target.noise_reduction, target.vignette, target.distortion) = (p.sharpening, p.noise_reduction, p.vignette, p.distortion);
        true
    }

    /// Apply a preset by name: the user's saved values first, else the built-in ones.
    pub fn apply(&self, name: &str, target: &mut ImageAdjustments) -> bool {
        self.apply_stored(name, target) || apply_builtin(name, target)
    }

    /// The values a preset currently stands for (what the editor shows).
    pub fn effective(&self, name: &str) -> ImageAdjustments {
        let mut a = ImageAdjustments::default();
        self.apply(name, &mut a);
        a
    }

    /// The editor's commit: store the values, or drop the override when a built-in is
    /// back to exactly its default.
    pub fn commit(&mut self, name: &str, adjustments: &ImageAdjustments) {
        if is_builtin(name) {
            let mut builtin = ImageAdjustments::default();
            apply_builtin(name, &mut builtin);
            if store::value_equals(adjustments, &builtin) {
                self.remove(name);
                return;
            }
        }
        self.set(name, adjustments);
    }

    /// 備份全部: the whole collection to a file (C# XmlSerializer style, so either build
    /// can restore it).
    pub fn export_to(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_xml(XmlStyle::DotNet))
    }

    /// 還原全部: read a backup. Err for an unreadable file, Ok(None) for XML that is not
    /// a preset collection.
    pub fn import_from(path: &Path) -> std::io::Result<Option<Self>> {
        let b = std::fs::read(path)?;
        Ok(Self::parse(&String::from_utf8_lossy(&b)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        let p = format!("{}/../../tests/fixtures/xml/csharp/presets.xml", env!("CARGO_MANIFEST_DIR"));
        String::from_utf8(std::fs::read(&p).unwrap()).unwrap()
    }

    #[test]
    fn csharp_presets_round_trip_byte_identical() {
        let src = fixture();
        let c = PresetCollection::parse(&src).expect("parses");
        assert_eq!(c.items.len(), 2);
        assert_eq!(c.items[0].name, "自訂1");
        assert_eq!(c.items[1].name, "我的夜景");
        assert_eq!(c.custom_names(), vec!["我的夜景".to_string()]);
        assert_eq!(c.to_xml(XmlStyle::DotNet), src);
        // Swift style reads back to the same values.
        assert_eq!(PresetCollection::parse(&c.to_xml(XmlStyle::Swift)).unwrap(), c);
        assert!(PresetCollection::parse("<PreviewList />").is_none());
    }

    #[test]
    fn builtins_match_csharp_and_keep_white_balance() {
        let mut a = ImageAdjustments { temperature: 3500.0, tint: 20.0, exposure: 1.5, crop_x: 0.2, ..Default::default() };
        assert!(apply_builtin("風景", &mut a));
        assert_eq!((a.exposure, a.contrast, a.highlights, a.shadows, a.whites, a.blacks), (0.0, 18.0, -20.0, 12.0, 8.0, -10.0));
        assert_eq!((a.vibrance, a.saturation, a.sharpening, a.noise_reduction), (28.0, 8.0, 25.0, 5.0));
        assert_eq!((a.temperature, a.tint, a.crop_x), (3500.0, 20.0, 0.2));
        assert!(apply_builtin("黑白", &mut a));
        assert_eq!((a.saturation, a.vibrance, a.contrast), (-100.0, 0.0, 22.0));
        assert!(apply_builtin(DEFAULT_NAME, &mut a));
        assert_eq!((a.temperature, a.crop_x), (5200.0, 0.0)); // full reset
        assert!(!apply_builtin("nope", &mut a));
    }

    #[test]
    fn overrides_and_custom_presets() {
        let mut c = PresetCollection::default();
        let mut v = ImageAdjustments { contrast: 50.0, temperature: 9000.0, tint: -30.0, ..Default::default() };
        c.set("風景", &v);
        let mut target = ImageAdjustments { temperature: 4000.0, ..Default::default() };
        assert!(c.apply("風景", &mut target));
        assert_eq!((target.contrast, target.highlights, target.temperature, target.tint), (50.0, 0.0, 4000.0, 0.0)); // override, WB untouched
        // Back to exactly the built-in values: the override is dropped.
        v = c.effective("鮮豔");
        c.commit("鮮豔", &v);
        assert!(c.get("鮮豔").is_none());
        c.commit("我的", &ImageAdjustments { exposure: 0.7, ..Default::default() });
        assert_eq!(c.custom_names(), vec!["我的".to_string()]);
        assert!(c.remove("我的") && !c.remove("我的"));
        // Backup / restore through a file.
        let dir = std::env::temp_dir().join(format!("awpr_presets_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        c.export_to(&dir.join("backup.xml")).unwrap();
        assert_eq!(PresetCollection::import_from(&dir.join("backup.xml")).unwrap(), Some(c.clone()));
        std::fs::write(dir.join("other.xml"), "<?xml version=\"1.0\"?><Something />").unwrap();
        assert_eq!(PresetCollection::import_from(&dir.join("other.xml")).unwrap(), None);
        // The Rust file wins over the C# one.
        std::fs::write(dir.join("presets.xml"), fixture()).unwrap();
        assert_eq!(PresetCollection::load_from(&dir).items.len(), 2);
        c.save_to(&dir).unwrap();
        assert_eq!(PresetCollection::load_from(&dir), c);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
