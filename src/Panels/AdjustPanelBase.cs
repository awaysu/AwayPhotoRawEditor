using System;
using System.Collections.Generic;
using System.Windows.Forms;
using AwayPhotoRawEditor.Controls;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Panels;

/// <summary>
/// Base for slider-driven adjustment sections. Sliders write directly into the
/// bound <see cref="ImageAdjustments"/>; <see cref="EditBegin"/> fires on
/// interaction start (undo snapshot) and <see cref="Changed"/> after every change.
/// </summary>
public abstract class AdjustPanelBase : SectionPanel
{
    protected ImageAdjustments? Adj;
    private readonly List<Action<ImageAdjustments>> _loaders = new();

    public event Action? EditBegin;
    public event Action? Changed;

    protected AdjustPanelBase(string title) : base(title) { ManualLayout = true; }

    /// <summary>Create a wired slider (registered for Bind) without placing it — caller sets Bounds.</summary>
    protected AdjustmentSlider CreateSlider(string label, double min, double max, double def,
        string fmt, bool bipolar, Func<ImageAdjustments, double> get, Action<ImageAdjustments, double> set,
        double wheelStep = double.NaN)
    {
        var s = new AdjustmentSlider
        {
            Label = label, Min = min, Max = max, DefaultValue = def, Format = fmt,
            Bipolar = bipolar, WheelStep = wheelStep, Height = 36
        };
        s.SetValueSilent(def);
        s.EditBegin += (_, _) => EditBegin?.Invoke();
        s.ValueChanged += (_, _) => { if (Adj != null) { set(Adj, s.Value); Changed?.Invoke(); } };
        _loaders.Add(a => s.SetValueSilent(get(a)));
        return s;
    }

    /// <summary>Create a wired slider and place it into ContentArea at the given bounds.</summary>
    protected AdjustmentSlider AddSliderAt(int x, int y, int w, int h, string label,
        double min, double max, double def, string fmt, bool bipolar,
        Func<ImageAdjustments, double> get, Action<ImageAdjustments, double> set, double wheelStep = double.NaN)
    {
        var s = CreateSlider(label, min, max, def, fmt, bipolar, get, set, wheelStep);
        s.SetBounds(x, y, w, h);
        ContentArea.Controls.Add(s);
        return s;
    }

    /// <summary>Load the panel's controls from the given adjustments without firing events.</summary>
    public virtual void Bind(ImageAdjustments adj)
    {
        Adj = null;
        foreach (var l in _loaders) l(adj);
        Adj = adj;
    }

    protected void RegisterLoader(Action<ImageAdjustments> loader) => _loaders.Add(loader);
    protected void RaiseEditBegin() => EditBegin?.Invoke();
    protected void RaiseChanged() => Changed?.Invoke();
}
