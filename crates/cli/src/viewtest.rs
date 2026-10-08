//! `viewtest <raw> [report]` — the editor's viewer shader against a CPU reference.
//!
//! The editor draws a rendered frame straight from its GPU buffer. This runs that same
//! shader (`awpr_gpu::display`) into an offscreen texture at several zooms — exact
//! pixels at 100 % / 200 %, the box filter when shrinking — and compares the result
//! with the CPU pipeline's output put through a CPU port of the shader's sampling. No
//! window is needed, so it runs on every backend over SSH.
//!
//! The zoom cases render to an Rgba32Float target and are quantised on the CPU exactly
//! like gputest, so they test the shader's logic with gputest's criterion: no 8-bit
//! channel off by 2 or more, at most 0.07% off by 1.
//!
//! Two more cases show what the hardware's own 8-bit output adds (not part of the
//! verdict beyond "never off by 2"): an Rgba8Unorm target, which is what egui presents
//! to on D3D12, Metal and Vulkan, and an sRGB target, which the editor only gets when a
//! window offers no plain 8-bit format (the shader linearises, the hardware re-encodes
//! with its own rounding).

use crate::hashtest;
use awpr_core::color::WhiteBalanceReference;
use awpr_core::{apply_to_float, FloatImage, ProcessContext};
use awpr_gpu::display::ViewUniform;
use awpr_gpu::GpuPipeline;
use std::path::Path;

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Float,
    Unorm,
    Srgb,
}

struct Case {
    name: &'static str,
    out_w: u32,
    out_h: u32,
    origin: [f32; 2],
    scale: f32,
    target: Target,
}

/// What the fragment shader computes for framebuffer pixel (px, py), in f32 like WGSL.
fn reference(img: &FloatImage, c: &Case, px: u32, py: u32, bg: [f32; 3]) -> [f32; 3] {
    let (w, h) = (img.width as f32, img.height as f32);
    let pos = [px as f32 + 0.5, py as f32 + 0.5];
    let fetch = |qx: f32, qy: f32| {
        let x = (qx.floor() as i32).clamp(0, img.width as i32 - 1) as usize;
        let y = (qy.floor() as i32).clamp(0, img.height as i32 - 1) as usize;
        let i = (y * img.width + x) * 4;
        [img.data[i], img.data[i + 1], img.data[i + 2]]
    };
    let q = [(pos[0] - c.origin[0]) / c.scale, (pos[1] - c.origin[1]) / c.scale];
    let col = if q[0] < 0.0 || q[1] < 0.0 || q[0] >= w || q[1] >= h {
        bg
    } else if c.scale >= 1.0 {
        fetch(q[0], q[1])
    } else {
        let f = 1.0 / c.scale;
        let n = (f.ceil() as i32).min(8);
        let step = f / n as f32;
        let base = [(pos[0] - 0.5 - c.origin[0]) / c.scale, (pos[1] - 0.5 - c.origin[1]) / c.scale];
        let mut sum = [0f32; 3];
        for j in 0..n {
            for i in 0..n {
                let s = fetch(base[0] + (i as f32 + 0.5) * step, base[1] + (j as f32 + 0.5) * step);
                for k in 0..3 {
                    sum[k] += s[k];
                }
            }
        }
        let nn = (n * n) as f32;
        [sum[0] / nn, sum[1] / nn, sum[2] / nn]
    };
    [col[0].clamp(0.0, 1.0), col[1].clamp(0.0, 1.0), col[2].clamp(0.0, 1.0)]
}

pub fn run(image_path: &str, report_path: Option<&str>) -> i32 {
    let mut lines: Vec<String> = Vec::new();
    let mut line = |s: String| {
        println!("{s}");
        lines.push(s);
    };
    line("=== 檢視器 shader 對照（GPU 直接畫 vs CPU 參考）===".into());
    let gpu = match GpuPipeline::new() {
        Ok(g) => g,
        Err(e) => {
            line(format!("!! GPU 不可用：{e}"));
            write(&lines, report_path);
            return 1;
        }
    };
    line(format!("GPU     : {}", gpu.status));
    let name = Path::new(image_path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    line(format!("目標檔案: {name}"));
    let Some((source, cam)) = hashtest::load_source(image_path) else {
        line("!! RAW 解碼失敗".into());
        write(&lines, report_path);
        return 1;
    };
    let ctx = ProcessContext { camera: cam, white_balance_reference: WhiteBalanceReference::Decode, ..Default::default() };
    let adj = hashtest::combined();
    let cpu = apply_to_float(&source, &adj, &ctx);
    let frame = match gpu.upload(&source).and_then(|s| gpu.render(&s, &adj, &ctx)) {
        Ok(f) => f,
        Err(e) => {
            line(format!("!! GPU 算圖失敗：{e}"));
            write(&lines, report_path);
            return 1;
        }
    };
    line(format!("畫面    : {} x {}（綜合調整）", cpu.width, cpu.height));
    line(String::new());

    let bg = [0x14 as f32 / 255.0; 3];
    use Target::*;
    let cases = [
        Case { name: "100%（逐像素，含偏移與邊緣）", out_w: 1200, out_h: 800, origin: [-300.0, -200.0], scale: 1.0, target: Float },
        Case { name: "200%（放大，最近點）", out_w: 900, out_h: 700, origin: [-1000.0, -600.0], scale: 2.0, target: Float },
        Case { name: "50%（2×2 平均）", out_w: (cpu.width / 2) as u32, out_h: (cpu.height / 2) as u32, origin: [0.0, 0.0], scale: 0.5, target: Float },
        Case { name: "37.12%（3×3 取樣，小數偏移，背景）", out_w: 1000, out_h: 700, origin: [13.25, 7.5], scale: 0.3712, target: Float },
        Case { name: "參考：100% 硬體 8-bit（Rgba8Unorm）", out_w: 1200, out_h: 800, origin: [-300.0, -200.0], scale: 1.0, target: Unorm },
        Case { name: "參考：100% sRGB 目標", out_w: 1200, out_h: 800, origin: [-300.0, -200.0], scale: 1.0, target: Srgb },
    ];
    let mut failures = 0;
    for c in &cases {
        let format = match c.target {
            Float => awpr_gpu::wgpu::TextureFormat::Rgba32Float,
            Unorm => awpr_gpu::wgpu::TextureFormat::Rgba8Unorm,
            Srgb => awpr_gpu::wgpu::TextureFormat::Rgba8UnormSrgb,
        };
        let view = ViewUniform::new(frame.width, frame.height, c.origin, c.scale, bg, c.target == Srgb);
        let got = match gpu.display_readback(&frame, c.out_w, c.out_h, view, format) {
            Ok(v) => v,
            Err(e) => {
                line(format!("❌ {}：{e}", c.name));
                failures += 1;
                continue;
            }
        };
        let floats: Vec<f32> = if c.target == Float { bytemuck_f32(&got) } else { Vec::new() };
        let (mut max, mut ge2, mut eq1, mut n, mut max_f) = (0u8, 0usize, 0usize, 0usize, 0f32);
        for py in 0..c.out_h {
            for px in 0..c.out_w {
                let r = reference(&cpu, c, px, py, bg);
                let i = ((py * c.out_w + px) * 4) as usize;
                for k in 0..3 {
                    let want = hashtest::to_byte(r[k]);
                    let have = if c.target == Float {
                        max_f = max_f.max((floats[i + k] - r[k]).abs());
                        hashtest::to_byte(floats[i + k])
                    } else {
                        got[i + k]
                    };
                    let d = have.abs_diff(want);
                    max = max.max(d);
                    if d >= 2 {
                        ge2 += 1;
                    } else if d == 1 {
                        eq1 += 1;
                    }
                    n += 1;
                }
            }
        }
        let pct = eq1 as f64 / n as f64 * 100.0;
        let ok = ge2 == 0 && (c.target != Float || pct <= 0.07);
        if !ok {
            failures += 1;
        }
        let float_note = if c.target == Float { format!("  浮點最大差 {max_f:.2e}") } else { String::new() };
        line(format!(
            "{} {:<36} {}x{}  最大差 {max}  差≥2 {ge2}  差1 {pct:.3}%{float_note}",
            if ok { "✅" } else { "❌" },
            c.name,
            c.out_w,
            c.out_h
        ));
    }
    line(String::new());
    line(if failures == 0 { "結果：全部通過".into() } else { format!("結果：{failures} 組失敗") });
    write(&lines, report_path);
    (failures > 0) as i32
}

fn bytemuck_f32(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn write(lines: &[String], path: Option<&str>) {
    if let Some(p) = path {
        let _ = std::fs::write(p, lines.join("\n") + "\n");
    }
}
