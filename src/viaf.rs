use std::collections::HashMap;

use crate::external_id::ExternalId;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::utility::Utility;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use serde_json::json;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;

lazy_static! {
    static ref KEY2PROP: HashMap<String, usize> = {
        let mut ret = HashMap::new();
        ret.insert("DNB".to_string(), 227);
        ret.insert("PLWABN".to_string(), 7293);
        ret.insert("BIBSYS".to_string(), 1015);
        ret.insert("ICCU".to_string(), 396);
        ret.insert("DBC".to_string(), 2753);
        ret.insert("FAST".to_string(), 2163);
        ret.insert("VLACC".to_string(), 7024);
        ret.insert("ISNI".to_string(), 213);
        ret.insert("DE633".to_string(), 5504);
        ret.insert("LNL".to_string(), 7026);
        ret.insert("CAOONL".to_string(), 8179);
        ret.insert("EGAXA".to_string(), 1309);
        ret.insert("LC".to_string(), 244);
        // ret.insert("NII".to_string(), XXXX);
        ret.insert("SIMACOB".to_string(), 1280);
        ret.insert("NUKAT".to_string(), 1207);
        ret.insert("CYT".to_string(), 1048);
        ret.insert("NDL".to_string(), 349);
        // ret.insert("NLB".to_string(), XXXX);
        // ret.insert("B2Q".to_string(), XXXX);
        ret.insert("ARBABN".to_string(), 3788);
        // ret.insert("NLA".to_string(), XXXX);
        ret.insert("BLBNB".to_string(), 4619);
        ret.insert("BNC".to_string(), 9984);
        ret.insert("BNCHL".to_string(), 7369);
        ret.insert("ERRR".to_string(), 6394);
        ret.insert("BNF".to_string(), 268);
        ret.insert("GRATEVE".to_string(), 3348);
        ret.insert("N6I".to_string(), 10227);
        ret.insert("NLI".to_string(), 949);
        ret.insert("KRNLK".to_string(), 5034);
        ret.insert("LNB".to_string(), 1368);
        ret.insert("LIH".to_string(), 7699);
        ret.insert("BNL".to_string(), 7028);
        ret.insert("MRBNR".to_string(), 7058);
        ret.insert("W2Z".to_string(), 1015);
        ret.insert("PTBNP".to_string(), 1005);
        ret.insert("NLR".to_string(), 7029);
        // ret.insert("BNE".to_string(), XXXX);
        ret.insert("SELIBR".to_string(), 906);
        ret.insert("NKC".to_string(), 691);
        // ret.insert("NTA".to_string(), XXXX);
        // ret.insert("NSZL".to_string(), XXXX);
        ret.insert("NSK".to_string(), 1375);
        ret.insert("UIY".to_string(), 7039);
        // ret.insert("PERSEUS".to_string(), XXXX);
        ret.insert("RERO".to_string(), 3065);
        ret.insert("NYNYRILM".to_string(), 9171);
        ret.insert("SKMASNL".to_string(), 7700);
        ret.insert("SUDOC".to_string(), 269);
        // ret.insert("SZ".to_string(), XXXX);
        ret.insert("SRP".to_string(), 6934);
        // ret.insert("JPG".to_string(), XXXX);
        // ret.insert("UAE".to_string(), XXXX);
        ret.insert("BAV".to_string(), 8034);
        // ret.insert("WKP".to_string(), XXXX); // Maybe not?
        ret
    };
}

#[derive(Clone, Debug)]
pub struct VIAF {
    id: String,
    graph: FastGraph,
}

unsafe impl Send for VIAF {}
unsafe impl Sync for VIAF {}

#[async_trait]
impl ExternalImporter for VIAF {
    fn my_property(&self) -> usize {
        214
    }
    fn my_stated_in(&self) -> &str {
        "Q54919"
    }
    fn primary_language(&self) -> String {
        "en".to_string()
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("http://viaf.org/viaf/{}", self.id)
    }

    fn my_id(&self) -> String {
        self.id.to_owned()
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
        let url = "https://viaf.org/api/cluster-record";
        let client = Utility::get_reqwest_client()?;
        let payload = json!({"reqValues":{"recordId":id,"isSourceId":false,"acceptFiletype":"rdf+xml"},"meta":{"pageIndex":0,"pageSize":1}});
        let response = client.post(url).json(&payload).send().await?.text().await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = xml::parser::parse_str(&response).add_to_graph(&mut graph)?;
        Ok(Self {
            id: id.to_string(),
            graph,
        })
    }

    // Takes a numeric Wikidata property ID and returns the corresponding VIAF key, if available
    pub fn prop2key(property: usize) -> Option<String> {
        KEY2PROP
            .iter()
            .find(|&(_, v)| *v == property)
            .map(|(k, _)| k.to_string())
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
                let source_id = captures.get(1).unwrap().as_str();
                let concept_id = captures.get(2).unwrap().as_str();
                if let Some(prop_id) = KEY2PROP.get(source_id) {
                    let extid = ExternalId::new(*prop_id, concept_id);
                    ret.add_claim(self.new_statement_string(extid.property(), extid.id()));
                }
            }
        }
        // let mut ids = Vec::new();
        // meta_item.add_external_ids(ids);
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
        assert_eq!(viaf.my_property(), 214);
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
