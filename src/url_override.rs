/// URL prefix override registry — used in tests to redirect external HTTP calls
/// to a local wiremock server.
///
/// In production the registry is always empty, so `maybe_rewrite` is a no-op.
/// The `register`, `unregister`, and `clear` entry points exist at runtime but
/// are intended for test use only; they are `#[doc(hidden)]` so they don't
/// appear in public docs.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static OVERRIDES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn overrides() -> &'static Mutex<HashMap<String, String>> {
    OVERRIDES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a URL prefix replacement: any URL that starts with `from` will
/// have that prefix replaced with `to`.  Test-only; no-op in production
/// because the map is never populated outside tests.
#[doc(hidden)]
pub fn register(from: impl Into<String>, to: impl Into<String>) {
    overrides()
        .lock()
        .expect("url_override mutex poisoned")
        .insert(from.into(), to.into());
}

/// Remove a single registered override by its `from` prefix.
#[doc(hidden)]
pub fn unregister(from: &str) {
    overrides()
        .lock()
        .expect("url_override mutex poisoned")
        .remove(from);
}

/// Remove all registered overrides.
#[doc(hidden)]
pub fn clear() {
    overrides()
        .lock()
        .expect("url_override mutex poisoned")
        .clear();
}

/// Rewrite `url` using any registered prefix overrides.
/// Returns the original URL unchanged when no override matches.
/// In production builds this is a no-op (the map is always empty).
pub fn maybe_rewrite(url: &str) -> String {
    let map = overrides().lock().expect("url_override mutex poisoned");
    for (from, to) in map.iter() {
        if url.starts_with(from.as_str()) {
            return format!("{}{}", to, &url[from.len()..]);
        }
    }
    url.to_string()
}
