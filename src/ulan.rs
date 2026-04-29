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

use crate::external_id::ExternalId;

#[derive(Debug)]
pub struct ULAN {
    id: String,
    graph: FastGraph,
}

#[async_trait]
impl ExternalImporter for ULAN {
    fn my_property(&self) -> usize {
        P_ULAN
    }
    fn my_stated_in(&self) -> &str {
        "Q2494649"
    }
    fn primary_language(&self) -> String {
        String::from("en")
    }
    fn get_key_url(&self, _key: &str) -> String {
        format!("http://vocab.getty.edu/ulan/{}", self.id)
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

        // let x = self.triples_iris("http://vocab.getty.edu/ontology#ulan1512_parent_of");
        // println!("x: {:?}", x);

        self.add_the_usual(&mut ret).await?;
        self.add_p31(&mut ret)?;
        self.add_children(&mut ret).await?;
        self.add_mentors(&mut ret).await?;
        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl ULAN {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = maybe_rewrite(&format!("https://vocab.getty.edu/ulan/{id}.rdf"));
        let client = Utility::get_reqwest_client()?;
        let resp = client.get(&rdf_url).send().await?.text().await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = xml::parser::parse_str(&resp).add_to_graph(&mut graph)?;
        Ok(Self {
            id: id.to_string(),
            graph,
        })
    }

    fn add_p31(&self, ret: &mut MetaItem) -> Result<()> {
        ret.add_claim(self.new_statement_item(P_INSTANCE_OF, "Q5"));
        Ok(())
    }

    async fn add_children(&self, ret: &mut MetaItem) -> Result<()> {
        let children = self.triples_iris("http://vocab.getty.edu/ontology#ulan1512_parent_of")?;
        for child in children {
            self.add_ulan_item(&child, P_CHILD, ret).await;
        }
        Ok(())
    }

    async fn add_mentors(&self, ret: &mut MetaItem) -> Result<()> {
        let children = self.triples_iris("http://vocab.getty.edu/ontology#ulan1102_student_of")?;
        for child in children {
            self.add_ulan_item(&child, P_STUDENT_OF, ret).await;
        }
        Ok(())
    }

    async fn add_ulan_item(&self, url: &str, property: usize, ret: &mut MetaItem) {
        if let Some(ulan_id) = url.split('/').next_back() {
            if let Some(item) = ExternalId::new(self.my_property(), ulan_id)
                .get_item_for_external_id_value()
                .await
            {
                ret.add_claim(self.new_statement_item(property, &item));
            } else {
                let _ = ret.add_prop_text(ExternalId::new(property, url));
            }
        } else {
            let _ = ret.add_prop_text(ExternalId::new(property, url));
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url_override;
    use serial_test::serial;
    use wikimisc::wikibase::{EntityTrait, Statement, Value};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_ID: &str = "500228559";

    async fn mock_ulan() -> (MockServer, ULAN) {
        let server = MockServer::start().await;
        let fixture = include_str!("../test_data/fixtures/ulan_500228559.rdf");

        Mock::given(method("GET"))
            .and(path("/ulan/500228559.rdf"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&server)
            .await;

        url_override::register("https://vocab.getty.edu", server.uri());

        let ulan = ULAN::new(TEST_ID).await.unwrap();
        (server, ulan)
    }

    fn cleanup() {
        url_override::unregister("https://vocab.getty.edu");
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    #[serial]
    async fn test_new() {
        let (_server, _ulan) = mock_ulan().await;
        cleanup();
    }

    /// Verifies that VIAF inference for ULAN flows through the shared
    /// `try_viaf` machinery in `ExternalImporter` (via the JPG → P_ULAN entry
    /// in VIAF's KEY2PROP map). The fixture mocks the VIAF cluster-record
    /// response that the lookup would receive for `JPG|500228559`.
    #[tokio::test]
    #[serial]
    async fn test_viaf_id_from_ulan() {
        let ulan_fixture = include_str!("../test_data/fixtures/ulan_500228559.rdf");

        // ── Case 1: VIAF returns a valid viafID ────────────────────────────
        {
            let server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path("/ulan/500228559.rdf"))
                .respond_with(ResponseTemplate::new(200).set_body_string(ulan_fixture))
                .mount(&server)
                .await;

            let viaf_fixture = include_str!("../test_data/fixtures/viaf_lookup_jpg_500228559.json");
            Mock::given(method("POST"))
                .and(path("/api/cluster-record"))
                .respond_with(ResponseTemplate::new(200).set_body_string(viaf_fixture))
                .mount(&server)
                .await;

            url_override::register("https://vocab.getty.edu", server.uri());
            url_override::register("https://viaf.org", server.uri());

            let ulan = ULAN::new(TEST_ID).await.unwrap();

            let mut meta_item = MetaItem::new();
            ulan.try_viaf(&mut meta_item).await.unwrap();

            let viaf_claims: Vec<&Statement> = meta_item
                .item
                .claims()
                .iter()
                .filter(|c| c.main_snak().property() == format!("P{P_VIAF}"))
                .collect();
            assert_eq!(viaf_claims.len(), 1, "expected exactly one VIAF claim");

            let snak = viaf_claims[0].main_snak();
            if let Some(dv) = snak.data_value() {
                match dv.value() {
                    Value::StringValue(s) => {
                        assert_eq!(s, "27063124");
                    }
                    other => panic!("expected StringValue, got {:?}", other),
                }
            } else {
                panic!("expected data value on VIAF snak");
            }

            cleanup();
        }

        // ── Case 2: VIAF returns no viafID ─────────────────────────────────
        {
            let server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path("/ulan/500228559.rdf"))
                .respond_with(ResponseTemplate::new(200).set_body_string(ulan_fixture))
                .mount(&server)
                .await;

            Mock::given(method("POST"))
                .and(path("/api/cluster-record"))
                .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
                .mount(&server)
                .await;

            url_override::register("https://vocab.getty.edu", server.uri());
            url_override::register("https://viaf.org", server.uri());

            let ulan = ULAN::new(TEST_ID).await.unwrap();

            let mut meta_item = MetaItem::new();
            ulan.try_viaf(&mut meta_item).await.unwrap();

            let viaf_claims: Vec<&Statement> = meta_item
                .item
                .claims()
                .iter()
                .filter(|c| c.main_snak().property() == format!("P{P_VIAF}"))
                .collect();
            assert!(
                viaf_claims.is_empty(),
                "expected no VIAF claim when VIAF returns no ID"
            );

            cleanup();
        }
    }
}
