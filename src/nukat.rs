use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::url_override::maybe_rewrite;
use crate::utility::Utility;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use regex::Regex;
use serde_json::json;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;

#[derive(Debug)]
pub struct NUKAT {
    id: String,
    viaf_id: String,
    graph: FastGraph,
}

#[async_trait]
impl ExternalImporter for NUKAT {
    fn my_property(&self) -> usize {
        P_NUKAT
    }
    fn my_stated_in(&self) -> &str {
        "Q11789729"
    }
    fn primary_language(&self) -> String {
        String::from("pl")
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("http://viaf.org/viaf/{}", self.viaf_id)
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
        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl NUKAT {
    /// Converts a Wikidata-format NUKAT ID (e.g., "n96637319") to the
    /// VIAF-format NUKAT ID (e.g., "n  96637319") by inserting two spaces
    /// after the initial letter prefix.
    fn id_for_viaf(id: &str) -> String {
        lazy_static! {
            static ref RE_NUKAT_ID: Regex = Regex::new(r"^([a-z]+)(\d+)$").expect("Regexp error");
        }
        match RE_NUKAT_ID.captures(id) {
            Some(caps) => format!("{}  {}", &caps[1], &caps[2]),
            None => id.to_string(),
        }
    }

    pub async fn new(id: &str) -> Result<Self> {
        let url = maybe_rewrite("https://viaf.org/api/cluster-record");
        let client = Utility::get_reqwest_client()?;
        let viaf_id = Self::id_for_viaf(id);
        let record_id = format!("NUKAT|{viaf_id}");

        // First, look up the VIAF cluster ID using the NUKAT source ID
        let payload = json!({"reqValues":{"recordId":record_id,"isSourceId":true},"meta":{"pageIndex":0,"pageSize":1}});
        let response: serde_json::Value = client
            .post(&url)
            .json(&payload)
            .send()
            .await?
            .json()
            .await?;
        let viaf_cluster_id = response["queryResult"]["viafID"]
            .as_i64()
            .ok_or_else(|| anyhow!("No VIAF cluster ID found for NUKAT ID '{id}'"))?
            .to_string();

        // Then, fetch the RDF data for the VIAF cluster
        let rdf_payload = json!({"reqValues":{"recordId":viaf_cluster_id,"isSourceId":false,"acceptFiletype":"rdf+xml"},"meta":{"pageIndex":0,"pageSize":1}});
        let rdf_response = client
            .post(&url)
            .json(&rdf_payload)
            .send()
            .await?
            .text()
            .await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = xml::parser::parse_str(&rdf_response).add_to_graph(&mut graph)?;

        Ok(Self {
            id: id.to_string(),
            viaf_id: viaf_cluster_id,
            graph,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url_override;
    use serial_test::serial;
    use wikimisc::wikibase::EntityTrait;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_ID: &str = "n96637319";

    async fn mock_nukat() -> (MockServer, NUKAT) {
        let server = MockServer::start().await;

        // First POST: NUKAT source-ID lookup (isSourceId: true) → returns VIAF cluster ID
        let lookup_fixture = include_str!("../test_data/fixtures/viaf_lookup_nukat_n96637319.json");
        // Second POST: fetch RDF for that VIAF cluster (isSourceId: false)
        let rdf_fixture = include_str!("../test_data/fixtures/viaf_98777888.rdf");

        // Distinguish the two POSTs by matching on the `isSourceId` field in the JSON body.
        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .and(body_partial_json(
                serde_json::json!({"reqValues":{"isSourceId":true}}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(lookup_fixture))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .and(body_partial_json(
                serde_json::json!({"reqValues":{"isSourceId":false}}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(rdf_fixture))
            .mount(&server)
            .await;

        url_override::register("https://viaf.org", server.uri());

        let nukat = NUKAT::new(TEST_ID).await.unwrap();
        (server, nukat)
    }

    fn cleanup() {
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    #[serial]
    async fn test_new() {
        let (_server, _nukat) = mock_nukat().await;
        cleanup();
    }

    #[tokio::test]
    #[serial]
    async fn test_my_property() {
        let (_server, nukat) = mock_nukat().await;
        assert_eq!(nukat.my_property(), P_NUKAT);
        cleanup();
    }

    #[tokio::test]
    #[serial]
    async fn test_my_stated_in() {
        let (_server, nukat) = mock_nukat().await;
        assert_eq!(nukat.my_stated_in(), "Q11789729");
        cleanup();
    }

    #[tokio::test]
    #[serial]
    async fn test_primary_language() {
        let (_server, nukat) = mock_nukat().await;
        assert_eq!(nukat.primary_language(), "pl");
        cleanup();
    }

    #[tokio::test]
    #[serial]
    async fn test_my_id() {
        let (_server, nukat) = mock_nukat().await;
        assert_eq!(nukat.my_id(), TEST_ID);
        cleanup();
    }

    #[tokio::test]
    async fn test_id_for_viaf() {
        assert_eq!(NUKAT::id_for_viaf("n96637319"), "n  96637319");
        assert_eq!(NUKAT::id_for_viaf("nx1234567890"), "nx  1234567890");
        assert_eq!(NUKAT::id_for_viaf("already spaced"), "already spaced");
    }

    #[tokio::test]
    #[serial]
    async fn test_run() {
        let (_server, nukat) = mock_nukat().await;
        let meta_item = nukat.run().await.unwrap();
        assert!(!meta_item.item.labels().is_empty());
        cleanup();
    }
}
