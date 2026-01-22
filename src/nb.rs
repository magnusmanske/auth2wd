use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::utility::Utility;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use serde::{Deserialize, Serialize};
use sophia::inmem::graph::FastGraph;
use wikimisc::wikibase::EntityTrait;
use wikimisc::wikibase::LocaleString;

#[derive(Serialize, Deserialize, Debug, Clone)]
enum TermType {
    BlankNode,
    NamedNode,
    Literal,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TripleEntry {
    #[serde(rename = "termType")]
    term_type: TermType,
    value: String,
    datatype: Option<String>,
    language: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Triple {
    #[serde(rename = "0")]
    s: TripleEntry,
    #[serde(rename = "1")]
    p: TripleEntry,
    #[serde(rename = "2")]
    o: TripleEntry,
}

impl Triple {
    fn is_named_node(&self, key: &str) -> bool {
        match self.p.term_type {
            TermType::NamedNode => self.p.value == key,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct NB {
    id: String,
    graph: FastGraph,
    data: Vec<Triple>,
}

unsafe impl Send for NB {}
unsafe impl Sync for NB {}

#[async_trait]
impl ExternalImporter for NB {
    fn my_property(&self) -> usize {
        1006
    }

    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    fn my_stated_in(&self) -> &str {
        "Q105488572"
    }

    fn graph(&self) -> &FastGraph {
        &self.graph
    }

    fn primary_language(&self) -> String {
        "nl".to_string()
    }

    fn get_key_url(&self, _key: &str) -> String {
        format!("http://data.bibliotheken.nl/id/thes/p{}", self.id)
    }

    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }

    async fn run(&self) -> Result<MetaItem> {
        let own_url = format!("http://data.bibliotheken.nl/id/thes/p{}", self.id);
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret).await?;

        for triple in &self.data {
            if triple.s.value != own_url {
                // Only gather data about the subject itself
                continue;
            }
            let language = triple.o.language.to_owned().unwrap_or("nl".to_string());
            if triple.is_named_node("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
                && triple.o.value == "http://schema.org/Person"
            {
                ret.add_claim(self.new_statement_item(31, "Q5"));
            }
            if triple.is_named_node("http://schema.org/alternateName") {
                ret.item
                    .aliases_mut()
                    .push(LocaleString::new(&language, &triple.o.value));
            }
            if triple.is_named_node("http://schema.org/name") {
                ret.item
                    .labels_mut()
                    .push(LocaleString::new(&language, &triple.o.value));
            }
            if triple.is_named_node("http://schema.org/description") {
                ret.item
                    .descriptions_mut()
                    .push(LocaleString::new(&language, &triple.o.value));
            }
            if triple.is_named_node("http://schema.org/nationality") {
                ret.add_prop_text(ExternalId::new(27, &triple.o.value));
            }
            if triple.is_named_node("http://schema.org/birthDate") {
                match ret.parse_date(&triple.o.value) {
                    Some((time, precision)) => {
                        ret.add_claim(self.new_statement_time(569, &time, precision))
                    }
                    None => ret.add_prop_text(ExternalId::new(569, &triple.o.value)),
                };
            }
            if triple.is_named_node("http://schema.org/deathDate") {
                match ret.parse_date(&triple.o.value) {
                    Some((time, precision)) => {
                        ret.add_claim(self.new_statement_time(570, &time, precision))
                    }
                    None => ret.add_prop_text(ExternalId::new(570, &triple.o.value)),
                };
            }
            if triple.is_named_node("http://schema.org/sameAs")
                || triple.is_named_node("http://www.w3.org/2002/07/owl#sameAs")
            {
                if let Some(ext_id) = self.url2external_id(&triple.o.value) {
                    ret.add_claim(self.new_statement_string(ext_id.property(), ext_id.id()));
                }
            }
        }

        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl NB {
    pub async fn new(id: &str) -> Result<Self> {
        let url = format!("http://data.bibliotheken.nl/id/thes/p{id}");
        let client = Utility::get_reqwest_client()?;
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        let data: Vec<Triple> = client
            .get(url)
            .headers(headers)
            .send()
            .await?
            .json()
            .await?;

        Ok(Self {
            id: id.to_string(),
            graph: FastGraph::new(),
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use wikimisc::wikibase::{EntityTrait, LocaleString};

    use super::*;

    const TEST_ID: &str = "068364229";

    #[tokio::test]
    async fn test_new() {
        assert!(NB::new(TEST_ID).await.is_ok());
    }

    #[tokio::test]
    async fn test_my_property() {
        let nb = NB::new(TEST_ID).await.unwrap();
        assert_eq!(nb.my_property(), 1006);
    }

    #[tokio::test]
    async fn test_my_stated_in() {
        let nb = NB::new(TEST_ID).await.unwrap();
        assert_eq!(nb.my_stated_in(), "Q105488572");
    }

    #[tokio::test]
    async fn test_primary_language() {
        let nb = NB::new(TEST_ID).await.unwrap();
        assert_eq!(nb.primary_language(), "nl");
    }

    #[tokio::test]
    async fn test_get_key_url() {
        let nb = NB::new(TEST_ID).await.unwrap();
        assert_eq!(
            nb.get_key_url(TEST_ID),
            "http://data.bibliotheken.nl/id/thes/p068364229"
        );
    }

    #[tokio::test]
    async fn test_run() {
        let nb = NB::new(TEST_ID).await.unwrap();
        let meta_item = nb.run().await.unwrap();
        assert_eq!(
            *meta_item.item.labels(),
            vec![LocaleString::new("nl", "Charles Robert Darwin")]
        );
    }
}
