//! `gputest <raw> [report]` — CPU vs GPU on the 14 hashtest cases plus three heal cases.
//! Port of `PipelineHash.gpuParity` in the Swift build.
//!
//! `gputest3 <raw> [report]` — the same for 處理版本 3: the 14 cases as version 3 on the
//! linear camera source, then 高光復原, HSL and curves, then one case on a gamma-encoded
//! source (the path JPEGs and RAWs without a linear decode take). Highlight reconstruction
//! itself runs on the CPU when the source is built (once per photo, full resolution: a
//! coarse-to-fine pyramid that a slider never changes), so both paths render from the same
//! reconstructed source; what the GPU does — the clip-neutral blend, white balance, tone,
//! shoulder, curves, OkLCh, gamut compression — is compared here.
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
use awpr_core::{apply_to_float, libraw, resize, v3, FloatImage, HealSpot, ImageAdjustments, ProcessContext, SourceKind};
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
        ("修護 複製（僅 GPU 對照）", ImageAdjustments { heal_spots: vec![clone], ..hashtest::v1() }),
        ("修護 填補（僅 GPU 對照）", ImageAdjustments { heal_spots: vec![inpaint], ..hashtest::v1() }),
        // Heal in the middle of a full pipeline: stages before it must be flushed to the
        // CPU, stages after it must see the edited pixels.
        ("綜合 + 修護 ×2（僅 GPU 對照）", h3),
    ]
}

/// The 14 cases as 處理版本 3, then the three version-3 tools.
pub fn v3_cases() -> Vec<(String, ImageAdjustments)> {
    let v3 = |a: ImageAdjustments| ImageAdjustments { pipeline_version: ImageAdjustments::V3_PIPELINE_VERSION, ..a };
    let mut out: Vec<(String, ImageAdjustments)> = hashtest::cases().into_iter().map(|(n, a)| (format!("v3｜{n}"), v3(a))).collect();
    out.push(("v3｜高光復原 80 + 曝光 +1".into(), v3(ImageAdjustments { exposure: 1.0, highlight_recovery: 80.0, ..Default::default() })));
    let mut hsl = v3(ImageAdjustments::default());
    hsl.hsl_hue = [30.0, 0.0, -20.0, 0.0, 0.0, 15.0, -40.0, 0.0];
    hsl.hsl_saturation = [0.0, 40.0, 50.0, -60.0, -80.0, 0.0, 0.0, 30.0];
    hsl.hsl_luminance = [-30.0, 20.0, 0.0, 0.0, 0.0, 40.0, 0.0, -20.0];
    hsl.vibrance = 25.0;
    out.push(("v3｜HSL 八色 + 鮮豔度".into(), hsl));
    let curves = v3(ImageAdjustments {
        curve_rgb: vec![(0.0, 0.0), (0.25, 0.18), (0.75, 0.84), (1.0, 1.0)],
        curve_red: vec![(0.0, 0.0), (0.5, 0.56), (1.0, 1.0)],
        curve_blue: vec![(0.0, 0.04), (0.5, 0.46), (1.0, 0.96)],
        contrast: 10.0,
        ..Default::default()
    });
    out.push(("v3｜曲線 RGB + 紅 + 藍".into(), curves));
    out
}

/// The version-3 source gputest3 renders from: LibRaw's linear decode, highlights
/// reconstructed, 2560 long edge (the editor's proxy), and its gain.
pub fn load_v3_source(path: &str) -> Option<(FloatImage, ProcessContext)> {
    let cam = libraw::read_camera_color(path)?;
    let mut full = libraw::decode_linear(path, None)?;
    let gain = v3::prepare_linear_source(&mut full, &cam);
    let ctx = ProcessContext { camera: Some(cam), source_kind: SourceKind::LinearCamera { gain }, ..Default::default() };
    Some((resize::resize_to_max_dim(full, 2560), ctx))
}

pub fn run(image_path: &str, report_path: Option<&str>) -> i32 {
    run_mode(image_path, report_path, false)
}

pub fn run_v3(image_path: &str, report_path: Option<&str>) -> i32 {
    run_mode(image_path, report_path, true)
}

fn run_mode(image_path: &str, report_path: Option<&str>, version3: bool) -> i32 {
    let mut lines: Vec<String> = Vec::new();
    let mut line = |s: String| {
        println!("{s}");
        lines.push(s);
    };

    line(if version3 { "=== CPU / GPU 對照（處理版本 3）===".into() } else { "=== CPU / GPU 對照 ===".into() });
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

    let Some((encoded, cam)) = hashtest::load_source(image_path) else {
        line("!! RAW 解碼失敗".into());
        write(&lines, report_path);
        return 1;
    };
    let encoded_ctx = ProcessContext { camera: cam, white_balance_reference: WhiteBalanceReference::Decode, ..Default::default() };
    // (source, context, cases) groups.
    let mut groups: Vec<(FloatImage, ProcessContext, Vec<(String, ImageAdjustments)>)> = Vec::new();
    if version3 {
        let Some((linear, ctx)) = load_v3_source(image_path) else {
            line("!! LibRaw 線性解碼失敗（處理版本 3 需要 3 色感光元件的 RAW）".into());
            write(&lines, report_path);
            return 1;
        };
        if let SourceKind::LinearCamera { gain } = ctx.source_kind {
            line(format!("來源    : 線性相機 RGB {} x {}，自動亮度 ×{gain:.4}", linear.width, linear.height));
        }
        line(format!("一般來源: {} x {}（LibRaw sRGB，給最後一組）", encoded.width, encoded.height));
        groups.push((linear, ctx, v3_cases()));
        let mut c = hashtest::combined();
        c.pipeline_version = ImageAdjustments::V3_PIPELINE_VERSION;
        c.hsl_saturation[0] = 30.0;
        c.curve_rgb = vec![(0.0, 0.0), (0.3, 0.25), (1.0, 1.0)];
        groups.push((encoded, encoded_ctx, vec![("v3｜綜合（一般圖檔來源）".into(), c)]));
    } else {
        line(format!("來源    : {} x {}", encoded.width, encoded.height));
        let cases = hashtest::cases().into_iter().chain(heal_cases()).map(|(n, a)| (n.to_string(), a)).collect();
        groups.push((encoded, encoded_ctx, cases));
    }
    line(String::new());

    let mut failures = 0;
    let mut worst = 0f32;
    for (source, ctx, cases) in &groups {
        let ctx = ctx.clone();
        // The editor's case: the proxy is uploaded once when a photo opens, and each slider
        // change renders from it and is drawn straight from the device.
        let resident = match gpu.upload(source) {
            Ok(f) => f,
            Err(e) => {
                line(format!("!! 無法上傳到 GPU：{e}"));
                write(&lines, report_path);
                return 1;
            }
        };

        for (name, adj) in cases.iter().cloned() {
            // Warm both paths once so the timings measure the render, not first-use setup.
            let _ = apply_to_float(source, &adj, &ctx);
            let _ = gpu.apply(source, &adj, &ctx);

            let t = Instant::now();
            let cpu = apply_to_float(source, &adj, &ctx);
            let cpu_ms = t.elapsed().as_secs_f64() * 1000.0;
            let t = Instant::now();
            let gpu_out = match gpu.apply(source, &adj, &ctx) {
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
                let plain = apply_to_float(source, &bare, &ctx);
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
    }

    line(String::new());
    line(format!("整體最大差: {worst:.2e}"));
    line(if failures == 0 { "全部通過 ✅".into() } else { format!("有 {failures} 項超出容許範圍 ❌") });
    write(&lines, report_path);
    i32::from(failures != 0)
}


/// `v3cmp <raw> <outDir>` — the same photo as 處理版本 2 and, upgraded, as 處理版本 3
/// (CPU, default sliders plus one edit), written as PNGs with the mean 8-bit difference:
/// how close 升級處理版本 keeps a photo on real data.
pub fn compare_versions(image_path: &str, out_dir: &str) -> i32 {
    let Some((encoded, cam)) = hashtest::load_source(image_path) else {
        eprintln!("!! RAW 解碼失敗");
        return 1;
    };
    let Some((linear, ctx3)) = load_v3_source(image_path) else {
        eprintln!("!! LibRaw 線性解碼失敗");
        return 1;
    };
    let ctx2 = ProcessContext { camera: cam.clone(), white_balance_reference: WhiteBalanceReference::Decode, ..Default::default() };
    let shot = cam.as_ref().and_then(awpr_core::color::as_shot).unwrap_or((5200.0, 0.0));
    let _ = std::fs::create_dir_all(out_dir);
    let stem = Path::new(image_path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let cases = [
        ("asshot", ImageAdjustments { pipeline_version: 1, temperature: shot.0, tint: shot.1, ..Default::default() }),
        ("edit", ImageAdjustments { pipeline_version: 1, temperature: shot.0 - 600.0, tint: shot.1 + 5.0, exposure: 0.5, contrast: 20.0, shadows: 25.0, ..Default::default() }),
    ];
    let mut code = 0;
    for (tag, v2) in cases {
        let mut up = v2.clone();
        awpr_photo::edits::upgrade(&mut up, None);
        let a = apply_to_float(&encoded, &v2, &ctx2);
        let b = apply_to_float(&linear, &up, &ctx3);
        let mut hl = up.clone();
        hl.highlight_recovery = 100.0;
        let c = apply_to_float(&linear, &hl, &ctx3);
        let (mut sum, mut n, mut big) = (0f64, 0usize, 0usize);
        for (x, y) in a.data.chunks_exact(4).zip(b.data.chunks_exact(4)) {
            for k in 0..3 {
                let d = (to_byte(x[k]) as i32 - to_byte(y[k]) as i32).abs();
                sum += d as f64;
                big += (d > 8) as usize;
                n += 1;
            }
        }
        println!("[{tag}] v2 vs v3: 平均 8-bit 差 {:.2}，差 >8 的通道 {:.2}%", sum / n as f64, big as f64 / n as f64 * 100.0);
        for (img, v) in [(&a, "v2"), (&b, "v3"), (&c, "v3-hl100")] {
            let p = format!("{out_dir}/{stem}-{tag}-{v}.png");
            if awpr_photo::codec::save_png(img, &p).is_err() {
                code = 1;
            }
        }
    }
    code
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
