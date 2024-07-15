use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::utility::Utility;
use anyhow::{anyhow, Result};
use axum::async_trait;
use regex::Regex;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;

lazy_static! {
    static ref RE_NUMERIC_ID: Regex =
        Regex::new(r#"^(\d{8,9})[0-9bcdfghjkmnpqrstvwxz]$"#).expect("Regexp error");
    static ref RE_URL: Regex =
        Regex::new(r#"<meta property="og:url" content="https://data.bnf.fr/\d+/(.+?)/" />"#)
            .expect("Regexp error");
}

pub struct BNF {
    id: String,
    graph: FastGraph,
}

unsafe impl Send for BNF {}
unsafe impl Sync for BNF {}

#[async_trait]
impl ExternalImporter for BNF {
    fn my_property(&self) -> usize {
        268
    }

    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    fn my_stated_in(&self) -> &str {
        "Q19938912"
    }

    fn graph(&self) -> &FastGraph {
        &self.graph
    }

    fn primary_language(&self) -> String {
        "fr".to_string()
    }

    fn get_key_url(&self, _key: &str) -> String {
        format!("https://data.bnf.fr/ark:/12148/cb{}#about", self.id)
    }

    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret).await?;

        // Born/died
        let birth_death = [
            ("http://rdvocab.info/ElementsGr2/dateOfBirth", 569),
            ("http://rdvocab.info/ElementsGr2/dateOfDeath", 570),
        ];
        for bd in birth_death {
            for s in self.triples_subject_iris(&self.get_id_url(), bd.0)? {
                let _ = match ret.parse_date(&s) {
                    Some((time, precision)) => {
                        ret.add_claim(self.new_statement_time(bd.1, &time, precision))
                    }
                    None => ret.add_prop_text(ExternalId::new(bd.1, &s)),
                };
            }
        }

        let birth_death = [
            ("http://vocab.org/bio/0.1/birth", 569),
            ("http://vocab.org/bio/0.1/death", 570),
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

        let born_died_in = [
            ("http://rdvocab.info/ElementsGr2/placeOfBirth", 19),
            ("http://rdvocab.info/ElementsGr2/placeOfDeath", 20),
        ];
        for (key, prop) in born_died_in {
            for s in self.triples_subject_literals(&self.get_id_url(), key)? {
                ret.add_prop_text(ExternalId::new(prop, &s));
            }
        }

        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl BNF {
    pub async fn new(id: &str) -> Result<Self> {
        if !RE_NUMERIC_ID.is_match(id) {
            return Err(anyhow!("ID format error for '{id}'"));
        }
        let numeric_id = RE_NUMERIC_ID.replace_all(id, "${1}");

        let name = match Self::get_name_for_id(&numeric_id).await {
            Some(name) => name,
            None => return Err(anyhow!("Name retrieval error for '{id}'")),
        };

        let rdf_url = format!("https://data.bnf.fr/{numeric_id}/{name}/rdf.xml");
        let resp = Utility::get_url(&rdf_url).await?;

        let mut graph: FastGraph = FastGraph::new();
        let _ = xml::parser::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self {
            id: id.to_string(),
            graph,
        })
    }

    async fn get_name_for_id(numeric_id: &str) -> Option<String> {
        let rdf_url = format!("https://data.bnf.fr/en/{numeric_id}");
        let resp = Utility::get_url(&rdf_url).await.ok()?;
        Some(RE_URL.captures(&resp)?.get(1)?.as_str().to_string())
    }
}

#[cfg(test)]
mod tests {
    use wikimisc::wikibase::{EntityTrait, LocaleString};

    use super::*;

    const TEST_ID: &str = "11898689q";
    const TEST_ID2: &str = "15585136v";

    #[tokio::test]
    async fn test_run() {
        let bnf = BNF::new(TEST_ID).await.unwrap();
        let meta_item = bnf.run().await.unwrap();
        assert_eq!(
            *meta_item.item.labels(),
            vec![LocaleString::new("fr", "Charles Darwin")]
        );
    }

    #[tokio::test]
    async fn test_run_birth_death_place() {
        let bnf = BNF::new(TEST_ID2).await.unwrap();
        let meta_item = bnf.run().await.unwrap();
        assert_eq!(
            *meta_item.item.labels(),
            vec![LocaleString::new("fr", "Louis Bassal")]
        );
        assert_eq!(meta_item.prop_text.len(), 2);
        assert_eq!(
            meta_item.prop_text[0],
            ExternalId::new(19, "Rivesaltes (Pyrénées-Orientales)")
        );
        assert_eq!(
            meta_item.prop_text[1],
            ExternalId::new(20, "Grenoble (Isère)")
        );

        println!("{:?}", meta_item.prop_text);
    }

    #[tokio::test]
    async fn test_new() {
        assert!(BNF::new(TEST_ID).await.is_ok());
    }

    #[tokio::test]
    async fn test_my_property() {
        let bnf = BNF::new(TEST_ID).await.unwrap();
        assert_eq!(bnf.my_property(), 268);
    }

    #[tokio::test]
    async fn test_my_stated_in() {
        let bnf = BNF::new(TEST_ID).await.unwrap();
        assert_eq!(bnf.my_stated_in(), "Q19938912");
    }

    #[tokio::test]
    async fn test_my_id() {
        let bnf = BNF::new(TEST_ID).await.unwrap();
        assert_eq!(bnf.my_id(), TEST_ID);
    }
}
