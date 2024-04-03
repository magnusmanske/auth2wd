use std::sync::Arc;

use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use anyhow::{anyhow, Result};
use axum::async_trait;
use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;

pub struct SELIBR {
    id: String,
    key: String,
    graph: Arc<FastGraph>,
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
    fn graph_mut(&mut self) -> &mut Arc<FastGraph> {
        &mut self.graph
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
        // let resp = ureq::builder()
        //     .redirects(10)
        //     .build()
        //     .get(&rdf_url)
        //     .call()?
        //     .into_string()?;
        let resp = reqwest::get(&rdf_url).await?.text().await?;
        // TODO check if redirect thing works
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        let mut ret = Self {
            id: id.to_string(),
            key: String::new(),
            graph: Arc::new(graph),
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
