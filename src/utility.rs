use crate::url_override::maybe_rewrite;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use wikimisc::http::{self, HttpRetryPolicy};
use wikimisc::wikidata::Wikidata;

/// The user agent for requests to the authority-control sources. Some of them
/// (LoC, SELIBR, ULAN) have refused clients that do not look like a browser,
/// hence the `Mozilla/5.0 (compatible; …)` form.
const HTTP_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; auth2wd/0.1; +https://github.com/magnusmanske/auth2wd)";

/// The user agent for the Wikidata API, as Wikimedia's user-agent policy asks for it.
const WIKIDATA_USER_AGENT: &str = wikimisc::user_agent!("https://github.com/magnusmanske/auth2wd");

const WIKIDATA_API_URL: &str = "https://www.wikidata.org/w/api.php";

const MIN_HOST_INTERVAL: Duration = Duration::from_millis(200);
/// With 3 retries the backoff waits 1 s, 2 s and 4 s.
const MAX_RETRIES: u32 = 3;
/// The longest `Retry-After` honoured as asked; a longer one is waited out
/// for this long only (and the request tried again) rather than given up on.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

/// Stores the time at which the next request to a given host:port may be issued.
/// Acts as both a throttle (concurrent callers reserve sequential slots) and a
/// way to spread bursts across hosts that rate-limit aggressively (e.g. DNB).
static HOST_NEXT_SLOT: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Shared between calls, so that its HTTP client (and connection pool) is too.
static WIKIDATA: LazyLock<Wikidata> = LazyLock::new(|| Wikidata::new(WIKIDATA_USER_AGENT));

#[derive(Copy, Clone, Debug)]
pub struct Utility {}

impl Utility {
    pub fn get_reqwest_client() -> Result<reqwest::Client> {
        Ok(http::client_builder(HTTP_USER_AGENT)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?)
    }

    /// The Wikidata client, pointed at a test server if one is registered
    /// in [`crate::url_override`].
    pub fn wikidata() -> Wikidata {
        let mut wikidata = WIKIDATA.clone();
        let api_url = maybe_rewrite(WIKIDATA_API_URL);
        if api_url != WIKIDATA_API_URL {
            wikidata.set_api_url(&api_url);
        }
        wikidata
    }

    /// GETs `url` and returns the body. Rate limiting, server errors and
    /// broken connections are retried up to [`MAX_RETRIES`] times, each
    /// attempt waiting for its slot with the host. A `Retry-After` is
    /// honoured up to [`MAX_RETRY_AFTER`]; a longer one is clamped to it.
    pub async fn get_url(url: &str) -> Result<String> {
        let url = maybe_rewrite(url);
        let client = Self::get_reqwest_client()?;
        let host_key = Self::host_key(&url);
        let policy = HttpRetryPolicy::new(MAX_RETRIES)
            .with_max_wait(MAX_RETRY_AFTER)
            .clamp_retry_after();
        let body = policy
            .run(&url, || async {
                Self::throttle(&host_key).await;
                // A single attempt, body included; the retries are `policy`'s.
                Ok(http::send(client.get(&url)).await?.text().await?)
            })
            .await?;
        Ok(body)
    }

    /// Stable key for throttling. Falls back to the full URL when parsing fails so
    /// malformed URLs are still throttled (just not grouped by host).
    fn host_key(url: &str) -> String {
        match reqwest::Url::parse(url) {
            Ok(u) => match (u.host_str(), u.port_or_known_default()) {
                (Some(host), Some(port)) => format!("{host}:{port}"),
                (Some(host), None) => host.to_string(),
                _ => url.to_string(),
            },
            Err(_) => url.to_string(),
        }
    }

    /// Reserve the next outbound slot for `host_key` and sleep until it is due.
    /// Concurrent callers stagger automatically because each reservation pushes
    /// the host's "next slot" forward by `MIN_HOST_INTERVAL`.
    async fn throttle(host_key: &str) {
        let wait = {
            let mut map = HOST_NEXT_SLOT.lock().expect("throttle map poisoned");
            let now = Instant::now();
            let next = map.get(host_key).copied().unwrap_or(now).max(now);
            map.insert(host_key.to_string(), next + MIN_HOST_INTERVAL);
            next.saturating_duration_since(now)
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn get_url_retries_a_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(1)
            .mount(&server)
            .await;
        let body = Utility::get_url(&format!("{}/retried", server.uri()))
            .await
            .unwrap();
        assert_eq!(body, "ok");
    }

    #[tokio::test]
    async fn get_url_does_not_retry_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        let result = Utility::get_url(&format!("{}/missing", server.uri())).await;
        assert!(result.is_err());
    }
}
