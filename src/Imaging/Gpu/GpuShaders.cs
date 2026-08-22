using ComputeSharp;

namespace AwayPhotoRawEditor.Imaging.Gpu;

// ---------------------------------------------------------------------------------------------
// GPU 版的 ImageProcessor 各階段（ComputeSharp → HLSL compute shader，編譯期由 source generator
// 產生 DXIL，執行期不需要 dxcompiler）。每個 shader 都是 CPU 版逐行對照的移植：
// 同一個公式、同一張 LUT、同樣的夾值位置，只差浮點運算順序（結果差在最後幾個 bit，8-bit 輸出看不出）。
// 改 ImageProcessor 的算式時這裡要同步改，GpuParity（--gputest）會量兩邊的差。
// ---------------------------------------------------------------------------------------------

/// <summary>PixelStageShader 的工作選擇旗標（位元可組合；白平衡三種互斥）。</summary>
internal static class PixelFlags
{
    public const int LegacyWb = 1;        // 舊版：直接乘在編碼值上
    public const int LinearMul = 2;       // v1 無相機資料：線性域乘黑體增益
    public const int LinearMatrix = 4;    // v1 有相機資料：線性域 3×3 矩陣
    public const int ToneLut = 8;
    public const int VibSat = 16;
    public const int Gradients = 32;
    public const int GradientLinear = 64; // 漸層曝光走線性（v1）
    public const int Vignette = 128;
}

/// <summary>一個線性漸層的 GPU 參數（對應 <see cref="Models.LinearGradient"/>，三角函數先算好）。</summary>
internal struct GradientGpu
{
    public float SinA, CosA, CenterX, CenterY, Inv2Range;
    public float Exposure, Contrast, Highlights, Shadows, Saturation;
}

/// <summary>
/// 逐像素階段（步驟 1+2 白平衡/曝光、3 色調 LUT、4 鮮豔度/飽和度、7 漸層、10c 暗角）融合成一個 kernel，
/// 對「影像的一段列」就地運算：<c>pixels</c> 只裝 <c>rowOffset</c> 起的若干列，需要全圖座標的部分
/// （漸層、暗角）用 <c>width/height/rowOffset</c> 還原。
/// </summary>
[ThreadGroupSize(DefaultThreadGroupSizes.XY)]
[GeneratedComputeShaderDescriptor]
internal readonly partial struct PixelStageShader : IComputeShader
{
    private readonly ReadWriteBuffer<float4> pixels;
    private readonly ReadOnlyBuffer<float> toneLut;
    private readonly ReadOnlyBuffer<float> decodeLut;
    private readonly ReadOnlyBuffer<float> encodeLut;
    private readonly ReadOnlyBuffer<GradientGpu> gradients;
    private readonly int width;
    private readonly int height;
    private readonly int rowOffset;
    private readonly int flags;
    private readonly int gradientCount;
    private readonly int toneLutSize;
    private readonly int decodeLutSize;
    private readonly int encodeLutSize;
    private readonly float3 wbMul;
    private readonly float3 m0;
    private readonly float3 m1;
    private readonly float3 m2;
    private readonly float sat;
    private readonly float vib;
    private readonly float vigAmount;
    private readonly float vigCx;
    private readonly float vigCy;
    private readonly float vigInvMax;

    public PixelStageShader(
        ReadWriteBuffer<float4> pixels, ReadOnlyBuffer<float> toneLut,
        ReadOnlyBuffer<float> decodeLut, ReadOnlyBuffer<float> encodeLut,
        ReadOnlyBuffer<GradientGpu> gradients,
        int width, int height, int rowOffset, int flags, int gradientCount,
        int toneLutSize, int decodeLutSize, int encodeLutSize,
        float3 wbMul, float3 m0, float3 m1, float3 m2,
        float sat, float vib,
        float vigAmount, float vigCx, float vigCy, float vigInvMax)
    {
        this.pixels = pixels; this.toneLut = toneLut;
        this.decodeLut = decodeLut; this.encodeLut = encodeLut; this.gradients = gradients;
        this.width = width; this.height = height; this.rowOffset = rowOffset;
        this.flags = flags; this.gradientCount = gradientCount;
        this.toneLutSize = toneLutSize; this.decodeLutSize = decodeLutSize; this.encodeLutSize = encodeLutSize;
        this.wbMul = wbMul; this.m0 = m0; this.m1 = m1; this.m2 = m2;
        this.sat = sat; this.vib = vib;
        this.vigAmount = vigAmount; this.vigCx = vigCx; this.vigCy = vigCy; this.vigInvMax = vigInvMax;
    }

    // ---- ColorScience.Linearize / Encode（同一張查表，n 格 + 1 個端點）----
    private float Linearize(float v)
    {
        if (!(v > 0f)) return 0f;
        if (v >= 1f) return decodeLut[decodeLutSize];
        float f = v * decodeLutSize;
        int i = (int)f;
        float t = f - i;
        return decodeLut[i] + (decodeLut[i + 1] - decodeLut[i]) * t;
    }

    private float Encode(float l)
    {
        if (!(l > 0f)) return 0f;
        if (l >= 1f) return encodeLut[encodeLutSize];
        float f = l * encodeLutSize;
        int i = (int)f;
        float t = f - i;
        return encodeLut[i] + (encodeLut[i + 1] - encodeLut[i]) * t;
    }

    // ---- ToneCurve.Sample ----
    private float Tone(float x)
    {
        if (x <= 0f) return toneLut[0];
        if (x >= 1f) return toneLut[toneLutSize - 1];
        float f = x * (toneLutSize - 1);
        int i = (int)f;
        float frac = f - i;
        return toneLut[i] + (toneLut[i + 1] - toneLut[i]) * frac;
    }

    private static float Clamp0(float v)
    {
        return v < 0f ? 0f : v;
    }

    public void Execute()
    {
        int x = ThreadIds.X;
        int yLocal = ThreadIds.Y;
        int y = rowOffset + yLocal;
        int idx = yLocal * width + x;
        float4 p = pixels[idx];
        float r = p.X, g = p.Y, b = p.Z;

        // 1 + 2
        if ((flags & PixelFlags.LegacyWb) != 0)
        {
            r *= wbMul.X; g *= wbMul.Y; b *= wbMul.Z;
        }
        else if ((flags & PixelFlags.LinearMatrix) != 0)
        {
            float3 lin = new float3(Linearize(r), Linearize(g), Linearize(b));
            r = Encode(Hlsl.Dot(m0, lin));
            g = Encode(Hlsl.Dot(m1, lin));
            b = Encode(Hlsl.Dot(m2, lin));
        }
        else if ((flags & PixelFlags.LinearMul) != 0)
        {
            r = Encode(Linearize(r) * wbMul.X);
            g = Encode(Linearize(g) * wbMul.Y);
            b = Encode(Linearize(b) * wbMul.Z);
        }

        // 3
        if ((flags & PixelFlags.ToneLut) != 0)
        {
            r = Tone(r); g = Tone(g); b = Tone(b);
        }

        // 4
        if ((flags & PixelFlags.VibSat) != 0)
        {
            float luma = 0.299f * r + 0.587f * g + 0.114f * b;
            float max = Hlsl.Max(r, Hlsl.Max(g, b));
            float min = Hlsl.Min(r, Hlsl.Min(g, b));
            float curSat = max <= 1e-4f ? 0f : (max - min) / max;
            float f = (1f + vib * (1f - curSat)) * (1f + sat);
            r = Clamp0(luma + (r - luma) * f);
            g = Clamp0(luma + (g - luma) * f);
            b = Clamp0(luma + (b - luma) * f);
        }

        // 7 — 每個漸層依序疊在前一個結果上（與 CPU 的 foreach 相同）
        if ((flags & PixelFlags.Gradients) != 0)
        {
            float nx = (float)x / width;
            float ny = (float)y / height;
            bool linearExp = (flags & PixelFlags.GradientLinear) != 0;
            for (int k = 0; k < gradientCount; k++)
            {
                GradientGpu gr = gradients[k];
                float d = (nx - gr.CenterX) * gr.SinA + (ny - gr.CenterY) * gr.CosA;
                float m = Hlsl.Clamp(d * gr.Inv2Range + 0.5f, 0f, 1f);
                m = m * m * (3f - 2f * m);
                if (m <= 0f) continue;

                float rr = r, gg = g, bb = b;
                if (gr.Exposure != 0f)
                {
                    float expMul = Hlsl.Exp2(gr.Exposure * m);
                    if (linearExp)
                    {
                        rr = Encode(Linearize(rr) * expMul);
                        gg = Encode(Linearize(gg) * expMul);
                        bb = Encode(Linearize(bb) * expMul);
                    }
                    else { rr *= expMul; gg *= expMul; bb *= expMul; }
                }

                float luma = 0.299f * rr + 0.587f * gg + 0.114f * bb;
                if (gr.Saturation != 0f)
                {
                    float f = 1f + gr.Saturation * m;
                    rr = luma + (rr - luma) * f; gg = luma + (gg - luma) * f; bb = luma + (bb - luma) * f;
                }
                if (gr.Contrast != 0f)
                {
                    float c = gr.Contrast * m;
                    rr = 0.5f + (rr - 0.5f) * (1f + c); gg = 0.5f + (gg - 0.5f) * (1f + c); bb = 0.5f + (bb - 0.5f) * (1f + c);
                }
                if (gr.Highlights != 0f)
                {
                    float wH = luma * luma * gr.Highlights * 0.5f * m;
                    rr += wH; gg += wH; bb += wH;
                }
                if (gr.Shadows != 0f)
                {
                    float wS = (1f - luma) * (1f - luma) * gr.Shadows * 0.5f * m;
                    rr += wS; gg += wS; bb += wS;
                }
                r = Clamp0(rr); g = Clamp0(gg); b = Clamp0(bb);
            }
        }

        // 10c
        if ((flags & PixelFlags.Vignette) != 0)
        {
            float dx = (x - vigCx) * vigInvMax;
            float dy = (y - vigCy) * vigInvMax;
            float rad = Hlsl.Sqrt(dx * dx + dy * dy);
            float m = Hlsl.Clamp((rad - 0.35f) / 0.65f, 0f, 1f);
            m = m * m * (3f - 2f * m);
            if (m > 0f)
            {
                float gain = Hlsl.Max(0f, 1f + vigAmount * m);
                r *= gain; g *= gain; b *= gain;
            }
        }

        pixels[idx] = new float4(r, g, b, p.W);
    }
}

/// <summary>方框模糊水平向（ImageProcessor.BoxBlur 前半），整個上傳區域每列都算。</summary>
[ThreadGroupSize(DefaultThreadGroupSizes.XY)]
[GeneratedComputeShaderDescriptor]
internal readonly partial struct BlurHShader : IComputeShader
{
    private readonly ReadWriteBuffer<float4> src;
    private readonly ReadWriteBuffer<float4> dst;
    private readonly int width;
    private readonly int radius;
    private readonly float norm;

    public BlurHShader(ReadWriteBuffer<float4> src, ReadWriteBuffer<float4> dst, int width, int radius, float norm)
    {
        this.src = src; this.dst = dst; this.width = width; this.radius = radius; this.norm = norm;
    }

    public void Execute()
    {
        int x = ThreadIds.X;
        int y = ThreadIds.Y;
        int row = y * width;
        float r = 0f, g = 0f, b = 0f;
        for (int k = -radius; k <= radius; k++)
        {
            int xx = Hlsl.Clamp(x + k, 0, width - 1);
            float4 s = src[row + xx];
            r += s.X; g += s.Y; b += s.Z;
        }
        dst[row + x] = new float4(r * norm, g * norm, b * norm, src[row + x].W);
    }
}

/// <summary>
/// 方框模糊垂直向 + 收尾（模糊結果不落地，直接算出該階段輸出）：
/// mode 0 = Blend（降噪／柔化：orig + (blur − orig) × amount）、
/// mode 1 = Unsharp（銳利化：clamp0(orig + amount × (orig − blur))）。
/// 只算輸出需要的列：<c>rowStart</c> 是輸出第一列在區域內的索引。
/// </summary>
[ThreadGroupSize(DefaultThreadGroupSizes.XY)]
[GeneratedComputeShaderDescriptor]
internal readonly partial struct BlurVFinishShader : IComputeShader
{
    private readonly ReadWriteBuffer<float4> tmp;
    private readonly ReadWriteBuffer<float4> orig;
    private readonly ReadWriteBuffer<float4> dst;
    private readonly int width;
    private readonly int regionRows;
    private readonly int rowStart;
    private readonly int radius;
    private readonly float norm;
    private readonly int mode;
    private readonly float amount;

    public BlurVFinishShader(ReadWriteBuffer<float4> tmp, ReadWriteBuffer<float4> orig, ReadWriteBuffer<float4> dst,
        int width, int regionRows, int rowStart, int radius, float norm, int mode, float amount)
    {
        this.tmp = tmp; this.orig = orig; this.dst = dst; this.width = width; this.regionRows = regionRows;
        this.rowStart = rowStart; this.radius = radius; this.norm = norm; this.mode = mode; this.amount = amount;
    }

    public void Execute()
    {
        int x = ThreadIds.X;
        int yo = ThreadIds.Y;
        int y = rowStart + yo;
        float r = 0f, g = 0f, b = 0f;
        for (int k = -radius; k <= radius; k++)
        {
            int yy = Hlsl.Clamp(y + k, 0, regionRows - 1);
            float4 s = tmp[yy * width + x];
            r += s.X; g += s.Y; b += s.Z;
        }
        r *= norm; g *= norm; b *= norm;
        float4 o = orig[y * width + x];
        float4 outPx;
        if (mode == 0)
        {
            outPx = new float4(o.X + (r - o.X) * amount, o.Y + (g - o.Y) * amount, o.Z + (b - o.Z) * amount, o.W);
        }
        else
        {
            float rr = o.X + amount * (o.X - r);
            float gg = o.Y + amount * (o.Y - g);
            float bb = o.Z + amount * (o.Z - b);
            outPx = new float4(rr < 0f ? 0f : rr, gg < 0f ? 0f : gg, bb < 0f ? 0f : bb, o.W);
        }
        dst[yo * width + x] = outPx;
    }
}

/// <summary>
/// 幾何重取樣（步驟 9 廣角變形、10 裁切／拉直預覽）：反向映射 + 雙線性，邊緣夾值與
/// ImageProcessor.Sample 相同。mode 0 = 徑向變形(k)；mode 1 = 繞 (cx,cy) 旋轉，輸出像素先減去 (ox,oy)。
/// 輸出可分段：<c>rowOffset</c> 是這一段的第一個輸出列。
/// </summary>
[ThreadGroupSize(DefaultThreadGroupSizes.XY)]
[GeneratedComputeShaderDescriptor]
internal readonly partial struct ResampleShader : IComputeShader
{
    private readonly ReadWriteBuffer<float4> src;
    private readonly ReadWriteBuffer<float4> dst;
    private readonly int srcW;
    private readonly int srcH;
    private readonly int dstW;
    private readonly int rowOffset;
    private readonly int mode;
    private readonly float k;
    private readonly float cx;
    private readonly float cy;
    private readonly float sinA;
    private readonly float cosA;
    private readonly float ox;
    private readonly float oy;

    public ResampleShader(ReadWriteBuffer<float4> src, ReadWriteBuffer<float4> dst, int srcW, int srcH, int dstW,
        int rowOffset, int mode, float k, float cx, float cy, float sinA, float cosA, float ox, float oy)
    {
        this.src = src; this.dst = dst; this.srcW = srcW; this.srcH = srcH; this.dstW = dstW;
        this.rowOffset = rowOffset; this.mode = mode; this.k = k; this.cx = cx; this.cy = cy;
        this.sinA = sinA; this.cosA = cosA; this.ox = ox; this.oy = oy;
    }

    public void Execute()
    {
        int x = ThreadIds.X;
        int yo = ThreadIds.Y;
        int y = rowOffset + yo;

        float sx, sy;
        if (mode == 0)
        {
            float nx = ((float)x / srcW - 0.5f) * 2f;
            float ny = ((float)y / srcH - 0.5f) * 2f;
            float r2 = nx * nx + ny * ny;
            float f = 1f + k * r2;
            sx = (nx * f * 0.5f + 0.5f) * srcW;
            sy = (ny * f * 0.5f + 0.5f) * srcH;
        }
        else
        {
            float rx = x - ox;
            float ry = y - oy;
            sx = cx + (rx * cosA - ry * sinA);
            sy = cy + (rx * sinA + ry * cosA);
        }

        // ImageProcessor.Sample：夾到 [0, W-1]，取整數格與鄰格做雙線性
        if (sx < 0f) sx = 0f; else if (sx > srcW - 1) sx = srcW - 1;
        if (sy < 0f) sy = 0f; else if (sy > srcH - 1) sy = srcH - 1;
        int x0 = (int)sx, y0 = (int)sy;
        int x1 = Hlsl.Min(x0 + 1, srcW - 1), y1 = Hlsl.Min(y0 + 1, srcH - 1);
        float tx = sx - x0, ty = sy - y0;
        float4 v00 = src[y0 * srcW + x0];
        float4 v10 = src[y0 * srcW + x1];
        float4 v01 = src[y1 * srcW + x0];
        float4 v11 = src[y1 * srcW + x1];
        float4 top = v00 + (v10 - v00) * tx;
        float4 bot = v01 + (v11 - v01) * tx;
        dst[yo * dstW + x] = top + (bot - top) * ty;
    }
}

/// <summary>90° 整數旋轉（ImageProcessor.RotateDiscrete）：純粹的像素搬移。rot 1 = R90、2 = R180、3 = R270。
/// 以來源座標 dispatch，每個 thread 把自己的像素搬到目的位置。</summary>
[ThreadGroupSize(DefaultThreadGroupSizes.XY)]
[GeneratedComputeShaderDescriptor]
internal readonly partial struct RotateShader : IComputeShader
{
    private readonly ReadWriteBuffer<float4> src;
    private readonly ReadWriteBuffer<float4> dst;
    private readonly int srcW;
    private readonly int srcH;
    private readonly int dstW;
    private readonly int rot;

    public RotateShader(ReadWriteBuffer<float4> src, ReadWriteBuffer<float4> dst, int srcW, int srcH, int dstW, int rot)
    {
        this.src = src; this.dst = dst; this.srcW = srcW; this.srcH = srcH; this.dstW = dstW; this.rot = rot;
    }

    public void Execute()
    {
        int x = ThreadIds.X;
        int y = ThreadIds.Y;
        int nx, ny;
        if (rot == 1) { nx = srcH - 1 - y; ny = x; }
        else if (rot == 2) { nx = srcW - 1 - x; ny = srcH - 1 - y; }
        else { nx = y; ny = srcW - 1 - x; }
        dst[ny * dstW + nx] = src[y * srcW + x];
    }
}
