//! The WGSL kernels. Each is a line-by-line transliteration of the Metal kernel in
//! `AwayPhotoRawEditor_Swift/…/MetalShaders.swift`, which in turn transliterates the CPU
//! function it replaces — same formulas, same uploaded lookup tables (rather than `pow()`
//! in the shader, so the two paths cannot drift), same clamp positions.
//!
//! One module per kernel: wgpu derives each pipeline's bind-group layout from the bindings
//! its entry point uses, and separate modules keep the binding numbers unambiguous.
//!
//! WGSL has no f64, so the geometry stages compute in f32 where the CPU uses f64 — the
//! same compromise the Metal and Direct3D paths make. That is where most of the CPU/GPU
//! difference comes from.

pub const WORKGROUP: u32 = 16;

const LUT_HELPERS: &str = r#"
const DECODE_LUT_SIZE: i32 = 4096;
const ENCODE_LUT_SIZE: i32 = 8192;
const TONE_LUT_SIZE: i32 = 1024;

fn clamp0(v: f32) -> f32 { return select(v, 0.0, v < 0.0); }
"#;

pub const PIXEL: &str = r#"
const FLAG_LEGACY_WB: u32       = 1u;
const FLAG_LINEAR_MUL: u32      = 2u;
const FLAG_LINEAR_MATRIX: u32   = 4u;
const FLAG_TONE_LUT: u32        = 8u;
const FLAG_VIB_SAT: u32         = 16u;
const FLAG_GRADIENTS: u32       = 32u;
const FLAG_GRADIENT_LINEAR: u32 = 64u;
const FLAG_VIGNETTE: u32        = 128u;

struct PixelParams {
    flags: u32,
    gradient_count: u32,
    wb_r: f32, wb_g: f32, wb_b: f32,
    m0: f32, m1: f32, m2: f32, m3: f32, m4: f32, m5: f32, m6: f32, m7: f32, m8: f32,
    sat: f32, vib: f32,
    vig_amount: f32, vig_cx: f32, vig_cy: f32, vig_inv_max: f32,
    width: u32, height: u32,
    _pad0: u32, _pad1: u32,
};

struct Gradient {
    sin_a: f32, cos_a: f32,
    center_x: f32, center_y: f32,
    inv2_range: f32,          // 1 / (2 * max(1e-3, range))
    exposure: f32,
    contrast: f32, highlights: f32, shadows: f32, saturation: f32,
};

@group(0) @binding(0) var<storage, read_write> img: array<vec4<f32>>;
@group(0) @binding(1) var<uniform> p: PixelParams;
@group(0) @binding(2) var<storage, read> decode_lut: array<f32>;
@group(0) @binding(3) var<storage, read> encode_lut: array<f32>;
@group(0) @binding(4) var<storage, read> tone_lut: array<f32>;
@group(0) @binding(5) var<storage, read> gradients: array<Gradient>;

// LUT sampling — identical to color::sample / tone::sample.
fn linearize(x: f32) -> f32 {
    if (!(x > 0.0)) { return 0.0; }            // also catches NaN
    if (x >= 1.0) { return decode_lut[DECODE_LUT_SIZE]; }
    let f = x * f32(DECODE_LUT_SIZE);
    let i = i32(f);
    let t = f - f32(i);
    return decode_lut[i] + (decode_lut[i + 1] - decode_lut[i]) * t;
}

fn encode_val(x: f32) -> f32 {
    if (!(x > 0.0)) { return 0.0; }
    if (x >= 1.0) { return encode_lut[ENCODE_LUT_SIZE]; }
    let f = x * f32(ENCODE_LUT_SIZE);
    let i = i32(f);
    let t = f - f32(i);
    return encode_lut[i] + (encode_lut[i + 1] - encode_lut[i]) * t;
}

// tone::sample differs: it indexes with (size - 1) and clamps to the end entries.
fn sample_tone(x: f32) -> f32 {
    if (x <= 0.0) { return tone_lut[0]; }
    if (x >= 1.0) { return tone_lut[TONE_LUT_SIZE - 1]; }
    let f = x * f32(TONE_LUT_SIZE - 1);
    let i = i32(f);
    let frac = f - f32(i);
    return tone_lut[i] + (tone_lut[i + 1] - tone_lut[i]) * frac;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.width || gid.y >= p.height) { return; }
    let idx = gid.y * p.width + gid.x;
    let px = img[idx];
    var r = px.r;
    var g = px.g;
    var b = px.b;

    // 1 + 2: white balance and exposure
    if ((p.flags & FLAG_LEGACY_WB) != 0u) {
        r *= p.wb_r; g *= p.wb_g; b *= p.wb_b;
    } else if ((p.flags & FLAG_LINEAR_MATRIX) != 0u) {
        let lr = linearize(r);
        let lg = linearize(g);
        let lb = linearize(b);
        r = encode_val(p.m0 * lr + p.m1 * lg + p.m2 * lb);
        g = encode_val(p.m3 * lr + p.m4 * lg + p.m5 * lb);
        b = encode_val(p.m6 * lr + p.m7 * lg + p.m8 * lb);
    } else if ((p.flags & FLAG_LINEAR_MUL) != 0u) {
        r = encode_val(linearize(r) * p.wb_r);
        g = encode_val(linearize(g) * p.wb_g);
        b = encode_val(linearize(b) * p.wb_b);
    }

    // 3: tone LUT
    if ((p.flags & FLAG_TONE_LUT) != 0u) {
        r = sample_tone(r);
        g = sample_tone(g);
        b = sample_tone(b);
    }

    // 4: vibrance / saturation
    if ((p.flags & FLAG_VIB_SAT) != 0u) {
        let luma = 0.299 * r + 0.587 * g + 0.114 * b;
        let mx = max(r, max(g, b));
        let mn = min(r, min(g, b));
        let cur_sat = select((mx - mn) / mx, 0.0, mx <= 1e-4);
        let f = (1.0 + p.vib * (1.0 - cur_sat)) * (1.0 + p.sat);
        r = clamp0(luma + (r - luma) * f);
        g = clamp0(luma + (g - luma) * f);
        b = clamp0(luma + (b - luma) * f);
    }

    // 7: graduated filters, stacked in order over the running value
    if ((p.flags & FLAG_GRADIENTS) != 0u) {
        let linear_exposure = (p.flags & FLAG_GRADIENT_LINEAR) != 0u;
        let nx = f32(gid.x) / f32(p.width);
        let ny = f32(gid.y) / f32(p.height);
        for (var i = 0u; i < p.gradient_count; i++) {
            let gr = gradients[i];
            let d = (nx - gr.center_x) * gr.sin_a + (ny - gr.center_y) * gr.cos_a;
            var m = clamp(d * gr.inv2_range + 0.5, 0.0, 1.0);
            m = m * m * (3.0 - 2.0 * m);          // smoothstep
            if (m <= 0.0) { continue; }

            if (gr.exposure != 0.0) {
                let exp_mul = pow(2.0, gr.exposure * m);
                if (linear_exposure) {
                    r = encode_val(linearize(r) * exp_mul);
                    g = encode_val(linearize(g) * exp_mul);
                    b = encode_val(linearize(b) * exp_mul);
                } else {
                    r *= exp_mul; g *= exp_mul; b *= exp_mul;
                }
            }

            let luma = 0.299 * r + 0.587 * g + 0.114 * b;
            if (gr.saturation != 0.0) {
                let f = 1.0 + gr.saturation * m;
                r = luma + (r - luma) * f;
                g = luma + (g - luma) * f;
                b = luma + (b - luma) * f;
            }
            if (gr.contrast != 0.0) {
                let c = gr.contrast * m;
                r = 0.5 + (r - 0.5) * (1.0 + c);
                g = 0.5 + (g - 0.5) * (1.0 + c);
                b = 0.5 + (b - 0.5) * (1.0 + c);
            }
            if (gr.highlights != 0.0) {
                let w_h = luma * luma * gr.highlights * 0.5 * m;
                r += w_h; g += w_h; b += w_h;
            }
            if (gr.shadows != 0.0) {
                let w_s = (1.0 - luma) * (1.0 - luma) * gr.shadows * 0.5 * m;
                r += w_s; g += w_s; b += w_s;
            }
            r = clamp0(r); g = clamp0(g); b = clamp0(b);
        }
    }

    // 10c: vignette
    if ((p.flags & FLAG_VIGNETTE) != 0u) {
        let dx = (f32(gid.x) - p.vig_cx) * p.vig_inv_max;
        let dy = (f32(gid.y) - p.vig_cy) * p.vig_inv_max;
        let rad = sqrt(dx * dx + dy * dy);
        var m = clamp((rad - 0.35) / 0.65, 0.0, 1.0);
        m = m * m * (3.0 - 2.0 * m);
        if (m > 0.0) {
            let gn = max(0.0, 1.0 + p.vig_amount * m);
            r *= gn; g *= gn; b *= gn;
        }
    }

    img[idx] = vec4<f32>(r, g, b, px.a);
}
"#;

/// 處理版本 3's colour pass: a transliteration of `awpr_core::v3::pixel`. Parameters come
/// from `V3Params::words` (offsets `W_*`, repeated below), the three tone tables from
/// `v3::build_luts`. Standalone (does not use `module`).
pub const PIXEL_V3: &str = r#"
const W_LINEAR: u32 = 0u;
const W_COLOR: u32 = 1u;
const W_MIXER: u32 = 2u;
const W_HL: u32 = 3u;
const W_VIB: u32 = 4u;
const W_SAT: u32 = 5u;
const W_SKIN: u32 = 6u;
const W_WIDTH: u32 = 7u;
const W_HEIGHT: u32 = 8u;
const W_WB: u32 = 9u;
const W_M_IN: u32 = 12u;
const W_OUT: u32 = 21u;
const W_OUT_Y: u32 = 30u;
const W_TO_LMS: u32 = 33u;
const W_FROM_LMS: u32 = 42u;
const W_TO_LAB: u32 = 51u;
const W_FROM_LAB: u32 = 60u;
const W_BANDS: u32 = 69u;
const W_HUE: u32 = 77u;
const W_HSAT: u32 = 85u;
const W_HLUM: u32 = 93u;

const LUT_SIZE: u32 = 4096u;
const LUT_MAX: f32 = 2.0;
const PI: f32 = 3.14159265358979;
const TAU: f32 = 6.28318530717959;

@group(0) @binding(0) var<storage, read_write> img: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> w: array<f32>;
@group(0) @binding(2) var<storage, read> luts: array<f32>;

fn mul3(o: u32, v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        w[o] * v.x + w[o + 1u] * v.y + w[o + 2u] * v.z,
        w[o + 3u] * v.x + w[o + 4u] * v.y + w[o + 5u] * v.z,
        w[o + 6u] * v.x + w[o + 7u] * v.y + w[o + 8u] * v.z);
}

fn oetf(l: f32) -> f32 {
    if (!(l > 0.0)) { return 0.0; }
    if (l < 0.018) { return 4.5 * l; }
    return 1.099 * pow(l, 0.45) - 0.099;
}

fn eotf(v: f32) -> f32 {
    if (!(v > 0.0)) { return 0.0; }
    if (v < 0.081) { return v / 4.5; }
    return pow((v + 0.099) / 1.099, 1.0 / 0.45);
}

fn cbrt_s(x: f32) -> f32 {
    if (x == 0.0) { return 0.0; }
    if (x < 0.0) { return -pow(-x, 1.0 / 3.0); }
    return pow(x, 1.0 / 3.0);
}

fn oklab_from(rgb: vec3<f32>) -> vec3<f32> {
    let l = mul3(W_TO_LMS, rgb);
    return mul3(W_TO_LAB, vec3<f32>(cbrt_s(l.x), cbrt_s(l.y), cbrt_s(l.z)));
}

fn oklab_to(lab: vec3<f32>) -> vec3<f32> {
    let l = mul3(W_FROM_LAB, lab);
    return mul3(W_FROM_LMS, l * l * l);
}

fn wrap(a: f32) -> f32 {
    let t = a + PI;
    return t - TAU * floor(t / TAU) - PI;
}

fn rem_tau(a: f32) -> f32 {
    return a - TAU * floor(a / TAU);
}

fn sample_lut(c: u32, x: f32) -> f32 {
    let base = c * LUT_SIZE;
    if (!(x > 0.0)) { return luts[base]; }
    let f = min(x, LUT_MAX) * (f32(LUT_SIZE - 1u) / LUT_MAX);
    let i = min(u32(f), LUT_SIZE - 2u);
    let t = f - f32(i);
    return luts[base + i] + (luts[base + i + 1u] - luts[base + i]) * t;
}

fn vib_factor(c: f32, h: f32) -> f32 {
    let vib = w[W_VIB];
    let low = 1.0 - clamp(c / 0.22, 0.0, 1.0);
    var skin = 1.0;
    if (vib > 0.0) {
        let d = wrap(h - w[W_SKIN]) / 0.35;
        skin = 1.0 - 0.6 * exp(-(d * d));
    }
    return max(1.0 + vib * low * low * skin * 1.2, 0.0);
}

fn color_ops(rgb: vec3<f32>) -> vec3<f32> {
    var lab = oklab_from(rgb);
    if (w[W_MIXER] == 0.0) {
        let c0 = sqrt(lab.y * lab.y + lab.z * lab.z);
        var c = c0;
        if (w[W_VIB] != 0.0) {
            c *= vib_factor(c, atan2(lab.z, lab.y));
        }
        if (w[W_SAT] != 0.0) {
            c *= max(1.0 + w[W_SAT], 0.0);
        }
        if (c0 > 0.0) {
            let k = c / c0;
            lab.y *= k;
            lab.z *= k;
        }
        return oklab_to(lab);
    }
    var l = lab.x;
    var c = sqrt(lab.y * lab.y + lab.z * lab.z);
    var h = atan2(lab.z, lab.y);
    // band weights (v3::band_weights)
    var dh = 0.0;
    var ds = 0.0;
    var dl = 0.0;
    for (var i = 0u; i < 8u; i++) {
        let a = w[W_BANDS + i];
        let b = w[W_BANDS + (i + 1u) % 8u];
        let span = rem_tau(wrap(b - a));
        let d = rem_tau(wrap(h - a));
        if (d <= span) {
            let t = d / span;
            let s = 0.5 - 0.5 * cos(t * PI);
            let j = (i + 1u) % 8u;
            dh = (1.0 - s) * w[W_HUE + i] + s * w[W_HUE + j];
            ds = (1.0 - s) * w[W_HSAT + i] + s * w[W_HSAT + j];
            dl = (1.0 - s) * w[W_HLUM + i] + s * w[W_HLUM + j];
            break;
        }
    }
    let chroma_w = min(c / 0.12, 1.0);
    h += dh * chroma_w;
    c *= max(1.0 + ds, 0.0);
    l += dl * chroma_w * sqrt(max(l, 0.05));
    if (w[W_VIB] != 0.0) {
        c *= vib_factor(c, h);
    }
    if (w[W_SAT] != 0.0) {
        c *= max(1.0 + w[W_SAT], 0.0);
    }
    return oklab_to(vec3<f32>(l, c * cos(h), c * sin(h)));
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let width = u32(w[W_WIDTH]);
    let height = u32(w[W_HEIGHT]);
    if (gid.x >= width || gid.y >= height) { return; }
    let idx = gid.y * width + gid.x;
    let px = img[idx];
    var v = px.rgb;
    if (w[W_LINEAR] != 0.0) {
        let wb = vec3<f32>(w[W_WB], w[W_WB + 1u], w[W_WB + 2u]);
        let lim = min(wb.x, min(wb.y, wb.z));
        let cn = min(v * wb, vec3<f32>(lim)) / wb;
        v = cn + w[W_HL] * (v - cn);
    } else {
        v = vec3<f32>(eotf(v.x), eotf(v.y), eotf(v.z));
    }
    var lin = mul3(W_M_IN, v);
    lin = vec3<f32>(eotf(sample_lut(0u, oetf(lin.x))), eotf(sample_lut(1u, oetf(lin.y))), eotf(sample_lut(2u, oetf(lin.z))));
    if (w[W_COLOR] != 0.0) {
        lin = color_ops(lin);
    }
    var s = mul3(W_OUT, lin);
    let yy = clamp(w[W_OUT_Y] * s.x + w[W_OUT_Y + 1u] * s.y + w[W_OUT_Y + 2u] * s.z, 0.0, 1.0);
    var tg = 1.0;
    for (var k = 0u; k < 3u; k++) {
        let c = s[k];
        if (c < 0.0) {
            tg = min(tg, yy / max(yy - c, 1e-9));
        } else if (c > 1.0) {
            tg = min(tg, (1.0 - yy) / max(c - yy, 1e-9));
        }
    }
    if (tg < 1.0) {
        s = yy + (s - yy) * tg;
    }
    img[idx] = vec4<f32>(oetf(clamp(s.x, 0.0, 1.0)), oetf(clamp(s.y, 0.0, 1.0)), oetf(clamp(s.z, 0.0, 1.0)), px.a);
}
"#;

/// 處理版本 3's 遮罩 pass: `awpr_core::masks::mask_px` for every active mask, with the
/// weights the CPU rasterized (`masks::packed_weights`) and the parameter words of
/// `masks::words`. Uses `module` (the LUT sizes and clamp0).
pub const MASKS: &str = r#"
@group(0) @binding(0) var<storage, read_write> img: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> w: array<f32>;
@group(0) @binding(2) var<storage, read> weights: array<f32>;
@group(0) @binding(3) var<storage, read> decode_lut: array<f32>;
@group(0) @binding(4) var<storage, read> encode_lut: array<f32>;

fn linearize(x: f32) -> f32 {
    if (!(x > 0.0)) { return 0.0; }
    if (x >= 1.0) { return decode_lut[DECODE_LUT_SIZE]; }
    let f = x * f32(DECODE_LUT_SIZE);
    let i = i32(f);
    let t = f - f32(i);
    return decode_lut[i] + (decode_lut[i + 1] - decode_lut[i]) * t;
}

fn encode_val(x: f32) -> f32 {
    if (!(x > 0.0)) { return 0.0; }
    if (x >= 1.0) { return encode_lut[ENCODE_LUT_SIZE]; }
    let f = x * f32(ENCODE_LUT_SIZE);
    let i = i32(f);
    let t = f - f32(i);
    return encode_lut[i] + (encode_lut[i + 1] - encode_lut[i]) * t;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let count = u32(w[0]);
    let width = u32(w[1]);
    let height = u32(w[2]);
    if (gid.x >= width || gid.y >= height) { return; }
    let idx = gid.y * width + gid.x;
    let px = img[idx];
    var r = px.r;
    var g = px.g;
    var b = px.b;
    for (var k = 0u; k < count; k++) {
        let m = weights[k * width * height + idx];
        if (m <= 0.0) { continue; }
        let o = 4u + k * 5u;
        let exposure = w[o];
        let contrast = w[o + 1u];
        let highlights = w[o + 2u];
        let shadows = w[o + 3u];
        let saturation = w[o + 4u];
        if (exposure != 0.0) {
            let exp_mul = pow(2.0, exposure * m);
            r = encode_val(linearize(r) * exp_mul);
            g = encode_val(linearize(g) * exp_mul);
            b = encode_val(linearize(b) * exp_mul);
        }
        let luma = 0.299 * r + 0.587 * g + 0.114 * b;
        if (saturation != 0.0) {
            let f = 1.0 + saturation * m;
            r = luma + (r - luma) * f;
            g = luma + (g - luma) * f;
            b = luma + (b - luma) * f;
        }
        if (contrast != 0.0) {
            let c = contrast * m;
            r = 0.5 + (r - 0.5) * (1.0 + c);
            g = 0.5 + (g - 0.5) * (1.0 + c);
            b = 0.5 + (b - 0.5) * (1.0 + c);
        }
        if (highlights != 0.0) {
            let w_h = luma * luma * highlights * 0.5 * m;
            r += w_h; g += w_h; b += w_h;
        }
        if (shadows != 0.0) {
            let w_s = (1.0 - luma) * (1.0 - luma) * shadows * 0.5 * m;
            r += w_s; g += w_s; b += w_s;
        }
        r = clamp0(r); g = clamp0(g); b = clamp0(b);
    }
    img[idx] = vec4<f32>(r, g, b, px.a);
}
"#;

const BLUR_PARAMS: &str = r#"
struct BlurParams {
    width: u32, height: u32,
    radius: i32,
    mode: i32,               // 0 = blend (denoise / soften), 1 = unsharp
    amount: f32,
    _pad0: u32, _pad1: u32, _pad2: u32,
};
"#;

pub const BLUR_H: &str = r#"
@group(0) @binding(0) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> p: BlurParams;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.width || gid.y >= p.height) { return; }
    let w = i32(p.width);
    let row = i32(gid.y) * w;
    var acc = vec3<f32>(0.0);
    for (var k = -p.radius; k <= p.radius; k++) {
        let xx = clamp(i32(gid.x) + k, 0, w - 1);
        acc += src[row + xx].rgb;
    }
    let norm = 1.0 / f32(p.radius * 2 + 1);
    let o = row + i32(gid.x);
    dst[o] = vec4<f32>(acc * norm, src[o].a);
}
"#;

pub const BLUR_V: &str = r#"
@group(0) @binding(0) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> p: BlurParams;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.width || gid.y >= p.height) { return; }
    let w = i32(p.width);
    let h = i32(p.height);
    var acc = vec3<f32>(0.0);
    for (var k = -p.radius; k <= p.radius; k++) {
        let yy = clamp(i32(gid.y) + k, 0, h - 1);
        acc += src[yy * w + i32(gid.x)].rgb;
    }
    let norm = 1.0 / f32(p.radius * 2 + 1);
    let o = i32(gid.y) * w + i32(gid.x);
    dst[o] = vec4<f32>(acc * norm, src[o].a);
}
"#;

/// Combines the blurred copy back into the image: blend for denoise/soften, unsharp for
/// sharpening. Matches pipeline::apply_blur_op.
pub const BLUR_COMBINE: &str = r#"
@group(0) @binding(0) var<storage, read_write> img: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> blurred: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> p: BlurParams;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.width || gid.y >= p.height) { return; }
    let idx = gid.y * p.width + gid.x;
    let a = img[idx];
    let bl = blurred[idx].rgb;
    var out_rgb: vec3<f32>;
    if (p.mode == 1) {
        out_rgb = vec3<f32>(clamp0(a.r + p.amount * (a.r - bl.r)),
                            clamp0(a.g + p.amount * (a.g - bl.g)),
                            clamp0(a.b + p.amount * (a.b - bl.b)));
    } else {
        let amt = clamp(p.amount, 0.0, 1.0);
        out_rgb = a.rgb + (bl - a.rgb) * amt;
    }
    img[idx] = vec4<f32>(out_rgb, a.a);
}
"#;

pub const RESAMPLE: &str = r#"
struct ResampleParams {
    src_width: u32, src_height: u32,
    out_width: u32, out_height: u32,
    mode: i32,               // 0 = radial distortion, 1 = rotate about (cx,cy)
    k: f32,
    cx: f32, cy: f32, sin_a: f32, cos_a: f32, ox: f32, oy: f32,
};

@group(0) @binding(0) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> p: ResampleParams;

fn sample_bilinear(w: i32, h: i32, fx_in: f32, fy_in: f32) -> vec4<f32> {
    let fx = clamp(fx_in, 0.0, f32(w - 1));
    let fy = clamp(fy_in, 0.0, f32(h - 1));
    let x0 = i32(fx);
    let y0 = i32(fy);
    let x1 = min(x0 + 1, w - 1);
    let y1 = min(y0 + 1, h - 1);
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    let v00 = src[y0 * w + x0];
    let v10 = src[y0 * w + x1];
    let v01 = src[y1 * w + x0];
    let v11 = src[y1 * w + x1];
    let top = v00 + (v10 - v00) * tx;
    let bot = v01 + (v11 - v01) * tx;
    return top + (bot - top) * ty;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.out_width || gid.y >= p.out_height) { return; }
    let w = i32(p.src_width);
    let h = i32(p.src_height);
    var sx: f32;
    var sy: f32;
    if (p.mode == 0) {
        let nx = (f32(gid.x) / f32(w) - 0.5) * 2.0;
        let ny = (f32(gid.y) / f32(h) - 0.5) * 2.0;
        let r2 = nx * nx + ny * ny;
        let f = 1.0 + p.k * r2;
        sx = (nx * f / 2.0 + 0.5) * f32(w);
        sy = (ny * f / 2.0 + 0.5) * f32(h);
    } else {
        let rx = f32(gid.x) - p.ox;
        let ry = f32(gid.y) - p.oy;
        sx = p.cx + (rx * p.cos_a - ry * p.sin_a);
        sy = p.cy + (rx * p.sin_a + ry * p.cos_a);
    }
    dst[gid.y * p.out_width + gid.x] = sample_bilinear(w, h, sx, sy);
}
"#;

pub const ROTATE: &str = r#"
struct RotateParams {
    width: u32, height: u32,
    rot: i32,                // 90, 180, 270
    _pad0: u32,
};

@group(0) @binding(0) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> p: RotateParams;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.width || gid.y >= p.height) { return; }
    let w = i32(p.width);
    let h = i32(p.height);
    let x = i32(gid.x);
    let y = i32(gid.y);
    var nx: i32;
    var ny: i32;
    var dw: i32;
    if (p.rot == 90)       { nx = h - 1 - y; ny = x;         dw = h; }
    else if (p.rot == 180) { nx = w - 1 - x; ny = h - 1 - y; dw = w; }
    else                   { nx = y;         ny = w - 1 - x; dw = h; }
    dst[ny * dw + nx] = src[y * w + x];
}
"#;

/// Full source of one kernel module: shared helpers, then its own declarations.
pub fn module(kernel: &str) -> String {
    let blur = if kernel.contains("BlurParams") { BLUR_PARAMS } else { "" };
    format!("{LUT_HELPERS}\n{blur}\n{kernel}")
}

/// Histogram workgroup width and the rows each invocation walks: few enough workgroups
/// that merging their local counts into the global bins stays cheap.
pub const HISTOGRAM_WG: u32 = 64;
pub const HISTOGRAM_ROWS: u32 = 64;

/// 8-bit RGB histogram, binned exactly like `ImageStats.computeHistogram`
/// (`Int(v * 255 + 0.5)` clamped to 0...255). Standalone (does not use `module`).
pub const HISTOGRAM: &str = r#"
@group(0) @binding(0) var<storage, read> img: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> bins: array<atomic<u32>, 768>;
@group(0) @binding(2) var<uniform> dims: vec4<u32>;

var<workgroup> local_bins: array<atomic<u32>, 768>;

fn bin(v: f32) -> u32 {
    return u32(clamp(i32(v * 255.0 + 0.5), 0, 255));
}

@compute @workgroup_size(64, 1, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>,
        @builtin(local_invocation_index) li: u32,
        @builtin(workgroup_id) wid: vec3<u32>) {
    for (var i = li; i < 768u; i += 64u) {
        atomicStore(&local_bins[i], 0u);
    }
    workgroupBarrier();
    let x = gid.x;
    if (x < dims.x) {
        let y0 = wid.y * 64u;
        let y1 = min(y0 + 64u, dims.y);
        for (var y = y0; y < y1; y += 1u) {
            let p = img[y * dims.x + x];
            atomicAdd(&local_bins[bin(p.r)], 1u);
            atomicAdd(&local_bins[256u + bin(p.g)], 1u);
            atomicAdd(&local_bins[512u + bin(p.b)], 1u);
        }
    }
    workgroupBarrier();
    for (var i = li; i < 768u; i += 64u) {
        let c = atomicLoad(&local_bins[i]);
        if (c != 0u) {
            atomicAdd(&bins[i], c);
        }
    }
}
"#;
