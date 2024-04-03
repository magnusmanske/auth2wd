use std::sync::Arc;

use crate::external_importer::*;
use crate::meta_item::*;
use anyhow::Result;
use axum::async_trait;
use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;

pub struct LOC {
    id: String,
    graph: Arc<FastGraph>,
}

unsafe impl Send for LOC {}
unsafe impl Sync for LOC {}

#[async_trait]
impl ExternalImporter for LOC {
    fn my_property(&self) -> usize {
        244
    }
    fn my_stated_in(&self) -> &str {
        "Q13219454"
    }
    fn primary_language(&self) -> String {
        "en".to_string()
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("http://id.loc.gov/authorities/names/{}", self.id)
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

impl LOC {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = format!("https://id.loc.gov/authorities/names/{id}.rdf");
        let resp = reqwest::get(&rdf_url).await?.text().await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self {
            id: id.to_string(),
            graph: Arc::new(graph),
        })
    }
}
