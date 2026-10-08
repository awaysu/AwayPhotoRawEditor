//! 檢查更新 against the awaysu.cc software API (the C# `UpdateCheck`): the server compares
//! versions (`update_available`); the notes come from `action=changelog&version=` for that
//! exact version, never from `release_notes` (which returns the oldest entry here).

use std::time::Duration;

const API_URL: &str = "https://www.awaysu.cc/software/api.php";
const APP_SLUG: &str = "awayphotoraweditor";
pub const PAGE_URL: &str = "https://www.awaysu.cc/software/awayphotoraweditor";
/// Notes longer than this are cut (they would fill the screen).
const MAX_NOTES_CHARS: usize = 900;

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateInfo {
    pub update_available: bool,
    /// Without a leading "v".
    pub latest_version: String,
    /// Only fetched when there is a newer version; empty when the site has none.
    pub notes: String,
    pub page_url: String,
}

pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into()
}

fn get_json(agent: &ureq::Agent, url: &str) -> Option<serde_json::Value> {
    // Some shared hosts reject an empty User-Agent.
    let mut res = agent.get(url).header("User-Agent", format!("AwayPhotoRawEditor/{}", env!("CARGO_PKG_VERSION"))).call().ok()?;
    let body = res.body_mut().read_to_string().ok()?;
    // A broken host often answers with an HTML error page.
    serde_json::from_str(&body).ok()
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// The newest version on the site. None for any failure (offline, timeout, bad answer).
pub fn fetch(version: &str) -> Option<UpdateInfo> {
    let agent = agent();
    let url = format!("{API_URL}?action=check_update&app={APP_SLUG}&platform={}&version={}", platform(), encode(version));
    let root = get_json(&agent, &url)?;
    parse_check(&root).map(|mut info| {
        if info.update_available {
            info.notes = fetch_notes(&agent, &info.latest_version);
        }
        info
    })
}

/// The `check_update` answer (without notes).
pub fn parse_check(root: &serde_json::Value) -> Option<UpdateInfo> {
    if root.get("ok")?.as_bool() != Some(true) {
        return None;
    }
    let latest = root.get("latest_version")?.as_str()?.trim().trim_start_matches('v').to_string();
    if latest.is_empty() {
        return None;
    }
    let page = root.get("page_url").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or(PAGE_URL).to_string();
    Some(UpdateInfo { update_available: root.get("update_available").and_then(|v| v.as_bool()) == Some(true), latest_version: latest, notes: String::new(), page_url: page })
}

fn fetch_notes(agent: &ureq::Agent, version: &str) -> String {
    let url = format!("{API_URL}?action=changelog&app={APP_SLUG}&version={}", encode(version));
    get_json(agent, &url).map(|r| parse_notes(&r)).unwrap_or_default()
}

/// The first entry's notes of a `changelog` answer, trimmed and cut to length.
pub fn parse_notes(root: &serde_json::Value) -> String {
    if root.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return String::new();
    }
    let notes = root.get("entries").and_then(|e| e.as_array()).and_then(|a| a.first()).and_then(|e| e.get("notes")).and_then(|n| n.as_str()).unwrap_or("").trim();
    if notes.chars().count() <= MAX_NOTES_CHARS {
        notes.to_string()
    } else {
        notes.chars().take(MAX_NOTES_CHARS).collect::<String>().trim_end().to_string() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_api_answers() {
        let v: serde_json::Value = serde_json::from_str(r#"{"ok":true,"latest_version":"v2.0.1","update_available":true,"page_url":""}"#).unwrap();
        let i = parse_check(&v).unwrap();
        assert_eq!((i.update_available, i.latest_version.as_str(), i.page_url.as_str()), (true, "2.0.1", PAGE_URL));
        let v: serde_json::Value = serde_json::from_str(r#"{"ok":true,"latest_version":"1.0.18","update_available":null}"#).unwrap();
        assert!(!parse_check(&v).unwrap().update_available);
        let v: serde_json::Value = serde_json::from_str(r#"{"ok":false}"#).unwrap();
        assert!(parse_check(&v).is_none());
        let n: serde_json::Value = serde_json::from_str(r#"{"ok":true,"entries":[{"notes":"  修正 A\n新增 B  "}]}"#).unwrap();
        assert_eq!(parse_notes(&n), "修正 A\n新增 B");
        let empty: serde_json::Value = serde_json::from_str(r#"{"ok":true,"entries":[]}"#).unwrap();
        assert_eq!(parse_notes(&empty), "");
        assert_eq!(encode("2.0.0-dev"), "2.0.0-dev");
        assert_eq!(encode("a b"), "a%20b");
    }

    /// The real server (network): `cargo test -p awpr-app live_update_check -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_update_check() {
        let r = fetch(env!("CARGO_PKG_VERSION"));
        println!("check_update({}) on {} → {r:#?}", env!("CARGO_PKG_VERSION"), platform());
        assert!(r.is_some(), "no answer from the server");
    }
}
