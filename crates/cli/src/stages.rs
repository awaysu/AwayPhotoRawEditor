//! `stages <raw>` — SHA of each intermediate the pipeline depends on, so two platforms
//! that disagree in `hashtest` can be bisected to the first stage that differs.

use awpr_core::color::{self, WhiteBalanceReference};
use awpr_core::{libraw, resize, tone, ImageAdjustments};
use sha2::{Digest, Sha256};

fn sha_f32(v: &[f32]) -> String {
    let mut h = Sha256::new();
    for x in v {
        h.update(x.to_bits().to_le_bytes());
    }
    h.finalize().iter().take(12).map(|b| format!("{b:02X}")).collect()
}

fn sha_f64(v: &[f64]) -> String {
    let mut h = Sha256::new();
    for x in v {
        h.update(x.to_bits().to_le_bytes());
    }
    h.finalize().iter().take(12).map(|b| format!("{b:02X}")).collect()
}

pub fn run(path: &str) -> i32 {
    println!("decode LUT      : {}", sha_f32(color::decode_lut()));
    println!("encode LUT      : {}", sha_f32(color::encode_lut()));
    println!("tone LUT (def)  : {}", sha_f32(&tone::build_lut(&ImageAdjustments::default())));
    let cam = libraw::read_camera_color(path);
    if let Some(c) = &cam {
        println!("camera colour   : {}", sha_f64(&[c.pre_mul.as_slice(), &c.cam_mul, &c.rgb_cam].concat()));
        if let Some(m) = color::white_balance_matrix(c, 5200.0, 0.0, WhiteBalanceReference::Decode) {
            println!("WB matrix 5200  : {}", sha_f32(&m));
        }
        if let Some((k, t)) = color::as_shot(c) {
            println!("as-shot         : {k:?} / {t:?}");
        }
    }
    let Some(full) = libraw::decode_full(path, 16, None) else {
        println!("!! decode failed");
        return 1;
    };
    println!("full decode     : {} ({} x {})", sha_f32(&full.data), full.width, full.height);
    let proxy = resize::resize_to_max_dim(full, 2560);
    println!("proxy 2560      : {} ({} x {})", sha_f32(&proxy.data), proxy.width, proxy.height);
    0
}
