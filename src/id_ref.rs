use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use crate::properties::*;
use crate::url_override::maybe_rewrite;
use anyhow::Result;
use async_trait::async_trait;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;

#[derive(Debug)]
pub struct IdRef {
    id: String,
    graph: FastGraph,
}

#[async_trait]
impl ExternalImporter for IdRef {
    fn my_property(&self) -> usize {
        P_IDREF
    }

    fn my_id(&self) -> String {
        self.id.clone()
    }

    fn my_stated_in(&self) -> &str {
        "Q47757534"
    }

    fn graph(&self) -> &FastGraph {
        &self.graph
    }

    fn primary_language(&self) -> String {
        String::from("fr")
    }

    fn get_key_url(&self, key: &str) -> String {
        format!("http://www.idref.fr/{}/{}", self.id, key)
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret).await?;

        for url in self.triples_iris("http://dbpedia.org/ontology/citizenship")? {
            match self.url2external_id(&url) {
                Some(extid) => {
                    let _ = match extid.get_item_for_external_id_value().await {
                        Some(item) => {
                            ret.add_claim(self.new_statement_item(P_COUNTRY_OF_CITIZENSHIP, &item))
                        }
                        None => ret.add_prop_text(ExternalId::new(P_COUNTRY_OF_CITIZENSHIP, &url)),
                    };
                }
                None => {
                    let _ = ret.add_prop_text(ExternalId::new(P_COUNTRY_OF_CITIZENSHIP, &url));
                }
            }
        }

        let birth_death = [("birth", P_DATE_OF_BIRTH), ("death", P_DATE_OF_DEATH)];
        for bd in birth_death {
            for s in self.triples_subject_literals(
                &format!("http://www.idref.fr/{}/{}", self.id, bd.0),
                "http://purl.org/vocab/bio/0.1/date",
            )? {
                let _ = match ret.parse_date(&s) {
                    Some((time, precision)) => {
                        ret.add_claim(self.new_statement_time(bd.1, &time, precision))
                    }
                    None => ret.add_prop_text(ExternalId::new(bd.1, &s)),
                };
            }
        }

        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl IdRef {
    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = maybe_rewrite(&format!("https://www.idref.fr/{id}.rdf"));
        let resp = reqwest::get(&rdf_url).await?.text().await?;
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
    use wikimisc::wikibase::{EntityTrait, LocaleString};

    use super::*;
    use crate::url_override;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_ID: &str = "026812304";

    async fn mock_idref() -> (MockServer, IdRef) {
        let server = MockServer::start().await;
        let fixture = include_str!("../test_data/fixtures/idref_026812304.rdf");

        Mock::given(method("GET"))
            .and(path("/026812304.rdf"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&server)
            .await;

        // try_viaf stub (IdRef P269 is mapped via SUDOC key in KEY2PROP)
        Mock::given(method("POST"))
            .and(path("/api/cluster-record"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;

        url_override::register("https://www.idref.fr", server.uri());
        url_override::register("https://viaf.org", server.uri());

        let idref = IdRef::new(TEST_ID).await.unwrap();
        (server, idref)
    }

    fn cleanup() {
        url_override::unregister("https://www.idref.fr");
        url_override::unregister("https://viaf.org");
    }

    #[tokio::test]
    async fn test_new() {
        let (_server, _idref) = mock_idref().await;
        cleanup();
    }

    #[tokio::test]
    async fn test_my_property() {
        let (_server, idref) = mock_idref().await;
        assert_eq!(idref.my_property(), P_IDREF);
        cleanup();
    }

    #[tokio::test]
    async fn test_my_stated_in() {
        let (_server, idref) = mock_idref().await;
        assert_eq!(idref.my_stated_in(), "Q47757534");
        cleanup();
    }

    #[tokio::test]
    async fn test_my_id() {
        let (_server, idref) = mock_idref().await;
        assert_eq!(idref.my_id(), TEST_ID);
        cleanup();
    }

    #[tokio::test]
    async fn test_run() {
        let (_server, idref) = mock_idref().await;
        let meta_item = idref.run().await.unwrap();
        assert_eq!(
            *meta_item.item.labels(),
            vec![LocaleString::new("fr", "Charles Darwin")]
        );
        cleanup();
    }
}
