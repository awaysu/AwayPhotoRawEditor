import Foundation

/// The Metal kernels, embedded as source and compiled once at runtime.
///
/// Embedded rather than shipped as a `.metallib` resource so the shaders travel inside
/// the binary: the app bundle is hand-assembled by Scripts/build_app.sh, and a separate
/// resource bundle would be one more thing to copy and sign correctly.
///
/// **Every kernel is a line-by-line transliteration of the CPU function it replaces**
/// (`ImageProcessor+CPU.swift`) — same formulas, same lookup tables, same clamp
/// positions. The tables are uploaded rather than recomputed with `pow()` in-shader
/// precisely so the two paths cannot drift apart. Where they can still differ is the
/// geometry stage: the CPU reference computes in `double` and Metal has none, which is
/// the same compromise the Windows GPU path makes.
enum MetalShaders {
    static let source = """
#include <metal_stdlib>
using namespace metal;

// ---- flags, mirroring ImageProcessor.PixelFlags -----------------------------
constant uint FLAG_LEGACY_WB       = 1u << 0;
constant uint FLAG_LINEAR_MUL      = 1u << 1;
constant uint FLAG_LINEAR_MATRIX   = 1u << 2;
constant uint FLAG_TONE_LUT        = 1u << 3;
constant uint FLAG_VIB_SAT         = 1u << 4;
constant uint FLAG_GRADIENTS       = 1u << 5;
constant uint FLAG_GRADIENT_LINEAR = 1u << 6;
constant uint FLAG_VIGNETTE        = 1u << 7;

constant int DECODE_LUT_SIZE = 4096;
constant int ENCODE_LUT_SIZE = 8192;
constant int TONE_LUT_SIZE   = 1024;

struct PixelParams {
    uint  flags;
    uint  gradientCount;
    float wbR, wbG, wbB;        // white-balance multipliers, exposure folded in
    float m0, m1, m2, m3, m4, m5, m6, m7, m8;   // 3x3, exposure folded in
    float sat, vib;
    float vigAmount, vigCx, vigCy, vigInvMax;
    uint  width, height;
};

struct GradientGpu {
    float sinA, cosA;
    float centerX, centerY;
    float inv2Range;            // 1 / (2 * max(1e-3, range))
    float exposure;
    float contrast, highlights, shadows, saturation;
};

struct BlurParams {
    uint  width, height;
    int   radius;
    int   mode;                 // 0 = blend (denoise / soften), 1 = unsharp
    float amount;
};

struct ResampleParams {
    uint  srcWidth, srcHeight;
    uint  outWidth, outHeight;
    int   mode;                 // 0 = radial distortion, 1 = rotate about (cx,cy)
    float k;
    float cx, cy, sinA, cosA, ox, oy;
};

struct RotateParams {
    uint width, height;
    int  rot;                   // 90, 180, 270
};

// ---- LUT sampling — identical to ColorScience.sample / ToneCurve.sample -----

static inline float sampleLut(const device float *lut, int n, float x) {
    if (!(x > 0.0f)) return 0.0f;          // also catches NaN
    if (x >= 1.0f) return lut[n];
    float f = x * (float)n;
    int i = (int)f;
    float t = f - (float)i;
    return lut[i] + (lut[i + 1] - lut[i]) * t;
}

static inline float linearize(const device float *decodeLut, float v) {
    return sampleLut(decodeLut, DECODE_LUT_SIZE, v);
}

static inline float encodeVal(const device float *encodeLut, float l) {
    return sampleLut(encodeLut, ENCODE_LUT_SIZE, l);
}

// ToneCurve.sample differs: it indexes with (size - 1) and clamps to the end entries.
static inline float sampleTone(const device float *lut, float x) {
    if (x <= 0.0f) return lut[0];
    if (x >= 1.0f) return lut[TONE_LUT_SIZE - 1];
    float f = x * (float)(TONE_LUT_SIZE - 1);
    int i = (int)f;
    float frac = f - (float)i;
    return lut[i] + (lut[i + 1] - lut[i]) * frac;
}

static inline float clamp0(float v) { return v < 0.0f ? 0.0f : v; }

// ---- 1-4 + 7 + 10c: the fused per-pixel stage ------------------------------

kernel void pixelStage(device float4              *img          [[buffer(0)]],
                       constant PixelParams       &p            [[buffer(1)]],
                       const device float         *decodeLut    [[buffer(2)]],
                       const device float         *encodeLut    [[buffer(3)]],
                       const device float         *toneLut      [[buffer(4)]],
                       const device GradientGpu   *gradients    [[buffer(5)]],
                       uint2                       gid          [[thread_position_in_grid]])
{
    if (gid.x >= p.width || gid.y >= p.height) return;
    uint idx = gid.y * p.width + gid.x;
    float4 px = img[idx];
    float r = px.r, g = px.g, b = px.b;

    // 1 + 2: white balance and exposure
    if (p.flags & FLAG_LEGACY_WB) {
        r *= p.wbR; g *= p.wbG; b *= p.wbB;
    } else if (p.flags & FLAG_LINEAR_MATRIX) {
        float lr = linearize(decodeLut, r);
        float lg = linearize(decodeLut, g);
        float lb = linearize(decodeLut, b);
        r = encodeVal(encodeLut, p.m0 * lr + p.m1 * lg + p.m2 * lb);
        g = encodeVal(encodeLut, p.m3 * lr + p.m4 * lg + p.m5 * lb);
        b = encodeVal(encodeLut, p.m6 * lr + p.m7 * lg + p.m8 * lb);
    } else if (p.flags & FLAG_LINEAR_MUL) {
        r = encodeVal(encodeLut, linearize(decodeLut, r) * p.wbR);
        g = encodeVal(encodeLut, linearize(decodeLut, g) * p.wbG);
        b = encodeVal(encodeLut, linearize(decodeLut, b) * p.wbB);
    }

    // 3: tone LUT
    if (p.flags & FLAG_TONE_LUT) {
        r = sampleTone(toneLut, r);
        g = sampleTone(toneLut, g);
        b = sampleTone(toneLut, b);
    }

    // 4: vibrance / saturation
    if (p.flags & FLAG_VIB_SAT) {
        float luma = 0.299f * r + 0.587f * g + 0.114f * b;
        float mx = max(r, max(g, b));
        float mn = min(r, min(g, b));
        float curSat = mx <= 1e-4f ? 0.0f : (mx - mn) / mx;
        float f = (1.0f + p.vib * (1.0f - curSat)) * (1.0f + p.sat);
        r = clamp0(luma + (r - luma) * f);
        g = clamp0(luma + (g - luma) * f);
        b = clamp0(luma + (b - luma) * f);
    }

    // 7: graduated filters, stacked in order over the running value
    if (p.flags & FLAG_GRADIENTS) {
        bool linearExposure = (p.flags & FLAG_GRADIENT_LINEAR) != 0;
        float nx = (float)gid.x / (float)p.width;
        float ny = (float)gid.y / (float)p.height;
        for (uint i = 0; i < p.gradientCount; i++) {
            GradientGpu gr = gradients[i];
            float d = (nx - gr.centerX) * gr.sinA + (ny - gr.centerY) * gr.cosA;
            float m = clamp(d * gr.inv2Range + 0.5f, 0.0f, 1.0f);
            m = m * m * (3.0f - 2.0f * m);          // smoothstep
            if (m <= 0.0f) continue;

            if (gr.exposure != 0.0f) {
                float expMul = pow(2.0f, gr.exposure * m);
                if (linearExposure) {
                    r = encodeVal(encodeLut, linearize(decodeLut, r) * expMul);
                    g = encodeVal(encodeLut, linearize(decodeLut, g) * expMul);
                    b = encodeVal(encodeLut, linearize(decodeLut, b) * expMul);
                } else {
                    r *= expMul; g *= expMul; b *= expMul;
                }
            }

            float luma = 0.299f * r + 0.587f * g + 0.114f * b;
            if (gr.saturation != 0.0f) {
                float f = 1.0f + gr.saturation * m;
                r = luma + (r - luma) * f;
                g = luma + (g - luma) * f;
                b = luma + (b - luma) * f;
            }
            if (gr.contrast != 0.0f) {
                float c = gr.contrast * m;
                r = 0.5f + (r - 0.5f) * (1.0f + c);
                g = 0.5f + (g - 0.5f) * (1.0f + c);
                b = 0.5f + (b - 0.5f) * (1.0f + c);
            }
            if (gr.highlights != 0.0f) {
                float wH = luma * luma * gr.highlights * 0.5f * m;
                r += wH; g += wH; b += wH;
            }
            if (gr.shadows != 0.0f) {
                float wS = (1.0f - luma) * (1.0f - luma) * gr.shadows * 0.5f * m;
                r += wS; g += wS; b += wS;
            }
            r = clamp0(r); g = clamp0(g); b = clamp0(b);
        }
    }

    // 10c: vignette
    if (p.flags & FLAG_VIGNETTE) {
        float dx = ((float)gid.x - p.vigCx) * p.vigInvMax;
        float dy = ((float)gid.y - p.vigCy) * p.vigInvMax;
        float rad = sqrt(dx * dx + dy * dy);
        float m = clamp((rad - 0.35f) / 0.65f, 0.0f, 1.0f);
        m = m * m * (3.0f - 2.0f * m);
        if (m > 0.0f) {
            float gn = max(0.0f, 1.0f + p.vigAmount * m);
            r *= gn; g *= gn; b *= gn;
        }
    }

    img[idx] = float4(r, g, b, px.a);
}

// ---- 5 / 6: box blur, two passes then a combine ----------------------------

kernel void blurH(const device float4  *src [[buffer(0)]],
                  device float4        *dst [[buffer(1)]],
                  constant BlurParams   &p  [[buffer(2)]],
                  uint2                 gid [[thread_position_in_grid]])
{
    if (gid.x >= p.width || gid.y >= p.height) return;
    int W = (int)p.width;
    int row = (int)gid.y * W;
    float3 acc = float3(0.0f);
    for (int k = -p.radius; k <= p.radius; k++) {
        int xx = clamp((int)gid.x + k, 0, W - 1);
        acc += src[row + xx].rgb;
    }
    float norm = 1.0f / (float)(p.radius * 2 + 1);
    int o = row + (int)gid.x;
    dst[o] = float4(acc * norm, src[o].a);
}

kernel void blurV(const device float4  *src [[buffer(0)]],
                  device float4        *dst [[buffer(1)]],
                  constant BlurParams   &p  [[buffer(2)]],
                  uint2                 gid [[thread_position_in_grid]])
{
    if (gid.x >= p.width || gid.y >= p.height) return;
    int W = (int)p.width, H = (int)p.height;
    float3 acc = float3(0.0f);
    for (int k = -p.radius; k <= p.radius; k++) {
        int yy = clamp((int)gid.y + k, 0, H - 1);
        acc += src[yy * W + (int)gid.x].rgb;
    }
    float norm = 1.0f / (float)(p.radius * 2 + 1);
    int o = (int)gid.y * W + (int)gid.x;
    dst[o] = float4(acc * norm, src[o].a);
}

/// Combines the blurred copy back into the image: blend for denoise/soften, unsharp for
/// sharpening. Matches ImageProcessor.applyBlurOp.
kernel void blurCombine(device float4        *img     [[buffer(0)]],
                        const device float4  *blurred [[buffer(1)]],
                        constant BlurParams   &p      [[buffer(2)]],
                        uint2                 gid     [[thread_position_in_grid]])
{
    if (gid.x >= p.width || gid.y >= p.height) return;
    uint idx = gid.y * p.width + gid.x;
    float4 a = img[idx];
    float3 bl = blurred[idx].rgb;
    float3 outRgb;
    if (p.mode == 1) {
        outRgb = float3(clamp0(a.r + p.amount * (a.r - bl.r)),
                        clamp0(a.g + p.amount * (a.g - bl.g)),
                        clamp0(a.b + p.amount * (a.b - bl.b)));
    } else {
        float amt = clamp(p.amount, 0.0f, 1.0f);
        outRgb = a.rgb + (bl - a.rgb) * amt;
    }
    img[idx] = float4(outRgb, a.a);
}

// ---- 9 / 10: geometry ------------------------------------------------------

static inline float4 sampleBilinear(const device float4 *src, int W, int H,
                                    float fx, float fy)
{
    fx = clamp(fx, 0.0f, (float)(W - 1));
    fy = clamp(fy, 0.0f, (float)(H - 1));
    int x0 = (int)fx, y0 = (int)fy;
    int x1 = min(x0 + 1, W - 1), y1 = min(y0 + 1, H - 1);
    float tx = fx - (float)x0, ty = fy - (float)y0;
    float4 v00 = src[y0 * W + x0], v10 = src[y0 * W + x1];
    float4 v01 = src[y1 * W + x0], v11 = src[y1 * W + x1];
    float4 top = v00 + (v10 - v00) * tx;
    float4 bot = v01 + (v11 - v01) * tx;
    return top + (bot - top) * ty;
}

kernel void resample(const device float4      *src [[buffer(0)]],
                     device float4            *dst [[buffer(1)]],
                     constant ResampleParams   &p  [[buffer(2)]],
                     uint2                     gid [[thread_position_in_grid]])
{
    if (gid.x >= p.outWidth || gid.y >= p.outHeight) return;
    int W = (int)p.srcWidth, H = (int)p.srcHeight;
    float sx, sy;
    if (p.mode == 0) {
        float nx = ((float)gid.x / (float)W - 0.5f) * 2.0f;
        float ny = ((float)gid.y / (float)H - 0.5f) * 2.0f;
        float r2 = nx * nx + ny * ny;
        float f = 1.0f + p.k * r2;
        sx = (nx * f / 2.0f + 0.5f) * (float)W;
        sy = (ny * f / 2.0f + 0.5f) * (float)H;
    } else {
        float rx = (float)gid.x - p.ox;
        float ry = (float)gid.y - p.oy;
        sx = p.cx + (rx * p.cosA - ry * p.sinA);
        sy = p.cy + (rx * p.sinA + ry * p.cosA);
    }
    dst[gid.y * p.outWidth + gid.x] = sampleBilinear(src, W, H, sx, sy);
}

kernel void rotate90(const device float4    *src [[buffer(0)]],
                     device float4          *dst [[buffer(1)]],
                     constant RotateParams   &p  [[buffer(2)]],
                     uint2                   gid [[thread_position_in_grid]])
{
    if (gid.x >= p.width || gid.y >= p.height) return;
    int W = (int)p.width, H = (int)p.height;
    int x = (int)gid.x, y = (int)gid.y;
    int nx, ny, DW;
    if (p.rot == 90)      { nx = H - 1 - y; ny = x;         DW = H; }
    else if (p.rot == 180){ nx = W - 1 - x; ny = H - 1 - y; DW = W; }
    else                  { nx = y;         ny = W - 1 - x; DW = H; }
    dst[ny * DW + nx] = src[y * W + x];
}
"""
}
