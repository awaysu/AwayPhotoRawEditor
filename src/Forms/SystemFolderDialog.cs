using System.IO;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;

namespace AwayPhotoRawEditor.Forms;

/// <summary>
/// Windows 內建的資料夾選擇對話框（.NET 8 的 <see cref="FolderBrowserDialog"/>，
/// 實際是 Vista 以後的 IFileDialog 資料夾模式：有網址列、搜尋、常用位置、新增資料夾）。
/// 2026-08 起取代自繪的 FolderPickerForm（使用者要求改用系統內建；舊版在 git 歷史）。
/// 注意：Win10 的系統對話框不跟著程式的深色主題走，一律是淺色。
/// </summary>
public static class SystemFolderDialog
{
    /// <summary>顯示對話框；取消或路徑無效回傳 null。`title` 為繁中 source text，內部會翻譯。</summary>
    public static string? Pick(IWin32Window owner, string? initial, string title)
    {
        using var dlg = new FolderBrowserDialog
        {
            Description = L.T(title),
            UseDescriptionForTitle = true,   // 不設的話 Description 只會變成對話框裡的一行小字
            ShowNewFolderButton = true,
            AutoUpgradeEnabled = true,       // 新式對話框（預設即 true，明寫以免被誤改）
        };
        if (!string.IsNullOrWhiteSpace(initial) && Directory.Exists(initial))
        {
            dlg.InitialDirectory = initial;
            dlg.SelectedPath = initial;
        }

        if (dlg.ShowDialog(owner) != DialogResult.OK) return null;
        var path = dlg.SelectedPath;
        if (string.IsNullOrWhiteSpace(path) || !Directory.Exists(path)) return null;

        // 系統對話框不像舊的自繪版會把 RAW_TEMP 藏起來；使用者點進快取資料夾時
        // 視為選了它所屬的相片資料夾，而不是打開一個只有快取檔的空資料夾。
        while (AppPaths.IsRawTemp(path))
        {
            var parent = Directory.GetParent(path)?.FullName;
            if (string.IsNullOrEmpty(parent)) break;
            path = parent;
        }
        return path;
    }
}
