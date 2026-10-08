//! Folder-level photo management on top of `preview_list.xml`: hiding, virtual copies and
//! deleting a photo with its caches (the C# `MainForm` hide / copy / delete handlers).

use crate::paths;
use crate::store::{self, PreviewList, VirtualCopyEntry};

/// 隱藏且不輸出.
pub fn hide(list: &mut PreviewList, key: &str) {
    if !list.hidden.iter().any(|k| k == key) {
        list.hidden.push(key.to_string());
    }
}

/// 取消隱藏.
pub fn unhide(list: &mut PreviewList, key: &str) {
    list.hidden.retain(|k| k != key);
}

/// The index a new virtual copy of `path` gets: one past the highest in use (paths
/// compared without case, like the C# build on Windows).
pub fn next_copy_index<'a>(existing: impl IntoIterator<Item = (&'a str, i32)>, path: &str) -> i32 {
    existing.into_iter().filter(|(p, _)| p.eq_ignore_ascii_case(path)).map(|(_, i)| i).max().unwrap_or(0) + 1
}

/// Rewrite the virtual-copy list from the copies on the strip (`SavePreviewList`). Copies
/// that are hidden — and so missing from the strip while hidden photos are not shown —
/// are kept, or 取消隱藏 would find them gone.
pub fn rewrite_virtual_copies(list: &mut PreviewList, on_strip: &[(String, i32)]) {
    let keys: Vec<String> = on_strip.iter().map(|(p, i)| store::make_key(p, *i)).collect();
    let kept: Vec<VirtualCopyEntry> = list
        .virtual_copies
        .iter()
        .filter(|v| {
            let k = store::make_key(&v.path, v.index);
            !keys.contains(&k) && list.hidden.contains(&k)
        })
        .cloned()
        .collect();
    list.virtual_copies = on_strip.iter().map(|(p, i)| VirtualCopyEntry { path: p.clone(), index: *i }).chain(kept).collect();
}

/// Remove a virtual copy for good: its entry, its hidden flag and its XML.
pub fn remove_virtual_copy(list: &mut PreviewList, path: &str, index: i32) {
    let key = store::make_key(path, index);
    list.virtual_copies.retain(|v| !(v.path == path && v.index == index));
    unhide(list, &key);
    let _ = std::fs::remove_file(paths::adjustment_xml_path(path, index));
}

/// Every RAW_TEMP file that belongs to a photo (caches and the XMLs of the original and
/// the given copies).
pub fn cache_files(path: &str, copies: &[i32]) -> Vec<String> {
    let mut v = vec![paths::thumbnail_path(path), paths::proxy_path(path), paths::proxy_f16_path(path), paths::proxy_source_path(path), paths::proxy_thumbnail_path(path), paths::adjustment_xml_path(path, 0)];
    v.extend(copies.iter().filter(|&&c| c > 0).map(|&c| paths::adjustment_xml_path(path, c)));
    v
}

/// 刪除檔案: the photo goes to the recycle bin / trash (recoverable), its caches and XMLs
/// are deleted (they can be regenerated), and its virtual copies leave the list.
pub fn delete_photo(list: &mut PreviewList, path: &str) -> Result<(), String> {
    delete_photo_with(list, path, |p| trash::delete(p).map_err(|e| e.to_string()))
}

/// `delete_photo` with the way the photo itself is removed passed in.
fn delete_photo_with(list: &mut PreviewList, path: &str, remove: impl FnOnce(&str) -> Result<(), String>) -> Result<(), String> {
    remove(path)?;
    let copies: Vec<i32> = list.virtual_copies.iter().filter(|v| v.path == path).map(|v| v.index).collect();
    for f in cache_files(path, &copies) {
        let _ = std::fs::remove_file(f);
    }
    list.virtual_copies.retain(|v| v.path != path);
    list.hidden.retain(|k| store::parse_key(k).0 != path);
    Ok(())
}

/// 關閉資料夾並刪除快取縮圖: the regenerable caches in a folder's RAW_TEMP (thumbnails and
/// proxies, also the C# `.f32` ones); edit XMLs and preview_list.xml stay. Returns how many
/// files went.
pub fn delete_cache_files(folder: &str) -> usize {
    const SUFFIXES: [&str; 7] = ["_thumb.jpg", ".rawpipe.png", ".rawpipe.png.f16", ".rawpipe.png.src", ".rawpipe.png.thumb.jpg", ".rawpipe.f32", ".f32"];
    let Ok(rd) = std::fs::read_dir(paths::raw_temp_dir(folder)) else { return 0 };
    let mut n = 0;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if SUFFIXES.iter().any(|s| name.ends_with(s)) && std::fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::XmlStyle;

    #[test]
    fn hide_unhide_and_copies_round_trip() {
        let mut l = PreviewList::default();
        hide(&mut l, r"D:\p\a.ARW");
        hide(&mut l, r"D:\p\a.ARW"); // once only
        hide(&mut l, r"D:\p\a.ARW|copy:1");
        assert_eq!(l.hidden.len(), 2);
        unhide(&mut l, r"D:\p\a.ARW");
        assert_eq!(l.hidden, vec![r"D:\p\a.ARW|copy:1".to_string()]);

        let strip = [(r"D:\p\a.ARW", 0), (r"d:\P\A.arw", 2), (r"D:\p\b.ARW", 0)];
        assert_eq!(next_copy_index(strip.iter().map(|(p, i)| (*p, *i)), r"D:\p\a.ARW"), 3);
        assert_eq!(next_copy_index(strip.iter().map(|(p, i)| (*p, *i)), r"D:\p\b.ARW"), 1);

        // copy:1 is hidden (so not on the strip) and must survive the rewrite.
        l.virtual_copies = vec![VirtualCopyEntry { path: r"D:\p\a.ARW".into(), index: 1 }, VirtualCopyEntry { path: r"D:\p\a.ARW".into(), index: 2 }];
        rewrite_virtual_copies(&mut l, &[(r"D:\p\a.ARW".into(), 2), (r"D:\p\a.ARW".into(), 3)]);
        let idx: Vec<i32> = l.virtual_copies.iter().map(|v| v.index).collect();
        assert_eq!(idx, vec![2, 3, 1]);
        // The C# format is unchanged.
        assert_eq!(PreviewList::parse(&l.to_xml(XmlStyle::DotNet)).unwrap(), l);

        remove_virtual_copy(&mut l, r"D:\p\a.ARW", 1);
        assert!(l.hidden.is_empty());
        assert_eq!(l.virtual_copies.len(), 2);
    }

    #[test]
    fn clearing_the_cache_keeps_edits() {
        let dir = std::env::temp_dir().join(format!("awpr_cache_{}", std::process::id()));
        let rt = dir.join(paths::RAW_TEMP);
        std::fs::create_dir_all(&rt).unwrap();
        let keep = ["a.ARW.rawpipe.xml", "a.ARW.copy1.rawpipe.xml", "preview_list.xml"];
        let gone = ["a.ARW_thumb.jpg", "a.ARW.rawpipe.png", "a.ARW.rawpipe.png.src", "a.ARW.rawpipe.png.thumb.jpg", "a.ARW.rawpipe.png.f16", "b.JPG.rawpipe.f32"];
        for f in keep.iter().chain(&gone) {
            std::fs::write(rt.join(f), b"x").unwrap();
        }
        assert_eq!(delete_cache_files(&dir.to_string_lossy()), gone.len());
        assert!(keep.iter().all(|f| rt.join(f).exists()));
        assert!(gone.iter().all(|f| !rt.join(f).exists()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_photo_caches_and_copies() {
        let dir = std::env::temp_dir().join(format!("awpr_lib_{}", std::process::id()));
        let raw = dir.join("x.ARW");
        std::fs::create_dir_all(dir.join(paths::RAW_TEMP)).unwrap();
        std::fs::write(&raw, b"raw").unwrap();
        let p = raw.to_string_lossy().into_owned();
        for f in cache_files(&p, &[1]) {
            std::fs::write(f, b"c").unwrap();
        }
        let mut l = PreviewList { hidden: vec![store::make_key(&p, 1)], virtual_copies: vec![VirtualCopyEntry { path: p.clone(), index: 1 }] };
        // A failed removal (no trash, locked file) changes nothing.
        assert!(delete_photo_with(&mut l, &p, |_| Err("locked".into())).is_err());
        assert!(raw.exists() && l.virtual_copies.len() == 1);
        // The real one sends the photo to the trash; the test deletes it instead, so it
        // never lands in the user's recycle bin.
        delete_photo_with(&mut l, &p, |f| std::fs::remove_file(f).map_err(|e| e.to_string())).unwrap();
        assert!(!raw.exists());
        assert!(cache_files(&p, &[1]).iter().all(|f| !std::path::Path::new(f).exists()));
        assert!(l.hidden.is_empty() && l.virtual_copies.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
