using System.Drawing;

namespace AwayPhotoRawEditor.Panels;

/// <summary>基本調整 (310x245): 曝光 / 對比 / 亮部 / 暗部 / 白色 / 黑色.</summary>
public sealed class BasicAdjustPanel : AdjustPanelBase
{
    public BasicAdjustPanel() : base("基本調整")
    {
        Size = new Size(310, 245);
        const int x = 12, w = 286, h = 34, gap = 35, y = 4;
        AddSliderAt(x, y + 0 * gap, w, h, "曝光", -5, 5, 0, "0.00", true, a => a.Exposure, (a, v) => a.Exposure = v, 0.05);
        AddSliderAt(x, y + 1 * gap, w, h, "對比", -100, 100, 0, "0", true, a => a.Contrast, (a, v) => a.Contrast = v, 1);
        AddSliderAt(x, y + 2 * gap, w, h, "亮部", -100, 100, 0, "0", true, a => a.Highlights, (a, v) => a.Highlights = v, 1);
        AddSliderAt(x, y + 3 * gap, w, h, "暗部", -100, 100, 0, "0", true, a => a.Shadows, (a, v) => a.Shadows = v, 1);
        AddSliderAt(x, y + 4 * gap, w, h, "白色", -100, 100, 0, "0", true, a => a.Whites, (a, v) => a.Whites = v, 1);
        AddSliderAt(x, y + 5 * gap, w, h, "黑色", -100, 100, 0, "0", true, a => a.Blacks, (a, v) => a.Blacks = v, 1);
    }
}
