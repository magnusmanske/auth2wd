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

lazy_static! {
    static ref KEY2PROP: HashMap<String, usize> = {
        let mut ret = HashMap::new();
        ret.insert(String::from("DNB"), 227);
        ret.insert(String::from("PLWABN"), 7293);
        ret.insert(String::from("BIBSYS"), 1015);
        ret.insert(String::from("ICCU"), 396);
        ret.insert(String::from("DBC"), 3846);
        ret.insert(String::from("FAST"), 2163);
        ret.insert(String::from("VLACC"), 7024);
        ret.insert(String::from("ISNI"), 213);
        ret.insert(String::from("DE633"), 5504);
        ret.insert(String::from("LNL"), 7026);
        ret.insert(String::from("CAOONL"), 8179);
        ret.insert(String::from("EGAXA"), 1309);
        ret.insert(String::from("LC"), 244);
        // ret.insert(String::from("NII"), XXXX);
        ret.insert(String::from("SIMACOB"), 1280);
        ret.insert(String::from("NUKAT"), 1207);
        ret.insert(String::from("CYT"), 1048);
        ret.insert(String::from("NDL"), 349);
        // ret.insert(String::from("NLB"), XXXX);
        // ret.insert(String::from("B2Q"), XXXX);
        ret.insert(String::from("ARBABN"), 3788);
        // ret.insert(String::from("NLA"), XXXX);
        ret.insert(String::from("BLBNB"), 4619);
        ret.insert(String::from("BNC"), 9984);
        ret.insert(String::from("BNCHL"), 7369);
        ret.insert(String::from("ERRR"), 6394);
        // ret.insert(String::from("BNF"), 268); // Deactivated for now; eg Q136170149 / 6471159248261404870006 gives truncated ID
        ret.insert(String::from("GRATEVE"), 3348);
        ret.insert(String::from("N6I"), 10227);
        ret.insert(String::from("NLI"), 949);
        ret.insert(String::from("KRNLK"), 5034);
        ret.insert(String::from("LNB"), 1368);
        // ret.insert(String::from("LIH"), 7699); // Something is wrong there
        ret.insert(String::from("BNL"), 7028);
        ret.insert(String::from("MRBNR"), 7058);
        ret.insert(String::from("W2Z"), 1015);
        ret.insert(String::from("PTBNP"), 1005);
        ret.insert(String::from("NLR"), 7029);
        // ret.insert(String::from("BNE"), XXXX);
        ret.insert(String::from("SELIBR"), 906);
        ret.insert(String::from("NKC"), 691);
        // ret.insert(String::from("NTA"), XXXX);
        // ret.insert(String::from("NSZL"), XXXX);
        ret.insert(String::from("NSK"), 1375);
        ret.insert(String::from("UIY"), 7039);
        // ret.insert(String::from("PERSEUS"), XXXX);
        ret.insert(String::from("RERO"), 3065);
        ret.insert(String::from("NYNYRILM"), 9171);
        ret.insert(String::from("SKMASNL"), 7700);
        ret.insert(String::from("SUDOC"), 269);
        // ret.insert(String::from("SZ"), XXXX);
        ret.insert(String::from("SRP"), 6934);
        // ret.insert(String::from("JPG"), XXXX);
        // ret.insert(String::from("UAE"), XXXX);
        ret.insert(String::from("BAV"), 8034);
        // ret.insert(String::from("WKP"), XXXX); // Maybe not?
        ret
    };
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

    // Takes a numeric Wikidata property ID and returns the corresponding VIAF key, if available
    pub fn prop2key(property: usize) -> Option<String> {
        KEY2PROP
            .iter()
            .find(|&(_, v)| *v == property)
            .map(|(k, _)| k.clone())
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

    const TEST_ID: &str = "30701597";

    #[tokio::test]
    async fn test_new() {
        assert!(VIAF::new(TEST_ID).await.is_ok());
    }

    #[tokio::test]
    async fn test_my_property() {
        let viaf = VIAF::new(TEST_ID).await.unwrap();
        assert_eq!(viaf.my_property(), P_VIAF);
    }

    #[tokio::test]
    async fn test_my_stated_in() {
        let viaf = VIAF::new(TEST_ID).await.unwrap();
        assert_eq!(viaf.my_stated_in(), "Q54919");
    }

    #[tokio::test]
    async fn test_primary_language() {
        let viaf = VIAF::new(TEST_ID).await.unwrap();
        assert_eq!(viaf.primary_language(), "en");
    }

    #[tokio::test]
    async fn test_get_key_url() {
        let viaf = VIAF::new(TEST_ID).await.unwrap();
        assert_eq!(viaf.get_key_url(TEST_ID), "http://viaf.org/viaf/30701597");
    }

    #[tokio::test]
    async fn test_my_id() {
        let viaf = VIAF::new(TEST_ID).await.unwrap();
        assert_eq!(viaf.my_id(), TEST_ID);
    }

    #[tokio::test]
    async fn test_transform_label() {
        let viaf = VIAF::new(TEST_ID).await.unwrap();
        assert_eq!(viaf.transform_label("Manske, Magnus"), "Magnus Manske");
        assert_eq!(viaf.transform_label("Manske,Magnus"), "Manske,Magnus");
        assert_eq!(viaf.transform_label("Magnus Manske"), "Magnus Manske");
    }

    #[tokio::test]
    async fn test_run() {
        let viaf = VIAF::new(TEST_ID).await.unwrap();
        let meta_item = viaf.run().await.unwrap();
        assert_eq!(
            *meta_item.item.labels(),
            vec![LocaleString::new("en", "Magnus Manske")]
        );
    }
}
