use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use anyhow::Result;
use axum::async_trait;
use regex::Regex;
use sophia::graph::inmem::FastGraph;
use sophia::triple::stream::TripleSource;
use std::sync::Arc;

lazy_static! {
    static ref RE_COUNTRY: Regex =
        Regex::new(r"^https?://d-nb.info/standards/vocab/gnd/geographic-area-code#XA-(.+)$")
            .expect("Regexp error");
}

#[derive(Clone)]
pub struct GND {
    id: String,
    graph: Arc<FastGraph>,
}

unsafe impl Send for GND {}
unsafe impl Sync for GND {}

#[async_trait]
impl ExternalImporter for GND {
    fn my_property(&self) -> usize {
        227
    }

    fn my_id(&self) -> String {
        self.id.to_owned()
    }

    fn my_stated_in(&self) -> &str {
        "Q36578"
    }

    fn graph(&self) -> &FastGraph {
        &self.graph
    }

    fn graph_mut(&mut self) -> &mut Arc<FastGraph> {
        &mut self.graph
    }

    fn primary_language(&self) -> String {
        "de".to_string()
    }

    fn get_key_url(&self, _key: &str) -> String {
        format!("https://d-nb.info/gnd/{}", self.id)
    }

    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret).await?;

        // Nationality
        for url in self.triples_subject_iris(
            &self.get_id_url(),
            "https://d-nb.info/standards/elementset/gnd#geographicAreaCode",
        )? {
            let country_code = RE_COUNTRY.replace(&url, "${1}");
            if country_code != url {
                let ext_id = ExternalId::new(297, &country_code);
                let _ = match ext_id.get_item_for_external_id_value().await {
                    Some(item) => ret.add_claim(self.new_statement_item(27, &item)),
                    None => ret.add_prop_text(ext_id).await,
                };
            }
        }

        // Born/died
        let birth_death = [
            (
                "https://d-nb.info/standards/elementset/gnd#dateOfBirth",
                569,
            ),
            (
                "https://d-nb.info/standards/elementset/gnd#dateOfDeath",
                570,
            ),
        ];
        for bd in birth_death {
            for s in self.triples_subject_literals(&self.get_id_url(), bd.0)? {
                let _ = match ret.parse_date(&s) {
                    Some((time, precision)) => {
                        ret.add_claim(self.new_statement_time(bd.1, &time, precision))
                    }
                    None => ret.add_prop_text(ExternalId::new(bd.1, &s)).await,
                };
            }
        }

        // Places
        let key_prop = [
            (
                "https://d-nb.info/standards/elementset/gnd#placeOfBirth",
                19,
            ),
            (
                "https://d-nb.info/standards/elementset/gnd#placeOfDeath",
                20,
            ),
            (
                "https://d-nb.info/standards/elementset/agrelon#hasChild",
                40,
            ),
            (
                "https://d-nb.info/standards/elementset/gnd#fieldOfStudy",
                101,
            ),
            (
                "https://d-nb.info/standards/elementset/gnd#professionOrOccupation",
                106,
            ),
            (
                "https://d-nb.info/standards/elementset/gnd#placeOfActivity",
                937,
            ),
            // TODO parent
        ];
        for kp in key_prop {
            for url in self.triples_subject_iris(&self.get_id_url(), kp.0)? {
                if let Some(gnd_id) = url.split('/').last() {
                    if let Some(item) = ExternalId::new(227, gnd_id)
                        .get_item_for_external_id_value()
                        .await
                    {
                        ret.add_claim(self.new_statement_item(kp.1, &item));
                    } else {
                        let _ = ret.add_prop_text(ExternalId::new(kp.1, &url)).await;
                    }
                } else {
                    let _ = ret.add_prop_text(ExternalId::new(kp.1, &url)).await;
                }
            }
        }

        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl GND {
    /// Changes internal ID in case of redirect
    fn fix_own_id(&mut self) -> Result<()> {
        let ids = self.triples_property_literals(
            "https://d-nb.info/standards/elementset/gnd#gndIdentifier",
        )?;
        if ids.len() == 1 {
            self.id = ids[0].to_owned();
        }
        Ok(())
    }

    pub async fn new(id: &str) -> Result<Self> {
        let rdf_url = format!("https://d-nb.info/gnd/{}/about/lds.rdf", id);
        let resp = reqwest::get(&rdf_url).await?.text().await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = sophia::parser::xml::parse_str(&resp).add_to_graph(&mut graph)?;
        let mut ret = Self {
            id: id.to_string(),
            graph: Arc::new(graph),
        };
        ret.fix_own_id()?;
        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use wikibase::{EntityTrait, LocaleString};

    use super::*;

    const TEST_ID: &str = "132539691";

    #[tokio::test]
    async fn test_new() {
        assert!(GND::new(TEST_ID).await.is_ok());
    }

    #[tokio::test]
    async fn test_my_property() {
        let gnd = GND::new(TEST_ID).await.unwrap();
        assert_eq!(gnd.my_property(), 227);
    }

    #[tokio::test]
    async fn test_my_stated_in() {
        let gnd = GND::new(TEST_ID).await.unwrap();
        assert_eq!(gnd.my_stated_in(), "Q36578");
    }

    #[tokio::test]
    async fn test_primary_language() {
        let gnd = GND::new(TEST_ID).await.unwrap();
        assert_eq!(gnd.primary_language(), "de");
    }

    #[tokio::test]
    async fn test_get_key_url() {
        let gnd = GND::new(TEST_ID).await.unwrap();
        assert_eq!(gnd.get_key_url(TEST_ID), "https://d-nb.info/gnd/132539691");
    }

    #[tokio::test]
    async fn test_my_id() {
        let gnd = GND::new(TEST_ID).await.unwrap();
        assert_eq!(gnd.my_id(), TEST_ID);
    }

    #[tokio::test]
    async fn test_transform_label() {
        let gnd = GND::new(TEST_ID).await.unwrap();
        assert_eq!(gnd.transform_label("Manske, Magnus"), "Magnus Manske");
        assert_eq!(gnd.transform_label("Manske,Magnus"), "Manske,Magnus");
        assert_eq!(gnd.transform_label("Magnus Manske"), "Magnus Manske");
    }

    #[tokio::test]
    async fn test_run() {
        let gnd = GND::new(TEST_ID).await.unwrap();
        let meta_item = gnd.run().await.unwrap();
        assert_eq!(
            *meta_item.item.labels(),
            vec![LocaleString::new("de", "Magnus Manske")]
        );
    }

    #[tokio::test]
    async fn test_graph() {
        let mut gnd = GND::new(TEST_ID).await.unwrap();
        let _graph = gnd.graph();
        let _graph = gnd.graph_mut();
    }
}
