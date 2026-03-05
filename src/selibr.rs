use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::url_override::maybe_rewrite;
use crate::utility::Utility;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;

#[derive(Debug)]
pub struct SELIBR {
    id: String,
    key: String,
    graph: FastGraph,
}

#[async_trait]
impl ExternalImporter for SELIBR {
    fn my_property(&self) -> usize {
        P_SELIBR
    }
    fn my_stated_in(&self) -> &str {
        "Q1798125"
    }
    fn primary_language(&self) -> String {
        String::from("sv")
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("{}#it", self.key)
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

        for url in self.triples_iris("https://id.kb.se/vocab/nationality")? {
            ret.add_prop_text(ExternalId::new(P_COUNTRY_OF_CITIZENSHIP, &url));
        }

        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl SELIBR {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = maybe_rewrite(&format!("http://libris.kb.se/resource/auth/{id}/data.rdf"));
        let client = Utility::get_reqwest_client()?;
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
            Some(first_id) => ret.key = first_id.clone(),
            None => return Err(anyhow!("could not find main key for '{id}'")),
        }

        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url_override;
    use serial_test::serial;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_ID: &str = "231727";

    async fn mock_selibr() -> (MockServer, SELIBR) {
        let server = MockServer::start().await;
        let fixture = include_str!("../test_data/fixtures/selibr_231727.rdf");

        Mock::given(method("GET"))
            .and(path("/resource/auth/231727/data.rdf"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&server)
            .await;

        url_override::register("http://libris.kb.se", server.uri());

        let selibr = SELIBR::new(TEST_ID).await.unwrap();
        (server, selibr)
    }

    #[tokio::test]
    #[serial]
    async fn test_new() {
        let (_server, _selibr) = mock_selibr().await;
        url_override::unregister("http://libris.kb.se");
    }

    #[tokio::test]
    #[serial]
    async fn test_my_property() {
        let (_server, selibr) = mock_selibr().await;
        assert_eq!(selibr.my_property(), P_SELIBR);
        url_override::unregister("http://libris.kb.se");
    }

    #[tokio::test]
    #[serial]
    async fn test_my_stated_in() {
        let (_server, selibr) = mock_selibr().await;
        assert_eq!(selibr.my_stated_in(), "Q1798125");
        url_override::unregister("http://libris.kb.se");
    }

    #[tokio::test]
    #[serial]
    async fn test_primary_language() {
        let (_server, selibr) = mock_selibr().await;
        assert_eq!(selibr.primary_language(), "sv");
        url_override::unregister("http://libris.kb.se");
    }

    #[tokio::test]
    #[serial]
    async fn test_get_key_url() {
        let (_server, selibr) = mock_selibr().await;
        assert_eq!(
            selibr.get_key_url(TEST_ID),
            "https://libris.kb.se/pm135sp73dmxqcf#it"
        );
        url_override::unregister("http://libris.kb.se");
    }

    #[tokio::test]
    #[serial]
    async fn test_my_id() {
        let (_server, selibr) = mock_selibr().await;
        assert_eq!(selibr.my_id(), TEST_ID);
        url_override::unregister("http://libris.kb.se");
    }

    #[tokio::test]
    #[serial]
    async fn test_transform_label() {
        let (_server, selibr) = mock_selibr().await;
        assert_eq!(selibr.transform_label("Månsson, Magnus"), "Magnus Månsson");
        assert_eq!(selibr.transform_label("Månsson,Magnus"), "Månsson,Magnus");
        assert_eq!(selibr.transform_label("Magnus Månsson"), "Magnus Månsson");
        url_override::unregister("http://libris.kb.se");
    }
}
