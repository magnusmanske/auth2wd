use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use anyhow::Result;
use async_trait::async_trait;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;

#[derive(Debug)]
pub struct NB {
    id: String,
    graph: FastGraph,
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
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret).await?;

        // Nationality
        for text in self.triples_literals("http://schema.org/nationality")? {
            let _ = ret.add_prop_text(ExternalId::new(27, &text));
        }

        // Born/died
        let birth_death = [
            ("http://schema.org/birthDate", 569),
            ("http://schema.org/deathDate", 570),
        ];
        for bd in birth_death {
            for s in self.triples_subject_literals(&self.get_id_url(), bd.0)? {
                let _ = match ret.parse_date(&s) {
                    Some((time, precision)) => {
                        ret.add_claim(self.new_statement_time(bd.1, &time, precision))
                    }
                    None => ret.add_prop_text(ExternalId::new(bd.1, &s)),
                };
            }
        }

        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl NB {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = format!("http://data.bibliotheken.nl/doc/thes/p{}.rdf", id);
        let resp = reqwest::get(&rdf_url).await?.text().await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = xml::parser::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self {
            id: id.to_string(),
            graph,
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
        let viaf = NB::new(TEST_ID).await.unwrap();
        let meta_item = viaf.run().await.unwrap();
        assert_eq!(
            *meta_item.item.labels(),
            vec![LocaleString::new("nl", "Charles Darwin")]
        );
    }
}
