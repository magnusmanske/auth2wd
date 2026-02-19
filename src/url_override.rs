/// URL prefix override registry — used in tests to redirect external HTTP calls
/// to a local wiremock server.
///
/// In production code this map is always empty, so `maybe_rewrite` is a no-op.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static OVERRIDES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn overrides() -> &'static Mutex<HashMap<String, String>> {
    OVERRIDES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a URL prefix replacement: any URL that starts with `from` will
/// have that prefix replaced with `to`.
///
/// Only called from test code (unit tests and integration tests).
/// Compiled into the library so integration tests in `tests/` can use it.
pub fn register(from: impl Into<String>, to: impl Into<String>) {
    overrides()
        .lock()
        .expect("url_override mutex poisoned")
        .insert(from.into(), to.into());
}

/// Remove all registered overrides.
///
/// Only called from test code (unit tests and integration tests).
/// Compiled into the library so integration tests in `tests/` can use it.
pub fn clear() {
    overrides()
        .lock()
        .expect("url_override mutex poisoned")
        .clear();
}

/// Rewrite `url` using any registered prefix overrides.
/// Returns the original URL unchanged when no override matches.
pub fn maybe_rewrite(url: &str) -> String {
    let map = overrides().lock().expect("url_override mutex poisoned");
    for (from, to) in map.iter() {
        if url.starts_with(from.as_str()) {
            return format!("{}{}", to, &url[from.len()..]);
        }
    }
    url.to_string()
}
