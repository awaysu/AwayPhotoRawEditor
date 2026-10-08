//! `bench <raw>` — the same eight cases, warm-up and best-of-three as the Swift
//! `awpr-cli bench`, so the CPU columns can be compared directly. The GPU columns render
//! from a resident source (uploaded once, as the editor does) without reading back.

use awpr_core::color::WhiteBalanceReference;
use awpr_core::{apply_to_float, libraw, resize, FloatImage, ImageAdjustments, LinearGradient, ProcessContext};
use awpr_gpu::{GpuFrame, GpuPipeline};
use std::time::Instant;

fn cases() -> Vec<(&'static str, ImageAdjustments)> {
    let d = ImageAdjustments::default;
    vec![
        ("白平衡+曝光", ImageAdjustments { exposure: 1.0, temperature: 3200.0, ..d() }),
        ("色調曲線", ImageAdjustments { contrast: 40.0, highlights: -50.0, shadows: 40.0, ..d() }),
        ("降噪+銳利化", ImageAdjustments { noise_reduction: 50.0, sharpening: 60.0, ..d() }),
        (
            "漸層",
            ImageAdjustments { gradients: vec![LinearGradient { exposure: -1.0, ..Default::default() }], ..d() },
        ),
        ("暗角", ImageAdjustments { vignette: 60.0, ..d() }),
        ("裁切+角度", ImageAdjustments { crop_x: 0.1, crop_width: 0.8, crop_angle: 5.0, ..d() }),
        ("廣角變形", ImageAdjustments { distortion: 40.0, ..d() }),
        (
            "綜合",
            ImageAdjustments {
                exposure: 0.5,
                contrast: 20.0,
                highlights: -30.0,
                shadows: 25.0,
                temperature: 4200.0,
                vibrance: 25.0,
                sharpening: 30.0,
                noise_reduction: 20.0,
                vignette: 25.0,
                crop_x: 0.05,
                crop_width: 0.9,
                crop_angle: 2.0,
                ..d()
            },
        ),
    ]
}

/// Display columns: CJK glyphs take two.
fn pad(s: &str, width: usize) -> String {
    let cols: usize = s.chars().map(|c| if c as u32 > 0x2E80 { 2 } else { 1 }).sum();
    format!("{s}{}", " ".repeat(width.saturating_sub(cols)))
}

fn time_cpu(buf: &FloatImage, adj: &ImageAdjustments, ctx: &ProcessContext) -> f64 {
    let _ = apply_to_float(buf, adj, ctx);
    (0..3)
        .map(|_| {
            let t = Instant::now();
            let _ = apply_to_float(buf, adj, ctx);
            ms(t)
        })
        .fold(f64::MAX, f64::min)
}

/// Best of ten back-to-back renders (see gputest: discrete GPUs need to clock up).
fn time_gpu(gpu: &GpuPipeline, src: &GpuFrame, adj: &ImageAdjustments, ctx: &ProcessContext) -> Option<f64> {
    gpu.render(src, adj, ctx).ok()?.wait().ok()?;
    let mut best = f64::MAX;
    for _ in 0..10 {
        let t = Instant::now();
        let out = gpu.render(src, adj, ctx).ok()?;
        out.wait().ok()?;
        best = best.min(ms(t));
    }
    Some(best)
}

fn cell(v: Option<f64>) -> String {
    match v {
        Some(ms) if ms < 10.0 => format!("{ms:>9.1} ms"),
        Some(ms) => format!("{ms:>9.0} ms"),
        None => format!("{:>12}", "—"),
    }
}

pub fn run(path: &str) -> i32 {
    println!(
        "threads  : {}",
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    );
    let t = Instant::now();
    let Some(full) = libraw::decode_full(path, 16, None) else {
        println!("!! RAW 解碼失敗");
        return 1;
    };
    println!("全解析度解碼 {}x{}: {:.0} ms", full.width, full.height, ms(t));
    let cam = libraw::read_camera_color(path);
    let t = Instant::now();
    let proxy = resize::resize_to_max_dim(full.clone(), 2560);
    println!("proxy {}x{}: {:.0} ms", proxy.width, proxy.height, ms(t));

    // The full-resolution GPU column ignores the practical 16 MP cap (it exists to be
    // re-measured); only the device's own limit applies.
    let gpu = match GpuPipeline::new() {
        Ok(mut g) => {
            g.max_pixels = g.device_max_pixels;
            println!("GPU      : {}（裝置上限 {} MP）", g.status, g.device_max_pixels / 1_000_000);
            Some(g)
        }
        Err(e) => {
            println!("GPU      : 不可用（{e}）");
            None
        }
    };
    let gpu_proxy = gpu.as_ref().and_then(|g| g.upload(&proxy).ok());
    let gpu_full = gpu.as_ref().and_then(|g| g.upload(&full).ok());
    if gpu.is_some() && gpu_full.is_none() {
        println!("（全圖超過 GPU 裝置上限，全圖 GPU 欄留空）");
    }
    println!();

    let ctx = ProcessContext { camera: cam, white_balance_reference: WhiteBalanceReference::Decode, ..Default::default() };
    println!(
        "{}{:>12}{:>12}{:>12}{:>12}",
        pad("階段", 16),
        "proxy CPU",
        "proxy GPU",
        "全圖 CPU",
        "全圖 GPU"
    );
    for (name, adj) in cases() {
        let pc = time_cpu(&proxy, &adj, &ctx);
        let pg = gpu.as_ref().zip(gpu_proxy.as_ref()).and_then(|(g, f)| time_gpu(g, f, &adj, &ctx));
        let fc = time_cpu(&full, &adj, &ctx);
        let fg = gpu.as_ref().zip(gpu_full.as_ref()).and_then(|(g, f)| time_gpu(g, f, &adj, &ctx));
        println!("{}{}{}{}{}", pad(name, 16), cell(Some(pc)), cell(pg), cell(Some(fc)), cell(fg));
    }
    0
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
