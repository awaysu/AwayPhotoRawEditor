//! Supported formats, the per-folder `RAW_TEMP` cache and its file names. Port of
//! `AppPaths` (identical naming in the C# and Swift builds, so a folder's caches and
//! edits are shared by all three).

use std::path::{Path, PathBuf};

pub const RAW_TEMP: &str = "RAW_TEMP";
pub const PREVIEW_LIST: &str = "preview_list.xml";

/// Camera RAW formats.
pub const RAW_EXTENSIONS: &[&str] =
    &["arw", "sr2", "srf", "cr2", "cr3", "crw", "nef", "nrw", "raf", "rw2", "orf", "pef", "dng"];

/// Regular bitmap formats. HEIC is listed like the Swift build does; whether it decodes
/// depends on the build (see `decode::heic_supported`).
pub const REGULAR_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "tif", "tiff", "bmp", "heic", "heif"];

pub fn ext(path: &str) -> String {
    Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

pub fn is_raw(path: &str) -> bool {
    RAW_EXTENSIONS.contains(&ext(path).as_str())
}

pub fn is_supported(path: &str) -> bool {
    let e = ext(path);
    RAW_EXTENSIONS.contains(&e.as_str()) || REGULAR_EXTENSIONS.contains(&e.as_str())
}

pub fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn join(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

pub fn raw_temp_dir(folder: &str) -> PathBuf {
    Path::new(folder).join(RAW_TEMP)
}

fn cache_dir(image: &str) -> PathBuf {
    raw_temp_dir(&Path::new(image).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
}

/// RAW_TEMP/{file}_thumb.jpg
pub fn thumbnail_path(image: &str) -> String {
    join(&cache_dir(image), &(file_name(image) + "_thumb.jpg"))
}

/// RAW_TEMP/{file}.rawpipe.png
pub fn proxy_path(image: &str) -> String {
    join(&cache_dir(image), &(file_name(image) + ".rawpipe.png"))
}

/// RAW_TEMP/{file}.rawpipe.png.f16 — the lossless 16-bit proxy.
pub fn proxy_f16_path(image: &str) -> String {
    proxy_path(image) + ".f16"
}

/// RAW_TEMP/{file}.rawpipe.png.src — how the proxy was decoded (Swift build's marker).
pub fn proxy_source_path(image: &str) -> String {
    proxy_path(image) + ".src"
}

/// RAW_TEMP/{file}.rawpipe.png.thumb.jpg — strip thumbnail cut from the proxy (RAW only),
/// so the strip starts from the editor's own pixels rather than the camera's preview.
pub fn proxy_thumbnail_path(image: &str) -> String {
    proxy_path(image) + ".thumb.jpg"
}

/// RAW_TEMP/{file}.rawpipe.v3.png — 處理版本 3's linear camera RGB proxy (16-bit,
/// square-root encoded; see `codec::save_linear_png`). Rust build only.
pub fn proxy_v3_path(image: &str) -> String {
    join(&cache_dir(image), &(file_name(image) + ".rawpipe.v3.png"))
}

/// RAW_TEMP/{file}.rawpipe.v3.png.txt — the proxy's auto-bright gain (`gain=…`), written
/// after the PNG.
pub fn proxy_v3_meta_path(image: &str) -> String {
    proxy_v3_path(image) + ".txt"
}

/// RAW_TEMP/{file}.rawpipe.v3.png.thumb.png — the linear proxy shrunk to strip-thumbnail
/// size (same encoding), so a version-3 thumbnail renders from the linear source cheaply.
pub fn proxy_v3_thumbnail_path(image: &str) -> String {
    proxy_v3_path(image) + ".thumb.png"
}

/// RAW_TEMP/{file}.rawpipe.xml, or {file}.copyN.rawpipe.xml for a virtual copy.
pub fn adjustment_xml_path(image: &str, copy_index: i32) -> String {
    let suffix = if copy_index <= 0 { ".rawpipe.xml".to_string() } else { format!(".copy{copy_index}.rawpipe.xml") };
    join(&cache_dir(image), &(file_name(image) + &suffix))
}

pub fn preview_list_path(folder: &str) -> String {
    join(&raw_temp_dir(folder), PREVIEW_LIST)
}

/// Every supported image in a folder, sorted the way the strip shows them (natural,
/// case-insensitive: `IMG_2` before `IMG_10`, like Finder and Explorer), skipping
/// hidden files and the RAW_TEMP cache.
pub fn images_in_folder(folder: &str) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut names: Vec<String> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && is_supported(n))
        .collect();
    names.sort_by(|a, b| natural_cmp(a, b));
    names.into_iter().map(|n| join(Path::new(folder), &n)).collect()
}

/// Case-insensitive comparison with digit runs compared by value.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut ia, mut ib) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ia.peek().copied(), ib.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ia.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ia.next();
                }
                let mut nb = String::new();
                while let Some(c) = ib.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    ib.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb)).then_with(|| na.len().cmp(&nb.len()));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(ca), Some(cb)) => {
                if ca != cb {
                    return ca.cmp(&cb);
                }
                ia.next();
                ib.next();
            }
        }
    }
}

/// Write a file atomically (temp file + rename), creating the directory.
pub fn write_atomic(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    let p = Path::new(path);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = format!("{path}.tmp{}", std::process::id());
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, p).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Remove temp files `write_atomic` left behind when the program quit mid-write
/// (`<name>.tmp<pid>` in RAW_TEMP; nothing else uses that pattern).
pub fn cleanup_stale_temp(folder: &str) {
    let Ok(rd) = std::fs::read_dir(raw_temp_dir(folder)) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        let name = e.file_name().to_string_lossy().into_owned();
        let stale = name.rfind(".tmp").is_some_and(|i| {
            let tail = &name[i + 4..];
            !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit())
        });
        if stale {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Where settings and presets live: %AppData%\AwayPhotoRawEditor on Windows,
/// ~/Library/Application Support/AwayPhotoRawEditor on macOS (both the native builds'
/// folders), $XDG_CONFIG_HOME/AwayPhotoRawEditor on Linux.
pub fn app_data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("AwayPhotoRawEditor")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["IMG_10.ARW", "img_2.arw", "IMG_1.ARW", "a.jpg", "IMG_02.ARW"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["a.jpg", "IMG_1.ARW", "img_2.arw", "IMG_02.ARW", "IMG_10.ARW"]);
    }

    #[test]
    fn cache_names() {
        let p = if cfg!(windows) { r"D:\p\DSC_1.NEF" } else { "/p/DSC_1.NEF" };
        assert!(adjustment_xml_path(p, 0).ends_with("RAW_TEMP/DSC_1.NEF.rawpipe.xml".replace('/', std::path::MAIN_SEPARATOR_STR).as_str()));
        assert!(adjustment_xml_path(p, 2).ends_with("DSC_1.NEF.copy2.rawpipe.xml"));
        assert!(thumbnail_path(p).ends_with("DSC_1.NEF_thumb.jpg"));
    }
}
