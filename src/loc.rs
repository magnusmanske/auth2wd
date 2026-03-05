use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::url_override::maybe_rewrite;
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
        P_LOC
    }
    fn my_stated_in(&self) -> &str {
        "Q13219454"
    }
    fn primary_language(&self) -> String {
        String::from("en")
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("http://id.loc.gov/authorities/names/{}", self.id)
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
        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl LOC {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = maybe_rewrite(&format!("https://id.loc.gov/authorities/names/{id}.rdf"));
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
    use crate::url_override;
    use serial_test::serial;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_ID: &str = "n78095637";

    async fn mock_loc() -> (MockServer, LOC) {
        let server = MockServer::start().await;
        let fixture = include_str!("../test_data/fixtures/loc_n78095637.rdf");

        Mock::given(method("GET"))
            .and(path("/authorities/names/n78095637.rdf"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&server)
            .await;

        url_override::register("https://id.loc.gov", server.uri());

        let loc = LOC::new(TEST_ID).await.unwrap();
        (server, loc)
    }

    #[tokio::test]
    #[serial]
    async fn test_new() {
        let (_server, _loc) = mock_loc().await;
        url_override::unregister("https://id.loc.gov");
    }
}
