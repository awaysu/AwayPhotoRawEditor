using System.Drawing;
using System.Globalization;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>
/// Self-drawn label + value + track slider. Supports bipolar fill, reversed
/// direction, double-click reset, mouse-wheel fine steps and click-to-type on the
/// value. Raises <see cref="EditBegin"/> on interaction start (for undo snapshots)
/// and <see cref="ValueChanged"/> continuously.
/// </summary>
public sealed class AdjustmentSlider : Control
{
    private double _value;
    private bool _dragging;
    private TextBox? _editor;

    public string Label { get; set; } = "";
    public double Min { get; set; } = -100;
    public double Max { get; set; } = 100;
    public double DefaultValue { get; set; } = 0;
    public double WheelStep { get; set; } = double.NaN; // NaN => derived from range
    public string Format { get; set; } = "0";
    public bool Bipolar { get; set; }
    public bool Reverse { get; set; }

    public double Value
    {
        get => _value;
        set => SetValue(value, raiseBegin: false, user: false);
    }

    /// <summary>Fires when the user starts an interaction (for pushing undo state).</summary>
    public event EventHandler? EditBegin;
    /// <summary>Fires whenever the value changes.</summary>
    public event EventHandler? ValueChanged;

    // 96 DPI 設計值，取用時才乘 Ui.Scale。
    private static int TrackTop => Ui.S(24);
    /// <summary>右上角數值欄的寬度（點進去可直接打字）。</summary>
    private static int ValueWidth => Ui.S(70);

    public AdjustmentSlider()
    {
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint |
                 ControlStyles.OptimizedDoubleBuffer | ControlStyles.ResizeRedraw, true);
        Height = Ui.S(40);
        Font = Theme.Normal;
        Cursor = Cursors.Hand;
    }

    public void SetValueSilent(double v) { _value = Math.Clamp(v, Min, Max); Invalidate(); }

    private void SetValue(double v, bool raiseBegin, bool user)
    {
        v = Math.Clamp(v, Min, Max);
        if (Math.Abs(v - _value) < 1e-9) { if (raiseBegin) return; }
        _value = v;
        Invalidate();
        ValueChanged?.Invoke(this, EventArgs.Empty);
    }

    private double Step => double.IsNaN(WheelStep) ? (Max - Min) / 200.0 : WheelStep;

    // Map value <-> track x
    private RectangleF TrackRect => new(Ui.S(2f), TrackTop, Width - Ui.S(4f), Ui.S(6f));
    private float ValueToX(double v)
    {
        double t = (v - Min) / (Max - Min);
        if (Reverse) t = 1 - t;
        var tr = TrackRect;
        return (float)(tr.X + t * tr.Width);
    }
    private double XToValue(int x)
    {
        var tr = TrackRect;
        double t = (x - tr.X) / tr.Width;
        t = Math.Clamp(t, 0, 1);
        if (Reverse) t = 1 - t;
        return Min + t * (Max - Min);
    }

    protected override void OnMouseDown(MouseEventArgs e)
    {
        Focus();
        if (e.Button == MouseButtons.Left)
        {
            // Click on the value text (top-right) -> inline numeric edit.
            if (e.Y < TrackTop && e.X > Width - ValueWidth)
            {
                BeginEdit();
            }
            else
            {
                _dragging = true;
                EditBegin?.Invoke(this, EventArgs.Empty);
                SetValue(XToValue(e.X), raiseBegin: true, user: true);
            }
        }
        base.OnMouseDown(e);
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        if (_dragging) SetValue(XToValue(e.X), raiseBegin: false, user: true);
        base.OnMouseMove(e);
    }

    protected override void OnMouseUp(MouseEventArgs e) { _dragging = false; base.OnMouseUp(e); }

    protected override void OnMouseDoubleClick(MouseEventArgs e)
    {
        if (e.Y >= TrackTop || e.X <= Width - ValueWidth)
        {
            EditBegin?.Invoke(this, EventArgs.Empty);
            SetValue(DefaultValue, raiseBegin: false, user: true);
        }
        base.OnMouseDoubleClick(e);
    }

    protected override void OnMouseWheel(MouseEventArgs e)
    {
        EditBegin?.Invoke(this, EventArgs.Empty);
        int steps = e.Delta / 120;
        SetValue(_value + steps * Step, raiseBegin: false, user: true);
        ((HandledMouseEventArgs)e).Handled = true;
        base.OnMouseWheel(e);
    }

    private void BeginEdit()
    {
        _editor?.Dispose();
        _editor = new TextBox
        {
            BorderStyle = BorderStyle.FixedSingle,
            BackColor = Theme.PanelBg3,
            ForeColor = Theme.Text,
            Text = _value.ToString(Format, CultureInfo.InvariantCulture),
            Bounds = new Rectangle(Width - Ui.S(68), Ui.S(1), Ui.S(64), Ui.S(20)),
            TextAlign = HorizontalAlignment.Right
        };
        _editor.SelectAll();
        _editor.KeyDown += (_, ke) =>
        {
            if (ke.KeyCode == Keys.Enter) { CommitEdit(); ke.SuppressKeyPress = true; }
            else if (ke.KeyCode == Keys.Escape) { EndEdit(); ke.SuppressKeyPress = true; }
        };
        _editor.LostFocus += (_, _) => CommitEdit();
        Controls.Add(_editor);
        _editor.Focus();
    }

    private void CommitEdit()
    {
        if (_editor is null) return;
        if (double.TryParse(_editor.Text, NumberStyles.Any, CultureInfo.InvariantCulture, out var v))
        {
            EditBegin?.Invoke(this, EventArgs.Empty);
            SetValue(v, raiseBegin: false, user: true);
        }
        EndEdit();
    }

    private void EndEdit()
    {
        if (_editor is null) return;
        var ed = _editor; _editor = null;
        Controls.Remove(ed);
        ed.Dispose();
        Invalidate();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        g.Clear(Theme.PanelBg);

        TextRenderer.DrawText(g, Label, Font, new Rectangle(0, Ui.S(2), Width - ValueWidth, Ui.S(18)),
            Theme.TextDim, TextFormatFlags.Left | TextFormatFlags.VerticalCenter);
        if (_editor is null)
            TextRenderer.DrawText(g, _value.ToString(Format, CultureInfo.InvariantCulture), Font,
                new Rectangle(Width - ValueWidth, Ui.S(2), Ui.S(68), Ui.S(18)), Theme.Text,
                TextFormatFlags.Right | TextFormatFlags.VerticalCenter);

        var tr = TrackRect;
        PaintHelpers.FillRounded(g, tr, Ui.S(3f), Theme.SliderTrack);

        // fill
        float knobX = ValueToX(_value);
        if (Bipolar)
        {
            float midX = ValueToX(0);
            float x0 = Math.Min(midX, knobX), x1 = Math.Max(midX, knobX);
            PaintHelpers.FillRounded(g, new RectangleF(x0, tr.Y, Math.Max(1, x1 - x0), tr.Height), Ui.S(3f), Theme.SliderFill);
        }
        else
        {
            float x0 = ValueToX(Min);
            float x1 = knobX;
            if (x1 < x0) (x0, x1) = (x1, x0);
            PaintHelpers.FillRounded(g, new RectangleF(x0, tr.Y, Math.Max(1, x1 - x0), tr.Height), Ui.S(3f), Theme.SliderFill);
        }

        // knob
        float ky = tr.Y + tr.Height / 2f;
        float kr = Ui.S(6f), kd = Ui.S(12f);
        using (var b = new SolidBrush(Theme.SliderKnob))
            g.FillEllipse(b, knobX - kr, ky - kr, kd, kd);
        using (var pen = new Pen(Theme.WindowBg, Ui.S(1.5f)))
            g.DrawEllipse(pen, knobX - kr, ky - kr, kd, kd);
    }
}
