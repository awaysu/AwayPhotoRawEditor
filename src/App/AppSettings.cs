using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Xml.Serialization;

namespace AwayPhotoRawEditor.App;

/// <summary>可由設定切換的整體介面風格。</summary>
public enum UiStyle
{
    ClassicDark,
    WarmPaper
}

/// <summary>Supported application UI languages.（新值一律往後加，settings.xml 存的是名稱但別重排既有順序）</summary>
public enum AppLanguage
{
    TraditionalChinese,
    English,
    Japanese,
    Korean,
    SimplifiedChinese,
    German,
    French,
    Spanish
}

/// <summary>
/// Persistent application settings (settings.xml under %AppData%\AwayPhotoRawEditor).
/// Governs RawLoader behaviour.
/// </summary>
public sealed class AppSettings
{
    /// <summary>Use LibRaw for RAW decoding (falls back to WIC/Bitmap when false or unavailable).</summary>
    public bool UseLibRaw { get; set; } = true;

    /// <summary>Use the float RGBA high-precision RAW pipeline (16-bit proxy cache).</summary>
    public bool UseHighPrecisionRawPipeline { get; set; } = false;

    /// <summary>Show the #1.. index number on each thumbnail (top-left).</summary>
    public bool ShowThumbnailNumber { get; set; } = true;

    /// <summary>視窗過矮時左右欄顯示捲軸（DarkScrollHost）；關閉時超出部分直接裁切。</summary>
    public bool ShowColumnScrollBars { get; set; } = false;

    /// <summary>整體介面風格；舊版設定檔未含此欄位時維持原本的經典深色。</summary>
    public UiStyle InterfaceStyle { get; set; } = UiStyle.ClassicDark;

    /// <summary>介面語言；舊版設定檔未含此欄位時維持繁體中文。</summary>
    public AppLanguage UiLanguage { get; set; } = AppLanguage.TraditionalChinese;

    // Last used folder — convenience, restored at startup.
    public string LastFolder { get; set; } = "";

    /// <summary>已開啟過的資料夾紀錄（最近在前）。</summary>
    public List<string> RecentFolders { get; set; } = new();

    /// <summary>把資料夾加入開啟紀錄（去重、最近在前、上限 20 筆）。</summary>
    public void PushRecentFolder(string folder)
    {
        if (string.IsNullOrWhiteSpace(folder)) return;
        RecentFolders.RemoveAll(f => string.Equals(f, folder, StringComparison.OrdinalIgnoreCase));
        RecentFolders.Insert(0, folder);
        if (RecentFolders.Count > 20) RecentFolders.RemoveRange(20, RecentFolders.Count - 20);
    }

    [XmlIgnore] public static AppSettings Current { get; private set; } = new();

    public static void Load()
    {
        try
        {
            if (File.Exists(AppPaths.SettingsPath))
            {
                using var fs = File.OpenRead(AppPaths.SettingsPath);
                var s = new XmlSerializer(typeof(AppSettings));
                if (s.Deserialize(fs) is AppSettings loaded) Current = loaded;
            }
        }
        catch
        {
            // Corrupt settings -> keep defaults.
            Current = new AppSettings();
        }
    }

    public void Save()
    {
        try
        {
            using var fs = File.Create(AppPaths.SettingsPath);
            new XmlSerializer(typeof(AppSettings)).Serialize(fs, this);
        }
        catch { /* non-fatal */ }
    }
}
