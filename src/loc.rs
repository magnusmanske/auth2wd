use crate::external_importer::*;
use crate::meta_item::*;
use crate::utility::Utility;
use anyhow::Result;
use async_trait::async_trait;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;

#[derive(Debug)]
pub struct LOC {
    id: String,
    graph: FastGraph,
}

// unsafe impl Send for LOC {}
// unsafe impl Sync for LOC {}

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
        let client = Utility::get_reqwest_client()?;
        let resp = client.get(&rdf_url).send().await?.text().await?;
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
    use super::*;

    const TEST_ID: &str = "n78095637";

    #[tokio::test]
    async fn test_new() {
        assert!(LOC::new(TEST_ID).await.is_ok());
    }
}
