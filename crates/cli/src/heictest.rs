//! `heictest <file>...` — HEIC through this platform's system decoder: the container's
//! size / transforms / primaries, the decoded (upright) size, the colour at the four
//! quarter points (to check the orientation), the precision, and the EXIF the info panel
//! shows.

use awpr_photo::{exif, heic, heif};
use std::time::Instant;

pub fn run(files: &[String]) -> i32 {
    println!("平台    : {} {}", std::env::consts::OS, std::env::consts::ARCH);
    match heic::availability() {
        Ok(()) => println!("解碼器  : 可用"),
        Err(e) => println!("解碼器  : 不可用（{e}）"),
    }
    let mut code = 0;
    for f in files {
        println!("\n[{}]", awpr_photo::paths::file_name(f));
        match heif::read(f) {
            Some(i) => println!("  容器    : ispe {}x{}  轉換 {:?}  顯示 {:?}  色域 {:?}  EXIF {}", i.width, i.height, i.transforms, i.display_size(), i.primaries, if i.exif_tiff.is_some() { "有" } else { "無" }),
            None => println!("  容器    : 無法解析"),
        }
        let e = exif::read(f);
        println!("  EXIF    : {} / {} / {}  尺寸 {}x{}", e.camera_make, e.camera_model, e.date_taken, e.width, e.height);
        let t = Instant::now();
        match heic::decode(f) {
            Ok(img) => {
                let at = |fx: f64, fy: f64| {
                    let x = ((img.width as f64 * fx) as usize).min(img.width - 1);
                    let y = ((img.height as f64 * fy) as usize).min(img.height - 1);
                    let o = img.index(x, y);
                    format!("({:.3},{:.3},{:.3})", img.data[o], img.data[o + 1], img.data[o + 2])
                };
                // More than 256 distinct values in a channel → deeper than 8 bits.
                let mut levels: Vec<u32> = img.data.chunks_exact(4).map(|p| (p[1] * 65535.0).round() as u32).collect();
                levels.sort_unstable();
                levels.dedup();
                let fine = img.data.chunks_exact(4).any(|p| ((p[1] * 255.0) - (p[1] * 255.0).round()).abs() > 0.01);
                println!("  解碼    : {}x{}  {} ms  {}", img.width, img.height, t.elapsed().as_millis(), if fine { "高於 8-bit 精度" } else { "8-bit 值" });
                println!("  四角    : 左上 {} 右上 {} 左下 {} 右下 {}", at(0.25, 0.2), at(0.75, 0.2), at(0.25, 0.8), at(0.75, 0.8));
            }
            Err(e) => {
                println!("  解碼    : 失敗（{e}）");
                code = 1;
            }
        }
    }
    code
}
