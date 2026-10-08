//! `hashtest <img> [report]` — the cross-implementation fingerprint of the colour pipeline.
//!
//! The 14 cases, the source preparation, the SHA input and the number formatting mirror
//! `awpr-cli/PipelineHash.swift` and the C# `Diagnostics/PipelineHash.cs`, so reports can
//! be diffed line for line (only the 平台 / 執行階段 header lines differ).
//!
//! Pass criteria, carried over from the C#/Swift GpuParity notes:
//! * SHA equal → bit-identical, ideal
//! * SHA differs but samples agree to < 1e-5 → libm ULP noise, acceptable
//! * differences of 1e-3 or more, or an 8-bit channel差 >= 2 → the maths diverged

use awpr_core::color::WhiteBalanceReference;
use awpr_core::{
    apply_to_float, libraw, resize, CameraColorInfo, FloatImage, ImageAdjustments, LinearGradient,
    ProcessContext, Rotation,
};
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn run(image_path: &str, report_path: Option<&str>) -> i32 {
    let mut lines: Vec<String> = Vec::new();
    let mut line = |s: String| {
        println!("{s}");
        lines.push(s);
    };

    line("=== AwayPhotoRawEditor 色彩管線指紋 ===".into());
    line(format!("平台    : {} {}", std::env::consts::OS, std::env::consts::ARCH));
    line(format!("執行階段: Rust / awpr-core (LibRaw {})", libraw::version()));
    let file_name = Path::new(image_path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    line(format!("目標檔案: {file_name}"));

    let Some((source, cam)) = load_source(image_path) else {
        line("!! RAW 解碼失敗".into());
        write(&lines, report_path);
        return 1;
    };
    line(format!("來源    : {} x {}", source.width, source.height));
    let valid = cam.as_ref().is_some_and(|c| c.is_valid());
    line(format!("相機色彩: {}", if valid { "有（矩陣白平衡）" } else { "無（黑體近似）" }));
    if let Some(c) = cam.as_ref().filter(|_| valid) {
        line(format!(
            "          pre_mul[{},{},{}] cam_mul[{},{},{}]",
            f4(c.pre_mul[0]),
            f4(c.pre_mul[1]),
            f4(c.pre_mul[2]),
            f4(c.cam_mul[0]),
            f4(c.cam_mul[1]),
            f4(c.cam_mul[2])
        ));
    }
    line(String::new());

    for (name, adj) in cases() {
        let ctx = ProcessContext {
            camera: cam.clone(),
            white_balance_reference: WhiteBalanceReference::Decode,
            ..Default::default()
        };
        let out = apply_to_float(&source, &adj, &ctx);
        line(format!("[{name}]"));
        line(format!("  尺寸    : {} x {}", out.width, out.height));
        line(format!("  8bitSHA : {}", sha8(&out)));
        let (mr, mg, mb) = means(&out);
        line(format!("  通道均值: R={} G={} B={}", f9(mr), f9(mg), f9(mb)));
        line(format!("  取樣點  : {}", samples(&out)));
        line(String::new());
    }

    write(&lines, report_path);
    0
}

/// Shared source preparation: LibRaw 16-bit full decode, no mask trim, 2560 long edge.
/// Matches `PipelineHash.loadSource` (RAW branch only — non-RAW decoding is a
/// platform codec and is outside this comparison).
pub fn load_source(path: &str) -> Option<(FloatImage, Option<CameraColorInfo>)> {
    let cam = libraw::read_camera_color(path);
    let full = libraw::decode_full(path, 16, None)?;
    Some((resize::resize_to_max_dim(full, 2560), cam))
}

fn write(lines: &[String], path: Option<&str>) {
    let Some(path) = path else { return };
    let mut s = lines.join("\n");
    s.push('\n');
    if std::fs::write(path, s).is_ok() {
        println!("報告已寫出: {path}");
    }
}

// ---- formatting (matched to .NET's F4 / F9 / G9 and the Swift port) -----------

fn f4(v: f64) -> String {
    format!("{v:.4}")
}

fn f9(v: f64) -> String {
    format!("{v:.9}")
}

/// C's `%.9g` with the exponent marker upper-cased, which is what both .NET "G9" and the
/// Swift port print.
pub fn g9(v: f32) -> String {
    let v = v as f64;
    if v == 0.0 {
        return if v.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    if !v.is_finite() {
        return format!("{v}");
    }
    const P: i32 = 9;
    // The exponent after rounding to P significant digits decides the style.
    let sci = format!("{:.*e}", (P - 1) as usize, v);
    let (mant, exp) = sci.split_once('e').unwrap();
    let x: i32 = exp.parse().unwrap();
    if x < -4 || x >= P {
        let mant = strip_zeros(mant);
        let sign = if x < 0 { '-' } else { '+' };
        format!("{mant}E{sign}{:02}", x.abs())
    } else {
        let prec = (P - 1 - x) as usize;
        strip_zeros(&format!("{v:.prec$}")).to_string()
    }
}

fn strip_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

#[inline(always)]
pub fn to_byte(v: f32) -> u8 {
    let i = (v * 255.0 + 0.5) as i64;
    i.clamp(0, 255) as u8
}

/// SHA-256 of the 8-bit output, over the BGRA bytes the exporter would write.
fn sha8(buf: &FloatImage) -> String {
    let mut bytes = vec![0u8; buf.width * buf.height * 4];
    for (o, px) in bytes.chunks_exact_mut(4).zip(buf.data.chunks_exact(4)) {
        o[0] = to_byte(px[2]);
        o[1] = to_byte(px[1]);
        o[2] = to_byte(px[0]);
        o[3] = to_byte(px[3]);
    }
    let digest = Sha256::digest(&bytes);
    let hex: String = digest.iter().map(|b| format!("{b:02X}")).collect();
    hex[..32].to_string()
}

/// Sequential on purpose: summation order changes the 9th decimal.
fn means(b: &FloatImage) -> (f64, f64, f64) {
    let (mut sr, mut sg, mut sb) = (0.0f64, 0.0f64, 0.0f64);
    for px in b.data.chunks_exact(4) {
        sr += px[0] as f64;
        sg += px[1] as f64;
        sb += px[2] as f64;
    }
    let n = (b.width * b.height) as f64;
    (sr / n, sg / n, sb / n)
}

fn samples(b: &FloatImage) -> String {
    let pts = [(0.13, 0.21), (0.5, 0.5), (0.77, 0.34), (0.29, 0.86)];
    pts.iter()
        .map(|&(u, v): &(f64, f64)| {
            let x = ((u * b.width as f64) as i64).clamp(0, b.width as i64 - 1) as usize;
            let y = ((v * b.height as f64) as i64).clamp(0, b.height as i64 - 1) as usize;
            let o = b.index(x, y);
            format!("({},{},{})", g9(b.data[o]), g9(b.data[o + 1]), g9(b.data[o + 2]))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---- the 14 cases ------------------------------------------------------------
// ⚠️ Must stay word for word identical to PipelineHash.swift `cases()` and the C#
// `Cases()`, or the comparison loses its meaning.

pub fn cases() -> Vec<(&'static str, ImageAdjustments)> {
    let d = ImageAdjustments::default;
    let mut out = Vec::new();

    out.push(("v1 預設（LUT 恆等 + 白平衡中性）", d()));

    out.push((
        "舊版 曝光/色溫/色調/對比",
        ImageAdjustments { exposure: 0.6, temperature: 4200.0, tint: 15.0, contrast: 20.0, pipeline_version: 0, ..d() },
    ));

    out.push(("v1 曝光/色溫/色調", ImageAdjustments { exposure: 1.2, temperature: 3600.0, tint: -20.0, ..d() }));

    out.push((
        "v1 色調曲線全開",
        ImageAdjustments { contrast: 35.0, highlights: -40.0, shadows: 30.0, whites: 15.0, blacks: -20.0, ..d() },
    ));

    out.push(("鮮豔度/飽和度", ImageAdjustments { vibrance: 40.0, saturation: -25.0, ..d() }));

    out.push(("降噪 + 銳利化", ImageAdjustments { noise_reduction: 60.0, sharpening: 70.0, ..d() }));

    out.push(("柔化（負銳利度）", ImageAdjustments { sharpening: -50.0, ..d() }));

    out.push(("漸層 ×2", with_gradients(d())));

    out.push(("漸層 + 銳利化（漸層獨立 pass）", with_gradients(ImageAdjustments { sharpening: 30.0, ..d() })));

    out.push(("暗角", ImageAdjustments { vignette: 60.0, ..d() }));

    out.push((
        "裁切 + 角度",
        ImageAdjustments { crop_x: 0.1, crop_y: 0.15, crop_width: 0.7, crop_height: 0.6, crop_angle: 7.0, ..d() },
    ));

    out.push(("廣角變形", ImageAdjustments { distortion: 40.0, ..d() }));

    out.push((
        "旋轉 90 + 裁切",
        ImageAdjustments {
            rotation: Rotation::R90,
            crop_x: 0.05,
            crop_y: 0.05,
            crop_width: 0.9,
            crop_height: 0.8,
            ..d()
        },
    ));

    out.push(("綜合", combined()));
    out
}

pub fn combined() -> ImageAdjustments {
    with_gradients(ImageAdjustments {
        exposure: 0.4,
        temperature: 6200.0,
        tint: 8.0,
        contrast: 15.0,
        highlights: -25.0,
        shadows: 20.0,
        vibrance: 20.0,
        saturation: 5.0,
        noise_reduction: 30.0,
        sharpening: 40.0,
        vignette: 35.0,
        crop_x: 0.05,
        crop_y: 0.05,
        crop_width: 0.85,
        crop_height: 0.85,
        crop_angle: -3.0,
        distortion: -20.0,
        ..ImageAdjustments::default()
    })
}

fn with_gradients(mut a: ImageAdjustments) -> ImageAdjustments {
    let g1 = LinearGradient {
        center_x: 0.5,
        center_y: 0.3,
        angle: 0.0,
        range: 0.25,
        exposure: -0.8,
        contrast: 10.0,
        saturation: -20.0,
        ..LinearGradient::default()
    };
    let g2 = LinearGradient {
        center_x: 0.4,
        center_y: 0.7,
        angle: 35.0,
        range: 0.2,
        exposure: 0.5,
        highlights: -30.0,
        shadows: 20.0,
        ..LinearGradient::default()
    };
    a.gradients = vec![g1, g2];
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g9_matches_printf() {
        assert_eq!(g9(0.44609338), "0.44609338");
        assert_eq!(g9(0.5), "0.5");
        assert_eq!(g9(1.0), "1");
        assert_eq!(g9(0.000012345), "1.2345E-05");
        assert_eq!(g9(3.0e-7), "3.00000011E-07");
        assert_eq!(g9(123456789.0), "123456792");
        assert_eq!(g9(0.0001), "9.99999975E-05");
        assert_eq!(g9(0.218250602), "0.218250602");
    }
}
