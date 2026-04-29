use crate::url_override::maybe_rewrite;
use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use reqwest::{header::RETRY_AFTER, StatusCode};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const HTTP_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; auth2wd/0.1; +https://github.com/magnusmanske/auth2wd)";

const MIN_HOST_INTERVAL: Duration = Duration::from_millis(200);
const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(8);

lazy_static! {
    /// Stores the time at which the next request to a given host:port may be issued.
    /// Acts as both a throttle (concurrent callers reserve sequential slots) and a
    /// way to spread bursts across hosts that rate-limit aggressively (e.g. DNB).
    static ref HOST_NEXT_SLOT: Mutex<HashMap<String, Instant>> = Mutex::new(HashMap::new());
}

#[derive(Copy, Clone, Debug)]
pub struct Utility {}

impl Utility {
    pub fn get_reqwest_client() -> Result<reqwest::Client> {
        Ok(reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent(HTTP_USER_AGENT)
            .build()?)
    }

    pub async fn get_url(url: &str) -> Result<String> {
        let url = maybe_rewrite(url);
        let client = Self::get_reqwest_client()?;
        let host_key = Self::host_key(&url);

        let mut backoff = INITIAL_BACKOFF;
        for attempt in 0..=MAX_RETRIES {
            Self::throttle(&host_key).await;
            let response = client.get(&url).send().await?;
            let status = response.status();

            if status.is_success() {
                return Ok(response.text().await?);
            }

            let retryable = matches!(
                status,
                StatusCode::TOO_MANY_REQUESTS
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::BAD_GATEWAY
                    | StatusCode::GATEWAY_TIMEOUT
            );
            if !retryable || attempt == MAX_RETRIES {
                return Err(anyhow!("HTTP {} for {}", status, url));
            }

            // Honor Retry-After (seconds form) when present, else exponential backoff.
            let wait = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(Duration::from_secs)
                .unwrap_or(backoff);
            tokio::time::sleep(wait).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
        unreachable!("loop returns on every iteration")
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
