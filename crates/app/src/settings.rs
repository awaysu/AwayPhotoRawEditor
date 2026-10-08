//! Settings. The first run inherits the installed native build's `settings.xml` (same
//! folder: %AppData%\AwayPhotoRawEditor / ~/Library/Application Support/…); the Rust
//! build then keeps its own `settings.rust.xml` beside it, so it never rewrites the C# /
//! Swift file and drops fields it does not know yet.

use awpr_photo::paths;
use awpr_photo::xml::{self, XmlNode, XmlStyle};

#[derive(Debug, Clone)]
pub struct Settings {
    pub use_libraw: bool,
    pub high_precision: bool,
    pub use_gpu: bool,
    pub show_hidden: bool,
    pub last_folder: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { use_libraw: true, high_precision: false, use_gpu: true, show_hidden: false, last_folder: String::new() }
    }
}

fn own_path() -> std::path::PathBuf {
    paths::app_data_dir().join("settings.rust.xml")
}

impl Settings {
    pub fn load() -> Self {
        let dir = paths::app_data_dir();
        let root = [own_path(), dir.join("settings.xml")]
            .iter()
            .find_map(|p| std::fs::read(p).ok().and_then(|b| xml::parse(&String::from_utf8_lossy(&b))));
        let d = Self::default();
        let Some(r) = root else { return d };
        Self {
            use_libraw: r.bool_or("UseLibRaw", d.use_libraw),
            high_precision: r.bool_or("UseHighPrecisionRawPipeline", d.high_precision),
            use_gpu: r.bool_or("UseGpu", d.use_gpu),
            show_hidden: r.bool_or("ShowHiddenPhotos", d.show_hidden),
            last_folder: r.string_or("LastFolder", ""),
        }
    }

    pub fn save(&self) {
        let mut root = XmlNode::new("AppSettings");
        root.add_bool("UseLibRaw", self.use_libraw);
        root.add_bool("UseHighPrecisionRawPipeline", self.high_precision);
        root.add_bool("UseGpu", self.use_gpu);
        root.add_bool("ShowHiddenPhotos", self.show_hidden);
        root.add_str("LastFolder", &self.last_folder);
        let _ = paths::write_atomic(&own_path().to_string_lossy(), root.to_document(XmlStyle::native()).as_bytes());
    }

    pub fn loader_options(&self) -> awpr_photo::loader::LoaderOptions {
        awpr_photo::loader::LoaderOptions { use_libraw: self.use_libraw, high_precision: self.high_precision }
    }
}
