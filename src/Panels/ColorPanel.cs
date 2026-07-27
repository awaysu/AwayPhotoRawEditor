using System;
using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.Controls;

namespace AwayPhotoRawEditor.Panels;

/// <summary>色彩 (310x210): 白平衡控制列 (滴管 / 拍攝時設定) + 色溫 / 色調 / 鮮豔度 / 飽和度.</summary>
public sealed class ColorPanel : AdjustPanelBase
{
    private readonly IconButton _picker;
    private readonly AdjustmentSlider _temp;
    private readonly Label _wbLabel;
    private readonly FlatButton _asShot;

    // Temperature is always stored as Kelvin (5200 = neutral). RAW files show the Kelvin
    // value directly; non-RAW files (jpg/bmp/…) show a 0-centered warm/cool scale that maps
    // onto Kelvin, since a baked-in file has no meaningful colour temperature.
    private bool _tempIsRaw = true;
    public const double NonRawScale = 30.0;    // slider ±100 → 5200 ± 3000 K

    /// <summary>Clamp a Kelvin value to what the non-RAW ±100 scale can represent, so the
    /// stored temperature never drifts outside the slider display (WB picker / as-shot).</summary>
    public static double ClampToNonRawRange(double kelvin) =>
        Math.Clamp(kelvin, 5200 - 100 * NonRawScale, 5200 + 100 * NonRawScale);

    public event Action<bool>? WbPickerToggled;
    public event Action? UseAsShotWb;

    public ColorPanel() : base("色彩")
    {
        Size = new Size(310, 210);

        _wbLabel = UiFactory.Label("白平衡選擇器", Theme.TextDim);
        _wbLabel.SetBounds(12, 4, 104, 26);

        _picker = new IconButton { Glyph = "🖉", Checkable = true };
        _picker.SetBounds(118, 2, 30, 30);
        _picker.CheckedChanged += (_, _) => WbPickerToggled?.Invoke(_picker.Checked);

        _asShot = new FlatButton { Text = "拍攝時設定" };
        _asShot.SetBounds(310 - 12 - 100, 2, 100, 30);
        _asShot.Click += (_, _) => UseAsShotWb?.Invoke();

        ContentArea.Controls.AddRange(new Control[] { _wbLabel, _picker, _asShot });

        const int x = 12, w = 286, h = 34, gap = 35, y = 40;
        _temp = AddSliderAt(x, y + 0 * gap, w, h, "色溫", 2000, 12000, 5200, "0", false,
            a => TempToSlider(a.Temperature), (a, v) => a.Temperature = SliderToTemp(v), 50);
        AddSliderAt(x, y + 1 * gap, w, h, "色調", -100, 100, 0, "0", true, a => a.Tint, (a, v) => a.Tint = v, 1);
        AddSliderAt(x, y + 2 * gap, w, h, "鮮豔度", -100, 100, 0, "0", true, a => a.Vibrance, (a, v) => a.Vibrance = v, 1);
        AddSliderAt(x, y + 3 * gap, w, h, "飽和度", -100, 100, 0, "0", true, a => a.Saturation, (a, v) => a.Saturation = v, 1);
    }

    private double TempToSlider(double kelvin) => _tempIsRaw ? kelvin : (kelvin - 5200) / NonRawScale;
    private double SliderToTemp(double sliderVal) => _tempIsRaw ? sliderVal : 5200 + sliderVal * NonRawScale;

    /// <summary>Switch the 色溫 slider between RAW (Kelvin) and non-RAW (0-centered) scales.
    /// Call before <see cref="AdjustPanelBase.Bind"/> so the value reloads in the right scale.</summary>
    public void SetTemperatureMode(bool isRaw)
    {
        _tempIsRaw = isRaw;
        if (isRaw)
        {
            _temp.Min = 2000; _temp.Max = 12000; _temp.DefaultValue = 5200;
            _temp.Bipolar = false; _temp.WheelStep = 50;
        }
        else
        {
            _temp.Min = -100; _temp.Max = 100; _temp.DefaultValue = 0;
            _temp.Bipolar = true; _temp.WheelStep = 1;
        }
        _temp.Invalidate();
    }

    public void SetPickerChecked(bool value) => _picker.Checked = value;

    /// <summary>編輯風格檔視窗用：風格檔沒有目標照片，隱藏 白平衡選擇器/拍攝時設定 列。</summary>
    public void HideWhiteBalanceRow() => _wbLabel.Visible = _picker.Visible = _asShot.Visible = false;
}
