use crate::external_id::ExternalId;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::url_override::maybe_rewrite;
use crate::utility::Utility;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use serde_json::json;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;
use std::collections::HashMap;
use tokio::sync::Mutex as AsyncMutex;

lazy_static! {
    /// In-process cache for VIAF source-ID lookups, keyed by `(property, id)`.
    /// Both successful matches (`Some(viaf_id)`) and definite "no match" results
    /// (`None`) are cached so that the same `(property, id)` is never queried
    /// twice in one process. Tests that exercise multiple mock responses for
    /// the same key can call [`VIAF::clear_lookup_cache`] between cases.
    static ref VIAF_LOOKUP_CACHE: AsyncMutex<HashMap<(usize, String), Option<String>>> =
        AsyncMutex::new(HashMap::new());

    static ref KEY2PROP: HashMap<&'static str, usize> = HashMap::from([
        ("DNB",     227),
        ("PLWABN",  7293),
        ("BIBSYS",  1015),
        ("ICCU",    396),
        ("DBC",     3846),
        ("FAST",    2163),
        ("VLACC",   7024),
        ("ISNI",    213),
        ("DE633",   5504),
        ("LNL",     7026),
        ("CAOONL",  8179),
        ("EGAXA",   1309),
        ("LC",      244),
        // ("NII",  XXXX),
        ("SIMACOB", 1280),
        ("NUKAT",   1207),
        ("CYT",     1048),
        ("NDL",     349),
        // ("NLB",  XXXX),
        // ("B2Q",  XXXX),
        ("ARBABN",  3788),
        // ("NLA",  XXXX),
        ("BLBNB",   4619),
        ("BNC",     9984),
        ("BNCHL",   7369),
        ("ERRR",    6394),
        // ("BNF",  268), // Deactivated; eg Q136170149 gives truncated ID
        ("GRATEVE", 3348),
        ("N6I",     10227),
        ("NLI",     949),
        ("KRNLK",   5034),
        ("LNB",     1368),
        // ("LIH",  7699), // Something is wrong there
        ("BNL",     7028),
        ("MRBNR",   7058),
        ("W2Z",     1015),
        ("PTBNP",   1005),
        ("NLR",     7029),
        // ("BNE",  XXXX),
        ("SELIBR",  906),
        ("NKC",     691),
        // ("NTA",  XXXX),
        // ("NSZL", XXXX),
        ("NSK",     1375),
        ("UIY",     7039),
        // ("PERSEUS", XXXX),
        ("RERO",    3065),
        ("NYNYRILM",9171),
        ("SKMASNL", 7700),
        ("SUDOC",   269),
        // ("SZ",   XXXX),
        ("SRP",     6934),
        ("JPG",     P_ULAN),
        // ("UAE",  XXXX),
        ("BAV",     8034),
        // ("WKP",  XXXX), // Maybe not?
    ]);
}

#[derive(Clone, Debug)]
pub struct VIAF {
    id: String,
    graph: FastGraph,
}

#[async_trait]
impl ExternalImporter for VIAF {
    fn my_property(&self) -> usize {
        P_VIAF
    }
    fn my_stated_in(&self) -> &str {
        "Q54919"
    }
    fn primary_language(&self) -> String {
        String::from("en")
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("http://viaf.org/viaf/{}", self.id)
    }

    fn my_id(&self) -> String {
        self.id.clone()
    }
    fn graph(&self) -> &FastGraph {
        &self.graph
    }
    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret).await?;
        self.external_ids(&mut ret)?;
        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl VIAF {
    pub async fn new(id: &str) -> Result<Self> {
        let url = maybe_rewrite("https://viaf.org/api/cluster-record");
        let client = Utility::get_reqwest_client()?;
        let payload = json!({"reqValues":{"recordId":id,"isSourceId":false,"acceptFiletype":"rdf+xml"},"meta":{"pageIndex":0,"pageSize":1}});
        let response = client
            .post(&url)
            .json(&payload)
            .send()
            .await?
            .text()
            .await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = xml::parser::parse_str(&response).add_to_graph(&mut graph)?;
        Ok(Self {
            id: String::from(id),
            graph,
        })
    }

    /// Returns the VIAF source key for a Wikidata property ID, if one is mapped.
    pub fn prop2key(property: usize) -> Option<&'static str> {
        KEY2PROP
            .iter()
            .find(|&(_, v)| *v == property)
            .map(|(k, _)| *k)
    }

    /// Queries VIAF's `cluster-record` endpoint for the cluster matching the
    /// given `(property, id)` source identifier and returns the VIAF ID if
    /// one is found.
    ///
    /// VIAF's `cluster-record` endpoint resolves a source ID to at most one
    /// cluster, so a successful response is by construction a single,
    /// unambiguous match — no manual deduplication is required.
    ///
    /// Returns `None` when:
    /// - the property has no known VIAF source key (see `KEY2PROP`),
    /// - VIAF has no cluster for this source ID,
    /// - the request fails for any reason. Lookup failures are intentionally
    ///   swallowed so that an unreachable VIAF cannot cascade into a parser
    ///   failure for the underlying source importer.
    ///
    /// Results are cached in-process; see [`Self::clear_lookup_cache`].
    pub async fn infer_viaf_id_for(property: usize, id: &str) -> Option<String> {
        let key = Self::prop2key(property)?;
        let cache_key = (property, id.to_string());
        if let Some(cached) = VIAF_LOOKUP_CACHE.lock().await.get(&cache_key) {
            return cached.clone();
        }
        let result = Self::query_cluster_record(key, id).await;
        VIAF_LOOKUP_CACHE
            .lock()
            .await
            .insert(cache_key, result.clone());
        result
    }

    async fn query_cluster_record(source_key: &str, id: &str) -> Option<String> {
        let url = maybe_rewrite("https://viaf.org/api/cluster-record");
        let payload = json!({
            "reqValues": {
                "recordId": format!("{source_key}|{id}"),
                "isSourceId": true,
            },
            "meta": { "pageIndex": 0, "pageSize": 1 },
        });
        let client = Utility::get_reqwest_client().ok()?;
        let response = client.post(&url).json(&payload).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        let json: serde_json::Value = response.json().await.ok()?;
        json["queryResult"]["viafID"]
            .as_i64()
            .map(|v| v.to_string())
    }

    /// Clears the in-process VIAF lookup cache. Intended for tests that
    /// exercise multiple distinct mock responses for the same `(property, id)`.
    pub async fn clear_lookup_cache() {
        VIAF_LOOKUP_CACHE.lock().await.clear();
    }

    fn external_ids(&self, ret: &mut MetaItem) -> Result<()> {
        lazy_static! {
            static ref RE_EXT_ID: Regex =
                Regex::new(r"^http://viaf.org/viaf/sourceID/(.+?)%7C(.+?)#skos:Concept$").unwrap();
        }
        let triples = self
            .triples_property_object_iris("http://xmlns.com/foaf/0.1/focus", &self.get_id_url())?;
        for url in triples {
            if let Some(captures) = RE_EXT_ID.captures(&url) {
                if let (Some(source_id), Some(concept_id)) = (captures.get(1), captures.get(2)) {
                    if let Some(prop_id) = KEY2PROP.get(source_id.as_str()) {
                        let extid = ExternalId::new(*prop_id, concept_id.as_str());
                        ret.add_claim(self.new_statement_string(extid.property(), extid.id()));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use wikimisc::wikibase::{EntityTrait, LocaleString};

    use super::*;
    use crate::url_override;
    use serial_test::serial;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_ID: &str = "30701597";

    async fn mock_viaf() -> (MockServer, VIAF) {
        let server = MockServer::start().await;
        let fixture = include_str!("../test_data/fixtures/viaf_30701597.rdf");

        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&server)
            .await;

        url_override::register("https://viaf.org", server.uri());

        let viaf = VIAF::new(TEST_ID).await.unwrap();
        (server, viaf)
    }

    #[tokio::test]
    #[serial]
    async fn test_new() {
        let (_server, _viaf) = mock_viaf().await;
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    #[serial]
    async fn test_my_property() {
        let (_server, viaf) = mock_viaf().await;
        assert_eq!(viaf.my_property(), P_VIAF);
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    #[serial]
    async fn test_my_stated_in() {
        let (_server, viaf) = mock_viaf().await;
        assert_eq!(viaf.my_stated_in(), "Q54919");
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    #[serial]
    async fn test_primary_language() {
        let (_server, viaf) = mock_viaf().await;
        assert_eq!(viaf.primary_language(), "en");
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    #[serial]
    async fn test_get_key_url() {
        let (_server, viaf) = mock_viaf().await;
        assert_eq!(viaf.get_key_url(TEST_ID), "http://viaf.org/viaf/30701597");
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    #[serial]
    async fn test_my_id() {
        let (_server, viaf) = mock_viaf().await;
        assert_eq!(viaf.my_id(), TEST_ID);
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    #[serial]
    async fn test_transform_label() {
        let (_server, viaf) = mock_viaf().await;
        assert_eq!(viaf.transform_label("Manske, Magnus"), "Magnus Manske");
        assert_eq!(viaf.transform_label("Manske,Magnus"), "Manske,Magnus");
        assert_eq!(viaf.transform_label("Magnus Manske"), "Magnus Manske");
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    #[serial]
    async fn test_run() {
        let (_server, viaf) = mock_viaf().await;
        let meta_item = viaf.run().await.unwrap();
        assert_eq!(
            *meta_item.item.labels(),
            vec![LocaleString::new("en", "Magnus Manske")]
        );
        url_override::unregister("https://viaf.org");
    }

    /// Returns `None` for a property that has no entry in `KEY2PROP`,
    /// without making any HTTP call.
    #[tokio::test]
    #[serial]
    async fn test_infer_viaf_id_for_unmapped_property() {
        VIAF::clear_lookup_cache().await;
        // P_INATURALIST_TAXON is supported by the importer set but is not in
        // KEY2PROP — VIAF doesn't index iNaturalist taxa.
        assert!(VIAF::prop2key(P_INATURALIST_TAXON).is_none());
        assert_eq!(
            None,
            VIAF::infer_viaf_id_for(P_INATURALIST_TAXON, "12345").await
        );
    }

    /// End-to-end of the inference function against a mocked VIAF endpoint:
    /// a successful response yields `Some(viaf_id)`, an empty body yields
    /// `None`, and the result is cached so a second call does not re-hit
    /// the (now-removed) mock.
    #[tokio::test]
    #[serial]
    async fn test_infer_viaf_id_for_caches_results() {
        VIAF::clear_lookup_cache().await;

        // ── Hit ─────────────────────────────────────────────────────────────
        {
            let server = MockServer::start().await;
            let fixture =
                include_str!("../test_data/fixtures/viaf_lookup_jpg_500228559.json");
            Mock::given(method("POST"))
                .and(path("/api/cluster-record"))
                .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
                .mount(&server)
                .await;
            url_override::register("https://viaf.org", server.uri());

            let result = VIAF::infer_viaf_id_for(P_ULAN, "500228559").await;
            assert_eq!(result, Some("27063124".to_string()));

            url_override::unregister("https://viaf.org");
        }

        // ── Cached: a fresh server returns nothing, but the cached value
        //    from the previous call must still be returned. ─────────────────
        {
            let server = MockServer::start().await;
            // Deliberately register no mock — any HTTP call would 404.
            url_override::register("https://viaf.org", server.uri());

            let result = VIAF::infer_viaf_id_for(P_ULAN, "500228559").await;
            assert_eq!(result, Some("27063124".to_string()));

            url_override::unregister("https://viaf.org");
        }

        VIAF::clear_lookup_cache().await;
    }

    /// An empty `queryResult` (i.e. no `viafID` field) yields `None` and is
    /// cached as such — a subsequent call does not re-query VIAF.
    #[tokio::test]
    #[serial]
    async fn test_infer_viaf_id_for_no_match() {
        VIAF::clear_lookup_cache().await;

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;
        url_override::register("https://viaf.org", server.uri());

        // Use a clearly-unique id to keep this test independent of others.
        let result = VIAF::infer_viaf_id_for(P_GND, "test-no-match").await;
        assert_eq!(result, None);

        url_override::unregister("https://viaf.org");
        VIAF::clear_lookup_cache().await;
    }
}
