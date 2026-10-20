//! auth2wd's additions to [`wikimisc::external_id::ExternalId`]: cached
//! Wikidata lookups, ID validity checks and the URLs not to take IDs from.

use crate::utility::Utility;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;
pub use wikimisc::external_id::ExternalId;

static EXTERNAL_IDS_OK_CACHE: LazyLock<Mutex<HashMap<ExternalId, bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// In-process cache for [`search_wikidata_single_item`], keyed by raw
/// `srsearch` query string. Both hits (`Some(QID)`) and misses (`None`)
/// are cached so the same Wikidata search isn't repeated within one run.
/// Tests that exercise multiple mock responses for the same query can
/// call [`clear_wikidata_search_cache`] between cases.
static WIKIDATA_SEARCH_CACHE: LazyLock<Mutex<HashMap<String, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Both caches above are unbounded, so their sizes go to the service dashboard.
fn record_cache_size(cache: &'static str, entries: usize) {
    wikimisc::metrics::gauge!("auth2wd_cache_entries", "cache" => cache).set(entries as f64);
}

/// The one Wikidata item matching the CirrusSearch `query` (e.g.
/// `haswbstatement:P31=Q5`), or `None` if there is none or more than one.
pub async fn search_wikidata_single_item(query: &str) -> Option<String> {
    if let Some(cached) = WIKIDATA_SEARCH_CACHE.lock().await.get(query) {
        return cached.clone();
    }
    // A failed request is not cached: it is treated as transient so a later
    // call can retry. Only well-formed responses populate the cache.
    let result = Utility::wikidata()
        .search_single_item(query)
        .await
        .ok()?
        .map(|item| item.to_string());
    let mut cache = WIKIDATA_SEARCH_CACHE.lock().await;
    cache.insert(query.to_string(), result.clone());
    record_cache_size("wikidata_search", cache.len());
    result
}

/// Clears the in-process Wikidata search cache. Intended for tests that
/// exercise multiple distinct mock responses for the same query string.
pub async fn clear_wikidata_search_cache() {
    WIKIDATA_SEARCH_CACHE.lock().await.clear();
    record_cache_size("wikidata_search", 0);
}

pub fn do_not_use_external_url(url: &str) -> bool {
    crate::DO_NOT_USE_EXTERNAL_URL_REGEXPS
        .iter()
        .any(|re| re.is_match(url))
}

/// Lookups and checks of an [`ExternalId`] (bring into scope to use).
#[async_trait]
pub trait ExternalIdExt {
    /// The one Wikidata item with this identifier, if there is exactly one.
    async fn wikidata_item(&self) -> Option<String>;

    /// The one Wikidata item with this identifier and `text` (e.g. a label),
    /// if there is exactly one.
    async fn wikidata_item_with_text(&self, text: &str) -> Option<String>;

    /// Checks some properties (eg GND) if the external ID is valid (eg not deprecated)
    async fn check_if_valid(&self) -> Result<bool>;
}

#[async_trait]
impl ExternalIdExt for ExternalId {
    async fn wikidata_item(&self) -> Option<String> {
        self.wikidata_item_with_text("").await
    }

    async fn wikidata_item_with_text(&self, text: &str) -> Option<String> {
        search_wikidata_single_item(&self.search_query(text).to_string()).await
    }

    async fn check_if_valid(&self) -> Result<bool> {
        if let Some(is_ok) = EXTERNAL_IDS_OK_CACHE.lock().await.get(self) {
            return Ok(*is_ok);
        }
        let mut ret = true;
        let mut was_checked = false;
        if self.property() == 227 {
            // GND
            was_checked = true;
            let url = format!("https://d-nb.info/gnd/{}/about/lds.rdf", self.id());
            let text = Utility::get_url(&url).await?;
            let check = format!("rdf:about=\"https://d-nb.info/gnd/{}\">", self.id());
            ret = text.contains(&check);
        }
        if was_checked {
            // No need to store the result if no check was run
            let mut cache = EXTERNAL_IDS_OK_CACHE.lock().await;
            cache.insert(self.clone(), ret);
            record_cache_size("external_ids_ok", cache.len());
        }
        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url_override;
    use serial_test::serial;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Answers the Wikidata search for `srsearch` (form-encoded) with `body`.
    async fn mock_search(server: &MockServer, srsearch: &str, body: &str) {
        Mock::given(method("POST"))
            .and(path("/w/api.php"))
            .and(body_string_contains(format!("srsearch={srsearch}&")))
            .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
            .mount(server)
            .await;
    }

    #[tokio::test]
    #[serial]
    async fn test_get_item_for_external_id() {
        clear_wikidata_search_cache().await;
        let server = MockServer::start().await;

        let single_result = r#"{"batchcomplete":"","query":{"searchinfo":{"totalhits":1},"search":[{"ns":0,"title":"Q13520818","pageid":15554972,"size":42000,"wordcount":0,"snippet":"","timestamp":"2024-01-01T00:00:00Z"}]}}"#;
        let no_results =
            r#"{"batchcomplete":"","query":{"searchinfo":{"totalhits":0},"search":[]}}"#;

        // Match: exact ID search  haswbstatement:"P214=30701597"
        mock_search(
            &server,
            "haswbstatement%3A%22P214%3D30701597%22",
            single_result,
        )
        .await;

        // Match: string + ID search  Magnus haswbstatement:"P214=30701597"
        mock_search(
            &server,
            "Magnus+haswbstatement%3A%22P214%3D30701597%22",
            single_result,
        )
        .await;

        // Match: nonsense string + ID search → no results
        mock_search(
            &server,
            "ocshs87gvdsu6gsdi7vchkuchs+haswbstatement%3A%22P214%3D30701597%22",
            no_results,
        )
        .await;

        // Match: bogus ID search → no results
        mock_search(
            &server,
            "haswbstatement%3A%22P214%3D3070159777777%22",
            no_results,
        )
        .await;

        url_override::register("https://www.wikidata.org", server.uri());

        // Test OK — exact ID lookup
        let ext_id1 = ExternalId::new(214, "30701597");
        assert_eq!(ext_id1.wikidata_item().await, Some("Q13520818".to_string()));

        // Test OK — string + ID lookup
        assert_eq!(
            ext_id1.wikidata_item_with_text("Magnus").await,
            Some("Q13520818".to_string())
        );

        // Test wrong string
        assert_eq!(
            ext_id1
                .wikidata_item_with_text("ocshs87gvdsu6gsdi7vchkuchs")
                .await,
            None
        );

        // Test wrong ID
        let ext_id2 = ExternalId::new(214, "3070159777777");
        assert_eq!(ext_id2.wikidata_item().await, None);

        url_override::unregister("https://www.wikidata.org");
        clear_wikidata_search_cache().await;
    }

    /// Once a query has been resolved, repeating it does not hit the network.
    /// Verified by tearing the mock server down between the two calls — the
    /// second call must still return the same value.
    #[tokio::test]
    #[serial]
    async fn test_search_wikidata_single_item_caches_results() {
        clear_wikidata_search_cache().await;

        // ── First call: live mock returns a single hit ─────────────────────
        {
            let server = MockServer::start().await;
            let single_result = r#"{"batchcomplete":"","query":{"searchinfo":{"totalhits":1},"search":[{"ns":0,"title":"Q42","pageid":1,"size":0,"wordcount":0,"snippet":"","timestamp":"2026-01-01T00:00:00Z"}]}}"#;
            mock_search(
                &server,
                "haswbstatement%3A%22P227%3Dcache-test-id%22",
                single_result,
            )
            .await;
            url_override::register("https://www.wikidata.org", server.uri());

            let ext_id = ExternalId::new(227, "cache-test-id");
            assert_eq!(ext_id.wikidata_item().await, Some("Q42".to_string()));

            url_override::unregister("https://www.wikidata.org");
        }

        // ── Second call: a fresh mock with no stub. A cache miss would 404
        //    and `search_wikidata_single_item` would return `None`. The
        //    cached `Some("Q42")` must win. ─────────────────────────────────
        {
            let server = MockServer::start().await;
            url_override::register("https://www.wikidata.org", server.uri());

            let ext_id = ExternalId::new(227, "cache-test-id");
            assert_eq!(ext_id.wikidata_item().await, Some("Q42".to_string()));

            url_override::unregister("https://www.wikidata.org");
        }

        clear_wikidata_search_cache().await;
    }
}
