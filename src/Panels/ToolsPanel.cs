using System;
using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.Controls;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Panels;

/// <summary>工具 (290x355): a tab strip (TopTab: 裁切/漸層/修護) over a ribbon area whose
/// three content panels overlap — only the active tab's panel is visible. The tab strip
/// allows deselection (no tab = no tool, ribbon locked). 標誌 lives in the export dialog now.</summary>
public sealed class ToolsPanel : AdjustPanelBase
{
    private readonly Panel _crop = new(), _grad = new(), _heal = new();
    private TopTab _tab = null!;
    private Panel _ribbonHost = null!;
    private int _active = -1;   // -1 = no tool selected (ribbon locked)

    // crop
    private ComboBox _aspect = null!;
    private NumericUpDown _cw = null!, _ch = null!;
    // heal
    private FlatButton _cloneBtn = null!, _inpaintBtn = null!;

    public event Action<ToolMode>? ToolChanged;
    public event Action? RotateLeft;
    public event Action? RotateRight;
    public event Action? ResetCrop;
    public event Action<string>? CropAspectChanged;
    public event Action<HealMode>? HealModeChanged;
    public event Action<double>? HealSizeChanged;
    public event Action? ClearHeal;

    public ToolsPanel() : base("工具")
    {
        Size = Ui.Sz(290, 355);

        // ---- tool tabs (裁切/漸層/修護), deselectable ----
        _tab = new TopTab { Tabs = new[] { "裁切", "漸層", "修護" }, AllowDeselect = true };
        Ui.Place(_tab, 12, 6, 266, 32);
        _tab.SelectedIndexChanged += (_, _) => ApplyTab(_tab.SelectedIndex);
        ContentArea.Controls.Add(_tab);

        // ---- ribbon host (266x260) ----
        _ribbonHost = new Panel { BackColor = Theme.PanelBg };
        Ui.Place(_ribbonHost, 12, 50, 266, 260);
        foreach (var r in new[] { _crop, _grad, _heal })
        {
            Ui.Place(r, 0, 0, 266, 260);
            r.BackColor = Theme.PanelBg;
            r.Visible = false;
            _ribbonHost.Controls.Add(r);
        }
        ContentArea.Controls.Add(_ribbonHost);

        BuildCrop();
        BuildGradient();
        BuildHeal();

        _crop.Visible = true;      // placeholder ribbon while nothing is selected
        _tab.SelectedIndex = -1;   // start with no tool selected -> ribbon locked
    }

    private void ApplyTab(int index)
    {
        _active = index;
        if (index >= 0)
        {
            _crop.Visible = index == 0;
            _grad.Visible = index == 1;
            _heal.Visible = index == 2;
        }
        _ribbonHost.Enabled = index >= 0;   // lock the parameter controls until a tool is picked
        ToolChanged?.Invoke(index switch
        { 0 => ToolMode.Crop, 1 => ToolMode.Gradient, 2 => ToolMode.Heal, _ => ToolMode.None });
    }

    public void SelectTool(ToolMode mode) =>
        _tab.SelectedIndex = mode switch { ToolMode.Crop => 0, ToolMode.Gradient => 1, ToolMode.Heal => 2, _ => -1 };

    // ---- crop ------------------------------------------------------------

    private void BuildCrop()
    {
        var lblRatio = UiFactory.Label("比例", Theme.TextDim); Ui.Place(lblRatio, 0, 2, 42, 26);
        _aspect = UiFactory.Combo("原始", "3:2", "4:3", "16:9", "1:1", "自訂"); Ui.Place(_aspect, 46, 0, 92, 28);
        _cw = UiFactory.Numeric(1, 99, 3); Ui.Place(_cw, 146, 0, 48, 28);
        var colon = UiFactory.Label(":", Theme.Text); Ui.Place(colon, 198, 2, 10, 26);
        _ch = UiFactory.Numeric(1, 99, 2); Ui.Place(_ch, 212, 0, 48, 28);
        _cw.Enabled = _ch.Enabled = false;

        _aspect.SelectedIndex = 0;
        _aspect.SelectedIndexChanged += (_, _) =>
        {
            string val = _aspect.SelectedIndex switch
            { 1 => "3:2", 2 => "4:3", 3 => "16:9", 4 => "1:1", 5 => "Custom", _ => "Original" };
            _cw.Enabled = _ch.Enabled = val == "Custom";
            if (Adj != null) { RaiseEditBegin(); Adj.CropAspectRatio = val == "Custom" ? $"{_cw.Value}:{_ch.Value}" : val; CropAspectChanged?.Invoke(Adj.CropAspectRatio); RaiseChanged(); }
        };
        RegisterLoader(a => _aspect.SelectedIndex = a.CropAspectRatio switch
        { "3:2" => 1, "4:3" => 2, "16:9" => 3, "1:1" => 4, "Original" => 0, _ => a.CropAspectRatio.Contains(':') ? 5 : 0 });
        // Restore the custom W:H numerics from a saved "W:H" ratio (loader above only sets the combo).
        RegisterLoader(a =>
        {
            if (a.CropAspectRatio is "3:2" or "4:3" or "16:9" or "1:1" or "Original") return;
            var parts = a.CropAspectRatio.Split(':');
            if (parts.Length == 2 && decimal.TryParse(parts[0], out var w) && decimal.TryParse(parts[1], out var h))
            { _cw.Value = Math.Clamp(w, 1, 99); _ch.Value = Math.Clamp(h, 1, 99); }
        });

        void CustomChanged(object? s, EventArgs e)
        {
            if (Adj != null && _aspect.SelectedIndex == 5)
            { RaiseEditBegin(); Adj.CropAspectRatio = $"{_cw.Value}:{_ch.Value}"; CropAspectChanged?.Invoke(Adj.CropAspectRatio); RaiseChanged(); }
        }
        _cw.ValueChanged += CustomChanged; _ch.ValueChanged += CustomChanged;

        // 角度：與廣角變形相同的滑桿（拖曳時即時重算畫面；預覽在裁切工具下也會旋轉）。
        // UI 值＝−CropAngle：使用者要求方向相反；只翻轉操作方向，儲存值意義不變（舊 XML 不受影響）。
        var angle = CreateSlider("角度", -45, 45, 0, "0.0", true, a => -a.CropAngle, (a, v) => a.CropAngle = -v, 0.5);
        Ui.Place(angle, 0, 38, 266, 36);

        var distortion = CreateSlider("廣角變形", -100, 100, 0, "0", true, a => a.Distortion, (a, v) => a.Distortion = v, 1);
        Ui.Place(distortion, 0, 76, 266, 36);

        var rotL = new FlatButton { Text = "照片左轉90度" }; Ui.Place(rotL, 0, 122, 126, 28);
        var rotR = new FlatButton { Text = "照片右轉90度" }; Ui.Place(rotR, 134, 122, 126, 28);
        rotL.Click += (_, _) => RotateLeft?.Invoke();
        rotR.Click += (_, _) => RotateRight?.Invoke();

        var reset = new FlatButton { Text = "裁切重設" }; Ui.Place(reset, 0, 158, 260, 28);
        reset.Click += (_, _) => ResetCrop?.Invoke();

        _crop.Controls.AddRange(new Control[] { lblRatio, _aspect, _cw, colon, _ch, angle, distortion, rotL, rotR, reset });
    }

    // ---- gradient --------------------------------------------------------

    private readonly System.Collections.Generic.List<AdjustmentSlider> _gradSliders = new();

    /// <summary>Raised when 新增線性漸層 is clicked; the caller adds a gradient and re-binds.</summary>
    public event Action? AddGradient;

    private void BuildGradient()
    {
        // Sliders edit the currently-selected gradient (a.ActiveGradient); disabled when none.
        AddGradSlider(0, "曝光", -2, 2, 0, "0.00", 0.05, g => g.Exposure, (g, v) => g.Exposure = v);
        AddGradSlider(36, "對比", -100, 100, 0, "0", 1, g => g.Contrast, (g, v) => g.Contrast = v);
        AddGradSlider(72, "亮部", -100, 100, 0, "0", 1, g => g.Highlights, (g, v) => g.Highlights = v);
        AddGradSlider(108, "暗部", -100, 100, 0, "0", 1, g => g.Shadows, (g, v) => g.Shadows = v);
        AddGradSlider(144, "飽和度", -100, 100, 0, "0", 1, g => g.Saturation, (g, v) => g.Saturation = v);

        var add = new FlatButton { Text = "新增線性漸層", Primary = true }; Ui.Place(add, 0, 184, 266, 30);
        add.Click += (_, _) => AddGradient?.Invoke();

        var reset = new FlatButton { Text = "漸層重設（清除全部）" }; Ui.Place(reset, 0, 218, 266, 30);
        reset.Click += (_, _) =>
        {
            if (Adj == null) return;
            RaiseEditBegin();
            Adj.Gradients.Clear();
            Adj.ActiveGradientIndex = -1;
            Bind(Adj);
            RaiseChanged();
        };
        _grad.Controls.Add(add);
        _grad.Controls.Add(reset);
    }

    private void AddGradSlider(int top, string label, double min, double max, double def, string fmt, double step,
        Func<LinearGradient, double> get, Action<LinearGradient, double> set)
    {
        var s = CreateSlider(label, min, max, def, fmt, true,
            a => a.ActiveGradient is { } g ? get(g) : def,
            (a, v) => { if (a.ActiveGradient is { } g) set(g, v); },
            step);
        Ui.Place(s, 0, top, 266, 36);
        _grad.Controls.Add(s);
        _gradSliders.Add(s);
    }

    public override void Bind(ImageAdjustments adj)
    {
        base.Bind(adj);
        bool has = adj.ActiveGradient != null;
        foreach (var s in _gradSliders) s.Enabled = has;
    }

    // ---- heal ------------------------------------------------------------

    private void BuildHeal()
    {
        _cloneBtn = new FlatButton { Text = "仿製" }; Ui.Place(_cloneBtn, 0, 0, 130, 30);
        _inpaintBtn = new FlatButton { Text = "修補" }; Ui.Place(_inpaintBtn, 136, 0, 130, 30);
        _cloneBtn.Primary = true;
        _cloneBtn.Click += (_, _) => SetHealMode(HealMode.Clone);
        _inpaintBtn.Click += (_, _) => SetHealMode(HealMode.Inpaint);

        var size = CreateSlider("大小", 0, 50, 10, "0", false, a => a.HealSize, (a, v) => a.HealSize = v, 1);
        Ui.Place(size, 0, 40, 266, 36);
        size.ValueChanged += (_, _) => HealSizeChanged?.Invoke(size.Value);

        var reset = new FlatButton { Text = "修護重設" }; Ui.Place(reset, 0, 84, 266, 30);
        reset.Click += (_, _) => ClearHeal?.Invoke();

        _heal.Controls.AddRange(new Control[] { _cloneBtn, _inpaintBtn, size, reset });
    }

    private void SetHealMode(HealMode mode)
    {
        _cloneBtn.Primary = mode == HealMode.Clone;
        _inpaintBtn.Primary = mode == HealMode.Inpaint;
        HealModeChanged?.Invoke(mode);
    }
}
