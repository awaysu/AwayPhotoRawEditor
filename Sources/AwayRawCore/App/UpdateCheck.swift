import Foundation

/// What "檢查更新" found. `notes` is only filled in when there really is a newer version.
public struct UpdateInfo: Sendable {
    /// The site's version is newer than the running one.
    public let updateAvailable: Bool
    /// The newest version on the site, without the leading v (e.g. "1.0.17").
    public let latestVersion: String
    /// That version's release notes; empty when they could not be fetched.
    public let notes: String
    /// The download page.
    public let pageUrl: String
}

/// The awaysu.cc/software update API. Spec lives in the private repo
/// `awaysu/software-web`, `readme_for_program.txt` section A.
///
/// **Version comparison is the server's job** (`update_available`): the readme states the
/// rule is PHP `version_compare`, and a second implementation here would eventually
/// disagree with it over suffixes like `1.0.17-beta < 1.0.17`.
///
/// **The notes never come from `release_notes`.** That field takes the first entry of the
/// version history, and this project's history on the site is oldest-first, so it returns
/// v1.0.0's text (measured 2026-08-24). `action=changelog&version=` names the version
/// explicitly and is order-independent. **If the named lookup finds nothing the notes stay
/// empty rather than falling back** — printing v1.0.0's notes under a "latest version
/// v1.0.17" heading is worse than printing none.
public enum UpdateCheck {

    private static let apiUrl = "https://www.awaysu.cc/software/api.php"
    // macOS 版在網站上是獨立的 app 條目（awayphotoraweditor_mac），與 Windows 版
    // （awayphotoraweditor）分開管理版本與下載檔。
    private static let appSlug = "awayphotoraweditor_mac"
    public static let pageUrl = "https://www.awaysu.cc/software/awayphotoraweditor_mac"

    /// Long notes would blow a dialog up to fill the screen; cut the tail.
    private static let maxNotesChars = 900

    /// The platform this build reports to the API. The Windows build sends `windows`.
    private static let platform = "macos"

    /// A single shared session — a new one per check leaks sockets. Ten seconds: the user
    /// is watching a button, so failure has to be quick rather than leaving "檢查中…" up.
    nonisolated(unsafe) private static let session: URLSession = {
        let c = URLSessionConfiguration.ephemeral
        c.timeoutIntervalForRequest = 10
        c.timeoutIntervalForResource = 10
        // Some shared hosts reject an empty User-Agent.
        c.httpAdditionalHeaders = ["User-Agent": "AwayPhotoRawEditor/\(AppVersionInfo.version)"]
        return URLSession(configuration: c)
    }()

    /// Ask the site for the newest version. **Any** failure — offline, timeout, bad shape,
    /// `ok: false` — returns nil, leaving it to the caller whether to say anything. The
    /// readme requires a failed automatic check at startup to be silently ignored.
    public static func fetch() async -> UpdateInfo? {
        guard var comps = URLComponents(string: apiUrl) else { return nil }
        comps.queryItems = [
            URLQueryItem(name: "action", value: "check_update"),
            URLQueryItem(name: "app", value: appSlug),
            URLQueryItem(name: "platform", value: platform),
            URLQueryItem(name: "version", value: AppVersionInfo.version),
        ]
        guard let url = comps.url,
              let root = await getJson(url),
              root["ok"] as? Bool == true,
              let latest = root["latest_version"] as? String, !latest.isEmpty
        else { return nil }

        // update_available is null when no version was supplied; we always supply one,
        // so treat anything but true as "no update".
        let available = root["update_available"] as? Bool == true

        var page = root["page_url"] as? String ?? ""
        if page.isEmpty { page = pageUrl }

        // No new version means no reason to spend a second request on the notes.
        let notes = available ? await fetchNotes(version: latest) : ""
        return UpdateInfo(updateAvailable: available, latestVersion: latest,
                          notes: notes, pageUrl: page)
    }

    /// Notes for a named version (`action=changelog&version=`). Empty when not found.
    private static func fetchNotes(version: String) async -> String {
        guard var comps = URLComponents(string: apiUrl) else { return "" }
        comps.queryItems = [
            URLQueryItem(name: "action", value: "changelog"),
            URLQueryItem(name: "app", value: appSlug),
            URLQueryItem(name: "version", value: version),
        ]
        guard let url = comps.url,
              let root = await getJson(url),
              root["ok"] as? Bool == true,
              let entries = root["entries"] as? [[String: Any]],
              let first = entries.first,
              let notes = first["notes"] as? String
        else { return "" }
        return trim(notes)
    }

    private static func getJson(_ url: URL) async -> [String: Any]? {
        do {
            let (data, response) = try await session.data(from: url)
            guard let http = response as? HTTPURLResponse,
                  (200..<300).contains(http.statusCode) else { return nil }
            // A host that is down often answers with an HTML error page.
            return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        } catch {
            return nil
        }
    }

    private static func trim(_ notes: String) -> String {
        let t = notes.trimmingCharacters(in: .whitespacesAndNewlines)
        guard t.count > maxNotesChars else { return t }
        return String(t.prefix(maxNotesChars))
            .trimmingCharacters(in: .whitespacesAndNewlines) + "…"
    }
}

/// The running version, shared between the About window and the update check.
public enum AppVersionInfo {
    public static let version = "1.0.18"
}
