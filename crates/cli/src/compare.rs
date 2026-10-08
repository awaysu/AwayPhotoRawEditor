//! Cross-platform source comparison.
//!
//! `dumpsrc <raw> <out.f32>` writes the 2560 proxy hashtest starts from (width, height,
//! then RGBA f32 little-endian). `cmpsrc <raw> <other.f32>` renders the 14 hashtest cases
//! from both the local proxy and the other platform's, and reports the differences with
//! the gputest criteria (8-bit diff ≥ 2 never allowed; diff 1 on ≤ 0.07% of channels).
//! The pipeline code and its LUTs are identical across platforms (see `stages`), so this
//! is exactly the difference the two platforms' outputs would show.

use crate::hashtest::{self, to_byte};
use awpr_core::color::WhiteBalanceReference;
use awpr_core::{apply_to_float, FloatImage, ProcessContext};

pub fn dump(path: &str, out: &str) -> i32 {
    let Some((src, _)) = hashtest::load_source(path) else {
        eprintln!("!! decode failed");
        return 1;
    };
    let mut bytes = Vec::with_capacity(8 + src.data.len() * 4);
    bytes.extend_from_slice(&(src.width as u32).to_le_bytes());
    bytes.extend_from_slice(&(src.height as u32).to_le_bytes());
    for v in &src.data {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(out, bytes).expect("write dump");
    println!("wrote {out} ({} x {})", src.width, src.height);
    0
}

fn load(path: &str) -> FloatImage {
    let b = std::fs::read(path).expect("read dump");
    let w = u32::from_le_bytes(b[0..4].try_into().unwrap()) as usize;
    let h = u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize;
    let data = b[8..].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    FloatImage { width: w, height: h, data }
}

struct Diff {
    max: f32,
    d1: usize,
    d2: usize,
    total: usize,
}

fn diff(a: &FloatImage, b: &FloatImage) -> Diff {
    let mut d = Diff { max: 0.0, d1: 0, d2: 0, total: a.width * a.height * 3 };
    for (i, (x, y)) in a.data.iter().zip(&b.data).enumerate() {
        if i % 4 == 3 {
            continue;
        }
        d.max = d.max.max((x - y).abs());
        let bd = (to_byte(*x) as i32 - to_byte(*y) as i32).abs();
        if bd >= 2 {
            d.d2 += 1;
        } else if bd == 1 {
            d.d1 += 1;
        }
    }
    d
}

pub fn run(path: &str, other: &str) -> i32 {
    let Some((local, cam)) = hashtest::load_source(path) else {
        eprintln!("!! decode failed");
        return 1;
    };
    let remote = load(other);
    if (local.width, local.height) != (remote.width, remote.height) {
        println!(
            "❌ source size differs: {}x{} vs {}x{}",
            local.width, local.height, remote.width, remote.height
        );
        return 1;
    }
    let s = diff(&local, &remote);
    println!(
        "[來源 proxy]  最大差 {:.2e}  8-bit 差1 {:.4}%  差≥2 {}",
        s.max,
        s.d1 as f64 / s.total as f64 * 100.0,
        s.d2
    );
    let ctx = ProcessContext {
        camera: cam,
        white_balance_reference: WhiteBalanceReference::Decode,
        ..Default::default()
    };
    let mut failures = 0;
    for (name, adj) in hashtest::cases() {
        let a = apply_to_float(&local, &adj, &ctx);
        let b = apply_to_float(&remote, &adj, &ctx);
        let d = diff(&a, &b);
        let pct = d.d1 as f64 / d.total as f64 * 100.0;
        let ok = d.d2 == 0 && pct <= 0.07;
        if !ok {
            failures += 1;
        }
        println!(
            "{} [{name}]  最大差 {:.2e}  8-bit 差1 {:.4}%  差≥2 {}",
            if ok { "✅" } else { "❌" },
            d.max,
            pct,
            d.d2
        );
    }
    println!("{}", if failures == 0 { "全部在容許範圍內 ✅" } else { "有超出容許範圍 ❌" });
    i32::from(failures != 0)
}
