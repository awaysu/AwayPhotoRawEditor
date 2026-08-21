using AwayPhotoRawEditor.Controls;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Panels;

/// <summary>基本調整 (310x245): 曝光 / 對比 / 亮部 / 暗部 / 白色 / 黑色.</summary>
public sealed class BasicAdjustPanel : AdjustPanelBase
{
    private readonly AdjustmentSlider _exposure;

    public BasicAdjustPanel() : base("基本調整")
    {
        Size = Ui.Sz(310, 245);
        const int x = 12, w = 286, h = 34, gap = 35, y = 4;
        _exposure = AddSliderAt(x, y + 0 * gap, w, h, "曝光", -5, 5, 0, "0.00", true, a => a.Exposure, (a, v) => a.Exposure = v, 0.05);
        AddSliderAt(x, y + 1 * gap, w, h, "對比", -100, 100, 0, "0", true, a => a.Contrast, (a, v) => a.Contrast = v, 1);
        AddSliderAt(x, y + 2 * gap, w, h, "亮部", -100, 100, 0, "0", true, a => a.Highlights, (a, v) => a.Highlights = v, 1);
        AddSliderAt(x, y + 3 * gap, w, h, "暗部", -100, 100, 0, "0", true, a => a.Shadows, (a, v) => a.Shadows = v, 1);
        AddSliderAt(x, y + 4 * gap, w, h, "白色", -100, 100, 0, "0", true, a => a.Whites, (a, v) => a.Whites = v, 1);
        AddSliderAt(x, y + 5 * gap, w, h, "黑色", -100, 100, 0, "0", true, a => a.Blacks, (a, v) => a.Blacks = v, 1);
    }

    /// <summary>曝光範圍隨處理版本：舊版算式乘在 gamma 值上、±2 就已經是 ±4.4 格，範圍維持 ±2
    /// 才不會讓舊照片的滑桿位置跟以前對不上；新版是真正的 EV，用 ±5。
    /// 範圍要在 base.Bind 載入值<b>之前</b>設好，不然值會被舊範圍夾住。</summary>
    public override void Bind(ImageAdjustments adj)
    {
        double range = adj.IsLegacyPipeline ? 2 : 5;
        if (_exposure.Max != range) { _exposure.Min = -range; _exposure.Max = range; _exposure.Invalidate(); }
        base.Bind(adj);
    }
}
