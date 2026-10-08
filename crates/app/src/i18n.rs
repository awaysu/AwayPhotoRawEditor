//! Interface languages (the C# `Localization` / `L`). The Traditional Chinese source text
//! is the key; every key has seven translations (En, Ja, Ko, Hans, De, Fr, Es) in
//! `i18n_table.rs`, which holds the whole C# table plus the strings this build added.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

include!("i18n_table.rs");

/// Order and names = the C# `AppLanguage` enum (stored in settings.xml).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    ZhTw,
    En,
    Ja,
    Ko,
    ZhCn,
    De,
    Fr,
    Es,
}

impl Lang {
    pub const ALL: [Lang; 8] = [Self::ZhTw, Self::En, Self::Ja, Self::Ko, Self::ZhCn, Self::De, Self::Fr, Self::Es];

    pub fn xml_name(self) -> &'static str {
        match self {
            Self::ZhTw => "TraditionalChinese",
            Self::En => "English",
            Self::Ja => "Japanese",
            Self::Ko => "Korean",
            Self::ZhCn => "SimplifiedChinese",
            Self::De => "German",
            Self::Fr => "French",
            Self::Es => "Spanish",
        }
    }

    pub fn from_xml(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.xml_name() == s)
    }

    /// How the language names itself (`LanguageDisplayName`).
    pub fn display_name(self) -> &'static str {
        match self {
            Self::ZhTw => "繁體中文（台灣）",
            Self::En => "English (United States)",
            Self::Ja => "日本語（日本）",
            Self::Ko => "한국어（대한민국）",
            Self::ZhCn => "简体中文（中国）",
            Self::De => "Deutsch (Deutschland)",
            Self::Fr => "Français (France)",
            Self::Es => "Español (España)",
        }
    }

    /// The English name, shown under the native one on the first-run cards (the only clue
    /// when a script cannot be drawn).
    pub fn english_name(self) -> &'static str {
        match self {
            Self::ZhTw => "Traditional Chinese",
            Self::En => "English",
            Self::Ja => "Japanese",
            Self::Ko => "Korean",
            Self::ZhCn => "Simplified Chinese",
            Self::De => "German",
            Self::Fr => "French",
            Self::Es => "Spanish",
        }
    }

    fn column(self) -> Option<usize> {
        match self {
            Self::ZhTw => None,
            Self::En => Some(0),
            Self::Ja => Some(1),
            Self::Ko => Some(2),
            Self::ZhCn => Some(3),
            Self::De => Some(4),
            Self::Fr => Some(5),
            Self::Es => Some(6),
        }
    }

    /// The first-run guess from the system's UI language; anything unknown → English
    /// (someone who has to pick a language is rarely a Chinese reader).
    pub fn guess_from_system() -> Self {
        Self::from_locale(&sys_locale::get_locale().unwrap_or_default())
    }

    pub fn from_locale(tag: &str) -> Self {
        let t = tag.replace('_', "-").to_ascii_lowercase();
        let primary = t.split('-').next().unwrap_or("");
        match primary {
            "zh" if t.contains("hant") || t.ends_with("-tw") || t.ends_with("-hk") || t.ends_with("-mo") => Self::ZhTw,
            "zh" => Self::ZhCn,
            "ja" => Self::Ja,
            "ko" => Self::Ko,
            "de" => Self::De,
            "fr" => Self::Fr,
            "es" => Self::Es,
            _ => Self::En,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    CURRENT.store(Lang::ALL.iter().position(|x| *x == l).unwrap_or(0) as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    Lang::ALL[CURRENT.load(Ordering::Relaxed) as usize % 8]
}

fn map() -> &'static HashMap<&'static str, &'static [&'static str; 7]> {
    static M: OnceLock<HashMap<&'static str, &'static [&'static str; 7]>> = OnceLock::new();
    M.get_or_init(|| TABLE.iter().map(|(k, v)| (*k, v)).collect())
}

/// The current language's text for a source string (the source itself in Traditional
/// Chinese, or when it is not in the table).
pub fn t(source: &'static str) -> &'static str {
    match (lang().column(), map().get(source)) {
        (Some(c), Some(tr)) => tr[c],
        _ => source,
    }
}

/// `t` for a string known only at run time (preset names, labels from other crates).
pub fn tr(source: &str) -> String {
    match (lang().column(), map().get(source)) {
        (Some(c), Some(tr)) => tr[c].to_string(),
        _ => source.to_string(),
    }
}

/// `t` with `{0}`, `{1}`… replaced by the arguments (`L.F`).
pub fn f(source: &'static str, args: &[&dyn Display]) -> String {
    let mut s = t(source).to_string();
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), &a.to_string());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_eight_languages() {
        let mut seen = std::collections::HashSet::new();
        for (k, v) in TABLE {
            assert!(seen.insert(*k), "duplicate key {k:?}");
            assert!(!k.is_empty());
            for (i, s) in v.iter().enumerate() {
                assert!(!s.trim().is_empty(), "{k:?}: language {i} is empty");
            }
            // Every placeholder of the source appears in each translation.
            for n in 0..4 {
                let p = format!("{{{n}}}");
                if k.contains(&p) {
                    assert!(v.iter().all(|s| s.contains(&p)), "{k:?}: {p} missing in a translation");
                }
            }
        }
    }

    #[test]
    fn lookups() {
        set_lang(Lang::En);
        assert_eq!(t("取消"), "Cancel");
        assert_eq!(f("已匯出 {0} 張相片", &[&3]), "Exported 3 photos");
        assert_eq!(t("不在表裡的字"), "不在表裡的字");
        set_lang(Lang::De);
        assert_eq!(tr("風景"), "Landschaft");
        set_lang(Lang::ZhTw);
        assert_eq!(t("取消"), "取消");
        assert_eq!(Lang::from_locale("zh-Hant-TW"), Lang::ZhTw);
        assert_eq!(Lang::from_locale("zh_CN"), Lang::ZhCn);
        assert_eq!(Lang::from_locale("ja-JP"), Lang::Ja);
        assert_eq!(Lang::from_locale("pt-BR"), Lang::En);
        assert_eq!(Lang::from_xml("Korean"), Some(Lang::Ko));
    }

    /// Every Chinese string literal in the app's sources (outside comments, tests and lines
    /// marked `// i18n-ignore`) must be a key of the table — i.e. translated.
    #[test]
    fn no_untranslated_chinese_literals() {
        let keys: std::collections::HashSet<&str> = TABLE.iter().map(|(k, _)| *k).collect();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let name = p.file_name().unwrap().to_string_lossy().into_owned();
                if !name.ends_with(".rs") || name.starts_with("i18n") {
                    continue;
                }
                let text = std::fs::read_to_string(&p).unwrap();
                let code = text.split("#[cfg(test)]").next().unwrap_or("");
                for (n, line) in code.lines().enumerate() {
                    if line.contains("i18n-ignore") {
                        continue;
                    }
                    for lit in string_literals(line) {
                        if lit.chars().any(is_cjk) && !keys.contains(lit.as_str()) {
                            missing.push(format!("{}:{}: {lit:?}", name, n + 1));
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "untranslated strings:\n{}", missing.join("\n"));
    }

    fn is_cjk(c: char) -> bool {
        matches!(c as u32, 0x3000..=0x9FFF | 0xAC00..=0xD7AF | 0xFF00..=0xFFEF)
    }

    /// The "…" literals of one line of code, unescaped; stops at a `//` comment.
    fn string_literals(line: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut it = line.chars().peekable();
        let mut prev = ' ';
        while let Some(c) = it.next() {
            if c == '/' && it.peek() == Some(&'/') {
                break;
            }
            // Skip char literals like '"' and lifetimes.
            if c == '\'' {
                let rest: String = it.clone().take(3).collect();
                if rest.len() >= 2 && (rest.chars().nth(1) == Some('\'') || rest.starts_with("\\") && rest.chars().nth(2) == Some('\'')) {
                    let skip = if rest.starts_with('\\') { 3 } else { 2 };
                    for _ in 0..skip {
                        it.next();
                    }
                }
                prev = c;
                continue;
            }
            if c == '"' && prev != 'r' {
                let mut s = String::new();
                while let Some(d) = it.next() {
                    match d {
                        '\\' => match it.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(o) => s.push(o),
                            None => {}
                        },
                        '"' => break,
                        _ => s.push(d),
                    }
                }
                out.push(s);
            }
            prev = c;
        }
        out
    }
}
