using System;
using System.Collections.Generic;

namespace AwayPhotoRawEditor.Models;

/// <summary>
/// A look preset ("風格檔"). Applying a preset resets the tonal/colour/detail
/// group and then applies the preset's values, leaving crop / gradient / heal /
/// watermark untouched. "預設時設定" performs a full reset instead.
/// </summary>
public sealed class PresetProfile
{
    public string Name { get; }
    public bool FullReset { get; }
    private readonly Action<ImageAdjustments>? _apply;

    public PresetProfile(string name, Action<ImageAdjustments>? apply, bool fullReset = false)
    {
        Name = name;
        _apply = apply;
        FullReset = fullReset;
    }

    public void ApplyTo(ImageAdjustments a)
    {
        if (FullReset) { a.ResetAll(); return; }
        a.ResetTonal();
        _apply?.Invoke(a);
    }

    // ---- Built-in registry (order = ComboBox order) ---------------------

    public const string DefaultName = "預設時設定";

    public static readonly IReadOnlyList<string> BuiltInNames = new[]
    {
        DefaultName, "風景", "人像", "鮮豔", "黑白", "柔和", "自訂1", "自訂2", "自訂3"
    };

    public static readonly IReadOnlyDictionary<string, PresetProfile> BuiltIn =
        new Dictionary<string, PresetProfile>(StringComparer.Ordinal)
        {
            [DefaultName] = new(DefaultName, null, fullReset: true),

            ["風景"] = new("風景", a =>
            {
                a.Contrast = 18; a.Highlights = -20; a.Shadows = 12; a.Whites = 8; a.Blacks = -10;
                a.Temperature = 5600; a.Vibrance = 28; a.Saturation = 8;
                a.Sharpening = 25; a.NoiseReduction = 5;
            }),

            ["人像"] = new("人像", a =>
            {
                // 曝光值以新版（真 EV）管線為準：舊版乘在 gamma 值上、同樣數字約強 2.2 倍。
                a.Exposure = 0.2; a.Contrast = -6; a.Highlights = -12; a.Shadows = 18;
                a.Temperature = 5400; a.Tint = 4; a.Vibrance = 10; a.Saturation = -4;
                a.Sharpening = 5; a.NoiseReduction = 12;
            }),

            ["鮮豔"] = new("鮮豔", a =>
            {
                a.Contrast = 14; a.Highlights = -10; a.Whites = 8; a.Blacks = -8;
                a.Vibrance = 38; a.Saturation = 16; a.Sharpening = 18;
            }),

            ["黑白"] = new("黑白", a =>
            {
                a.Contrast = 22; a.Highlights = -15; a.Shadows = 8; a.Whites = 10; a.Blacks = -16;
                a.Saturation = -100; a.Sharpening = 20;
            }),

            ["柔和"] = new("柔和", a =>
            {
                a.Exposure = 0.3; a.Contrast = -14; a.Highlights = -18; a.Shadows = 24; a.Blacks = 6;
                a.Temperature = 5650; a.Vibrance = 6; a.Saturation = -6;
                a.Sharpening = -10; a.NoiseReduction = 15;
            }),

            // 自訂1..3 are user-defined; loaded/overridden from PresetStore in Phase 2.
            ["自訂1"] = new("自訂1", null),
            ["自訂2"] = new("自訂2", null),
            ["自訂3"] = new("自訂3", null),
        };

    public static PresetProfile? Get(string name) =>
        BuiltIn.TryGetValue(name, out var p) ? p : null;
}
