use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use anyhow::{anyhow, Result};
use axum::async_trait;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;

pub struct SELIBR {
    id: String,
    key: String,
    graph: FastGraph,
}

unsafe impl Send for SELIBR {}
unsafe impl Sync for SELIBR {}

#[async_trait]
impl ExternalImporter for SELIBR {
    fn my_property(&self) -> usize {
        906
    }
    fn my_stated_in(&self) -> &str {
        "Q1798125"
    }
    fn primary_language(&self) -> String {
        "sv".to_string()
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("{}#it", self.key)
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

        for url in self.triples_iris("https://id.kb.se/vocab/nationality")? {
            ret.add_prop_text(ExternalId::new(27, &url)).await;
        }

        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl SELIBR {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = format!("http://libris.kb.se/resource/auth/{}/data.rdf", id);
        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?;
        let resp = client
            .get(&rdf_url)
            .header(reqwest::header::ACCEPT, "application/rdf+xml")
            .send()
            .await?
            .text()
            .await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = xml::parser::parse_str(&resp).add_to_graph(&mut graph)?;
        let mut ret = Self {
            id: id.to_string(),
            key: String::new(),
            graph,
        };

        let ids = ret.triples_property_object_iris(
            "https://id.kb.se/vocab/sameAs",
            &format!("http://libris.kb.se/auth/{id}"),
        )?;
        match ids.first() {
            Some(id) => ret.key = id.to_owned(),
            None => return Err(anyhow!("could not find main key for '{id}'")),
        }

        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ID: &str = "231727";

    #[tokio::test]
    async fn test_new() {
        assert!(SELIBR::new(TEST_ID).await.is_ok());
    }

    #[tokio::test]
    async fn test_my_property() {
        let selibr = SELIBR::new(TEST_ID).await.unwrap();
        assert_eq!(selibr.my_property(), 906);
    }

    #[tokio::test]
    async fn test_my_stated_in() {
        let selibr = SELIBR::new(TEST_ID).await.unwrap();
        assert_eq!(selibr.my_stated_in(), "Q1798125");
    }

    #[tokio::test]
    async fn test_primary_language() {
        let selibr = SELIBR::new(TEST_ID).await.unwrap();
        assert_eq!(selibr.primary_language(), "sv");
    }

    #[tokio::test]
    async fn test_get_key_url() {
        let selibr = SELIBR::new(TEST_ID).await.unwrap();
        assert_eq!(
            selibr.get_key_url(TEST_ID),
            "https://libris.kb.se/pm135sp73dmxqcf#it"
        );
    }

    #[tokio::test]
    async fn test_my_id() {
        let selibr = SELIBR::new(TEST_ID).await.unwrap();
        assert_eq!(selibr.my_id(), TEST_ID);
    }

    #[tokio::test]
    async fn test_transform_label() {
        let selibr = SELIBR::new(TEST_ID).await.unwrap();
        assert_eq!(selibr.transform_label("Månsson, Magnus"), "Magnus Månsson");
        assert_eq!(selibr.transform_label("Månsson,Magnus"), "Månsson,Magnus");
        assert_eq!(selibr.transform_label("Magnus Månsson"), "Magnus Månsson");
    }
}
