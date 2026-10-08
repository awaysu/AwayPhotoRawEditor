//! `gputest <raw> [report]` — CPU vs GPU on the 14 hashtest cases plus three heal cases.
//! Port of `PipelineHash.gpuParity` in the Swift build.
//!
//! Byte-identical output is not expected: the geometry stages compute in f64 on the CPU
//! and WGSL has no f64, and GPU transcendentals round differently. The pass criterion is
//! the 8-bit one the Windows and Swift builds settled on, because that is what reaches a
//! file:
//!
//! * no 8-bit channel may differ by 2 or more — that would mean the maths diverged
//! * at most 0.07% of channels may differ by 1

use crate::hashtest::{self, to_byte};
use awpr_core::color::WhiteBalanceReference;
use awpr_core::{apply_to_float, HealSpot, ImageAdjustments, ProcessContext};
use awpr_gpu::GpuPipeline;
use std::path::Path;
use std::time::Instant;

/// Not part of the 14 (hashtest must match the C# list), so only gputest runs them. The
/// spots sit where a 2560 proxy has content, with radii (0.03 × long edge ≈ 77 px) large
/// enough that a mistake would show. Identical to `healCases()` in the Swift build.
fn heal_cases() -> Vec<(&'static str, ImageAdjustments)> {
    let clone = HealSpot { target_x: 0.35, target_y: 0.45, source_x: 0.55, source_y: 0.40, radius_norm: 0.03, ..Default::default() };
    let inpaint = HealSpot { target_x: 0.65, target_y: 0.6, radius_norm: 0.025, use_inpaint: true, ..Default::default() };
    let mut h3 = hashtest::combined();
    h3.heal_spots = vec![clone.clone(), inpaint.clone()];
    vec![
        ("修護 複製（僅 GPU 對照）", ImageAdjustments { heal_spots: vec![clone], ..Default::default() }),
        ("修護 填補（僅 GPU 對照）", ImageAdjustments { heal_spots: vec![inpaint], ..Default::default() }),
        // Heal in the middle of a full pipeline: stages before it must be flushed to the
        // CPU, stages after it must see the edited pixels.
        ("綜合 + 修護 ×2（僅 GPU 對照）", h3),
    ]
}

pub fn run(image_path: &str, report_path: Option<&str>) -> i32 {
    let mut lines: Vec<String> = Vec::new();
    let mut line = |s: String| {
        println!("{s}");
        lines.push(s);
    };

    line("=== CPU / GPU 對照 ===".into());
    let gpu = match GpuPipeline::new() {
        Ok(g) => g,
        Err(e) => {
            line(format!("!! GPU 不可用：{e}"));
            write(&lines, report_path);
            return 1;
        }
    };
    line(format!("GPU     : {}", gpu.status));
    line(format!("尺寸上限: {} MP", gpu.max_pixels / 1_000_000));
    let name = Path::new(image_path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    line(format!("目標檔案: {name}"));

    let Some((source, cam)) = hashtest::load_source(image_path) else {
        line("!! RAW 解碼失敗".into());
        write(&lines, report_path);
        return 1;
    };
    line(format!("來源    : {} x {}", source.width, source.height));
    line(String::new());

    let ctx = ProcessContext { camera: cam, white_balance_reference: WhiteBalanceReference::Decode, ..Default::default() };
    let mut failures = 0;
    let mut worst = 0f32;
    // The editor's case: the proxy is uploaded once when a photo opens, and each slider
    // change renders from it and is drawn straight from the device.
    let resident = match gpu.upload(&source) {
        Ok(f) => f,
        Err(e) => {
            line(format!("!! 無法上傳到 GPU：{e}"));
            write(&lines, report_path);
            return 1;
        }
    };

    for (name, adj) in hashtest::cases().into_iter().chain(heal_cases()) {
        // Warm both paths once so the timings measure the render, not first-use setup.
        let _ = apply_to_float(&source, &adj, &ctx);
        let _ = gpu.apply(&source, &adj, &ctx);

        let t = Instant::now();
        let cpu = apply_to_float(&source, &adj, &ctx);
        let cpu_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        let gpu_out = match gpu.apply(&source, &adj, &ctx) {
            Ok(o) => o,
            Err(e) => {
                line(format!("[{name}] ❌ GPU 失敗：{e}"));
                failures += 1;
                continue;
            }
        };
        let gpu_ms = t.elapsed().as_secs_f64() * 1000.0;
        let res_ms = match resident_ms(&gpu, &resident, &adj, &ctx) {
            Ok(ms) => ms,
            Err(e) => {
                line(format!("[{name}] ❌ GPU 常駐路徑失敗：{e}"));
                failures += 1;
                continue;
            }
        };

        if (cpu.width, cpu.height) != (gpu_out.width, gpu_out.height) {
            line(format!(
                "[{name}] ❌ 尺寸不同 CPU={}x{} GPU={}x{}",
                cpu.width, cpu.height, gpu_out.width, gpu_out.height
            ));
            failures += 1;
            continue;
        }

        // Both paths heal with the same CPU routine, so a heal that changed nothing would
        // agree trivially: prove the spots actually touched pixels.
        let mut healed_px: i64 = -1;
        if !adj.heal_spots.is_empty() {
            let bare = ImageAdjustments { heal_spots: Vec::new(), ..adj.clone() };
            let plain = apply_to_float(&source, &bare, &ctx);
            if (plain.width, plain.height) == (cpu.width, cpu.height) {
                healed_px = plain
                    .data
                    .chunks_exact(4)
                    .zip(cpu.data.chunks_exact(4))
                    .filter(|(a, b)| a[0] != b[0] || a[1] != b[1] || a[2] != b[2])
                    .count() as i64;
            }
        }

        let mut max_diff = 0f32;
        let (mut d2, mut d1) = (0usize, 0usize);
        for (i, (a, b)) in cpu.data.iter().zip(&gpu_out.data).enumerate() {
            if i % 4 == 3 {
                continue;
            }
            max_diff = max_diff.max((a - b).abs());
            let bd = (to_byte(*a) as i32 - to_byte(*b) as i32).abs();
            if bd >= 2 {
                d2 += 1;
            } else if bd == 1 {
                d1 += 1;
            }
        }
        worst = worst.max(max_diff);
        let total = cpu.width * cpu.height * 3;
        let pct1 = d1 as f64 / total as f64 * 100.0;
        let heal_ok = adj.heal_spots.is_empty() || healed_px > 0;
        let ok = d2 == 0 && pct1 <= 0.07 && heal_ok;
        if !ok {
            failures += 1;
        }
        line(format!("[{name}]"));
        line(format!(
            "  {}  最大差 {:.2e}  8-bit 差1 {:.3}%  差≥2 {d2}",
            if ok { "✅" } else { "❌" },
            max_diff,
            pct1
        ));
        if !adj.heal_spots.is_empty() {
            let shown = if healed_px < 0 { "（無法比對）".to_string() } else { format!("{healed_px} px") };
            line(format!("  修護實際改動 {shown}{}", if heal_ok { "" } else { "  ❌ 修護沒有改到任何像素" }));
        }
        line(format!(
            "  CPU {:.0} ms  →  GPU 常駐 {:.1} ms ({:.1}×)  含上傳+讀回 {:.0} ms",
            cpu_ms,
            res_ms,
            cpu_ms / res_ms.max(0.001),
            gpu_ms
        ));
    }

    line(String::new());
    line(format!("整體最大差: {worst:.2e}"));
    line(if failures == 0 { "全部通過 ✅".into() } else { format!("有 {failures} 項超出容許範圍 ❌") });
    write(&lines, report_path);
    i32::from(failures != 0)
}

/// Best of ten back-to-back renders from the resident source, waiting for the device each
/// time. Back-to-back on purpose: a discrete GPU drops its clocks while the CPU reference
/// runs, and a single render after that measured 5 ms for a kernel that takes 0.55 ms
/// once clocked up (RTX 3060). Dragging a slider is the back-to-back case.
fn resident_ms(gpu: &GpuPipeline, src: &awpr_gpu::GpuFrame, adj: &ImageAdjustments, ctx: &ProcessContext) -> Result<f64, awpr_core::pipeline::StageError> {
    gpu.render(src, adj, ctx)?.wait()?;
    let mut best = f64::MAX;
    for _ in 0..10 {
        let t = Instant::now();
        let out = gpu.render(src, adj, ctx)?;
        out.wait()?;
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(best)
}

fn write(lines: &[String], path: Option<&str>) {
    let Some(path) = path else { return };
    let mut s = lines.join("\n");
    s.push('\n');
    if std::fs::write(path, s).is_ok() {
        println!("報告已寫出: {path}");
    }
}
