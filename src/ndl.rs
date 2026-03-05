use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::utility::Utility;
use anyhow::Result;
use async_trait::async_trait;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;

#[derive(Debug)]
pub struct NDL {
    id: String,
    graph: FastGraph,
}

#[async_trait]
impl ExternalImporter for NDL {
    fn my_property(&self) -> usize {
        P_NDL
    }
    fn my_stated_in(&self) -> &str {
        "Q477675"
    }
    fn primary_language(&self) -> String {
        String::from("ja")
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("http://id.ndl.go.jp/auth/entity/{}", self.id)
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

        // Birth/death dates from RDA vocabulary
        let birth_death = [
            (
                "http://RDVocab.info/ElementsGr2/dateOfBirth",
                P_DATE_OF_BIRTH,
            ),
            (
                "http://RDVocab.info/ElementsGr2/dateOfDeath",
                P_DATE_OF_DEATH,
            ),
        ];
        for (iri, property) in birth_death {
            let entity_url = self.get_key_url("id");
            for s in self.triples_subject_literals(&entity_url, iri)? {
                let _ = match ret.parse_date(&s) {
                    Some((time, precision)) => {
                        ret.add_claim(self.new_statement_time(property, &time, precision))
                    }
                    None => ret.add_prop_text(crate::external_id::ExternalId::new(property, &s)),
                };
            }
        }

        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl NDL {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = format!("https://id.ndl.go.jp/auth/ndlna/{id}.rdf");
        let text = Utility::get_url(&rdf_url).await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = xml::parser::parse_str(&text).add_to_graph(&mut graph)?;
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
    use wikimisc::wikibase::EntityTrait;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Natsume Soseki - well-known Japanese author (1867-1916)
    const TEST_ID: &str = "00054222";

    async fn mock_ndl() -> (MockServer, NDL) {
        let server = MockServer::start().await;
        let fixture = include_str!("../test_data/fixtures/ndl_00054222.rdf");

        Mock::given(method("GET"))
            .and(path("/auth/ndlna/00054222.rdf"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&server)
            .await;

        // try_viaf stub (NDL P349 is mapped in KEY2PROP)
        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;

        url_override::register("https://id.ndl.go.jp", server.uri());
        url_override::register("https://viaf.org", server.uri());

        let ndl = NDL::new(TEST_ID).await.unwrap();
        (server, ndl)
    }

    fn cleanup() {
        url_override::unregister("https://id.ndl.go.jp");
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    #[serial]
    async fn test_new() {
        let (_server, _ndl) = mock_ndl().await;
        cleanup();
    }

    #[tokio::test]
    #[serial]
    async fn test_my_property() {
        let (_server, ndl) = mock_ndl().await;
        assert_eq!(ndl.my_property(), P_NDL);
        cleanup();
    }

    #[tokio::test]
    #[serial]
    async fn test_my_stated_in() {
        let (_server, ndl) = mock_ndl().await;
        assert_eq!(ndl.my_stated_in(), "Q477675");
        cleanup();
    }

    #[tokio::test]
    #[serial]
    async fn test_primary_language() {
        let (_server, ndl) = mock_ndl().await;
        assert_eq!(ndl.primary_language(), "ja");
        cleanup();
    }

    #[tokio::test]
    #[serial]
    async fn test_get_key_url() {
        let (_server, ndl) = mock_ndl().await;
        assert_eq!(
            ndl.get_key_url(TEST_ID),
            "http://id.ndl.go.jp/auth/entity/00054222"
        );
        cleanup();
    }

    #[tokio::test]
    #[serial]
    async fn test_my_id() {
        let (_server, ndl) = mock_ndl().await;
        assert_eq!(ndl.my_id(), TEST_ID);
        cleanup();
    }

    #[tokio::test]
    #[serial]
    async fn test_transform_label() {
        let (_server, ndl) = mock_ndl().await;
        assert_eq!(ndl.transform_label("Natsume, Soseki"), "Soseki Natsume");
        assert_eq!(ndl.transform_label("Natsume,Soseki"), "Natsume,Soseki");
        assert_eq!(ndl.transform_label("Soseki Natsume"), "Soseki Natsume");
        cleanup();
    }

    #[tokio::test]
    #[serial]
    async fn test_run() {
        let (_server, ndl) = mock_ndl().await;
        let meta_item = ndl.run().await.unwrap();
        assert!(!meta_item.item.labels().is_empty());
        cleanup();
    }
}
