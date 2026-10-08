//! `awpr exporttest <image> <outDir> <report>` — the export pipeline end to end (port of
//! the C# `SelfTest.RunExport`, extended): JPEG / PNG / TIFF, 8 and 16 bit, watermark,
//! name de-duplication, and the 8-bit output checked against the CPU pipeline.
//!
//! The source is copied into `<outDir>/source` first, so the test edits (rawpipe.xml)
//! never touch the original folder.

use awpr_core::{apply_to_float, ImageAdjustments, ProcessContext};
use awpr_gpu::GpuPipeline;
use awpr_photo::export::{self, ConflictMode, ExportFormat, ExportItem, ExportLocation, ExportSettings, RenameMode};
use awpr_photo::loader::{self, LoaderOptions};
use awpr_photo::watermark::{WatermarkColor, WatermarkPosition};
use awpr_photo::{codec, exif, paths, store};
use image::ImageDecoder;
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

pub fn run(image_path: &str, out_dir: &str, report_path: &str) -> i32 {
    let mut r = String::new();
    let ok = match test(image_path, out_dir, &mut r) {
        Ok(ok) => ok,
        Err(e) => {
            let _ = writeln!(r, "!!! 失敗 !!!\n{e}");
            false
        }
    };
    let _ = writeln!(r, "{}", if ok { "=== 完成（成功）===" } else { "=== 完成（有項目未通過）===" });
    print!("{r}");
    if let Err(e) = std::fs::write(report_path, &r) {
        eprintln!("cannot write {report_path}: {e}");
        return 1;
    }
    if ok {
        0
    } else {
        1
    }
}

/// What a written file actually contains, read back from disk.
fn inspect(path: &Path) -> Result<(u32, u32, &'static str, bool), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let reader = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let is_tiff = reader.format() == Some(image::ImageFormat::Tiff);
    let mut dec = reader.into_decoder().map_err(|e| e.to_string())?;
    let (w, h) = dec.dimensions();
    let depth = match dec.color_type() {
        image::ColorType::Rgb16 | image::ColorType::Rgba16 | image::ColorType::L16 | image::ColorType::La16 => "16-bit",
        _ => "8-bit",
    };
    let icc = if is_tiff {
        // image's TIFF decoder does not return the tag; ask tiff directly.
        tiff::decoder::Decoder::new(std::io::Cursor::new(&bytes)).ok().and_then(|mut d| d.get_tag_u8_vec(tiff::tags::Tag::IccProfile).ok()).is_some_and(|p| p == export::srgb_icc())
    } else {
        dec.icc_profile().ok().flatten().is_some_and(|p| p == export::srgb_icc())
    };
    Ok((w, h, depth, icc))
}

fn test(image_path: &str, out_dir: &str, r: &mut String) -> Result<bool, String> {
    let out = Path::new(out_dir);
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let _ = writeln!(r, "=== 匯出流程測試 ===");
    let _ = writeln!(r, "平台    : {} {}", std::env::consts::OS, std::env::consts::ARCH);
    let _ = writeln!(r, "版本    : AwayPhotoRawEditor {}", env!("CARGO_PKG_VERSION"));

    // Work on a copy so the test XML never lands next to the original.
    let src_dir = out.join("source");
    std::fs::create_dir_all(&src_dir).map_err(|e| e.to_string())?;
    let src = src_dir.join(paths::file_name(image_path));
    if !src.exists() {
        std::fs::copy(image_path, &src).map_err(|e| format!("無法複製來源：{e}"))?;
    }
    let src = src.to_string_lossy().into_owned();
    let _ = writeln!(r, "目標檔案: {}", paths::file_name(image_path));

    // A vivid edit so the output visibly differs (the C# test values, without its preset).
    let adj = ImageAdjustments { contrast: 30.0, vibrance: 40.0, saturation: 12.0, exposure: 0.3, temperature: 6500.0, ..Default::default() };
    let mut e = exif::read(&src);
    if e.camera.is_none() && paths::is_raw(&src) {
        e.camera = awpr_core::libraw::read_camera_color(&src);
    }
    store::save(&src, &adj, 0, Some(&e)).map_err(|e| e.to_string())?;

    let gpu = GpuPipeline::new().ok();
    let used_gpu = std::sync::atomic::AtomicBool::new(false);
    let render = |img: &awpr_core::FloatImage, a: &ImageAdjustments, ctx: &ProcessContext| {
        if let Some(g) = gpu.as_ref().filter(|g| g.can_host(img.width, img.height)) {
            if let Ok(o) = g.apply(img, a, ctx) {
                used_gpu.store(true, std::sync::atomic::Ordering::Relaxed);
                return o;
            }
        }
        apply_to_float(img, a, ctx)
    };
    let _ = writeln!(r, "GPU     : {}", gpu.as_ref().map(|g| g.status.as_str()).unwrap_or("無（CPU 算圖）"));
    let _ = writeln!(r);

    let base = ExportSettings {
        location: ExportLocation::Custom,
        custom_path: out_dir.to_string(),
        use_sub_folder: false,
        rename: RenameMode::Original,
        conflict: ConflictMode::AppendNumber,
        open_explorer_after: false,
        ..Default::default()
    };
    let watermarked = ExportSettings {
        watermark_enabled: true,
        watermark_text: "AwayPhotoRawEditor 浮水印".into(),
        watermark_font_size: 96.0,
        watermark_transparency: 20,
        watermark_color: WatermarkColor::White,
        watermark_position: WatermarkPosition::BottomRight,
        watermark_margin: 40,
        ..base.clone()
    };
    struct Case {
        name: &'static str,
        s: ExportSettings,
        high_precision: bool,
    }
    let cases = [
        Case { name: "JPEG q90 長邊 2000 浮水印", s: ExportSettings { format: ExportFormat::Jpeg, jpeg_quality: 90, max_long_edge: 2000, ..watermarked.clone() }, high_precision: false },
        Case { name: "PNG 8-bit 原尺寸（比對用）", s: ExportSettings { format: ExportFormat::Png, max_long_edge: 0, ..base.clone() }, high_precision: false },
        Case { name: "PNG 16-bit 長邊 2400 浮水印", s: ExportSettings { format: ExportFormat::Png, ..watermarked.clone() }, high_precision: true },
        Case { name: "TIFF 8-bit 長邊 2400", s: ExportSettings { format: ExportFormat::Tiff, ..base.clone() }, high_precision: false },
        Case { name: "TIFF 16-bit 長邊 2400 浮水印", s: ExportSettings { format: ExportFormat::Tiff, ..watermarked.clone() }, high_precision: true },
        Case { name: "JPEG 第二次（測試去重）", s: ExportSettings { format: ExportFormat::Jpeg, jpeg_quality: 90, max_long_edge: 2000, ..watermarked.clone() }, high_precision: false },
    ];
    let item = ExportItem { path: src.clone(), copy: 0 };
    let mut ok = true;
    let mut compare_png = None;
    for c in &cases {
        let opt = LoaderOptions { use_libraw: true, high_precision: c.high_precision };
        used_gpu.store(false, std::sync::atomic::Ordering::Relaxed);
        let t0 = Instant::now();
        let w = export::export_all(std::slice::from_ref(&item), &c.s, opt, &render, |_, _, _| true)?;
        let ms = t0.elapsed().as_millis();
        let w = w.first().ok_or("沒有輸出檔案")?;
        let (iw, ih, depth, icc) = inspect(&w.path)?;
        let want_depth = if w.sixteen { "16-bit" } else { "8-bit" };
        let good = (iw as usize, ih as usize) == (w.width, w.height) && depth == want_depth && icc;
        ok &= good;
        let _ = writeln!(
            r,
            "[{}]\n  檔案    : {}\n  尺寸    : {iw} x {ih}\n  位元深度: {depth}\n  ICC     : {}\n  大小    : {} KB\n  耗時    : {ms} ms（{}）{}",
            c.name,
            w.path.file_name().unwrap().to_string_lossy(),
            if icc { "有（sRGB）" } else { "無" },
            w.bytes / 1024,
            if used_gpu.load(std::sync::atomic::Ordering::Relaxed) { "GPU" } else { "CPU" },
            if good { "" } else { "\n  !!! 與預期不符" }
        );
        if c.s.format == ExportFormat::Png && !c.high_precision {
            compare_png = Some(w.path.clone());
        }
    }
    let names: Vec<String> = std::fs::read_dir(out).map_err(|e| e.to_string())?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".jpg")).collect();
    let stem = Path::new(&src).file_stem().unwrap().to_string_lossy().into_owned();
    let dedupe = names.contains(&format!("{stem}_edited.jpg")) && names.contains(&format!("{stem}_edited_1.jpg"));
    ok &= dedupe;
    let _ = writeln!(r, "\n去重    : {}（{}）", if dedupe { "通過" } else { "未通過" }, names.join(", "));

    // The 8-bit PNG against the CPU pipeline run directly on the same decode (the linear
    // one for a 處理版本 3 RAW, as the export uses).
    if let Some(p) = compare_png {
        let opt = LoaderOptions { use_libraw: true, high_precision: false };
        let (full, ctx) = loader::decode_for_render(&src, &adj, e.camera.as_ref(), opt).ok_or("無法解碼影像")?;
        let _ = writeln!(r, "
處理版本 : {}（{}）", adj.pipeline_version + 1, if matches!(ctx.source_kind, awpr_core::SourceKind::LinearCamera { .. }) { "線性相機來源" } else { "一般來源" });
        let reference = codec::to_rgb8(&apply_to_float(&full, &adj, &ctx));
        let png = image::open(&p).map_err(|e| e.to_string())?.to_rgb8();
        let (mut max, mut over1) = (0u8, 0usize);
        if png.as_raw().len() == reference.len() {
            for (a, b) in png.as_raw().iter().zip(&reference) {
                let d = a.abs_diff(*b);
                max = max.max(d);
                over1 += (d > 1) as usize;
            }
            let good = max <= 1;
            ok &= good;
            let _ = writeln!(r, "8-bit 比對: 最大差 {max}，差 >1 的通道 {over1} 個 → {}", if good { "通過" } else { "未通過" });
        } else {
            ok = false;
            let _ = writeln!(r, "8-bit 比對: 尺寸不同（PNG {} 位元組 vs 參考 {}）→ 未通過", png.as_raw().len(), reference.len());
        }
    }
    Ok(ok)
}
