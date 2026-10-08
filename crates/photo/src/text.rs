//! User-visible messages from this crate (export errors) go through the app's translator:
//! the Traditional Chinese text is the key, `{0}`, `{1}`… the arguments.

use std::fmt::Display;
use std::sync::OnceLock;

static TRANSLATOR: OnceLock<fn(&str) -> String> = OnceLock::new();

/// Called once by the app with its `i18n::tr`.
pub fn set_translator(f: fn(&str) -> String) {
    let _ = TRANSLATOR.set(f);
}

pub fn tr(key: &str, args: &[&dyn Display]) -> String {
    let mut s = TRANSLATOR.get().map(|f| f(key)).unwrap_or_else(|| key.to_string());
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), &a.to_string());
    }
    s
}
