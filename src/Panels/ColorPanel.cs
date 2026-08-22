using System;
using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Controls;

namespace AwayPhotoRawEditor.Panels;

/// <summary>色彩 (310x210): 白平衡控制列 (滴管 / 拍攝時設定) + 色溫 / 色調 / 鮮豔度 / 飽和度.</summary>
public sealed class ColorPanel : AdjustPanelBase
{
    private readonly IconButton _picker;
    private readonly AdjustmentSlider _temp;
    private AdjustmentSlider _tint = null!, _vib = null!, _sat = null!;
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
        Size = Ui.Sz(310, 210);

        _wbLabel = UiFactory.Label("白平衡選擇器", Theme.TextDim);
        Ui.Place(_wbLabel, 12, 4, 104, 26);

        _picker = new IconButton { Glyph = "🖉", Checkable = true };
        Ui.Place(_picker, 118, 2, 30, 30);
        _picker.CheckedChanged += (_, _) => WbPickerToggled?.Invoke(_picker.Checked);

        _asShot = new FlatButton { Text = "拍攝時設定" };
        Ui.Place(_asShot, 310 - 12 - 100, 2, 100, 30);
        _asShot.Click += (_, _) => UseAsShotWb?.Invoke();

        ContentArea.Controls.AddRange(new Control[] { _wbLabel, _picker, _asShot });

        // 這四條的左右各代表什麼，用軌道漸層直接畫出來（見 AdjustmentSlider.Gradient）。
        const int x = 12, w = 286, h = 34, gap = 35, y = 40;
        _temp = AddSliderAt(x, y + 0 * gap, w, h, "色溫", 2000, 12000, 5200, "0", false,
            a => TempToSlider(a.Temperature), (a, v) => a.Temperature = SliderToTemp(v), 50);
        _temp.Gradient = SliderGradient.Temperature;
        _tint = AddSliderAt(x, y + 1 * gap, w, h, "色調", -100, 100, 0, "0", true, a => a.Tint, (a, v) => a.Tint = v, 1);
        _tint.Gradient = SliderGradient.Tint;
        _vib = AddSliderAt(x, y + 2 * gap, w, h, "鮮豔度", -100, 100, 0, "0", true, a => a.Vibrance, (a, v) => a.Vibrance = v, 1);
        _vib.Gradient = SliderGradient.Saturation;
        _sat = AddSliderAt(x, y + 3 * gap, w, h, "飽和度", -100, 100, 0, "0", true, a => a.Saturation, (a, v) => a.Saturation = v, 1);
        _sat.Gradient = SliderGradient.Saturation;
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

    /// <summary>白平衡列改成量文字寬度再排：標籤文字會隨語言（L.Apply）與使用者調整的字級變動，
    /// 寫死 104px 在大字級或長翻譯下會把「白平衡選擇器」的最後一個字切掉。
    /// 在 OnHandleCreated 做，此時翻譯與字型都已確定。</summary>
    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        LayoutWhiteBalanceRow();
    }

    private void LayoutWhiteBalanceRow()
    {
        if (!_wbLabel.Visible) return;
        int textW = TextRenderer.MeasureText(_wbLabel.Text, _wbLabel.Font).Width + Ui.S(4);
        _wbLabel.Width = Math.Max(Ui.S(60), textW);
        // 滴管接在標籤右側，但不能疊到右側的「拍攝時設定」
        int left = _wbLabel.Right + Ui.S(8);
        _picker.Left = Math.Min(left, _asShot.Left - _picker.Width - Ui.S(8));
    }

    public void SetPickerChecked(bool value) => _picker.Checked = value;

    /// <summary>編輯風格檔視窗用：風格檔沒有目標照片，隱藏 白平衡選擇器/拍攝時設定 列。</summary>
    public void HideWhiteBalanceRow() => _wbLabel.Visible = _picker.Visible = _asShot.Visible = false;

    /// <summary>編輯風格檔視窗用：滑桿照常顯示（風格檔仍可存色溫/色調），但套用時不會用到——
    /// 在藏起來的白平衡列位置放一行說明，免得使用者以為是 bug（2026-08-23 使用者指定這個做法）。</summary>
    public void ShowPresetWhiteBalanceNote()
    {
        var note = new Label
        {
            Text = L.T("套用風格檔時維持照片目前的色溫／色調"),
            ForeColor = Theme.TextFaint, Font = Theme.Small, BackColor = Theme.PanelBg,
            TextAlign = ContentAlignment.MiddleLeft
        };
        Ui.Place(note, 12, 2, 286, 34);   // 兩行高，長翻譯（德/法/西）可自動換行
        ContentArea.Controls.Add(note);
    }
}
