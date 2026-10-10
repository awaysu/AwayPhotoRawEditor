//! Settings (the C# `AppSettings`). The first run inherits the installed native build's
//! `settings.xml` (same folder: %AppData%\AwayPhotoRawEditor / ~/Library/Application
//! Support/…); the Rust build then keeps its own `settings.rust.xml` beside it, so it never
//! rewrites the C# / Swift file. Element names and order follow the C# class.

use crate::i18n::Lang;
use awpr_photo::paths;
use awpr_photo::xml::{self, XmlNode, XmlStyle};
use std::path::{Path, PathBuf};

/// The twelve UI type sizes, in whole pixels at 100% (`FontSizes`). The user tunes them in
/// 字體大小…; the interface size scales all of them on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontSizes {
    pub small: i64,
    pub mono: i64,
    pub normal: i64,
    pub section_title: i64,
    pub about_body: i64,
    pub folder_glyph: i64,
    pub icon_glyph: i64,
    pub progress_title: i64,
    pub dialog_title: i64,
    pub about_title: i64,
    pub logo: i64,
    pub menu_glyph: i64,
}

impl Default for FontSizes {
    fn default() -> Self {
        Self { small: 15, mono: 11, normal: 15, section_title: 16, about_body: 16, folder_glyph: 15, icon_glyph: 16, progress_title: 16, dialog_title: 17, about_title: 22, logo: 22, menu_glyph: 27 }
    }
}

/// One of the twelve sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontKind {
    Small,
    Mono,
    Normal,
    SectionTitle,
    AboutBody,
    FolderGlyph,
    IconGlyph,
    ProgressTitle,
    DialogTitle,
    AboutTitle,
    Logo,
    MenuGlyph,
}

impl FontKind {
    pub const ALL: [FontKind; 12] = [
        Self::Small,
        Self::Mono,
        Self::Normal,
        Self::SectionTitle,
        Self::AboutBody,
        Self::FolderGlyph,
        Self::IconGlyph,
        Self::ProgressTitle,
        Self::DialogTitle,
        Self::AboutTitle,
        Self::Logo,
        Self::MenuGlyph,
    ];

    pub fn xml_name(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Mono => "Mono",
            Self::Normal => "Normal",
            Self::SectionTitle => "SectionTitle",
            Self::AboutBody => "AboutBody",
            Self::FolderGlyph => "FolderGlyph",
            Self::IconGlyph => "IconGlyph",
            Self::ProgressTitle => "ProgressTitle",
            Self::DialogTitle => "DialogTitle",
            Self::AboutTitle => "AboutTitle",
            Self::Logo => "Logo",
            Self::MenuGlyph => "MenuGlyph",
        }
    }
}

impl FontSizes {
    pub const MIN_PX: i64 = 8;
    pub const MAX_PX: i64 = 48;

    pub fn get(&self, k: FontKind) -> i64 {
        match k {
            FontKind::Small => self.small,
            FontKind::Mono => self.mono,
            FontKind::Normal => self.normal,
            FontKind::SectionTitle => self.section_title,
            FontKind::AboutBody => self.about_body,
            FontKind::FolderGlyph => self.folder_glyph,
            FontKind::IconGlyph => self.icon_glyph,
            FontKind::ProgressTitle => self.progress_title,
            FontKind::DialogTitle => self.dialog_title,
            FontKind::AboutTitle => self.about_title,
            FontKind::Logo => self.logo,
            FontKind::MenuGlyph => self.menu_glyph,
        }
    }

    pub fn set(&mut self, k: FontKind, v: i64) {
        let v = v.clamp(Self::MIN_PX, Self::MAX_PX);
        match k {
            FontKind::Small => self.small = v,
            FontKind::Mono => self.mono = v,
            FontKind::Normal => self.normal = v,
            FontKind::SectionTitle => self.section_title = v,
            FontKind::AboutBody => self.about_body = v,
            FontKind::FolderGlyph => self.folder_glyph = v,
            FontKind::IconGlyph => self.icon_glyph = v,
            FontKind::ProgressTitle => self.progress_title = v,
            FontKind::DialogTitle => self.dialog_title = v,
            FontKind::AboutTitle => self.about_title = v,
            FontKind::Logo => self.logo = v,
            FontKind::MenuGlyph => self.menu_glyph = v,
        }
    }

    fn from_xml(n: Option<&XmlNode>) -> Self {
        let mut f = Self::default();
        if let Some(n) = n {
            for k in FontKind::ALL {
                let v = n.i64_or(k.xml_name(), f.get(k));
                f.set(k, v);
            }
        }
        f
    }
}

/// Interface style; only the classic dark one is drawn by this build, but the value is
/// kept so the C# setting survives a round trip.
pub const INTERFACE_STYLES: [&str; 2] = ["ClassicDark", "WarmPaper"];

/// Folders kept in 紀錄 (the menu shows the first ten).
const RECENT_LIMIT: usize = 20;

/// 介面大小 custom range (1.1.1).
pub const UI_SCALE_MIN: i64 = 60;
pub const UI_SCALE_MAX: i64 = 150;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub use_libraw: bool,
    pub high_precision: bool,
    pub use_gpu: bool,
    pub show_thumbnail_number: bool,
    pub show_column_scroll_bars: bool,
    pub show_hidden: bool,
    pub interface_style: String,
    pub language: Lang,
    /// 0 = automatic (fit the screen), else 100–200.
    /// 介面大小 in percent, 0 = 自動; a non-zero value is within 60–150.
    pub ui_scale_percent: i64,
    pub font_sizes: FontSizes,
    pub last_folder: String,
    pub recent_folders: Vec<String>,
    /// 支援 XMP: shows 匯出／匯入 XMP in the menus (Rust build only; off by default).
    pub xmp_support: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            use_libraw: true,
            high_precision: false,
            use_gpu: true,
            show_thumbnail_number: true,
            show_column_scroll_bars: false,
            show_hidden: false,
            interface_style: "ClassicDark".into(),
            language: Lang::ZhTw,
            ui_scale_percent: 0,
            font_sizes: FontSizes::default(),
            last_folder: String::new(),
            recent_folders: Vec::new(),
            xmp_support: false,
        }
    }
}

impl Settings {
    fn files(dir: &Path) -> [PathBuf; 2] {
        [dir.join("settings.rust.xml"), dir.join("settings.xml")]
    }

    /// True when neither this build nor the C# / Swift one has saved settings yet: the
    /// first run, which asks for the language.
    pub fn is_first_run() -> bool {
        Self::files(&paths::app_data_dir()).iter().all(|p| !p.exists())
    }

    pub fn load() -> Self {
        Self::load_from(&paths::app_data_dir())
    }

    pub fn load_from(dir: &Path) -> Self {
        Self::files(dir)
            .iter()
            .find_map(|p| std::fs::read(p).ok().and_then(|b| xml::parse(&String::from_utf8_lossy(&b))))
            .map(|r| Self::from_xml(&r))
            .unwrap_or_default()
    }

    pub fn from_xml(r: &XmlNode) -> Self {
        let d = Self::default();
        let style = r.string_or("InterfaceStyle", &d.interface_style);
        Self {
            use_libraw: r.bool_or("UseLibRaw", d.use_libraw),
            high_precision: r.bool_or("UseHighPrecisionRawPipeline", d.high_precision),
            use_gpu: r.bool_or("UseGpu", d.use_gpu),
            show_thumbnail_number: r.bool_or("ShowThumbnailNumber", d.show_thumbnail_number),
            show_column_scroll_bars: r.bool_or("ShowColumnScrollBars", d.show_column_scroll_bars),
            show_hidden: r.bool_or("ShowHiddenPhotos", d.show_hidden),
            interface_style: if INTERFACE_STYLES.contains(&style.as_str()) { style } else { d.interface_style },
            language: r.string("UiLanguage").and_then(Lang::from_xml).unwrap_or(d.language),
            ui_scale_percent: {
                // 0 = 自動; anything outside 60–150 (a 175 / 200 from 1.1.0) is 自動 too.
                let v = r.i64_or("UiScalePercent", 0);
                if (UI_SCALE_MIN..=UI_SCALE_MAX).contains(&v) { v } else { 0 }
            },
            font_sizes: FontSizes::from_xml(r.child("FontSizes")),
            last_folder: r.string_or("LastFolder", ""),
            recent_folders: r.child("RecentFolders").map(|n| n.children_named("string").filter_map(|s| s.text.clone()).filter(|s| !s.is_empty()).collect()).unwrap_or_default(),
            xmp_support: r.bool_or("XmpSupport", d.xmp_support),
        }
    }

    pub fn to_xml(&self, style: XmlStyle) -> String {
        let mut root = XmlNode::new("AppSettings");
        root.add_bool("UseLibRaw", self.use_libraw);
        root.add_bool("UseHighPrecisionRawPipeline", self.high_precision);
        root.add_bool("UseGpu", self.use_gpu);
        root.add_bool("ShowThumbnailNumber", self.show_thumbnail_number);
        root.add_bool("ShowColumnScrollBars", self.show_column_scroll_bars);
        root.add_bool("ShowHiddenPhotos", self.show_hidden);
        root.add_str("InterfaceStyle", &self.interface_style);
        root.add_str("UiLanguage", self.language.xml_name());
        root.add_i64("UiScalePercent", self.ui_scale_percent);
        let f = root.add(XmlNode::new("FontSizes"));
        for k in FontKind::ALL {
            f.add_i64(k.xml_name(), self.font_sizes.get(k));
        }
        root.add_str("LastFolder", &self.last_folder);
        let r = root.add(XmlNode::new("RecentFolders"));
        for p in &self.recent_folders {
            r.add_str("string", p);
        }
        // After the C# fields, so their order stays as the C# class writes it.
        root.add_bool("XmpSupport", self.xmp_support);
        root.to_document(style)
    }

    pub fn save(&self) {
        let dir = paths::app_data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let _ = paths::write_atomic(&dir.join("settings.rust.xml").to_string_lossy(), self.to_xml(XmlStyle::native()).as_bytes());
    }

    /// Put a folder first in 紀錄 (no duplicates, case-insensitively).
    pub fn push_recent_folder(&mut self, folder: &str) {
        if folder.trim().is_empty() {
            return;
        }
        self.recent_folders.retain(|f| !f.eq_ignore_ascii_case(folder));
        self.recent_folders.insert(0, folder.to_string());
        self.recent_folders.truncate(RECENT_LIMIT);
    }

    pub fn loader_options(&self) -> awpr_photo::loader::LoaderOptions {
        awpr_photo::loader::LoaderOptions { use_libraw: self.use_libraw, high_precision: self.high_precision }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_csharp_fields() {
        let mut s = Settings { language: Lang::De, ui_scale_percent: 150, show_thumbnail_number: false, high_precision: true, xmp_support: true, ..Default::default() };
        s.font_sizes.set(FontKind::Normal, 18);
        s.font_sizes.set(FontKind::Logo, 99); // clamped to 48
        s.push_recent_folder(r"D:\a");
        s.push_recent_folder(r"D:\b");
        s.push_recent_folder(r"d:\A"); // moves to the front, no duplicate
        assert_eq!(s.recent_folders, vec![r"d:\A".to_string(), r"D:\b".to_string()]);
        assert_eq!(s.font_sizes.logo, 48);
        let x = s.to_xml(XmlStyle::DotNet);
        // Element order = the C# property order.
        let order = ["UseLibRaw", "UseHighPrecisionRawPipeline", "UseGpu", "ShowThumbnailNumber", "ShowColumnScrollBars", "ShowHiddenPhotos", "InterfaceStyle", "UiLanguage", "UiScalePercent", "FontSizes", "LastFolder", "RecentFolders", "XmpSupport"];
        let pos: Vec<usize> = order.iter().map(|t| x.find(&format!("<{t}")).unwrap_or_else(|| panic!("{t}"))).collect();
        assert!(pos.windows(2).all(|w| w[0] < w[1]));
        assert!(x.contains("<UiLanguage>German</UiLanguage>") && x.contains("<Normal>18</Normal>"));
        assert_eq!(Settings::from_xml(&xml::parse(&x).unwrap()), s);
        // A C# file from before FontSizes / RecentFolders existed keeps the defaults.
        let old = xml::parse("<AppSettings><UseGpu>false</UseGpu><UiLanguage>Japanese</UiLanguage><UiScalePercent>999</UiScalePercent></AppSettings>").unwrap();
        let o = Settings::from_xml(&old);
        assert_eq!((o.use_gpu, o.language, o.ui_scale_percent, o.font_sizes, o.xmp_support), (false, Lang::Ja, 0, FontSizes::default(), false));
    }

    #[test]
    fn ui_scale_out_of_range_loads_as_auto() {
        let load = |v: i64| Settings::from_xml(&xml::parse(&format!("<AppSettings><UiScalePercent>{v}</UiScalePercent></AppSettings>")).unwrap()).ui_scale_percent;
        assert_eq!((load(0), load(60), load(95), load(150)), (0, 60, 95, 150));
        assert_eq!((load(59), load(151), load(175), load(200), load(-5)), (0, 0, 0, 0, 0));
    }
}
