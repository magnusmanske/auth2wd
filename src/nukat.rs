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
    use wikimisc::wikibase::EntityTrait;

    const TEST_ID: &str = "n96637319";

    #[tokio::test]
    async fn test_new() {
        assert!(NUKAT::new(TEST_ID).await.is_ok());
    }

    #[tokio::test]
    async fn test_my_property() {
        let nukat = NUKAT::new(TEST_ID).await.unwrap();
        assert_eq!(nukat.my_property(), P_NUKAT);
    }

    #[tokio::test]
    async fn test_my_stated_in() {
        let nukat = NUKAT::new(TEST_ID).await.unwrap();
        assert_eq!(nukat.my_stated_in(), "Q11789729");
    }

    #[tokio::test]
    async fn test_primary_language() {
        let nukat = NUKAT::new(TEST_ID).await.unwrap();
        assert_eq!(nukat.primary_language(), "pl");
    }

    #[tokio::test]
    async fn test_my_id() {
        let nukat = NUKAT::new(TEST_ID).await.unwrap();
        assert_eq!(nukat.my_id(), TEST_ID);
    }

    #[tokio::test]
    async fn test_id_for_viaf() {
        assert_eq!(NUKAT::id_for_viaf("n96637319"), "n  96637319");
        assert_eq!(NUKAT::id_for_viaf("nx1234567890"), "nx  1234567890");
        assert_eq!(NUKAT::id_for_viaf("already spaced"), "already spaced");
    }

    #[tokio::test]
    async fn test_run() {
        let nukat = NUKAT::new(TEST_ID).await.unwrap();
        let meta_item = nukat.run().await.unwrap();
        assert!(!meta_item.item.labels().is_empty());
    }
}
