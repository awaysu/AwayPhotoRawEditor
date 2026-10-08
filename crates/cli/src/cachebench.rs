//! `cachebench <workDir> <raw>...` — what a 處理版本 3 photo adds to building the folder
//! caches. Per file, on a fresh copy in `workDir`:
//!
//! * A: the usual proxy only (`ensure_proxy_cache`, what a version-2 photo gets),
//! * B: the usual + linear proxies the old way (two LibRaw decodes),
//! * C: both from one open + unpack (`ensure_proxy_caches`, what the editor does),
//!
//! and checks that `decode_both` gives exactly what `decode_full` and `decode_linear` give
//! on their own (every float equal) and that C's cache files equal B's byte for byte.

use awpr_core::libraw;
use awpr_photo::loader::{self, LoaderOptions};
use awpr_photo::{exif, paths};
use std::path::Path;
use std::time::Instant;

fn fresh_copy(work: &Path, src: &str) -> Option<String> {
    let dir = work.join(paths::file_name(src).replace(['.', ' ', '@'], "_"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let dst = dir.join(paths::file_name(src));
    std::fs::copy(src, &dst).ok()?;
    Some(dst.to_string_lossy().into_owned())
}

fn clear_caches(image: &str) {
    let _ = std::fs::remove_dir_all(paths::raw_temp_dir(&Path::new(image).parent().unwrap().to_string_lossy()));
}

fn read(p: &str) -> Vec<u8> {
    std::fs::read(p).unwrap_or_default()
}

pub fn run(work: &str, files: &[String]) -> i32 {
    let work = Path::new(work);
    let opt = LoaderOptions::default();
    let mut ok = true;
    let (mut ta, mut tb, mut tc) = (0.0, 0.0, 0.0);
    println!("{:<44} {:>9} {:>9} {:>9} {:>7} {:>7}  一致", "檔案", "A 版本2", "B 兩次解碼", "C 一次解碼", "B/A", "C/A");
    for src in files {
        let Some(img) = fresh_copy(work, src) else {
            println!("!! 無法複製 {src}");
            ok = false;
            continue;
        };
        let Some(cam) = libraw::read_camera_color(&img) else {
            println!("!! {src}: 沒有相機色彩資料（不能用處理版本 3）");
            ok = false;
            continue;
        };
        let secs = |f: &dyn Fn()| {
            clear_caches(&img);
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64()
        };
        // Warm the file cache once so A does not pay for the first read alone.
        let _ = std::fs::read(&img);
        let a = secs(&|| {
            loader::ensure_proxy_cache(&img, opt);
        });
        let b = secs(&|| {
            loader::ensure_proxy_cache(&img, opt);
            loader::ensure_proxy_v3(&img, &cam);
        });
        let files_b = [read(&paths::proxy_path(&img)), read(&paths::proxy_v3_path(&img)), read(&paths::proxy_v3_meta_path(&img)), read(&paths::proxy_thumbnail_path(&img))];
        let c = secs(&|| {
            loader::ensure_proxy_caches(&img, opt, Some(&cam));
        });
        let files_c = [read(&paths::proxy_path(&img)), read(&paths::proxy_v3_path(&img)), read(&paths::proxy_v3_meta_path(&img)), read(&paths::proxy_thumbnail_path(&img))];
        let same_files = files_b.iter().zip(&files_c).all(|(x, y)| !x.is_empty() && x == y);

        // The decodes themselves, float for float.
        let vis = exif::visible_size(&img);
        let (enc, lin) = libraw::decode_both(&img, 8, vis);
        let full = libraw::decode_full(&img, 8, vis);
        let linear = libraw::decode_linear(&img, vis);
        let eq = |x: &Option<awpr_core::FloatImage>, y: &Option<awpr_core::FloatImage>| match (x, y) {
            (Some(x), Some(y)) => x.width == y.width && x.height == y.height && x.data == y.data,
            _ => false,
        };
        let same_decode = eq(&enc, &full) && eq(&lin, &linear);
        let size = full.as_ref().map(|f| format!("{}x{}", f.width, f.height)).unwrap_or_default();
        let good = same_files && same_decode;
        ok &= good;
        ta += a;
        tb += b;
        tc += c;
        let name: String = paths::file_name(src).chars().take(30).collect();
        println!(
            "{:<44} {:>8.2}s {:>8.2}s {:>8.2}s {:>7.2} {:>7.2}  {}",
            format!("{name} ({size})"),
            a,
            b,
            c,
            b / a,
            c / a,
            if good { "✅ 解碼與快取逐位元組相同" } else { "❌ 不同" }
        );
        if !same_decode {
            println!("   decode_both 與單獨解碼不同（8-bit {}，線性 {}）", eq(&enc, &full), eq(&lin, &linear));
        }
        if !same_files {
            println!("   快取檔不同或缺少");
        }
        let _ = std::fs::remove_dir_all(Path::new(&img).parent().unwrap());
    }
    println!("{:<44} {:>8.2}s {:>8.2}s {:>8.2}s {:>7.2} {:>7.2}", "合計", ta, tb, tc, tb / ta, tc / ta);
    i32::from(!ok)
}
