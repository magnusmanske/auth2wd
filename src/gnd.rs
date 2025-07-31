use crate::external_id::*;
use crate::external_importer::*;
use crate::meta_item::*;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::xml;
use wikimisc::wikibase::EntityTrait;
use wikimisc::wikibase::{Snak, StatementRank};

lazy_static! {
    static ref RE_COUNTRY: Regex =
        Regex::new(r"^https?://d-nb.info/standards/vocab/gnd/geographic-area-code#XA-(.+)$")
            .expect("Regexp error");
}

#[derive(Clone, Debug)]
pub struct GND {
    id: String,
    graph: FastGraph,
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

    fn primary_language(&self) -> String {
        "de".to_string()
    }

    fn get_key_url(&self, _key: &str) -> String {
        format!("https://d-nb.info/gnd/{}", self.id)
    }

    fn transform_label(&self, s: &str) -> String {
        self.transform_label_last_first_name(s)
    }

    fn add_own_id(&self, ret: &mut MetaItem) -> Result<()> {
        let mut statement = self.new_statement_string(self.my_property(), &self.my_id());

        if self.is_undifferentiated_person()? {
            statement.set_rank(StatementRank::Deprecated);
            let snak = Snak::new_item("P2241", "Q68648103");
            statement.add_qualifier_snak(snak);
        }

        ret.add_claim(statement);
        Ok(())
    }

    async fn run(&self) -> Result<MetaItem> {
        let mut ret = MetaItem::new();
        self.add_the_usual(&mut ret).await?;
        ret.item.descriptions_mut().clear(); // Descriptions are usually nonsense

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
                    None => ret.add_prop_text(ext_id),
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
                    None => ret.add_prop_text(ExternalId::new(bd.1, &s)),
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
                "https://d-nb.info/standards/elementset/agrelon#hasSibling",
                3373,
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
        for (elementset, property) in key_prop {
            for url in self.triples_iris(elementset)? {
                self.add_gnd_item(&url, property, &mut ret).await;
            }
        }

        // Blank nodes aka multiple values
        for (elementset, property) in key_prop {
            self.bnodes(elementset, property, &mut ret).await?;
        }

        // Work activity
        self.activity(&mut ret).await?;

        self.try_rescue_prop_text(&mut ret).await?;
        ret.cleanup();
        Ok(ret)
    }
}

impl GND {
    async fn activity(&self, ret: &mut MetaItem) -> Result<()> {
        lazy_static! {
            static ref RE_SINGLE_YEAR: Regex = Regex::new(r"^(\d{3,4})$").expect("Regexp error");
            static ref RE_YEAR_RANGE: Regex =
                Regex::new(r"^(\d{3,4}) *- *(\d{3,4})$").expect("Regexp error");
        }
        let lits = self.triples_property_literals(
            "https://d-nb.info/standards/elementset/gnd#periodOfActivity",
        )?;
        if lits.len() != 1 {
            return Ok(());
        }
        if let Some(lit) = lits.first() {
            if let Some(year) = RE_SINGLE_YEAR.captures(lit) {
                if let Some(year) = year.get(1) {
                    if let Some((time, precision)) = ret.parse_date(year.as_str()) {
                        ret.add_claim(self.new_statement_time(1317, &time, precision));
                    }
                }
            } else if let Some(result) = RE_YEAR_RANGE.captures(lit) {
                if let Some(start_year) = result.get(1) {
                    if let Some((time, precision)) = ret.parse_date(start_year.as_str()) {
                        ret.add_claim(self.new_statement_time(2031, &time, precision));
                    }
                }
                if let Some(end_year) = result.get(2) {
                    if let Some((time, precision)) = ret.parse_date(end_year.as_str()) {
                        ret.add_claim(self.new_statement_time(2032, &time, precision));
                    }
                }
            }
        }
        Ok(())
    }

    async fn bnodes(&self, url: &str, property: usize, ret: &mut MetaItem) -> Result<()> {
        for bnode_id in self.triples_subject_iris_blank_nodes(
            &self.get_id_url(),
            url,
            // "https://d-nb.info/standards/elementset/gnd#professionOrOccupation",
        )? {
            let mut gnd_urls = vec![];
            let b = sophia::api::term::BnodeId::new(bnode_id.to_owned()).unwrap();
            let _ = self
                .graph()
                .triples_matching([b], Any, Any)
                .for_each_triple(|t| {
                    if let Some(iri) = t.p().iri() {
                        if iri.starts_with("http://www.w3.org/1999/02/22-rdf-syntax-ns#_") {
                            if let Some(gnd_irl) = t.o().iri() {
                                gnd_urls.push(gnd_irl.to_string());
                            }
                        }
                    }
                });
            for gnd_url in gnd_urls {
                self.add_gnd_item(&gnd_url, property, ret).await;
            }
        }
        Ok(())
    }

    async fn add_gnd_item(&self, url: &str, property: usize, ret: &mut MetaItem) {
        if let Some(gnd_id) = url.split('/').next_back() {
            if let Some(item) = ExternalId::new(self.my_property(), gnd_id)
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
        let rdf_url = format!("https://d-nb.info/gnd/{id}/about/lds.rdf");
        let resp = reqwest::get(&rdf_url).await?.text().await?;
        let mut graph: FastGraph = FastGraph::new();
        let _ = xml::parser::parse_str(&resp).add_to_graph(&mut graph)?;
        let mut ret = Self {
            id: id.to_string(),
            graph,
        };
        ret.fix_own_id()?;
        Ok(ret)
    }

    fn is_undifferentiated_person(&self) -> Result<bool> {
        Ok(self
            .triples_subject_iris(
                &self.get_id_url(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            )?
            .iter()
            .any(|x| x == "https://d-nb.info/standards/elementset/gnd#UndifferentiatedPerson"))
    }
}

#[cfg(test)]
mod tests {
    use wikimisc::wikibase::{EntityTrait, LocaleString};

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
}
