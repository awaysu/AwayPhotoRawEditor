using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Xml.Serialization;
using AwayPhotoRawEditor.App;
using AwayPhotoRawEditor.Models;

namespace AwayPhotoRawEditor.Storage;

public sealed class NamedPreset
{
    public string Name { get; set; } = "";
    public ImageAdjustments Adjustments { get; set; } = new();
}

[XmlRoot("Presets")]
public sealed class PresetCollection
{
    public List<NamedPreset> Items { get; set; } = new();
}

/// <summary>
/// Stores user-defined presets (自訂1..3 and any saved custom looks) in
/// %AppData%\AwayPhotoRawEditor\presets.xml. Applying only copies tonal/colour/detail
/// fields onto the target, matching PresetProfile semantics.
/// </summary>
public static class PresetStore
{
    private static readonly XmlSerializer Serializer = new(typeof(PresetCollection));

    public static PresetCollection Load()
    {
        if (!File.Exists(AppPaths.PresetsPath)) return new PresetCollection();
        try
        {
            using var fs = File.OpenRead(AppPaths.PresetsPath);
            return Serializer.Deserialize(fs) as PresetCollection ?? new PresetCollection();
        }
        catch { return new PresetCollection(); }
    }

    public static void SaveAll(PresetCollection presets)
    {
        try
        {
            using var fs = File.Create(AppPaths.PresetsPath);
            Serializer.Serialize(fs, presets);
        }
        catch { }
    }

    /// <summary>Save (or overwrite) a named custom preset from a full adjustment set.</summary>
    public static void Save(string name, ImageAdjustments source)
    {
        var col = Load();
        col.Items.RemoveAll(p => p.Name == name);
        col.Items.Add(new NamedPreset { Name = name, Adjustments = source.Clone() });
        SaveAll(col);
    }

    /// <summary>Remove a preset's custom override so it reverts to its built-in default. </summary>
    public static void Remove(string name)
    {
        var col = Load();
        if (col.Items.RemoveAll(p => p.Name == name) > 0) SaveAll(col);
    }

    /// <summary>True when the named preset has a user-saved override.</summary>
    public static bool HasOverride(string name) => Load().Items.Any(p => p.Name == name);

    /// <summary>presets.xml 中不屬於內建清單的名稱（「編輯風格檔」視窗新增的自訂風格檔）。</summary>
    public static List<string> CustomNames()
        => Load().Items.Select(p => p.Name)
            .Where(n => !PresetProfile.BuiltIn.ContainsKey(n))
            .ToList();

    /// <summary>恢復預設：刪除所有自訂風格檔與所有內建風格檔的覆寫值。</summary>
    public static void ResetAllToDefaults() => SaveAll(new PresetCollection());

    /// <summary>備份：把整份風格檔設定（等同 presets.xml）寫到指定路徑。失敗擲出例外。</summary>
    public static void ExportTo(string path)
    {
        using var fs = File.Create(path);
        Serializer.Serialize(fs, Load());
    }

    /// <summary>還原：讀入備份檔並整份取代 presets.xml。
    /// 格式錯誤擲出例外；能解析但不是 PresetCollection 時回傳 false。</summary>
    public static bool ImportFrom(string path)
    {
        PresetCollection? col;
        using (var fs = File.OpenRead(path))
            col = Serializer.Deserialize(fs) as PresetCollection;
        if (col is null) return false;
        SaveAll(col);
        return true;
    }

    public static ImageAdjustments? Get(string name)
        => Load().Items.FirstOrDefault(p => p.Name == name)?.Adjustments;

    /// <summary>Apply a stored custom preset's tonal fields onto target. True if found.</summary>
    public static bool ApplyCustom(string name, ImageAdjustments target)
    {
        var p = Get(name);
        if (p is null) return false;
        target.ResetTonal();
        target.Exposure = p.Exposure; target.Contrast = p.Contrast; target.Highlights = p.Highlights;
        target.Shadows = p.Shadows; target.Whites = p.Whites; target.Blacks = p.Blacks;
        target.Temperature = p.Temperature; target.Tint = p.Tint;
        target.Vibrance = p.Vibrance; target.Saturation = p.Saturation;
        target.Sharpening = p.Sharpening; target.NoiseReduction = p.NoiseReduction;
        target.Vignette = p.Vignette; target.Distortion = p.Distortion;
        return true;
    }
}
