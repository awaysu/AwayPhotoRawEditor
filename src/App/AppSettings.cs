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
/// 全部 UI 字級，單位是「100%（96 DPI）下的像素」——刻意用整數像素而不是點數，
/// 這樣使用者在設定裡看到與微調的都是沒有小數的數字（點數換算為 px × 0.75）。
/// 實際渲染時 <c>Theme</c> 會再乘上 <c>Ui.FontScale</c> 對應系統 DPI 與介面大小。
///
/// 欄位新增一律往後加；舊 settings.xml 缺少的欄位會沿用這裡的預設值。
/// </summary>
public sealed class FontSizes
{
    // 預設值＝使用者在 v1.0.6 實機調整後認定「100% 下剛好」的比例
    // （原始設計值較小：Small 11 / Normal 12 / SectionTitle 13 / Logo 20）。

    /// <summary>小字：縮圖檔名、#編號/copy 標記、EXIF 欄位名、灰色提示、進度條 %。</summary>
    public int Small { get; set; } = 15;
    /// <summary>等寬（Consolas）：直方圖下方 R/G/B 平均值。</summary>
    public int Mono { get; set; } = 11;
    /// <summary>一般（預設字級）：調整區滑桿標籤與數值、EXIF 值、下拉、輸入框、按鈕、工具分頁。</summary>
    public int Normal { get; set; } = 15;
    /// <summary>區塊標題（粗體）：基本調整／色彩／細節／直方圖／照片資訊／工具／風格檔種類。</summary>
    public int SectionTitle { get; set; } = 16;
    /// <summary>關於視窗內文。</summary>
    public int AboutBody { get; set; } = 16;
    /// <summary>選擇資料夾清單的 📁 / 💽 圖示。</summary>
    public int FolderGlyph { get; set; } = 15;
    /// <summary>小圖示按鈕（白平衡滴管等）。</summary>
    public int IconGlyph { get; set; } = 16;
    /// <summary>進度視窗標題（粗體）。</summary>
    public int ProgressTitle { get; set; } = 16;
    /// <summary>對話框標題（粗體）：匯出照片／設定。</summary>
    public int DialogTitle { get; set; } = 17;
    /// <summary>關於視窗標題（粗體）。</summary>
    public int AboutTitle { get; set; } = 22;
    /// <summary>左上 logo「AwayPhotoRawEditor」（粗體）。</summary>
    public int Logo { get; set; } = 22;
    /// <summary>左上漢堡選單 ☰。</summary>
    public int MenuGlyph { get; set; } = 27;

    /// <summary>可調範圍（px @100%）。太小看不見、太大會撐破固定寬度的框。</summary>
    public const int MinPx = 8;
    public const int MaxPx = 48;

    /// <summary>把所有欄位夾回合法範圍（防手改壞 settings.xml 造成介面不可用）。</summary>
    public void Clamp()
    {
        foreach (var p in typeof(FontSizes).GetProperties())
        {
            if (!p.CanWrite || p.PropertyType != typeof(int)) continue;
            int v = (int)(p.GetValue(this) ?? 0);
            p.SetValue(this, Math.Clamp(v, MinPx, MaxPx));
        }
    }

    public FontSizes Clone()
    {
        var c = new FontSizes();
        foreach (var p in typeof(FontSizes).GetProperties())
            if (p.CanWrite && p.PropertyType == typeof(int)) p.SetValue(c, p.GetValue(this));
        return c;
    }

    /// <summary>用於偵測「使用者改了字級 → 需要重啟」。</summary>
    public string Signature()
    {
        var sb = new System.Text.StringBuilder();
        foreach (var p in typeof(FontSizes).GetProperties())
            if (p.CanWrite && p.PropertyType == typeof(int)) sb.Append(p.GetValue(this)).Append(',');
        return sb.ToString();
    }
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

    /// <summary>用 GPU（Direct3D 12 / ComputeSharp）算圖；偵測不到硬體裝置或失敗時自動退回 CPU。</summary>
    public bool UseGpu { get; set; } = true;

    /// <summary>Show the #1.. index number on each thumbnail (top-left).</summary>
    public bool ShowThumbnailNumber { get; set; } = true;

    /// <summary>視窗過矮時左右欄顯示捲軸（DarkScrollHost）；關閉時超出部分直接裁切。</summary>
    public bool ShowColumnScrollBars { get; set; } = false;

    /// <summary>預覽列「顯示全部」模式：true = 連同已隱藏（不輸出）的照片一起顯示（帶隱藏 icon）；
    /// false =「不顯示隱藏」（預設，隱藏的照片不出現在預覽列）。</summary>
    public bool ShowHiddenPhotos { get; set; } = false;

    /// <summary>整體介面風格；舊版設定檔未含此欄位時維持原本的經典深色。</summary>
    public UiStyle InterfaceStyle { get; set; } = UiStyle.ClassicDark;

    /// <summary>介面語言；舊版設定檔未含此欄位時維持繁體中文。</summary>
    public AppLanguage UiLanguage { get; set; } = AppLanguage.TraditionalChinese;

    /// <summary>介面大小（%）。0 = 自動：跟隨系統縮放，但不超過螢幕容得下的大小
    /// （版面是照 100% 螢幕設計的，1920×1080 開 150% 硬放大會裁掉整塊區域）。
    /// 指定 100/125/150/175/200 則固定該倍率，放不下時左右欄自動出現捲軸。</summary>
    public int UiScalePercent { get; set; } = 0;

    /// <summary>全部 UI 字級（100% 下的整數像素）。設定 →「字體大小…」可微調。</summary>
    public FontSizes FontSizes { get; set; } = new();

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

    /// <summary>啟動時 settings.xml 不存在＝第一次執行（舊版名稱資料夾的設定已由 AppPaths
    /// 複製過來，所以搬家過來的使用者不算）。用來決定要不要先跳語言選擇畫面。</summary>
    public static bool IsFirstRun { get; private set; }

    public static void Load()
    {
        IsFirstRun = !File.Exists(AppPaths.SettingsPath);
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
        Current.FontSizes ??= new FontSizes();   // 舊設定檔沒有這個節點
        Current.FontSizes.Clamp();
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
