//! `tiletest <raw> [report]` — a full-resolution export render on the GPU in strips
//! (`GpuPipeline::apply_tiled`) against the CPU pipeline on the whole image, for
//! 處理版本 2 and 3: the "綜合" case plus heal spots (and for version 3 masks, HSL and a
//! curve). Pass: no 8-bit channel differs by 2 or more (gputest's criterion).

use crate::hashtest::{self, to_byte};
use awpr_core::{apply_to_float, HealSpot, ImageAdjustments};
use awpr_gpu::GpuPipeline;
use awpr_photo::loader::{self, LoaderOptions};
use std::fmt::Write as _;
use std::time::Instant;

pub fn cases() -> Vec<(&'static str, ImageAdjustments)> {
    let heal = vec![
        HealSpot { target_x: 0.35, target_y: 0.45, source_x: 0.55, source_y: 0.40, radius_norm: 0.02, ..Default::default() },
        HealSpot { target_x: 0.65, target_y: 0.6, radius_norm: 0.015, use_inpaint: true, ..Default::default() },
    ];
    let mut v2 = hashtest::combined();
    v2.heal_spots = heal.clone();
    let mut v3 = hashtest::combined();
    v3.pipeline_version = ImageAdjustments::V3_PIPELINE_VERSION;
    v3.heal_spots = heal;
    v3.highlight_recovery = 40.0;
    v3.hsl_saturation = [20.0, 0.0, -30.0, 25.0, 0.0, 15.0, 0.0, 0.0];
    v3.curve_rgb = vec![(0.0, 0.0), (0.3, 0.26), (0.7, 0.75), (1.0, 1.0)];
    v3.masks = crate::gputest::mask_cases().into_iter().flat_map(|(_, a)| a.masks).collect();
    vec![("處理版本 2（綜合＋修護）", v2), ("處理版本 3（綜合＋修護＋遮罩＋HSL＋曲線）", v3)]
}

pub fn run(path: &str, report: Option<&str>) -> i32 {
    let (r, ok) = compare(path);
    print!("{r}");
    if let Some(p) = report {
        let _ = std::fs::write(p, &r);
    }
    i32::from(!ok)
}

/// The comparison as report text, and whether it passed (exporttest appends it).
pub fn compare(path: &str) -> (String, bool) {
    let mut r = String::new();
    let mut ok = true;
    let gpu = match GpuPipeline::new() {
        Ok(g) => g,
        Err(e) => return (format!("!! GPU 不可用：{e}\n"), false),
    };
    let _ = writeln!(r, "=== 大圖分段 GPU 對照 ===\n平台    : {} {}\nGPU     : {}（單張上限 {} MP）\n目標檔案: {}", std::env::consts::OS, std::env::consts::ARCH, gpu.status, gpu.max_pixels / 1_000_000, awpr_photo::paths::file_name(path));
    let camera = awpr_core::libraw::read_camera_color(path);
    let opt = LoaderOptions { use_libraw: true, high_precision: true };
    for (name, adj) in cases() {
        let Some((full, ctx)) = loader::decode_for_render(path, &adj, camera.as_ref(), opt) else {
            let _ = writeln!(r, "[{name}] !! 解碼失敗");
            ok = false;
            continue;
        };
        let t = Instant::now();
        let cpu = apply_to_float(&full, &adj, &ctx);
        let cpu_ms = t.elapsed().as_millis();
        let t = Instant::now();
        let out = match gpu.apply_tiled(&full, &adj, &ctx) {
            Ok(o) => o,
            Err(e) => {
                let _ = writeln!(r, "[{name}] !! GPU 失敗：{e}");
                ok = false;
                continue;
            }
        };
        let gpu_ms = t.elapsed().as_millis();
        let (mut d1, mut d2, mut worst) = (0usize, 0usize, 0i32);
        if (cpu.width, cpu.height) == (out.width, out.height) {
            for (i, (a, b)) in cpu.data.iter().zip(&out.data).enumerate() {
                if i % 4 == 3 {
                    continue;
                }
                let d = (to_byte(*a) as i32 - to_byte(*b) as i32).abs();
                worst = worst.max(d);
                d1 += (d == 1) as usize;
                d2 += (d >= 2) as usize;
            }
        } else {
            d2 = usize::MAX;
        }
        let total = cpu.width * cpu.height * 3;
        let good = d2 == 0;
        ok &= good;
        let strips = full.height.div_ceil(((gpu.max_pixels / full.width).saturating_sub(16)).min(4096).max(1));
        let _ = writeln!(
            r,
            "[{name}]\n  來源 {}x{} → 輸出 {}x{}（{} 條）\n  {}  8-bit 最大差 {worst}，差1 {:.4}%，差≥2 {}\n  CPU 全圖 {cpu_ms} ms → GPU 分段 {gpu_ms} ms",
            full.width,
            full.height,
            out.width,
            out.height,
            strips,
            if good { "✅" } else { "❌" },
            d1 as f64 / total as f64 * 100.0,
            if d2 == usize::MAX { "尺寸不同".to_string() } else { d2.to_string() },
        );
    }
    let _ = writeln!(r, "{}", if ok { "全部通過 ✅" } else { "有未通過的項目 ❌" });
    (r, ok)
}
