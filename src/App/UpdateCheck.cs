using System;
using System.Net.Http;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace AwayPhotoRawEditor.App;

/// <summary>「檢查更新」查到的結果（只在確定有新版時才帶 Notes）。</summary>
/// <param name="UpdateAvailable">網站版本比目前執行的版本新。</param>
/// <param name="LatestVersion">網站上的最新版本，不含開頭的 v（例如 "1.0.17"）。</param>
/// <param name="Notes">該版的更新說明；查不到就是空字串。</param>
/// <param name="PageUrl">下載頁網址。</param>
public sealed record UpdateInfo(bool UpdateAvailable, string LatestVersion, string Notes, string PageUrl);

/// <summary>awaysu.cc/software 的「檢查更新」API。
/// 規格見 GitHub private repo <c>awaysu/software-web</c> 的 <c>readme_for_program.txt</c> A 節
/// （那份檔案只在該 repo 裡，本機沒有副本）。
///
/// <para>版本比較交給伺服器（<c>update_available</c>）：readme 註明規則是 PHP <c>version_compare</c>，
/// 自己在程式端重寫一份遲早會跟伺服器分岔（例如 1.0.17-beta &lt; 1.0.17 這種後綴規則）。</para>
///
/// <para><b>更新說明一律不讀 <c>release_notes</c></b>：那個欄位取的是版本歷史的第一筆，而本專案在網站上的
/// 歷史是<b>舊版在上</b>，所以它回的是 v1.0.0 的內容（2026-08-24 實測）。改用 <c>action=changelog&amp;version=</c>
/// 指名版本，與排序無關。<b>指名查不到就留空，不退回 <c>release_notes</c></b>——網站上還沒補該版歷史時，
/// 退路會在「最新版本 v1.0.17」底下印出 v1.0.0 的說明，比什麼都不顯示更糟。</para></summary>
public static class UpdateCheck
{
    private const string ApiUrl = "https://www.awaysu.cc/software/api.php";
    private const string AppSlug = "awayphotoraweditor";
    public const string PageUrl = "https://www.awaysu.cc/software/awayphotoraweditor";

    /// <summary>更新說明太長時 MessageBox 會撐滿整個螢幕，截掉尾巴。</summary>
    private const int MaxNotesChars = 900;

    // 一次檢查最多兩發短連線；共用單一 HttpClient（每次 new 會耗盡 socket）。
    // 逾時 10 秒：使用者按了按鈕在等，失敗要快，不要讓「檢查中…」卡很久。
    private static readonly HttpClient _http = CreateClient();

    private static HttpClient CreateClient()
    {
        var c = new HttpClient { Timeout = TimeSpan.FromSeconds(10) };
        // 有些共享主機會擋空的 User-Agent
        c.DefaultRequestHeaders.UserAgent.ParseAdd("AwayPhotoRawEditor/" + AppVersion.Version);
        return c;
    }

    /// <summary>查詢網站上的最新版本。任何失敗（沒網路、逾時、回傳格式不對、ok=false）一律回 null，
    /// 由呼叫端決定要不要提示——readme 要求啟動時的自動檢查失敗必須靜默略過。</summary>
    public static async Task<UpdateInfo?> FetchAsync(CancellationToken ct = default)
    {
        try
        {
            string url = ApiUrl + "?action=check_update&app=" + AppSlug + "&platform=windows"
                       + "&version=" + Uri.EscapeDataString(AppVersion.Version);

            using var doc = await GetJsonAsync(url, ct).ConfigureAwait(false);
            if (doc is null) return null;

            var root = doc.RootElement;
            if (root.ValueKind != JsonValueKind.Object) return null;
            if (!root.TryGetProperty("ok", out var ok) || ok.ValueKind != JsonValueKind.True) return null;

            string latest = Str(root, "latest_version");
            if (latest.Length == 0) return null;

            // update_available 沒帶 version 時為 null；這裡一定有帶，所以 null 視為「沒有新版」
            bool available = root.TryGetProperty("update_available", out var ua) && ua.ValueKind == JsonValueKind.True;

            string page = Str(root, "page_url");
            if (page.Length == 0) page = PageUrl;

            // 沒有新版就不必多打一次 API 拿說明。
            // 指名版本查不到就留空——不退回 release_notes，那個欄位在本專案必定是別版的內容（見類別註解）。
            string notes = available ? await FetchNotesAsync(latest, ct).ConfigureAwait(false) : "";

            return new UpdateInfo(available, latest, notes, page);
        }
        catch { return null; }   // 連線類例外都當成「查不到」
    }

    /// <summary>指名版本取更新說明（action=changelog&amp;version=）。查不到回空字串。</summary>
    private static async Task<string> FetchNotesAsync(string version, CancellationToken ct)
    {
        try
        {
            string url = ApiUrl + "?action=changelog&app=" + AppSlug
                       + "&version=" + Uri.EscapeDataString(version);

            using var doc = await GetJsonAsync(url, ct).ConfigureAwait(false);
            if (doc is null) return "";

            var root = doc.RootElement;
            if (root.ValueKind != JsonValueKind.Object) return "";
            if (!root.TryGetProperty("ok", out var ok) || ok.ValueKind != JsonValueKind.True) return "";
            if (!root.TryGetProperty("entries", out var entries) || entries.ValueKind != JsonValueKind.Array) return "";
            if (entries.GetArrayLength() == 0) return "";

            return Trim(Str(entries[0], "notes"));
        }
        catch { return ""; }
    }

    private static async Task<JsonDocument?> GetJsonAsync(string url, CancellationToken ct)
    {
        using var res = await _http.GetAsync(url, ct).ConfigureAwait(false);
        if (!res.IsSuccessStatusCode) return null;
        string body = await res.Content.ReadAsStringAsync(ct).ConfigureAwait(false);
        try { return JsonDocument.Parse(body); }
        catch (JsonException) { return null; }   // 主機掛掉時常常回 HTML 錯誤頁
    }

    private static string Str(JsonElement obj, string name) =>
        obj.TryGetProperty(name, out var el) && el.ValueKind == JsonValueKind.String ? el.GetString() ?? "" : "";

    private static string Trim(string notes)
    {
        notes = notes.Trim();
        return notes.Length <= MaxNotesChars ? notes : notes[..MaxNotesChars].TrimEnd() + "…";
    }
}
