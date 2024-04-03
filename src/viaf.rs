use std::sync::Arc;

use crate::external_importer::*;
use crate::meta_item::*;
use anyhow::Result;
use axum::async_trait;
use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;

#[derive(Clone)]
pub struct VIAF {
    id: String,
    graph: Arc<FastGraph>,
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
    fn graph_mut(&mut self) -> &mut Arc<FastGraph> {
        &mut self.graph
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

impl VIAF {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = format!("https://viaf.org/viaf/{}/rdf.xml", id);
        // let resp = ureq::get(&rdf_url).call()?.into_string()?;
        let resp = reqwest::get(&rdf_url).await?.text().await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self {
            id: id.to_string(),
            graph: Arc::new(graph),
        })
    }
}

#[cfg(test)]
mod tests {
    use wikibase::{EntityTrait, LocaleString};

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

    #[tokio::test]
    async fn test_graph() {
        let mut viaf = VIAF::new(TEST_ID).await.unwrap();
        let _graph = viaf.graph();
        let _graph = viaf.graph_mut();
    }
}
