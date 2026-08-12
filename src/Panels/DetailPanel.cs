using AwayPhotoRawEditor.Controls;

namespace AwayPhotoRawEditor.Panels;

/// <summary>細節 (310x145): 銳利度 (正=銳化, 負=柔化) / 暗角 (正=壓暗四角) / 降噪.</summary>
public sealed class DetailPanel : AdjustPanelBase
{
    public DetailPanel() : base("細節")
    {
        Size = Ui.Sz(310, 145);
        AddSliderAt(12, 4, 286, 34, "銳利度", -100, 100, 0, "0", true, a => a.Sharpening, (a, v) => a.Sharpening = v, 1);
        AddSliderAt(12, 39, 286, 34, "暗角", -100, 100, 0, "0", true, a => a.Vignette, (a, v) => a.Vignette = v, 1);
        AddSliderAt(12, 74, 286, 34, "降噪", 0, 100, 0, "0", false, a => a.NoiseReduction, (a, v) => a.NoiseReduction = v, 1);
    }
}
